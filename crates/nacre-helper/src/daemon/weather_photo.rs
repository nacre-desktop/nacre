//! City photos are leased by topic subscription, with cancellable async network work.
mod fetch;
use super::{
    hub::{Notice, Shared},
    session_policy,
};
use crate::{paths::Paths, Result};
use futures_util::future::LocalBoxFuture;
pub use nacre_protocol::weather_photo::TOPIC;
use nacre_protocol::weather_photo::{Asset, Location, State, Status};
use std::rc::Rc;

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
/// Notices that can change this producer's inputs; the rest are skipped
/// before touching any snapshot.
fn relevant(
    notice: &std::result::Result<Notice, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    match notice {
        Ok(Notice::Changed { topic, .. }) => {
            topic == super::weather::TOPIC
                || topic == "settings.store.dashboard-extras"
                || topic == nacre_protocol::session::STATE_TOPIC
        }
        Ok(_) => false,
        Err(_) => true,
    }
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
    run_inner(hub, paths, Rc::new(|p, l| Box::pin(fetch::load(p, l)))).await
}
async fn run_inner(hub: Shared, paths: Paths, load: Loader) -> Result<()> {
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
                result = notices.recv() => { if !relevant(&result) { continue; } let consent = enabled(&hub); publish(&hub, consent, if consent {Status::Idle} else {Status::Disabled}, None, None)?; }
            }
        }
        // Unknown session state is paused: no request before the first logind reading.
        let paused = || session_policy::locked_or_asleep(&hub);
        let mut attempted: Option<Location> = None;
        let mut pending: Option<LocalBoxFuture<'static, Result<fetch::Photo>>> = None;
        let mut current = None;
        let mut entered = false;
        while *interest.borrow() {
            tokio::select! {
                biased;
                result = interest.changed() => {result?; if !*interest.borrow() {break;}},
                // The first pass reads the leased inputs at once, without waiting for a notice.
                _ = std::future::ready(()), if !entered => {entered = true;},
                result = notices.recv() => { if !relevant(&result) { continue; } },
                result = async {pending.as_mut().expect("pending").await}, if pending.is_some() => {
                    pending = None;
                    if enabled(&hub) && !paused() {
                        match result {
                            Ok(photo) if Some(&photo.location) == current.as_ref() => {
                                publish(&hub, true, Status::Ready, current.clone(), Some(Asset {path:photo.path, checked:photo.checked, author:photo.author, license:photo.license, source:photo.source}))?;
                            }
                            _ => publish(&hub, true, Status::Unavailable, current.clone(), None)?,
                        }
                    }
                },
            }
            let location = super::weather::location(&hub);
            if location != current {
                pending = None;
                attempted = None;
                current = location;
            }
            let consent = enabled(&hub);
            if !consent || paused() {
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
        // Cancel before returning to the unleased wait.
        drop(pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;
    use std::time::Duration;

    fn consent(hub: &Shared, value: bool) {
        hub.borrow_mut()
            .publish(
                "settings.store.dashboard-extras",
                json!({"exists":true,"error":null,"data":{"weatherPhotos":value}}),
            )
            .unwrap();
    }
    fn session(hub: &Shared, revision: u64, locked: bool) {
        hub.borrow_mut()
            .publish(
                nacre_protocol::session::STATE_TOPIC,
                json!({"locked":locked,"sleeping":false,"preparing_sleep":false,"revision":revision}),
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
                session(&hub, 1, false);
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
                let task = tokio::task::spawn_local(run_inner(hub.clone(), paths, loader));
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

    #[tokio::test(flavor = "current_thread")]
    async fn a_new_weather_location_restarts_the_photo_without_a_timer() -> Result<()> {
        tokio::task::LocalSet::new()
            .run_until(async {
                let home = tempfile::tempdir()?;
                let hub = super::super::hub::Hub::new(
                    "photo-loc".into(),
                    false,
                    Duration::from_millis(1),
                )
                .shared();
                initial(&hub)?;
                let place = |name: &str| {
                    hub.borrow_mut()
                        .publish(
                            super::super::weather::TOPIC,
                            json!({"current":{"location":name,"latitude":60,"longitude":5}}),
                        )
                        .unwrap()
                };
                place("Harbour");
                consent(&hub, true);
                // Unknown session state is paused, like a locked session.
                super::super::session_policy::initial_state(&hub)?;
                let calls = Rc::new(Cell::new(0));
                let count = calls.clone();
                let loader: Loader = Rc::new(move |_, _| {
                    count.set(count.get() + 1);
                    Box::pin(std::future::pending())
                });
                let task = tokio::task::spawn_local(run_inner(
                    hub.clone(),
                    Paths::new(home.path()),
                    loader,
                ));
                let (client, _) = hub.borrow_mut().add_client()?;
                hub.borrow_mut().subscribe(client, &[TOPIC.into()], false)?;
                until(|| hub.borrow().snapshot(TOPIC).unwrap()["status"] == "paused").await;
                assert_eq!(calls.get(), 0);
                session(&hub, 1, false);
                until(|| calls.get() == 1).await;
                place("Fjord");
                until(|| calls.get() == 2).await;
                session(&hub, 2, true);
                until(|| hub.borrow().snapshot(TOPIC).unwrap()["status"] == "paused").await;
                session(&hub, 3, false);
                until(|| calls.get() == 3).await;
                task.abort();
                let _ = task.await;
                Ok(())
            })
            .await
    }
}
