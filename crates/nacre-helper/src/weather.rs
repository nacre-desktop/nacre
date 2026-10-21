//! Bounded Open-Meteo weather worker used by the dashboard and lock widgets.
use crate::{paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use ureq::ResponseExt;

const LIMIT: u64 = 512_000;
const WMO: &[(i64, &str, u32)] = &[
    (0, "Clear", 113),
    (1, "Mostly clear", 116),
    (2, "Partly cloudy", 116),
    (3, "Overcast", 122),
    (45, "Fog", 143),
    (48, "Freezing fog", 260),
    (51, "Light drizzle", 266),
    (53, "Drizzle", 266),
    (55, "Heavy drizzle", 281),
    (56, "Freezing drizzle", 281),
    (57, "Freezing drizzle", 284),
    (61, "Light rain", 296),
    (63, "Rain", 302),
    (65, "Heavy rain", 308),
    (66, "Freezing rain", 311),
    (67, "Freezing rain", 314),
    (71, "Light snow", 326),
    (73, "Snow", 332),
    (75, "Heavy snow", 338),
    (77, "Snow grains", 350),
    (80, "Rain showers", 353),
    (81, "Rain showers", 356),
    (82, "Heavy showers", 359),
    (85, "Snow showers", 368),
    (86, "Heavy snow showers", 371),
    (95, "Thunderstorm", 200),
    (96, "Thunderstorm with hail", 389),
    (99, "Thunderstorm with hail", 389),
];

pub fn url(base: &str, query: &[(&str, &str)]) -> String {
    let encode = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    char::from(b).to_string()
                }
                b' ' => "+".into(),
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    format!(
        "{base}?{}",
        query
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    )
}
fn fetch(url: &str) -> Result<Value> {
    if !url.starts_with("https://") {
        return Err("weather endpoint must use HTTPS".into());
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .https_only(true)
        .build()
        .new_agent();
    let mut response = agent.get(url).header("User-Agent", "Nacre/1.0").call()?;
    if response.get_uri().scheme_str() != Some("https") {
        return Err("HTTPS redirect required".into());
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(LIMIT + 1)
        .read_to_vec()?;
    if bytes.len() as u64 > LIMIT {
        return Err("Weather response too large".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn normalize(
    f: &Value,
    location: &str,
    latitude: f64,
    longitude: f64,
    checked: f64,
) -> Result<Value> {
    let current = &f["current"];
    let daily = &f["daily"];
    let hourly = &f["hourly"];
    let wmo = current["weather_code"]
        .as_i64()
        .ok_or("weather response missing weather code")?;
    let (desc, icon) = WMO
        .iter()
        .find(|x| x.0 == wmo)
        .map(|x| (x.1, x.2))
        .unwrap_or(("Weather", 122));
    let stamp = current["time"]
        .as_str()
        .ok_or("weather response missing time")?;
    let mut hours = Vec::new();
    for (i, time) in hourly["time"]
        .as_array()
        .ok_or("weather response missing hours")?
        .iter()
        .enumerate()
    {
        if time.as_str().unwrap_or("") < stamp {
            continue;
        }
        hours.push(json!({"time":time,"temperature":hourly["temperature_2m"][i],"rain":hourly["precipitation_probability"][i],"code":hourly["weather_code"][i]}));
        if hours.len() == 24 {
            break;
        }
    }
    let mut days = Vec::new();
    for (i, date) in daily["time"]
        .as_array()
        .ok_or("weather response missing days")?
        .iter()
        .take(10)
        .enumerate()
    {
        days.push(json!({"date":date,"low":daily["temperature_2m_min"][i],"high":daily["temperature_2m_max"][i],"code":daily["weather_code"][i],"rain":daily["precipitation_probability_max"][i],"sunrise":daily["sunrise"][i],"sunset":daily["sunset"][i],"uv":daily["uv_index_max"][i]}));
    }
    Ok(
        json!({"location":location,"latitude":latitude,"longitude":longitude,"code":icon.to_string(),"wmo":wmo,"description":desc,"temperature":current["temperature_2m"],"feelsLike":current["apparent_temperature"],"high":days.first().and_then(|d|d.get("high")),"low":days.first().and_then(|d|d.get("low")),"humidity":current["relative_humidity_2m"],"wind":current["wind_speed_10m"],"direction":current["wind_direction_10m"],"pressure":current["surface_pressure"],"isDay":current["is_day"].as_i64().unwrap_or(0)!=0,"hours":hours,"days":days,"timezone":f["timezone"].as_str().unwrap_or(""),"localTime":stamp,"checked":checked,"stale":false,"error":"","provider":"Open-Meteo"}),
    )
}
fn cache(paths: &Paths) -> PathBuf {
    paths.cache.join("weather.json")
}
fn save(path: &PathBuf, data: &Value) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("cache has no parent")?)?;
    fs::set_permissions(
        path.parent().ok_or("cache has no parent")?,
        fs::Permissions::from_mode(0o700),
    )?;
    if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err("refusing linked weather cache".into());
    }
    let tmp = path.with_extension("tmp");
    if tmp.exists() && fs::symlink_metadata(&tmp)?.file_type().is_symlink() {
        return Err("refusing linked weather cache temporary".into());
    }
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(serde_json::to_string(data)?.as_bytes())?;
    f.sync_all()?;
    f.set_permissions(fs::Permissions::from_mode(0o600))?;
    fs::rename(tmp, path)?;
    Ok(())
}
fn stale(previous: Value) -> Value {
    let has_cache = previous.get("checked").is_some();
    let mut value = previous;
    if let Some(object) = value.as_object_mut() {
        object.insert("stale".into(), json!(true));
        object.insert(
            "error".into(),
            json!(if has_cache {
                "Weather unavailable; showing cached conditions"
            } else {
                "Weather unavailable. Set a city or try again."
            }),
        );
    } else {
        return json!({"stale":true,"error":"Weather unavailable. Set a city or try again."});
    }
    value
}
fn refresh(paths: &Paths) -> Value {
    let path = cache(paths);
    let previous = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(json!({}));
    let result = (|| -> Result<Value> {
        let settings = fs::read_to_string(paths.config.join("desktop.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(json!({}));
        let mut location = settings["weatherLocation"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_owned();
        if location.is_empty() {
            location = fetch("https://ipinfo.io/json")?["city"]
                .as_str()
                .unwrap_or("")
                .to_owned();
        }
        if location.is_empty() {
            return Err("Set a weather city in Settings".into());
        }
        let (lat, lon) = if previous["location"] == location
            && previous["latitude"].is_number()
            && previous["longitude"].is_number()
        {
            (
                previous["latitude"].as_f64().unwrap(),
                previous["longitude"].as_f64().unwrap(),
            )
        } else {
            let q = url(
                "https://geocoding-api.open-meteo.com/v1/search",
                &[
                    ("name", &location),
                    ("count", "1"),
                    ("language", "en"),
                    ("format", "json"),
                ],
            );
            let choice = fetch(&q)?["results"][0].clone();
            if choice.is_null() {
                return Err("Weather city not found".into());
            }
            (
                choice["latitude"].as_f64().ok_or("bad latitude")?,
                choice["longitude"].as_f64().ok_or("bad longitude")?,
            )
        };
        let lat_s = lat.to_string();
        let lon_s = lon.to_string();
        let fc=url("https://api.open-meteo.com/v1/forecast",&[("latitude",&lat_s),("longitude",&lon_s),("timezone","auto"),("forecast_days","10"),("current","temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code,surface_pressure,wind_speed_10m,wind_direction_10m"),("hourly","temperature_2m,precipitation_probability,weather_code"),("daily","weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max,precipitation_probability_max")]);
        let mut data = normalize(
            &fetch(&fc)?,
            &location,
            lat,
            lon,
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
        )?;
        let aq = url(
            "https://air-quality-api.open-meteo.com/v1/air-quality",
            &[
                ("latitude", &lat_s),
                ("longitude", &lon_s),
                ("current", "us_aqi"),
                ("timezone", "auto"),
            ],
        );
        data["aqi"] = fetch(&aq)
            .ok()
            .and_then(|v| v["current"]["us_aqi"].as_f64().map(|n| json!(n)))
            .unwrap_or(Value::Null);
        save(&path, &data)?;
        Ok(data)
    })();
    match result {
        Ok(v) => v,
        Err(_) => stale(previous),
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    if args.first().is_some_and(|a| a == "url") {
        let v: Value = serde_json::from_str(args.get(1).ok_or("missing URL arguments")?)?;
        let base = v["args"][0].as_str().ok_or("missing URL base")?;
        let q = v["kwargs"].as_object().ok_or("missing URL query")?;
        let pairs = q
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap_or("")))
            .collect::<Vec<_>>();
        println!("{}", url(base, &pairs));
        return Ok(0);
    }
    let paths = Paths::environment()?;
    let data = refresh(&paths);
    println!("{}", serde_json::to_string(&data)?);
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forecast_shape_retains_hour_filter_daily_range_and_checked_time() {
        let f = json!({"timezone":"Europe/Oslo","current":{"weather_code":0,"time":"2099-10-11T09:00","temperature_2m":28,"apparent_temperature":32,"relative_humidity_2m":50,"wind_speed_10m":8,"wind_direction_10m":90,"surface_pressure":1010,"is_day":1},"hourly":{"time":["2099-10-11T08:00","2099-10-11T09:00","2099-10-11T10:00"],"temperature_2m":[12,14,15],"precipitation_probability":[90,85,70],"weather_code":[61,61,63]},"daily":{"time":["2099-10-11"],"temperature_2m_min":[27],"temperature_2m_max":[34],"weather_code":[0],"precipitation_probability_max":[85],"sunrise":["07"],"sunset":["18"],"uv_index_max":[2]}});
        let d = normalize(&f, "Oslo", 59., 10., 100.).unwrap();
        assert_eq!(d["temperature"], 28);
        assert_eq!(d["feelsLike"], 32);
        assert_eq!(
            (d["high"].as_i64(), d["low"].as_i64()),
            (Some(34), Some(27))
        );
        assert_eq!(d["code"], "113");
        assert_eq!(d["description"], "Clear");
        assert_eq!(d["hours"].as_array().unwrap().len(), 2);
        assert_eq!(d["days"][0]["low"], 27);
        assert_eq!(d["checked"], 100.);
        assert_eq!(d["stale"], false);
        assert_eq!(d["timezone"], "Europe/Oslo");
    }
    #[test]
    fn failed_refresh_preserves_old_timestamp_and_weather_values() {
        let cached = json!({"checked":42,"temperature":10,"location":"Oslo"});
        let result = stale(cached);
        assert_eq!(result["checked"], 42);
        assert_eq!(result["temperature"], 10);
        assert_eq!(result["stale"], true);
    }
    #[test]
    fn private_weather_cache_is_written_mode_0600() {
        let root = std::env::temp_dir().join(format!("nacre-weather-{}", std::process::id()));
        let path = root.join("weather.json");
        let _ = fs::remove_dir_all(&root);
        save(&path, &json!({"temperature":2})).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn weather_url_matches_python_query_encoding() {
        assert_eq!(
            url(
                "https://example.invalid",
                &[("city", "Public City"), ("units", "metric")]
            ),
            "https://example.invalid?city=Public+City&units=metric"
        );
    }
}
