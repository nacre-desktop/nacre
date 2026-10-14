//! Existing helper English catalog and pseudo-language behavior.
use crate::paths::Paths;
use regex::Regex;
use serde_json::Value;
use std::{env, fs, sync::OnceLock};

fn english() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        if let Ok(paths) = Paths::environment() {
            let assets = paths.assets();
            if let Some(parent) = assets.parent() {
                for name in ["source/i18n/en.json", "shell/i18n/en.json"] {
                    if let Ok(bytes) = fs::read(parent.join(name)) {
                        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                            if value.is_object() {
                                return value;
                            }
                        }
                    }
                }
            }
        }
        Value::Null
    })
}
fn language() -> String {
    if let Ok(value) = env::var("NACRE_LANGUAGE") {
        if !value.is_empty() {
            return value;
        }
    }
    Paths::environment()
        .ok()
        .and_then(|paths| fs::read(paths.config.join("desktop.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value["language"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "system".into())
}
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
pub fn tr(identifier: &str, fallback: &str) -> String {
    let source = &english()[identifier];
    let text = source
        .as_str()
        .or_else(|| source["other"].as_str())
        .unwrap_or(fallback);
    if language() == "pseudo" {
        pseudo(text)
    } else {
        text.to_owned()
    }
}
