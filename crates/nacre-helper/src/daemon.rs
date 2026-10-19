//! The persistent desktop service. Tasks run on one current-thread runtime.
pub mod attach;
pub mod client;
pub mod compositor;
pub mod compositor_control;
pub mod crashes;
pub mod helpers;
pub mod hub;
pub mod menu;
pub mod menu_worker;
pub mod network;
pub mod niri_actions;
pub mod niri_events;
pub mod notify;
pub mod serve_bridge;
pub mod session;
pub mod session_policy;
pub mod settings;
pub mod settings_controller;
pub mod settings_validation;
pub mod socket;
pub mod supervisor;
pub mod wallpaper;
pub mod widths;
pub mod workers;
pub mod writer_lease;

use crate::{paths::Paths, Result};
use serde_json::json;
use std::{cell::RefCell, rc::Rc, time::Duration};

async fn serve(
    mut listener: socket::Listener,
    mut counter: crashes::Counter,
    frame: Duration,
    paths: Paths,
    writer_lease: std::sync::Arc<writer_lease::WriterLease>,
) -> Result<()> {
    let listener_socket = tokio::net::UnixListener::from_std(listener.take()?)?;
    helpers::cleanup()?;
    let hub = hub::Hub::new(uuid::Uuid::new_v4().to_string(), counter.safe, frame).shared();
    hub.borrow_mut().publish("compositor",json!({"available":false,"error":"Connecting to compositor","outputs":{},"outputOrder":[],"workspaces":{},"workspaceOrder":[],"windows":{},"windowOrder":[]}))?;
    let status = hub.borrow().status();
    hub.borrow_mut().publish("daemon", status)?;
    let environment: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    let control = compositor_control::Control::new(paths.clone(), environment.clone());
    hub.borrow_mut().compositor = Some(control.clone());
    let settings_hub = hub.clone();
    let settings = settings::Service::load(hub.clone(), paths.clone(), Some(writer_lease)).await?;
    hub.borrow_mut().settings = Some(settings.clone());
    let (settings_ready, settings_initialized) = tokio::sync::oneshot::channel();
    let settings_ready = Rc::new(RefCell::new(Some(settings_ready)));
    hub.borrow_mut().service("settings", "starting", 0)?;
    let settings_service =
        tokio::task::spawn_local(supervisor::run("settings", hub.clone(), move || {
            settings::run(
                settings_hub.clone(),
                settings.clone(),
                settings_ready.clone(),
            )
        }));
    let session_control = control.clone();
    let session_paths = paths.clone();
    let width_environment = environment.clone();
    let width_control = control.clone();
    if matches!(crate::compositor::backend::select(&environment), Ok("niri")) {
        hub.borrow_mut().publish(
            niri_events::TOPIC,
            json!({"backend":"niri","available":false,"connectionId":null}),
        )?;
    }
    let (ready, initialized) = tokio::sync::oneshot::channel();
    let ready = Rc::new(RefCell::new(Some(ready)));
    let _ready_keepalive = ready.clone();
    let service_hub = hub.clone();
    let factory_hub = hub.clone();
    let compositor_service =
        tokio::task::spawn_local(supervisor::run("compositor", service_hub, move || {
            compositor::run(
                factory_hub.clone(),
                control.clone(),
                environment.clone(),
                ready.clone(),
            )
        }));
    let session_service = if !counter.safe {
        hub.borrow_mut().service("session-watch", "starting", 0)?;
        hub.borrow_mut()
            .publish("session", json!({"available":false,"record":null}))?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "session-watch",
            service_hub,
            move || {
                session_policy::run(
                    factory_hub.clone(),
                    session_paths.clone(),
                    session_control.clone(),
                )
            },
        )))
    } else {
        None
    };
    if !counter.safe {
        hub.borrow_mut().service("helper-lanes", "running", 0)?;
    }
    let network_service = if !counter.safe {
        network::publish(&hub, 0, false, None)?;
        hub.borrow_mut().service("network-watch", "starting", 0)?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        let generation = Rc::new(std::cell::Cell::new(0));
        Some(tokio::task::spawn_local(supervisor::run(
            "network-watch",
            service_hub,
            move || network::run(factory_hub.clone(), generation.clone()),
        )))
    } else {
        None
    };
    let wallpaper_service = if !counter.safe {
        let generations = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
        wallpaper::publish(&hub, &generations, false, None)?;
        hub.borrow_mut().service("wallpaper-watch", "starting", 0)?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        let wallpaper_paths = paths.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "wallpaper-watch",
            service_hub,
            move || {
                wallpaper::run(
                    factory_hub.clone(),
                    wallpaper_paths.clone(),
                    generations.clone(),
                )
            },
        )))
    } else {
        None
    };
    let menu_service = if !counter.safe {
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        let menu_paths = paths.clone();
        hub.borrow_mut().service("menu", "starting", 0)?;
        Some(tokio::task::spawn_local(supervisor::run(
            "menu",
            service_hub,
            move || menu::run(factory_hub.clone(), menu_paths.clone()),
        )))
    } else {
        None
    };
    let workers = workers::Workers::default();
    let width_shutdown = widths::Shutdown::default();
    let width_service = if !counter.safe
        && matches!(
            crate::compositor::backend::select(&width_environment),
            Ok("niri")
        )
        && width_environment
            .get("DISPLAY")
            .is_some_and(|v| !v.is_empty())
    {
        hub.borrow_mut().service("niri-widths", "starting", 0)?;
        hub.borrow_mut().publish(
            widths::TOPIC,
            json!({"available":false,"opening_width":50.0,"relaxed":0}),
        )?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        let stop = width_shutdown.clone();
        let opening = Rc::new(std::cell::Cell::new(50.0));
        Some(tokio::task::spawn_local(supervisor::run(
            "niri-widths",
            service_hub,
            move || {
                widths::run(
                    factory_hub.clone(),
                    width_control.clone(),
                    width_environment.clone(),
                    opening.clone(),
                    stop.clone(),
                )
            },
        )))
    } else {
        None
    };
    let mut notice_worker = None;
    if counter.safe {
        let notice = json!({"id":"daemon-safe-mode","summary":"Nacre safe mode","body":"Some desktop services are paused after repeated crashes."});
        hub.borrow_mut().publish(
            "daemon.notice",
            json!({"items":{"daemon-safe-mode":notice},"order":["daemon-safe-mode"]}),
        )?;
        if counter.notify {
            notice_worker = Some(tokio::task::spawn_local(async move {
                if workers.safe_notice().await.is_err() {
                    tracing::warn!("Safe-mode notification could not be delivered; notice remains available on the private link");
                }
            }));
        }
    }
    let ready = async {
        initialized.await?;
        settings_initialized.await?;
        notify::ready().await?;
        Ok::<(), crate::Error>(())
    };
    tokio::pin!(ready);
    let mut is_ready = false;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut clients = tokio::task::JoinSet::new();
    let watchdog = notify::watchdog();
    tokio::pin!(watchdog);
    loop {
        tokio::select! {
            result=&mut ready,if !is_ready=>{result?;is_ready=true;tracing::info!("Daemon ready");},
            result=&mut watchdog=>{result?;return Err("Watchdog task ended".into());},
            _=terminate.recv()=>break,
            _=interrupt.recv()=>break,
            accepted=listener_socket.accept()=>{
                let (stream,_)=accepted?;let hub=hub.clone();clients.spawn_local(async move {session::serve(stream,hub).await});
            },
            result=clients.join_next(),if !clients.is_empty()=>match result {
                Some(Ok(Ok(())))=>{},
                Some(Ok(Err(error)))=>tracing::debug!(code=?error.code,"Client disconnected"),
                Some(Err(_))=>tracing::error!("Client task failed"),
                None=>{},
            },
        }
    }
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    if let Some(service) = width_service {
        if width_shutdown.finish().await.is_err() {
            tracing::warn!("Width observer did not confirm hint restoration before shutdown");
        }
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Width supervisor failed during shutdown");
            }
        }
    }
    compositor_service.abort();
    settings_service.abort();
    if let Err(error) = settings_service.await {
        if !error.is_cancelled() {
            tracing::error!("Settings supervisor failed during shutdown");
        }
    }
    if let Some(service) = network_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Network supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = wallpaper_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Wallpaper supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = menu_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Launcher supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = session_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Session supervisor failed during shutdown");
            }
        }
    }
    if let Err(error) = compositor_service.await {
        if !error.is_cancelled() {
            tracing::error!("Compositor supervisor failed during shutdown");
        }
    }
    if let Some(worker) = notice_worker {
        worker.abort();
        if let Err(error) = worker.await {
            if !error.is_cancelled() {
                tracing::error!("Notification worker task failed during shutdown");
            }
        }
    }
    counter.clean()?;
    tracing::info!("Daemon stopped");
    Ok(())
}
pub fn run(arguments: &[String]) -> Result<i32> {
    if !arguments.is_empty() {
        return Err(
            "Expected nacre-helper daemon (NACRE_DAEMON_SOCKET selects a private endpoint)".into(),
        );
    }
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) => tracing_subscriber::EnvFilter::try_new(value)
            .map_err(|_| "Invalid daemon logging filter")?,
        Err(std::env::VarError::NotPresent) => {
            tracing_subscriber::EnvFilter::new("nacre_helper=info")
        }
        Err(_) => return Err("Invalid daemon logging filter".into()),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|_| "Cannot initialize daemon logging")?;
    std::panic::set_hook(Box::new(|_| {
        tracing::error!("Daemon task panicked; payload omitted")
    }));
    let paths = Paths::environment()?;
    let writer_lease = std::sync::Arc::new(writer_lease::WriterLease::acquire_for_paths(
        &paths.config,
        &paths.state,
    )?);
    let path = socket::path()?;
    let listener = socket::Listener::acquire(path.clone())?;
    let counter = crashes::Counter::begin(path.parent().ok_or("Missing daemon socket folder")?)?;
    let frame = std::env::var("NACRE_DAEMON_FRAME_MS")
        .ok()
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(16);
    if !(1..=1000).contains(&frame) {
        return Err("Invalid daemon frame period".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .thread_keep_alive(Duration::from_secs(1))
        .build()?;
    let result = tokio::task::LocalSet::new().block_on(
        &runtime,
        serve(
            listener,
            counter,
            Duration::from_millis(frame),
            paths,
            writer_lease,
        ),
    );
    runtime.shutdown_timeout(Duration::from_secs(2));
    result?;
    Ok(0)
}
