use super::*;
use crate::daemon::hub::Hub;
use nacre_protocol::Message;
use std::time::Instant;

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
