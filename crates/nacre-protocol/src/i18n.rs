//! UI language and message catalog for shells. The `i18n` topic carries only the
//! language and a revision; shells fetch bounded catalog pages by namespace.
use crate::{Capability, ErrorCode, Failure, Method, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const TOPIC: &str = "i18n";
/// Largest page, by entry count and by the summed byte length of ids and texts.
pub const MAX_PAGE_ENTRIES: usize = 200;
pub const MAX_PAGE_BYTES: usize = 32 * 1024;

/// `language` is the language the catalog serves (`en`, `pseudo` or a tag such as
/// `nb`); `fallback` is the language used for ids a translation lacks. `revision`
/// starts at 1 per daemon run and increases whenever `language` changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct I18nState {
    #[schemars(
        length(min = 2, max = 35),
        regex(pattern = r"^(pseudo|[a-z]{2,3}([-_][A-Za-z0-9]{1,8}){0,3})$")
    )]
    pub language: String,
    #[schemars(
        length(min = 2, max = 35),
        regex(pattern = r"^(pseudo|[a-z]{2,3}([-_][A-Za-z0-9]{1,8}){0,3})$")
    )]
    pub fallback: String,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
}

/// `namespace` is the first dotted segment of a message id (`bead`, `launcher`).
/// `revision`, when set, must equal the published revision or the call fails with
/// `stale_revision`. `after` continues a page: the last id of the previous page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct I18nCatalogParams {
    #[schemars(length(min = 1, max = 32), regex(pattern = r"^[a-z][a-z0-9]{0,31}$"))]
    pub namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub after: Option<String>,
}

/// Plain text, or `{one, other}` forms selected by a `count` argument. Texts use
/// `{name}` placeholders the shell substitutes; pseudo text arrives already
/// transformed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Message {
    Text(#[schemars(length(max = 1024))] String),
    Plural(Plural),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plural {
    #[schemars(length(max = 1024))]
    pub one: String,
    #[schemars(length(max = 1024))]
    pub other: String,
}

/// `entries` holds ids under the requested namespace, sorted. `next` is present
/// when more remain: pass it as `after`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct I18nCatalog {
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    #[schemars(schema_with = "entries_schema")]
    pub entries: BTreeMap<String, Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub next: Option<String>,
}
fn entries_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let message = generator.subschema_for::<Message>();
    schemars::json_schema!({
        "type":"object", "maxProperties":MAX_PAGE_ENTRIES,
        "propertyNames":{"type":"string","minLength":3,"maxLength":128},
        "additionalProperties":message
    })
}

fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid i18n request", false).dispatch_state(false)
}
impl I18nState {
    pub fn validate(&self) -> Result<(), Failure> {
        if !language(&self.language)
            || !language(&self.fallback)
            || !(1..=MAX_SAFE_INTEGER).contains(&self.revision)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl I18nCatalogParams {
    pub fn validate(&self) -> Result<(), Failure> {
        let namespace = self.namespace.as_bytes();
        if namespace.is_empty()
            || namespace.len() > 32
            || !namespace[0].is_ascii_lowercase()
            || !namespace
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            || self
                .revision
                .is_some_and(|r| r == 0 || r > MAX_SAFE_INTEGER)
            || self
                .after
                .as_ref()
                .is_some_and(|a| a.is_empty() || a.len() > 128 || a.contains('\0'))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// A language the catalog can name: `pseudo` or a lowercase base plus subtags.
pub fn language(value: &str) -> bool {
    value == "pseudo" || {
        let mut parts = value.split(['-', '_']);
        parts.next().is_some_and(|b| {
            (2..=3).contains(&b.len()) && b.bytes().all(|c| c.is_ascii_lowercase())
        }) && parts.clone().count() <= 3
            && parts
                .all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphanumeric()))
    }
}

pub fn methods() -> BTreeMap<String, Method> {
    [(
        "i18n.catalog".to_owned(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 5_000,
            params: serde_json::to_value(schemars::schema_for!(I18nCatalogParams))
                .expect("i18n params schema"),
            result: serde_json::to_value(schemars::schema_for!(I18nCatalog))
                .expect("i18n result schema"),
        },
    )]
    .into()
}

pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, file, schema) in [
        ("Plural", "i18n-plural", schemars::schema_for!(Plural)),
        ("Message", "i18n-message", schemars::schema_for!(Message)),
        ("I18nState", "i18n-state", schemars::schema_for!(I18nState)),
        (
            "I18nCatalogParams",
            "i18n-catalog-params",
            schemars::schema_for!(I18nCatalogParams),
        ),
        (
            "I18nCatalog",
            "i18n-catalog",
            schemars::schema_for!(I18nCatalog),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("i18n schema");
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
    files.insert("protocol/luau/i18n.d.luau".into(), types);
    let page = json!({"revision":1,"entries":{"bead.bar.clock":"Clock","bead.panel.windows":{"one":"{count} window","other":"{count} windows"}},"next":"bead.panel.windows"});
    let frames = [
        json!({"t":"snap","topic":"i18n","seq":0,"data":{"language":"en","fallback":"en","revision":1}}),
        json!({"t":"call","id":601,"m":"i18n.catalog","p":{"namespace":"bead","revision":1}}),
        json!({"t":"ret","id":601,"ok":page}),
        json!({"t":"call","id":602,"m":"i18n.catalog","p":{"namespace":"bead","revision":1,"after":"bead.panel.windows"}}),
        json!({"t":"ret","id":602,"ok":{"revision":1,"entries":{"bead.panel.workspaces":"Workspaces"}}}),
        json!({"t":"patch","topic":"i18n","base":0,"seq":1,"data":{"language":"nb","revision":2}}),
        json!({"t":"call","id":603,"m":"i18n.catalog","p":{"namespace":"bead","revision":1}}),
        json!({"t":"ret","id":603,"err":{"code":"stale_revision","message":"Language changed; refresh the catalog","retry":true}}),
    ];
    files.insert(
        "protocol/fixtures/i18n.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("i18n transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn languages_namespaces_and_cursors_fail_closed() {
        for ok in ["en", "pseudo", "nb", "pt_BR", "zh-Hant-TW", "fil"] {
            assert!(language(ok), "{ok}");
        }
        for bad in [
            "",
            "EN",
            "e",
            "english",
            "../tr",
            "en_",
            "nb\0",
            "C.UTF-8",
            "a-b-c-d-e",
        ] {
            assert!(!language(bad), "{bad}");
        }
        let ok = |namespace: &str| I18nCatalogParams {
            namespace: namespace.into(),
            revision: Some(1),
            after: None,
        };
        assert!(ok("bead").validate().is_ok());
        for bad in ["", "Bead", "1bead", "bead.bar", "../x", &"a".repeat(33)] {
            assert!(ok(bad).validate().is_err(), "{bad}");
        }
        let mut params = ok("bead");
        params.revision = Some(0);
        assert!(params.validate().is_err());
        params.revision = None;
        params.after = Some(String::new());
        assert!(params.validate().is_err());
        assert!(
            serde_json::from_value::<I18nCatalogParams>(json!({"namespace":"bead","x":1})).is_err()
        );
    }
    #[test]
    fn state_validates_and_messages_round_trip() {
        let state = I18nState {
            language: "nb".into(),
            fallback: "en".into(),
            revision: 2,
        };
        assert!(state.validate().is_ok());
        assert!(I18nState {
            revision: 0,
            ..state.clone()
        }
        .validate()
        .is_err());
        assert!(I18nState {
            language: "x".into(),
            ..state
        }
        .validate()
        .is_err());
        let page: I18nCatalog = serde_json::from_value(json!({
            "revision":1,"entries":{"a.b":"Text","a.c":{"one":"1","other":"n"}}
        }))
        .unwrap();
        assert!(matches!(page.entries["a.b"], Message::Text(_)));
        assert!(matches!(page.entries["a.c"], Message::Plural(_)));
        assert!(serde_json::from_value::<Message>(json!({"one":"x"})).is_err());
    }
    #[test]
    fn method_is_registered_once() {
        assert_eq!(methods().keys().collect::<Vec<_>>(), ["i18n.catalog"]);
    }
}
