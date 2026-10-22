//! Short-lived GeoClue fixes; no coordinates are persisted.
//! The offline resolver uses zone.tab representative points and zone-derived
//! labels because this crate has no safe GWeather Rust binding. This can differ
//! from GWeather's nearest populated-city choice while retaining boundary checks.
use crate::{i18n::tr, Result};
use futures_lite::StreamExt;
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Duration};

pub fn validate_fix(fix: &Value, now: f64) -> Result<Value> {
    let latitude = fix["latitude"].as_f64().ok_or("Invalid device location")?;
    let longitude = fix["longitude"].as_f64().ok_or("Invalid device location")?;
    let accuracy = fix["accuracy"].as_f64().ok_or("Invalid device location")?;
    let timestamp = fix["timestamp"].as_f64().ok_or("Invalid device location")?;
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !accuracy.is_finite()
        || !timestamp.is_finite()
    {
        return Err(tr(
            "helper.device-location.invalid-device-location",
            "Invalid device location",
        )
        .into());
    }
    if !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
        || !(0.0..=5000.0).contains(&accuracy)
    {
        return Err(tr(
            "helper.device-location.device-location-is-too-imprecise",
            "Device location is too imprecise",
        )
        .into());
    }
    if !(-60.0..=300.0).contains(&(now - timestamp)) {
        return Err(tr(
            "helper.device-location.device-location-is-stale",
            "Device location is stale",
        )
        .into());
    }
    if fix["description"]
        .as_str()
        .unwrap_or("")
        .to_lowercase()
        .contains("geoip")
    {
        return Err(tr(
            "helper.device-location.ip-only-location-is-not-trusted",
            "IP-only location is not trusted",
        )
        .into());
    }
    Ok(fix.clone())
}

fn coordinate(text: &str) -> Option<f64> {
    let (body, hemisphere) = text.split_at(text.len().checked_sub(1)?);
    let (degree, rest) = body.split_at(body.len().checked_sub(4)?);
    let minutes = rest.get(..2)?.parse::<f64>().ok()?;
    let seconds = rest.get(2..)?.parse::<f64>().ok()?;
    let value = degree.parse::<f64>().ok()? + minutes / 60.0 + seconds / 3600.0;
    Some(if hemisphere == "S" || hemisphere == "W" {
        -value
    } else {
        value
    })
}
fn nearest_zone(latitude: f64, longitude: f64) -> Result<String> {
    let content = std::fs::read_to_string("/usr/share/zoneinfo/zone.tab")?;
    let mut nearest: Option<(f64, String)> = None;
    for row in content.lines().filter(|row| !row.starts_with('#')) {
        let fields = row.split('\t').collect::<Vec<_>>();
        if fields.len() < 3 {
            continue;
        }
        let (Some(lat), Some(lon)) = (coordinate(fields[1]), coordinate(fields[2])) else {
            continue;
        };
        let dx = (lon - longitude) * latitude.to_radians().cos();
        let dy = lat - latitude;
        let distance = dx * dx + dy * dy;
        if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
            nearest = Some((distance, fields[2].to_owned()));
        }
    }
    let (_, zone) = nearest.ok_or("No reliable local timezone")?;
    Ok(zone)
}
fn zone_for_fix_with(
    fix: &Value,
    now: f64,
    mut lookup: impl FnMut(f64, f64) -> Result<String>,
) -> Result<Value> {
    validate_fix(fix, now)?;
    let lat = fix["latitude"].as_f64().ok_or("Invalid device location")?;
    let lon = fix["longitude"].as_f64().ok_or("Invalid device location")?;
    let radius = fix["accuracy"].as_f64().unwrap_or(0.0).max(5000.0);
    let dlat = radius / 111000.0;
    let dlon = dlat / lat.to_radians().cos().abs().max(0.1);
    let mut zones = BTreeSet::new();
    for (north, east) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let y = (lat + north * dlat).clamp(-90.0, 90.0);
        let x = (lon + east * dlon + 180.0).rem_euclid(360.0) - 180.0;
        zones.insert(lookup(y, x)?);
    }
    if zones.len() != 1 {
        return Err("Location is near an uncertain timezone boundary".into());
    }
    let timezone = zones.into_iter().next().unwrap();
    if timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err("No reliable local timezone".into());
    }
    Ok(
        json!({"timezone":timezone,"city":timezone.rsplit('/').next().unwrap_or(""),"accuracyMeters":fix["accuracy"].as_f64().unwrap_or(0.0).round(),"source":"Device location (GeoClue)"}),
    )
}
fn zone_for_fix(fix: &Value) -> Result<Value> {
    zone_for_fix_with(fix, chrono::Utc::now().timestamp() as f64, nearest_zone)
}
async fn detect_async() -> Result<Value> {
    use zbus::{zvariant::OwnedObjectPath, Connection, Proxy};
    let connection = Connection::system().await?;
    let manager = Proxy::new(
        &connection,
        "org.freedesktop.GeoClue2",
        "/org/freedesktop/GeoClue2/Manager",
        "org.freedesktop.GeoClue2.Manager",
    )
    .await?;
    let client: OwnedObjectPath = manager.call("CreateClient", &()).await?;
    let proxy = Proxy::new(
        &connection,
        "org.freedesktop.GeoClue2",
        client.clone(),
        "org.freedesktop.GeoClue2.Client",
    )
    .await?;
    proxy.set_property("DesktopId", "nacre-timezone").await?;
    proxy.set_property("RequestedAccuracyLevel", 5u32).await?;
    let mut updates = proxy.receive_signal("LocationUpdated").await?;
    let _: () = proxy.call("Start", &()).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    let mut location: Option<OwnedObjectPath> = None;
    loop {
        let path: OwnedObjectPath = proxy.get_property("Location").await?;
        if path.as_str() != "/" {
            location = Some(path);
            break;
        }
        if tokio::time::timeout_at(deadline, updates.next())
            .await
            .is_err()
        {
            break;
        }
    }
    let result = if let Some(path) = location {
        let place = Proxy::new(
            &connection,
            "org.freedesktop.GeoClue2",
            path,
            "org.freedesktop.GeoClue2.Location",
        )
        .await?;
        let latitude: f64 = place.get_property("Latitude").await?;
        let longitude: f64 = place.get_property("Longitude").await?;
        let accuracy: f64 = place.get_property("Accuracy").await?;
        let (seconds, micros): (u64, u64) = place.get_property("Timestamp").await?;
        let description: String = place.get_property("Description").await?;
        zone_for_fix(
            &json!({"latitude":latitude,"longitude":longitude,"accuracy":accuracy,"timestamp":seconds as f64+micros as f64/1_000_000.0,"description":description}),
        )
    } else {
        Err("No fresh, accurate device location available".into())
    };
    let _: std::result::Result<(), _> = proxy.call("Stop", &()).await;
    result
}

pub fn run(args: &[String]) -> Result<i32> {
    if let [operation, body] = args {
        if operation == "validate_fix" {
            let input: Value = serde_json::from_str(body)?;
            let fix = input["args"]
                .as_array()
                .and_then(|v| v.first())
                .ok_or("Invalid device location")?;
            let now = input["kwargs"]["now"]
                .as_f64()
                .unwrap_or_else(|| chrono::Utc::now().timestamp() as f64);
            match validate_fix(fix, now) {
                Ok(value) => println!("{}", crate::json::encode(&value)?),
                Err(error) => {
                    println!(
                        "{}",
                        crate::json::encode(
                            &json!({"error":error.to_string(),"type":"ValueError"})
                        )?
                    );
                    return Ok(1);
                }
            }
            return Ok(0);
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    match runtime.block_on(tokio::time::timeout(
        Duration::from_secs(13),
        detect_async(),
    )) {
        Ok(Ok(value)) => {
            println!("{}", crate::json::encode(&value)?);
            Ok(0)
        }
        _ => {
            let message = tr(
                "helper.device-location.no-trustworthy-device-location-available",
                "No trustworthy device location available",
            );
            println!("{}", crate::json::encode(&json!({"error":message}))?);
            Ok(1)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fix_is_checked_without_persisting_coordinates() {
        let fix = json!({"latitude":59.0,"longitude":10.0,"accuracy":100.0,"timestamp":1000.0});
        assert!(validate_fix(&fix, 1000.0).is_ok());
        assert_eq!(
            validate_fix(&fix, 2000.0).unwrap_err().to_string(),
            "Device location is stale"
        );
        assert!(validate_fix(
            &json!({"latitude":100,"longitude":10,"accuracy":100,"timestamp":1000}),
            1000.0
        )
        .is_err());
        assert!(validate_fix(&json!({"latitude":59,"longitude":10,"accuracy":100,"timestamp":1000,"description":"GeoIP"}),1000.0).is_err());
    }
    #[test]
    fn offline_mapping_requires_all_uncertainty_samples_to_agree() {
        let fix = json!({"latitude":29.76,"longitude":-95.37,"accuracy":200,"timestamp":1000});
        let exact = zone_for_fix_with(&fix, 1000.0, |_, _| Ok("America/Chicago".into())).unwrap();
        assert_eq!(exact["timezone"], "America/Chicago");
        let boundary = zone_for_fix_with(&fix, 1000.0, |latitude, _| {
            Ok((if latitude < 29.78 {
                "America/Chicago"
            } else {
                "America/New_York"
            })
            .to_owned())
        });
        assert!(boundary.is_err());
    }
    #[test]
    fn offline_zone_table_resolves_its_representative_point() {
        assert_eq!(coordinate("+415100"), Some(41.85));
        assert_eq!(coordinate("-0873900"), Some(-87.65));
        assert_eq!(nearest_zone(41.85, -87.65).unwrap(), "America/Chicago");
    }
}
