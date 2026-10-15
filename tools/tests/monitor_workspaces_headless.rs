use niri::layout::LayoutElement;
use niri::niri::State;
use niri_config::Config;
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use std::{env, path::Path};
fn main() {
    let mut args = env::args().skip(1);
    let config = args.next().expect("config");
    let count: u8 = args.next().unwrap_or("3".into()).parse().unwrap();
    let telemetry = args.next();
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
        .run(None, &mut state, |state| {
            state.refresh_and_flush_clients();
            if let Some(path) = &telemetry {
                if let Some(ws) = state.niri.layout.active_workspace() {
                    let tiles: Vec<_> = ws
                        .tiles_with_render_positions()
                        .map(|(tile, pos, _)| {
                            format!("[{}, {}, {}]", tile.window().id().get(), pos.x, pos.y)
                        })
                        .collect();
                    let zoom = state.niri.layout.overview_zoom();
                    let mut visible = Vec::new();
                    for mon in state.niri.layout.monitors() {
                        for (workspace, geo) in mon.workspaces_with_render_geo() {
                            for (tile, pos, shown) in workspace.tiles_with_render_positions() {
                                if shown {
                                    let size = tile.tile_size();
                                    visible.push(format!(
                                        "[{}, {}, {}, {}, {}]",
                                        tile.window().id().get(),
                                        geo.loc.x + pos.x * zoom,
                                        geo.loc.y + pos.y * zoom,
                                        size.w * zoom,
                                        size.h * zoom
                                    ));
                                }
                            }
                        }
                    }
                    let selected = state
                        .niri
                        .layout
                        .focus()
                        .map_or("null".into(), |window| window.id().get().to_string());
                    let value = format!(
                        "{{\"tiles\":[{}],\"visible\":[{}],\"selected\":{}}}",
                        tiles.join(","),
                        visible.join(","),
                        selected
                    );
                    let next = format!("{path}.next");
                    std::fs::write(&next, value).unwrap();
                    std::fs::rename(next, path).unwrap();
                }
            }
        })
        .unwrap();
}
