use crate::get_file_path;
use crate::models::{Service, Vendor};
use once_cell::sync::Lazy;
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

const EMBEDDED_COMPANY_IDS_JSON: &str = include_str!("../company_ids.json");
const EMBEDDED_SERVICE_UUIDS_JSON: &str = include_str!("../service_uuids.json");

fn load_vendors() -> HashMap<u16, String> {
    let file_path = get_file_path("company_ids.json");
    let data: Vec<Vendor> = if Path::new(&file_path).exists() {
        let file = File::open(&file_path);
        if let Ok(file) = file {
            serde_json::from_reader(file).unwrap_or_else(|_| {
                serde_json::from_str(EMBEDDED_COMPANY_IDS_JSON).unwrap_or_default()
            })
        } else {
            serde_json::from_str(EMBEDDED_COMPANY_IDS_JSON).unwrap_or_default()
        }
    } else {
        serde_json::from_str(EMBEDDED_COMPANY_IDS_JSON).unwrap_or_default()
    };
    data.into_iter().map(|v| (v.code, v.name)).collect()
}

fn load_services() -> HashMap<String, String> {
    let file_path = get_file_path("service_uuids.json");
    let data: Vec<Service> = if Path::new(&file_path).exists() {
        let file = File::open(&file_path);
        if let Ok(file) = file {
            serde_json::from_reader(file).unwrap_or_else(|_| {
                serde_json::from_str(EMBEDDED_SERVICE_UUIDS_JSON).unwrap_or_default()
            })
        } else {
            serde_json::from_str(EMBEDDED_SERVICE_UUIDS_JSON).unwrap_or_default()
        }
    } else {
        serde_json::from_str(EMBEDDED_SERVICE_UUIDS_JSON).unwrap_or_default()
    };
    data.into_iter()
        .map(|s| (s.uuid.to_lowercase(), s.name))
        .collect()
}

pub static VENDOR_DB: Lazy<HashMap<u16, String>> = Lazy::new(load_vendors);

pub static SERVICE_DB: Lazy<HashMap<String, String>> = Lazy::new(load_services);
