//! N0050: Colors app contracts (wallpaper colour explorer).
//!
//! Additive, v1, Desktop class. Every method runs what the QML Colors app ran
//! (`nacre-helper colors catalog|preview|apply|export`, `app-theme`); only the shapes are new.
use crate::{pill_controls::PillEmpty, Capability, ErrorCode, Failure, Method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub const MAX_ENTRIES: usize = 512;
const MAX_REQUEST_BYTES: usize = 65_536;

/// `colors.preview` parameters. `request` is the QML request object: `image` (absolute path),
/// `mode` dark|light, `personality` natural|pigment|harmony|pop|mist|vivid|pearl|tide,
/// `backgroundFromWallpaper`, `overrides` {primary?, secondary?, tertiary?: "rrggbb"},
/// `brightness` -0.15..0.15, `saturation` 0..1.6, `hour` 0..23, `autoMode`, `tideAutomatic`,
/// `timeTint`, `workspaceColors`, `favoriteId?`, `accent?`, `pick?` {x, y} (0..1) and `compare`
/// (true computes `comparisons`). The engine validates every field; a refusal is `validation`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColorsPreview {
    pub request: Map<String, Value>,
}
/// `colors.apply` and `colors.export` parameters: the `id` of a preview reply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColorsId {
    #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))]
    pub id: String,
}
/// `colors.set_target`: the same switch as the QML Settings toggle. It writes
/// `colors appThemes.<id>` and re-publishes that target; `settings.set` alone does neither.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ColorsSetTarget {
    #[schemars(length(min = 1, max = 16), regex(pattern = "^[a-z0-9]{1,16}$"))]
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CatalogEntry {
    pub path: String,
    pub name: String,
    pub dynamic: bool,
    /// Absolute PNG in `$XDG_CACHE_HOME/nacre/shell/colors-thumbs/` (at most 256 px and 256 KiB),
    /// or empty.
    pub thumbnail: String,
}
/// `colors.catalog` reply. At most 512 entries. `favorites`, `history` and `choices` (only for the
/// listed wallpapers) are the stored values of the `colors` store, unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColorsCatalog {
    pub current: String,
    pub entries: Vec<CatalogEntry>,
    pub favorites: Vec<Value>,
    pub history: Vec<Value>,
    pub choices: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImageColor {
    pub hex: String,
    pub coverage: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuditText {
    pub foreground: String,
    pub background: String,
    pub ratio: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Audit {
    pub text_passes: bool,
    pub minimum_text_contrast: f64,
    pub warnings: Vec<String>,
    pub text: Vec<AuditText>,
    /// `protan`, `deutan`, `tritan`: role to colour.
    pub simulated: BTreeMap<String, BTreeMap<String, String>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub personality: String,
    pub colours: BTreeMap<String, String>,
    pub minimum_contrast: f64,
}
/// `colors.preview` reply. `thumbnail` is the preview poster, a PNG in
/// `$XDG_CACHE_HOME/nacre/shell/colors-preview/` (at most 1280 px and 2 MiB, the image's aspect
/// ratio), or empty. `comparisons` is empty unless the request set `compare`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ColorsPreviewResult {
    pub id: String,
    pub request: Map<String, Value>,
    pub name: String,
    pub image: String,
    pub thumbnail: String,
    pub frames: u32,
    pub favorite: bool,
    pub other_coverage: f64,
    pub image_colors: Vec<ImageColor>,
    pub colours: BTreeMap<String, String>,
    pub audit: Audit,
    pub comparisons: Vec<Comparison>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ColorsApplied {
    pub applied: bool,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ColorsExported {
    pub directory: String,
    pub files: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TargetStatus {
    /// An output of the target exists.
    Current,
    /// Enabled but nothing has been written yet.
    NotPublished,
    /// The application's tool is not installed.
    ToolMissing,
    /// The user switched the target off.
    Paused,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ColorsTarget {
    pub id: String,
    pub label: String,
    pub status: TargetStatus,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ColorsTargets {
    pub targets: Vec<ColorsTarget>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ColorsChanged {
    pub changed: bool,
}

fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid Colors request", false).dispatch_state(false)
}
impl ColorsPreview {
    pub fn validate(&self) -> Result<(), Failure> {
        let image = self.request.get("image").and_then(Value::as_str);
        let size = serde_json::to_vec(&self.request).map_or(usize::MAX, |bytes| bytes.len());
        if size <= MAX_REQUEST_BYTES
            && image.is_some_and(|path| {
                path.starts_with('/') && path.len() <= 4096 && !path.contains('\0')
            })
        {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
fn hex_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
impl ColorsId {
    pub fn validate(&self) -> Result<(), Failure> {
        hex_id(&self.id).then_some(()).ok_or_else(invalid)
    }
}
impl ColorsSetTarget {
    pub fn validate(&self) -> Result<(), Failure> {
        ((1..=16).contains(&self.id.len())
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
        .then_some(())
        .ok_or_else(invalid)
    }
}

fn table() -> Vec<(&'static str, Value, u32, Value)> {
    let schema = |value: schemars::Schema| serde_json::to_value(value).expect("colors schema");
    vec![
        (
            "colors.catalog",
            schema(schemars::schema_for!(PillEmpty)),
            10_000,
            schema(schemars::schema_for!(ColorsCatalog)),
        ),
        (
            "colors.preview",
            schema(schemars::schema_for!(ColorsPreview)),
            60_000,
            schema(schemars::schema_for!(ColorsPreviewResult)),
        ),
        (
            "colors.apply",
            schema(schemars::schema_for!(ColorsId)),
            60_000,
            schema(schemars::schema_for!(ColorsApplied)),
        ),
        (
            "colors.export",
            schema(schemars::schema_for!(ColorsId)),
            60_000,
            schema(schemars::schema_for!(ColorsExported)),
        ),
        (
            "colors.targets",
            schema(schemars::schema_for!(PillEmpty)),
            10_000,
            schema(schemars::schema_for!(ColorsTargets)),
        ),
        (
            "colors.set_target",
            schema(schemars::schema_for!(ColorsSetTarget)),
            60_000,
            schema(schemars::schema_for!(ColorsChanged)),
        ),
    ]
}

/// Every method of this unit; the daemon advertises them unless it runs in safe mode or has no
/// settings service.
pub fn methods() -> BTreeMap<String, Method> {
    table()
        .into_iter()
        .map(|(name, params, timeout, result)| {
            (
                name.into(),
                Method {
                    v: 1,
                    class: Capability::Desktop,
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
            "ColorsPreview",
            "colors-preview",
            schemars::schema_for!(ColorsPreview),
        ),
        ("ColorsId", "colors-id", schemars::schema_for!(ColorsId)),
        (
            "ColorsSetTarget",
            "colors-set-target",
            schemars::schema_for!(ColorsSetTarget),
        ),
        (
            "ColorsCatalog",
            "colors-catalog",
            schemars::schema_for!(ColorsCatalog),
        ),
        (
            "ColorsPreviewResult",
            "colors-preview-result",
            schemars::schema_for!(ColorsPreviewResult),
        ),
        (
            "ColorsApplied",
            "colors-applied",
            schemars::schema_for!(ColorsApplied),
        ),
        (
            "ColorsExported",
            "colors-exported",
            schemars::schema_for!(ColorsExported),
        ),
        (
            "ColorsTargets",
            "colors-targets",
            schemars::schema_for!(ColorsTargets),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("colors schema");
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
    files.insert("protocol/luau/colors.d.luau".into(), types);
    let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let roles = json!({"primary":"3a6ea5","secondary":"7a8fa8","tertiary":"a58a6e","surface":"10141a","onSurface":"e4e8ee"});
    let calls = [
        (
            "colors.catalog",
            json!({}),
            json!({"current":"/home/user/Pictures/lake.png","entries":[{"path":"/home/user/Pictures/lake.png","name":"lake","dynamic":false,"thumbnail":"/home/user/.cache/nacre/shell/colors-thumbs/0a1b.png"}],"favorites":[],"history":[],"choices":{}}),
        ),
        (
            "colors.preview",
            json!({"request":{"image":"/home/user/Pictures/lake.png","mode":"dark","personality":"natural","compare":false}}),
            json!({"id":id,"request":{"image":"/home/user/Pictures/lake.png","mode":"dark","personality":"natural","compare":false},"name":"lake","image":"/home/user/Pictures/lake.png","thumbnail":"/home/user/.cache/nacre/shell/colors-preview/9f8e.png","frames":1,"favorite":false,"otherCoverage":0.12,"imageColors":[{"hex":"3a6ea5","coverage":0.42}],"colours":roles,"audit":{"textPasses":true,"minimumTextContrast":7.12,"warnings":[],"text":[{"foreground":"e4e8ee","background":"10141a","ratio":14.2}],"simulated":{"protan":roles,"deutan":roles,"tritan":roles}},"comparisons":[]}),
        ),
        (
            "colors.apply",
            json!({"id":id}),
            json!({"applied":true,"name":"lake"}),
        ),
        (
            "colors.export",
            json!({"id":id}),
            json!({"directory":"/home/user/Documents/Nacre Colors/20271001-120000-01234567","files":["/home/user/Documents/Nacre Colors/20271001-120000-01234567/palette.json"]}),
        ),
        (
            "colors.targets",
            json!({}),
            json!({"targets":[{"id":"gtk3","label":"GTK 3","status":"current"},{"id":"kitty","label":"Kitty","status":"tool-missing"}]}),
        ),
        (
            "colors.set_target",
            json!({"id":"kitty","enabled":false}),
            json!({"changed":true}),
        ),
    ];
    let mut frames = Vec::new();
    for (index, (method, params, ok)) in calls.into_iter().enumerate() {
        let id = 600 + index as u64;
        frames.push(json!({"t":"call","id":id,"m":method,"p":params}));
        frames.push(json!({"t":"ret","id":id,"ok":ok}));
    }
    files.insert(
        "protocol/fixtures/colors.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("colors transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_are_closed_and_validated() {
        let preview = |value| serde_json::from_value::<ColorsPreview>(value).unwrap();
        assert!(preview(json!({"request":{"image":"/a.png"}}))
            .validate()
            .is_ok());
        for bad in [
            json!({"request":{}}),
            json!({"request":{"image":"relative.png"}}),
            json!({"request":{"image":"/a\u{0}b"}}),
            json!({"request":{"image":"/a.png","pad":"x".repeat(70_000)}}),
        ] {
            assert!(preview(bad).validate().is_err());
        }
        assert!(serde_json::from_value::<ColorsPreview>(json!({"request":{},"x":1})).is_err());
        assert!(serde_json::from_value::<ColorsPreview>(json!({"request":"a"})).is_err());
        let id = |text: &str| ColorsId { id: text.into() };
        assert!(id(&"a1".repeat(32)).validate().is_ok());
        for bad in [
            id(""),
            id(&"A1".repeat(32)),
            id(&"a1".repeat(31)),
            id(&"g".repeat(64)),
        ] {
            assert!(bad.validate().is_err());
        }
        assert!(serde_json::from_value::<ColorsId>(json!({"id":"a","extra":1})).is_err());
        let target = |text: &str| ColorsSetTarget {
            id: text.into(),
            enabled: true,
        };
        assert!(target("gtk3").validate().is_ok());
        for bad in [
            target(""),
            target("../x"),
            target("Kitty"),
            target(&"k".repeat(17)),
        ] {
            assert!(bad.validate().is_err());
        }
    }
    #[test]
    fn methods_are_desktop_v1_with_published_deadlines() {
        let all = methods();
        assert_eq!(all.len(), 6);
        assert!(all
            .values()
            .all(|m| m.v == 1 && m.class == Capability::Desktop && m.timeout <= 120_000));
        assert_eq!(all["colors.catalog"].timeout, 10_000);
        assert_eq!(all["colors.preview"].timeout, 60_000);
    }
    #[test]
    fn golden_replies_decode_into_the_published_types() {
        let files = exports();
        let frames: Vec<Value> = files["protocol/fixtures/colors.jsonl"]
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let ok = |index: usize| frames[index * 2 + 1]["ok"].clone();
        serde_json::from_value::<ColorsCatalog>(ok(0)).unwrap();
        let preview: ColorsPreviewResult = serde_json::from_value(ok(1)).unwrap();
        assert_eq!(preview.colours.len(), 5);
        serde_json::from_value::<ColorsApplied>(ok(2)).unwrap();
        serde_json::from_value::<ColorsExported>(ok(3)).unwrap();
        let targets: ColorsTargets = serde_json::from_value(ok(4)).unwrap();
        assert_eq!(targets.targets[1].status, TargetStatus::ToolMissing);
        serde_json::from_value::<ColorsChanged>(ok(5)).unwrap();
    }
}
