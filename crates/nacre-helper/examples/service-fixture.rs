//! Developer-only replacements for the frozen injected Python test providers.
//! The installed helper never exposes these handlers or loads Python modules.
use nacre_helper::{json, launcher_preferences, notification_history, service, Result};
use serde_json::json as value;
use std::{
    io::{self, Read, Write},
    path::Path,
};

fn wait_for_release() -> Result<()> {
    use rustix::fs::inotify;
    let watcher = inotify::init(inotify::CreateFlags::CLOEXEC)?;
    inotify::add_watch(
        &watcher,
        "/case",
        inotify::WatchFlags::CREATE
            | inotify::WatchFlags::MOVED_TO
            | inotify::WatchFlags::CLOSE_WRITE,
    )?;
    let mut events = std::fs::File::from(watcher);
    let mut buffer = [0; 8192];
    while !Path::new("/case/release").exists() {
        match events.read(&mut buffer) {
            Ok(0) => return Err("Fixture event source closed".into()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mode = std::env::args()
        .nth(1)
        .ok_or("Missing native fixture mode")?;
    if !["gate", "crash", "exit", "oversized", "serve"].contains(&mode.as_str()) {
        return Err("Unknown native fixture mode".into());
    }
    let status = service::serve(
        io::stdin().lock(),
        io::stdout(),
        service::Limits::default(),
        move |request| {
            let result = match (mode.as_str(), request.tool.as_str()) {
                ("serve", "dashboard-data") => {
                    let marker = std::env::var_os("NACRE_TEST_MARKER")
                        .ok_or("Missing private transport marker")?;
                    let mut marker = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(marker)?;
                    writeln!(marker, "started")?;
                    marker.flush()?;
                    drop(marker);
                    // Deliberately stalled developer provider. QML must reject
                    // both requests, stop this worker, restart and never replay.
                    loop {
                        std::thread::park();
                    }
                }
                ("gate", "dashboard-data") => {
                    wait_for_release()?;
                    value!({"value":request.arguments.get(1).map(String::as_str).unwrap_or("")})
                }
                ("crash", "dashboard-media") => return Err("fixture helper crashed".into()),
                ("exit", "dashboard-media") => std::process::exit(17),
                ("oversized", "dashboard-data") => value!({"value":"x".repeat(8*1024*1024)}),
                (_, "launcher-preferences") => {
                    launcher_preferences::execute(&request.arguments, request.input.clone())?
                        .ok_or("Missing fixture response")?
                }
                (_, "notification-history") => {
                    notification_history::execute(&request.arguments, request.input.clone())?
                        .ok_or("Missing fixture response")?
                }
                _ => return Err("Unsupported helper operation".into()),
            };
            Ok(format!("{}\n", json::encode(&result)?))
        },
    )?;
    std::process::exit(status);
}
