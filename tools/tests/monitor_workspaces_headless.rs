use niri::layout::LayoutElement;
use niri::niri::State;
use niri_config::Config;
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use std::{env, path::Path, io::{BufRead, Write}};
fn main() {
    let mut args = env::args().skip(1);
    let config = args.next().expect("config");
    let count: u8 = args.next().unwrap_or("3".into()).parse().unwrap();
    let telemetry = args.next();
    let config_path = niri_config::ConfigPath::Explicit(Path::new(&config).to_path_buf());
    let parsed = Config::load(Path::new(&config));
    let includes = parsed.includes;
    let config = parsed.config.unwrap();
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
    niri::utils::watcher::setup(&mut state, &config_path, includes);
    state.backend.headless().add_renderer().unwrap();
    for n in 1..=count {
        state
            .backend
            .headless()
            .add_output(&mut state.niri, n, (800, 600));
    }
    // Commands arrive only on this owned driver's stdin. Removing several
    // connectors in one dispatch models an undock without intermediate client work.
    let (sender, receiver) = smithay::reexports::calloop::channel::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if sender.send(line).is_err() { break; }
        }
    });
    event_loop.handle().insert_source(receiver, |event, _, state| {
        if let smithay::reexports::calloop::channel::Event::Msg(line) = event {
            let mut words = line.split_whitespace();
            let command = words.next().unwrap_or("");
            for number in words.filter_map(|s| s.parse::<u8>().ok()) {
                let name = format!("headless-{number}");
                let output = state.niri.global_space.outputs().find(|o| o.name() == name).cloned();
                if command == "remove" {
                    if let Some(output) = output {
                        state.niri.remove_output(&output);
                        state.backend.headless().ipc_outputs().lock().unwrap().retain(|_, o| o.name != name);
                    }
                } else if command == "add" && output.is_none() {
                    state.backend.headless().add_output(&mut state.niri, number, (800,600));
                }
            }
            state.niri.on_ipc_outputs_changed();
            state.refresh_and_flush_clients();
            println!("done {line}");
            std::io::stdout().flush().unwrap();
        }
    }).unwrap();
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
