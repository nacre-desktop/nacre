//! Direct virtual-pointer control, confined to the nested proof compositor.
use crate::Result;
use std::{
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};
use wayland_client::{
    protocol::{wl_callback, wl_registry},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1 as manager, zwlr_virtual_pointer_v1 as pointer,
};

#[derive(Default)]
struct State {
    manager: Option<manager::ZwlrVirtualPointerManagerV1>,
    done: bool,
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == "zwlr_virtual_pointer_manager_v1" {
                state.manager = Some(registry.bind(name, version.min(2), queue, ()));
            }
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            state.done = true;
        }
    }
}
impl Dispatch<manager::ZwlrVirtualPointerManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &manager::ZwlrVirtualPointerManagerV1,
        _: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<pointer::ZwlrVirtualPointerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &pointer::ZwlrVirtualPointerV1,
        _: pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

pub struct Pointer {
    connection: Connection,
    queue: wayland_client::EventQueue<State>,
    state: State,
    pointer: pointer::ZwlrVirtualPointerV1,
}
impl Pointer {
    pub fn connect(socket: &Path) -> Result<Self> {
        let connection = Connection::from_socket(UnixStream::connect(socket)?)?;
        let mut queue = connection.new_event_queue::<State>();
        let handle = queue.handle();
        let _registry = connection.display().get_registry(&handle, ());
        let mut state = State::default();
        roundtrip(&connection, &mut queue, &mut state)?;
        let manager = state
            .manager
            .as_ref()
            .ok_or("Nested compositor has no virtual pointer manager")?;
        let pointer = manager.create_virtual_pointer(None, &handle, ());
        roundtrip(&connection, &mut queue, &mut state)?;
        Ok(Self {
            connection,
            queue,
            state,
            pointer,
        })
    }
    pub fn move_to(&mut self, x: u32, y: u32, width: u32, height: u32) -> Result<()> {
        self.pointer.motion_absolute(1, x, y, width, height);
        self.pointer.frame();
        roundtrip(&self.connection, &mut self.queue, &mut self.state)
    }
}
impl Drop for Pointer {
    fn drop(&mut self) {
        self.pointer.destroy();
        if let Err(error) = self.connection.flush() {
            eprintln!("Nested pointer cleanup failed: {error}");
        }
    }
}

fn roundtrip(
    connection: &Connection,
    queue: &mut wayland_client::EventQueue<State>,
    state: &mut State,
) -> Result<()> {
    state.done = false;
    let _callback = connection.display().sync(&queue.handle(), ());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match connection.flush() {
            Ok(()) => {}
            Err(wayland_client::backend::WaylandError::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                crate::compositor::wayland_registry::wait(
                    connection,
                    rustix::event::PollFlags::OUT,
                    deadline,
                )?;
            }
            Err(error) => return Err(error.into()),
        }
        queue.dispatch_pending(state)?;
        if state.done {
            return Ok(());
        }
        let Some(read) = connection.prepare_read() else {
            continue;
        };
        crate::compositor::wayland_registry::wait(
            connection,
            rustix::event::PollFlags::IN,
            deadline,
        )?;
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
