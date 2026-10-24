//! Shared standalone weather worker, scheduled outside the daemon readiness path.
use super::hub::{failure, Shared};
use crate::{paths::Paths, Result};
use nacre_protocol::{
    core_services::{CoreSnapshot, Health, WeatherState},
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

pub const TOPIC: &str = "dashboard.weather";
const INTERVAL: Duration = Duration::from_secs(1800);
static LANE: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

pub fn methods() -> BTreeMap<String, Method> {
    BTreeMap::from([(
        "dashboard.weather.refresh".into(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 10000,
            params: json!({"type":"object","additionalProperties":false}),
            result: serde_json::from_str(include_str!(
                "../../../../protocol/schema/core-service-result.json"
            ))
            .expect("core result schema"),
        },
    )])
}
fn publish(hub: &Shared, state: &WeatherState) -> Result<()> {
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(
        hub,
        &WeatherState {
            health: Health {
                available: false,
                stale: false,
                error: None,
            },
            refreshing: false,
            checked: None,
            provider: "Open-Meteo".into(),
            current: None,
        },
    )
}

// Preserve the worker's Celsius/kph/hPa values, translating only the reserved wire names
// and stable entity maps. Both shells therefore consume the same normalization.
fn state(data: Value) -> Result<WeatherState> {
    let mut current = data.clone();
    let object = current.as_object_mut().ok_or("Invalid weather reading")?;
    for (old, new) in [
        ("feelsLike", "feels_like"),
        ("localTime", "local_time"),
        ("isDay", "is_day"),
    ] {
        object.insert(new.into(), data[old].clone());
    }
    for (field, prefix) in [("hours", "hour"), ("days", "day")] {
        let mut items = BTreeMap::new();
        let mut order = Vec::new();
        for (index, value) in data[field].as_array().into_iter().flatten().enumerate() {
            let id = format!("{prefix}-{index}");
            let mut value = value.clone();
            value["wmo"] = value["code"].clone();
            items.insert(id.clone(), value);
            order.push(id);
        }
        object.insert(field.into(), json!({"items":items,"order":order}));
    }
    let has_reading = data["checked"].as_f64().is_some_and(|n| n > 0.0)
        && data["temperature"].as_f64().is_some()
        && data["location"].as_str().is_some_and(|s| !s.is_empty());
    if data.get("checked").is_some() && !has_reading {
        return Err("Invalid checked weather reading".into());
    }
    let stale = data["stale"].as_bool().unwrap_or(true);
    let error = data["error"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| failure(ErrorCode::Unavailable, s, true));
    let result = WeatherState {
        health: Health {
            available: has_reading,
            stale,
            error,
        },
        refreshing: false,
        checked: data["checked"].as_f64(),
        provider: "Open-Meteo".into(),
        current: if has_reading {
            Some(serde_json::from_value(current)?)
        } else {
            None
        },
    };
    if !CoreSnapshot::Weather(Box::new(result.clone())).valid() {
        return Err("Invalid weather topic reading".into());
    }
    Ok(result)
}
fn settings(hub: &Shared) -> Option<Value> {
    hub.borrow()
        .snapshot("settings.store.desktop")
        .filter(|s| s["error"].is_null())
        .map(|s| s["data"].clone())
}
fn start(hub: Shared, paths: Paths) -> std::result::Result<(), Failure> {
    let settings = settings(&hub)
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Weather settings unavailable", true))?;
    let guard = LANE
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
        .try_lock_owned()
        .map_err(|_| {
            failure(
                ErrorCode::Busy,
                "A weather refresh is already running",
                true,
            )
        })?;
    let mut previous: WeatherState =
        serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap_or(Value::Null))
            .map_err(|_| failure(ErrorCode::Unavailable, "Weather unavailable", true))?;
    previous.refreshing = true;
    publish(&hub, &previous)
        .map_err(|_| failure(ErrorCode::Internal, "Cannot publish weather", false))?;
    tokio::task::spawn_local(async move {
        let requested = settings.clone();
        let result = crate::process::offload(move || {
            // Linux nice priority is per thread; leave the daemon's readiness thread alone.
            let data = (|| -> Result<Value> {
                if rustix::process::getpriority_process(None)? < 10 {
                    rustix::process::setpriority_process(None, 10)?;
                }
                Ok(crate::weather::refresh(&paths, &settings))
            })();
            (data, guard)
        })
        .await;
        let (data, _guard) = match result {
            Ok((data, guard)) => (data.ok(), Some(guard)),
            Err(_) => (None, None),
        };
        // A changed city/consent must never publish an obsolete in-flight result.
        if weather_settings(&requested) != weather_settings(&settings_snapshot(&hub)) {
            // Settings failure may already have marked the live snapshot stale.
            // Keep that transition instead of restoring the pre-request error state.
            if let Some(current) = hub
                .borrow()
                .snapshot(TOPIC)
                .and_then(|v| serde_json::from_value(v).ok())
            {
                previous = current;
            }
            previous.refreshing = false;
            previous.health.stale = true;
            let _ = publish(&hub, &previous);
            return;
        }
        let next = complete(previous, data);
        if publish(&hub, &next).is_err() {
            tracing::warn!("Cannot publish weather result");
        }
    });
    Ok(())
}
fn complete(mut previous: WeatherState, data: Option<Value>) -> WeatherState {
    if let Some(mut next) = data.and_then(|data| state(data).ok()) {
        // The cache may have been deleted while the daemon still has a good reading.
        // QML also retains its in-memory reading when a failed worker returns no data.
        if next.current.is_none() && previous.current.is_some() {
            next.current = previous.current;
            next.checked = previous.checked;
            next.health.available = true;
            next.health.stale = true;
        }
        return next;
    }
    previous.refreshing = false;
    previous.health.stale = true;
    previous.health.error = Some(failure(
        ErrorCode::Unavailable,
        "Weather unavailable; showing cached conditions",
        true,
    ));
    previous
}

fn settings_snapshot(hub: &Shared) -> Value {
    settings(hub).unwrap_or(Value::Null)
}
fn weather_settings(settings: &Value) -> Value {
    json!([
        settings.is_object(),
        settings["weatherLocation"],
        settings["weatherFahrenheit"],
        settings["weatherIpLookup"]
    ])
}
pub(super) fn location(hub: &Shared) -> Option<nacre_protocol::weather_photo::Location> {
    let data = hub.borrow().snapshot(TOPIC)?;
    let current = &data["current"];
    let location = nacre_protocol::weather_photo::Location {
        city: current["location"]
            .as_str()?
            .trim()
            .chars()
            .take(100)
            .collect(),
        latitude: current["latitude"].as_f64()?,
        longitude: current["longitude"].as_f64()?,
    };
    (!location.city.is_empty()
        && !location.city.chars().any(char::is_control)
        && location.latitude.is_finite()
        && (-90.0..=90.0).contains(&location.latitude)
        && location.longitude.is_finite()
        && (-180.0..=180.0).contains(&location.longitude))
    .then_some(location)
}

pub fn dispatch(hub: Shared, params: Value) -> std::result::Result<Value, Failure> {
    if params.as_object().is_none_or(|p| !p.is_empty()) {
        return Err(failure(
            ErrorCode::Validation,
            "Expected an empty refresh request",
            false,
        ));
    }
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Weather requests disabled in safe mode",
            false,
        ));
    }
    let paths = Paths::environment()
        .map_err(|_| failure(ErrorCode::Unavailable, "Weather paths unavailable", true))?;
    start(hub, paths)?;
    Ok(json!({"changed":false}))
}
pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    // Display the existing cache while the first network refresh is in flight.
    let cache_paths = paths.clone();
    if let Ok(mut cached) =
        crate::process::offload(move || crate::weather::cached(&cache_paths)).await
    {
        let age = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs_f64()
            - cached["checked"].as_f64().unwrap_or(0.0);
        cached["stale"] = json!(cached["stale"] == true || age > INTERVAL.as_secs_f64());
        let current = hub.borrow().snapshot(TOPIC).unwrap_or(Value::Null);
        if current["refreshing"] != true && current["checked"].is_null() {
            if let Ok(cached) = state(cached) {
                publish(&hub, &cached)?;
            }
        }
    }
    let mut notices = hub.borrow().observe();
    let mut timer = tokio::time::interval(INTERVAL);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last = Value::Null;
    let mut pending = false;
    loop {
        tokio::select! {
            _ = timer.tick() => pending = true,
            notice = notices.recv() => {
                if notice.is_err() {pending = true;}
            },
        }
        let current = weather_settings(&settings_snapshot(&hub));
        if current != last {
            last = current;
            pending = true;
        }
        if pending {
            match start(hub.clone(), paths.clone()) {
                Ok(()) => pending = false,
                Err(e) if e.code == ErrorCode::Busy => {}
                Err(error) => {
                    pending = false;
                    let previous: WeatherState = serde_json::from_value(
                        hub.borrow().snapshot(TOPIC).unwrap_or(Value::Null),
                    )?;
                    let refreshing = previous.refreshing;
                    let mut unavailable = complete(previous, None);
                    unavailable.refreshing = refreshing;
                    unavailable.health.error = Some(error);
                    publish(&hub, &unavailable)?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
