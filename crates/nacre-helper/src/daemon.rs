//! The persistent desktop service. Tasks run on one current-thread runtime.
pub mod client;
pub mod compositor;
pub mod crashes;
pub mod hub;
pub mod notify;
pub mod session;
pub mod socket;
pub mod supervisor;
pub mod workers;

use crate::{paths::Paths, Result};
use serde_json::json;
use std::{cell::RefCell, rc::Rc, time::Duration};

async fn serve(
    mut listener: socket::Listener,
    mut counter: crashes::Counter,
    frame: Duration,
) -> Result<()> {
    let listener_socket = tokio::net::UnixListener::from_std(listener.take()?)?;
    let hub = hub::Hub::new(uuid::Uuid::new_v4().to_string(), counter.safe, frame).shared();
    hub.borrow_mut().publish("compositor",json!({"available":false,"error":"Connecting to compositor","outputs":{},"outputOrder":[],"workspaces":{},"workspaceOrder":[],"windows":{},"windowOrder":[]}))?;
    let status = hub.borrow().status();
    hub.borrow_mut().publish("daemon", status)?;
    let paths = Paths::environment()?;
    let environment: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    let (ready, initialized) = tokio::sync::oneshot::channel();
    let ready = Rc::new(RefCell::new(Some(ready)));
    let _ready_keepalive = ready.clone();
    let service_hub = hub.clone();
    let factory_hub = hub.clone();
    let compositor_service =
        tokio::task::spawn_local(supervisor::run("compositor", service_hub, move || {
            compositor::run(
                factory_hub.clone(),
                paths.clone(),
                environment.clone(),
                ready.clone(),
            )
        }));
    let workers = workers::Workers::default();
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
    compositor_service.abort();
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
        .thread_keep_alive(Duration::from_secs(30))
        .build()?;
    let result = tokio::task::LocalSet::new().block_on(
        &runtime,
        serve(listener, counter, Duration::from_millis(frame)),
    );
    runtime.shutdown_timeout(Duration::from_secs(2));
    result?;
    Ok(0)
}
