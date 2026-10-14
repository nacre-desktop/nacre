//! Read-only availability for AI software; never bootstrap an uninspected repository.
use crate::{paths::Paths, process};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, process::Command, time::Duration};

fn which(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|folder| {
        let path = folder.join(name);
        path.is_file() && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    })
}
fn official_chatgpt_metadata(text: &str) -> bool {
    let fields = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect::<std::collections::BTreeMap<_, _>>();
    fields.get("Name") == Some(&"chatgpt")
        && fields.get("URL").is_some_and(|url| {
            *url == "https://openai.com"
                || *url == "https://chatgpt.com"
                || url.starts_with("https://openai.com/")
                || url.starts_with("https://chatgpt.com/")
        })
}
pub fn snapshot(paths: &Paths) -> Value {
    let package = if which("pacman") {
        let mut command = Command::new("pacman");
        command
            .args(["-Si", "chatgpt"])
            .env("LC_ALL", "C")
            .current_dir(&paths.home);
        process::capture(command, None, Duration::from_secs(3)).is_ok_and(|output| {
            output.status.success()
                && official_chatgpt_metadata(&String::from_utf8_lossy(&output.stdout))
        })
    } else {
        false
    };
    let web_apps = [
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "brave",
        "brave-browser",
    ]
    .iter()
    .any(|p| which(p));
    json!({"chatgptPackage":package,"webApps":web_apps,"terminal":which("kitty")})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_package_is_not_confused_with_a_community_cli() {
        assert!(official_chatgpt_metadata(
            "Name : chatgpt\nURL : https://openai.com/chatgpt\n"
        ));
        assert!(!official_chatgpt_metadata(
            "Name : chatgpt-bin\nURL : https://github.com/j178/chatgpt\n"
        ));
        assert!(!official_chatgpt_metadata(
            "Name : chatgpt\nURL : https://openai.com.attacker.example/\n"
        ));
    }
}
