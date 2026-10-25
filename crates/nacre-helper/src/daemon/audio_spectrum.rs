//! N0072: a lease-only default-output monitor. Reuses OSD's audio backend for
//! sink-change signals. No capture, FFT plan, polling or audio connection at READY.
use super::hub::{failure, Notice, Shared};
use crate::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use nacre_protocol::{
    core_services::{AudioSpectrumState, Health, SpectrumBands, SpectrumLease},
    Capability, ErrorCode, Failure, Method,
};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use serde_json::{json, Value};
use std::{collections::BTreeMap, ffi::OsString, process::Stdio, sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub const TOPIC: &str = "audio.spectrum";
pub(super) const CLIENT: &str = "nacre-spectrum";
const RATE: usize = 48000;
const HOP: usize = RATE / 60;
const SIZE: usize = 4096;
const SILENCE: f32 = 0.0001;

#[derive(Clone)]
pub struct Env {
    pub parec: OsString,
    pub retry: Duration,
    pub silence_after: Duration,
}
impl Env {
    pub fn production() -> Self {
        Self {
            parec: "parec".into(),
            retry: Duration::from_secs(2),
            silence_after: Duration::from_secs(3),
        }
    }
}

pub fn methods() -> BTreeMap<String, Method> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../protocol/schema/core-service-call.json"
    ))
    .expect("core call schema");
    let variant = schema["oneOf"]
        .as_array()
        .expect("call variants")
        .iter()
        .find(|v| v["properties"]["m"]["const"] == "audio.spectrum.configure")
        .expect("spectrum call");
    BTreeMap::from([(
        "audio.spectrum.configure".into(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 1000,
            params: json!({"$schema":schema["$schema"],"$defs":schema["$defs"],"$ref":variant["properties"]["p"]["$ref"]}),
            result: json!({"type":"object","properties":{"changed":{"type":"boolean"}},"required":["changed"]}),
        },
    )])
}
pub fn configure(hub: &Shared, client: u64, params: Value) -> std::result::Result<Value, Failure> {
    let lease: SpectrumLease = serde_json::from_value(params)
        .map_err(|_| failure(ErrorCode::InvalidRequest, "Invalid spectrum lease", false))?;
    if !(1..=60).contains(&lease.rate_hz) {
        return Err(failure(
            ErrorCode::InvalidRequest,
            "Spectrum rate must be 1..60 Hz",
            false,
        ));
    }
    hub.borrow_mut().configure_spectrum(client, lease)?;
    Ok(json!({"changed":true}))
}

/// Project one shared 64-band analysis into each client's independent lease.
pub fn project(data: &Value, bands: SpectrumBands) -> Value {
    let mut data = data.clone();
    if let Some(raw) = data["bands"].as_str().and_then(|s| STANDARD.decode(s).ok()) {
        let count = bands.count();
        if raw.len() == 64 {
            let reduced: Vec<u8> = raw
                .chunks_exact(64 / count)
                .map(|chunk| *chunk.iter().max().unwrap_or(&0))
                .collect();
            data["bands"] = Value::String(STANDARD.encode(reduced));
        }
    }
    data
}
fn empty(available: bool) -> AudioSpectrumState {
    AudioSpectrumState {
        health: Health {
            available,
            stale: false,
            error: None,
        },
        bands: STANDARD.encode([0; 64]),
        level: 0.0,
        peak: 0.0,
        onset: 0,
        beat: false,
        paused: true,
    }
}
fn publish(hub: &Shared, state: &AudioSpectrumState) -> Result<()> {
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, &empty(false))
}

struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    buffer: Vec<Complex<f32>>,
    samples: Vec<f32>,
    window: Vec<f32>,
    bins: [(usize, usize); 64],
    smooth: [f32; 64],
    quiet: Duration,
    floor: f32,
    since_beat: Duration,
    state: AudioSpectrumState,
}
impl Analyzer {
    fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(SIZE);
        let bins = std::array::from_fn(|i| {
            let edge = |i: usize| {
                ((20.0 * 1000.0_f32.powf(i as f32 / 64.0)) * SIZE as f32 / RATE as f32).round()
                    as usize
            };
            let low = edge(i).max(1);
            (low, edge(i + 1).max(low + 1).min(SIZE / 2))
        });
        Self {
            scratch: vec![Complex::default(); fft.get_inplace_scratch_len()],
            fft,
            buffer: vec![Complex::default(); SIZE],
            samples: vec![0.0; SIZE],
            window: (0..SIZE)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / SIZE as f32).cos())
                .collect(),
            bins,
            smooth: [0.0; 64],
            quiet: Duration::ZERO,
            floor: 0.0,
            since_beat: Duration::from_secs(1),
            state: empty(false),
        }
    }
    fn ingest(&mut self, bytes: &[u8], silence_after: Duration) {
        self.state.health.available = true;
        self.samples.copy_within(HOP.., 0);
        let mut energy = 0.0;
        let mut peak = 0.0_f32;
        for (out, pair) in self.samples[SIZE - HOP..]
            .iter_mut()
            .zip(bytes.as_chunks::<2>().0)
        {
            *out = f32::from(i16::from_le_bytes(*pair)) / 32768.0;
            energy += *out * *out;
            peak = peak.max(out.abs());
        }
        let rms = (energy / HOP as f32).sqrt();
        let dt = Duration::from_secs_f64(HOP as f64 / RATE as f64);
        self.quiet = if rms < SILENCE {
            self.quiet + dt
        } else {
            Duration::ZERO
        };
        self.since_beat += dt;
        self.state.paused = self.quiet >= silence_after;
        self.state.level = (f64::from(rms) * 1000.0).round() / 1000.0;
        self.state.peak = (f64::from(peak) * 1000.0).round() / 1000.0;
        self.state.beat = !self.state.paused
            && rms > 0.01
            && rms > self.floor * 1.6
            && self.since_beat >= Duration::from_millis(180);
        if self.state.beat {
            self.state.onset = self
                .state
                .onset
                .saturating_add(1)
                .min(nacre_protocol::MAX_SAFE_INTEGER);
            self.since_beat = Duration::ZERO;
            // Do not count a sustained first note again while a baseline that
            // began at silence catches up with it.
            self.floor = rms;
        }
        self.floor += (rms - self.floor) * 0.05;
    }
    fn analyze(&mut self, period: Duration) -> &AudioSpectrumState {
        if self.state.paused {
            self.smooth.fill(0.0);
            self.state.bands = STANDARD.encode([0; 64]);
            return &self.state;
        }
        for ((out, sample), window) in self.buffer.iter_mut().zip(&self.samples).zip(&self.window) {
            *out = Complex::new(sample * window, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        let mut bytes = [0; 64];
        for (i, &(lo, hi)) in self.bins.iter().enumerate() {
            let amplitude = self.buffer[lo..hi]
                .iter()
                .map(|c| c.norm_sqr())
                .fold(0.0_f32, f32::max)
                .sqrt()
                * (4.0 / SIZE as f32);
            // -60 dBFS..0 dBFS is the display range; not loudness normalization.
            let value = ((20.0 * amplitude.max(0.000001).log10() + 60.0) / 60.0).clamp(0.0, 1.0);
            let tau = if value > self.smooth[i] { 0.025 } else { 0.2 };
            self.smooth[i] +=
                (value - self.smooth[i]) * (1.0 - (-period.as_secs_f32() / tau).exp());
            bytes[i] = (self.smooth[i] * 255.0).round() as u8;
        }
        self.state.bands = STANDARD.encode(bytes);
        &self.state
    }
}

fn sink(hub: &Shared) -> Option<String> {
    hub.borrow()
        .snapshot_ref("audio")
        .and_then(|s| s["default_output"].as_str())
        .map(str::to_owned)
}
async fn capture(hub: &Shared, env: &Env, onset: &mut u64) -> Result<()> {
    let mut changes = hub.borrow().observe();
    let mut current_sink = sink(hub);
    let mut child = Command::new(&env.parec)
        .args([
            "--device=@DEFAULT_MONITOR@",
            "--raw",
            "--format=s16le",
            "--rate=48000",
            "--channels=1",
            "--latency-msec=10",
            "--process-time-msec=10",
            &format!("--client-name={CLIENT}"),
            &format!("--stream-name={CLIENT}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child.stdout.take().ok_or("capture stdout missing")?;
    let mut analyzer = Analyzer::new();
    analyzer.state.onset = *onset;
    let mut bytes = [0; HOP * 2];
    let mut filled = 0;
    let mut elapsed = Duration::ZERO;
    let mut idle_at = tokio::time::Instant::now() + env.silence_after;
    let mut stalled = false;
    loop {
        tokio::select! {
            result = stdout.read(&mut bytes[filled..]) => {
                let count = result?;
                if count == 0 { return Err("Spectrum capture ended".into()); }
                filled += count;
                if filled != bytes.len() { continue; }
                filled = 0;
                idle_at = tokio::time::Instant::now() + env.silence_after;
                stalled = false;
                analyzer.ingest(&bytes, env.silence_after);
                *onset = analyzer.state.onset;
                elapsed += Duration::from_secs_f64(1.0 / 60.0);
                let period = hub.borrow().spectrum_period();
                if elapsed >= period {
                    publish(hub, analyzer.analyze(elapsed))?;
                    elapsed = Duration::ZERO;
                }
            }
            _ = tokio::time::sleep_until(idle_at), if !stalled => {
                // Suspended monitors may stop sending PCM entirely. Clear the
                // display once, then wait without a timer until audio resumes.
                stalled = true;
                analyzer.samples.fill(0.0);
                analyzer.quiet = env.silence_after;
                analyzer.floor = 0.0;
                analyzer.state.level = 0.0;
                analyzer.state.peak = 0.0;
                analyzer.state.beat = false;
                analyzer.state.paused = true;
                publish(hub, analyzer.analyze(Duration::ZERO))?;
            }
            notice = changes.recv() => {
                if matches!(notice, Ok(Notice::Changed { ref topic, .. }) if topic == "audio") || notice.is_err() {
                    let next = sink(hub);
                    if next != current_sink {
                        // A stream binds its initial default. Reopen after a backend sink change.
                        return Ok(());
                    }
                    current_sink = next;
                }
            }
        }
    }
}
async fn keep_capturing(hub: &Shared, env: &Env, onset: &mut u64) -> Result<()> {
    loop {
        match capture(hub, env, onset).await {
            Ok(()) => {} // A sink switch restarts immediately.
            Err(error) => {
                tracing::debug!(%error, "Spectrum capture unavailable");
                let mut state = empty(false);
                state.onset = *onset;
                publish(hub, &state)?;
                tokio::time::sleep(env.retry).await;
            }
        }
    }
}
pub async fn run(hub: Shared, env: Env) -> Result<()> {
    let mut onset = hub
        .borrow()
        .snapshot_ref(TOPIC)
        .and_then(|s| s["onset"].as_u64())
        .unwrap_or(0);
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    hub.borrow_mut().service("audio-spectrum", "running", 0)?;
    loop {
        while !*interest.borrow() {
            interest.changed().await?;
        }
        let unwanted = async {
            while *interest.borrow() {
                interest.changed().await?;
            }
            Result::Ok(())
        };
        tokio::select! { result = keep_capturing(&hub, &env, &mut onset) => result?, result = unwanted => result? }
        let mut state = empty(false);
        state.onset = onset;
        publish(&hub, &state)?;
    }
}

#[cfg(test)]
mod tests;
