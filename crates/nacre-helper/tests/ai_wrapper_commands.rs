use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn helper() -> &'static str {
    env!("CARGO_BIN_EXE_nacre-helper")
}
fn invoke(home: &Path, path: &Path, args: &[&str]) -> Output {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(path.to_path_buf()).chain(std::env::split_paths(&inherited)),
    )
    .unwrap();
    Command::new(helper())
        .args(args)
        .env("HOME", home)
        .env("PATH", path)
        .output()
        .unwrap()
}
fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn account_wrapper_uses_fresh_isolated_home_and_private_provider_umask() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".codex")).unwrap();
    fs::write(home.join(".codex/auth.json"), "fixture default auth").unwrap();
    fs::write(home.join(".codex/AGENTS.md"), "fixture guidance").unwrap();
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    executable(
        &home.join(".local/bin/nacre-ai-chat"),
        "#!/bin/sh\nprintf 'chat:%s:%s\\n' \"$CODEX_HOME\" \"$1\"\n",
    );
    let fake_path = temp.path().join("path");
    fs::create_dir(&fake_path).unwrap();
    executable(
        &fake_path.join("codex"),
        "#!/bin/sh\nprintf 'provider:%s:%s:%s\\n' \"$CODEX_HOME\" \"$(umask)\" \"$*\"\ntouch \"$HOME/provider-created\"\n",
    );
    let output = invoke(&home, &fake_path, &["nacre-ai-account", "second", "login"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let profile = home.join(".local/share/nacre/ai/accounts/second");
    assert!(stdout.contains(&format!("provider:{}:", profile.display())));
    assert!(stdout.contains(":login\n"));
    assert!(stdout.contains(&format!("chat:{}:configure", profile.display())));
    assert!(!profile.join("auth.json").exists());
    assert_eq!(
        fs::read_to_string(profile.join("config.toml")).unwrap(),
        "cli_auth_credentials_store = \"auto\"\n"
    );
    assert!(fs::symlink_metadata(profile.join("AGENTS.md"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(home.join(".codex/auth.json")).unwrap(),
        "fixture default auth"
    );
    assert_eq!(
        fs::metadata(home.join("provider-created"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn existing_account_storage_choice_is_unchanged() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join(".local/share/nacre/ai/accounts/second");
    fs::create_dir_all(&profile).unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        profile.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();
    let fake_path = temp.path().join("path");
    fs::create_dir(&fake_path).unwrap();
    executable(&fake_path.join("codex"), "#!/bin/sh\nexit 0\n");
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    executable(
        &home.join(".local/bin/nacre-ai-chat"),
        "#!/bin/sh\nexit 0\n",
    );
    let output = invoke(&home, &fake_path, &["nacre-ai-account", "second"]);
    assert!(output.status.success());
    assert_eq!(
        fs::read_to_string(profile.join("config.toml")).unwrap(),
        "cli_auth_credentials_store = \"file\"\n"
    );
}

#[test]
fn tool_wrapper_adds_private_skill_tool_paths_and_forwards_arguments() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let root = home.join(".local/share/nacre/ai/skill-tools");
    fs::create_dir_all(root.join("python/bin")).unwrap();
    fs::create_dir_all(root.join("node/bin")).unwrap();
    fs::create_dir_all(root.join("node_modules/.bin")).unwrap();
    let fake_path = temp.path().join("path");
    fs::create_dir(&fake_path).unwrap();
    executable(
        &fake_path.join("python"),
        "#!/bin/sh\nprintf '%s\\n%s\\n%s\\n' \"$PATH\" \"$NODE_PATH\" \"$*\"\n",
    );
    let output = invoke(
        &home,
        &fake_path,
        &["nacre-ai-tools", "python", "-c", "fixture arguments"],
    );
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with(&format!(
        "{}:{}:{}:",
        root.join("python/bin").display(),
        root.join("node/bin").display(),
        root.join("node_modules/.bin").display()
    )));
    assert!(text.contains(&root.join("node_modules").display().to_string()));
    assert!(text.contains("-c fixture arguments"));
}

#[test]
fn doctor_backend_status_reports_only_secret_service_name_ownership() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join(".codex")).unwrap();
    fs::write(
        home.join(".codex/config.toml"),
        "cli_auth_credentials_store = \"auto\"\n",
    )
    .unwrap();
    fs::write(
        home.join(".codex/auth.json"),
        "fixture auth must remain unread",
    )
    .unwrap();
    let fake_path = temp.path().join("path");
    fs::create_dir(&fake_path).unwrap();
    let busctl = fake_path.join("busctl");
    executable(
        &busctl,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$HOME/busctl-args\"\nprintf 'b true\\n'\n",
    );
    let output = invoke(&home, &fake_path, &["ai-credentials-status"]);
    assert!(output.status.success());
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["secretService"]["available"], true);
    assert_eq!(
        status["codex"]["defaultHome"]["configured"],
        "auto (keyring preferred; file fallback)"
    );
    let args = fs::read_to_string(home.join("busctl-args")).unwrap();
    assert!(args.contains("NameHasOwner s org.freedesktop.secrets"));
    assert!(!args.contains("SearchItems"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("fixture auth"));
}

#[test]
fn remote_wrapper_invalid_inputs_keep_exit_two_contract() {
    let temp = tempdir().unwrap();
    let output = invoke(
        temp.path(),
        temp.path(),
        &["nacre-ai-remote", "bad/name", "/repo"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn remote_wrapper_quotes_paths_and_keeps_permission_defaults() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home with spaces");
    fs::create_dir_all(&home).unwrap();
    let fake_path = temp.path().join("path");
    fs::create_dir(&fake_path).unwrap();
    executable(
        &fake_path.join("git"),
        "#!/bin/sh\nprintf 'git:%s\\n' \"$*\" >> \"$HOME/git-args\"\nif [ \"$3\" = rev-parse ]; then printf '/repo with spaces\\n'; fi\nexit 0\n",
    );
    executable(
        &fake_path.join("tmux"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/tmux-args\"\nexit 0\n",
    );
    executable(&fake_path.join("codex"), "#!/bin/sh\nexit 0\n");
    let output = invoke(
        &home,
        &fake_path,
        &["nacre-ai-remote", "project", "/repo with spaces", "second"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tmux_args = fs::read_to_string(home.join("tmux-args")).unwrap();
    assert!(tmux_args.starts_with("new-session\n-s\nai-project-profile-second-"));
    assert!(tmux_args.contains("umask 077;"));
    assert!(tmux_args.contains("danger-full-access"));
    assert!(tmux_args.contains("--ask-for-approval"));
    assert!(tmux_args.contains("never"));
    assert!(tmux_args.contains(&format!(
        "'{}' 'second'",
        home.join(".local/bin/nacre-ai-account").display()
    )));
    assert!(tmux_args.contains(&format!(
        "cd '{}",
        home.join(".local/share/nacre/ai/worktrees/project")
            .display()
    )));
    assert_eq!(
        fs::metadata(home.join("Documents/Nacre-Brain"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let git = fs::read_to_string(home.join("git-args")).unwrap();
    assert!(git.contains("/repo with spaces rev-parse --show-toplevel"));
    assert!(git.contains("/repo with spaces worktree add -b codex/ai-project-"));
}
