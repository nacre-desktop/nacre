//! Block on X11/Niri events and shutdown; the existing opening timer is one-shot.
use super::{x11, Policy, MAX_BYTES};
use crate::{
    compositor::{backend, niri_transport},
    Result,
};
use nix::sys::signal::{SigSet, Signal};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

pub fn check_session(environment: &BTreeMap<String, String>) -> Result<bool> {
    Ok(backend::select(environment)? == "niri"
        && environment
            .get("NIRI_SOCKET")
            .is_some_and(|value| !value.is_empty())
        && environment
            .get("DISPLAY")
            .is_some_and(|value| !value.is_empty()))
}
struct SignalMask(SigSet);
impl Drop for SignalMask {
    fn drop(&mut self) {
        if let Err(error) = self.0.thread_set_mask() {
            eprintln!("nacre-helper: width observer signal mask restoration failed: {error}");
        }
    }
}
fn shutdown_socket() -> Result<(UnixStream, SignalMask)> {
    let previous = SigSet::thread_get_mask()?;
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGTERM);
    mask.add(Signal::SIGINT);
    mask.thread_block()?;
    let guard = SignalMask(previous);
    let (read, mut write) = UnixStream::pair()?;
    std::thread::spawn(move || {
        if let Err(error) = mask.wait() {
            eprintln!("nacre-helper: width observer signal wait failed: {error}");
        }
        if let Err(error) = write.write_all(&[0]) {
            eprintln!("nacre-helper: width observer shutdown wake failed: {error}");
        }
    });
    Ok((read, guard))
}
pub fn observe(path: &Path, opening_width: f64) -> Result<()> {
    let mut x11 = x11::Native::connect()?;
    let mut events = UnixStream::connect(path)?;
    events.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request =
        crate::json::encode(&serde_json::to_value(niri_ipc::Request::EventStream)?)? + "\n";
    events.write_all(request.as_bytes())?;
    events.set_nonblocking(true)?;
    let (shutdown, _signal_mask) = shutdown_socket()?;
    let started = Instant::now();
    let mut policy = Policy::default();
    let mut actions = niri_transport::Connection::new(path);
    let mut buffer = Vec::new();
    let result = (|| -> Result<()> {
        policy.reconcile(&mut x11, started.elapsed().as_secs_f64())?;
        loop {
            let now = started.elapsed().as_secs_f64();
            policy.apply_openings(now, |id| {
                let request = niri_ipc::Request::Action(niri_ipc::Action::SetWindowWidth {
                    id: Some(id),
                    change: niri_ipc::SizeChange::SetProportion(opening_width),
                });
                if let Err(error) = actions.request(&request, Duration::from_secs(2), MAX_BYTES) {
                    eprintln!("nacre-helper: opening-width action failed: {error}");
                }
                Ok(())
            })?;
            if x11.drain()? {
                policy.reconcile(&mut x11, started.elapsed().as_secs_f64())?;
                continue;
            }
            let timeout = policy
                .opening
                .values()
                .copied()
                .reduce(f64::min)
                .map(|deadline| Duration::try_from_secs_f64((deadline - now).max(0.0)))
                .transpose()?;
            let timeout = timeout.map(|value| rustix::event::Timespec {
                tv_sec: value.as_secs() as i64,
                tv_nsec: i64::from(value.subsec_nanos()),
            });
            let mut descriptors = [
                rustix::event::PollFd::new(&events, rustix::event::PollFlags::IN),
                rustix::event::PollFd::new(x11.connection.stream(), rustix::event::PollFlags::IN),
                rustix::event::PollFd::new(&shutdown, rustix::event::PollFlags::IN),
            ];
            match rustix::event::poll(&mut descriptors, timeout.as_ref()) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            let ready = [
                descriptors[0].revents(),
                descriptors[1].revents(),
                descriptors[2].revents(),
            ];
            if !ready[2].is_empty() {
                return Ok(());
            }
            let mut changed = false;
            if !ready[1].is_empty() {
                changed = x11.drain()?;
            }
            if !ready[0].is_empty() {
                let mut chunk = [0u8; 65536];
                let count = match events.read(&mut chunk) {
                    Ok(0) => return Err("Niri width event stream disconnected".into()),
                    Ok(count) => count,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => 0,
                    Err(error) => return Err(error.into()),
                };
                buffer.extend_from_slice(&chunk[..count]);
                if buffer.len() > MAX_BYTES {
                    return Err("Oversized Niri event stream message".into());
                }
                while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
                    let event: Value = serde_json::from_slice(&buffer[..end])?;
                    buffer.drain(..=end);
                    if event.get("Err").is_some() {
                        return Err("Niri rejected the event stream".into());
                    }
                    changed = policy.event(&event)? || changed;
                }
            }
            if changed {
                policy.reconcile(&mut x11, started.elapsed().as_secs_f64())?;
            }
        }
    })();
    let restore = policy.close(&mut x11);
    match (result, restore) {
        (Ok(()), restore) => restore,
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(restore)) => {
            Err(format!("{error}; constraint restoration failed: {restore}").into())
        }
    }
}
