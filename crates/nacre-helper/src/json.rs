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

/// Truth values used by the existing dynamic helper contracts.
pub fn truthy(value: &serde_json::Value) -> bool {
    use serde_json::Value;
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}

/// Match json.dumps for durable helper files and canonical cache identities,
/// including Python's decimal/scientific thresholds and exponent padding.
/// Existing encode callers retain their original byte contract.
pub fn dumps<T: serde::Serialize>(value: &T, sort_keys: bool) -> Result<String> {
    fn render(value: &serde_json::Value, sorted: bool) -> Result<String> {
        use serde_json::Value;
        Ok(match value {
            Value::Number(number) if number.is_f64() => {
                let value = number.as_f64().ok_or("Invalid JSON number")?;
                let absolute = value.abs();
                if absolute >= 1e16 || (absolute > 0.0 && absolute < 1e-4) {
                    let text = format!("{value:e}");
                    let (mantissa, exponent) =
                        text.split_once('e').ok_or("Invalid scientific number")?;
                    let exponent = exponent.parse::<i32>()?;
                    format!(
                        "{mantissa}e{}{:02}",
                        if exponent < 0 { '-' } else { '+' },
                        exponent.unsigned_abs()
                    )
                } else {
                    let mut text = format!("{value}");
                    if !text.contains('.') {
                        text.push_str(".0");
                    }
                    text
                }
            }
            Value::Array(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|item| render(item, sorted))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ),
            Value::Object(items) => {
                let mut entries: Vec<_> = items.iter().collect();
                if sorted {
                    entries.sort_by_key(|(a, _)| *a);
                }
                format!(
                    "{{{}}}",
                    entries
                        .into_iter()
                        .map(|(key, value)| Ok(format!(
                            "{}: {}",
                            encode(key)?,
                            render(value, sorted)?
                        )))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                )
            }
            _ => encode(value)?,
        })
    }
    render(&serde_json::to_value(value)?, sort_keys)
}

/// Common decoder diagnostics retained by the stream/module contracts.
pub fn parse_legacy_value(text: &str) -> Result<serde_json::Value> {
    match serde_json::from_str(text) {
        Ok(value) => Ok(value),
        Err(error) if error.to_string().starts_with("key must be a string") => {
            let line = error.line();
            let column = error.column();
            let prefix = text
                .split_inclusive('\n')
                .take(line.saturating_sub(1))
                .map(str::chars)
                .map(Iterator::count)
                .sum::<usize>();
            let row = text.lines().nth(line.saturating_sub(1)).unwrap_or("");
            let character_column = row
                .char_indices()
                .take_while(|(byte, _)| *byte < column.saturating_sub(1))
                .count()
                + 1;
            Err(format!("Expecting property name enclosed in double quotes: line {line} column {character_column} (char {})",prefix+character_column-1).into())
        }
        Err(_) => parse_value(text),
    }
}
