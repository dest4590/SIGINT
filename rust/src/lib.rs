pub mod db;
pub mod models;
pub mod utils;
pub mod sync;

use crate::models::Device;
use btleplug::platform::PeripheralId;
use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jfloat, jint, jobjectArray, jshort, jstring};
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

static RUNTIME: OnceCell<Runtime> = OnceCell::new();
pub static FILES_DIR: OnceCell<String> = OnceCell::new();
pub static JVM: OnceCell<jni::JavaVM> = OnceCell::new();
static SAVED_DEVICES: OnceCell<Arc<HashMap<String, Device>>> = OnceCell::new();
static DEVICES: OnceCell<Arc<Mutex<HashMap<PeripheralId, Device>>>> = OnceCell::new();
static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn get_file_path(filename: &str) -> String {
    if let Some(dir) = FILES_DIR.get() {
        format!("{}/{}", dir, filename)
    } else {
        filename.to_string()
    }
}
mod scanner;
pub use scanner::run_scanner;

unsafe fn vec_to_jlist<'a>(env: &JNIEnv<'a>, items: &[String]) -> JObject<'a> {
    let list_class = env
        .find_class("java/util/ArrayList")
        .expect("ArrayList not found");
    let list = env
        .new_object(list_class, "()V", &[])
        .expect("ArrayList ctor");
    for item in items {
        let jstr = env.new_string(item).expect("new_string");
        env.call_method(
            list,
            "add",
            "(Ljava/lang/Object;)Z",
            &[JValue::from(JObject::from(jstr))],
        )
        .expect("ArrayList.add");
    }
    list
}

const DEVICE_CTOR: &str = "(Ljava/lang/String;Ljava/lang/String;SLjava/util/Map;Ljava/util/List;\
     Ljava/lang/String;Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;\
     FZZSSFLjava/lang/String;Ljava/lang/String;)V";

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_init(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("sigint"),
    );

    if let Ok(jvm) = env.get_java_vm() {
        JVM.get_or_init(|| jvm);
    }

    #[cfg(target_os = "android")]
    {
        let _ = btleplug::platform::init(&env);
    }

    DEVICES.get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
    RUNTIME.get_or_init(|| Runtime::new().expect("tokio runtime"));

    let files_dir: String = env
        .call_method(context, "getFilesDir", "()Ljava/io/File;", &[])
        .and_then(|v| v.l())
        .and_then(|f| env.call_method(f, "getAbsolutePath", "()Ljava/lang/String;", &[]))
        .and_then(|v| v.l())
        .map(JString::from)
        .and_then(|s| env.get_string(s))
        .map(String::from)
        .unwrap_or_else(|_| "/data/local/tmp".to_string());

    FILES_DIR.get_or_init(|| files_dir);

    let loaded_devices = crate::utils::load_stats().unwrap_or_else(|_| HashMap::new());
    SAVED_DEVICES.get_or_init(|| Arc::new(loaded_devices));
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_start(
    _env: JNIEnv,
    _class: JClass,
) {
    if SCAN_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let rt = RUNTIME.get().expect("Runtime not initialized");
    let devices = DEVICES.get().expect("DEVICES not initialized");
    let devices_clone = Arc::clone(devices);
    let saved_devices = SAVED_DEVICES.get().expect("SAVED_DEVICES not initialized");
    let saved_devices_clone = Arc::clone(saved_devices);
    rt.spawn(async move {
        scanner::background_scan(devices_clone, saved_devices_clone, &SCAN_RUNNING).await;
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_stop(
    _env: JNIEnv,
    _class: JClass,
) {
    SCAN_RUNNING.store(false, Ordering::SeqCst);

    let rt = RUNTIME.get().expect("Runtime not initialized");
    let devices = DEVICES.get().expect("DEVICES not initialized");
    let devices = Arc::clone(devices);
    let _ = rt.block_on(async {
        let lock = devices.lock().await;
        crate::utils::save_stats(&lock).await
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_scan(
    env: JNIEnv,
    _class: JClass,
) -> jobjectArray {
    let rt = RUNTIME.get().expect("Runtime not initialized");
    let devices = DEVICES.get().expect("DEVICES not initialized");

    let snapshot: Vec<Device> = rt.block_on(async {
        let lock = devices.lock().await;
        lock.values().cloned().collect()
    });

    let device_class = env
        .find_class("com/dest4590/sigint/sniffer/Device")
        .expect("Device class");
    let array = env
        .new_object_array(snapshot.len() as i32, device_class, JObject::null())
        .expect("array");

    for (i, dev) in snapshot.into_iter().enumerate() {
        let (detailed_name, _) = dev.identify_vendor();

        let id = env.new_string(&dev.id).unwrap();
        let name = env.new_string(&dev.name).unwrap();
        let first_seen = env
            .new_string(dev.first_seen.format("%H:%M:%S").to_string())
            .unwrap();
        let last_seen = env
            .new_string(dev.last_seen.format("%H:%M:%S").to_string())
            .unwrap();
        let vendor = env.new_string(detailed_name).unwrap();
        let device_type = env.new_string(dev.device_type.as_str()).unwrap();

        let history_str = format!(
            "[{}]",
            dev.rssi_history
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let rssi_history_json = env.new_string(&history_str).unwrap();

        let svc_str = dev.services_resolved.join("|");
        let services_resolved = env.new_string(&svc_str).unwrap();

        let map_class = env.find_class("java/util/HashMap").unwrap();
        let manufacturer_data = env.new_object(map_class, "()V", &[]).unwrap();
        let svc_list = unsafe { vec_to_jlist(&env, &dev.services_resolved) };

        let device_obj = env
            .new_object(
                device_class,
                DEVICE_CTOR,
                &[
                    JValue::from(JObject::from(id)),
                    JValue::from(JObject::from(name)),
                    JValue::from(dev.rssi as jshort),
                    JValue::from(manufacturer_data),
                    JValue::from(svc_list),
                    JValue::from(JObject::from(first_seen)),
                    JValue::from(JObject::from(last_seen)),
                    JValue::from(dev.hit_count as jint),
                    JValue::from(JObject::from(vendor)),
                    JValue::from(JObject::from(device_type)),
                    JValue::from(dev.distance_m as jfloat),
                    JValue::from(dev.is_connectable as jboolean),
                    JValue::from(dev.beacon_type.is_some() as jboolean),
                    JValue::from(dev.signal_min as jshort),
                    JValue::from(dev.signal_max as jshort),
                    JValue::from(dev.signal_avg as jfloat),
                    JValue::from(JObject::from(rssi_history_json)),
                    JValue::from(JObject::from(services_resolved)),
                ],
            )
            .expect("Device object");

        env.set_object_array_element(array, i as i32, device_obj)
            .expect("set element");
    }

    array
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_getDeviceCount(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    let rt = RUNTIME.get().expect("Runtime");
    let devices = DEVICES.get().expect("DEVICES");
    rt.block_on(async { devices.lock().await.len() as jint })
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Database_getStatsJson(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let rt = RUNTIME.get().expect("Runtime");
    let devices = DEVICES.get().expect("DEVICES");

    let json = rt.block_on(async {
        let lock = devices.lock().await;
        let total = lock.len();
        let mut type_counts: HashMap<&str, usize> = HashMap::new();
        let mut vendor_counts: HashMap<String, usize> = HashMap::new();
        let (mut rssi_sum, mut rssi_count) = (0i64, 0usize);
        let mut beacons = 0usize;
        let mut connectable = 0usize;

        for dev in lock.values() {
            *type_counts.entry(dev.device_type.as_str()).or_insert(0) += 1;
            let (vendor, _) = dev.identify_vendor();
            *vendor_counts.entry(vendor).or_insert(0) += 1;
            rssi_sum += dev.rssi as i64;
            rssi_count += 1;
            if dev.beacon_type.is_some() { beacons += 1; }
            if dev.is_connectable { connectable += 1; }
        }

        let avg_rssi = if rssi_count > 0 { rssi_sum / rssi_count as i64 } else { 0 };

        let types: String = type_counts
            .iter()
            .map(|(k, v)| format!("\"{}\":{}", k, v))
            .collect::<Vec<_>>()
            .join(",");
        let vendors: String = vendor_counts
            .iter()
            .map(|(k, v)| format!("\"{}\":{}", k.replace('"', "\\\""), v))
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"total\":{},\"beacons\":{},\"connectable\":{},\"avgRssi\":{},\"types\":{{{}}},\"vendors\":{{{}}}}}",
            total, beacons, connectable, avg_rssi, types, vendors
        )
    });

    env.new_string(json).expect("new_string").into_inner()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Database_getSavedStatsJson(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let devices = crate::utils::load_stats().unwrap_or_default();

    let total = devices.len();
    let mut type_counts: HashMap<&str, usize> = HashMap::new();
    let mut vendor_counts: HashMap<String, usize> = HashMap::new();
    let (mut rssi_sum, mut rssi_count) = (0i64, 0usize);
    let mut beacons = 0usize;
    let mut connectable = 0usize;

    for dev in devices.values() {
        *type_counts.entry(dev.device_type.as_str()).or_insert(0) += 1;
        let (vendor, _) = dev.identify_vendor();
        *vendor_counts.entry(vendor).or_insert(0) += 1;
        rssi_sum += dev.rssi as i64;
        rssi_count += 1;
        if dev.beacon_type.is_some() {
            beacons += 1;
        }
        if dev.is_connectable {
            connectable += 1;
        }
    }

    let avg_rssi = if rssi_count > 0 {
        rssi_sum / rssi_count as i64
    } else {
        0
    };
    let types: String = type_counts
        .iter()
        .map(|(k, v)| format!("\"{}\":{}", k, v))
        .collect::<Vec<_>>()
        .join(",");
    let vendors: String = vendor_counts
        .iter()
        .map(|(k, v)| format!("\"{}\":{}", k.replace('"', "\\\""), v))
        .collect::<Vec<_>>()
        .join(",");

    let result = format!(
        "{{\"total\":{},\"beacons\":{},\"connectable\":{},\"avgRssi\":{},\"types\":{{{}}},\"vendors\":{{{}}}}}",
        total, beacons, connectable, avg_rssi, types, vendors
    );
    env.new_string(result).expect("new_string").into_inner()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Database_saveStats(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let rt = RUNTIME.get().expect("Runtime");
    let devices = DEVICES.get().expect("DEVICES");
    let result = rt.block_on(async {
        let lock = devices.lock().await;
        match crate::utils::save_stats(&lock).await {
            Ok(path) => format!("Saved {} devices to: {}", lock.len(), path),
            Err(e) => format!("Error: {:?}", e),
        }
    });
    env.new_string(result).expect("new_string").into_inner()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Database_clear(
    _env: JNIEnv,
    _class: JClass,
) {
    let _ = std::fs::remove_file(crate::get_file_path("device_stats.json"));
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_dest4590_sigint_sniffer_Sniffer_clear(
    _env: JNIEnv,
    _class: JClass,
) {
    let rt = RUNTIME.get().expect("Runtime");
    let devices = DEVICES.get().expect("DEVICES");
    rt.block_on(async {
        devices.lock().await.clear();
    });
}
