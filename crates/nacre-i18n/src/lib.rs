//! Catalog translation shared by helper messages and launcher views.
//! Callers own catalog loading, language selection and UI lifetime.
#![deny(unsafe_code)]
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

pub fn pseudo(text: &str) -> String {
    const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const ACCENTS: &str = "áƀçďëƒğħïĵķľḿñöþɋŕšţüṽŵẋÿžÁɃÇĎËƑĞĦÏĴĶĽḾÑÖÞɊŔŠŢÜṼŴẊŸŽ";
    static TOKENS: OnceLock<Regex> = OnceLock::new();
    let tokens = TOKENS.get_or_init(|| Regex::new(r"<[^>]+>|\{[A-Za-z_][A-Za-z0-9_]*\}").unwrap());
    let accents: Vec<_> = ACCENTS.chars().collect();
    let mut rendered = String::new();
    let mut letters = 0;
    let mut previous = 0;
    fn translate(slice: &str, rendered: &mut String, letters: &mut usize, accents: &[char]) {
        for ch in slice.chars() {
            if ch.is_ascii_alphabetic() {
                *letters += 1;
                rendered.push(accents[ALPHABET.find(ch).unwrap()]);
            } else {
                rendered.push(ch);
            }
        }
    }
    for token in tokens.find_iter(text) {
        translate(
            &text[previous..token.start()],
            &mut rendered,
            &mut letters,
            &accents,
        );
        rendered.push_str(token.as_str());
        previous = token.end();
    }
    translate(&text[previous..], &mut rendered, &mut letters, &accents);
    format!("[{rendered} {}]", "~".repeat((letters * 4).div_ceil(10)))
}
fn literal(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value
            .as_f64()
            .map(|value| value.to_string())
            .unwrap_or_else(|| value.to_string()),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                if value.is_null() {
                    String::new()
                } else {
                    literal(value)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
fn count_is_one(value: &Value) -> bool {
    value.as_f64() == Some(1.0)
        || value == &Value::Bool(true)
        || value
            .as_str()
            .is_some_and(|value| value.trim().parse::<f64>() == Ok(1.0))
}
pub fn translate(
    messages: &Value,
    language: &str,
    id: &str,
    fallback: &str,
    args: &Value,
) -> String {
    let value = &messages[id];
    let message = value
        .as_str()
        .or_else(|| {
            if count_is_one(&args["count"]) {
                value["one"].as_str().or_else(|| value["other"].as_str())
            } else {
                value["other"].as_str()
            }
        })
        .unwrap_or(fallback);
    let message = if language == "pseudo" {
        pseudo(message)
    } else {
        message.into()
    };
    if args.as_object().is_none_or(|args| args.is_empty()) {
        return message;
    }
    static PLACEHOLDERS: OnceLock<Regex> = OnceLock::new();
    PLACEHOLDERS
        .get_or_init(|| Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("placeholder pattern"))
        .replace_all(&message, |capture: &regex::Captures<'_>| {
            args.as_object()
                .and_then(|args| args.get(&capture[1]))
                .map(literal)
                .unwrap_or_else(|| capture[0].into())
        })
        .into_owned()
}
pub fn localize(value: &Value, messages: &Value, language: &str) -> Value {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| localize(value, messages, language))
            .collect(),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    let translated = if key.ends_with("Id") {
                        value.clone()
                    } else if let Some(id) = values
                        .get(&format!("{key}Id"))
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                    {
                        json!(translate(
                            messages,
                            language,
                            id,
                            value
                                .as_str()
                                .or_else(|| value["other"].as_str())
                                .unwrap_or(""),
                            &Value::Null
                        ))
                    } else {
                        localize(value, messages, language)
                    };
                    (key.clone(), translated)
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}
