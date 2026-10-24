//! City photos are leased by topic subscription, with cancellable async network work.
mod fetch;
use super::{behaviours, hub::Shared};
use crate::{paths::Paths, Result};
use futures_util::future::LocalBoxFuture;
pub use nacre_protocol::weather_photo::TOPIC;
use nacre_protocol::weather_photo::{Asset, Location, State, Status};
use std::{path::Path, rc::Rc, time::Duration};

type Loader = Rc<dyn Fn(Paths, Location) -> LocalBoxFuture<'static, Result<fetch::Photo>>>;

fn publish(
    hub: &Shared,
    enabled: bool,
    status: Status,
    location: Option<Location>,
    asset: Option<Asset>,
) -> Result<()> {
    hub.borrow_mut().publish(
        TOPIC,
        serde_json::to_value(State {
            enabled,
            ready: status == Status::Ready,
            status,
            location,
            asset,
        })?,
    )?;
    Ok(())
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, false, Status::Disabled, None, None)
}
// Missing/invalid settings fail closed until the single settings writer has loaded.
fn enabled(hub: &Shared) -> bool {
    let state = hub.borrow().snapshot("settings.store.dashboard-extras");
    state.is_some_and(|s| {
        s["error"].is_null()
            && s["exists"].is_boolean()
            && s["data"]["weatherPhotos"].as_bool().unwrap_or(true)
    })
}
pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    run_inner(
        hub,
        paths,
        Rc::new(|p, l| Box::pin(fetch::load(p, l))),
        true,
    )
    .await
}
async fn run_inner(hub: Shared, paths: Paths, load: Loader, watch_session: bool) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    let mut notices = hub.borrow().observe();
    hub.borrow_mut().service("weather-photo", "running", 0)?;
    loop {
        let consent = enabled(&hub);
        publish(
            &hub,
            consent,
            if consent {
                Status::Idle
            } else {
                Status::Disabled
            },
            None,
            None,
        )?;
        while !*interest.borrow() {
            tokio::select! {
                result = interest.changed() => result?,
                result = notices.recv() => { let _ = result; let consent = enabled(&hub); publish(&hub, consent, if consent {Status::Idle} else {Status::Disabled}, None, None)?; }
            }
        }
        // Unknown session state is paused: no request before the first logind reading.
        let mut session = if watch_session {
            Some(behaviours::Session::spawn(Path::new("/proc/self/exe"))?)
        } else {
            None
        };
        let mut paused = watch_session;
        let mut attempted: Option<Location> = None;
        let mut pending: Option<LocalBoxFuture<'static, Result<fetch::Photo>>> = None;
        let mut current = None;
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        while *interest.borrow() {
            tokio::select! {
                biased;
                result = interest.changed() => {result?; if !*interest.borrow() {break;}},
                result = notices.recv() => {let _ = result;},
                state = async {session.as_mut().expect("session").next().await}, if session.is_some() => {paused = state?;},
                result = async {pending.as_mut().expect("pending").await}, if pending.is_some() => {
                    pending = None;
                    if enabled(&hub) && !paused {
                        match result {
                            Ok(photo) if Some(&photo.location) == current.as_ref() => {
                                publish(&hub, true, Status::Ready, current.clone(), Some(Asset {path:photo.path, checked:photo.checked, author:photo.author, license:photo.license, source:photo.source}))?;
                            }
                            _ => publish(&hub, true, Status::Unavailable, current.clone(), None)?,
                        }
                    }
                },
                _ = timer.tick() => {},
            }
            let location = super::weather::location(&hub);
            if location != current {
                pending = None;
                attempted = None;
                current = location;
            }
            let consent = enabled(&hub);
            if !consent || paused {
                // Dropping the future drops/kills its curl child. No completion can publish.
                pending = None;
                attempted = None;
                current = None;
                publish(
                    &hub,
                    consent,
                    if consent {
                        Status::Paused
                    } else {
                        Status::Disabled
                    },
                    None,
                    None,
                )?;
                continue;
            }
            let Some(location) = current.clone() else {
                publish(&hub, true, Status::WaitingLocation, None, None)?;
                continue;
            };
            // Like QML: one attempt per city/activation; no timer-driven request retries.
            if attempted.as_ref() != Some(&location) {
                attempted = Some(location.clone());
                publish(&hub, true, Status::Loading, Some(location.clone()), None)?;
                pending = Some(load(paths.clone(), location));
            }
        }
        // Cancel before returning to the unleased wait; do not keep a session watcher.
        drop(pending);
        drop(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    fn consent(hub: &Shared, value: bool) {
        hub.borrow_mut()
            .publish(
                "settings.store.dashboard-extras",
                json!({"exists":true,"error":null,"data":{"weatherPhotos":value}}),
            )
            .unwrap();
    }
    async fn until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("source transition within two seconds");
    }
    struct Cancelled(Rc<Cell<usize>>);
    impl Drop for Cancelled {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lease_consent_disconnect_and_readiness_while_fetch_hangs() -> Result<()> {
        tokio::task::LocalSet::new()
            .run_until(async {
                let home = tempfile::tempdir()?;
                let paths = Paths::new(home.path());
                let hub = super::super::hub::Hub::new(
                    "photo-test".into(),
                    false,
                    Duration::from_millis(1),
                )
                .shared();
                initial(&hub)?;
                hub.borrow_mut().publish(
                    super::super::weather::TOPIC,
                    json!({"current":{"location":"Harbour","latitude":60,"longitude":5}}),
                )?;
                consent(&hub, false);
                let calls = Rc::new(Cell::new(0));
                let cancelled = Rc::new(Cell::new(0));
                let (count, drops) = (calls.clone(), cancelled.clone());
                let loader: Loader = Rc::new(move |_, _| {
                    let (count, drops) = (count.clone(), drops.clone());
                    Box::pin(async move {
                        count.set(count.get() + 1);
                        let _guard = Cancelled(drops);
                        std::future::pending().await
                    })
                });
                let task = tokio::task::spawn_local(run_inner(hub.clone(), paths, loader, false));
                let (client, _) = hub.borrow_mut().add_client()?;
                hub.borrow_mut().subscribe(client, &[TOPIC.into()], false)?;
                until(|| hub.borrow().snapshot(TOPIC).unwrap()["status"] == "disabled").await;
                tokio::time::sleep(Duration::from_millis(25)).await;
                assert_eq!(
                    calls.get(),
                    0,
                    "consent off makes zero requests even with a lease"
                );
                consent(&hub, true);
                until(|| calls.get() == 1).await;
                // A fully hung fetch must leave the current-thread runtime and daemon hub responsive.
                let notify = home.path().join("notify.sock");
                let socket = tokio::net::UnixDatagram::bind(&notify)?;
                tokio::time::timeout(Duration::from_secs(2), async {
                    super::super::notify::send_to(notify.as_os_str(), "READY=1")
                        .await
                        .unwrap();
                    let mut packet = [0; 32];
                    let size = socket.recv(&mut packet).await.unwrap();
                    assert_eq!(&packet[..size], b"READY=1");
                    hub.borrow_mut()
                        .service("readiness-fixture", "running", 0)
                        .unwrap();
                    tokio::task::yield_now().await;
                    assert!(hub.borrow().status()["services"]
                        .get("readiness-fixture")
                        .is_some());
                })
                .await?;
                consent(&hub, false);
                until(|| cancelled.get() == 1).await;
                assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["status"], "disabled");
                consent(&hub, true);
                until(|| calls.get() == 2).await;
                let (second, _) = hub.borrow_mut().add_client()?;
                hub.borrow_mut().subscribe(second, &[TOPIC.into()], false)?;
                hub.borrow_mut().remove_client(client);
                tokio::time::sleep(Duration::from_millis(25)).await;
                assert_eq!(cancelled.get(), 1, "second consumer still holds the lease");
                assert_eq!(calls.get(), 2, "consumers share the fetch");
                hub.borrow_mut().remove_client(second);
                until(|| cancelled.get() == 2).await;
                assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["status"], "idle");
                tokio::time::sleep(Duration::from_millis(25)).await;
                assert_eq!(calls.get(), 2, "no work after disconnect");
                task.abort();
                let _ = task.await;
                Ok(())
            })
            .await
    }
}
