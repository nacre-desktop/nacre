//! Temporary output-rate preference. Saved desktop display preferences stay untouched.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Rate {
    width: Value,
    height: Value,
    previous: f64,
    preferred: f64,
}
#[derive(Default, Serialize, Deserialize)]
struct Journal {
    session: String,
    rates: BTreeMap<String, Rate>,
}

fn sixty_hz(row: &Value) -> Option<f64> {
    let prefix = format!("{}x{}@", row["width"], row["height"]);
    row["availableModes"]
        .as_array()?
        .iter()
        .filter_map(|mode| {
            let hz = mode
                .as_str()?
                .strip_prefix(&prefix)?
                .trim_end_matches("Hz")
                .parse::<f64>()
                .ok()?;
            (hz.is_finite() && (59.0..=61.0).contains(&hz)).then_some(hz)
        })
        .min_by(|a, b| (a - 60.0).abs().total_cmp(&(b - 60.0).abs()))
}

/// Change only refresh, retaining the live output geometry. A manual mode change
/// observed when disabling saver wins over restoration; disconnected outputs wait for reconnect.
fn plan(rows: &[Value], active: bool, journal: &mut Journal) -> Vec<Value> {
    let mut changes = Vec::new();
    for row in rows {
        let (Some(name), Some(current)) = (row["name"].as_str(), row["refreshRate"].as_f64())
        else {
            continue;
        };
        let mut target = row.clone();
        if active {
            let Some(preferred) = sixty_hz(row).filter(|hz| current > hz + 0.1) else {
                continue;
            };
            let saved = journal.rates.entry(name.into()).or_insert_with(|| Rate {
                width: row["width"].clone(),
                height: row["height"].clone(),
                previous: current,
                preferred,
            });
            if saved.width != row["width"] || saved.height != row["height"] {
                *saved = Rate {
                    width: row["width"].clone(),
                    height: row["height"].clone(),
                    previous: current,
                    preferred,
                };
            }
            target["refreshRate"] = json!(preferred);
        } else if let Some(saved) = journal.rates.remove(name) {
            if saved.width != row["width"]
                || saved.height != row["height"]
                || (current - saved.preferred).abs() > 0.1
            {
                continue;
            }
            target["refreshRate"] = json!(saved.previous);
        }
        if target != *row {
            changes.push(target);
        }
    }
    changes
}

/// Only geometry/mode changes matter; workspace focus must not trigger output queries.
pub(super) fn output_key(hub: &Shared) -> Value {
    let hub = hub.borrow();
    json!(hub
        .snapshot_ref("compositor")
        .and_then(|state| state["outputs"].as_object())
        .map(|outputs| outputs
            .values()
            .map(|row| json!([
                row["name"],
                row["width"],
                row["height"],
                row["refreshRate"],
                row["availableModes"],
                row["scale"],
                row["x"],
                row["y"],
                row["transform"]
            ]))
            .collect::<Vec<_>>())
        .unwrap_or_default())
}

pub(super) async fn update(hub: &Shared, paths: &Paths, active: bool) -> Result<()> {
    let Some(control) = hub.borrow().compositor.clone() else {
        return Ok(());
    };
    let path = paths.state.join("battery-saver-rates.json");
    control
        .with(move |backend| {
            let mut journal: Journal = match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice(&bytes)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Journal::default(),
                Err(error) => return Err(error.into()),
            };
            let session = format!("{}:{}", backend.name(), backend.session_id());
            if journal.session != session {
                journal = Journal {
                    session,
                    ..Journal::default()
                };
            }
            if !active && journal.rates.is_empty() {
                return Ok(());
            }
            // Read back immediately before applying: a quick disable can precede the
            // compositor topic's acknowledgement of the rate we just changed.
            let rows = backend.query("monitors", Duration::from_secs(5), Some(4 * 1024 * 1024))?;
            let rows = rows.as_array().ok_or("Invalid output records")?;
            let before = serde_json::to_vec(&journal)?;
            let changes = plan(rows, active, &mut journal);
            // Save before lowering a rate, so a daemon restart can still restore it.
            if active && !changes.is_empty() {
                crate::files::atomic(&path, &serde_json::to_vec(&journal)?)?;
            }
            if !changes.is_empty() {
                // Both adapters validate through their existing config generators;
                // niri's output transaction rolls back a partially applied change.
                backend.action(
                    "configure_monitors",
                    &[json!(changes)],
                    &json!({"timeout":5}),
                )?;
            }
            let after = serde_json::to_vec(&journal)?;
            if after != before {
                crate::files::atomic(&path, &after)?;
            }
            Ok(())
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn output() -> Value {
        json!({"name":"eDP-1","width":1920,"height":1080,"refreshRate":120.0,"scale":1.25,"x":0,"y":0,"transform":0,
            "availableModes":["1920x1080@120.000Hz","1920x1080@59.940Hz","1280x720@60.000Hz"]})
    }
    #[test]
    fn battery_saver_prefers_supported_sixty_and_restores_original_rate() {
        let original = output();
        let mut journal = Journal::default();
        let lowered = plan(std::slice::from_ref(&original), true, &mut journal);
        assert_eq!(lowered[0]["refreshRate"], 59.94);
        assert_eq!(lowered[0]["scale"], original["scale"]);
        assert!(plan(&lowered, true, &mut journal).is_empty());
        let restored = plan(&lowered, false, &mut journal);
        assert_eq!(restored, vec![original]);
        assert!(journal.rates.is_empty());
    }
    #[test]
    fn battery_saver_preserves_unsupported_low_and_manually_changed_modes() {
        let mut journal = Journal::default();
        let mut row = output();
        row["availableModes"] = json!(["1280x720@60.000Hz"]);
        assert!(plan(&[row.clone()], true, &mut journal).is_empty());
        row = output();
        row["refreshRate"] = json!(48.0);
        assert!(plan(&[row], true, &mut journal).is_empty());
        let mut lowered = plan(&[output()], true, &mut journal);
        lowered[0]["refreshRate"] = json!(90.0);
        assert!(plan(&lowered, false, &mut journal).is_empty());
        assert!(journal.rates.is_empty());
    }
    #[test]
    fn battery_saver_restores_the_users_new_resolution_rate() {
        let mut journal = Journal::default();
        plan(&[output()], true, &mut journal);
        let mut changed = output();
        changed["width"] = json!(2560);
        changed["height"] = json!(1440);
        changed["refreshRate"] = json!(144.0);
        changed["availableModes"] = json!(["2560x1440@144.000Hz", "2560x1440@60.000Hz"]);
        let lowered = plan(std::slice::from_ref(&changed), true, &mut journal);
        assert_eq!(lowered[0]["refreshRate"], 60.0);
        assert_eq!(plan(&lowered, false, &mut journal), vec![changed]);
    }
    #[test]
    fn battery_saver_journal_survives_restart_and_disconnected_output() {
        let mut journal = Journal::default();
        let lowered = plan(&[output()], true, &mut journal);
        let mut restarted: Journal =
            serde_json::from_slice(&serde_json::to_vec(&journal).unwrap()).unwrap();
        assert!(plan(&[], false, &mut restarted).is_empty());
        assert_eq!(plan(&lowered, false, &mut restarted), vec![output()]);
    }
    #[test]
    fn battery_saver_generated_modes_keep_both_compositors_valid() {
        let rows = plan(&[output()], true, &mut Journal::default());
        assert!(crate::compositor::niri_config::monitors(&json!(rows))
            .unwrap()
            .contains("1920x1080@59.940"));
        assert!(crate::compositor::hyprland::render_monitors(&json!(rows))
            .unwrap()
            .contains("1920x1080@59.940"));
    }
}
