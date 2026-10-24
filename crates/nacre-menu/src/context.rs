//! Provider inputs are desktop data, with no dependency on a view framework.
use crate::{
    model, sources,
    text::{self, OrderingRules},
    Result,
};
use serde_json::{json, Value};

pub fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub fn yes(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}
pub fn hash(value: &str) -> String {
    format!("{:x}", md5::compute(value))
}
pub struct Context {
    pub(crate) queries: std::sync::OnceLock<crate::index::QueryCache>,
    pub catalog: Value,
    pub snapshot: Value,
    pub settings: Value,
    pub extras: Value,
    pub audio: Value,
    pub network: Value,
    pub bluetooth: Value,
    pub wallpapers: Value,
    pub compositor: Value,
    /// Stable window IDs and workspace IDs for the command palette only.
    pub palette_windows: Value,
    pub controls: Value,
    pub players: Value,
    pub power: Value,
    pub updates: Value,
    pub workflows: Value,
    pub previews: Value,
    pub library: Value,
    pub preferences: Value,
    pub legacy_actions: Vec<Value>,
    pub source_options: Value,
    pub translations: Value,
    pub language: String,
    pub constants: Value,
    pub all_apps: Vec<std::sync::Arc<Value>>,
    pub visible_apps: Vec<std::sync::Arc<Value>>,
    pub emoji_entries: Vec<Value>,
    pub notes: Vec<Value>,
    pub brain_query: String,
    pub files: Vec<Value>,
    pub file_query: String,
    pub packages: Vec<Value>,
    pub package_query: String,
    pub personality: String,
    pub recording_active: bool,
    pub can_ask: bool,
    pub now: f64,
    pub utc_offset_seconds: Option<i32>,
    pub ordering: OrderingRules,
}
impl Context {
    pub fn new(catalog: Value, constants: Value, locale: &str, now: f64) -> Result<Self> {
        let emoji_entries = array(&constants["emoji"]).to_vec();
        Ok(Self {
            queries: std::sync::OnceLock::new(),
            catalog,
            constants,
            now,
            utc_offset_seconds: None,
            ordering: OrderingRules::new(locale)?,
            snapshot: json!({}),
            settings: json!({}),
            extras: json!({}),
            audio: json!({}),
            network: json!({}),
            bluetooth: json!({}),
            wallpapers: json!({}),
            compositor: json!({}),
            palette_windows: json!([]),
            controls: json!({}),
            players: json!({}),
            power: json!({}),
            updates: json!({}),
            workflows: json!({}),
            previews: json!({}),
            library: json!({}),
            preferences: json!({}),
            legacy_actions: Vec::new(),
            source_options: sources::defaults(),
            translations: json!({}),
            language: "en".into(),
            all_apps: Vec::new(),
            visible_apps: Vec::new(),
            emoji_entries,
            notes: Vec::new(),
            brain_query: String::new(),
            files: Vec::new(),
            file_query: String::new(),
            packages: Vec::new(),
            package_query: String::new(),
            personality: String::new(),
            recording_active: false,
            can_ask: false,
        })
    }
    /// Call after changing desktop inputs. Query-specific provider replies need no reset.
    pub fn queries_prepared(&self) -> bool {
        self.queries.get().is_some()
    }
    pub fn inputs_changed(&mut self) {
        self.queries = std::sync::OnceLock::new();
    }
    pub fn tr(&self, id: &str, fallback: &str, args: Value) -> String {
        nacre_i18n::translate(&self.translations, &self.language, id, fallback, &args)
    }
    pub fn rows(&self) -> &[Value] {
        array(&self.catalog["rows"])
    }
    pub fn apps(&self) -> Vec<Value> {
        self.visible_apps
            .iter()
            .map(|app| model::application(app))
            .collect()
    }
    // Keep the frozen row constructor's named fields visible at each provider.
    #[allow(clippy::too_many_arguments)]
    pub fn value_row(
        &self,
        parent: &str,
        id: &str,
        label: &str,
        operation: &str,
        value: Value,
        icon: &str,
        description: &str,
        extra: Value,
    ) -> Value {
        let breadcrumb = model::trail(parent, self.rows())
            .iter()
            .map(|v| text::field(v, "label"))
            .collect::<Vec<_>>()
            .join(" › ");
        let mut row = json!({"id":format!("{parent}.{id}"),"kind":"action","name":label,"label":label,"operation":operation,"value":value,"icon":icon,"description":description,"breadcrumb":breadcrumb,"disabled":false});
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                row[key] = value.clone();
            }
        }
        row
    }
    fn checked(&self, row: &Value) -> Option<bool> {
        let state = row["state"].as_str()?;
        let value = &row["value"];
        Some(match state {
            "power-profile" => {
                self.power["profile"]
                    == match value.as_str() {
                        Some("power-saver") => json!(0),
                        Some("balanced") => json!(1),
                        Some("performance") => json!(2),
                        _ => Value::Null,
                    }
            }
            "setting" => self.settings[value.as_str().unwrap_or("")] != false,
            "personality" => value.as_str() == Some(self.personality.as_str()),
            "palette-mode" => self.wallpapers["appearancePreferences"]["paletteMode"] == *value,
            "background" => {
                yes(&self.wallpapers["appearancePreferences"]["paletteBackgroundFromWallpaper"])
                    == (value == "wallpaper")
            }
            "dnd" => yes(&self.settings["dnd"]),
            "night-light" => yes(&self.controls["nightLightEnabled"]),
            "stay-awake" => yes(&self.snapshot["stayAwake"]),
            "mute" => self.audio["muted"].as_bool()?,
            "mic-mute" => self.audio["micMuted"].as_bool()?,
            "wifi" => self.network["wifiEnabled"].as_bool()?,
            "bluetooth" => self.bluetooth["powered"].as_bool()?,
            _ => return None,
        })
    }
    pub fn decorate(&self, row: &Value) -> Value {
        let mut out = model::entry(row, self.rows());
        let checked = self.checked(row);
        let tr = |id, fallback| self.tr(id, fallback, json!({}));
        out["shortcut"] = array(&self.extras["keys"])
            .iter()
            .find(|v| v["menu"] == row["id"])
            .map(|v| v["key"].clone())
            .unwrap_or(json!(""));
        out["checked"] = json!(checked == Some(true));
        out["status"] = json!(if row["state"] == "updates" {
            if yes(&self.updates["count"]) {
                self.tr(
                    "launcher.updates.available",
                    "{count} available",
                    json!({"count":self.updates["count"]}),
                )
            } else {
                tr("ui.menu-context.status.up-to-date", "Up to date")
            }
        } else if let Some(checked) = checked {
            if matches!(
                row["state"].as_str(),
                Some(
                    "setting"
                        | "dnd"
                        | "night-light"
                        | "stay-awake"
                        | "mute"
                        | "mic-mute"
                        | "wifi"
                        | "bluetooth"
                )
            ) {
                if checked {
                    tr("ui.menu-context.status.on", "On")
                } else {
                    tr("ui.menu-context.status.off", "Off")
                }
            } else if checked {
                tr("ui.menu-context.status.in-use", "In use")
            } else {
                String::new()
            }
        } else {
            String::new()
        });
        let capability = text::field(row, "capability");
        let disabled = if capability == "niriWorkspaces" {
            self.compositor["backendName"] != "niri"
        } else {
            !capability.is_empty()
                && self.snapshot["capabilities"][&capability] != true
                && self.workflows["capabilities"][&capability] != true
        };
        out["disabled"] = json!(disabled);
        out["reason"] = json!(if capability == "niriWorkspaces" && disabled {
            tr("workspaces.requires-niri", "Available on Niri only.")
        } else if disabled {
            tr(
                "launcher.disabled.optional-tool-is-not-installed",
                "Optional tool is not installed",
            )
        } else {
            String::new()
        });
        if row["id"] == "software.tui.new" && array(&self.library["tui"]["terminals"]).is_empty() {
            out["disabled"] = json!(true);
            out["reason"] = json!(tr(
                "software.tui.requires-terminal",
                "Install a supported terminal first"
            ));
        }
        if row["operation"] == "media" {
            let player = &self.players["active"];
            let cap = match row["value"].as_str() {
                Some("playPause") => "canTogglePlaying",
                Some("next") => "canGoNext",
                Some("previous") => "canGoPrevious",
                _ => "",
            };
            let disabled = !yes(player) || !yes(&player[cap]);
            out["disabled"] = json!(disabled);
            out["reason"] = json!(if !yes(player) {
                tr(
                    "launcher.disabled.no-active-media-player",
                    "No active media player",
                )
            } else if disabled {
                tr(
                    "launcher.disabled.this-player-does-not-support-that-action",
                    "This player does not support that action",
                )
            } else {
                String::new()
            });
        }
        if row["value"] == "record-stop" {
            out["disabled"] = json!(!self.recording_active);
            out["reason"] = json!(if self.recording_active {
                String::new()
            } else {
                tr(
                    "launcher.disabled.no-recording-is-running",
                    "No recording is running",
                )
            });
        }
        if matches!(
            row["value"].as_str(),
            Some("record-start" | "record-screen")
        ) && self.recording_active
        {
            out["disabled"] = json!(true);
            out["reason"] = json!(tr(
                "launcher.disabled.a-recording-is-already-running",
                "A recording is already running"
            ));
        }
        if row["value"] == "night-light" {
            let external = yes(&self.controls["nightLightExternal"]);
            let supported = yes(&self.controls["nightLightSupported"]);
            out["disabled"] = json!(!supported || external);
            out["reason"] = json!(if external {
                tr(
                    "launcher.disabled.managed-by-an-existing-controller",
                    "Managed by an existing controller",
                )
            } else if !supported {
                tr(
                    "launcher.disabled.install-hyprsunset-to-use-night-light",
                    "Install hyprsunset to use Night light",
                )
            } else {
                String::new()
            });
        }
        if row["operation"] == "power-profile"
            && row["value"] == "performance"
            && !yes(&self.power["hasPerformanceProfile"])
        {
            out["disabled"] = json!(true);
            out["reason"] = json!(tr(
                "launcher.disabled.performance-profile-is-unavailable-on-this-device",
                "Performance profile is unavailable on this device"
            ));
        }
        if row["value"] == "mute" {
            out["disabled"] = json!(!yes(&self.audio["available"]));
        }
        if row["value"] == "mic-mute" {
            out["disabled"] = json!(!yes(&self.audio["micAvailable"]));
        }
        if row["operation"] == "toggle" && row["value"] == "wifi" {
            out["disabled"] = json!(self.network["wifiAvailable"] == false);
            out["reason"] = json!(if self.network["wifiAvailable"] == false {
                tr(
                    "ui.menu-context.reason.no-wi-fi-adapter",
                    "No Wi-Fi adapter",
                )
            } else {
                String::new()
            });
        }
        if row["value"] == "bluetooth" {
            out["disabled"] = json!(!yes(&self.bluetooth["available"]));
        }
        out
    }
    pub fn all_static(&self) -> Vec<Value> {
        self.rows().iter().map(|r| self.decorate(r)).collect()
    }
    pub fn node(&self, id: &str) -> Option<Value> {
        self.rows()
            .iter()
            .find(|r| r["id"] == id)
            .map(|r| self.decorate(r))
    }
}
