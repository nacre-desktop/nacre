//! Debounced filesystem changes. Opaque source keys contain no personal paths.
use crate::{ErrorCode, Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperWatch {
    pub available: bool,
    #[schemars(schema_with = "generations_schema")]
    pub generations: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub error: Option<String>,
}
fn generations_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type":"object", "maxProperties":32,
        "propertyNames":{"type":"string","pattern":"^[0-9a-f]{64}$"},
        "additionalProperties":{"type":"integer","minimum":0,"maximum":MAX_SAFE_INTEGER}
    })
}
impl WallpaperWatch {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.generations.len() > 32
            || self.generations.iter().any(|(key, generation)| {
                key.len() != 64
                    || !key
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || *generation > MAX_SAFE_INTEGER
            })
            || self.error.as_ref().is_some_and(|e| e.len() > 256)
        {
            return Err(Failure::new(
                ErrorCode::Validation,
                "Invalid wallpaper watch state",
                false,
            ));
        }
        Ok(())
    }
}

/// Published static wallpaper state. `current` is the global default
/// value; `outputs` repeats that same value under its output name for stable
/// consumers that address outputs explicitly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperActive {
    pub available: bool,
    pub stale: bool,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub revision: u64,
    pub current: Option<ActiveWallpaper>,
    #[schemars(schema_with = "outputs_schema")]
    pub outputs: BTreeMap<String, ActiveWallpaper>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    Crop,
    Fit,
    Fill,
    Centre,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActiveWallpaper {
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub palette_revision: u64,
    #[schemars(
        length(min = 1, max = 4096),
        regex(pattern = r"^/(?!.*(?:^|/)\.\.(?:/|$))(?!.*://)[^\u0000]*$")
    )]
    pub path: String,
    pub file_identity: WallpaperFileIdentity,
    pub presentation: Presentation,
    pub palette: WallpaperPalette,
    /// Present only when the active wallpaper is a video or animated GIF; `path`
    /// and `file_identity` above stay the cached still poster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<WallpaperMotion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MotionKind {
    Video,
    Animation,
}

/// The playable source of a moving wallpaper. Same path rules as the poster;
/// the identity `size` is not capped at the poster's 128 MiB.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperMotion {
    pub kind: MotionKind,
    #[schemars(
        length(min = 1, max = 4096),
        regex(pattern = r"^/(?!.*(?:^|/)\.\.(?:/|$))(?!.*://)[^\u0000]*$")
    )]
    pub path: String,
    pub file_identity: WallpaperFileIdentity,
}

/// Largest still poster; motion files are bounded only by the safe-integer size.
pub const POSTER_BYTES: u64 = 134_217_728;

/// Stable metadata tuple for a bounded local file read. Consumers compare it
/// before and after decoding through the same open file descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperFileIdentity {
    #[schemars(length(min = 1, max = 20), regex(pattern = "^(0|[1-9][0-9]{0,19})$"))]
    pub device: String,
    #[schemars(length(min = 1, max = 20), regex(pattern = "^(0|[1-9][0-9]{0,19})$"))]
    pub inode: String,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub size: u64,
    #[schemars(range(min = -9007199254740991_i64, max = 9007199254740991_i64))]
    pub mtime_sec: i64,
    #[schemars(range(min = 0, max = 999999999_u32))]
    pub mtime_nsec: u32,
    #[schemars(range(min = -9007199254740991_i64, max = 9007199254740991_i64))]
    pub ctime_sec: i64,
    #[schemars(range(min = 0, max = 999999999_u32))]
    pub ctime_nsec: u32,
}
impl WallpaperFileIdentity {
    pub fn validate(&self) -> Result<(), Failure> {
        fn valid_id(value: &str) -> bool {
            !value.is_empty()
                && value.len() <= 20
                && value.bytes().all(|b| b.is_ascii_digit())
                && (value == "0" || !value.starts_with('0'))
                && value.parse::<u64>().is_ok()
        }
        if !valid_id(&self.device)
            || !valid_id(&self.inode)
            || self.size > MAX_SAFE_INTEGER
            || self.mtime_sec.unsigned_abs() > MAX_SAFE_INTEGER
            || self.ctime_sec.unsigned_abs() > MAX_SAFE_INTEGER
            || self.mtime_nsec > 999_999_999
            || self.ctime_nsec > 999_999_999
        {
            return Err(Failure::new(
                ErrorCode::Validation,
                "Invalid wallpaper file identity",
                false,
            ));
        }
        Ok(())
    }

    #[cfg(unix)]
    pub fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev().to_string(),
            inode: metadata.ino().to_string(),
            size: metadata.size(),
            mtime_sec: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec() as u32,
            ctime_sec: metadata.ctime(),
            ctime_nsec: metadata.ctime_nsec() as u32,
        }
    }

    #[cfg(unix)]
    pub fn matches(&self, metadata: &std::fs::Metadata) -> bool {
        Self::from_metadata(metadata) == *self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperPalette {
    pub mode: PaletteMode,
    #[schemars(schema_with = "colours_schema")]
    pub colours: BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PaletteMode {
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperActivate {
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub revision: u64,
    /// Optional catalogue source id. When present the daemon selects this item,
    /// after checking the active revision and that the id is in the catalogue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 4096))]
    pub id: Option<String>,
}
impl WallpaperActivate {
    pub fn validate(&self) -> Result<(), Failure> {
        if (self.revision == 0 && self.id.is_none())
            || self.revision > MAX_SAFE_INTEGER
            || self.id.as_ref().is_some_and(|id| !valid_path(id))
        {
            Err(Failure::new(
                ErrorCode::Validation,
                "Invalid wallpaper revision",
                false,
            ))
        } else {
            Ok(())
        }
    }
}

/// Lease-only wallpaper picker catalogue. Thumbnail paths refer to the helper's
/// bounded cache and are empty when a cached thumbnail is not available.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperCatalogue {
    #[schemars(length(max = 2048))]
    pub entries: Vec<WallpaperCatalogueEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperKind {
    Image,
    Video,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WallpaperCatalogueEntry {
    #[schemars(length(min = 1, max = 4096))]
    pub id: String,
    pub kind: WallpaperKind,
    #[schemars(length(max = 512))]
    pub title: String,
    /// JPEG, at most 640x400 and 256 KiB, in the 128-entry `$XDG_CACHE_HOME/nacre/wallpaper-media/` thumbnail cache.
    #[schemars(length(max = 4096))]
    pub thumbnail: String,
    /// JPEG, at most 1600x1000 and 2 MiB, in the same bounded cache; empty if unavailable.
    #[schemars(length(max = 4096))]
    pub preview: String,
    pub active: bool,
}
impl WallpaperCatalogue {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.entries.len() > 2048
            || self.entries.iter().any(|entry| {
                !valid_path(&entry.id)
                    || entry.title.len() > 512
                    || entry.thumbnail.len() > 4096
                    || entry.preview.len() > 4096
                    || (!entry.thumbnail.is_empty() && !valid_path(&entry.thumbnail))
                    || (!entry.preview.is_empty() && !valid_path(&entry.preview))
            })
        {
            return Err(Failure::new(
                ErrorCode::Validation,
                "Invalid wallpaper catalogue",
                false,
            ));
        }
        Ok(())
    }
}

fn outputs_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let value = generator.subschema_for::<ActiveWallpaper>();
    schemars::json_schema!({"type":"object","maxProperties":32,"propertyNames":{"type":"string","minLength":1,"maxLength":256},"additionalProperties":value})
}
fn colours_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"object","minProperties":3,"maxProperties":128,"required":["background","onSurface","primary"],"propertyNames":{"type":"string","minLength":1,"maxLength":64},"additionalProperties":{"type":"string","pattern":"^#[0-9A-Fa-f]{6}$"}})
}
impl WallpaperActive {
    pub fn validate(&self) -> Result<(), Failure> {
        let valid = self.revision <= MAX_SAFE_INTEGER
            && self.outputs.len() <= 32
            && self.outputs.iter().all(|(name, value)| {
                !name.is_empty() && name.len() <= 256 && value.validate().is_ok()
            })
            && self.error.as_ref().is_none_or(|e| e.len() <= 256)
            && if self.available {
                !self.stale && self.current.is_some()
            } else {
                true
            };
        let valid = valid
            && match self.current.as_ref() {
                Some(current) => {
                    current.validate().is_ok()
                        && current.revision == self.revision
                        && self.stale == !self.available
                        && self.outputs.values().all(|o| o == current)
                }
                None => {
                    self.outputs.is_empty() && self.revision == 0 && !self.stale && !self.available
                }
            };
        if valid {
            Ok(())
        } else {
            Err(Failure::new(
                ErrorCode::Validation,
                "Invalid active wallpaper state",
                false,
            ))
        }
    }
}
impl WallpaperMotion {
    pub fn is_valid(&self) -> bool {
        valid_path(&self.path) && self.file_identity.validate().is_ok()
    }
}
fn valid_path(value: &str) -> bool {
    let path = std::path::Path::new(value);
    value.len() <= 4096
        && path.is_absolute()
        && !value.contains('\0')
        && !value.contains("://")
        && !path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}
impl ActiveWallpaper {
    pub fn validate(&self) -> Result<(), Failure> {
        let palette = &self.palette;
        if self.revision == 0
            || self.revision > MAX_SAFE_INTEGER
            || self.palette_revision != self.revision
            || self.palette_revision > MAX_SAFE_INTEGER
            || !valid_path(&self.path)
            || self.file_identity.validate().is_err()
            || self.file_identity.size > POSTER_BYTES
            || self.motion.as_ref().is_some_and(|m| !m.is_valid())
            || palette.colours.is_empty()
            || palette.colours.len() > 128
            || !["background", "onSurface", "primary"]
                .iter()
                .all(|role| palette.colours.contains_key(*role))
            || palette.colours.iter().any(|(k, v)| {
                k.is_empty()
                    || k.len() > 64
                    || v.len() != 7
                    || !v.starts_with('#')
                    || !v[1..].bytes().all(|b| b.is_ascii_hexdigit())
            })
        {
            return Err(Failure::new(
                ErrorCode::Validation,
                "Invalid active wallpaper",
                false,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_keys_and_counters_are_bounded_and_optional_fields_remain_forward_compatible() {
        let mut state: WallpaperWatch = serde_json::from_value(serde_json::json!({
            "available":true,"generations":{"0".repeat(64):3},"futureOptional":true
        }))
        .unwrap();
        assert!(state.validate().is_ok());
        state.generations.insert("bad".into(), 0);
        assert!(state.validate().is_err());
        state.generations.remove("bad");
        state
            .generations
            .insert("0".repeat(64), MAX_SAFE_INTEGER + 1);
        assert!(state.validate().is_err());
    }

    #[test]
    fn catalogue_is_bounded_and_accepts_only_local_absolute_ids_and_thumbnails() {
        let good = WallpaperCatalogue {
            entries: vec![WallpaperCatalogueEntry {
                id: "/pictures/lake.png".into(),
                kind: WallpaperKind::Image,
                title: "Lake".into(),
                thumbnail: "/cache/wallpaper-media/lake.jpg".into(),
                preview: "/cache/wallpaper-media/lake-preview.jpg".into(),
                active: true,
            }],
        };
        assert!(good.validate().is_ok());
        let mut bad = good.clone();
        bad.entries[0].id = "https://example.invalid/image".into();
        assert!(bad.validate().is_err());
        let mut bad = good;
        bad.entries[0].thumbnail = "/cache/../private.jpg".into();
        assert!(bad.validate().is_err());
    }

    #[test]
    fn selection_is_additive_and_revision_zero_is_only_for_initial_selection() {
        let legacy: WallpaperActivate =
            serde_json::from_value(serde_json::json!({"revision":1})).unwrap();
        assert!(legacy.validate().is_ok());
        let selection: WallpaperActivate = serde_json::from_value(serde_json::json!({
            "revision":0,"id":"/pictures/lake.png"
        }))
        .unwrap();
        assert!(selection.validate().is_ok());
        let invalid: WallpaperActivate =
            serde_json::from_value(serde_json::json!({"revision":0})).unwrap();
        assert!(invalid.validate().is_err());
    }
}
