use crate::get_file_path;
use crate::models::Device;
use btleplug::platform::PeripheralId;
use serde_json;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub fn load_stats() -> Result<HashMap<String, Device>, Box<dyn Error>> {
    let stats_file = get_file_path("device_stats.json");
    if !Path::new(&stats_file).exists() {
        return Ok(HashMap::new());
    }

    let mut file = File::open(&stats_file)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let devices: HashMap<String, Device> = serde_json::from_str(&contents)?;
    Ok(devices)
}

pub async fn save_stats(devices: &HashMap<PeripheralId, Device>) -> Result<String, Box<dyn Error>> {
    let stats_file = get_file_path("device_stats.json");

    let mut all_devices = load_stats().unwrap_or_default();

    for (id, dev) in devices {
        let id_str = format!("{:?}", id);
        all_devices.insert(id_str, dev.clone());
    }

    let json = serde_json::to_string_pretty(&all_devices)?;
    let mut file = File::create(&stats_file)?;
    file.write_all(json.as_bytes())?;
    log::info!(
        "Stats saved to {} (total {} devices)",
        stats_file,
        all_devices.len()
    );
    Ok(stats_file)
}

pub async fn save_stats_by_id(devices: &HashMap<String, Device>) -> Result<String, Box<dyn Error>> {
    let stats_file = get_file_path("device_stats.json");

    let mut all_devices = load_stats().unwrap_or_default();

    for (id, dev) in devices {
        all_devices.insert(id.clone(), dev.clone());
    }

    let json = serde_json::to_string_pretty(&all_devices)?;
    let mut file = File::create(&stats_file)?;
    file.write_all(json.as_bytes())?;
    log::info!(
        "Stats saved to {} (total {} devices)",
        stats_file,
        all_devices.len()
    );
    Ok(stats_file)
}
