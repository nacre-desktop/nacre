//! Python-compatible JSON spacing and ASCII escaping for existing file contracts.
use crate::Result;
use serde::Serialize;
pub fn encode<T: Serialize>(value: &T) -> Result<String> {
    let text = serde_json::to_string(value)?;
    let mut result = String::with_capacity(text.len());
    let mut quoted = false;
    let mut escaped = false;
    for ch in text.chars() {
        if ch as u32 >= 127 {
            for unit in ch.encode_utf16(&mut [0; 2]).iter() {
                result.push_str(&format!("\\u{unit:04x}"));
            }
            escaped = false;
            continue;
        }
        result.push(ch);
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && quoted {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        }
        if !quoted && [':', ','].contains(&ch) {
            result.push(' ');
        }
    }
    Ok(result)
}

/// Preserve the common Python diagnostic when a JSON input has no value token.
pub fn parse_value(text: &str) -> Result<serde_json::Value> {
    serde_json::from_str(text).map_err(|error| {
        let first = text.trim_start_matches([' ', '\t', '\r', '\n']);
        let opening = first.chars().next();
        let number =
            opening == Some('-') && first.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
        if opening.is_none()
            || (!number && !opening.is_some_and(|c| c.is_ascii_digit() || "{[\"tfn".contains(c)))
        {
            let prefix = &text[..text.len() - first.len()];
            let offset = prefix.chars().count();
            let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
            let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
            format!("Expecting value: line {line} column {column} (char {offset})").into()
        } else {
            error.into()
        }
    })
}
/// Python-compatible indented JSON with ASCII escaping, without a final newline.
pub fn pretty<T: Serialize>(value: &T) -> Result<String> {
    let source = serde_json::to_string_pretty(value)?;
    let mut rendered = String::new();
    for c in source.chars() {
        if c as u32 >= 127 {
            for unit in c.encode_utf16(&mut [0; 2]).iter() {
                rendered.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            rendered.push(c);
        }
    }
    Ok(rendered)
}
/// Python str() for JSON-shaped values, including repr() for container members.
pub fn python_str(value: &serde_json::Value) -> String {
    if let serde_json::Value::String(value) = value {
        return value.clone();
    }
    python_repr(value)
}
pub(crate) fn python_repr(value: &serde_json::Value) -> String {
    use serde_json::Value;
    match value {
        Value::Null => "None".into(),
        Value::Bool(value) => if *value { "True" } else { "False" }.into(),
        Value::Number(value) => {
            if value.is_f64() {
                let text = format!("{:?}", value.as_f64().expect("finite JSON number"));
                if let Some((mantissa, exponent)) = text.split_once('e') {
                    let number: i32 = exponent.parse().expect("formatted exponent");
                    format!(
                        "{mantissa}e{}{number:02}",
                        if number < 0 { "-" } else { "+" },
                        number = number.unsigned_abs()
                    )
                } else {
                    text
                }
            } else {
                value.to_string()
            }
        }
        Value::String(value) => {
            use unicode_general_category::{get_general_category, GeneralCategory};
            let quote = if value.contains('\'') && !value.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut output = quote.to_string();
            for character in value.chars() {
                match character {
                    '\\' => output.push_str("\\\\"),
                    '\n' => output.push_str("\\n"),
                    '\r' => output.push_str("\\r"),
                    '\t' => output.push_str("\\t"),
                    character if character == quote => {
                        output.push('\\');
                        output.push(character);
                    }
                    character
                        if crate::unicode::is_other(character)
                            || (character != ' '
                                && matches!(
                                    get_general_category(character),
                                    GeneralCategory::SpaceSeparator
                                        | GeneralCategory::LineSeparator
                                        | GeneralCategory::ParagraphSeparator
                                )) =>
                    {
                        let code = character as u32;
                        if code <= 255 {
                            output.push_str(&format!("\\x{code:02x}"));
                        } else if code <= 65535 {
                            output.push_str(&format!("\\u{code:04x}"));
                        } else {
                            output.push_str(&format!("\\U{code:08x}"));
                        }
                    }
                    character => output.push(character),
                }
            }
            output.push(quote);
            output
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_repr)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    python_repr(&Value::String(key.clone())),
                    python_repr(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
