use super::*;
use crate::daemon::hub::Hub;
use nacre_protocol::Message;
use std::{os::unix::fs::PermissionsExt, time::Instant};

fn pcm(freq: f32, frame: usize, amplitude: f32) -> Vec<u8> {
    (0..HOP)
        .flat_map(|i| {
            let phase = (frame * HOP + i) as f32 * freq * std::f32::consts::TAU / RATE as f32;
            ((phase.sin() * amplitude * 32767.0) as i16).to_le_bytes()
        })
        .collect()
}
#[test]
fn sweep_rms_peak_and_onsets() {
    for freq in [60.0_f32, 250.0, 1000.0, 4000.0, 12000.0] {
        let mut analyzer = Analyzer::new();
        for frame in 0..60 {
            analyzer.ingest(&pcm(freq, frame, 0.5), Duration::from_secs(3));
            analyzer.analyze(Duration::from_secs_f64(1.0 / 60.0));
        }
        let bytes = STANDARD.decode(&analyzer.state.bands).unwrap();
        let top = bytes.iter().enumerate().max_by_key(|(_, b)| **b).unwrap().0;
        let expected = ((freq / 20.0).ln() / 1000.0_f32.ln() * 64.0).floor() as usize;
        assert!(top.abs_diff(expected) <= 2, "{freq}: {top} vs {expected}");
        assert!((analyzer.state.level - 0.3535).abs() < 0.04);
        assert!((analyzer.state.peak - 0.5).abs() < 0.01);
        assert_eq!(analyzer.state.onset, 1, "steady tone should not retrigger");
    }
    let mut analyzer = Analyzer::new();
    let mut beats = 0;
    for frame in 0..180 {
        analyzer.ingest(
            &pcm(80.0, frame, if frame % 30 < 3 { 0.8 } else { 0.0 }),
            Duration::from_secs(3),
        );
        beats += usize::from(analyzer.state.beat);
    }
    assert_eq!(beats, 6);
}
#[test]
fn silence_pauses_and_sound_resumes() {
    let mut analyzer = Analyzer::new();
    assert!(!analyzer.state.health.available, "no PCM has arrived yet");
    for frame in 0..240 {
        analyzer.ingest(&pcm(1000.0, frame, 0.0), Duration::from_secs(3));
    }
    analyzer.buffer.fill(Complex::new(99.0, 99.0));
    assert!(analyzer.analyze(Duration::from_millis(17)).paused);
    assert!(analyzer.state.health.available);
    assert_eq!(
        analyzer.buffer[0],
        Complex::new(99.0, 99.0),
        "paused analysis must skip the FFT"
    );
    assert!(STANDARD
        .decode(&analyzer.state.bands)
        .unwrap()
        .iter()
        .all(|b| *b == 0));
    analyzer.ingest(&pcm(1000.0, 241, 0.5), Duration::from_secs(3));
    assert!(!analyzer.analyze(Duration::from_millis(17)).paused);
    assert!(analyzer.state.beat);
}
#[test]
fn independent_leases_are_compact_and_rate_limited() {
    let hub = Hub::new("fake".into(), false, Duration::from_millis(20)).shared();
    initial(&hub).unwrap();
    let (a, _) = hub.borrow_mut().add_client().unwrap();
    let (b, _) = hub.borrow_mut().add_client().unwrap();
    assert!(configure(&hub, a, json!({})).is_err());
    for id in [a, b] {
        hub.borrow_mut()
            .subscribe(id, &[TOPIC.into()], false)
            .unwrap();
    }
    configure(&hub, a, json!({"bands":"16","rate_hz":10})).unwrap();
    configure(&hub, b, json!({"bands":"64","rate_hz":60})).unwrap();
    assert!(configure(&hub, a, json!({"rate_hz":0})).is_err());
    assert!(configure(&hub, a, json!({"rate_hz":61})).is_err());
    assert!(configure(&hub, a, json!({"bands":"17"})).is_err());
    assert_eq!(hub.borrow().spectrum_period(), Duration::from_millis(20));
    let now = Instant::now();
    for (id, len) in [(a, 16), (b, 64)] {
        let packet = hub.borrow_mut().next(id, now).0.unwrap();
        let Message::Snap { data, .. } = &packet.message else {
            panic!("snapshot expected")
        };
        assert_eq!(
            STANDARD
                .decode(data["bands"].as_str().unwrap())
                .unwrap()
                .len(),
            len
        );
        assert!(
            nacre_protocol::framing::encode(&packet.message)
                .unwrap()
                .len()
                < 400
        );
        hub.borrow_mut().sent(id, &packet, now);
    }
    let mut state = empty(true);
    state.level = 0.5;
    publish(&hub, &state).unwrap();
    assert!(hub
        .borrow_mut()
        .next(a, now + Duration::from_millis(30))
        .0
        .is_none());
    assert!(hub
        .borrow_mut()
        .next(b, now + Duration::from_millis(30))
        .0
        .is_some());
    hub.borrow_mut().remove_client(b);
    assert_eq!(hub.borrow().spectrum_period(), Duration::from_millis(100));
    assert!(hub
        .borrow_mut()
        .next(a, now + Duration::from_millis(100))
        .0
        .is_some());
    configure(&hub, a, json!({"bands":"32","rate_hz":30})).unwrap();
    let packet = hub
        .borrow_mut()
        .next(a, now + Duration::from_millis(101))
        .0
        .unwrap();
    let Message::Snap { data, .. } = packet.message else {
        panic!("reconfigure must reset baseline")
    };
    assert_eq!(
        STANDARD
            .decode(data["bands"].as_str().unwrap())
            .unwrap()
            .len(),
        32
    );
    assert_eq!(
        hub.borrow().spectrum_period(),
        Duration::from_secs_f64(1.0 / 30.0)
    );
}
async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition timed out");
}
#[tokio::test(flavor = "current_thread")]
async fn fake_capture_lifecycle_and_sink_switch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parec");
    // exec replaces the shell: the only process started is the recorded Python PID.
    std::fs::write(
        &path,
        format!(
            r#"#!/usr/bin/python
import os,sys,time
with open({log:?}, 'a') as f: f.write(str(os.getpid())+' '+' '.join(sys.argv[1:])+'\n')
while True:
    os.write(1, bytes(1600))
    time.sleep(1/60)
"#,
            log = dir.path().join("starts").to_str().unwrap()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hub = Hub::new("fake".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    let starts = || std::fs::read_to_string(dir.path().join("starts")).unwrap_or_default();
    tokio::task::LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(run(
                hub.clone(),
                Env {
                    parec: path.into(),
                    retry: Duration::from_millis(10),
                    silence_after: Duration::from_millis(50),
                },
            ));
            tokio::time::sleep(Duration::from_millis(80)).await;
            assert!(starts().is_empty());
            let (a, _) = hub.borrow_mut().add_client().unwrap();
            let (b, _) = hub.borrow_mut().add_client().unwrap();
            for id in [a, b] {
                hub.borrow_mut()
                    .subscribe(id, &[TOPIC.into()], false)
                    .unwrap();
            }
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["health"]["available"] == true).await;
            assert_eq!(starts().lines().count(), 1);
            assert!(starts().contains("--device=@DEFAULT_MONITOR@"));
            assert!(starts().contains("--latency-msec=10"));
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["paused"] == true).await;
            hub.borrow_mut()
                .publish("audio", json!({"default_output":"fake-sink-b"}))
                .unwrap();
            wait_for(|| starts().lines().count() == 2).await;
            hub.borrow_mut().remove_client(a);
            tokio::time::sleep(Duration::from_millis(80)).await;
            assert_eq!(starts().lines().count(), 2);
            hub.borrow_mut().remove_client(b);
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["health"]["available"] == false)
                .await;
            for line in starts().lines() {
                let pid = line.split_whitespace().next().unwrap();
                wait_for(|| !std::path::Path::new(&format!("/proc/{pid}")).exists()).await;
            }
            task.abort();
            let _ = task.await;
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn failed_capture_keeps_onset_and_retries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parec");
    std::fs::write(
        &path,
        r#"#!/usr/bin/python
import math,os,struct,time
os.write(1,b''.join(struct.pack('<h',int(16000*math.sin(i*math.tau/48))) for i in range(800)))
time.sleep(0.1)
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hub = Hub::new("fake".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    let (id, _) = hub.borrow_mut().add_client().unwrap();
    hub.borrow_mut()
        .subscribe(id, &[TOPIC.into()], false)
        .unwrap();
    tokio::task::LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(run(
                hub.clone(),
                Env {
                    parec: path.into(),
                    retry: Duration::from_millis(80),
                    silence_after: Duration::from_secs(3),
                },
            ));
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["onset"] == 1).await;
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["health"]["available"] == false)
                .await;
            assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["onset"], 1);
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["onset"] == 2).await;
            hub.borrow_mut().remove_client(id);
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["health"]["available"] == false)
                .await;
            assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["onset"], 2);
            task.abort();
            let _ = task.await;
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_suspended_pcm_stream_clears_the_display_and_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parec");
    std::fs::write(
        &path,
        r#"#!/usr/bin/python
import os,time
os.write(1,b'\x00\x40'*800)
time.sleep(0.2)
os.write(1,b'\x00\x40'*800)
time.sleep(1)
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hub = Hub::new("fake".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    let (id, _) = hub.borrow_mut().add_client().unwrap();
    hub.borrow_mut()
        .subscribe(id, &[TOPIC.into()], false)
        .unwrap();
    tokio::task::LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(run(
                hub.clone(),
                Env {
                    parec: path.into(),
                    retry: Duration::from_millis(20),
                    silence_after: Duration::from_millis(50),
                },
            ));
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["level"] == 0.5).await;
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["paused"] == true).await;
            let state = hub.borrow().snapshot(TOPIC).unwrap();
            assert_eq!(state["level"], 0.0);
            assert_eq!(state["peak"], 0.0);
            assert!(STANDARD
                .decode(state["bands"].as_str().unwrap())
                .unwrap()
                .iter()
                .all(|b| *b == 0));
            wait_for(|| hub.borrow().snapshot(TOPIC).unwrap()["level"] == 0.5).await;
            assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["paused"], false);
            hub.borrow_mut().remove_client(id);
            task.abort();
            let _ = task.await;
        })
        .await;
}

/// Run in an isolated release build; includes ingest, FFT, JSON, per-lease
/// projection, hub diff and framing. No audio server or real device is opened.
#[test]
#[ignore = "explicit release CPU budget measurement"]
fn cpu_budget() {
    let hub = Hub::new("benchmark".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    let (id, _) = hub.borrow_mut().add_client().unwrap();
    hub.borrow_mut()
        .subscribe(id, &[TOPIC.into()], false)
        .unwrap();
    let mut analyzer = Analyzer::new();
    let frames: Vec<_> = (0..60)
        .map(|i| pcm(80.0 + i as f32 * 100.0, i, 0.2 + i as f32 / 100.0))
        .collect();
    let period = Duration::from_secs_f64(1.0 / 60.0);
    let now = Instant::now();
    // /proc process ticks avoid counting the deliberate virtual 60-Hz pacing.
    let ticks = || {
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        let fields: Vec<_> = stat
            .rsplit_once(") ")
            .unwrap()
            .1
            .split_whitespace()
            .collect();
        fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
    };
    let before = ticks();
    for frame in 0..3600 {
        analyzer.ingest(&frames[frame % 60], Duration::from_secs(3));
        publish(&hub, analyzer.analyze(period)).unwrap();
        let at = now + period * (frame as u32 + 1);
        let packet = hub.borrow_mut().next(id, at).0;
        if let Some(packet) = packet {
            std::hint::black_box(nacre_protocol::framing::encode(&packet.message).unwrap());
            // Drop the RefMut from next() before committing the packet.
            hub.borrow_mut().sent(id, &packet, at);
        }
    }
    let hz = nix::unistd::sysconf(nix::unistd::SysconfVar::CLK_TCK)
        .unwrap()
        .unwrap() as f64;
    let cpu = (ticks() - before) as f64 / hz;
    println!(
        "N0072: 3600 frames / 60 simulated seconds, CPU={cpu:.4}s, one-core={:.4}%",
        cpu / 60.0 * 100.0
    );
    assert!(cpu <= 1.2, "60-Hz spectrum exceeds 2% of one core");
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "explicit release real-time fake-capture CPU measurement"]
async fn service_cpu_budget() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parec");
    std::fs::write(&path,format!(r#"#!/usr/bin/python
import math,os,struct,time
with open({pid:?},'w') as f: f.write(str(os.getpid()))
frames=[b''.join(struct.pack('<h',int(16000*math.sin(i*math.tau*(80+j*100)/48000))) for i in range(800)) for j in range(60)]
deadline=time.monotonic()
frame=0
while True:
    with open({mode:?}) as f: silent=f.read()=='silent'
    os.write(1,bytes(1600) if silent else frames[frame%60])
    frame+=1
    deadline+=1/60
    time.sleep(max(0,deadline-time.monotonic()))
"#,pid=dir.path().join("pid").to_str().unwrap(),mode=dir.path().join("mode").to_str().unwrap())).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.path().join("mode"), "music").unwrap();
    let ticks = |pid: &str| {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let fields: Vec<_> = stat
            .rsplit_once(") ")
            .unwrap()
            .1
            .split_whitespace()
            .collect();
        fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
    };
    let hz = nix::unistd::sysconf(nix::unistd::SysconfVar::CLK_TCK)
        .unwrap()
        .unwrap() as f64;
    let hub = Hub::new("benchmark".into(), false, Duration::from_millis(1)).shared();
    initial(&hub).unwrap();
    tokio::task::LocalSet::new().run_until(async {
        let service = tokio::task::spawn_local(run(hub.clone(),Env {parec:path.into(),..Env::production()}));
        tokio::task::yield_now().await;
        let before = ticks("self");
        tokio::time::sleep(Duration::from_secs(2)).await;
        let unleased = ticks("self")-before;
        assert!(!dir.path().join("pid").exists());
        println!("N0072 real-time unleased: {unleased} CPU ticks / 2s (tick rate {hz})");
        let (id,wake) = hub.borrow_mut().add_client().unwrap();
        hub.borrow_mut().subscribe(id,&[TOPIC.into()],false).unwrap();
        let writer_hub = hub.clone();
        let writer = tokio::task::spawn_local(async move {
            loop {
                let now = Instant::now();
                let (packet,deadline) = writer_hub.borrow_mut().next(id,now);
                if let Some(packet) = packet {
                    std::hint::black_box(nacre_protocol::framing::encode(&packet.message).unwrap());
                    writer_hub.borrow_mut().sent(id,&packet,now);
                } else if let Some(at) = deadline {
                    tokio::select! { _ = wake.notified() => {}, _ = tokio::time::sleep_until(at.into()) => {} }
                } else { wake.notified().await; }
            }
        });
        wait_for(|| dir.path().join("pid").exists()).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let pid = std::fs::read_to_string(dir.path().join("pid")).unwrap();
        for mode in ["music","silent"] {
            std::fs::write(dir.path().join("mode"),mode).unwrap();
            if mode == "silent" { tokio::time::sleep(Duration::from_secs(4)).await; }
            let parent = ticks("self");
            let child = ticks(&pid);
            let sequence = hub.borrow().topic_seq(TOPIC).unwrap();
            let started = Instant::now();
            tokio::time::sleep(Duration::from_secs(10)).await;
            let elapsed = started.elapsed().as_secs_f64();
            let cpu = (ticks("self")-parent) as f64/hz/elapsed*100.0;
            let capture = (ticks(&pid)-child) as f64/hz/elapsed*100.0;
            let updates = hub.borrow().topic_seq(TOPIC).unwrap()-sequence;
            println!("N0072 real-time {mode}: daemon={cpu:.3}% fake-capture={capture:.3}% one core, {elapsed:.3}s, {updates} topic updates, 60Hz/32 bands");
            assert!(cpu <= 2.0,"daemon CPU budget exceeded");
            if mode == "music" { assert!(updates >= 550,"capture did not sustain the requested rate"); }
            if mode == "silent" { assert!(hub.borrow().snapshot(TOPIC).unwrap()["paused"] == true); }
        }
        hub.borrow_mut().remove_client(id);
        writer.abort(); let _ = writer.await;
        service.abort(); let _ = service.await;
        wait_for(|| !std::path::Path::new(&format!("/proc/{pid}")).exists()).await;
    }).await;
}
