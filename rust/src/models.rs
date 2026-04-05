use crate::db::{SERVICE_DB, VENDOR_DB};
use chrono::{DateTime, Local};
use colored::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Vendor {
    pub code: u16,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Service {
    pub uuid: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DeviceType {
    Phone,
    Laptop,
    Headphones,
    Speaker,
    Watch,
    Beacon,
    HeartRate,
    Keyboard,
    Mouse,
    Printer,
    TV,
    IoT,
    Car,
    Medical,
    Fitness,
    Unknown,
    Ignored,
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeviceType::Phone => "PHONE",
            DeviceType::Laptop => "LAPTOP",
            DeviceType::Headphones => "HEADPHONES",
            DeviceType::Speaker => "SPEAKER",
            DeviceType::Watch => "WATCH",
            DeviceType::Beacon => "BEACON",
            DeviceType::HeartRate => "HEART RATE",
            DeviceType::Keyboard => "KEYBOARD",
            DeviceType::Mouse => "MOUSE",
            DeviceType::Printer => "PRINTER",
            DeviceType::TV => "TV",
            DeviceType::IoT => "IOT",
            DeviceType::Car => "CAR",
            DeviceType::Medical => "MEDICAL",
            DeviceType::Fitness => "FITNESS",
            DeviceType::Unknown => "UNKNOWN",
            DeviceType::Ignored => "IGNORED",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum BeaconType {
    IBeacon,
    Eddystone,
    AltBeacon,
    Exposure,
    Tile,
    TrackR,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub rssi: i16,
    pub manufacturer_data: HashMap<u16, Vec<u8>>,
    pub services: Vec<Uuid>,
    pub first_seen: DateTime<Local>,
    pub last_seen: DateTime<Local>,
    pub hit_count: u32,
    pub device_type: DeviceType,
    pub beacon_type: Option<BeaconType>,
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
    #[serde(skip)]
    pub last_description: Option<String>,
    #[serde(default)]
    pub classification_confidence: u8,
}

impl Device {
    pub fn parse_apple_continuity(data: &[u8]) -> (DeviceType, String) {
        if data.len() < 2 {
            return (DeviceType::Unknown, "Apple Device".to_string());
        }

        let apple_type = data[0];
        let apple_len = data[1] as usize;

        if data.len() < 2 + apple_len {
            return (DeviceType::Unknown, "Apple Device (Truncated)".to_string());
        }

        match apple_type {
            0x10 if apple_len >= 1 => {
                let status_byte = data[2];

                let screen_on = (status_byte & 0x08) == 0;
                let screen_str = if screen_on { "ON" } else { "OFF" };

                let device_class = status_byte & 0x0F;
                match device_class {
                    0x01 => (
                        DeviceType::Phone,
                        format!("iPhone (Screen: {})", screen_str),
                    ),
                    0x02 => (
                        DeviceType::Laptop,
                        format!("iPad/Mac (Screen: {})", screen_str),
                    ),
                    0x03 => (
                        DeviceType::Watch,
                        format!("Apple Watch (Screen: {})", screen_str),
                    ),
                    _ => (
                        DeviceType::Unknown,
                        format!(
                            "Apple Device 0x{:02X} (Screen: {})",
                            device_class, screen_str
                        ),
                    ),
                }
            }
            0x07 => (DeviceType::Headphones, "AirPods/Beats".to_string()),
            0x05 => (DeviceType::Phone, "iPhone (AirDrop)".to_string()),
            0x09 => (DeviceType::Phone, "Apple Device (Handoff)".to_string()),
            0x02 if apple_len == 0x15 => (DeviceType::Beacon, "iBeacon".to_string()),
            0x01 => (DeviceType::Unknown, "Apple (Nearby Action)".to_string()),
            0x06 => (DeviceType::IoT, "Apple (HomeKit)".to_string()),
            0x08 => (DeviceType::Unknown, "Apple (Hey Siri)".to_string()),
            0x0b => (DeviceType::Unknown, "Apple (Nearby Info)".to_string()),
            0x0c => (DeviceType::Unknown, "Apple (Find My)".to_string()),
            0x12 => (DeviceType::Ignored, "Apple (Type 0x12)".to_string()),
            0x10 => (DeviceType::Phone, "Apple Continuity".to_string()),
            _ => (
                DeviceType::Unknown,
                format!("Apple (Type 0x{:02X})", apple_type),
            ),
        }
    }
    pub fn identify_vendor(&self) -> (String, Color) {
        if let Some(apple_data) = self.manufacturer_data.get(&0x004Cu16) {
            let (_, description) = Self::parse_apple_continuity(apple_data);
            return (description.to_uppercase(), Color::White);
        }

        for &code in self.manufacturer_data.keys() {
            if let Some(name) = VENDOR_DB.get(&code) {
                let color = match code {
                    0x0006 => Color::Blue,
                    0x0075 => Color::Cyan,
                    0x00E0 => Color::Green,
                    0x000a => Color::Red,
                    0x004c => Color::White,
                    _ => Color::Yellow,
                };
                return (name.to_uppercase(), color);
            }
        }
        (
            "UNKNOWN".to_string(),
            Color::TrueColor {
                r: 100,
                g: 100,
                b: 100,
            },
        )
    }

    pub fn identify_services(&self) -> Vec<String> {
        self.services
            .iter()
            .map(|uuid| {
                let s = uuid.to_string().to_lowercase();
                if let Some(name) = SERVICE_DB.get(&s) {
                    return name.clone();
                }
                if s.ends_with("-0000-1000-8000-00805f9b34fb") {
                    let short = &s[4..8];
                    if let Some(name) = SERVICE_DB.get(short) {
                        return name.clone();
                    }
                }
                "Unknown".to_string()
            })
            .collect()
    }

    pub fn classify_type(
        manufacturer_data: &HashMap<u16, Vec<u8>>,
        services: &[Uuid],
        name: &str,
    ) -> (DeviceType, u8) {
        let name_lower = name.to_lowercase();

        if name_lower.contains("tv")
            || name_lower.contains("samsung") && name_lower.contains("series")
            || name_lower.contains("lg") && name_lower.contains("webos")
        {
            return (DeviceType::TV, 95);
        }
        if name_lower.contains("watch") || name_lower.contains("fitbit") || name_lower.contains("garmin") {
            return (DeviceType::Watch, 95);
        }
        if name_lower.contains("headphone")
            || name_lower.contains("earbud")
            || name_lower.contains("airpods")
            || name_lower.contains("beats")
            || name_lower.contains("sony") && name_lower.contains("wh-")
        {
            return (DeviceType::Headphones, 95);
        }
        if name_lower.contains("tesla") || name_lower.contains("bmw") || name_lower.contains("audi") {
            return (DeviceType::Car, 90);
        }

        if let Some(data) = manufacturer_data.get(&0x004Cu16) {
            let (dtype, _) = Self::parse_apple_continuity(data);
            if dtype != DeviceType::Unknown {
                return (dtype, 100);
            }
            return (DeviceType::Unknown, 80);
        }

        if manufacturer_data.contains_key(&0x0006) {
            return (DeviceType::Laptop, 90);
        }

        if let Some(_) = manufacturer_data.get(&0x0075u16) {
            return (DeviceType::Phone, 85);
        }

        if manufacturer_data.contains_key(&0x00E0) || manufacturer_data.contains_key(&0x027D) {
            return (DeviceType::Phone, 85);
        }

        for uuid in services {
            let s = uuid.to_string().to_lowercase();
            let short = if s.ends_with("-0000-1000-8000-00805f9b34fb") {
                &s[4..8]
            } else {
                &s
            };
            match short {
                "180d" | "1810" => return (DeviceType::HeartRate, 95),
                "1812" => return (DeviceType::Keyboard, 95),
                "1803" | "1804" => return (DeviceType::IoT, 70),
                "0000110b" | "110b" | "110c" | "110e" => return (DeviceType::Speaker, 90),
                "0000111f" | "111f" | "1108" => return (DeviceType::Headphones, 90),
                "00001811" | "1811" => return (DeviceType::Phone, 60),
                "00001819" | "1819" => return (DeviceType::IoT, 80),
                "0000fe11" | "fe11" => return (DeviceType::TV, 95),
                "181d" => return (DeviceType::Fitness, 90),
                "181e" => return (DeviceType::Medical, 90),
                "feaa" | "fd6f" | "feed" => return (DeviceType::Beacon, 100),
                _ => {}
            }
        }

        if !manufacturer_data.is_empty() {
            return (DeviceType::Unknown, 20);
        }

        (DeviceType::Unknown, 0)
    }

    pub fn estimate_distance(rssi: i16) -> f32 {
        if rssi == 0 {
            return -1.0;
        }
        let tx_power = -59.0;
        10_f32.powf((tx_power - rssi as f32) / 20.0)
    }

    pub fn classify_beacon(
        manufacturer_data: &HashMap<u16, Vec<u8>>,
        services: &[Uuid],
    ) -> Option<BeaconType> {
        if let Some(data) = manufacturer_data.get(&0x004Cu16)
            && data.len() >= 2
            && data[0] == 0x02
            && data[1] == 0x15
        {
            return Some(BeaconType::IBeacon);
        }
        for uuid in services {
            let s = uuid.to_string().to_lowercase();
            if s.starts_with("0000feaa") {
                return Some(BeaconType::Eddystone);
            }
            if s.starts_with("0000fd6f") {
                return Some(BeaconType::Exposure);
            }
        }
        None
    }

    pub fn parse_ibeacon(data: &[u8]) -> Option<(String, u16, u16)> {
        if data.len() < 22 || data[0] != 0x02 || data[1] != 0x15 {
            return None;
        }
        let uuid = data[2..18]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("");
        let major = u16::from_be_bytes([data[18], data[19]]);
        let minor = u16::from_be_bytes([data[20], data[21]]);
        Some((uuid, major, minor))
    }

    pub fn classify_address_type(mac: &str) -> String {
        let first_byte = mac.split(':').next().unwrap_or("00");
        if let Ok(b) = u8::from_str_radix(first_byte, 16) {
            match b & 0xC0 {
                0xC0 => "random static",
                0x40 => "random resolvable",
                0x00 => "public",
                _ => "random non-resolvable",
            }
            .to_string()
        } else {
            "unknown".to_string()
        }
    }
}
