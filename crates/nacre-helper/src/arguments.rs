//! Integer argument compatibility shared by the light and selector CLIs.
use unicode_general_category::{get_general_category, GeneralCategory};

/// Two remaining argparse entrypoints: one action and an optional value.
pub fn hosted(arguments: &[String], tool: &str) -> Result<Option<Vec<String>>, i32> {
    let choices = if tool == "launcher-preferences" {
        Some(["load", "favorite", "hide", "record"])
    } else {
        None
    };
    let action_label = choices
        .as_ref()
        .map_or_else(|| "action".to_owned(), |c| format!("{{{}}}", c.join(",")));
    let optional = if choices.is_none() { " [value]" } else { "" };
    let usage = format!("usage: {tool}.py [-h] {action_label}{optional}\n");
    let invalid = |message: String| {
        eprintln!("{usage}{tool}.py: error: {message}");
        2
    };
    let quote = |value: &str| crate::json::python_repr(&serde_json::Value::String(value.into()));
    let mut words = Vec::new();
    let mut unknown = Vec::new();
    let mut positional = false;
    let negative =
        regex::Regex::new(r"\A-(?:[0-9]+|[0-9]*\.[0-9]+)\z").expect("checked argument pattern");
    for word in arguments {
        if !positional {
            if word == "--" {
                positional = true;
                continue;
            }
            let (option, explicit) = word
                .split_once('=')
                .map_or((word.as_str(), None), |(name, value)| (name, Some(value)));
            if option == "-h" || (option.len() > 2 && "--help".starts_with(option)) {
                if let Some(value) = explicit {
                    return Err(invalid(format!(
                        "argument -h/--help: ignored explicit argument {}",
                        quote(value)
                    )));
                }
                let help_spacing = if choices.is_some() {
                    "            "
                } else {
                    "  "
                };
                print!("{usage}\npositional arguments:\n  {action_label}\n{}\noptions:\n  -h, --help{help_spacing}show this help message and exit\n",if choices.is_none(){"  value\n"}else{""});
                return Ok(None);
            }
            if word.starts_with('-') && word != "-" && !negative.is_match(word) {
                unknown.push(word.as_str());
                continue;
            }
        }
        if words.is_empty() {
            if let Some(choices) = choices {
                if !choices.contains(&word.as_str()) {
                    return Err(invalid(format!(
                        "argument action: invalid choice: {} (choose from {})",
                        quote(word),
                        choices
                            .iter()
                            .map(|c| quote(c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
        if words.is_empty() || (words.len() == 1 && choices.is_none()) {
            words.push(word.clone());
        } else {
            unknown.push(word.as_str());
        }
    }
    if words.is_empty() {
        return Err(invalid(
            "the following arguments are required: action".into(),
        ));
    }
    if !unknown.is_empty() {
        return Err(invalid(format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    if choices.is_none() && words.len() == 1 {
        words.push(String::new());
    }
    Ok(Some(words))
}

pub fn integer(value: &str) -> Result<i64, String> {
    let mapped: String = value
        .chars()
        .map(|c| {
            if get_general_category(c) == GeneralCategory::DecimalNumber {
                let code = c as u32;
                let mut start = code;
                while char::from_u32(start - 1)
                    .is_some_and(|c| get_general_category(c) == GeneralCategory::DecimalNumber)
                {
                    start -= 1;
                }
                char::from_digit((code - start) % 10, 10).expect("decimal digit")
            } else if !value.is_ascii() && c.is_whitespace() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let text = mapped.trim();
    let pattern = regex::Regex::new(r"\A[+-]?[0-9](?:_?[0-9])*\z").expect("integer expression");
    if !pattern.is_match(text) || text.bytes().filter(u8::is_ascii_digit).count() > 4300 {
        return Err("Invalid integer argument".into());
    }
    // Both consumers bound their operational value afterwards. Saturation keeps
    // Python's arbitrary-size integers valid without changing those bounds.
    Ok(text.replace('_', "").parse().unwrap_or_else(|_| {
        if text.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    }))
}
