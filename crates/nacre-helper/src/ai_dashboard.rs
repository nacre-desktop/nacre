//! Native entry point for the standalone Nacre AI terminal dashboard.
use crate::{ai_core, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::{
    env, fs,
    io::{self, IsTerminal, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::{Command, Stdio},
};

const DASHBOARD_OPTIONS: [&str; 6] = [
    "New chat",
    "Resume latest chat",
    "Load chat",
    "Open brain",
    "Settings",
    "Close",
];

struct Terminal {
    original: nix::sys::termios::Termios,
}
impl Terminal {
    fn enter() -> Result<Self> {
        use std::os::fd::AsFd;
        let input = io::stdin();
        if !input.is_terminal() {
            return Err("nacre-ai requires an interactive terminal".into());
        }
        let original = nix::sys::termios::tcgetattr(input.as_fd())?;
        let mut raw = original.clone();
        nix::sys::termios::cfmakeraw(&mut raw);
        nix::sys::termios::tcsetattr(input.as_fd(), nix::sys::termios::SetArg::TCSAFLUSH, &raw)?;
        print!("\x1b[?1049h\x1b[?25l");
        io::stdout().flush()?;
        Ok(Self { original })
    }
    fn key(&self) -> Result<u8> {
        let mut byte = [0];
        io::stdin().read_exact(&mut byte)?;
        if byte[0] == 27 {
            use std::os::fd::AsFd;
            let input = io::stdin();
            let mut events = [nix::poll::PollFd::new(
                input.as_fd(),
                nix::poll::PollFlags::POLLIN,
            )];
            if nix::poll::poll(&mut events, nix::poll::PollTimeout::from(60u16))? > 0 {
                let mut sequence = [0; 2];
                if io::stdin().read_exact(&mut sequence).is_ok() && sequence[0] == b'[' {
                    return Ok(match sequence[1] {
                        b'A' => 0x81,
                        b'B' => 0x82,
                        _ => 27,
                    });
                }
            }
        }
        Ok(byte[0])
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        use std::os::fd::AsFd;
        let _ = nix::sys::termios::tcsetattr(
            io::stdin().as_fd(),
            nix::sys::termios::SetArg::TCSAFLUSH,
            &self.original,
        );
        print!("\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
    }
}

fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn valid_account(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && value.len() <= 48
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

fn terminal_size() -> (usize, usize) {
    Command::new("stty")
        .arg("size")
        .stdin(std::process::Stdio::inherit())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|output| {
            let mut fields = output
                .split_whitespace()
                .filter_map(|field| field.parse().ok());
            Some((fields.next()?, fields.next()?))
        })
        .unwrap_or((24, 80))
}

fn localized_title(value: &str) -> String {
    match value {
        "Workspace" => crate::i18n::tr("ai.ui.workspace", "Workspace"),
        "Settings" => crate::i18n::tr("ai.ui.settings", "Settings"),
        "Accounts" => "Accounts".into(),
        "Usage" => crate::i18n::tr("ai.ui.usage", "Usage"),
        "Projects" => "Projects".into(),
        "Load chat" => crate::i18n::tr("ai.ui.load-chat", "Load chat"),
        "New local project" => crate::i18n::tr("ai.ui.new-local-project", "New local project"),
        "Default assistant" => crate::i18n::tr("ai.ui.default-assistant", "Default assistant"),
        "Select account provider" => {
            crate::i18n::tr("ai.ui.settings-accounts", "Settings · Accounts")
        }
        "Setup status" => crate::i18n::tr("ai.ui.setup-status", "Setup status"),
        other => return clean(other).replace('>', "").to_uppercase(),
    }
    .replace('>', "")
    .to_uppercase()
}

fn localized_row(value: &str) -> String {
    match value {
        "New chat" => crate::i18n::tr("ai.ui.new-chat", "New chat"),
        "Resume latest chat" => crate::i18n::tr("ai.ui.resume-latest-chat", "Resume latest chat"),
        "Load chat" => crate::i18n::tr("ai.ui.load-chat", "Load chat"),
        "Open brain" => crate::i18n::tr("ai.ui.open-brain", "Open brain"),
        "Settings" => crate::i18n::tr("ai.ui.settings", "Settings"),
        "Close" => crate::i18n::tr("ai.ui.close", "Close"),
        "Usage" => crate::i18n::tr("ai.ui.usage", "Usage"),
        "Accounts" => "Accounts".into(),
        "Projects" => "Projects".into(),
        "Check setup" => crate::i18n::tr("ai.ui.check-setup", "Check setup"),
        "Back" | "← Back" => crate::i18n::tr("ai.ui.back", "Back"),
        "Add project" => crate::i18n::tr("ai.ui.add-project", "Add project"),
        "Create project" => crate::i18n::tr("ai.ui.create-project", "Create project"),
        "Codex" => crate::i18n::tr("ai.ui.codex", "Codex"),
        "Claude Code" => crate::i18n::tr("ai.ui.claude-code", "Claude Code"),
        other => other.to_owned(),
    }
}

fn put(y: usize, x: usize, text: &str, style: &str, width: usize) {
    if width == 0 {
        return;
    }
    let clipped = text.chars().take(width).collect::<String>();
    print!("\x1b[{};{}H{}{}\x1b[0m", y + 1, x + 1, style, clipped);
}

fn set_terminal_title(title: &str) {
    if io::stdout().is_terminal() {
        print!("\x1b]2;{}\x07", clean(title));
        let _ = io::stdout().flush();
    }
}

fn kitty_logo(path: &PathBuf, y: usize, x: usize) -> Result<()> {
    let encoded = BASE64.encode(fs::read(path)?);
    print!("\x1b7\x1b[{};{}H", y + 1, x + 1);
    let chunks = encoded.as_bytes().chunks(4096);
    let count = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let more = usize::from(index + 1 < count);
        let fields = if index == 0 {
            format!(
                "a=T,f=100,r=3,C=1,q=2,i={},p=1,m={more}",
                std::process::id()
            )
        } else {
            format!("m={more},q=2")
        };
        print!("\x1b_G{fields};{}\x1b\\", std::str::from_utf8(chunk)?);
    }
    print!("\x1b8");
    Ok(())
}

fn select<S: AsRef<str>>(title: &str, rows: &[S], back: bool) -> Result<Option<usize>> {
    select_with(title, rows, back, None, None)
}

fn select_with<S: AsRef<str>>(
    title: &str,
    rows: &[S],
    back: bool,
    header: Option<&str>,
    details: Option<&[String]>,
) -> Result<Option<usize>> {
    if rows.is_empty() {
        return Ok(None);
    }
    let terminal = Terminal::enter()?;
    let mut selected = 0usize;
    let mut query = String::new();
    let mut choices = rows
        .iter()
        .map(|row| row.as_ref().to_owned())
        .collect::<Vec<_>>();
    if back
        && !choices
            .iter()
            .any(|row| matches!(row.as_str(), "Back" | "← Back"))
    {
        choices.push(String::from("← Back"));
    }
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let text_logo = home.join(".local/share/nacre/branding/nacre-ai-text.txt");
    let png_logo = home.join(".local/share/nacre/branding/nacre-ai.png");
    let graphic = env::var("TERM").is_ok_and(|term| term == "xterm-kitty")
        && io::stdout().is_terminal()
        && png_logo.is_file()
        && env::var_os("TMUX").is_none();
    let title = localized_title(title);
    loop {
        let matches = choices
            .iter()
            .enumerate()
            .filter(|(index, row)| {
                matches!(row.as_str(), "Back" | "← Back")
                    || query.is_empty()
                    || row.to_lowercase().contains(&query)
                    || details
                        .and_then(|entries| entries.get(*index))
                        .is_some_and(|detail| detail.to_lowercase().contains(&query))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if !matches.is_empty() {
            selected = selected.min(matches.len() - 1);
        }
        let (height, width) = terminal_size();
        let box_width = 62usize.min(16usize.max(width.saturating_sub(6)));
        let x = 1usize.max(width.saturating_sub(box_width) / 2);
        let y = 1usize.max(5usize.min(height / 7));
        let accent = "\x1b[34m";
        print!("\x1b[H\x1b[2J");
        if graphic {
            put(
                y + 1,
                x + 12,
                &crate::i18n::tr("ai.ui.nacre-ai", "Nacre AI"),
                "\x1b[34;1m",
                box_width.saturating_sub(12),
            );
        } else {
            let logo = fs::read_to_string(&text_logo).unwrap_or_else(|_| String::from("\nNacre\n"));
            let mut lines = logo.lines().take(3).map(clean).collect::<Vec<_>>();
            while lines.len() < 3 {
                lines.push(String::new());
            }
            lines[1].push_str("   Nacre AI");
            for (offset, line) in lines.iter().enumerate() {
                put(y + offset, x, line, "\x1b[34;1m", box_width);
            }
        }
        put(y + 4, x, &"─".repeat(box_width), accent, box_width);
        put(y + 6, x + 2, &title, "\x1b[2m", box_width.saturating_sub(2));
        let header_lines = header
            .map(|header| {
                header
                    .lines()
                    .take(height.saturating_sub(y + 14))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (offset, line) in header_lines.iter().enumerate() {
            put(
                y + 7 + offset,
                x + 2,
                &clean(line),
                "\x1b[2m",
                box_width.saturating_sub(2),
            );
        }
        let top = y + 8 + header_lines.len();
        let row_height = if details.is_some() { 2usize } else { 1usize };
        let room = 1usize.max(height.saturating_sub(top + 5) / row_height);
        let start = selected.saturating_sub(room.saturating_sub(1));
        for (row, index) in matches.iter().skip(start).take(room).enumerate() {
            let caption = &choices[*index];
            if matches!(caption.as_str(), "Back" | "← Back") {
                continue;
            }
            let active = start + row == selected;
            let text = format!(
                "{}{}",
                if active { "  ›  " } else { "     " },
                localized_row(caption)
            );
            if active {
                put(top + row * row_height, x, &text, "\x1b[44;1m", box_width);
            } else {
                put(top + row * row_height, x, &text, "\x1b[37m", box_width);
            }
            if let Some(detail) = details.and_then(|entries| entries.get(*index)) {
                put(
                    top + row * row_height + 1,
                    x + 5,
                    &clean(detail),
                    "\x1b[2m",
                    box_width.saturating_sub(5),
                );
            }
        }
        if back {
            let back_index = matches
                .iter()
                .position(|index| matches!(choices[*index].as_str(), "Back" | "← Back"));
            let back_active = back_index == Some(selected);
            let back_text = crate::i18n::tr("ai.ui.back", "Back");
            put(
                height.saturating_sub(4),
                x,
                &format!(
                    "{}{}",
                    if back_active { "  ›  " } else { "     " },
                    back_text
                ),
                if back_active { "\x1b[44;1m" } else { "\x1b[2m" },
                box_width,
            );
        }
        if choices.len() > 10 || !query.is_empty() {
            put(
                height.saturating_sub(2),
                x + 2,
                &format!("⌕ {query}"),
                accent,
                box_width.saturating_sub(2),
            );
        }
        if matches.is_empty()
            || matches
                .iter()
                .all(|index| matches!(choices[*index].as_str(), "Back" | "← Back"))
        {
            put(
                top,
                x + 2,
                &crate::i18n::tr("ai.ui.no-matching-tasks", "No matching tasks"),
                "\x1b[2m",
                box_width.saturating_sub(2),
            );
        }
        io::stdout().flush()?;
        if graphic {
            kitty_logo(&png_logo, y + 1, x + 12)?;
            io::stdout().flush()?;
        }
        match terminal.key()? {
            b'\n' | b'\r' => {
                if let Some(index) = matches.get(selected) {
                    if back && matches!(choices[*index].as_str(), "Back" | "← Back") {
                        return Ok(None);
                    }
                    if *index < rows.len() {
                        return Ok(Some(*index));
                    }
                    return Ok(None);
                }
            }
            3 => return Ok(None),
            0x81 => selected = (selected + matches.len().saturating_sub(1)) % matches.len().max(1),
            0x82 => selected = (selected + 1) % matches.len().max(1),
            b'k' if query.is_empty() => {
                selected = (selected + matches.len().saturating_sub(1)) % matches.len().max(1)
            }
            b'j' if query.is_empty() => selected = (selected + 1) % matches.len().max(1),
            127 | 8 => {
                query.pop();
                selected = 0;
            }
            byte if choices.len() > 10 && (byte.is_ascii_graphic() || byte == b' ') => {
                query.push((byte as char).to_ascii_lowercase());
                selected = 0;
            }
            27 => {
                if escape_cancels(&mut query) {
                    return Ok(None);
                }
                selected = 0;
            }
            _ => {}
        }
    }
}

fn launch_task(command: &str, args: &[String], title: &str) -> Result<()> {
    let mut child = Command::new("kitty");
    child.args(kitty_task_args(command, args, title));
    launch_background(child)?;
    Ok(())
}

fn launch_background(mut command: Command) -> Result<()> {
    launch_background_at(&mut command, &home())
}

fn launch_background_at(command: &mut Command, home: &std::path::Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let directory = home.join(".local/state/nacre/ai");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(directory.join("launcher.log"))?;
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log.try_clone()?))
        .stderr(std::process::Stdio::from(log))
        .process_group(0)
        .spawn()?;
    Ok(())
}

fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
}

fn projects_path() -> PathBuf {
    env::var_os("NACRE_AI_PROJECTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config/nacre/ai/projects.json"))
}

fn configured_projects() -> Result<Vec<serde_json::Value>> {
    let document: serde_json::Value = crate::state::read_as(
        &projects_path(),
        Some(crate::state::by_id("ai-projects")?),
        serde_json::json!({}),
    )?;
    let mut result = vec![serde_json::json!({
        "id":"general-chat", "label":"General chat", "chat_directory":true,
        "path":home().join(".local/share/nacre/ai/chats").display().to_string()
    })];
    result.extend(document["projects"].as_array().cloned().unwrap_or_default());
    Ok(result)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn codex_resume_args(
    id: &str,
    cwd: &str,
    brain: &str,
    full_access: bool,
    history_home: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "resume".into(),
        id.into(),
        "--sandbox".into(),
        if full_access {
            "danger-full-access"
        } else {
            "workspace-write"
        }
        .into(),
        "--ask-for-approval".into(),
        "never".into(),
        "-C".into(),
        cwd.into(),
        "--add-dir".into(),
        brain.into(),
    ];
    if let Some(history_home) = history_home {
        args.extend([
            "--no-daemon".into(),
            "-c".into(),
            format!(
                "sqlite_home={}",
                serde_json::to_string(history_home).unwrap()
            ),
        ]);
    }
    args
}

fn codex_general_args(path: &str, brain: &str, full_access: bool) -> Vec<String> {
    vec![
        "--sandbox".into(),
        if full_access {
            "danger-full-access"
        } else {
            "workspace-write"
        }
        .into(),
        "--ask-for-approval".into(),
        "never".into(),
        "-C".into(),
        path.into(),
        "--add-dir".into(),
        brain.into(),
    ]
}

fn kitty_task_args(command: &str, args: &[String], title: &str) -> Vec<String> {
    [
        vec![
            "--class".into(),
            "nacre-ai-task".into(),
            "--title".into(),
            title.into(),
            "--".into(),
            command.into(),
        ],
        args.to_vec(),
    ]
    .concat()
}

fn transient_codex_failure(stderr: &str) -> bool {
    let detail = stderr.to_lowercase();
    [
        "draining",
        "reconnect",
        "connection refused",
        "no such file",
        "startup lock",
        "timed out",
    ]
    .iter()
    .any(|needle| detail.contains(needle))
}

fn escape_cancels(query: &mut String) -> bool {
    if query.is_empty() {
        true
    } else {
        query.clear();
        false
    }
}

fn history_row(index: usize, item: &serde_json::Value) -> (String, String) {
    let title = clean(item["title"].as_str().unwrap_or("Untitled task"));
    let project = item["project"]["label"]
        .as_str()
        .or_else(|| item["project"]["id"].as_str())
        .unwrap_or("Project");
    let host = item["project"]["host"].as_str().unwrap_or("local");
    let agent = if item["agent"] == "claude" {
        "Claude Code"
    } else {
        "Codex"
    };
    let account = item["account"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or("Default");
    (
        format!("{:02}  {title}", index + 1),
        format!(
            "{project} · {host} · {agent} · {account} · {}",
            item["state"].as_str().unwrap_or("saved")
        ),
    )
}

fn sort_history(records: &mut [serde_json::Value]) {
    records.sort_by(|a, b| {
        b["updated_at"]
            .as_f64()
            .partial_cmp(&a["updated_at"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn remote_all_accounts_unsupported(stderr: &str) -> bool {
    stderr.contains("unrecognized arguments: --all-accounts")
}

fn run_remote(host: &str, command: &[String], capture: bool) -> Result<std::process::Output> {
    run_remote_with(std::path::Path::new("ssh"), host, command, capture)
}

fn run_remote_with(
    executable: &std::path::Path,
    host: &str,
    command: &[String],
    capture: bool,
) -> Result<std::process::Output> {
    let script = command
        .iter()
        .map(|part| {
            part.strip_prefix("$HOME/")
                .map(|rest| format!("\"$HOME/{rest}\""))
                .unwrap_or_else(|| shell_quote(part))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut ssh = Command::new(executable);
    if !capture {
        ssh.arg("-t");
    }
    ssh.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=5",
        host,
        &script,
    ])
    .env("TERM", "xterm-256color");
    if capture {
        Ok(ssh.output()?)
    } else {
        let status = ssh.status()?;
        Ok(std::process::Output {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }
}

fn provider_list(
    project: &serde_json::Value,
    agent: &str,
    account: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut args = if agent == "claude" {
        vec!["nacre-ai-claude".to_owned(), "list".to_owned()]
    } else {
        vec![
            home()
                .join(".local/bin/nacre-ai-chat")
                .display()
                .to_string(),
            "list".to_owned(),
        ]
    };
    for (key, value) in [
        ("--project", project["id"].as_str().unwrap_or("")),
        ("--path", project["path"].as_str().unwrap_or("")),
        ("--account", account),
    ] {
        args.push(key.to_owned());
        args.push(value.to_owned());
    }
    if agent == "codex" {
        args.push("--all-accounts".into());
    }
    if project["chat_directory"] == true {
        args.push("--chat-directory".into());
    }
    let remote_host = project["host"].as_str();
    let mut output = if let Some(host) = remote_host {
        let remote = args
            .iter()
            .map(|part| {
                if part == "nacre-ai-claude" {
                    "\u{24}HOME/.local/bin/nacre-ai-claude".to_owned()
                } else if part.contains("/.local/bin/nacre-ai-chat") {
                    "\u{24}HOME/.local/bin/nacre-ai-chat".to_owned()
                } else {
                    part.clone()
                }
            })
            .collect::<Vec<_>>();
        run_remote(host, &remote, true)?
    } else {
        let mut command = Command::new(&args[0]);
        command.args(&args[1..]);
        if agent == "codex" {
            let delta = ai_core::environment_delta("codex", Some(account))?;
            if let Some(path) = delta["set"]["CODEX_HOME"].as_str() {
                command.env("CODEX_HOME", path);
            }
        } else {
            let delta = ai_core::environment_delta("claude", Some(account))?;
            if let Some(path) = delta["set"]["CLAUDE_CONFIG_DIR"].as_str() {
                command.env("CLAUDE_CONFIG_DIR", path);
            }
        }
        command.output()?
    };
    if !output.status.success() && agent == "codex" {
        let unsupported = remote_all_accounts_unsupported(&String::from_utf8_lossy(&output.stderr));
        if let Some(host) = remote_host.filter(|_| unsupported) {
            args.retain(|argument| argument != "--all-accounts");
            let remote = args
                .iter()
                .map(|part| {
                    if part == "nacre-ai-claude" {
                        "$HOME/.local/bin/nacre-ai-claude".to_owned()
                    } else if part.contains("/.local/bin/nacre-ai-chat") {
                        "$HOME/.local/bin/nacre-ai-chat".to_owned()
                    } else {
                        part.clone()
                    }
                })
                .collect::<Vec<_>>();
            output = run_remote(host, &remote, true)?;
        }
    }
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()
            .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let mut items = value
        .as_array()
        .cloned()
        .ok_or("Invalid history response")?;
    for item in &mut items {
        item["project"] = project.clone();
        item["agent"] = serde_json::json!(agent);
        item["account"] = item
            .get("account")
            .and_then(serde_json::Value::as_str)
            .map_or_else(
                || serde_json::json!(account),
                |value| serde_json::json!(value),
            );
    }
    Ok(items)
}

fn history(command: &str, account_override: Option<&str>) -> Result<()> {
    if let Some(status) = launch_niri_history(command, account_override)? {
        return if status.success() {
            Ok(())
        } else {
            Err("History worker did not finish successfully".into())
        };
    }
    let mut records = Vec::new();
    let mut errors = Vec::new();
    for project in configured_projects()? {
        let mut sources = vec![project.clone()];
        for source in project["task_sources"].as_array().into_iter().flatten() {
            if let (Some(base), Some(extra)) = (project.as_object(), source.as_object()) {
                let mut merged = base.clone();
                merged.extend(extra.clone());
                sources.push(serde_json::Value::Object(merged));
            }
        }
        for source in sources {
            for agent in ["codex", "claude"] {
                let account = ai_core::effective_account(agent, account_override)?;
                match provider_list(&source, agent, &account) {
                    Ok(rows) => records.extend(rows),
                    Err(error) => errors.push(error.to_string()),
                }
            }
        }
    }
    sort_history(&mut records);
    if command == "latest" && !errors.is_empty() {
        return Err(format!("Cannot establish the latest task while a history source is unavailable; use Load chat. {}", errors.join("; ")).into());
    }
    if records.is_empty() {
        let message = if errors.is_empty() {
            "No saved tasks yet"
        } else {
            "History unavailable"
        };
        let header = (!errors.is_empty()).then(|| errors.join("; "));
        let _ = select_with(message, &["Back"], true, header.as_deref(), None)?;
        return Ok(());
    }
    let selected = if command == "latest" {
        Some(0)
    } else {
        let (labels, details): (Vec<_>, Vec<_>) = records
            .iter()
            .enumerate()
            .map(|(index, item)| history_row(index, item))
            .unzip();
        let refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        let warning = (!errors.is_empty()).then(|| {
            crate::i18n::tr(
                "ai.ui.some-history-sources-unavailable",
                "Some history sources unavailable",
            )
        });
        select_with("Load chat", &refs, true, warning.as_deref(), Some(&details))?
    };
    if let Some(index) = selected {
        set_terminal_title(records[index]["title"].as_str().unwrap_or("Load chat"));
        resume_record(&records[index])?;
    }
    Ok(())
}

fn launch_niri_history(
    action: &str,
    account_override: Option<&str>,
) -> Result<Option<std::process::ExitStatus>> {
    if !env::var("NACRE_COMPOSITOR").is_ok_and(|compositor| compositor == "niri") {
        return Ok(None);
    }
    let mapping = match fs::read_to_string("/proc/self/uid_map") {
        Ok(mapping) => mapping,
        Err(_) => return Ok(None),
    };
    if mapping
        .lines()
        .any(|row| row.split_whitespace().nth(1) == Some("0"))
    {
        return Ok(None);
    }
    if Command::new("systemd-run")
        .arg("--version")
        .output()
        .is_err()
    {
        return Ok(None);
    }
    let mut runner = Command::new("systemd-run");
    let cwd = env::current_dir()?;
    runner.args([
        "--user",
        "--quiet",
        "--wait",
        "--collect",
        if io::stdin().is_terminal() {
            "--pty"
        } else {
            "--pipe"
        },
    ]);
    runner.arg(format!("--working-directory={}", cwd.display()));
    for name in [
        "PATH",
        "TERM",
        "LANG",
        "LC_ALL",
        "SSH_AUTH_SOCK",
        "CODEX_HOME",
        "NACRE_AI_PROJECTS",
        "NACRE_BRAIN",
        "NACRE_BRAIN_DIR",
        "NACRE_COMPOSITOR",
    ] {
        if let Some(value) = env::var_os(name) {
            runner.arg(format!("--setenv={name}={}", value.to_string_lossy()));
        }
    }
    runner
        .arg("--")
        .arg(env::current_exe()?)
        .arg("nacre-ai")
        .arg(action);
    if let Some(account) = account_override {
        runner.args(["--account", account]);
    }
    Ok(Some(runner.status()?))
}

fn resume_record(item: &serde_json::Value) -> Result<()> {
    let project = &item["project"];
    let agent = item["agent"].as_str().unwrap_or("codex");
    let account = item["account"].as_str().unwrap_or("");
    let id = item["id"].as_str().ok_or("History record has no id")?;
    if let (Some(host), Some(session)) = (project["host"].as_str(), item["session"].as_str()) {
        let output = run_remote(
            host,
            &[
                "tmux".into(),
                "attach-session".into(),
                "-t".into(),
                format!("={session}"),
            ],
            false,
        )?;
        if !output.status.success() {
            return Err("Could not attach to the running task".into());
        }
        return Ok(());
    }
    if agent == "claude" {
        let mut args = vec![
            "nacre-ai-claude".into(),
            "resume".into(),
            "--project".into(),
            project["id"].as_str().unwrap_or("").into(),
            "--path".into(),
            item["cwd"]
                .as_str()
                .or(project["path"].as_str())
                .unwrap_or("")
                .into(),
            "--account".into(),
            account.into(),
            "--thread-id".into(),
            id.into(),
        ];
        if project["chat_directory"] == true {
            args.push("--chat-directory".into());
        }
        if let Some(host) = project["host"].as_str() {
            args.push("--tmux".into());
            let output = run_remote(host, &args, false)?;
            if !output.status.success() {
                return Err("Could not resume Claude session".into());
            }
        } else {
            if Command::new("tmux")
                .arg("-V")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
            {
                args.push("--tmux".into());
            }
            let status = Command::new(&args[0]).args(&args[1..]).status()?;
            if !status.success() {
                return Err("Could not resume Claude session".into());
            }
        }
    } else if let Some(host) = project["host"].as_str() {
        let mut args = vec![
            "$HOME/.local/bin/nacre-ai-chat".into(),
            "resume".into(),
            "--thread-id".into(),
            id.into(),
            "--project".into(),
            project["id"].as_str().unwrap_or("").into(),
            "--account".into(),
            account.into(),
        ];
        if let Some(history_home) = item["history_home"].as_str() {
            args.extend(["--history-home".into(), history_home.into()]);
        }
        let output = run_remote(host, &args, false)?;
        if !output.status.success() {
            return Err("Could not resume Codex task".into());
        }
    } else {
        let executable = home().join(".local/bin/codex");
        let history_home = item["history_home"]
            .as_str()
            .filter(|_| item["shared_history"] == false);
        let args = codex_resume_args(
            id,
            item["cwd"]
                .as_str()
                .or(project["path"].as_str())
                .unwrap_or(""),
            &env::var("NACRE_BRAIN")
                .unwrap_or_else(|_| home().join("Documents/Nacre-Brain").display().to_string()),
            ai_core::full_access(),
            history_home,
        );
        let delta = ai_core::environment_delta("codex", Some(account))?;
        let mut command = Command::new(executable);
        command.args(args);
        let code_home = delta["set"]["CODEX_HOME"].as_str().unwrap_or("").to_owned();
        if !code_home.is_empty() {
            command.env("CODEX_HOME", &code_home);
        }
        if let Some(history_home) = history_home {
            command.env("NACRE_AI_HISTORY_HOME", history_home);
        }
        prepare_codex(command.get_program().to_string_lossy().as_ref(), &code_home)?;
        let status = command.status()?;
        if !status.success() {
            return Err("Could not resume Codex task".into());
        }
    }
    Ok(())
}

fn argument(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
}

fn create_general_chat_directory() -> Result<PathBuf> {
    create_general_chat_directory_at(&home())
}

fn create_general_chat_directory_at(project_home: &std::path::Path) -> Result<PathBuf> {
    let root = project_home.join(".local/share/nacre/ai/chats");
    fs::create_dir_all(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let directory = root.join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    const GUIDANCE: &str = "# General chat\n\nThis is a general conversation, not an assigned coding project. Answer questions normally. Do not create a project or change project files merely because they are mentioned.\n\nWhen the user asks to work on a project later in this conversation, read ~/.config/nacre/ai/projects.json and resolve the requested project there. Read that project's AGENTS.md and relevant specifications before acting. Use explicit command working directories and an isolated task worktree for edits. Retain this conversation; do not launch a second AI just to change projects. The user has configured access according to the selected profile; keep changes scoped to their request. Respect build_host/build_path for heavy builds.\n\nUse nacre-ai-context QUERY to find project candidates and routing; do not silently guess an ambiguous target. Use nacre-ai-memory with --world and --topic to capture meaningful durable knowledge from any subject, including personal interests and research, not just code. Read ~/.agents/skills/nacre-brain/SKILL.md.\n";
    let guidance = directory.join("AGENTS.md");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&guidance)?;
    file.write_all(GUIDANCE.as_bytes())?;
    file.sync_all()?;
    Ok(directory)
}

fn trust_general_chat(directory: &std::path::Path, code_home: &std::path::Path) -> Result<()> {
    trust_general_chat_at(
        &home().join(".local/share/nacre/ai/chats"),
        directory,
        code_home,
    )
}

fn trust_general_chat_at(
    root: &std::path::Path,
    directory: &std::path::Path,
    code_home: &std::path::Path,
) -> Result<()> {
    use nix::fcntl::{Flock, FlockArg};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if directory.parent() != Some(root)
        || !directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.len() == 32
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
        || fs::symlink_metadata(directory)?.file_type().is_symlink()
        || fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?
            != [std::ffi::OsString::from("AGENTS.md")]
    {
        return Err("Not an owned chat folder".into());
    }
    match fs::symlink_metadata(code_home) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("Unsafe Codex home".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(code_home)?;
            fs::set_permissions(code_home, fs::Permissions::from_mode(0o700))?;
        }
        Err(error) => return Err(error.into()),
    }
    let lock_path = code_home.join("nacre-config.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&lock_path)?;
    let metadata = lock.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err("Unsafe Codex configuration lock".into());
    }
    let _guard = Flock::lock(lock, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
    let config = code_home.join("config.toml");
    if let Ok(metadata) = fs::symlink_metadata(&config) {
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.uid() != nix::unistd::getuid().as_raw()
        {
            return Err("Refusing unsafe Codex settings file".into());
        }
    }
    let original = match fs::read_to_string(&config) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let parsed = if original.is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        toml::from_str::<toml::Value>(&original)?
    };
    let directory = directory.to_string_lossy();
    if parsed
        .get("projects")
        .and_then(|projects| projects.get(directory.as_ref()))
        .is_some()
    {
        return Ok(());
    }
    let section = serde_json::to_string(directory.as_ref())?;
    let updated = format!("{original}\n[projects.{section}]\ntrust_level = \"trusted\"\n");
    let _: toml::Value = toml::from_str(&updated)?;
    let current = match fs::read_to_string(&config) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    if current != original {
        return Err("Codex settings changed concurrently; retry New chat".into());
    }
    crate::files::atomic(&config, updated.as_bytes())
}

fn launch_general_chat(account: &str) -> Result<i32> {
    let directory = create_general_chat_directory()?;
    let environment = ai_core::local_environment_delta(Some(account))?;
    let code_home = environment["set"]["CODEX_HOME"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".codex"));
    trust_general_chat(&directory, &code_home)?;
    let brain = env::var("NACRE_BRAIN")
        .unwrap_or_else(|_| home().join("Documents/Nacre-Brain").display().to_string());
    let executable = home().join(".local/bin/codex");
    let mut command = Command::new(&executable);
    command.args(codex_general_args(
        &directory.display().to_string(),
        &brain,
        ai_core::full_access(),
    ));
    if let Some(profile) = environment["set"]["CODEX_HOME"].as_str() {
        command.env("CODEX_HOME", profile);
    }
    prepare_codex(
        &executable.display().to_string(),
        environment["set"]["CODEX_HOME"].as_str().unwrap_or(""),
    )?;
    let status = command.status()?;
    Ok(status.code().unwrap_or(1))
}

fn run_project(command_name: &str, args: &[String]) -> Result<i32> {
    let project_id = argument(args, "--project");
    let agent = argument(args, "--agent").unwrap_or(ai_core::default_assistant()?);
    let account = argument(args, "--account");
    let project = if command_name == "new" && project_id.is_none() {
        configured_projects()?
            .into_iter()
            .next()
            .ok_or("General chat unavailable")?
    } else {
        configured_projects()?
            .into_iter()
            .find(|project| project["id"].as_str() == project_id.as_deref())
            .ok_or("Project is not registered")?
    };
    let path = project["path"].as_str().ok_or("Project path is missing")?;
    let label = project["label"]
        .as_str()
        .or_else(|| project["id"].as_str())
        .unwrap_or("Project");
    let location = project["host"].as_str().unwrap_or("this computer");
    if command_name == "shell" {
        let shell = env::var("SHELL").unwrap_or_else(|_| String::from("/bin/bash"));
        println!("Project terminal · {label} · {location}");
        println!(
            "This is a command prompt, not an AI chat. Type exit or press Ctrl+D to close it."
        );
        if let Some(host) = project["host"].as_str() {
            let status = run_remote(
                host,
                &[
                    "sh".into(),
                    "-lc".into(),
                    format!(
                        "cd {} && exec \"${{SHELL:-/bin/sh}}\" -l",
                        shell_quote(path)
                    ),
                ],
                false,
            )?;
            return Ok(status.status.code().unwrap_or(1));
        }
        std::env::set_current_dir(path)?;
        let status = Command::new(shell).arg("-l").status()?;
        return Ok(status.code().unwrap_or(1));
    }
    if project["chat_directory"] == true {
        println!(
            "{}",
            crate::i18n::tr("ai.ui.nacre-ai-new-chat", "Nacre AI · New chat")
        );
    } else {
        println!("AI workspace · {label} · {location}");
    }
    println!("Type your request when the selected assistant opens. The dashboard stays in its own window.");
    if project.get("host").is_some() {
        println!(
            "To leave a running task: Ctrl+B, then D. Return with Resume latest chat or Load chat."
        );
    }
    if agent == "claude" {
        let executable = "nacre-ai-claude";
        let mut command = Command::new(executable);
        command.args([
            "new",
            "--project",
            project["id"].as_str().unwrap_or(""),
            "--path",
            path,
            "--account",
            account.as_deref().unwrap_or(""),
        ]);
        if project["chat_directory"] == true {
            command.arg("--chat-directory");
        }
        if let Some(host) = project["host"].as_str() {
            let mut remote = vec![
                "$HOME/.local/bin/nacre-ai-claude".into(),
                "new".into(),
                "--project".into(),
                project["id"].as_str().unwrap_or("").into(),
                "--path".into(),
                path.into(),
                "--account".into(),
                account.unwrap_or_default(),
            ];
            if project["chat_directory"] == true {
                remote.push("--chat-directory".into());
            }
            remote.push("--tmux".into());
            let status = run_remote(host, &remote, false)?;
            return Ok(status.status.code().unwrap_or(1));
        }
        if Command::new("tmux")
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            command.arg("--tmux");
        }
        let status = command.status()?;
        return Ok(status.code().unwrap_or(1));
    }
    if let Some(host) = project["host"].as_str() {
        let account = ai_core::effective_account("codex", account.as_deref())?;
        let helper = project["remote_helper"]
            .as_str()
            .unwrap_or("$HOME/.local/bin/nacre-ai-remote");
        let status = run_remote(
            host,
            &[
                helper.into(),
                project["id"].as_str().unwrap_or("").into(),
                path.into(),
                account,
            ],
            false,
        )?;
        return Ok(status.status.code().unwrap_or(1));
    }
    let account = ai_core::effective_account("codex", account.as_deref())?;
    let delta = ai_core::local_environment_delta(Some(&account))?;
    let mut command = Command::new(home().join(".local/bin/codex"));
    let mut args = vec![
        "--sandbox".into(),
        if ai_core::full_access() {
            "danger-full-access"
        } else {
            "workspace-write"
        }
        .into(),
        "--ask-for-approval".into(),
        "never".into(),
    ];
    if project["chat_directory"] == true {
        return launch_general_chat(&account);
    }
    args.extend([
        "--worktree".into(),
        "-C".into(),
        path.into(),
        "--add-dir".into(),
        env::var("NACRE_BRAIN")
            .unwrap_or_else(|_| home().join("Documents/Nacre-Brain").display().to_string()),
    ]);
    command.args(args);
    let code_home = delta["set"]["CODEX_HOME"].as_str().unwrap_or("").to_owned();
    if !code_home.is_empty() {
        command.env("CODEX_HOME", &code_home);
    }
    let executable = command.get_program().to_string_lossy().into_owned();
    prepare_codex(&executable, &code_home)?;
    let status = command.status()?;
    Ok(status.code().unwrap_or(1))
}

fn prepare_codex(executable: &str, profile: &str) -> Result<()> {
    for attempt in 0..3 {
        let mut command = Command::new(executable);
        command.args(["app-server", "daemon", "start"]);
        if !profile.is_empty() {
            command.env("CODEX_HOME", profile);
        }
        let result = command.output();
        match result {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                if !transient_codex_failure(&String::from_utf8_lossy(&output.stderr)) {
                    return Err("Codex could not prepare its shared server. Run codex doctor; existing tasks were left running.".into());
                }
            }
            Err(error) if attempt == 2 => return Err(error.into()),
            Err(_) => {}
        }
        if attempt < 2 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    Err("Codex's shared server is still transitioning. Existing tasks were left running; retry shortly or inspect codex doctor.".into())
}

fn dashboard(account_override: Option<&str>) -> Result<()> {
    loop {
        match select("Workspace", &DASHBOARD_OPTIONS, false)? {
            Some(0) => {
                let agent = ai_core::default_assistant()?;
                let mut args = vec![
                    "new".into(),
                    "--project".into(),
                    "general-chat".into(),
                    "--agent".into(),
                    agent,
                ];
                if let Some(account) = account_override {
                    args.extend(["--account".into(), account.into()]);
                }
                launch_task("nacre-ai", &args, "New chat")?;
            }
            Some(1) => {
                let mut args = vec!["latest".into()];
                if let Some(account) = account_override {
                    args.extend(["--account".into(), account.into()]);
                }
                launch_task("nacre-ai", &args, "Load chat")?;
            }
            Some(2) => {
                let mut args = vec!["sessions".into()];
                if let Some(account) = account_override {
                    args.extend(["--account".into(), account.into()]);
                }
                launch_task("nacre-ai", &args, "Load chat")?;
            }
            Some(3) => {
                let program = PathBuf::from(env::var_os("HOME").unwrap_or_default())
                    .join(".local/bin/nacre-brain-ui");
                let mut command = Command::new(program);
                command.arg("brain");
                launch_background(command)?;
            }
            Some(4) => settings()?,
            Some(5) => return Ok(()),
            None => continue,
            _ => {}
        }
    }
}

fn settings_path() -> PathBuf {
    env::var_os("NACRE_AI_SETTINGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default())
                .join(".config/nacre/ai/settings.json")
        })
}

fn set_preference(key: &str, value: serde_json::Value) -> Result<()> {
    let path = settings_path();
    crate::state::update_as(
        &path,
        crate::state::by_id("ai-settings")?,
        |mut settings| {
            settings[key] = value;
            Ok(settings)
        },
    )
}

fn accounts(agent: &str) -> Result<()> {
    let root = PathBuf::from(env::var_os("HOME").unwrap_or_default())
        .join(".local/share/nacre/ai")
        .join(if agent == "claude" {
            "claude-accounts"
        } else {
            "accounts"
        });
    let active = ai_core::effective_account(agent, None)?;
    let mut profiles = vec![String::new()];
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let file_name = entry.file_name();
                if let Some(name) = file_name.to_str() {
                    if valid_account(name) {
                        profiles.push(name.to_owned());
                    }
                }
            }
        }
    }
    profiles[1..].sort_by_key(|name| name.to_lowercase());
    loop {
        let mut labels = profiles
            .iter()
            .map(|name| {
                format!(
                    "{}{}",
                    if name.is_empty() { "Default" } else { name },
                    if name == &active { "  ✓" } else { "" }
                )
            })
            .collect::<Vec<_>>();
        labels.push(crate::i18n::tr("ai.ui.add-account", "Add account"));
        labels.push(crate::i18n::tr(
            "ai.ui.sign-in-to-selected-account",
            "Sign in to selected account",
        ));
        let refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(index) = select("Accounts", &refs, true)? else {
            return Ok(());
        };
        if index < profiles.len() {
            set_account(agent, &profiles[index])?;
            return Ok(());
        }
        if index == profiles.len() {
            let Some(name) = prompt_project_name()? else {
                continue;
            };
            let account = project_id(&name);
            if !valid_account(&account) {
                let _ = select("Account name", &["Use letters, digits, _ or -"], true)?;
                continue;
            }
            if profiles.iter().any(|profile| profile == &account) {
                let _ = select("Account name", &["That account already exists"], true)?;
                continue;
            }
            let profile_path = home()
                .join(".local/share/nacre/ai")
                .join(if agent == "claude" {
                    "claude-accounts"
                } else {
                    "accounts"
                })
                .join(&account);
            let account_root = profile_path.parent().ok_or("Invalid account path")?;
            fs::create_dir_all(account_root)?;
            fs::set_permissions(account_root, fs::Permissions::from_mode(0o700))?;
            fs::create_dir(&profile_path)?;
            fs::set_permissions(&profile_path, fs::Permissions::from_mode(0o700))?;
            profiles.push(account.clone());
            launch_account_login(agent, &account)?;
            return Ok(());
        }
        let current = ai_core::effective_account(agent, None)?;
        launch_account_login(agent, &current)?;
        return Ok(());
    }
}

fn set_account(agent: &str, account: &str) -> Result<()> {
    let path = settings_path();
    crate::state::update_as(
        &path,
        crate::state::by_id("ai-settings")?,
        |mut settings| {
            update_account_settings(&mut settings, agent, account);
            Ok(settings)
        },
    )
}

fn update_account_settings(settings: &mut serde_json::Value, agent: &str, account: &str) {
    if !settings
        .get("accounts")
        .is_some_and(serde_json::Value::is_object)
    {
        settings["accounts"] = serde_json::json!({});
    }
    settings["accounts"][agent] = serde_json::json!(account);
}

fn launch_account_login(agent: &str, account: &str) -> Result<()> {
    let mut kitty = Command::new("kitty");
    kitty.args([
        "--class",
        "nacre-ai-login",
        "--title",
        if agent == "claude" {
            "Claude Code sign in"
        } else {
            "Codex sign in"
        },
        "--",
        "nacre-ai",
        "login",
        "--agent",
        agent,
        "--account",
        account,
        "--wait-on-close",
    ]);
    launch_background(kitty)
}

fn usage() -> Result<()> {
    let helper =
        PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/bin/nacre-ai-usage");
    loop {
        let mut lines = Vec::new();
        for agent in ["codex", "claude"] {
            let account = ai_core::effective_account(agent, None).unwrap_or_default();
            match Command::new(&helper)
                .args(["read", "--agent", agent, "--account", &account])
                .output()
            {
                Ok(output) if output.status.success() => {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                        lines.extend(
                            value["rows"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(serde_json::Value::as_str)
                                .map(str::to_owned),
                        );
                    }
                }
                _ => lines.push(format!("{agent}: usage unavailable")),
            }
        }
        if lines.is_empty() {
            lines.push(String::from("Usage unavailable"));
        }
        lines.push(crate::i18n::tr("ai.ui.refresh", "Refresh"));
        let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
        match select("Usage", &refs, true)? {
            Some(index) if index + 1 == lines.len() => continue,
            _ => return Ok(()),
        }
    }
}

fn projects() -> Result<()> {
    let path = env::var_os("NACRE_AI_PROJECTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default())
                .join(".config/nacre/ai/projects.json")
        });
    let projects: serde_json::Value = crate::state::read_as(
        &path,
        Some(crate::state::by_id("ai-projects")?),
        serde_json::json!({}),
    )?;
    let rows = projects["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|project| {
            format!(
                "{} · {}",
                project["label"]
                    .as_str()
                    .or_else(|| project["id"].as_str())
                    .unwrap_or("Project"),
                project["host"].as_str().unwrap_or("local")
            )
        })
        .collect::<Vec<_>>();
    let mut menu = vec![String::from("Add project")];
    if rows.is_empty() {
        menu.push(String::from("No projects configured"));
    } else {
        menu.extend(rows);
    }
    let refs = menu.iter().map(String::as_str).collect::<Vec<_>>();
    if select("Projects", &refs, true)? == Some(0) {
        if let Some(name) = prompt_project_name()? {
            let confirm = [format!("Create project · {} · ~/Projects", clean(&name))];
            if select("New local project", &confirm, true)?.is_some() {
                if let Err(error) = create_project(&name) {
                    let message = error.to_string();
                    let _ = select(&message, &["Back"], true)?;
                }
            }
        }
    }
    Ok(())
}

fn prompt_project_name() -> Result<Option<String>> {
    let terminal = Terminal::enter()?;
    let mut name = String::new();
    let mut back_selected = false;
    loop {
        let (height, width) = terminal_size();
        let x = 2usize;
        print!("\x1b[H\x1b[2J");
        put(2, x, "NEW PROJECT", "\x1b[2m", width.saturating_sub(x + 2));
        put(
            4,
            x,
            &format!("{}{}", crate::i18n::tr("ai.ui.name", "Name: "), name),
            "\x1b[34m",
            width.saturating_sub(x + 2),
        );
        let slug = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let slug = slug
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        put(
            6,
            x,
            &format!(
                "{}{}",
                crate::i18n::tr("ai.ui.folder-projects", "Folder: ~/Projects/"),
                slug
            ),
            "\x1b[2m",
            width.saturating_sub(x + 2),
        );
        put(
            8,
            x,
            &format!(
                "{}{}",
                if back_selected { "› " } else { "  " },
                crate::i18n::tr("ai.ui.back", "Back")
            ),
            if back_selected {
                "\x1b[44;1m"
            } else {
                "\x1b[2m"
            },
            width.saturating_sub(x + 2),
        );
        put(
            height.saturating_sub(4),
            x,
            &crate::i18n::tr(
                "ai.ui.enter-continue-tab-switch-esc-back",
                "Enter continue · Tab switch · Esc back",
            ),
            "\x1b[2m",
            width.saturating_sub(x + 2),
        );
        io::stdout().flush()?;
        match terminal.key()? {
            b'\n' | b'\r' if back_selected => return Ok(None),
            b'\n' | b'\r' if !name.trim().is_empty() => return Ok(Some(name.trim().to_owned())),
            b'\t' => back_selected = !back_selected,
            27 | 3 => return Ok(None),
            127 | 8 if !back_selected => {
                name.pop();
            }
            byte if !back_selected
                && (byte.is_ascii_graphic() || byte == b' ')
                && name.len() < 64 =>
            {
                name.push(byte as char)
            }
            _ => {}
        }
    }
}

fn project_id(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn create_project(name: &str) -> Result<serde_json::Value> {
    create_project_at(name, &home(), &projects_path())
}

fn create_project_at(
    name: &str,
    project_home: &std::path::Path,
    registry: &std::path::Path,
) -> Result<serde_json::Value> {
    let name = clean(name);
    let id = project_id(&name);
    if id == "general-chat" {
        return Err("General chat already exists in New chat; choose a project name".into());
    }
    if id.is_empty() || id.len() > 64 {
        return Err("Use a project name containing letters or numbers".into());
    }
    let folder = project_home.join("Projects").join(&id);
    fs::create_dir_all(folder.parent().ok_or("Invalid project path")?)?;
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let initialized = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&folder)
        .status();
    if !initialized.is_ok_and(|status| status.success()) {
        fs::remove_dir_all(&folder)?;
        return Err("Could not initialize project repository".into());
    }
    let project = serde_json::json!({"id":id,"label":name,"path":folder.display().to_string()});
    let result = crate::state::update_as(
        registry,
        crate::state::by_id("ai-projects")?,
        |mut document| {
            if document["projects"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == id))
            {
                return Err("That project already exists".into());
            }
            if !document["projects"].is_array() {
                document["projects"] = serde_json::json!([]);
            }
            document["projects"]
                .as_array_mut()
                .unwrap()
                .push(project.clone());
            Ok(document)
        },
    );
    result?;
    Ok(project)
}

fn setup_status() -> Vec<String> {
    let mut rows = Vec::new();
    for (agent, label, command_args) in [
        ("codex", "Codex", vec!["login", "status"]),
        ("claude", "Claude Code", vec!["auth", "status"]),
    ] {
        if Command::new(agent).arg("--version").output().is_err() {
            rows.push(format!("{label}: not installed"));
            continue;
        }
        let Ok(account) = ai_core::effective_account(agent, None) else {
            rows.push(format!("{label}: could not check login"));
            continue;
        };
        let account_label = if account.is_empty() {
            "Default"
        } else {
            &account
        };
        let Ok(delta) = ai_core::environment_delta(agent, Some(&account)) else {
            rows.push(format!("{label} · {account_label}: could not check login"));
            continue;
        };
        let mut command = Command::new(agent);
        command.args(command_args);
        for (name, value) in [
            ("CODEX_HOME", delta["set"]["CODEX_HOME"].as_str()),
            (
                "CLAUDE_CONFIG_DIR",
                delta["set"]["CLAUDE_CONFIG_DIR"].as_str(),
            ),
        ] {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        let state = match command.output() {
            Ok(output) if output.status.success() => "signed in",
            Ok(_) => "not signed in",
            Err(_) => "could not check login",
        };
        rows.push(format!("{label} · {account_label}: {state}"));
    }
    let vault = env::var_os("NACRE_BRAIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("Documents/Nacre-Brain"));
    rows.push(format!(
        "Shared brain: {}",
        if vault.is_dir() {
            "available"
        } else {
            "missing"
        }
    ));
    rows
}

fn settings() -> Result<()> {
    loop {
        let current = ai_core::default_assistant()?;
        let rows = [
            format!(
                "Assistant · {}",
                if current == "claude" {
                    "Claude Code"
                } else {
                    "Codex"
                }
            ),
            String::from("Usage"),
            String::from("Accounts"),
            String::from("Projects"),
            String::from("Check setup"),
        ];
        let refs = rows.iter().map(String::as_str).collect::<Vec<_>>();
        match select("Settings", &refs, true)? {
            Some(0) => {
                let choices = ["Codex", "Claude Code"];
                if let Some(index) = select("Default assistant", &choices, true)? {
                    set_preference(
                        "default_assistant",
                        serde_json::json!(if index == 0 { "codex" } else { "claude" }),
                    )?;
                }
            }
            Some(1) => usage()?,
            Some(2) => {
                let choices = ["Codex", "Claude Code"];
                if let Some(index) = select("Select account provider", &choices, true)? {
                    accounts(if index == 0 { "codex" } else { "claude" })?;
                }
            }
            Some(3) => projects()?,
            Some(4) => {
                let rows = setup_status();
                let refs = rows.iter().map(String::as_str).collect::<Vec<_>>();
                let _ = select("Setup status", &refs, true)?;
            }
            Some(_) | None => return Ok(()),
        }
    }
}

pub fn run(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("usage: nacre-ai [-h] [--project PROJECT] [--agent {{codex,claude}}]\n                [--account ACCOUNT]\n                [{{dashboard,new,latest,sessions,shell,brain,login,projects,window,account-login,settings}}]\n\nLaunch independent local or SSH Codex tasks, sessions, and the personal vault.\n\npositional arguments:\n  {{dashboard,new,latest,sessions,shell,brain,login,projects,window,account-login,settings}}\n\noptions:\n  -h, --help            show this help message and exit\n  --project PROJECT\n  --agent {{codex,claude}}\n  --account ACCOUNT     Account profile override; Codex accounts share chats\n                        and keep separate logins\n");
        return Ok(0);
    }
    let mut index = 0;
    let command = if let Some(value) = args.first().filter(|value| !value.starts_with('-')) {
        index = 1;
        value.as_str()
    } else {
        "dashboard"
    };
    if args.iter().any(|arg| arg == "--agent") && argument(args, "--agent").is_none() {
        return Ok(2);
    }
    if argument(args, "--agent").is_some_and(|agent| !matches!(agent.as_str(), "codex" | "claude"))
    {
        return Ok(2);
    }
    if args.iter().any(|arg| arg == "--account") && argument(args, "--account").is_none() {
        return Ok(2);
    }
    if argument(args, "--account")
        .is_some_and(|account| !account.is_empty() && !valid_account(&account))
    {
        return Err("Invalid account".into());
    }
    let mut account_override = None;
    while command == "dashboard" && index < args.len() {
        match args[index].as_str() {
            "--account" => {
                account_override = args.get(index + 1).map(String::as_str);
                if account_override.is_none() {
                    return Err("--account requires a value".into());
                }
                index += 2;
            }
            "--agent" | "--project" => {
                if args.get(index + 1).is_none() {
                    return Ok(2);
                }
                index += 2;
            }
            other if other.starts_with('-') => {
                return Err(format!("Unknown nacre-ai option: {other}").into())
            }
            _ => return Err(format!("Unexpected nacre-ai argument: {}", args[index]).into()),
        }
    }
    match command {
        "dashboard" | "" => dashboard(account_override)?,
        "projects" => {
            let path = env::var_os("NACRE_AI_PROJECTS")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env::var_os("HOME").unwrap_or_default())
                        .join(".config/nacre/ai/projects.json")
                });
            let data: serde_json::Value = crate::state::read_as(
                &path,
                Some(crate::state::by_id("ai-projects")?),
                serde_json::json!({}),
            )?;
            for project in data["projects"].as_array().into_iter().flatten() {
                println!(
                    "{}: {} ({})",
                    project["id"].as_str().unwrap_or(""),
                    project["label"].as_str().unwrap_or(""),
                    project["host"].as_str().unwrap_or("local")
                );
            }
        }
        "settings" => settings()?,
        "latest" | "sessions" => {
            history(command, argument(&args[index..], "--account").as_deref())?
        }
        "new" | "shell" => return run_project(command, &args[index..]),
        "login" => {
            let login_args = &args[index..];
            let agent = argument(login_args, "--agent").unwrap_or(ai_core::default_assistant()?);
            if !matches!(agent.as_str(), "codex" | "claude") {
                return Err("Unknown assistant".into());
            }
            let account = argument(login_args, "--account")
                .unwrap_or(ai_core::effective_account(&agent, None)?);
            if !account.is_empty() && !valid_account(&account) {
                return Err("Invalid account".into());
            }
            let status = if agent == "claude" {
                Command::new("nacre-ai-claude")
                    .args([
                        "login",
                        "--project",
                        "general-chat",
                        "--path",
                        &home()
                            .join(".local/share/nacre/ai/chats")
                            .display()
                            .to_string(),
                        "--account",
                        &account,
                    ])
                    .status()?
            } else {
                let mut command = Command::new(home().join(".local/bin/codex"));
                command.arg("login");
                let delta = ai_core::environment_delta("codex", Some(&account))?;
                if let Some(profile) = delta["set"]["CODEX_HOME"].as_str() {
                    command.env("CODEX_HOME", profile);
                }
                command.status()?
            };
            if status.success() {
                set_account(&agent, &account)?;
                println!(
                    "Signed in. Active {agent} account: {}",
                    if account.is_empty() {
                        "Default"
                    } else {
                        &account
                    }
                );
            } else {
                println!("Sign-in did not finish. Your selected account is unchanged.");
            }
            if login_args.iter().any(|arg| arg == "--wait-on-close") && io::stdin().is_terminal() {
                print!(
                    "{}",
                    crate::i18n::tr(
                        "ai.ui.press-enter-to-close-this-sign-in-window",
                        "Press Enter to close this sign-in window. "
                    )
                );
                io::stdout().flush()?;
                let mut line = String::new();
                let _ = io::stdin().read_line(&mut line);
            }
            return Ok(status.code().unwrap_or(1));
        }
        _ => return Err(format!("Unknown nacre-ai command: {command}").into()),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_drop_terminal_controls() {
        assert_eq!(clean(" A\x1b[31m  B\nC "), "A[31m BC");
    }

    #[test]
    fn root_menu_keeps_the_daily_actions_and_order() {
        assert_eq!(
            DASHBOARD_OPTIONS,
            [
                "New chat",
                "Resume latest chat",
                "Load chat",
                "Open brain",
                "Settings",
                "Close"
            ]
        );
    }

    #[test]
    fn account_profiles_reject_shell_syntax_and_bound_names() {
        assert!(valid_account("work_2"));
        assert!(!valid_account(""));
        assert!(!valid_account("../work"));
        assert!(!valid_account("a".repeat(49).as_str()));
    }

    #[test]
    fn account_switch_preserves_default_assistant_and_other_profiles() {
        let mut settings = serde_json::json!({
            "default_assistant":"claude",
            "accounts":{"codex":"old","claude":"team"},
            "preserved":"future field"
        });
        update_account_settings(&mut settings, "codex", "work");
        assert_eq!(settings["default_assistant"], "claude");
        assert_eq!(settings["accounts"]["codex"], "work");
        assert_eq!(settings["accounts"]["claude"], "team");
        assert_eq!(settings["preserved"], "future field");
    }

    #[test]
    fn project_names_cannot_shadow_general_chat_or_create_empty_ids() {
        assert_eq!(project_id("Camera Work"), "camera-work");
        assert_eq!(project_id("General Chat"), "general-chat");
        assert_eq!(project_id("---"), "");
    }

    #[test]
    fn codex_resume_keeps_permission_defaults_and_private_history_override() {
        let standard = codex_resume_args("thread", "/repo", "/brain", false, None);
        assert_eq!(standard[1], "thread");
        assert_eq!(standard[3], "workspace-write");
        assert_eq!(standard[5], "never");
        assert!(standard.windows(2).any(|pair| pair == ["-C", "/repo"]));

        let full_access = codex_resume_args("thread", "/repo", "/brain", true, None);
        assert_eq!(full_access[3], "danger-full-access");

        let legacy = codex_resume_args("thread", "/repo", "/brain", false, Some("/history path"));
        assert_eq!(&legacy[10..12], &["--no-daemon", "-c"]);
        assert_eq!(legacy[12], "sqlite_home=\"/history path\"");
    }

    #[test]
    fn ssh_shell_arguments_quote_apostrophes_and_metacharacters() {
        assert_eq!(
            shell_quote("a'b; $(touch nope)"),
            "'a'\\''b; $(touch nope)'"
        );
    }

    #[test]
    fn fake_ssh_receives_a_tty_only_for_interactive_workers() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let capture = fixture.path().join("ssh-argv.txt");
        let fake = fixture.path().join("ssh");
        fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n",
                shell_quote(&capture.display().to_string())
            ),
        )?;
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755))?;
        let command = vec![
            "$HOME/.local/bin/codex".into(),
            "resume".into(),
            "thread; $(literal)".into(),
        ];
        assert!(run_remote_with(&fake, "task-host", &command, false)?
            .status
            .success());
        let interactive = fs::read_to_string(&capture)?;
        assert_eq!(interactive.lines().next(), Some("-t"));
        assert!(interactive.contains("task-host"));
        assert!(interactive.contains("\"$HOME/.local/bin/codex\" 'resume' 'thread; $(literal)'"));

        assert!(run_remote_with(&fake, "task-host", &command, true)?
            .status
            .success());
        let listing = fs::read_to_string(&capture)?;
        assert!(!listing.lines().any(|argument| argument == "-t"));
        Ok(())
    }

    #[test]
    fn task_window_uses_the_owned_class_and_preserves_literal_arguments() {
        let args = kitty_task_args(
            "nacre-ai",
            &["new".into(), "--project".into(), "My project".into()],
            "New chat",
        );
        assert_eq!(
            args,
            [
                "--class",
                "nacre-ai-task",
                "--title",
                "New chat",
                "--",
                "nacre-ai",
                "new",
                "--project",
                "My project"
            ]
        );
    }

    #[test]
    fn general_chat_permissions_keep_the_explicit_defaults() {
        let standard = codex_general_args("/chat", "/brain", false);
        assert_eq!(standard[1], "workspace-write");
        assert_eq!(standard[3], "never");
        assert!(standard.windows(2).any(|pair| pair == ["-C", "/chat"]));
        let full = codex_general_args("/chat", "/brain", true);
        assert_eq!(full[1], "danger-full-access");
        assert!(full.windows(2).any(|pair| pair == ["--add-dir", "/brain"]));
    }

    #[test]
    fn general_chat_directory_and_codex_trust_use_only_the_temporary_home() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let fixture_home = fixture.path().join("home");
        fs::create_dir_all(&fixture_home)?;
        let directory = create_general_chat_directory_at(&fixture_home)?;
        assert_eq!(
            fs::metadata(&directory)?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(directory.join("AGENTS.md"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let codex_home = fixture_home.join(".codex");
        fs::create_dir(&codex_home)?;
        let config = codex_home.join("config.toml");
        fs::write(&config, "[other]\nkey = \"preserve\"\n")?;
        trust_general_chat_at(
            &fixture_home.join(".local/share/nacre/ai/chats"),
            &directory,
            &codex_home,
        )?;
        let trusted = fs::read_to_string(&config)?;
        assert!(trusted.contains("trust_level = \"trusted\""));
        assert!(trusted.contains("key = \"preserve\""));
        trust_general_chat_at(
            &fixture_home.join(".local/share/nacre/ai/chats"),
            &directory,
            &codex_home,
        )?;
        assert_eq!(fs::read_to_string(config)?, trusted);
        Ok(())
    }

    #[test]
    fn latest_history_orders_most_recent_first() {
        let mut records = vec![
            serde_json::json!({"id":"older","updated_at":10}),
            serde_json::json!({"id":"newer","updated_at":30}),
            serde_json::json!({"id":"middle","updated_at":20}),
        ];
        sort_history(&mut records);
        assert_eq!(records[0]["id"], "newer");
        assert_eq!(records[1]["id"], "middle");
        assert_eq!(records[2]["id"], "older");
    }

    #[test]
    fn history_row_keeps_project_host_agent_account_and_state_visible() {
        let (title, details) = history_row(
            0,
            &serde_json::json!({
                "title":"Camera work",
                "state":"running",
                "agent":"claude",
                "account":"team",
                "project":{"id":"demo","label":"Public demo","host":"builder"}
            }),
        );
        assert_eq!(title, "01  Camera work");
        assert_eq!(
            details,
            "Public demo · builder · Claude Code · team · running"
        );
    }

    #[test]
    fn escape_clears_search_before_leaving_a_menu() {
        let mut query = String::from("camera");
        assert!(!escape_cancels(&mut query));
        assert!(query.is_empty());
        assert!(escape_cancels(&mut query));
    }

    #[test]
    fn background_gui_diagnostics_go_to_a_private_log() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let mut child = Command::new("sh");
        child.args(["-c", "printf visible; printf diagnostic >&2"]);
        launch_background_at(&mut child, fixture.path())?;
        let log = fixture.path().join(".local/state/nacre/ai/launcher.log");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < until {
            if fs::read_to_string(&log).is_ok_and(|contents| {
                contents.contains("visible") && contents.contains("diagnostic")
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let metadata = fs::metadata(&log)?;
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let contents = fs::read_to_string(log)?;
        assert!(contents.contains("visible"));
        assert!(contents.contains("diagnostic"));
        Ok(())
    }

    #[test]
    fn remote_history_retries_only_the_known_old_helper_option_error() {
        assert!(remote_all_accounts_unsupported(
            "nacre-ai-chat: unrecognized arguments: --all-accounts"
        ));
        assert!(!remote_all_accounts_unsupported("SSH connection refused"));
    }

    #[test]
    fn codex_startup_retries_only_known_transient_failures() {
        for detail in [
            "Server is draining",
            "Reconnect to continue",
            "Connection refused",
            "Startup lock held",
            "Timed out",
        ] {
            assert!(transient_codex_failure(detail), "{detail}");
        }
        assert!(!transient_codex_failure("Configuration permission denied"));
    }

    #[test]
    fn new_project_is_private_registered_and_never_replaces_existing_folder() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let fixture_home = fixture.path().join("home");
        fs::create_dir_all(&fixture_home)?;
        let registry = fixture_home.join(".config/nacre/ai/projects.json");
        let project = create_project_at("My New Project", &fixture_home, &registry)?;
        let folder = PathBuf::from(project["path"].as_str().unwrap());
        assert_eq!(project["id"], "my-new-project");
        assert_eq!(fs::metadata(&folder)?.permissions().mode() & 0o777, 0o700);
        assert!(folder.join(".git").is_dir());
        let before = fs::read(&registry)?;
        let marker = folder.join("keep.txt");
        fs::write(&marker, "preserve")?;
        assert!(create_project_at("My New Project", &fixture_home, &registry).is_err());
        assert_eq!(fs::read(&marker)?, b"preserve");
        assert_eq!(fs::read(&registry)?, before);
        Ok(())
    }
}
