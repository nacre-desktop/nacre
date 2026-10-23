//! N0049: the system clock for Settings. The `time` topic mirrors `org.freedesktop.timedate1`
//! (time zone, NTP) and `time.zones|set_timezone|set_ntp` call the same service `timedatectl`
//! (the QML `TimezoneSettings`) calls. Writes are privileged: they are sent with interactive
//! authorisation allowed, so polkit asks the session's authentication agent and the call waits
//! for the answer (up to the method deadline). No process is spawned and nothing is stored.
use super::hub::{failure, Hub, Shared};
use crate::Result;
use futures_util::StreamExt;
use nacre_protocol::{core_services::Health, system_settings::*, ErrorCode, Failure, Method};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, time::Duration};
use zbus::{fdo::PropertiesProxy, zvariant::OwnedValue, Connection};

const DEST: &str = "org.freedesktop.timedate1";
const PATH: &str = "/org/freedesktop/timedate1";
const QUICK: Duration = Duration::from_secs(5);
const MAX_ZONES: usize = 2048;

thread_local! {
    static CONNECTION: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

pub fn handles(method: &str) -> bool {
    matches!(method, "time.zones" | "time.set_timezone" | "time.set_ntp")
}
/// Advertised while timedated has answered a read.
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let up = !hub.safe
        && hub
            .snapshot_ref(TIME_TOPIC)
            .is_some_and(|state| state["health"]["available"] == true);
    methods()
        .into_iter()
        .filter(|(name, _)| up && handles(name))
        .collect()
}

fn unavailable() -> Failure {
    failure(
        ErrorCode::Unavailable,
        "Time settings are unavailable",
        true,
    )
    .dispatch_state(false)
}
fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid time request", false).dispatch_state(false)
}
/// polkit refusal, a dismissed prompt and a missing agent all arrive as `AccessDenied`.
fn map_error(error: &zbus::Error) -> Failure {
    let name = match error {
        zbus::Error::MethodError(name, _, _) => name.as_str(),
        _ => "",
    };
    match name {
        "org.freedesktop.DBus.Error.AccessDenied"
        | "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired" => failure(
            ErrorCode::PermissionDenied,
            "Administrator authentication was not given",
            true,
        )
        .dispatch_state(false),
        "org.freedesktop.DBus.Error.InvalidArgs" => invalid(),
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => unavailable(),
        _ => failure(ErrorCode::Io, "The clock service failed", false).dispatch_state(true),
    }
}

fn health(available: bool) -> Health {
    Health {
        available,
        stale: false,
        error: (!available).then(unavailable),
    }
}
async fn read(connection: &Connection) -> zbus::Result<TimeState> {
    let properties = PropertiesProxy::builder(connection)
        .destination(DEST)?
        .path(PATH)?
        .build()
        .await?;
    let all = tokio::time::timeout(
        QUICK,
        properties.get_all(zbus::names::InterfaceName::from_static_str_unchecked(DEST)),
    )
    .await
    .map_err(|_| {
        zbus::Error::InputOutput(std::sync::Arc::new(std::io::ErrorKind::TimedOut.into()))
    })??;
    let get = |name: &str| -> zbus::Result<OwnedValue> {
        all.get(name)
            .and_then(|value| value.try_clone().ok())
            .ok_or_else(|| zbus::Error::Failure(format!("missing {name}")))
    };
    let timezone = String::try_from(get("Timezone")?)?;
    Ok(TimeState {
        health: health(true),
        timezone: timezone
            .chars()
            .filter(|c| !c.is_control())
            .take(100)
            .collect(),
        ntp: Some(TimeNtp {
            supported: bool::try_from(get("CanNTP")?)?,
            enabled: bool::try_from(get("NTP")?)?,
        }),
    })
}
fn publish(hub: &Shared, state: &TimeState) -> Result<()> {
    debug_assert!(state.validate().is_ok());
    hub.borrow_mut()
        .publish(TIME_TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
/// Read and publish; a failed read keeps the last value and marks it unavailable.
async fn refresh(hub: &Shared, connection: &Connection) -> Option<TimeState> {
    match read(connection).await {
        Ok(state) => {
            publish(hub, &state).ok()?;
            Some(state)
        }
        Err(_) => {
            let mut state = hub
                .borrow()
                .snapshot(TIME_TOPIC)
                .and_then(|data| serde_json::from_value::<TimeState>(data).ok())
                .unwrap_or(TimeState {
                    health: health(false),
                    timezone: String::new(),
                    ntp: None,
                });
            state.health = health(false);
            publish(hub, &state).ok()?;
            None
        }
    }
}

pub fn initial(hub: &Shared) -> Result<()> {
    if hub.borrow().snapshot(TIME_TOPIC).is_none() {
        publish(
            hub,
            &TimeState {
                health: health(false),
                timezone: String::new(),
                ntp: None,
            },
        )?;
    }
    Ok(())
}

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Time settings are disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    let connection = CONNECTION
        .with(|slot| slot.borrow().clone())
        .ok_or_else(unavailable)?;
    let current = refresh(&hub, &connection).await.ok_or_else(unavailable)?;
    let call = |name: &'static str, body: zbus::Result<zbus::Message>| {
        body.map_err(|error| {
            tracing::warn!(method = name, "Clock service call failed");
            map_error(&error)
        })
    };
    let reply = match method {
        "time.zones" => {
            serde_json::from_value::<nacre_protocol::pill_controls::PillEmpty>(params)
                .map_err(|_| invalid())?;
            let message = call(
                "ListTimezones",
                connection
                    .call_method(Some(DEST), PATH, Some(DEST), "ListTimezones", &())
                    .await,
            )?;
            let mut zones: Vec<String> = message.body().deserialize().map_err(|_| unavailable())?;
            zones.retain(|zone| zone_valid(zone));
            zones.truncate(MAX_ZONES);
            return Ok(json!({"zones":zones}));
        }
        "time.set_timezone" => {
            let request: TimeSetTimezone = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            if request.zone == current.timezone {
                return Ok(json!({"changed":false}));
            }
            call(
                "SetTimezone",
                connection
                    .call_method(
                        Some(DEST),
                        PATH,
                        Some(DEST),
                        "SetTimezone",
                        &(request.zone.as_str(), true),
                    )
                    .await,
            )?;
            json!({"changed":true})
        }
        "time.set_ntp" => {
            let request: nacre_protocol::control_center::EnabledSet =
                serde_json::from_value(params).map_err(|_| invalid())?;
            let ntp = current.ntp.ok_or_else(unavailable)?;
            if !ntp.supported {
                return Err(unavailable());
            }
            if request.enabled == ntp.enabled {
                return Ok(json!({"changed":false}));
            }
            call(
                "SetNTP",
                connection
                    .call_method(
                        Some(DEST),
                        PATH,
                        Some(DEST),
                        "SetNTP",
                        &(request.enabled, true),
                    )
                    .await,
            )?;
            json!({"changed":true})
        }
        _ => return Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    };
    refresh(&hub, &connection).await;
    Ok(reply)
}

pub async fn run(hub: Shared) -> Result<()> {
    run_on(hub, zbus::connection::Builder::system()?).await
}
async fn run_on(hub: Shared, builder: zbus::connection::Builder<'static>) -> Result<()> {
    let connection = builder
        .method_timeout(Duration::from_millis(u64::from(PROMPT_TIMEOUT_MS)))
        .build()
        .await?;
    CONNECTION.with(|slot| *slot.borrow_mut() = Some(connection.clone()));
    hub.borrow_mut().service("time", "running", 0)?;
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut owners = bus
        .receive_name_owner_changed_with_args(&[(0, DEST)])
        .await?;
    let properties = PropertiesProxy::builder(&connection)
        .destination(DEST)?
        .path(PATH)?
        .build()
        .await?;
    let mut changes = properties.receive_properties_changed().await?;
    // One read at start. timedated exits when idle, so losing its name is not a reason to read
    // (that would restart it forever); only its arrival or a property change is.
    refresh(&hub, &connection).await;
    loop {
        tokio::select! {
            Some(_) = changes.next() => { refresh(&hub, &connection).await; }
            Some(change) = owners.next() => {
                if change.args().is_ok_and(|args| args.new_owner().as_ref().is_some()) {
                    refresh(&hub, &connection).await;
                }
            }
            else => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
