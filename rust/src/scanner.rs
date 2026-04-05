use crate::models::Device;
use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, PeripheralId};
use chrono::Local;
use futures::stream::StreamExt;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

const RSSI_HISTORY_LEN: usize = 20;

fn resolve_services(services: &[uuid::Uuid]) -> Vec<String> {
    services
        .iter()
        .map(|uuid| {
            let s = uuid.to_string().to_lowercase();
            if let Some(n) = crate::db::SERVICE_DB.get(&s) {
                return n.clone();
            }
            if s.ends_with("-0000-1000-8000-00805f9b34fb") {
                let short = &s[4..8];
                if let Some(n) = crate::db::SERVICE_DB.get(short) {
                    return n.clone();
                }
            }
            uuid.to_string()
        })
        .collect()
}

async fn upsert_device(
    id: PeripheralId,
    adapter: &btleplug::platform::Adapter,
    shared: &Arc<Mutex<HashMap<PeripheralId, Device>>>,
    saved_devices: &HashMap<String, Device>,
) {
    let now = Local::now();
    let periph = match adapter.peripheral(&id).await {
        Ok(p) => p,
        Err(_) => return,
    };
    let props = match periph.properties().await {
        Ok(Some(p)) => p,
        _ => return,
    };

    let rssi = props.rssi.unwrap_or(-100) as i16;
    let name = props
        .local_name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "HIDDEN".to_string());

    let (device_type, confidence) =
        Device::classify_type(&props.manufacturer_data, &props.services, &name);
    if device_type == crate::models::DeviceType::Ignored {
        return;
    }
    let beacon_type = Device::classify_beacon(&props.manufacturer_data, &props.services);

    let mut beacon_uuid = None;
    let mut beacon_major = None;
    let mut beacon_minor = None;
    if let Some(data) = props.manufacturer_data.get(&0x004C_u16)
        && let Some((u, maj, min)) = Device::parse_ibeacon(data)
    {
        beacon_uuid = Some(u);
        beacon_major = Some(maj);
        beacon_minor = Some(min);
    }

    let id_str = format!("{:?}", id);
    let address_type = Device::classify_address_type(&id_str);
    let distance_m = Device::estimate_distance(rssi);
    let services_resolved = resolve_services(&props.services);

    let mut lock = shared.lock().await;
    let dev = lock.entry(id.clone()).or_insert_with(|| {
        if let Some(saved) = saved_devices.get(&id_str) {
            let mut restored = saved.clone();
            restored.name = name.clone();
            restored.rssi = rssi;
            restored.manufacturer_data = props.manufacturer_data.clone();
            restored.services = props.services.clone();
            restored.last_seen = now;
            restored.device_type = device_type.clone();
            restored.classification_confidence = confidence;
            restored.beacon_type = beacon_type.clone();
            restored.distance_m = distance_m;
            restored.address_type = address_type.clone();
            restored.services_resolved = services_resolved.clone();
            restored.is_connectable = true;
            restored
        } else {
            Device {
                id: id_str.clone(),
                name: name.clone(),
                rssi,
                manufacturer_data: props.manufacturer_data.clone(),
                services: props.services.clone(),
                first_seen: now,
                last_seen: now,
                hit_count: 0,
                device_type: device_type.clone(),
                classification_confidence: confidence,
                beacon_type: beacon_type.clone(),
                distance_m,
                is_connectable: true,
                rssi_history: Vec::with_capacity(RSSI_HISTORY_LEN),
                signal_min: rssi,
                signal_max: rssi,
                signal_avg: rssi as f32,
                address_type: address_type.clone(),
                beacon_uuid: beacon_uuid.clone(),
                beacon_major,
                beacon_minor,
                services_resolved: services_resolved.clone(),
                last_description: None,
            }
        }
    });

    dev.name = name;
    dev.rssi = rssi;
    dev.last_seen = now;
    dev.hit_count = dev.hit_count.saturating_add(1);
    dev.manufacturer_data = props.manufacturer_data;
    dev.services = props.services;

    if confidence >= dev.classification_confidence {
        dev.device_type = device_type;
        dev.classification_confidence = confidence;
    }

    dev.beacon_type = beacon_type;
    dev.distance_m = distance_m;
    dev.services_resolved = services_resolved;
    dev.address_type = address_type;
    dev.is_connectable = true;

    dev.rssi_history.push(rssi);
    if dev.rssi_history.len() > RSSI_HISTORY_LEN {
        dev.rssi_history.remove(0);
    }
    dev.signal_min = dev.signal_min.min(rssi);
    dev.signal_max = dev.signal_max.max(rssi);
    let sum: f32 = dev.rssi_history.iter().map(|&r| r as f32).sum();
    dev.signal_avg = sum / dev.rssi_history.len() as f32;

    if beacon_uuid.is_some() {
        dev.beacon_uuid = beacon_uuid;
        dev.beacon_major = beacon_major;
        dev.beacon_minor = beacon_minor;
    }
}

pub async fn run_scanner<F, Fut>(mut callback: F) -> Result<(), Box<dyn Error>>
where
    F: FnMut(
            CentralEvent,
            Manager,
            btleplug::platform::Adapter,
            Arc<Mutex<HashMap<PeripheralId, Device>>>,
        ) -> Fut
        + Send
        + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let manager = Manager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or("NO_ADAPTER")?;

    let devices: Arc<Mutex<HashMap<PeripheralId, Device>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut events = adapter.events().await?;
    adapter.start_scan(ScanFilter::default()).await?;

    while let Some(event) = events.next().await {
        callback(
            event,
            manager.clone(),
            adapter.clone(),
            Arc::clone(&devices),
        )
        .await;
        let mut devices_lock = devices.lock().await;
        devices_lock.retain(|_, d| (Local::now() - d.last_seen).num_seconds() < 30);
    }

    Ok(())
}

pub async fn background_scan(
    shared_devices: Arc<Mutex<HashMap<PeripheralId, Device>>>,
    saved_devices: Arc<HashMap<String, Device>>,
    running: &AtomicBool,
) {
    if let Some(jvm) = crate::JVM.get() {
        if jvm.attach_current_thread_permanently().is_err() {
            return;
        }
    } else {
        return;
    }

    let manager = match Manager::new().await {
        Ok(m) => m,
        Err(_) => return,
    };

    let adapter = match manager.adapters().await {
        Ok(mut list) if !list.is_empty() => list.remove(0),
        _ => return,
    };

    let mut events = match adapter.events().await {
        Ok(e) => e,
        _ => return,
    };

    if let Err(e) = adapter.start_scan(ScanFilter::default()).await {
        log::error!("Failed to start scan: {:?}", e);
        return;
    }

    while running.load(Ordering::SeqCst) {
        tokio::select! {
            event = events.next() => {
                match event {
                    Some(CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id)) => {
                        upsert_device(id, &adapter, &shared_devices, &saved_devices).await;
                    }
                    Some(CentralEvent::DeviceDisconnected(id)) => {
                        let mut lock = shared_devices.lock().await;
                        if let Some(dev) = lock.get_mut(&id) {
                            dev.is_connectable = false;
                        }
                    }
                    None => {
                        let _ = adapter.start_scan(ScanFilter::default()).await;
                        if let Ok(new_events) = adapter.events().await {
                            events = new_events;
                        }
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                let mut lock = shared_devices.lock().await;
                lock.retain(|_, d| (Local::now() - d.last_seen).num_seconds() < 300);
            }
        }
    }

    let _ = adapter.stop_scan().await;
}
