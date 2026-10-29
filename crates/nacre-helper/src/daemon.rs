//! The persistent desktop service. Tasks run on one current-thread runtime.
pub mod active_wallpaper;
pub mod ai_chat;
pub mod attach;
pub mod audio_peak;
pub mod audio_spectrum;
pub mod bar_services;
pub mod bar_tray_watcher;
pub mod behaviours;
pub mod bluetooth;
pub mod brain;
pub mod calendar;
pub mod capabilities;
pub mod client;
pub mod colors_app;
pub mod compositor;
pub mod compositor_control;
pub mod control_center;
pub mod crashes;
pub mod dashboard_extras;
pub mod dashboard_metrics;
pub mod dashboard_player;
pub mod displays;
pub mod fast;
pub mod helpers;
pub mod hub;
pub mod i18n;
pub mod lock_content;
pub mod login_theme;
pub mod media;
pub mod menu;
pub mod menu_worker;
pub mod network;
pub mod niri_actions;
pub mod niri_events;
pub mod notifications;
pub mod notify;
pub mod osd;
pub mod pill_controls;
pub mod serve_bridge;
pub mod session;
pub mod session_policy;
pub mod settings;
pub mod settings_controller;
pub mod settings_validation;
pub mod shell_actions;
pub mod shortcuts;
pub mod socket;
pub mod supervisor;
pub mod system_power;
pub mod system_status;
#[cfg(test)]
pub(crate) mod test_logs;
pub mod time_settings;
pub mod tools_launch;
pub mod updates;
pub mod wallpaper;
pub mod weather;
pub mod weather_photo;
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
    managed_notifications: bool,
    managed_behaviours: bool,
) -> Result<()> {
    let fast_listener = socket::FastListener::acquire(&listener.path)?;
    let listener_socket = tokio::net::UnixListener::from_std(listener.take()?)?;
    let (fast_requests, mut fast_rx) = tokio::sync::mpsc::channel(256);
    let (audio_step_events, mut audio_step_rx) = tokio::sync::mpsc::unbounded_channel();
    let audio_step_worker = pill_controls::AudioStepWorker::start(audio_step_events)?;
    let audio_available = pill_controls::fast_backend_present("audio.step");
    let fast_server = fast::Server::start(
        fast_listener,
        fast_requests,
        audio_step_worker.sender(),
        counter.safe,
        audio_available,
    )?;
    helpers::cleanup()?;
    let hub = hub::Hub::new(uuid::Uuid::new_v4().to_string(), counter.safe, frame).shared();
    hub.borrow_mut().publish("compositor",json!({"available":false,"error":"Connecting to compositor","outputs":{},"outputOrder":[],"workspaces":{},"workspaceOrder":[],"windows":{},"windowOrder":[]}))?;
    let status = hub.borrow().status();
    hub.borrow_mut().publish("daemon", status)?;
    let environment: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    let control = compositor_control::Control::new(paths.clone(), environment.clone());
    hub.borrow_mut().compositor = Some(control.clone());
    let tray_watcher_service = if !counter.safe {
        Some(tokio::task::spawn_local(supervisor::run(
            "tray-watcher",
            hub.clone(),
            bar_tray_watcher::run,
        )))
    } else {
        None
    };
    let tray_service = if !counter.safe {
        hub.borrow_mut().service("tray", "starting", 0)?;
        let tray_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "tray",
            hub.clone(),
            move || bar_services::run_tray(tray_hub.clone()),
        )))
    } else {
        None
    };
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
        network::publish_core_starting(&hub)?;
        hub.borrow_mut().service("network-watch", "starting", 0)?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        let generation = Rc::new(std::cell::Cell::new(0));
        let identity_generation = Rc::new(std::cell::Cell::new(0));
        Some(tokio::task::spawn_local(supervisor::run(
            "network-watch",
            service_hub,
            move || {
                network::run(
                    factory_hub.clone(),
                    generation.clone(),
                    identity_generation.clone(),
                )
            },
        )))
    } else {
        None
    };
    let wallpaper_service = if !counter.safe {
        let generations = Rc::new(RefCell::new(std::collections::BTreeMap::new()));
        wallpaper::publish(&hub, &generations, false, None)?;
        active_wallpaper::initial(&hub)?;
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
    let calendar_service = if !counter.safe {
        calendar::initial(&hub)?;
        let wake = Rc::new(tokio::sync::Notify::new());
        hub.borrow_mut().calendar = Some(wake.clone());
        let calendar_hub = hub.clone();
        let calendar_paths = paths.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "dashboard-calendar",
            hub.clone(),
            move || calendar::run(calendar_hub.clone(), calendar_paths.clone(), wake.clone()),
        )))
    } else {
        None
    };
    let media_service = if !counter.safe {
        media::initial(&hub)?;
        let media_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "dashboard-media",
            hub.clone(),
            move || media::run(media_hub.clone()),
        )))
    } else {
        None
    };
    let updates_service = if !counter.safe {
        updates::initial(&hub)?;
        let updates_hub = hub.clone();
        let wake = std::sync::Arc::new(tokio::sync::Notify::new());
        hub.borrow_mut().updates_wake = Some(wake.clone());
        Some(tokio::task::spawn_local(updates::run(updates_hub, wake)))
    } else {
        None
    };
    let weather_service = if !counter.safe {
        weather::initial(&hub)?;
        Some(tokio::task::spawn_local(weather::run(
            hub.clone(),
            paths.clone(),
        )))
    } else {
        None
    };
    weather_photo::initial(&hub)?;
    let weather_photo_service = if !counter.safe {
        let photo_hub = hub.clone();
        let photo_paths = paths.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "weather-photo",
            hub.clone(),
            move || weather_photo::run(photo_hub.clone(), photo_paths.clone()),
        )))
    } else {
        None
    };
    let dashboard_service = if !counter.safe {
        let requests = dashboard_extras::Requests::new(paths.clone());
        hub.borrow_mut().dashboard = Some(requests.clone());
        dashboard_extras::initial(&hub, &requests)?;
        hub.borrow_mut()
            .service("dashboard-extras", "starting", 0)?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "dashboard-extras",
            service_hub,
            move || dashboard_extras::run(factory_hub.clone(), requests.clone()),
        )))
    } else {
        None
    };
    let (player_service, metrics_service) = if !counter.safe {
        dashboard_player::initial(&hub)?;
        dashboard_metrics::initial(&hub)?;
        let (player_hub, player_paths) = (hub.clone(), paths.clone());
        let player = tokio::task::spawn_local(supervisor::run(
            "dashboard-player",
            hub.clone(),
            move || dashboard_player::run(player_hub.clone(), player_paths.clone()),
        ));
        let metrics_hub = hub.clone();
        let metrics = tokio::task::spawn_local(supervisor::run(
            "dashboard-system",
            hub.clone(),
            move || {
                dashboard_metrics::run(metrics_hub.clone(), dashboard_metrics::Sources::system())
            },
        ));
        (Some(player), Some(metrics))
    } else {
        (None, None)
    };
    // N0059: publish unavailable immediately; background probes never gate READY.
    let system_power_service = if !counter.safe {
        system_power::initial(&hub)?;
        let power_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "system-power",
            hub.clone(),
            move || system_power::run(power_hub.clone()),
        )))
    } else {
        None
    };
    // N0043/N0059: one audio subscription serves both OSD and audio state leases.
    let osd_service = if !counter.safe {
        osd::initial(&hub)?;
        let (osd_hub, osd_paths) = (hub.clone(), paths.clone());
        Some(tokio::task::spawn_local(supervisor::run(
            "osd",
            hub.clone(),
            move || osd::run(osd_hub.clone(), osd_paths.clone()),
        )))
    } else {
        None
    };
    // N0061: the microphone peak capture runs only while `audio.peak` has a subscriber.
    let audio_peak_service = if !counter.safe {
        audio_peak::initial(&hub)?;
        let peak_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "audio-peak",
            hub.clone(),
            move || audio_peak::run(peak_hub.clone(), audio_peak::Env::production()),
        )))
    } else {
        None
    };
    let spectrum_service = if !counter.safe {
        audio_spectrum::initial(&hub)?;
        let spectrum_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "audio-spectrum",
            hub.clone(),
            move || audio_spectrum::run(spectrum_hub.clone(), audio_spectrum::Env::production()),
        )))
    } else {
        None
    };
    // N0045: connects to the system bus; the agent is registered only while a pairing runs.
    let bluetooth_service = if !counter.safe {
        bluetooth::initial(&hub)?;
        let bluetooth_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "bluetooth-pairing",
            hub.clone(),
            move || bluetooth::run(bluetooth_hub.clone()),
        )))
    } else {
        None
    };
    // N0046: the `controls` topic; producers run only while a client subscribes.
    let controls_service = if !counter.safe {
        let controls_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "controls",
            hub.clone(),
            move || {
                let hub = controls_hub.clone();
                async move {
                    control_center::initial(&hub).await?;
                    control_center::run(hub).await
                }
            },
        )))
    } else {
        None
    };
    // N0049: the `time` and `capabilities` topics for the Settings pages.
    let settings_page_services = if !counter.safe {
        time_settings::initial(&hub)?;
        capabilities::initial(&hub)?;
        shortcuts::initial(&hub)?;
        displays::initial(&hub)?;
        let (time_hub, capabilities_hub, shortcuts_hub, displays_hub) =
            (hub.clone(), hub.clone(), hub.clone(), hub.clone());
        vec![
            tokio::task::spawn_local(supervisor::run("displays", hub.clone(), move || {
                displays::run(displays_hub.clone())
            })),
            tokio::task::spawn_local(supervisor::run("shortcuts", hub.clone(), move || {
                shortcuts::run(shortcuts_hub.clone())
            })),
            tokio::task::spawn_local(supervisor::run("time", hub.clone(), move || {
                time_settings::run(time_hub.clone())
            })),
            tokio::task::spawn_local(supervisor::run("capabilities", hub.clone(), move || {
                capabilities::run(capabilities_hub.clone())
            })),
        ]
    } else {
        Vec::new()
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
    let ai_chat_service = if !counter.safe {
        ai_chat::initial(&hub)?;
        hub.borrow_mut().service("ai-chat", "starting", 0)?;
        let service_hub = hub.clone();
        let factory_hub = hub.clone();
        Some(tokio::task::spawn_local(supervisor::run(
            "ai-chat",
            service_hub,
            move || ai_chat::run(factory_hub.clone()),
        )))
    } else {
        None
    };
    if !counter.safe {
        brain::initial(&hub)?;
    }
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
    let notifications_service = if managed_notifications && !counter.safe {
        let notifications_hub = hub.clone();
        let notifications_paths = paths.clone();
        Some(tokio::task::spawn_local(async move {
            let result = notifications::run(notifications_hub.clone(), notifications_paths).await;
            if let Err(error) = &result {
                let _ = notifications_hub
                    .borrow_mut()
                    .service("notifications", "unavailable", 0);
                tracing::warn!(
                    reason = %error,
                    "Managed notifications stopped; other daemon services remain available"
                );
            }
            result
        }))
    } else {
        None
    };
    // N0039 startup authority. The `behaviours` service entry tells UIs to stand down,
    // so it appears only when the behaviours really run (never in safe mode).
    let behaviour_services = if managed_behaviours && !counter.safe {
        let (rotation_hub, rotation_paths) = (hub.clone(), paths.clone());
        let (power_hub, alerts_hub, alerts_paths) = (hub.clone(), hub.clone(), paths.clone());
        let (lock_hub, lock_paths) = (hub.clone(), paths.clone());
        let services = vec![
            tokio::task::spawn_local(supervisor::run(
                "wallpaper-rotation",
                hub.clone(),
                move || behaviours::run_rotation(rotation_hub.clone(), rotation_paths.clone()),
            )),
            tokio::task::spawn_local(supervisor::run("power-profile", hub.clone(), move || {
                behaviours::run_power_profile(power_hub.clone())
            })),
            tokio::task::spawn_local(supervisor::run("battery-alerts", hub.clone(), move || {
                behaviours::run_battery_alerts(alerts_hub.clone(), alerts_paths.clone())
            })),
            tokio::task::spawn_local(supervisor::run("lock-content", hub.clone(), move || {
                lock_content::run_lock_content(lock_hub.clone(), lock_paths.clone())
            })),
        ];
        hub.borrow_mut().service("behaviours", "running", 0)?;
        services
    } else {
        Vec::new()
    };
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
    let mut fast_calls = tokio::task::JoinSet::new();
    let watchdog = notify::watchdog();
    tokio::pin!(watchdog);
    // N0102: the audio step worker keeps the default sink's level from the `audio` topic.
    let audio_observer = audio_step_worker.sender();
    let mut audio_notices = hub.borrow().observe();
    let mut observed_audio = None;
    let mut observing_audio = true;
    loop {
        tokio::select! {
            result=&mut ready,if !is_ready=>{result?;is_ready=true;tracing::info!("Daemon ready");},
            result=&mut watchdog=>{result?;return Err("Watchdog task ended".into());},
            _=terminate.recv()=>break,
            _=interrupt.recv()=>break,
            accepted=listener_socket.accept()=>{
                let (stream,_)=accepted?;let hub=hub.clone();clients.spawn_local(async move {session::serve(stream,hub).await});
            },
            Some(request)=fast_rx.recv()=>{
                let hub=hub.clone();
                fast_calls.spawn_local(async move {
                    if let Err(error)=session::dispatch_fast(hub,request).await {
                        tracing::warn!(code=?error.code,"Queued one-shot daemon call failed");
                    }
                });
            },
            notice=audio_notices.recv(),if observing_audio=>match notice {
                Ok(hub::Notice::Changed{topic,..}) if topic=="audio"=>{
                    let observation=pill_controls::audio_observation(&hub);
                    if observation.is_some() && observation!=observed_audio {
                        observed_audio=observation.clone();
                        if let Some(observation)=observation {
                            let _=audio_observer.try_send(pill_controls::AudioStepCommand::Observe(observation));
                        }
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed)=>observing_audio=false,
                _=>{},
            },
            Some(completion)=audio_step_rx.recv()=>{
                match completion.result {
                    Ok(outcome)=>{
                        if let Err(error)=pill_controls::publish_audio_step(&hub,&outcome,completion.key_ms) {
                            tracing::warn!(code=?error.code,"Completed audio step could not be published");
                        }
                    },
                    Err(error)=>tracing::warn!(code=?error.code,"Queued audio step failed"),
                }
            },
            result=fast_calls.join_next(),if !fast_calls.is_empty()=>match result {
                Some(Ok(()))=>{},
                Some(Err(_))=>tracing::error!("Queued one-shot daemon call task failed"),
                None=>{},
            },
            result=clients.join_next(),if !clients.is_empty()=>match result {
                Some(Ok(Ok(())))=>{},
                Some(Ok(Err(error)))=>tracing::debug!(code=?error.code,"Client disconnected"),
                Some(Err(_))=>tracing::error!("Client task failed"),
                None=>{},
            },
        }
    }
    drop(fast_server);
    drop(audio_observer);
    drop(audio_step_worker);
    fast_calls.abort_all();
    while fast_calls.join_next().await.is_some() {}
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    let mut notification_flush_failed = false;
    if let Some(mut service) = notifications_service {
        if let Some(requests) = &hub.borrow().notifications {
            requests.stop();
        }
        // A finished JoinHandle must not be polled again, so abort only on timeout.
        match tokio::time::timeout(Duration::from_secs(10), &mut service).await {
            Ok(Ok(Ok(()))) => {}
            Ok(_) => {
                notification_flush_failed = true;
                tracing::warn!("Notification service had already failed before shutdown");
            }
            Err(_) => {
                notification_flush_failed = true;
                service.abort();
                let _ = service.await;
                tracing::warn!("Notification shutdown did not confirm its history flush");
            }
        }
    }
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
    if let Some(service) = weather_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = media_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = calendar_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = updates_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = weather_photo_service {
        service.abort();
        let _ = service.await;
    }
    compositor_service.abort();
    if let Some(service) = tray_watcher_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Tray watcher supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = tray_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Tray supervisor failed during shutdown");
            }
        }
    }
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
    if let Some(service) = bluetooth_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Bluetooth supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = system_power_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = osd_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("OSD supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = audio_peak_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = spectrum_service {
        service.abort();
        let _ = service.await;
    }
    if let Some(service) = controls_service {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Controls supervisor failed during shutdown");
            }
        }
    }
    for service in settings_page_services {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Settings page supervisor failed during shutdown");
            }
        }
    }
    for service in behaviour_services {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Behaviour supervisor failed during shutdown");
            }
        }
    }
    if let Some(service) = ai_chat_service {
        // Dropping the host closes every worker pipe; workers stop their providers.
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Assistant chat supervisor failed during shutdown");
            }
        }
    }
    for service in [dashboard_service, player_service, metrics_service]
        .into_iter()
        .flatten()
    {
        service.abort();
        if let Err(error) = service.await {
            if !error.is_cancelled() {
                tracing::error!("Dashboard supervisor failed during shutdown");
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
    if notification_flush_failed {
        return Err("Notification history flush was not confirmed".into());
    }
    counter.clean()?;
    tracing::info!("Daemon stopped");
    Ok(())
}
/// Startup authorities: `--notifications=legacy|daemon`, `--behaviours=legacy|daemon`.
fn authorities(arguments: &[String]) -> Result<(bool, bool)> {
    let (mut notifications, mut behaviours) = (None, None);
    for argument in arguments {
        let (slot, value) = match argument.as_str() {
            "--notifications=legacy" => (&mut notifications, false),
            "--notifications=daemon" => (&mut notifications, true),
            "--behaviours=legacy" => (&mut behaviours, false),
            "--behaviours=daemon" => (&mut behaviours, true),
            _ => {
                return Err("Expected nacre-helper daemon [--notifications=legacy|daemon] [--behaviours=legacy|daemon]".into());
            }
        };
        if slot.replace(value).is_some() {
            return Err("Daemon authority options may be given once".into());
        }
    }
    Ok((notifications.unwrap_or(false), behaviours.unwrap_or(false)))
}
pub fn run(arguments: &[String]) -> Result<i32> {
    crate::process::configure_daemon_allocator();
    let (managed_notifications, managed_behaviours) = authorities(arguments)?;
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
    // The payload may carry user data, so only the source location is logged.
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(at) => tracing::error!(
            file = at.file(),
            line = at.line(),
            "Daemon task panicked; payload omitted"
        ),
        None => tracing::error!("Daemon task panicked; payload omitted"),
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
            managed_notifications,
            managed_behaviours,
        ),
    );
    runtime.shutdown_timeout(Duration::from_secs(2));
    result?;
    Ok(0)
}
