//! Preserve the menu's small argparse grammar, including order-sensitive help.
use crate::json;
const ACTIONS: [&str; 16] = [
    "state",
    "files",
    "convert",
    "packages",
    "installed",
    "fonts",
    "ownership",
    "tool",
    "install",
    "remove",
    "webapp-create",
    "webapp-remove",
    "webapp-launch",
    "reminder",
    "cancel-reminder",
    "config",
];
fn usage() -> String {
    format!("usage: menu-backend.py [-h] [--force]\n                       {{{}}}\n                       [value]\n", ACTIONS.join(","))
}
fn help() -> String {
    format!("{}\nBounded menu discovery and explicit user-selected tools; no search-text\nexecution.\n\npositional arguments:\n  {{{}}}\n  value\n\noptions:\n  -h, --help            show this help message and exit\n  --force\n", usage(), ACTIONS.join(","))
}
fn invalid(message: &str) -> i32 {
    eprintln!("{}menu-backend.py: error: {message}", usage());
    2
}
fn quoted(value: &str) -> String {
    json::python_repr(&serde_json::Value::String(value.into()))
}
pub fn parse(arguments: &[String]) -> std::result::Result<Option<Vec<String>>, i32> {
    let mut action: Option<&str> = None;
    let mut value = None;
    let mut force = false;
    let mut positional = false;
    let mut unknown = Vec::new();
    let negative = regex::Regex::new(r"\A-(?:[0-9]+|[0-9]*\.[0-9]+)\z")
        .expect("checked negative number pattern");
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
                    return Err(invalid(&format!(
                        "argument -h/--help: ignored explicit argument {}",
                        quoted(value)
                    )));
                }
                print!("{}", help());
                return Ok(None);
            }
            if option.len() > 2 && "--force".starts_with(option) {
                if let Some(value) = explicit {
                    return Err(invalid(&format!(
                        "argument --force: ignored explicit argument {}",
                        quoted(value)
                    )));
                }
                force = true;
                continue;
            }
            // argparse treats ordinary negative decimal numbers as positionals.
            if word.starts_with('-') && word != "-" && !negative.is_match(word) {
                unknown.push(word.as_str());
                continue;
            }
        }
        if action.is_none() {
            if !ACTIONS.contains(&word.as_str()) {
                return Err(invalid(&format!(
                    "argument action: invalid choice: {} (choose from {})",
                    quoted(word),
                    ACTIONS
                        .iter()
                        .map(|a| quoted(a))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            action = Some(word);
        } else if value.is_none() {
            value = Some(word.as_str());
        } else {
            unknown.push(word.as_str());
        }
    }
    let action = action.ok_or_else(|| invalid("the following arguments are required: action"))?;
    if !unknown.is_empty() {
        return Err(invalid(&format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    let arguments = match action {
        "state" => {
            if force {
                vec![action.into(), "--force".into()]
            } else {
                vec![action.into()]
            }
        }
        "installed" | "fonts" | "webapp-create" | "reminder" => vec![action.into()],
        _ => vec![action.into(), value.unwrap_or("").into()],
    };
    Ok(Some(arguments))
}
