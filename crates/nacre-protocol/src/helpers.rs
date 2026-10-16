//! Compatibility lanes retain their original payloads and use files for large packets.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelperRequest {
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub id: u64,
    pub tool: String,
    #[schemars(length(max = 8))]
    pub arguments: Vec<String>,
    pub input: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum HelperCall {
    Inline { request: HelperRequest },
    File { file: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelperResult {
    #[schemars(range(min = 0, max = 1))]
    pub code: i32,
    pub output: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum HelperReply {
    Inline(HelperResult),
    File { file: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HelperTransfer {
    /// Private directory leased to this connection, removed when it disconnects.
    pub directory: String,
}

impl JsonSchema for HelperCall {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "HelperCall".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"oneOf":[
            {"type":"object","properties":{"request":generator.subschema_for::<HelperRequest>()},"required":["request"],"not":{"required":["file"]}},
            {"type":"object","properties":{"file":{"type":"string"}},"required":["file"],"not":{"required":["request"]}}
        ]})
    }
}
impl JsonSchema for HelperReply {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "HelperReply".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut inline = serde_json::to_value(HelperResult::json_schema(generator))
            .expect("finite helper result schema");
        inline["not"] = serde_json::json!({"required":["file"]});
        schemars::json_schema!({"oneOf":[
            inline,
            {"type":"object","properties":{"file":{"type":"string"}},"required":["file"],"not":{"anyOf":[{"required":["code"]},{"required":["output"]}]}}
        ]})
    }
}
impl<'de> Deserialize<'de> for HelperCall {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match (value.get("request"), value.get("file")) {
            (Some(request), None) => Ok(Self::Inline {
                request: serde_json::from_value(request.clone())
                    .map_err(serde::de::Error::custom)?,
            }),
            (None, Some(file)) => Ok(Self::File {
                file: serde_json::from_value(file.clone()).map_err(serde::de::Error::custom)?,
            }),
            _ => Err(serde::de::Error::custom(
                "Helper call requires exactly one of request and file",
            )),
        }
    }
}
impl<'de> Deserialize<'de> for HelperReply {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match (value.get("code"), value.get("output"), value.get("file")) {
            (Some(_), Some(_), None) => Ok(Self::Inline(
                serde_json::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            (None, None, Some(file)) => Ok(Self::File {
                file: serde_json::from_value(file.clone()).map_err(serde::de::Error::custom)?,
            }),
            _ => Err(serde::de::Error::custom(
                "Helper reply requires exactly one inline result or file",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn optional_extensions_are_ignored_but_payload_variants_are_exclusive() {
        assert!(
            serde_json::from_value::<HelperCall>(json!({"file":"packet-a","future":true})).is_ok()
        );
        assert!(
            serde_json::from_value::<HelperReply>(json!({"code":0,"output":"","future":true}))
                .is_ok()
        );
        assert!(
            serde_json::from_value::<HelperReply>(json!({"file":"packet-a","future":true})).is_ok()
        );
        assert!(serde_json::from_value::<HelperReply>(
            json!({"file":"packet-a","code":0,"output":""})
        )
        .is_err());
        assert!(
            serde_json::from_value::<HelperCall>(json!({"file":"packet-a","request":null}))
                .is_err()
        );
    }
}
