//! N0065: keeps the lock screen's prepared content fresh without QML.
//!
//! `LockWidgets.qml` fed `nacre-helper lock-prepare` its presentation; under Bead nothing did,
//! so the secure lock kept showing stale colours, wallpaper, media, weather and notices. This
//! behaviour builds the same presentation from daemon topics and calls the helper's own
//! `lock_prepare::prepare` in-process (no CLI), so the files, modes and privacy rules are the
//! helper's. Part of the N0039 `behaviours` bundle: QML stands down through `behavioursHeld`.
use super::{behaviours::changed, hub::Shared, menu};
use crate::{
    image_text::FontBook, lock_dashboard, lock_info::Info, lock_prepare, paths::Paths, Result,
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Map, Value};
use std::{path::PathBuf, time::Duration};

const TOPICS: [&str; 5] = [
    "wallpaper.active",
    "dashboard.player",
    "dashboard.weather",
    "notifications",
    "settings.store.desktop",
];
const SYSTEM: &str = "dashboard.system";

#[derive(Clone)]
pub struct Options {
    /// Quiet time after the last relevant change before preparing.
    pub debounce: Duration,
    /// Refresh of clock-derived content (greeting) and gauges when nothing changed.
    pub tick: Duration,
    /// Tests only: fixture fonts directory and helper assets instead of Fontconfig and the install.
    pub fixtures: Option<(PathBuf, PathBuf)>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(750),
            tick: Duration::from_secs(1800),
            fixtures: None,
        }
    }
}

/// Topic snapshots the presentation is built from (plain data, so it can leave the hub thread).
struct Inputs {
    wallpaper: Value,
    player: Value,
    weather: Value,
    notices: Value,
    desktop: Value,
    system: Value,
}
impl Inputs {
    fn read(hub: &Shared) -> Self {
        let hub = hub.borrow();
        let get = |topic: &str| hub.snapshot(topic).unwrap_or(Value::Null);
        let settings = get("settings.store.desktop");
        let desktop = if settings["exists"] == true && settings["error"].is_null() {
            settings["data"].clone()
        } else {
            json!({})
        };
        Self {
            wallpaper: get("wallpaper.active"),
            player: get("dashboard.player"),
            weather: get("dashboard.weather"),
            notices: get("notifications"),
            desktop,
            system: get(SYSTEM),
        }
    }
}

fn ordered(entities: &Value) -> impl Iterator<Item = &Value> {
    entities["order"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| entities["items"].get(id.as_str()?))
}
fn slug(text: &str) -> String {
    let mut out = String::new();
    for character in text.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').into()
}
fn degrees(value: &Value, fahrenheit: bool) -> Option<String> {
    let celsius = value.as_f64()?;
    // JavaScript `Math.round` rounds halves up.
    Some(if fahrenheit {
        format!("{}°F", (celsius * 9.0 / 5.0 + 32.0 + 0.5).floor())
    } else {
        format!("{}°C", (celsius + 0.5).floor())
    })
}
fn weather_icon(wmo: Option<u64>) -> &'static str {
    match wmo {
        Some(0 | 1) => "clear_day",
        Some(2) => "partly_cloudy_day",
        Some(3) => "cloud",
        Some(45 | 48) => "foggy",
        Some(51..=67 | 80..=82) => "rainy",
        Some(71..=77 | 85 | 86) => "weather_snowy",
        Some(95..=99) => "thunderstorm",
        _ => "cloud_off",
    }
}
fn file_url(path: &str) -> String {
    format!(
        "file://{}",
        path.replace('%', "%25")
            .replace('?', "%3F")
            .replace('#', "%23")
    )
}
fn fraction(used: Option<f64>, total: Option<f64>) -> Value {
    match (used, total) {
        (Some(used), Some(total)) if total > 0.0 => json!(used / total),
        _ => Value::Null,
    }
}

/// The QML `LockWidgets.snapshot()` shape, from daemon topics. With
/// `lockNotificationContents` off, summaries and bodies never enter the presentation.
fn presentation(inputs: &Inputs, messages: &Value, language: &str) -> Value {
    let desktop = &inputs.desktop;
    let flag = |key: &str, default: bool| desktop[key].as_bool().unwrap_or(default);
    let contents = flag("lockNotificationContents", false);
    let say = |id: &str, fallback: &str, args: Value| {
        nacre_i18n::translate(messages, language, id, fallback, &args)
    };
    let current = &inputs.wallpaper["current"];

    let health = &inputs.weather["health"];
    let reading = &inputs.weather["current"];
    let stale = health["stale"] == true || health["available"] != true;
    let fahrenheit = flag("weatherFahrenheit", false);
    let description = reading["description"].as_str().unwrap_or("");
    let temperature = degrees(&reading["temperature"], fahrenheit);
    let weather = json!({
        "location": reading["location"].as_str().unwrap_or(""),
        "icon": if description.is_empty() { "cloud_off" } else { weather_icon(reading["wmo"].as_u64()) },
        "detail": degrees(&reading["feels_like"], fahrenheit).map_or(String::new(), |value0|
            say("ui.weather.detail.feels-like-value0", "Feels like {value0}", json!({"value0": value0}))),
        "range": match (degrees(&reading["high"], fahrenheit), degrees(&reading["low"], fahrenheit)) {
            (Some(high), Some(low)) => say("weather.high-low", "High {high} · Low {low}", json!({"high": high, "low": low})),
            _ => String::new(),
        },
        "description": if description.is_empty() { String::new() } else {
            say(&format!("weather.condition.{}", slug(description)), description, Value::Null) },
        "temperature": match temperature {
            Some(text) if stale => say("weather.cached-temperature", "{temperature} (cached)", json!({"temperature": text})),
            Some(text) => text,
            None => String::new(),
        },
        "stale": stale,
        "error": if reading.is_null() {
            say("ui.weather.error.weather-unavailable-try-again-or-set-a-city-in-settings",
               "Weather unavailable; try again or set a city in Settings.", Value::Null)
        } else { String::new() },
    });

    let player = inputs.player["active"]
        .as_str()
        .and_then(|id| inputs.player["players"]["items"].get(id));
    let media = player.map_or(Value::Null, |player| {
        json!({
            "title": player["title"], "artist": player["artist"], "album": player["album"],
            "art": player["art_path"].as_str().map_or(Value::Null, |path| json!(file_url(path))),
            "playing": player["status"] == "playing",
            "canToggle": player["can_play_pause"],
        })
    });

    // Retained notices: kept history plus non-transient active ones, newest first, at most 64.
    let mut rows: Vec<&Value> = ordered(&inputs.notices["history"]).collect();
    rows.extend(ordered(&inputs.notices["active"]).filter(|row| row["transient"] != true));
    rows.sort_by_key(|row| std::cmp::Reverse(row["created_ms"].as_u64().unwrap_or(0)));
    let notifications: Vec<Value> = rows
        .iter()
        .take(64)
        .map(|row| {
            let time = Utc
                .timestamp_millis_opt(row["created_ms"].as_i64().unwrap_or(0))
                .single()
                .map_or(String::new(), |time| {
                    time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                });
            json!({
                "app": row["app_name"], "time": time, "icon": row["icon"].as_str().unwrap_or(""),
                "summary": if contents { row["summary"].clone() } else { json!("") },
                "body": if contents { row["body"].clone() } else { json!("") },
            })
        })
        .collect();
    let count = inputs.notices["history_count"]
        .as_u64()
        .map_or(notifications.len() as u64, |total| {
            total.max(notifications.len() as u64)
        });

    let system = &inputs.system;
    let disk = ordered(&system["disks"])
        .find(|disk| disk["path"] == "/")
        .or_else(|| ordered(&system["disks"]).next());
    let mut preferences = Map::new();
    for (key, default) in [
        ("lockMedia", true),
        ("lockWeather", true),
        ("lockNotifications", true),
        ("lockNotificationContents", false),
    ] {
        preferences.insert(key.into(), json!(flag(key, default)));
    }
    json!({
        "colors": current["palette"]["colours"],
        "wallpaper": current["path"],
        "system": {
            "cpu": system["cpu_percent"].as_f64().map_or(Value::Null, |p| json!(p / 100.0)),
            "memory": fraction(system["memory"]["used"].as_f64(), system["memory"]["total"].as_f64()),
            "storage": fraction(disk.and_then(|d| d["used"].as_f64()), disk.and_then(|d| d["total"].as_f64())),
            "temperature": Value::Null,
        },
        "preferences": preferences,
        "weather": weather,
        "media": media,
        "notifications": notifications,
        "count": count,
    })
}

/// Build and prepare once; `Ok(None)` when nothing changed since `last`.
fn prepare(
    paths: &Paths,
    options: &Options,
    inputs: &Inputs,
    last: Option<&str>,
) -> Result<Option<String>> {
    let language = menu::message_language(&inputs.desktop);
    let messages = menu::translations(paths, &language).unwrap_or(json!({}));
    let data = presentation(inputs, &messages, &language);
    // The greeting follows the hour; everything else is in the presentation itself.
    let key = format!("{}|{}", chrono::Local::now().format("%Y%m%d%H"), data);
    if last == Some(key.as_str()) {
        return Ok(None);
    }
    let mut info = Info::new(paths.clone());
    let mut context = lock_dashboard::Context::default();
    let mut assets = paths.assets();
    if let Some((fonts, tools)) = &options.fixtures {
        info.font_path = Some(fonts.join("FixtureSans.ttf"));
        context.fonts = FontBook::with_overrides(
            [
                ("IBM Plex Sans".into(), fonts.join("FixtureSans.ttf")),
                (
                    "Material Symbols Rounded".into(),
                    fonts.join("FixtureSymbols.ttf"),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assets = tools.clone();
    }
    lock_prepare::prepare(&data, &mut info, &assets, &mut context)?;
    Ok(Some(key))
}

pub async fn lock_content(hub: Shared, paths: Paths, options: Options) -> Result<()> {
    let mut notices = hub.borrow().observe();
    let mut last: Option<String> = None;
    loop {
        // Restart the quiet period on every relevant change (the first pass is the startup run).
        while let Ok(result) =
            tokio::time::timeout(options.debounce, changed(&mut notices, &TOPICS)).await
        {
            result?;
        }
        let (inputs, work_paths, previous) = (Inputs::read(&hub), paths.clone(), last.clone());
        let work_options = options.clone();
        match tokio::task::spawn_blocking(move || {
            prepare(&work_paths, &work_options, &inputs, previous.as_deref())
        })
        .await
        {
            Ok(Ok(Some(key))) => last = Some(key),
            Ok(Ok(None)) => {}
            // Retry on the next change or tick; the previous prepared files stay in place.
            Ok(Err(error)) => {
                last = None;
                tracing::warn!(%error, "Lock content could not be prepared");
            }
            Err(_) => return Err("Lock preparation task failed".into()),
        }
        // A change that arrived during the run is already queued and wakes this at once.
        tokio::select! {
            result = changed(&mut notices, &TOPICS) => result?,
            _ = tokio::time::sleep(options.tick) => {}
        }
    }
}

pub async fn run_lock_content(hub: Shared, paths: Paths) -> Result<()> {
    lock_content(hub, paths, Options::default()).await
}

#[cfg(test)]
mod tests;
