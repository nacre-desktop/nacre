//! N0061: the `audio.peak` topic, the default microphone's peak level (the QML input meter).
//!
//! Capturing costs CPU and lights the desktop's recording indicator, so the monitor runs only
//! while a client subscribes. It is one `parec` child at a low rate (8 kHz mono, one peak per
//! 66 ms window, so at most 15 updates a second), killed when the last lease ends. Nothing is
//! probed at startup; a missing or failing `parec` publishes the topic as unavailable.
use super::{hub::Shared, osd::PEAK_CLIENT};
use crate::Result;
use nacre_protocol::core_services::{AudioPeakState, CoreSnapshot, Health};
use std::{ffi::OsString, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub const TOPIC: &str = "audio.peak";
const RATE: usize = 8000;
/// 528 samples of two bytes: one 66 ms window.
const WINDOW_BYTES: usize = RATE / 15 * 2;

#[derive(Clone)]
pub struct Env {
    pub parec: OsString,
    /// Wait before restarting a failed capture.
    pub retry: Duration,
}
impl Env {
    pub fn production() -> Self {
        Self {
            parec: "parec".into(),
            retry: Duration::from_secs(2),
        }
    }
}

fn state(available: bool, level: f64) -> AudioPeakState {
    AudioPeakState {
        health: Health {
            available,
            stale: false,
            error: None,
        },
        level,
    }
}
fn publish(hub: &Shared, state: AudioPeakState) -> Result<()> {
    debug_assert!(CoreSnapshot::AudioPeak(state.clone()).valid());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, state(false, 0.0))
}

/// Peak of 16-bit little-endian samples, 0..1, in steps of 0.01.
fn peak(window: &[u8]) -> f64 {
    let top = window
        .chunks_exact(2)
        .map(|pair| i32::from(i16::from_le_bytes([pair[0], pair[1]])).unsigned_abs())
        .max()
        .unwrap_or(0);
    (f64::from(top) / 32768.0 * 100.0).round().min(100.0) / 100.0
}

/// One capture until it ends; the topic is published only when the level changes.
async fn capture(hub: &Shared, env: &Env) -> Result<()> {
    let Ok(mut child) = Command::new(&env.parec)
        .args([
            "--device=@DEFAULT_SOURCE@",
            "--raw",
            "--format=s16le",
            "--rate=8000",
            "--channels=1",
            "--latency-msec=66",
            &format!("--client-name={PEAK_CLIENT}"),
            &format!("--stream-name={PEAK_CLIENT}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    else {
        return Ok(());
    };
    let Some(mut stdout) = child.stdout.take() else {
        return Ok(());
    };
    let mut window = [0u8; WINDOW_BYTES];
    let mut last = None::<f64>;
    while stdout.read_exact(&mut window).await.is_ok() {
        let level = peak(&window);
        if last != Some(level) {
            last = Some(level);
            publish(hub, state(true, level))?;
        }
    }
    Ok(())
}

/// Capture, restarting after a failure, until dropped.
async fn keep_capturing(hub: &Shared, env: &Env) -> Result<()> {
    loop {
        capture(hub, env).await?;
        publish(hub, state(false, 0.0))?;
        tokio::time::sleep(env.retry).await;
    }
}

/// Runs for the daemon's life; captures only while the topic has a subscriber.
pub async fn run(hub: Shared, env: Env) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    hub.borrow_mut().service("audio-peak", "running", 0)?;
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
        tokio::select! {
            result = keep_capturing(&hub, &env) => result?,
            result = unwanted => result?,
        }
        publish(&hub, state(false, 0.0))?;
    }
}

#[cfg(test)]
mod tests;
