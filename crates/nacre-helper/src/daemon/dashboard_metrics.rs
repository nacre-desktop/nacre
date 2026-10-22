//! `dashboard.system`: process/disk snapshot plus CPU, memory, GPU and network
//! readings. Sampling runs only while a client subscribes and stops with it.
use super::hub::{failure, Shared};
use crate::{dashboard_system, resource_state, Result};
use nacre_protocol::{
    core_services::{
        CoreSnapshot, DashboardSystemState, Entities, EntityId, Health, NetworkSample, SystemDisk,
        SystemGpu, SystemHost, SystemMemory, SystemProcess,
    },
    ErrorCode,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{task::JoinHandle, time::MissedTickBehavior};

pub const TOPIC: &str = "dashboard.system";
const TICK: Duration = Duration::from_secs(2);
const WARM: Duration = Duration::from_millis(500);
/// GPU sysfs/NVML reads are the heaviest source: every fifth tick (10 s).
const GPU_EVERY: u32 = 5;
const NETWORK_SAMPLES: usize = 32;

#[derive(Clone)]
pub struct Sources {
    pub proc: PathBuf,
    pub sys: PathBuf,
    /// Holds `os-release`.
    pub etc: PathBuf,
    pub user: Option<String>,
}
impl Sources {
    pub fn system() -> Self {
        Self {
            proc: "/proc".into(),
            sys: "/sys".into(),
            etc: "/etc".into(),
            user: nix::unistd::User::from_uid(nix::unistd::getuid())
                .ok()
                .flatten()
                .map(|user| user.name),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cpu {
    total: u64,
    idle: u64,
}
fn parse_cpu(text: &str) -> Option<Cpu> {
    let row = text
        .lines()
        .find(|line| line.split_whitespace().next() == Some("cpu"))?;
    let columns: Vec<u64> = row
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|v| v.parse().ok())
        .collect::<Option<_>>()?;
    (columns.len() >= 4).then(|| Cpu {
        total: columns.iter().sum(),
        idle: columns[3] + columns.get(4).copied().unwrap_or(0),
    })
}
fn cpu_percent(before: Cpu, after: Cpu) -> Option<f64> {
    let total = after.total.checked_sub(before.total).filter(|t| *t > 0)?;
    let idle = after
        .idle
        .checked_sub(before.idle)
        .filter(|i| *i <= total)?;
    Some((total - idle) as f64 * 100.0 / total as f64)
}
fn parse_memory(text: &str) -> Option<SystemMemory> {
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(name), Some(value), Some("kB")) = (fields.next(), fields.next(), fields.next())
        {
            if let (Some(name), Ok(value)) = (name.strip_suffix(':'), value.parse::<u64>()) {
                values.insert(name.to_owned(), value);
            }
        }
    }
    let total = *values.get("MemTotal").filter(|t| **t > 0)?;
    let available = match values.get("MemAvailable") {
        Some(value) => *value,
        None => {
            let part = |name: &str| values.get(name).copied().unwrap_or(0);
            (part("MemFree") + part("Buffers") + part("Cached") + part("SReclaimable"))
                .saturating_sub(part("Shmem"))
        }
    };
    Some(SystemMemory {
        used: (total - available.min(total)) * 1024,
        total: total * 1024,
    })
}
/// The default route's interface; the dashboard graphs that link only.
fn route_interface(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        (fields.get(1) == Some(&"00000000")).then(|| fields[0].to_owned())
    })
}
/// Summed receive and transmit byte counters, loopback excluded.
fn parse_network(text: &str, interface: Option<&str>) -> Option<(u64, u64)> {
    let (mut received, mut sent, mut seen) = (0u64, 0u64, false);
    for line in text.lines() {
        let Some((name, counters)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name == "lo" || interface.is_some_and(|wanted| wanted != name) {
            continue;
        }
        let fields: Vec<u64> = counters
            .split_whitespace()
            .map(|v| v.parse().unwrap_or(0))
            .collect();
        if fields.len() >= 16 {
            received = received.saturating_add(fields[0]);
            sent = sent.saturating_add(fields[8]);
            seen = true;
        }
    }
    seen.then_some((received, sent))
}

/// Delta state for the cheap `/proc` readings.
#[derive(Default)]
struct Metrics {
    cpu: Option<Cpu>,
    net: Option<(Instant, Option<String>, u64, u64)>,
    samples: VecDeque<NetworkSample>,
    cpu_percent: Option<f64>,
    memory: Option<SystemMemory>,
}
impl Metrics {
    fn sample(&mut self, sources: &Sources, now: Instant) {
        let read = |name: &str| fs::read_to_string(sources.proc.join(name)).ok();
        let cpu = read("stat").as_deref().and_then(parse_cpu);
        self.cpu_percent = self.cpu.zip(cpu).and_then(|(a, b)| cpu_percent(a, b));
        self.cpu = cpu;
        self.memory = read("meminfo").as_deref().and_then(parse_memory);
        let interface = read("net/route").as_deref().and_then(route_interface);
        let counters = read("net/dev")
            .as_deref()
            .and_then(|t| parse_network(t, interface.as_deref()));
        let Some((received, sent)) = counters else {
            self.net = None;
            return;
        };
        if let Some((at, previous_interface, before_received, before_sent)) = &self.net {
            let elapsed = now.saturating_duration_since(*at).as_secs_f64();
            if *previous_interface != interface {
                self.samples.clear();
            } else if elapsed > 0.0 && received >= *before_received && sent >= *before_sent {
                if self.samples.len() == NETWORK_SAMPLES {
                    self.samples.pop_front();
                }
                self.samples.push_back(NetworkSample {
                    down_bps: ((received - before_received) as f64 / elapsed).round() as u64,
                    up_bps: ((sent - before_sent) as f64 / elapsed).round() as u64,
                });
            }
        }
        self.net = Some((now, interface, received, sent));
    }
}

/// The host card's static facts, read once per subscription.
fn host_state(sources: &Sources, now_ms: u64) -> Option<SystemHost> {
    let read = |path: PathBuf| fs::read_to_string(path).ok();
    let field = |text: &str, name: &str| {
        text.lines().find_map(|line| {
            let value = line.strip_prefix(name)?.strip_prefix('=')?.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            (!value.is_empty()).then(|| value.to_owned())
        })
    };
    let clean = |text: String, max: usize| {
        let text = text.trim().to_owned();
        (!text.is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control))
            .then_some(text)
    };
    let os = read(sources.etc.join("os-release"))
        .and_then(|t| field(&t, "PRETTY_NAME").or_else(|| field(&t, "NAME")));
    let uptime = read(sources.proc.join("uptime"))
        .and_then(|t| t.split_whitespace().next()?.parse::<f64>().ok())
        .filter(|u| u.is_finite() && *u >= 0.0 && *u < 1e12);
    let host = SystemHost {
        user: sources.user.clone().and_then(|u| clean(u, 64)),
        os: os.and_then(|o| clean(o, 128)),
        kernel: read(sources.proc.join("sys/kernel/osrelease")).and_then(|k| clean(k, 128)),
        uptime_s: uptime.map(|u| u as u64),
        boot_ms: uptime.and_then(|u| now_ms.checked_sub((u * 1000.0) as u64)),
    };
    (host
        != SystemHost {
            user: None,
            os: None,
            kernel: None,
            uptime_s: None,
            boot_ms: None,
        })
    .then_some(host)
}

fn gpu_state(sys: &std::path::Path) -> Option<SystemGpu> {
    let report = resource_state::sensors(sys, &mut resource_state::nvidia_samples);
    let busiest = report["gpus"].as_array()?.iter().max_by(|a, b| {
        a["usage"]
            .as_f64()
            .unwrap_or(-1.0)
            .total_cmp(&b["usage"].as_f64().unwrap_or(-1.0))
    })?;
    let vram = |name: &str| {
        let card = busiest["card"].as_str().filter(|card| {
            card.strip_prefix("card")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })?;
        fs::read_to_string(sys.join("class/drm").join(card).join("device").join(name))
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()
    };
    let (memory_used, memory_total) = match (
        busiest["memoryUsed"].as_u64(),
        busiest["memoryTotal"].as_u64(),
    ) {
        (Some(used), Some(total)) => (Some(used), Some(total)),
        _ => (vram("mem_info_vram_used"), vram("mem_info_vram_total")),
    };
    let memory_total = memory_total.filter(|t| *t > 0);
    let memory_used = memory_used.filter(|u| memory_total.is_some_and(|t| *u <= t));
    let gpu = SystemGpu {
        percent: busiest["usage"]
            .as_f64()
            .filter(|u| (0.0..=1.0).contains(u))
            .map(|u| u * 100.0),
        memory_used: memory_total.and(memory_used),
        memory_total,
    };
    (gpu.percent.is_some() || gpu.memory_total.is_some()).then_some(gpu)
}

fn entities<T>(rows: Vec<(String, T)>) -> Entities<T> {
    let mut items = BTreeMap::new();
    let mut order = Vec::new();
    for (id, row) in rows {
        let id = EntityId(id);
        if id.valid() && !items.contains_key(&id) {
            order.push(id.clone());
            items.insert(id, row);
        }
    }
    Entities { items, order }
}
/// Convert the helper's process/disk JSON into contract entities.
fn listings(snapshot: &Value) -> Option<(Entities<SystemProcess>, Entities<SystemDisk>)> {
    let processes = snapshot["processes"]
        .as_array()?
        .iter()
        .filter_map(|p| {
            let (pid, start) = (p["pid"].as_u64()?, p["start"].as_str()?);
            Some((
                format!("process-{pid}-{start}"),
                SystemProcess {
                    pid: u32::try_from(pid).ok()?,
                    name: p["name"].as_str()?.to_owned(),
                    start: start.to_owned(),
                    uid: u32::try_from(p["uid"].as_u64()?).ok()?,
                    memory: p["memory"].as_u64()?,
                    cpu_seconds: p["cpuSeconds"]
                        .as_f64()
                        .filter(|c| c.is_finite() && *c >= 0.0)?,
                    parent: u32::try_from(p["parent"].as_u64()?).ok()?,
                },
            ))
        })
        .collect();
    let disks = snapshot["disks"]
        .as_array()?
        .iter()
        .enumerate()
        .filter_map(|(index, d)| {
            let (total, used) = (d["total"].as_u64()?, d["used"].as_u64()?);
            let path = d["path"].as_str()?.to_owned();
            (used <= total).then(|| {
                (
                    format!("disk-{}", index + 1),
                    SystemDisk { path, total, used },
                )
            })
        })
        .collect();
    Some((entities(processes), entities(disks)))
}

#[derive(Default)]
struct Latest {
    listing: Option<(Entities<SystemProcess>, Entities<SystemDisk>)>,
    listing_failed: bool,
    gpu: Option<SystemGpu>,
    host: Option<SystemHost>,
}
fn state(latest: &Latest, metrics: &Metrics) -> DashboardSystemState {
    let (processes, disks) = latest
        .listing
        .clone()
        .unwrap_or_else(|| (entities(vec![]), entities(vec![])));
    DashboardSystemState {
        health: Health {
            available: !latest.listing_failed,
            stale: latest.listing_failed && latest.listing.is_some(),
            error: latest.listing_failed.then(|| {
                failure(
                    ErrorCode::Unavailable,
                    "System details are unavailable",
                    true,
                )
            }),
        },
        processes,
        disks,
        cpu_percent: metrics.cpu_percent,
        memory: metrics.memory.clone(),
        gpu: latest.gpu.clone(),
        host: latest.host.clone(),
        network: metrics.samples.iter().cloned().collect(),
    }
}
fn publish(hub: &Shared, state: DashboardSystemState) -> Result<()> {
    if !CoreSnapshot::System(state.clone()).valid() {
        return Err("Invalid system state".into());
    }
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
/// Nothing is retained while nobody watches: the topic returns to its starting state.
fn publish_idle(hub: &Shared) -> Result<()> {
    let mut state = state(&Latest::default(), &Metrics::default());
    state.health.available = false;
    publish(hub, state)
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish_idle(hub)
}

async fn finished<T>(job: &mut Option<JoinHandle<T>>) -> Option<T> {
    match job {
        Some(handle) => {
            let result = handle.await.ok();
            *job = None;
            result
        }
        None => std::future::pending().await,
    }
}

pub async fn run(hub: Shared, sources: Sources) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    hub.borrow_mut().service("dashboard-system", "running", 0)?;
    loop {
        publish_idle(&hub)?;
        while !*interest.borrow() {
            interest.changed().await?;
        }
        let mut metrics = Metrics::default();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let mut latest = Latest {
            host: host_state(&sources, now_ms),
            ..Latest::default()
        };
        let mut listing: Option<JoinHandle<Result<Value>>> = None;
        let mut gpu: Option<JoinHandle<Option<SystemGpu>>> = None;
        let mut ticks = 0u32;
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now(), WARM);
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        while *interest.borrow() {
            tokio::select! {
                changed = interest.changed() => changed?,
                _ = timer.tick() => {
                    // Warm up quickly: a counter needs two reads before a rate.
                    if ticks == 1 {
                        timer = tokio::time::interval_at(tokio::time::Instant::now() + TICK, TICK);
                        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
                    }
                    metrics.sample(&sources, Instant::now());
                    if listing.is_none() {
                        let proc = sources.proc.clone();
                        listing = Some(tokio::task::spawn_blocking(move || dashboard_system::snapshot(&proc)));
                    }
                    if gpu.is_none() && ticks.is_multiple_of(GPU_EVERY) {
                        let sys = sources.sys.clone();
                        gpu = Some(tokio::task::spawn_blocking(move || gpu_state(&sys)));
                    }
                    ticks = ticks.wrapping_add(1);
                    publish(&hub, state(&latest, &metrics))?;
                }
                Some(result) = finished(&mut listing) => {
                    match result.ok().and_then(|v| listings(&v)) {
                        Some(rows) => { latest.listing = Some(rows); latest.listing_failed = false; }
                        None => latest.listing_failed = true,
                    }
                    publish(&hub, state(&latest, &metrics))?;
                }
                Some(result) = finished(&mut gpu) => {
                    latest.gpu = result;
                    publish(&hub, state(&latest, &metrics))?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_fixture(stat: &str, dev: &str) -> Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        fs::create_dir(dir.path().join("net"))?;
        fs::write(dir.path().join("stat"), stat)?;
        fs::write(dir.path().join("mounts"), "")?;
        fs::write(
            dir.path().join("meminfo"),
            "MemTotal:       16000 kB\nMemFree: 1000 kB\nMemAvailable:    4000 kB\n",
        )?;
        fs::write(
            dir.path().join("net/route"),
            "Iface\tDestination\tGateway\nwlan0\t00000000\t0100A8C0\neth9\t0000A8C0\t00000000\n",
        )?;
        fs::write(dir.path().join("net/dev"), dev)?;
        fs::write(
            dir.path().join("os-release"),
            "NAME=\"Fixture\"\nPRETTY_NAME='Fixture Linux 1'\n",
        )?;
        fs::write(dir.path().join("uptime"), "90061.25 1000.00\n")?;
        fs::create_dir_all(dir.path().join("sys/kernel"))?;
        fs::write(dir.path().join("sys/kernel/osrelease"), "6.12.1-fixture\n")?;
        Ok(dir)
    }
    fn dev(received: u64, sent: u64, other: u64) -> String {
        let row = |name: &str, rx: u64, tx: u64| {
            format!("{name}: {rx} 1 0 0 0 0 0 0 {tx} 1 0 0 0 0 0 0\n")
        };
        format!(
            "Inter-|   Receive\n face |bytes\n{}{}{}",
            row("lo", 999_999, 999_999),
            row("wlan0", received, sent),
            row("eth9", other, other)
        )
    }

    #[test]
    fn cpu_memory_and_network_need_two_readings_and_ignore_other_links() -> Result<()> {
        let first = proc_fixture("cpu  100 0 100 700 100 0 0 0\n", &dev(1000, 500, 5))?;
        let sources = Sources {
            proc: first.path().into(),
            sys: first.path().into(),
            etc: first.path().into(),
            user: None,
        };
        let mut metrics = Metrics::default();
        let start = Instant::now();
        metrics.sample(&sources, start);
        assert!(metrics.cpu_percent.is_none(), "one reading has no rate");
        assert!(metrics.samples.is_empty());
        let memory = metrics.memory.clone().expect("memory");
        assert_eq!((memory.used, memory.total), (12_000 * 1024, 16_000 * 1024));
        fs::write(first.path().join("stat"), "cpu  200 0 200 1400 200 0 0 0\n")?;
        fs::write(first.path().join("net/dev"), dev(3000, 1500, 9_000_000))?;
        metrics.sample(&sources, start + Duration::from_secs(2));
        // 200 busy of 1000 jiffies; idle counts idle + iowait.
        assert_eq!(metrics.cpu_percent, Some(20.0));
        assert_eq!(
            metrics.samples,
            VecDeque::from([NetworkSample {
                down_bps: 1000,
                up_bps: 500
            }])
        );
        // Counter reset (interface replaced) yields no negative or huge sample.
        fs::write(first.path().join("net/dev"), dev(10, 10, 0))?;
        metrics.sample(&sources, start + Duration::from_secs(4));
        assert_eq!(metrics.samples.len(), 1);
        Ok(())
    }

    #[test]
    fn network_history_is_bounded_and_resets_when_the_route_moves() -> Result<()> {
        let dir = proc_fixture("cpu  1 0 1 1 0 0 0 0\n", &dev(0, 0, 0))?;
        let sources = Sources {
            proc: dir.path().into(),
            sys: dir.path().into(),
            etc: dir.path().into(),
            user: Some("fixture".into()),
        };
        let mut metrics = Metrics::default();
        let start = Instant::now();
        for step in 0..40u64 {
            fs::write(dir.path().join("net/dev"), dev(step * 100, step * 10, 0))?;
            metrics.sample(&sources, start + Duration::from_secs(step));
        }
        assert_eq!(metrics.samples.len(), NETWORK_SAMPLES);
        fs::write(
            dir.path().join("net/route"),
            "Iface\tDestination\neth9\t00000000\n",
        )?;
        metrics.sample(&sources, start + Duration::from_secs(41));
        assert!(metrics.samples.is_empty());
        Ok(())
    }

    #[test]
    fn unreadable_or_malformed_sources_stay_unknown() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("stat"), "cpu garbage\n")?;
        fs::write(dir.path().join("meminfo"), "MemTotal: 0 kB\n")?;
        let sources = Sources {
            proc: dir.path().into(),
            sys: dir.path().into(),
            etc: dir.path().into(),
            user: Some("fixture".into()),
        };
        let mut metrics = Metrics::default();
        metrics.sample(&sources, Instant::now());
        assert!(metrics.cpu_percent.is_none() && metrics.memory.is_none());
        assert!(metrics.samples.is_empty());
        assert_eq!(
            parse_memory("MemTotal: 100 kB\nMemFree: 10 kB\nCached: 20 kB\nShmem: 5 kB\n")
                .map(|m| m.used),
            Some((100 - 25) * 1024),
            "MemAvailable falls back to the QML formula"
        );
        Ok(())
    }

    #[test]
    fn gpu_reports_the_busiest_device_with_independent_optional_members() -> Result<()> {
        let sys = tempfile::tempdir()?;
        let device = sys.path().join("bus/pci/devices/0000:03:00.0");
        fs::create_dir_all(&device)?;
        fs::create_dir_all(sys.path().join("class/drm/card0"))?;
        std::os::unix::fs::symlink(&device, sys.path().join("class/drm/card0/device"))?;
        fs::write(device.join("vendor"), "0x1002\n")?;
        fs::write(device.join("class"), "0x030000\n")?;
        fs::write(device.join("gpu_busy_percent"), "37\n")?;
        fs::write(device.join("mem_info_vram_used"), "1073741824\n")?;
        fs::write(device.join("mem_info_vram_total"), "8589934592\n")?;
        let gpu = gpu_state(sys.path()).expect("gpu");
        assert_eq!(gpu.percent, Some(37.0));
        assert_eq!(
            (gpu.memory_used, gpu.memory_total),
            (Some(1 << 30), Some(8 << 30))
        );
        fs::remove_file(device.join("gpu_busy_percent"))?;
        fs::write(device.join("mem_info_vram_used"), "9999999999999\n")?;
        let gpu = gpu_state(sys.path()).expect("memory only");
        assert_eq!(gpu.percent, None);
        assert_eq!(gpu.memory_used, None, "used above total is unknown");
        assert_eq!(gpu.memory_total, Some(8 << 30));
        assert!(gpu_state(tempfile::tempdir()?.path()).is_none());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sampling_runs_only_while_a_client_is_subscribed() -> Result<()> {
        use crate::daemon::hub::Hub;
        let dir = proc_fixture("cpu  100 0 100 700 100 0 0 0\n", &dev(1000, 500, 5))?;
        let sources = Sources {
            proc: dir.path().into(),
            sys: dir.path().into(),
            etc: dir.path().into(),
            user: Some("fixture".into()),
        };
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let hub = Hub::new("boot".into(), false, Duration::from_millis(16)).shared();
                initial(&hub)?;
                let mut service = tokio::task::spawn_local(run(hub.clone(), sources));
                let read = |hub: &Shared| -> DashboardSystemState {
                    serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap()).unwrap()
                };
                let wait = |hub: Shared,
                            what: &'static str,
                            check: fn(&DashboardSystemState) -> bool| async move {
                    for _ in 0..250 {
                        if check(
                            &serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap()).unwrap(),
                        ) {
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    panic!(
                        "Timed out waiting for {what}: {}",
                        hub.borrow().snapshot(TOPIC).unwrap()
                    );
                };
                tokio::time::sleep(Duration::from_millis(300)).await;
                let idle = read(&hub);
                assert!(!idle.health.available && idle.memory.is_none() && idle.network.is_empty());
                let (client, _wake) = hub.borrow_mut().add_client()?;
                hub.borrow_mut().subscribe(client, &[TOPIC.into()], false)?;
                tokio::time::sleep(Duration::from_millis(300)).await;
                if service.is_finished() {
                    (&mut service).await??;
                }
                wait(hub.clone(), "first sample", |s| {
                    s.health.available && s.memory.is_some()
                })
                .await;
                assert!(
                    read(&hub).cpu_percent.is_none(),
                    "one CPU reading is not a rate"
                );
                let host = read(&hub).host.expect("host block");
                assert_eq!(host.os.as_deref(), Some("Fixture Linux 1"));
                assert_eq!(host.uptime_s, Some(90061));
                fs::write(dir.path().join("stat"), "cpu  200 0 200 1400 200 0 0 0\n")?;
                fs::write(dir.path().join("net/dev"), dev(1_000_000, 500_000, 0))?;
                wait(hub.clone(), "warm second sample", |s| {
                    s.cpu_percent == Some(20.0) && !s.network.is_empty()
                })
                .await;
                assert!(CoreSnapshot::System(read(&hub)).valid());
                hub.borrow_mut().unsubscribe(client, &[TOPIC.into()]);
                wait(hub.clone(), "idle reset", |s| {
                    !s.health.available && s.memory.is_none() && s.host.is_none()
                })
                .await;
                let seq = hub.borrow().topic_seq(TOPIC);
                fs::write(dir.path().join("stat"), "cpu  300 0 300 2100 300 0 0 0\n")?;
                tokio::time::sleep(Duration::from_millis(2600)).await;
                assert_eq!(
                    hub.borrow().topic_seq(TOPIC),
                    seq,
                    "no sampling without a subscriber"
                );
                service.abort();
                Ok(())
            })
            .await
    }

    #[test]
    fn host_block_reads_fake_files_and_omits_what_is_unknown() -> Result<()> {
        let dir = proc_fixture("cpu  1 0 1 1 0 0 0 0\n", &dev(1, 1, 1))?;
        let sources = Sources {
            proc: dir.path().into(),
            sys: dir.path().into(),
            etc: dir.path().into(),
            user: Some("fixture".into()),
        };
        let host = host_state(&sources, 1_800_000_000_000).expect("host");
        assert_eq!(host.user.as_deref(), Some("fixture"));
        assert_eq!(host.os.as_deref(), Some("Fixture Linux 1"));
        assert_eq!(host.kernel.as_deref(), Some("6.12.1-fixture"));
        assert_eq!(host.uptime_s, Some(90061));
        assert_eq!(host.boot_ms, Some(1_800_000_000_000 - 90_061_250));
        fs::write(dir.path().join("os-release"), "NAME=Plain\n")?;
        fs::write(dir.path().join("uptime"), "bad\n")?;
        fs::write(dir.path().join("sys/kernel/osrelease"), "\n")?;
        let host = host_state(
            &Sources {
                user: None,
                ..sources.clone()
            },
            5,
        )
        .expect("os only");
        assert_eq!(host.os.as_deref(), Some("Plain"));
        assert!(host.user.is_none() && host.kernel.is_none());
        assert!(host.uptime_s.is_none() && host.boot_ms.is_none());
        fs::remove_file(dir.path().join("os-release"))?;
        assert!(host_state(
            &Sources {
                user: None,
                ..sources
            },
            5
        )
        .is_none());
        Ok(())
    }

    #[test]
    fn listings_convert_helper_rows_and_drop_invalid_ones() {
        let snapshot = serde_json::json!({
            "processes":[
                {"pid":10,"name":"worker","start":"77","uid":1000,"memory":4096,"cpuSeconds":1.5,"parent":1},
                {"pid":11,"name":"bad","start":"78","uid":1000,"memory":1,"cpuSeconds":-1.0,"parent":1}],
            "disks":[{"path":"/","total":100,"used":40},{"path":"/bad","total":1,"used":2}]});
        let (processes, disks) = listings(&snapshot).expect("rows");
        assert_eq!(processes.order, [EntityId("process-10-77".into())]);
        assert_eq!(disks.order, [EntityId("disk-1".into())]);
        assert!(processes.valid() && disks.valid());
    }
}
