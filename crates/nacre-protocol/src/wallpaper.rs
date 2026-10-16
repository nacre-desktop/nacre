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
}
