//! N0049: contracts the Bead Settings pages need: the system clock (`time` topic, `time.*`
//! methods), the privileged login-theme install and the `capabilities` topic.
//!
//! Everything is additive and v1. The notification history and the workflow presets need no new
//! contract (`notifications` topic and `notifications.clear`, `settings.desktop`).
use crate::{
    control_center::EnabledSet, core_services::Health, pill_controls::PillEmpty, Capability,
    ErrorCode, Failure, Method,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const TIME_TOPIC: &str = "time";
pub const CAPABILITIES_TOPIC: &str = "capabilities";
pub const MAX_FLAGS: usize = 64;
/// A person may take this long to answer the polkit prompt.
pub const PROMPT_TIMEOUT_MS: u32 = 120_000;

/// An IANA zone name as `timedatectl list-timezones` prints it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeSetTimezone {
    #[schemars(length(min = 1, max = 100))]
    pub zone: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeNtp {
    /// `CanNTP`: a time-synchronisation service is installed.
    pub supported: bool,
    /// `NTP`: the service is switched on.
    pub enabled: bool,
}
/// The `time` topic, read from `org.freedesktop.timedate1` and refreshed on its
/// `PropertiesChanged`. `ntp` is absent while `health.available` is false.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeState {
    pub health: Health,
    #[schemars(length(max = 100))]
    pub timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ntp: Option<TimeNtp>,
}
/// The `capabilities` topic: the flags the Settings registry's `visibleWhen`/`enabledWhen`
/// reference. A flag that is absent is false.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilitiesState {
    pub health: Health,
    pub flags: BTreeMap<String, bool>,
}

pub fn zone_valid(zone: &str) -> bool {
    !zone.is_empty()
        && zone.len() <= 100
        && zone.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-'))
        })
}
fn invalid() -> Failure {
    Failure::new(
        ErrorCode::Validation,
        "Invalid system settings request",
        false,
    )
    .dispatch_state(false)
}
impl TimeSetTimezone {
    pub fn validate(&self) -> Result<(), Failure> {
        zone_valid(&self.zone).then_some(()).ok_or_else(invalid)
    }
}
impl TimeState {
    pub fn validate(&self) -> Result<(), Failure> {
        (self.timezone.len() <= 100 && !self.timezone.chars().any(char::is_control))
            .then_some(())
            .ok_or_else(invalid)
    }
}
impl CapabilitiesState {
    pub fn validate(&self) -> Result<(), Failure> {
        let names = self.flags.len() <= MAX_FLAGS
            && self.flags.keys().all(|name| {
                (1..=64).contains(&name.len())
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            });
        names.then_some(()).ok_or_else(invalid)
    }
}

/// Name, params schema, result schema and capability class. Both `time.set_*` calls and the
/// login install wait for a polkit prompt, so they carry the full deadline.
fn table() -> Vec<(
    &'static str,
    serde_json::Value,
    serde_json::Value,
    Capability,
    u32,
)> {
    let schema = |value: schemars::Schema| serde_json::to_value(value).expect("settings schema");
    let changed = json!({"type":"object","properties":{"changed":{"type":"boolean"}},"required":["changed"],"additionalProperties":false});
    vec![
        (
            "time.zones",
            schema(schemars::schema_for!(PillEmpty)),
            json!({"type":"object","properties":{"zones":{"type":"array","items":{"type":"string","maxLength":100},"maxItems":2048}},"required":["zones"],"additionalProperties":false}),
            Capability::System,
            10_000,
        ),
        (
            "time.set_timezone",
            schema(schemars::schema_for!(TimeSetTimezone)),
            changed.clone(),
            Capability::System,
            PROMPT_TIMEOUT_MS,
        ),
        (
            "time.set_ntp",
            schema(schemars::schema_for!(EnabledSet)),
            changed,
            Capability::System,
            PROMPT_TIMEOUT_MS,
        ),
        (
            "login.install_theme",
            schema(schemars::schema_for!(PillEmpty)),
            json!({"type":"object","properties":{"installed":{"type":"boolean"},"manifest":{"type":"string","maxLength":4096},"appearance_published":{"type":"boolean"}},"required":["installed","appearance_published"],"additionalProperties":false}),
            Capability::System,
            PROMPT_TIMEOUT_MS,
        ),
    ]
}
/// Every method of this unit; the daemon advertises the subset whose backend is present.
pub fn methods() -> BTreeMap<String, Method> {
    table()
        .into_iter()
        .map(|(name, params, result, class, timeout)| {
            (
                name.into(),
                Method {
                    v: 1,
                    class,
                    timeout,
                    params,
                    result,
                },
            )
        })
        .collect()
}

pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, file, schema) in [
        (
            "TimeSetTimezone",
            "time-set-timezone",
            schemars::schema_for!(TimeSetTimezone),
        ),
        ("TimeState", "time-state", schemars::schema_for!(TimeState)),
        (
            "CapabilitiesState",
            "capabilities-state",
            schemars::schema_for!(CapabilitiesState),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("settings schema");
        types.push_str(&format!(
            "export type {name} = {}\n",
            crate::artifacts::luau(&schema)
        ));
        files.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(&schema).expect("schema JSON") + "\n",
        );
    }
    types.push_str("return {}\n");
    files.insert("protocol/luau/system-settings.d.luau".into(), types);
    let calls = [
        (
            "time.zones",
            json!({}),
            json!({"zones":["Europe/Oslo","UTC"]}),
        ),
        (
            "time.set_timezone",
            json!({"zone":"Europe/Oslo"}),
            json!({"changed":true}),
        ),
        (
            "time.set_ntp",
            json!({"enabled":false}),
            json!({"changed":true}),
        ),
        (
            "login.install_theme",
            json!({}),
            json!({"installed":true,"manifest":"/var/lib/nacre/login/backups/20261023T100000000000Z/manifest.json","appearance_published":true}),
        ),
    ];
    let mut frames = vec![
        json!({"t":"snap","topic":TIME_TOPIC,"seq":0,"data":{"health":{"available":true,"stale":false},"timezone":"Europe/Oslo","ntp":{"supported":true,"enabled":true}}}),
        json!({"t":"patch","topic":TIME_TOPIC,"base":0,"seq":1,"data":{"timezone":"UTC","ntp":{"enabled":false}}}),
        json!({"t":"snap","topic":CAPABILITIES_TOPIC,"seq":0,"data":{"health":{"available":true,"stale":false},"flags":{"desktop.blur":true,"input.controls":true,"niriWorkspaces":true,"powerProfiles":true,"touchpad":true}}}),
    ];
    for (index, (method, params, ok)) in calls.into_iter().enumerate() {
        let id = 600 + index as u64;
        frames.push(json!({"t":"call","id":id,"m":method,"p":params}));
        frames.push(json!({"t":"ret","id":id,"ok":ok}));
    }
    files.insert(
        "protocol/fixtures/system-settings.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("settings transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zones_are_iana_names_only() {
        for good in [
            "UTC",
            "Europe/Oslo",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+5",
        ] {
            assert!(zone_valid(good), "{good}");
        }
        for bad in [
            "",
            "/UTC",
            "UTC/",
            "a//b",
            "../etc",
            "Europe/Oslo\n",
            "a b",
            &"z".repeat(101),
        ] {
            assert!(!zone_valid(bad), "{bad:?}");
        }
        assert!(serde_json::from_value::<TimeSetTimezone>(json!({"zone":"UTC","x":1})).is_err());
    }
    #[test]
    fn methods_are_stable() {
        let methods = methods();
        assert_eq!(methods.len(), 4);
        assert!(methods.values().all(|m| m.v == 1 && m.timeout <= 120_000));
    }
    #[test]
    fn flag_names_are_bounded() {
        let mut state = CapabilitiesState {
            health: Health {
                available: true,
                stale: false,
                error: None,
            },
            flags: BTreeMap::from([("desktop.gapsIn".into(), true)]),
        };
        assert!(state.validate().is_ok());
        state.flags.insert("bad name".into(), true);
        assert!(state.validate().is_err());
    }
}
