use niri::niri::State;
use niri_config::Config;
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use std::{env, path::Path};
fn main() {
    let mut args = env::args().skip(1);
    let config = args.next().expect("config");
    let count: u8 = args.next().unwrap_or("3".into()).parse().unwrap();
    let config = Config::load(Path::new(&config)).config.unwrap();
    let mut event_loop = EventLoop::<State>::try_new().unwrap();
    let mut state = State::new(
        config,
        event_loop.handle(),
        event_loop.get_signal(),
        Display::new().unwrap(),
        true,
        true,
        false,
    )
    .unwrap();
    state.backend.headless().add_renderer().unwrap();
    for n in 1..=count {
        state
            .backend
            .headless()
            .add_output(&mut state.niri, n, (800, 600));
    }
    state.refresh_and_flush_clients();
    println!(
        "{}\t{}",
        state.niri.socket_name.as_ref().unwrap().to_string_lossy(),
        state
            .niri
            .ipc_server
            .as_ref()
            .unwrap()
            .socket_path
            .as_ref()
            .unwrap()
            .display()
    );
    event_loop
        .run(None, &mut state, |state| state.refresh_and_flush_clients())
        .unwrap();
}
