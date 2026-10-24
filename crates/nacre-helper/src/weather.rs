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

fn require_https(url: &str) -> Result<()> {
    if !url.starts_with("https://") {
        return Err("weather endpoint must use HTTPS".into());
    }
    Ok(())
}
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
    require_https(url)?;
    fetch_using(url, true, Duration::from_secs(8))
}
// Only the tests call this with a loopback HTTP endpoint. No environment override or
// insecure endpoint is accepted by the standalone/daemon production entry point.
fn fetch_using(url: &str, https: bool, timeout: Duration) -> Result<Value> {
    #[cfg(not(test))]
    require_https(url)?;
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .https_only(https);
    let agent = if https {
        config.build()
    } else {
        config.max_redirects(0).build()
    }
    .new_agent();
    let mut response = agent.get(url).header("User-Agent", "Nacre/1.0").call()?;
    if response.status().is_redirection()
        || (https && response.get_uri().scheme_str() != Some("https"))
    {
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
pub fn cached(paths: &Paths) -> Value {
    fs::read_to_string(cache(paths))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(Value::is_object)
        .unwrap_or(json!({}))
}
fn cache_lock(paths: &Paths) -> Result<Option<nix::fcntl::Flock<fs::File>>> {
    fs::create_dir_all(&paths.cache)?;
    fs::set_permissions(&paths.cache, fs::Permissions::from_mode(0o700))?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(paths.cache.join("weather.lock"))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    match nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Ok(guard) => Ok(Some(guard)),
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, error)) => Err(error.into()),
    }
}
pub fn refresh(paths: &Paths, settings: &Value) -> Value {
    refresh_with(paths, settings, fetch)
}

fn refresh_with(
    paths: &Paths,
    settings: &Value,
    mut fetch: impl FnMut(&str) -> Result<Value>,
) -> Value {
    let path = cache(paths);
    let previous = cached(paths);
    let location = settings["weatherLocation"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_owned();
    if location.is_empty() {
        return stale(previous);
    }
    // QML can still invoke the standalone command: share the lane across processes too.
    let _guard = match cache_lock(paths) {
        Ok(Some(guard)) => guard,
        _ => return stale(previous),
    };
    let result = (|| -> Result<Value> {
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
        println!("{}", serde_json::to_string(&url(base, &pairs))?);
        return Ok(0);
    }
    let paths = Paths::environment()?;
    let settings = fs::read_to_string(paths.config.join("desktop.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(json!({}));
    let data = refresh(&paths, &settings);
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
    // Security addition: HTTP and HTTPS-to-HTTP redirect paths are denied by construction.
    #[test]
    fn weather_endpoint_requires_https() {
        assert!(require_https("https://api.open-meteo.com/v1/forecast").is_ok());
        assert!(require_https("http://api.open-meteo.com/v1/forecast").is_err());
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn server(
        body: Vec<u8>,
        headers: &'static str,
        delay: Duration,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            thread::sleep(delay);
            let _ = write!(
                stream,
                "HTTP/1.1 {headers}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
        });
        (endpoint, handle)
    }
    fn fake(body: &[u8]) -> Result<Value> {
        let (endpoint, handle) = server(body.to_vec(), "200 OK", Duration::ZERO);
        let result = fetch_using(&endpoint, false, Duration::from_secs(2));
        handle.join().unwrap();
        result
    }
    #[test]
    fn http_ok_timeout_oversize_redirect_and_bad_json() {
        assert_eq!(fake(br#"{"ok":true}"#).unwrap()["ok"], true);
        assert!(fake(b"not JSON").is_err());
        let mut boundary = br#"{"ok":true}"#.to_vec();
        boundary.resize(LIMIT as usize, b' ');
        assert_eq!(fake(&boundary).unwrap()["ok"], true);
        boundary.push(b' ');
        let error = fake(&boundary).unwrap_err().to_string().to_lowercase();
        assert!(
            error.contains("limit") || error.contains("too large"),
            "{error}"
        );
        let (endpoint, handle) = server(
            Vec::new(),
            "302 Found\r\nLocation: http://127.0.0.1:1/forbidden",
            Duration::ZERO,
        );
        assert!(fetch_using(&endpoint, false, Duration::from_secs(2))
            .unwrap_err()
            .to_string()
            .to_lowercase()
            .contains("redirect"));
        handle.join().unwrap();
        let (endpoint, handle) = server(Vec::new(), "200 OK", Duration::from_millis(200));
        let started = std::time::Instant::now();
        assert!(fetch_using(&endpoint, false, Duration::from_millis(30)).is_err());
        assert!(started.elapsed() < Duration::from_millis(150));
        handle.join().unwrap();
        assert!(fetch("http://127.0.0.1:1/").is_err());
    }
    fn forecast() -> Value {
        json!({"timezone":"UTC","current":{"weather_code":0,"time":"2027-01-15T08:00","temperature_2m":12,"apparent_temperature":11,"is_day":1},"hourly":{"time":["2027-01-15T08:00"],"temperature_2m":[12],"precipitation_probability":[10],"weather_code":[0]},"daily":{"time":["2027-01-15"],"temperature_2m_min":[6],"temperature_2m_max":[14],"weather_code":[0],"precipitation_probability_max":[10],"sunrise":["07"],"sunset":["17"],"uv_index_max":[2]}})
    }
    #[test]
    fn consent_city_resolution_cache_and_stale_failure_share_the_worker() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path());
        fs::create_dir_all(&paths.cache).unwrap();
        fs::write(cache(&paths), "null").unwrap();
        assert!(
            cached(&paths).is_object(),
            "Malformed cache cannot break daemon startup"
        );
        for settings in [
            json!({}),
            json!({"weatherLocation":"  "}),
            json!({"weatherIpLookup":true}),
        ] {
            let result = refresh_with(&paths, &settings, |_| {
                panic!("Consent off must make zero requests")
            });
            assert_eq!(result["stale"], true);
        }
        assert!(
            !paths.cache.join("weather.lock").exists(),
            "Consent off cannot create coordination files"
        );
        let guard = cache_lock(&paths).unwrap().unwrap();
        let busy = refresh_with(&paths, &json!({"weatherLocation":"Harbour"}), |_| {
            panic!("Concurrent standalone/daemon fetch forbidden")
        });
        assert_eq!(busy["stale"], true);
        drop(guard);
        for legacy in [json!({}), json!({"weatherIpLookup":true})] {
            let mut calls = Vec::new();
            let data = refresh_with(
                &paths,
                &json!({"weatherLocation":"Harbour","weatherIpLookup":legacy.get("weatherIpLookup")}),
                |endpoint| {
                    calls.push(endpoint.to_owned());
                    let body = if endpoint.contains("geocoding") {
                        json!({"results":[{"latitude":60,"longitude":5}]})
                    } else if endpoint.contains("air-quality") {
                        json!({"current":{"us_aqi":20}})
                    } else {
                        forecast()
                    };
                    fake(serde_json::to_string(&body).unwrap().as_bytes())
                },
            );
            assert!(
                !calls.iter().any(|s| s.contains("ipinfo")),
                "Only the configured city is used"
            );
            assert_eq!(data["stale"], false);
            assert_eq!(data["aqi"], 20.0);
            assert_eq!(data["latitude"], 60.0);
            let cached: Value = serde_json::from_slice(&fs::read(cache(&paths)).unwrap()).unwrap();
            assert_eq!(cached, data);
        }
        let cached: Value = serde_json::from_slice(&fs::read(cache(&paths)).unwrap()).unwrap();
        let stale = refresh_with(&paths, &json!({"weatherLocation":"Harbour"}), |_| {
            Err("fake timeout".into())
        });
        assert_eq!(stale["checked"], cached["checked"]);
        assert_eq!(stale["temperature"], cached["temperature"]);
        assert_eq!(stale["stale"], true);
        let mut calls = Vec::new();
        let data = refresh_with(&paths, &json!({"weatherLocation":"Harbour"}), |endpoint| {
            calls.push(endpoint.to_owned());
            Ok(if endpoint.contains("air-quality") {
                json!({})
            } else {
                forecast()
            })
        });
        assert!(!calls.iter().any(|s| s.contains("ipinfo")));
        assert!(
            !calls.iter().any(|s| s.contains("geocoding")),
            "Same city reuses cached coordinates"
        );
        assert_eq!(data["stale"], false);
        assert!(data["aqi"].is_null(), "AQI is optional");
    }
}
