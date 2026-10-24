//! User-owned `custom.kdl` binds win over generated binds in the niri-on-pc session.
use std::collections::BTreeSet;

/// Canonical form of a niri key combo (case-insensitive modifiers, order-free).
fn normalize(combo: &str) -> String {
    let mut parts: Vec<String> = combo.split('+').map(|p| p.to_ascii_lowercase()).collect();
    let key = parts.pop().unwrap_or_default();
    for part in &mut parts {
        *part = match part.as_str() {
            "win" | "mod" => "super".into(),
            "control" => "ctrl".into(),
            other => other.into(),
        };
    }
    parts.sort();
    parts.push(key);
    parts.join("+")
}

/// Keys bound inside top-level `binds { }` blocks. Comments and strings are skipped.
pub fn bound_keys(text: &str) -> BTreeSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut keys = BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    let (mut index, mut start, mut name) = (0, true, String::new());
    while index < chars.len() {
        let c = chars[index];
        match c {
            '/' if chars.get(index + 1) == Some(&'/') => {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
                continue;
            }
            '/' if chars.get(index + 1) == Some(&'*') => {
                index += 2;
                while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                    index += 1;
                }
                index += 2;
                continue;
            }
            '"' => {
                index += 1;
                while index < chars.len() && chars[index] != '"' {
                    index += if chars[index] == '\\' { 2 } else { 1 };
                }
                start = false;
            }
            '{' => {
                stack.push(std::mem::take(&mut name));
                start = true;
            }
            '}' => {
                stack.pop();
                start = true;
            }
            ';' | '\n' => start = true,
            c if c.is_whitespace() => {}
            _ if start => {
                let begin = index;
                while index < chars.len() && !" \t\r\n{};\"".contains(chars[index]) {
                    index += 1;
                }
                let token: String = chars[begin..index].iter().collect();
                if stack.len() == 1 && stack[0] == "binds" {
                    keys.insert(normalize(&token));
                }
                name = token;
                start = false;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    keys
}

/// Drop one-line generated binds whose key is overridden. Returns the new text
/// and the overridden key spellings (as written in the generated file).
pub fn drop_overridden(text: &str, overrides: &BTreeSet<String>) -> (String, Vec<String>) {
    let (mut out, mut dropped) = (String::new(), Vec::new());
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("binds") && trimmed.ends_with('{') {
            inside = true;
        } else if trimmed == "}" {
            inside = false;
        } else if inside && line.starts_with("    ") {
            if let Some(key) = trimmed.split_whitespace().next() {
                if overrides.contains(&normalize(key)) {
                    dropped.push(key.to_owned());
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    (out, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_binds_and_ignores_comments_strings_and_other_blocks() {
        let keys = bound_keys(
            "// Mod+Z\nlayout { gaps 4 }\nbinds {\n  Super+Tab { toggle-overview; }\n  // Print x\n  ctrl+SUPER+q repeat=false { spawn-sh \"Super+X {\"; }\n}\n",
        );
        assert_eq!(
            keys.into_iter().collect::<Vec<_>>(),
            ["ctrl+super+q", "super+tab"]
        );
    }
    #[test]
    fn drops_only_matching_lines() {
        let text = "binds {\n    Print a { x; }\n    Super+Shift+S b { y; }\n}\n";
        let wanted = bound_keys("binds {\n  shift+Mod+s { x; }\n}\n");
        let (out, dropped) = drop_overridden(text, &wanted);
        assert_eq!(out, "binds {\n    Print a { x; }\n}\n");
        assert_eq!(dropped, ["Super+Shift+S"]);
    }
}
