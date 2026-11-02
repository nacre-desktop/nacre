//! Preserve the observer's options and help; never connect in unsupported sessions.
use crate::Result;
use std::{collections::BTreeMap, path::Path};
use unicode_general_category::{get_general_category, GeneralCategory};

pub const HELP:&str="usage: niri_widths.py [-h] [--check-session] [--opening-width OPENING_WIDTH]\n\nRelax advisory X11 minimum widths only for ordinary Niri tiled windows. New\nconstrained windows receive one proportional opening-width correction. Later\ngeometry, application zoom, maximum sizes and height hints stay intact. The\nobserver starts only from main(), never when imported.\n\noptions:\n  -h, --help            show this help message and exit\n  --check-session\n  --opening-width OPENING_WIDTH\n                        Initial width percentage for newly constrained tiled\n                        X11 windows\n";
#[derive(Debug, PartialEq)]
pub enum Arguments {
    Help,
    Observe { check: bool, opening_width: f64 },
}
fn number(text: &str) -> Result<f64> {
    let normalized: String = text
        .trim()
        .chars()
        .map(|character| {
            if get_general_category(character) == GeneralCategory::DecimalNumber {
                let mut start = character as u32;
                while start > 0
                    && char::from_u32(start - 1).is_some_and(|previous| {
                        get_general_category(previous) == GeneralCategory::DecimalNumber
                    })
                {
                    start -= 1;
                }
                char::from(b'0' + ((character as u32 - start) % 10) as u8)
            } else {
                character
            }
        })
        .collect();
    let grammar = regex::Regex::new(
        r"(?i)^[+-]?(?:(?:[0-9](?:_?[0-9])*(?:\.[0-9](?:_?[0-9])*|\.)?|\.[0-9](?:_?[0-9])*)(?:e[+-]?[0-9](?:_?[0-9])*)?|inf(?:inity)?|nan)$",
    )?;
    if !grammar.is_match(&normalized) {
        return Err("Invalid opening width".into());
    }
    Ok(normalized.replace('_', "").parse()?)
}
impl Arguments {
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut check = false;
        let mut opening_width = 50.0;
        let mut index = 0;
        while index < args.len() {
            let raw = &args[index];
            if raw == "--" {
                return Err("Unexpected observer arguments".into());
            }
            let (name, value) = raw
                .split_once('=')
                .map_or((raw.as_str(), None), |(name, value)| (name, Some(value)));
            let option = if name == "-h" {
                "--help"
            } else if name.starts_with("--") {
                let matches: Vec<&str> = ["--check-session", "--opening-width", "--help"]
                    .into_iter()
                    .filter(|option| option.starts_with(name))
                    .collect();
                if matches.len() != 1 {
                    return Err("Unknown or ambiguous observer option".into());
                }
                matches[0]
            } else {
                return Err("Unexpected observer argument".into());
            };
            match option {
                "--help" if value.is_none() => return Ok(Self::Help),
                "--check-session" if value.is_none() => check = true,
                "--opening-width" => {
                    let text = match value {
                        Some(value) => value,
                        None => {
                            index += 1;
                            args.get(index)
                                .map(String::as_str)
                                .ok_or("Missing opening width")?
                        }
                    };
                    opening_width = number(text)?;
                }
                _ => return Err("Observer flag does not accept a value".into()),
            }
            index += 1;
        }
        if !opening_width.is_finite() || opening_width <= 0.0 || opening_width > 100.0 {
            return Err("Opening width must be greater than zero and at most 100 percent".into());
        }
        Ok(Self::Observe {
            check,
            opening_width,
        })
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    let args = match Arguments::parse(args) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("niri_widths.py: error: {error}");
            return Ok(2);
        }
    };
    let Arguments::Observe {
        check,
        opening_width,
    } = args
    else {
        print!("{HELP}");
        return Ok(0);
    };
    let environment: BTreeMap<String, String> = std::env::vars().collect();
    let supported = super::runtime::check_session(&environment)?;
    if check {
        return Ok(i32::from(!supported));
    }
    if supported {
        if let Some(code) = crate::daemon::widths::observe_if_daemon(opening_width, &environment)? {
            return Ok(code);
        }
        super::runtime::observe(
            Path::new(
                environment
                    .get("NIRI_SOCKET")
                    .ok_or("NIRI_SOCKET is unavailable")?,
            ),
            opening_width,
        )?;
    }
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn current_python_cli_stdout_and_exit_contracts() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-widths-cli-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing CLI cases")? {
            let args: Vec<String> = serde_json::from_value(case["request"]["args"].clone())?;
            let (stdout, code) = match Arguments::parse(&args) {
                Ok(Arguments::Help) => (HELP, 0),
                Ok(Arguments::Observe { check, .. }) => ("", i32::from(check)),
                Err(_) => ("", 2),
            };
            assert_eq!(stdout, case["observed"]["stdout"], "{args:?}");
            assert_eq!(code, case["observed"]["exit"], "{args:?}");
        }
        Ok(())
    }
}
