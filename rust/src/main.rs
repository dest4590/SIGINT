use btleplug::api::{Central, CentralEvent, Peripheral as _};
use btleplug::platform::PeripheralId;
use chrono::Local;
use clap::Parser;
use colored::*;
use sigint::models::Device;
use sigint::run_scanner;
use sigint::sync::SyncManager;
use sigint::utils::{load_stats, save_stats};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "http://localhost:8000")]
    sync_url: Option<String>,

    #[arg(short, long, default_value = "your-secret-key")]
    api_key: Option<String>,

    #[arg(short, long, default_value_t = 60)]
    sync_interval: u64,

    #[arg(short, long)]
    device_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let devices: Arc<Mutex<HashMap<PeripheralId, Device>>> = Arc::new(Mutex::new(HashMap::new()));
    let devices_clone = Arc::clone(&devices);

    let source_id = args.device_id.unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "unknown-rust-cli".to_string())
    });

    if let Some(sync_url) = &args.sync_url {
        let devices_sync = Arc::clone(&devices);
        let sync_url = sync_url.clone();
        let api_key = args.api_key.clone();
        let source_id = source_id.clone();
        let interval = args.sync_interval;

        tokio::spawn(async move {
            let mut sync_manager = SyncManager::new(sync_url, api_key, source_id);
            let mut ticker = time::interval(Duration::from_secs(interval));
            ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;
                let current_devices = {
                    let lock = devices_sync.lock().await;
                    let mut map = HashMap::new();
                    for (id, dev) in lock.iter() {
                        map.insert(format!("{:?}", id), dev.clone());
                    }
                    map
                };

                if !current_devices.is_empty() {
                    match sync_manager.sync(&current_devices).await {
                        Ok(count) => println!(
                            "{} Successfully synced {} devices to backend",
                            "[SYNC]".bright_green().bold(),
                            count
                        ),
                        Err(e) => eprintln!("{} Sync failed: {}", "[ERROR]".red().bold(), e),
                    }
                }
            }
        });
    }

    tokio::spawn(async move {
        #[cfg(unix)]
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

        tokio::select! {
            _ = tokio::signal::ctrl_c() => println!("\n{} Ctrl-C received, shutting down...", "[SYSTEM]".bright_blue().bold()),
            _ = async {
                #[cfg(unix)]
                sigterm.recv().await;
                #[cfg(not(unix))]
                std::future::pending::<()>().await;
            } => println!("\n{} SIGTERM received, shutting down...", "[SYSTEM]".bright_blue().bold()),
        }

        let final_devices = devices_clone.lock().await;
        if let Err(e) = save_stats(&final_devices).await {
            eprintln!("{} Failed to save stats: {}", "[ERROR]".red().bold(), e);
        }
        std::process::exit(0);
    });

    let saved_devices = Arc::new(load_stats().unwrap_or_default());
    if !saved_devices.is_empty() {
        println!("Loaded {} saved devices from local DB", saved_devices.len());
    }

    println!("{}", "--- [ SIGINT ] ---".bright_yellow().bold());
    if let Some(url) = &args.sync_url {
        println!(
            "{} Syncing to {} every {}s",
            "[INFO]".bright_cyan(),
            url,
            args.sync_interval
        );
    }
    println!(
        "{:<12} | {:<8} | {:<15} | {:<10} | METADATA",
        "TIMESTAMP", "EVENT", "ID", "VENDOR"
    );

    run_scanner(move |event, _manager, adapter, devices| {
        let saved_devices_clone = Arc::clone(&saved_devices);
        async move {
            let now_str = Local::now().format("%H:%M:%S").to_string();
            match event {
                CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => {
                    let peripheral_res = adapter.peripheral(&id).await;
                    if let Ok(peripheral) = peripheral_res
                        && let Ok(Some(props)) = peripheral.properties().await
                    {
                        let mut devices_lock = devices.lock().await;
                        let rssi = props.rssi.unwrap_or(-100);
                        let name = props
                            .local_name
                            .clone()
                            .unwrap_or_else(|| "HIDDEN".to_string());
                        let now = Local::now();

                        let is_new = !devices_lock.contains_key(&id);
                        let id_str = format!("{:?}", id);
                        let rssi_i16 = rssi;
                        let (device_type, confidence) =
                            Device::classify_type(&props.manufacturer_data, &props.services, &name);
                        if device_type == sigint::models::DeviceType::Ignored {
                            return;
                        }
                        let beacon_type =
                            Device::classify_beacon(&props.manufacturer_data, &props.services);
                        let distance_m = Device::estimate_distance(rssi_i16);
                        let address_type = Device::classify_address_type(&id_str);

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

                        let services_resolved = props
                            .services
                            .iter()
                            .map(|uuid| {
                                let s = uuid.to_string().to_lowercase();
                                if let Some(n) = sigint::db::SERVICE_DB.get(&s) {
                                    return n.clone();
                                }
                                if s.ends_with("-0000-1000-8000-00805f9b34fb") {
                                    let short = &s[4..8];
                                    if let Some(n) = sigint::db::SERVICE_DB.get(short) {
                                        return n.clone();
                                    }
                                }
                                uuid.to_string()
                            })
                            .collect::<Vec<_>>();

                        let device = devices_lock.entry(id.clone()).or_insert_with(|| {
                            let device_type = device_type.clone();
                            let beacon_type = beacon_type.clone();
                            let address_type = address_type.clone();
                            let beacon_uuid = beacon_uuid.clone();

                            if let Some(saved) = saved_devices_clone.get(&id_str) {
                                let mut restored = saved.clone();
                                restored.name = name.clone();
                                restored.rssi = rssi_i16;
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
                                    rssi: rssi_i16,
                                    manufacturer_data: props.manufacturer_data.clone(),
                                    services: props.services.clone(),
                                    first_seen: now,
                                    last_seen: now,
                                    hit_count: 0,
                                    device_type,
                                    classification_confidence: confidence,
                                    beacon_type,
                                    distance_m,
                                    is_connectable: true,
                                    rssi_history: Vec::new(),
                                    signal_min: rssi_i16,
                                    signal_max: rssi_i16,
                                    signal_avg: rssi_i16 as f32,
                                    address_type,
                                    beacon_uuid,
                                    beacon_major,
                                    beacon_minor,
                                    services_resolved: services_resolved.clone(),
                                    last_description: None,
                                }
                            }
                        });

                        device.hit_count = device.hit_count.saturating_add(1);
                        device.last_seen = now;
                        device.rssi = rssi_i16;
                        device.services = props.services;
                        device.manufacturer_data = props.manufacturer_data;

                        if confidence >= device.classification_confidence {
                            device.device_type = device_type;
                            device.classification_confidence = confidence;
                        }

                        device.beacon_type = beacon_type;
                        device.distance_m = distance_m;
                        device.services_resolved = services_resolved;
                        device.address_type = address_type;
                        device.is_connectable = true;

                        device.rssi_history.push(rssi_i16);
                        if device.rssi_history.len() > 20 {
                            device.rssi_history.remove(0);
                        }
                        device.signal_min = device.signal_min.min(rssi_i16);
                        device.signal_max = device.signal_max.max(rssi_i16);
                        let sum: f32 = device.rssi_history.iter().map(|&r| r as f32).sum();
                        device.signal_avg = sum / device.rssi_history.len() as f32;

                        if beacon_uuid.is_some() {
                            device.beacon_uuid = beacon_uuid;
                            device.beacon_major = beacon_major;
                            device.beacon_minor = beacon_minor;
                        }

                        let (v_name, v_color) = device.identify_vendor();
                        let state_changed = device.last_description.as_ref() != Some(&v_name);
                        device.last_description = Some(v_name.clone());

                        if is_new || state_changed {
                            let services = device.identify_services();
                            let services_str = if services.is_empty() {
                                "".to_string()
                            } else {
                                format!(" SERVICES: [{}]", services.join(", "))
                            };

                            let event_type = if is_new {
                                "DETECTED".green().bold()
                            } else {
                                "UPDATED ".cyan().bold()
                            };

                            println!(
                                "{:<12} | {:<8} | {:<15} | {:<10} | {}",
                                now_str.dimmed(),
                                event_type,
                                format!("{:?}", id)
                                    .chars()
                                    .take(15)
                                    .collect::<String>()
                                    .bright_black(),
                                v_name.color(v_color).bold(),
                                format!("NAME: {} RSSI: {}dBm{}", name, device.rssi, services_str)
                                    .white()
                            );
                        }

                        if devices_lock.len() % 10 == 0 && !devices_lock.is_empty() {
                            let _ = save_stats(&devices_lock).await;
                        }
                    }
                }
                _ => {}
            }
        }
    })
    .await?;

    Ok(())
}
