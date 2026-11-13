//! N0061: the `audio.peak` topic, the default microphone's peak level (the QML input meter).
//!
//! Capturing costs CPU and lights the desktop's recording indicator, so the PipeWire client
//! (`audio`) opens the capture stream only while a client subscribes: 8 kHz mono, one peak per
//! 66 ms window, so at most 15 updates a second. Nothing is probed at startup.
use super::hub::Shared;
use crate::Result;
use nacre_protocol::core_services::{AudioPeakState, CoreSnapshot, Health};

pub const TOPIC: &str = "audio.peak";
const RATE: usize = 8000;
/// 528 samples of two bytes: one 66 ms window.
pub(super) const WINDOW_BYTES: usize = RATE / 15 * 2;

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
pub(super) fn publish(hub: &Shared, available: bool, level: f64) -> Result<()> {
    let state = state(available, level);
    debug_assert!(CoreSnapshot::AudioPeak(state.clone()).valid());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, false, 0.0)
}

/// Peak of 16-bit little-endian samples, 0..1, in steps of 0.01.
pub(super) fn peak(window: &[u8]) -> f64 {
    let top = window
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i32::from(i16::from_le_bytes(*pair)).unsigned_abs())
        .max()
        .unwrap_or(0);
    (f64::from(top) / 32768.0 * 100.0).round().min(100.0) / 100.0
}

#[cfg(test)]
mod tests;
