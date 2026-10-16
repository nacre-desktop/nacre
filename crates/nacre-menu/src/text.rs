//! ECMAScript text semantics used by the frozen launcher algorithms.
use crate::Result;
use icu_collator::{Collator, CollatorBorrowed};
use serde_json::Value;
use std::cmp::Ordering;

pub fn whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000b}' | '\u{000c}' | '\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}
pub fn trim(value: &str) -> &str {
    value.trim_matches(whitespace)
}
pub fn string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(false) => String::new(),
        Value::Bool(true) => "true".into(),
        Value::Number(n) if n.as_f64() == Some(0.0) => String::new(),
        Value::Number(n) => n.to_string(),
        Value::Array(values) => values.iter().map(string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
pub fn literal(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Array(v) => v
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    literal(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
pub fn field(row: &Value, name: &str) -> String {
    string(&row[name])
}
pub fn uri_component(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            result.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut result, "%{byte:02X}").expect("writing to a String");
        }
    }
    result
}
pub struct OrderingRules(CollatorBorrowed<'static>);
impl OrderingRules {
    pub fn new(locale: &str) -> Result<Self> {
        let locale: icu_locale_core::Locale = locale.parse()?;
        Ok(Self(Collator::try_new(locale.into(), Default::default())?))
    }
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        self.0.compare(a, b)
    }
}
