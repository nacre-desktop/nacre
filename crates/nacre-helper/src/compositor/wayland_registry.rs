//! Bounded registry capability probe; waits for socket readiness, no helper process.
use crate::Result;
use std::time::{Duration, Instant};
use wayland_client::{
    protocol::{wl_callback, wl_registry},
    Connection, Dispatch, QueueHandle,
};
#[derive(Default)]
struct Registry {
    gamma: bool,
    done: bool,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Registry {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, .. } = event {
            state.gamma |= interface == "zwlr_gamma_control_manager_v1";
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for Registry {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.done = true;
        }
    }
}
pub fn gamma_available(timeout: Duration) -> Result<bool> {
    probe(Connection::connect_to_env()?, timeout)
}
fn probe(connection: Connection, timeout: Duration) -> Result<bool> {
    let mut queue = connection.new_event_queue::<Registry>();
    let handle = queue.handle();
    let _registry = connection.display().get_registry(&handle, ());
    let _completion = connection.display().sync(&handle, ());
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("Invalid Wayland probe timeout")?;
    let mut state = Registry::default();
    loop {
        let _remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or("Wayland registry probe timed out")?;
        loop {
            match connection.flush() {
                Ok(()) => break,
                Err(wayland_client::backend::WaylandError::Io(error))
                    if error.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    wait(&connection, rustix::event::PollFlags::OUT, deadline)?
                }
                Err(error) => return Err(error.into()),
            }
        }
        queue.dispatch_pending(&mut state)?;
        if state.done {
            return Ok(state.gamma);
        }
        let Some(read) = connection.prepare_read() else {
            continue;
        };
        wait(&connection, rustix::event::PollFlags::IN, deadline)?;
        match read.read() {
            Ok(_) => {}
            Err(wayland_client::backend::WaylandError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
}
pub(crate) fn wait(
    connection: &Connection,
    events: rustix::event::PollFlags,
    deadline: Instant,
) -> Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or("Wayland registry probe timed out")?;
        let mut descriptors = [rustix::event::PollFd::new(connection, events)];
        let timeout = rustix::event::Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        match rustix::event::poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => return Err("Wayland registry probe timed out".into()),
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
    };
    fn event(id: u32, opcode: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_ne_bytes().to_vec();
        bytes.extend((((payload.len() + 8) as u32) << 16 | u32::from(opcode)).to_ne_bytes());
        bytes.extend(payload);
        bytes
    }
    #[test]
    fn private_registry_reports_gamma_and_times_out_without_processes() -> Result<()> {
        for gamma in [true, false] {
            let (client, mut server) = UnixStream::pair()?;
            let peer = std::thread::spawn(move || -> Result<()> {
                let mut requests = [0; 24];
                server.read_exact(&mut requests)?;
                let registry = u32::from_ne_bytes(requests[8..12].try_into()?);
                let callback = u32::from_ne_bytes(requests[20..24].try_into()?);
                let name = if gamma {
                    "zwlr_gamma_control_manager_v1"
                } else {
                    "wl_compositor"
                };
                let mut payload = 1u32.to_ne_bytes().to_vec();
                payload.extend(((name.len() + 1) as u32).to_ne_bytes());
                payload.extend(name.as_bytes());
                payload.push(0);
                while payload.len() & 3 != 0 {
                    payload.push(0);
                }
                payload.extend(1u32.to_ne_bytes());
                server.write_all(&event(registry, 0, &payload))?;
                server.write_all(&event(callback, 0, &1u32.to_ne_bytes()))?;
                // Keep the stream open until the client releases it.
                let mut tail = Vec::new();
                server.read_to_end(&mut tail)?;
                Ok(())
            });
            assert_eq!(
                probe(Connection::from_socket(client)?, Duration::from_secs(1))?,
                gamma
            );
            peer.join().map_err(|_| "Wayland fixture panicked")??;
        }
        let (client, _server) = UnixStream::pair()?;
        let began = Instant::now();
        assert!(
            probe(Connection::from_socket(client)?, Duration::from_millis(20))
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        assert!(began.elapsed() < Duration::from_secs(1));
        Ok(())
    }
}
