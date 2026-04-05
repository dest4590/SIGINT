use crate::models::Device;
use chrono::{DateTime, Local};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;

#[derive(Serialize, Deserialize, Debug)]
pub struct Finding {
    pub id: String,
    pub name: String,
    pub rssi: i16,
    pub manufacturer_data: HashMap<u16, String>,
    pub services: Vec<String>,
    pub first_seen: String,
    pub last_seen: String,
    pub hit_count: u32,
    pub device_type: String,
    pub beacon_type: Option<String>,
    pub distance_m: f32,
    pub is_connectable: bool,
    pub rssi_history: Vec<i16>,
    pub signal_min: i16,
    pub signal_max: i16,
    pub signal_avg: f32,
    pub address_type: String,
    pub beacon_uuid: Option<String>,
    pub beacon_major: Option<u16>,
    pub beacon_minor: Option<u16>,
    pub services_resolved: Vec<String>,
    pub source_device_id: String,
}

impl From<(&Device, &str)> for Finding {
    fn from((device, source_id): (&Device, &str)) -> Self {
        let mut manufacturer_data = HashMap::new();
        for (k, v) in &device.manufacturer_data {
            manufacturer_data.insert(*k, hex::encode(v));
        }

        Finding {
            id: device.id.clone(),
            name: device.name.clone(),
            rssi: device.rssi,
            manufacturer_data,
            services: device.services.iter().map(|u| u.to_string()).collect(),
            first_seen: device.first_seen.to_rfc3339(),
            last_seen: device.last_seen.to_rfc3339(),
            hit_count: device.hit_count,
            device_type: device.device_type.as_str().to_string(),
            beacon_type: device.beacon_type.as_ref().map(|b| format!("{:?}", b)),
            distance_m: device.distance_m,
            is_connectable: device.is_connectable,
            rssi_history: device.rssi_history.clone(),
            signal_min: device.signal_min,
            signal_max: device.signal_max,
            signal_avg: device.signal_avg,
            address_type: device.address_type.clone(),
            beacon_uuid: device.beacon_uuid.clone(),
            beacon_major: device.beacon_major,
            beacon_minor: device.beacon_minor,
            services_resolved: device.services_resolved.clone(),
            source_device_id: source_id.to_string(),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct SyncResponse {
    pub status: String,
    pub synced_count: u32,
    pub new_findings: Vec<Finding>,
}

pub struct SyncManager {
    client: Client,
    base_url: String,
    api_key: Option<String>,
    source_id: String,
    last_sync: Option<DateTime<Local>>,
}

impl SyncManager {
    pub fn new(base_url: String, api_key: Option<String>, source_id: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            api_key,
            source_id,
            last_sync: None,
        }
    }

    pub async fn sync(&mut self, devices: &HashMap<String, Device>) -> Result<u32, Box<dyn Error>> {
        let findings: Vec<Finding> = devices
            .values()
            .map(|d| Finding::from((d, self.source_id.as_str())))
            .collect();

        let mut url = format!("{}/sync", self.base_url);
        if let Some(last) = self.last_sync {
            url.push_str(&format!("?since={}", last.to_rfc3339()));
        }

        let mut request = self.client.post(&url).json(&findings);

        if let Some(key) = &self.api_key {
            request = request.header("X-API-Key", key);
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(format!("Sync failed with status: {}", response.status()).into());
        }

        let sync_res: SyncResponse = response.json().await?;
        self.last_sync = Some(Local::now());

        Ok(sync_res.synced_count)
    }
}
