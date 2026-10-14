//! Read-only sysfs telemetry and bounded direct NVML sampling.
use crate::Result;
use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    error::NvmlError,
    Nvml,
};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

fn read(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(value) => value.trim().to_owned(),
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("Resource telemetry: {error}");
            }
            String::new()
        }
    }
}
fn entries(path: &Path) -> Vec<PathBuf> {
    match fs::read_dir(path) {
        Ok(entries) => {
            let mut result = Vec::new();
            for entry in entries {
                match entry {
                    Ok(entry) => result.push(entry.path()),
                    Err(error) => eprintln!("Resource directory: {error}"),
                }
            }
            result.sort();
            result
        }
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("Resource directory: {error}");
            }
            Vec::new()
        }
    }
}
fn resolved(path: &Path) -> PathBuf {
    let mut target = path.to_path_buf();
    for _ in 0..40 {
        if let Ok(path) = fs::canonicalize(&target) {
            return path;
        }
        match fs::read_link(&target) {
            Ok(link) => {
                target = if link.is_absolute() {
                    link
                } else {
                    target.parent().unwrap_or(Path::new("/")).join(link)
                }
            }
            Err(error) => {
                if !matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
                ) {
                    eprintln!("Resource link: {error}");
                }
                return target;
            }
        }
    }
    eprintln!("Resource link: excessive symbolic link depth");
    target
}
pub fn numeric(text: &str, scale: f64, low: Option<f64>, high: Option<f64>) -> Option<f64> {
    let value = text.parse::<f64>().ok()? / scale;
    (value.is_finite()
        && !low.is_some_and(|low| value < low)
        && !high.is_some_and(|high| value > high))
    .then_some(value)
}
fn metric(path: &Path, scale: f64, low: f64, high: f64) -> Option<f64> {
    numeric(&read(path), scale, Some(low), Some(high))
}
fn temperatures(hwmon: &Path) -> Vec<f64> {
    entries(hwmon)
        .iter()
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("temp") && name.ends_with("_input")
            })
        })
        .filter_map(|path| metric(path, 1000.0, -40.0, 150.0))
        .collect()
}
fn maximum(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    values.into_iter().max_by(f64::total_cmp)
}
pub fn pci_id(value: &str) -> String {
    Regex::new(r"\A(?:0{4})?([0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7])\z")
        .expect("Static PCI pattern")
        .captures(&value.to_ascii_lowercase())
        .map(|value| value[1].to_owned())
        .unwrap_or_default()
}

fn gpu_devices(root: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut devices = BTreeMap::new();
    let card_pattern = Regex::new(r"\Acard\d+\z").expect("Static DRM pattern");
    for card in entries(&root.join("class/drm")) {
        if !card_pattern.is_match(&card.file_name().unwrap_or_default().to_string_lossy()) {
            continue;
        }
        let device = resolved(&card.join("device"));
        if device.exists() {
            devices.insert(device, Some(card));
        }
    }
    for device in entries(&root.join("bus/pci/devices")) {
        if read(&device.join("class")).starts_with("0x03") {
            devices.entry(resolved(&device)).or_insert(None);
        }
    }
    devices.into_iter().collect()
}
fn inventory(devices: &[(PathBuf, Option<PathBuf>)]) -> Vec<Value> {
    devices.iter().map(|(device,card)| {
        let name=device.file_name().unwrap_or_default().to_string_lossy();
        let ident=pci_id(&name);
        let vendor=read(&device.join("vendor"));
        let device_id=read(&device.join("device"));
        let driver=device.join("driver");
        json!({"id":if ident.is_empty(){card.as_ref().unwrap_or(device).file_name().unwrap_or_default().to_string_lossy().into_owned()}else{ident},"card":card.as_ref().map(|card|card.file_name().unwrap_or_default().to_string_lossy().into_owned()),"vendor":match vendor.as_str(){"0x1002"=>"AMD","0x10de"=>"NVIDIA","0x8086"=>"Intel",_=>"Unknown"},"deviceId":(!device_id.is_empty()).then_some(device_id),"driver":driver.is_symlink().then(||resolved(&driver).file_name().unwrap_or_default().to_string_lossy().into_owned())})
    }).collect()
}

fn optional<T>(result: std::result::Result<T, NvmlError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(NvmlError::NotSupported) => None,
        Err(error) => {
            eprintln!("NVIDIA telemetry: {error}");
            None
        }
    }
}
fn nvml_sample(nvml: &Nvml, ids: &[String], deadline: Instant) -> Result<BTreeMap<String, Value>> {
    let driver = nvml.sys_driver_version()?;
    let mut samples = BTreeMap::new();
    for id in ids {
        if Instant::now() >= deadline {
            return Err("NVIDIA telemetry deadline expired".into());
        }
        let device = match nvml.device_by_pci_bus_id(id.as_str()) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("NVIDIA device lookup: {error}");
                continue;
            }
        };
        let temperature = optional(device.temperature(TemperatureSensor::Gpu))
            .map(f64::from)
            .filter(|value| (-40.0..=150.0).contains(value));
        let usage = optional(device.utilization_rates())
            .map(|value| f64::from(value.gpu) / 100.0)
            .filter(|value| (0.0..=1.0).contains(value));
        let frequency = optional(device.clock_info(Clock::Graphics))
            .map(f64::from)
            .filter(|value| (0.0..=100000.0).contains(value));
        let name = optional(device.name()).unwrap_or_else(|| "N/A".into());
        samples.insert(id.clone(),json!({"name":name,"temperature":temperature,"usage":usage,"frequencyMHz":frequency,"driverVersion":driver}));
    }
    Ok(samples)
}
struct NvmlRequest {
    ids: Vec<String>,
    deadline: Instant,
    reply: mpsc::SyncSender<std::result::Result<BTreeMap<String, Value>, String>>,
}
pub fn nvidia_samples(ids: &[String]) -> BTreeMap<String, Value> {
    static WORKER: OnceLock<std::result::Result<mpsc::SyncSender<NvmlRequest>, String>> =
        OnceLock::new();
    if ids.is_empty() {
        return BTreeMap::new();
    }
    let worker = WORKER.get_or_init(|| {
        let (send, receive) = mpsc::sync_channel::<NvmlRequest>(1);
        std::thread::Builder::new()
            .name("nacre-nvml".into())
            .spawn(move || {
                let mut library = None;
                for request in receive {
                    if request.deadline <= Instant::now() {
                        continue;
                    }
                    let result = (|| -> Result<BTreeMap<String, Value>> {
                        if library.is_none() {
                            library = Some(Nvml::init()?);
                        }
                        let result = nvml_sample(
                            library.as_ref().expect("Initialized NVML"),
                            &request.ids,
                            request.deadline,
                        )?;
                        if Instant::now() >= request.deadline {
                            return Err("NVIDIA telemetry deadline expired".into());
                        }
                        Ok(result)
                    })();
                    if request
                        .reply
                        .send(result.map_err(|error| error.to_string()))
                        .is_err()
                    {
                        eprintln!("NVIDIA telemetry: caller deadline expired");
                    }
                }
                if let Some(library) = library {
                    if let Err(error) = library.shutdown() {
                        eprintln!("NVIDIA telemetry shutdown: {error}");
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(send)
    });
    let result = (|| -> std::result::Result<BTreeMap<String, Value>, String> {
        let worker = worker.as_ref().map_err(Clone::clone)?;
        let (reply, receive) = mpsc::sync_channel(1);
        let deadline = Instant::now() + Duration::from_secs(3);
        worker
            .try_send(NvmlRequest {
                ids: ids.to_vec(),
                deadline,
                reply,
            })
            .map_err(|error| error.to_string())?;
        receive
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|error| error.to_string())?
    })();
    match result {
        Ok(samples) => samples,
        Err(error) => {
            eprintln!("NVIDIA telemetry: {error}");
            BTreeMap::new()
        }
    }
}

pub fn sensors(
    root: &Path,
    sample: &mut impl FnMut(&[String]) -> BTreeMap<String, Value>,
) -> Value {
    let mut cpu = Vec::new();
    let devices = gpu_devices(root);
    let mut records = inventory(&devices);
    let mut associated = HashSet::new();
    for (record, (device, card)) in records.iter_mut().zip(&devices) {
        let mut hwmons: Vec<_> = entries(&device.join("hwmon"))
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("hwmon"))
            })
            .collect();
        hwmons.extend(
            entries(&root.join("class/hwmon"))
                .into_iter()
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("hwmon"))
                        && resolved(&path.join("device")) == *device
                }),
        );
        let mut temps = Vec::new();
        for hwmon in hwmons {
            associated.insert(resolved(&hwmon));
            temps.extend(temperatures(&hwmon));
        }
        let mut frequency_paths = vec![device.join("gt_act_freq_mhz")];
        if let Some(card) = card {
            frequency_paths.push(card.join("gt_act_freq_mhz"));
            frequency_paths.extend(
                entries(&card.join("gt"))
                    .into_iter()
                    .filter(|path| {
                        path.file_name()
                            .is_some_and(|name| name.to_string_lossy().starts_with("gt"))
                    })
                    .map(|path| path.join("rps_act_freq_mhz")),
            );
        }
        for tile in entries(device).into_iter().filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("tile"))
        }) {
            for gt in entries(&tile).into_iter().filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("gt"))
            }) {
                frequency_paths.extend(
                    entries(&gt)
                        .into_iter()
                        .filter(|path| {
                            path.file_name()
                                .is_some_and(|name| name.to_string_lossy().starts_with("freq"))
                        })
                        .map(|path| path.join("act_freq")),
                );
            }
        }
        record["temperature"] = json!(maximum(temps));
        record["usage"] = json!(metric(&device.join("gpu_busy_percent"), 100.0, 0.0, 1.0));
        record["frequencyMHz"] = json!(maximum(
            frequency_paths
                .iter()
                .filter_map(|path| metric(path, 1.0, 0.0, 100000.0))
        ));
    }
    for hwmon in entries(&root.join("class/hwmon"))
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("hwmon"))
        })
    {
        let name = read(&hwmon.join("name")).to_lowercase();
        if ["coretemp", "k10temp", "zenpower", "cpu_thermal"].contains(&name.as_str()) {
            for source in entries(&hwmon).into_iter().filter(|path| {
                path.file_name().is_some_and(|name| {
                    let name = name.to_string_lossy();
                    name.starts_with("temp") && name.ends_with("_input")
                })
            }) {
                if let Some(value) = metric(&source, 1000.0, -40.0, 150.0) {
                    let label = read(
                        &source.with_file_name(
                            source
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .replace("_input", "_label"),
                        ),
                    )
                    .to_lowercase();
                    cpu.push((
                        ["package", "tctl", "tdie"]
                            .iter()
                            .any(|word| label.contains(word)),
                        value,
                    ));
                }
            }
        } else if ["amdgpu", "nouveau", "i915", "xe"]
            .contains(&name.split("_gt").next().unwrap_or(""))
            && !associated.contains(&resolved(&hwmon))
        {
            records.push(json!({"id":hwmon.file_name().unwrap_or_default().to_string_lossy(),"card":null,"vendor":match name.as_str(){"amdgpu"=>"AMD","nouveau"=>"NVIDIA",_=>"Intel"},"driver":name,"temperature":maximum(temperatures(&hwmon)),"usage":null,"frequencyMHz":null}));
        }
    }
    let ids: Vec<_> = records
        .iter()
        .filter(|record| record["driver"] == "nvidia")
        .filter_map(|record| record["id"].as_str().map(str::to_owned))
        .collect();
    if !ids.is_empty() {
        let samples = sample(&ids);
        for record in &mut records {
            if record["driver"] == "nvidia" {
                if let Some(sample) = record["id"]
                    .as_str()
                    .and_then(|id| samples.get(id))
                    .and_then(Value::as_object)
                {
                    record
                        .as_object_mut()
                        .expect("GPU record")
                        .extend(sample.clone());
                }
            }
        }
    }
    let preferred: Vec<_> = cpu
        .iter()
        .filter(|(package, _)| *package)
        .map(|(_, value)| *value)
        .collect();
    let usage: Vec<_> = records
        .iter()
        .filter_map(|record| record["usage"].as_f64())
        .collect();
    json!({"cpuTemp":maximum(if preferred.is_empty(){cpu.into_iter().map(|(_,value)|value).collect()}else{preferred}),"gpuTemp":maximum(records.iter().filter_map(|record|record["temperature"].as_f64())),"gpuPerc":maximum(usage.iter().copied()),"gpuUsageAvailable":!usage.is_empty(),"gpuFrequencyMHz":maximum(records.iter().filter_map(|record|record["frequencyMHz"].as_f64())),"gpus":records})
}

fn storage_kib(blocks: u64, free: u64, block_size: u64) -> (f64, f64) {
    (
        (blocks - free) as f64 * block_size as f64 / 1024.0,
        blocks as f64 * block_size as f64 / 1024.0,
    )
}
pub fn snapshot(root: &Path, mount: &Path) -> Value {
    let mut result = sensors(root, &mut nvidia_samples);
    match rustix::fs::statvfs(mount) {
        Ok(stats) => {
            let (used, total) = storage_kib(stats.f_blocks, stats.f_bfree, stats.f_frsize);
            result["storageUsed"] = json!(used);
            result["storageTotal"] = json!(total);
        }
        Err(error) => {
            eprintln!("Resource storage telemetry: {error}");
            result["storageUsed"] = Value::Null;
            result["storageTotal"] = Value::Null;
        }
    }
    result
}
pub fn gpu_inventory(root: &Path) -> Value {
    Value::Array(inventory(&gpu_devices(root)))
}
pub fn execute(arguments: &[String], _input: Option<Value>) -> Result<Option<Value>> {
    if arguments
        .first()
        .is_some_and(|action| action == "inventory")
    {
        return Ok(Some(gpu_inventory(Path::new(
            arguments.get(1).map(String::as_str).unwrap_or("/sys"),
        ))));
    }
    Ok(Some(snapshot(Path::new("/sys"), Path::new("/"))))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(root: &Path, name: &str, value: &str) -> Result<()> {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, value)?;
        Ok(())
    }
    fn gpu(root: &Path, card: &str, ident: &str, vendor: &str, driver: &str) -> Result<PathBuf> {
        let device = root.join("bus/pci/devices").join(ident);
        fs::create_dir_all(&device)?;
        fs::write(device.join("class"), "0x030000")?;
        fs::write(device.join("vendor"), vendor)?;
        fs::write(device.join("device"), "0x1234")?;
        std::os::unix::fs::symlink(
            root.join("bus/pci/drivers").join(driver),
            device.join("driver"),
        )?;
        let target = root.join("class/drm").join(card);
        fs::create_dir_all(&target)?;
        std::os::unix::fs::symlink(&device, target.join("device"))?;
        Ok(device)
    }
    #[test]
    fn cpu_package_priority_and_optional_metrics_are_preserved() -> Result<()> {
        let root = tempfile::tempdir()?;
        put(root.path(), "class/hwmon/hwmon0/name", "coretemp")?;
        put(root.path(), "class/hwmon/hwmon0/temp1_input", "42000")?;
        put(
            root.path(),
            "class/hwmon/hwmon0/temp1_label",
            "Package id 0",
        )?;
        put(root.path(), "class/hwmon/hwmon0/temp2_input", "99000")?;
        let result = sensors(root.path(), &mut |_| panic!("No NVIDIA query expected"));
        assert_eq!(result["cpuTemp"], 42.0);
        assert_eq!(result["gpuTemp"], Value::Null);
        assert!(!result["gpuUsageAvailable"].as_bool().unwrap());
        for text in ["NaN", "inf", "-100000", "999999", ""] {
            assert!(numeric(text, 1000.0, Some(-40.0), Some(150.0)).is_none());
        }
        Ok(())
    }
    #[test]
    fn vendor_identity_idle_zero_and_frequency_paths_are_preserved() -> Result<()> {
        let root = tempfile::tempdir()?;
        let amd = gpu(root.path(), "card0", "0000:01:00.0", "0x1002", "amdgpu")?;
        put(&amd, "gpu_busy_percent", "0")?;
        put(&amd, "hwmon/hwmon0/temp1_input", "45000")?;
        let i915 = gpu(root.path(), "card1", "0000:00:02.0", "0x8086", "i915")?;
        put(&i915, "gt_act_freq_mhz", "350")?;
        put(&i915, "hwmon/hwmon1/temp1_input", "51000")?;
        let intel = gpu(root.path(), "card2", "0000:03:00.0", "0x8086", "xe")?;
        put(&intel, "tile0/gt0/freq0/act_freq", "750")?;
        put(&intel, "tile0/gt1/freq0/act_freq", "950")?;
        put(&intel, "hwmon/hwmon2/temp1_input", "62000")?;
        put(&intel, "gpu_busy_percent", "20")?;
        let result = sensors(root.path(), &mut |_| panic!("No NVIDIA query expected"));
        assert_eq!(result["gpus"].as_array().unwrap().len(), 3);
        let rows: BTreeMap<_, _> = result["gpus"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["driver"].as_str().unwrap(), row))
            .collect();
        assert_eq!(rows["amdgpu"]["vendor"], "AMD");
        assert_eq!(rows["amdgpu"]["usage"], 0.0);
        assert_eq!(rows["i915"]["usage"], Value::Null);
        assert_eq!(rows["i915"]["temperature"], 51.0);
        assert_eq!(rows["i915"]["frequencyMHz"], 350.0);
        assert_eq!(rows["xe"]["frequencyMHz"], 950.0);
        assert_eq!(result["gpuTemp"], 62.0);
        assert_eq!(result["gpuPerc"], 0.2);
        assert_eq!(result["gpuFrequencyMHz"], 950.0);
        assert_eq!(result["gpuUsageAvailable"], true);
        Ok(())
    }
    #[test]
    fn nvidia_partial_metrics_missing_driver_and_hwmon_deduplication() -> Result<()> {
        let root = tempfile::tempdir()?;
        let device = gpu(root.path(), "card0", "0000:01:00.0", "0x10de", "nvidia")?;
        let other = gpu(root.path(), "card1", "0000:02:00.0", "0x10de", "nvidia")?;
        let result = sensors(root.path(), &mut |ids| {
            assert_eq!(ids.len(), 2);
            BTreeMap::from([
                (
                    ids[0].clone(),
                    json!({"name":"NVIDIA Fixture","temperature":60.0,"usage":0.0,"frequencyMHz":210.0,"driverVersion":"580.0"}),
                ),
                (
                    ids[1].clone(),
                    json!({"name":"NVIDIA Other","temperature":null,"usage":0.5,"frequencyMHz":null,"driverVersion":"580.0"}),
                ),
            ])
        });
        assert_eq!(result["gpuPerc"], 0.5);
        assert_eq!(result["gpus"][0]["usage"], 0.0);
        assert_eq!(result["gpus"][1]["temperature"], Value::Null);
        assert_eq!(pci_id("00000000:01:00.0"), "0000:01:00.0");
        fs::remove_file(device.join("driver"))?;
        fs::remove_file(other.join("driver"))?;
        put(&device, "hwmon/hwmon0/name", "amdgpu")?;
        put(&device, "hwmon/hwmon0/temp1_input", "45000")?;
        fs::create_dir_all(root.path().join("class/hwmon"))?;
        std::os::unix::fs::symlink(
            device.join("hwmon/hwmon0"),
            root.path().join("class/hwmon/hwmon0"),
        )?;
        let result = sensors(root.path(), &mut |_| {
            panic!("Unbound GPUs must not query NVIDIA")
        });
        assert_eq!(result["gpus"].as_array().unwrap().len(), 2);
        assert_eq!(result["gpuTemp"], 45.0);
        assert_eq!(result["gpus"][0]["driver"], Value::Null);
        Ok(())
    }
    #[test]
    fn storage_totals_use_kib_without_signed_overflow() {
        assert_eq!(
            storage_kib(2_147_483_648, 1_342_177_280, 4096),
            (3_221_225_472.0, 8_589_934_592.0)
        );
        assert_eq!(
            storage_kib(10_737_418_240, 5_368_709_120, 4096),
            (21_474_836_480.0, 42_949_672_960.0)
        );
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            snapshot(root.path(), &root.path().join("absent"))["storageTotal"],
            Value::Null
        );
    }

    #[test]
    fn empty_unrelated_malformed_and_nouveau_metrics_stay_unknown() -> Result<()> {
        let root = tempfile::tempdir()?;
        let no_nvidia =
            &mut |_: &[String]| -> BTreeMap<String, Value> { panic!("No NVIDIA query expected") };
        assert_eq!(
            sensors(root.path(), no_nvidia),
            json!({"cpuTemp":null,"gpuTemp":null,"gpuPerc":null,"gpuUsageAvailable":false,"gpuFrequencyMHz":null,"gpus":[]})
        );
        put(root.path(), "class/hwmon/hwmon1/name", "nvme")?;
        put(root.path(), "class/hwmon/hwmon1/temp1_input", "35000")?;
        assert_eq!(sensors(root.path(), no_nvidia)["cpuTemp"], Value::Null);
        let device = gpu(root.path(), "card0", "0000:01:00.0", "0x10de", "nouveau")?;
        put(&device, "gpu_busy_percent", "NaN")?;
        put(&device, "hwmon/hwmon0/temp1_input", "48000")?;
        put(&device, "gt_act_freq_mhz", "-1")?;
        let result = sensors(root.path(), no_nvidia);
        assert_eq!(result["gpuTemp"], 48.0);
        assert_eq!(result["gpuUsageAvailable"], false);
        assert_eq!(result["gpuFrequencyMHz"], Value::Null);
        put(&device, "gpu_busy_percent", "0")?;
        assert_eq!(sensors(root.path(), no_nvidia)["gpuUsageAvailable"], true);
        put(&device, "gpu_busy_percent", "101")?;
        assert_eq!(sensors(root.path(), no_nvidia)["gpuUsageAvailable"], false);
        Ok(())
    }

    #[test]
    fn nvml_probe_child() {
        let Ok(mode) = std::env::var("NACRE_TEST_NVML_PROBE") else {
            return;
        };
        let samples = nvidia_samples(&["0000:01:00.0".into(), "0000:02:00.0".into()]);
        if mode != "ok" {
            assert!(samples.is_empty());
            return;
        }
        assert_eq!(
            samples["0000:01:00.0"],
            json!({"name":"NVIDIA Fixture","temperature":60.0,"usage":0.0,"frequencyMHz":210.0,"driverVersion":"580.0"})
        );
        assert_eq!(
            samples["0000:02:00.0"],
            json!({"name":"NVIDIA Other","temperature":null,"usage":0.5,"frequencyMHz":null,"driverVersion":"580.0"})
        );
    }

    #[test]
    fn direct_nvml_abi_partial_metrics_failures_and_deadline() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("nvml.c");
        fs::write(&source, include_str!("../tests/fixtures/nvml.c"))?;
        let build = std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(&source)
            .arg("-o")
            .arg(directory.path().join("libnvidia-ml.so.1"))
            .output()?;
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        for mode in ["ok", "fail", "stall"] {
            let mut command = std::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "--exact",
                    "resource_state::tests::nvml_probe_child",
                    "--nocapture",
                ])
                .env("LD_LIBRARY_PATH", directory.path())
                .env("NACRE_TEST_NVML_PROBE", mode)
                .env_remove("NACRE_TEST_NVML_FAIL")
                .env_remove("NACRE_TEST_NVML_STALL");
            if mode == "fail" {
                command.env("NACRE_TEST_NVML_FAIL", "1");
            }
            if mode == "stall" {
                command.env("NACRE_TEST_NVML_STALL", "1");
            }
            let result = command.output()?;
            assert!(
                result.status.success(),
                "NVML {mode}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        Ok(())
    }
}
