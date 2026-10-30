//! Process-level contracts use private HOMEs and synthetic providers only.
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join(".local/bin").display()),
        )
        .env("NACRE_HELPER_BINARY", env!("CARGO_BIN_EXE_nacre-helper"))
        .output()
        .unwrap()
}
fn ok(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn setup(home: &Path) {
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    script(
        &home.join(".local/bin/nacre-ai-skills"),
        "#!/bin/sh\nexit 0\n",
    );
}

#[test]
fn claude_launch_preserves_profile_permissions_and_private_session_home() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    fs::create_dir_all(home.join(".config/nacre")).unwrap();
    script(&home.join(".local/bin/claude"), "#!/usr/bin/python3\nimport json,os,sys\nprint(json.dumps({'argv':sys.argv[1:],'cwd':os.getcwd(),'config':os.environ['CLAUDE_CONFIG_DIR'],'umask':os.umask(0)}))\n");
    let root = home.join("chats");
    for full in [false, true] {
        fs::write(
            home.join(".config/nacre/profile.json"),
            json!({"aiFullAccess":full}).to_string(),
        )
        .unwrap();
        let result = run(
            home,
            &[
                "nacre-ai-claude",
                "new",
                "--chat-directory",
                "--path",
                root.to_str().unwrap(),
                "--account",
                "work",
            ],
        );
        ok(&result);
        let row: Value = serde_json::from_slice(&result.stdout).unwrap();
        let argv = row["argv"].as_array().unwrap();
        assert_eq!(
            argv.contains(&json!("--dangerously-skip-permissions")),
            full
        );
        assert_eq!(row["umask"], 0o077);
        assert_eq!(
            row["config"],
            home.join(".local/share/nacre/ai/claude-accounts/work")
                .to_str()
                .unwrap()
        );
        let cwd = Path::new(row["cwd"].as_str().unwrap());
        assert_eq!(cwd.parent(), Some(root.as_path()));
        assert_eq!(cwd.file_name().unwrap().len(), 32);
        assert_eq!(
            fs::metadata(cwd).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(argv.iter().any(|v| v
            .as_str()
            .is_some_and(|s| s.contains("Codex-specific title tools do not apply to Claude"))));
    }
}

#[test]
fn claude_tmux_new_reuses_one_uuid_for_tmux_child_and_claude_session() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    script(
        &home.join(".local/bin/tmux"),
        r#"#!/usr/bin/python3
import json,os,sys,subprocess
if sys.argv[1] == 'list-panes':
    raise SystemExit(0)
if sys.argv[1] == 'new-session':
    open(os.path.join(os.environ['HOME'],'tmux-args.json'),'w').write(json.dumps(sys.argv[1:]))
    raise SystemExit(subprocess.run(['/bin/sh','-c',sys.argv[-1]]).returncode)
print(json.dumps(sys.argv[1:]))
"#,
    );
    script(
        &home.join(".local/bin/claude"),
        "#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n",
    );
    let root = home.join("chats");
    fs::create_dir_all(&root).unwrap();
    let result = run(
        home,
        &[
            "nacre-ai-claude",
            "new",
            "--tmux",
            "--project",
            "fixture",
            "--path",
            root.to_str().unwrap(),
            "--chat-directory",
        ],
    );
    ok(&result);
    let tmux_args: Value =
        serde_json::from_slice(&fs::read(home.join("tmux-args.json")).unwrap()).unwrap();
    let name = tmux_args[3].as_str().unwrap();
    assert_eq!(tmux_args[5], ";");
    assert_eq!(
        &tmux_args[6..],
        [
            json!("set-option"),
            json!("-t"),
            json!(name),
            json!("status"),
            json!("off"),
            json!(";"),
            json!("set-option"),
            json!("-t"),
            json!(name),
            json!("mouse"),
            json!("on"),
        ]
    );
    let whole_id = name.strip_prefix("ai-fixture-claude-default-").unwrap();
    assert_eq!(
        whole_id.split('-').map(str::len).collect::<Vec<_>>(),
        [8, 4, 4, 4, 12]
    );
    let child = tmux_args[4].as_str().unwrap();
    assert!(child.contains(&format!("--thread-id '{whole_id}'")));
    let claude_args: Vec<String> = serde_json::from_slice(&result.stdout).unwrap();
    assert!(claude_args
        .windows(2)
        .any(|pair| pair[0] == "--session-id" && pair[1] == whole_id));
    let settings: Value = serde_json::from_str(
        &claude_args[claude_args
            .iter()
            .position(|arg| arg == "--settings")
            .unwrap()
            + 1],
    )
    .unwrap();
    assert!(settings["hooks"]["UserPromptSubmit"].is_array());
}

#[test]
fn claude_new_rejects_a_non_uuid_thread_id_before_starting_tmux() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    script(
        &home.join(".local/bin/tmux"),
        "#!/bin/sh\ntouch \"$HOME/tmux-called\"\nexit 0\n",
    );
    script(
        &home.join(".local/bin/claude"),
        "#!/bin/sh\ntouch \"$HOME/claude-called\"\n",
    );
    let root = home.join("chats");
    fs::create_dir_all(&root).unwrap();
    let result = run(
        home,
        &[
            "nacre-ai-claude",
            "new",
            "--tmux",
            "--project",
            "fixture",
            "--path",
            root.to_str().unwrap(),
            "--thread-id",
            "not-a-uuid",
        ],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("valid Claude session ID"));
    assert!(!home.join("tmux-called").exists());
    assert!(!home.join("claude-called").exists());
}

fn write_claude_transcript(home: &Path, cwd: &Path, id: &str, title: Value) {
    let project_name = cwd
        .to_string_lossy()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let project = home.join(".claude/projects").join(project_name);
    fs::create_dir_all(&project).unwrap();
    let user = json!({
        "type":"user", "sessionId":id,
        "uuid":"11111111-1111-4111-8111-111111111111",
        "cwd":cwd, "message":{"content":"first prompt"}
    });
    fs::write(
        project.join(format!("{id}.jsonl")),
        format!("{}\n{}\n", user, title),
    )
    .unwrap();
}

#[test]
fn claude_resume_uses_ai_title_but_never_overwrites_a_custom_title() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    script(
        &home.join(".local/bin/tmux"),
        "#!/bin/sh\nif [ \"$1\" = list-panes ]; then exit 0; fi\nexit 0\n",
    );
    script(
        &home.join(".local/bin/claude"),
        "#!/usr/bin/python3\nimport json,sys\nif sys.argv[1:]==['agents','--json']: print('[]')\nelse: print(json.dumps(sys.argv[1:]))\n",
    );
    let cwd = home.join("saved-chat");
    fs::create_dir_all(&cwd).unwrap();
    for (id, title, expected_name) in [
        (
            "11111111-1111-4111-8111-111111111111",
            json!({"type":"ai-title","aiTitle":"AI chosen title"}),
            Some("AI chosen title"),
        ),
        (
            "22222222-2222-4222-8222-222222222222",
            json!({"type":"custom-title","customTitle":"Owner rename"}),
            None,
        ),
    ] {
        write_claude_transcript(home, &cwd, id, title);
        let result = run(
            home,
            &[
                "nacre-ai-claude",
                "resume",
                "--project",
                "fixture",
                "--path",
                cwd.to_str().unwrap(),
                "--thread-id",
                id,
            ],
        );
        ok(&result);
        let args: Vec<String> = serde_json::from_slice(&result.stdout).unwrap();
        let named = args.windows(2).find(|pair| pair[0] == "--name");
        assert_eq!(named.map(|pair| pair[1].as_str()), expected_name);
        assert!(args.iter().any(|arg| arg == "--resume"));
        let settings: Value = serde_json::from_str(
            &args[args.iter().position(|arg| arg == "--settings").unwrap() + 1],
        )
        .unwrap();
        assert!(settings["hooks"]["UserPromptSubmit"].is_array());
    }
}

#[test]
fn claude_resume_attaches_exact_and_unique_legacy_tmux_sessions_and_rejects_ambiguous_cwd() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    let cwd = home.join("saved-chat");
    fs::create_dir_all(&cwd).unwrap();
    let id = "11111111-1111-4111-8111-111111111111";
    let legacy = "ai-fixture-claude-default-legacy";
    let tmux_body = format!(
        "#!/usr/bin/python3\nimport json,sys\nif sys.argv[1]=='list-panes': print({:?})\nelse: print(json.dumps(sys.argv[1:]))\n",
        format!("{legacy}\t{}", cwd.display())
    );
    script(&home.join(".local/bin/tmux"), &tmux_body);
    script(
        &home.join(".local/bin/claude"),
        "#!/usr/bin/python3\nimport json,sys\nif sys.argv[1:]==['agents','--json']: print('[]')\nelse: open(sys.argv[0] + '.called','w').write('called')\n",
    );

    write_claude_transcript(home, &cwd, id, json!({"type":"summary","summary":"Title"}));
    let exact_name = format!("ai-fixture-claude-default-{id}");
    let exact_body = format!(
        "#!/usr/bin/python3\nimport json,sys\nif sys.argv[1]=='list-panes': print({:?})\nelse: print(json.dumps(sys.argv[1:]))\n",
        format!("{exact_name}\t{}", cwd.display())
    );
    script(&home.join(".local/bin/tmux"), &exact_body);
    let exact = run(
        home,
        &[
            "nacre-ai-claude",
            "resume",
            "--project",
            "fixture",
            "--path",
            cwd.to_str().unwrap(),
            "--thread-id",
            id,
        ],
    );
    ok(&exact);
    assert_eq!(
        serde_json::from_slice::<Value>(&exact.stdout).unwrap(),
        json!([
            "attach-session", "-t", format!("={exact_name}"), ";", "set-option", "-t",
            exact_name, "status", "off", ";", "set-option", "-t", exact_name, "mouse", "on"
        ])
    );

    script(&home.join(".local/bin/tmux"), &tmux_body);
    let unique = run(
        home,
        &[
            "nacre-ai-claude",
            "resume",
            "--project",
            "fixture",
            "--path",
            cwd.to_str().unwrap(),
            "--thread-id",
            id,
        ],
    );
    ok(&unique);
    assert_eq!(
        serde_json::from_slice::<Value>(&unique.stdout).unwrap(),
        json!([
            "attach-session", "-t", format!("={legacy}"), ";", "set-option", "-t", legacy,
            "status", "off", ";", "set-option", "-t", legacy, "mouse", "on"
        ])
    );

    let other_cwd = home.join("different-chat");
    fs::create_dir_all(&other_cwd).unwrap();
    let other_id = "33333333-3333-4333-8333-333333333333";
    write_claude_transcript(
        home,
        &other_cwd,
        other_id,
        json!({"type":"summary","summary":"Different chat"}),
    );
    let wrong_row_tmux = format!(
        "#!/usr/bin/python3\nimport json,sys\nif sys.argv[1]=='list-panes': print({:?})\nelse: print(json.dumps(sys.argv[1:]))\n",
        format!("{legacy}\t{}", other_cwd.display())
    );
    script(&home.join(".local/bin/tmux"), &wrong_row_tmux);
    let wrong_row = run(
        home,
        &[
            "nacre-ai-claude",
            "resume",
            "--project",
            "fixture",
            "--path",
            other_cwd.to_str().unwrap(),
            "--thread-id",
            id,
        ],
    );
    assert!(!wrong_row.status.success());
    assert!(String::from_utf8_lossy(&wrong_row.stderr).contains("cannot be matched safely"));

    write_claude_transcript(
        home,
        &cwd,
        "22222222-2222-4222-8222-222222222222",
        json!({"type":"summary","summary":"Second"}),
    );
    script(&home.join(".local/bin/tmux"), &tmux_body);
    let ambiguous = run(
        home,
        &[
            "nacre-ai-claude",
            "resume",
            "--project",
            "fixture",
            "--path",
            cwd.to_str().unwrap(),
            "--thread-id",
            id,
        ],
    );
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("cannot be matched safely"));
    assert!(!home.join(".local/bin/claude.called").exists());

    // A known different running chat in a shared cwd must not block this saved chat.
    script(&home.join(".local/bin/tmux"), &exact_body);
    let distinct = run(
        home,
        &[
            "nacre-ai-claude",
            "resume",
            "--project",
            "fixture",
            "--path",
            cwd.to_str().unwrap(),
            "--thread-id",
            "22222222-2222-4222-8222-222222222222",
        ],
    );
    ok(&distinct);
    assert!(home.join(".local/bin/claude.called").exists());
}

#[test]
fn codex_resume_keeps_exact_thread_history_and_permission_flags() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    script(
        &home.join(".local/bin/tmux"),
        "#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n",
    );
    let store = home.join("old store");
    fs::create_dir(&store).unwrap();
    let result = run(
        home,
        &[
            "nacre-ai-chat",
            "resume",
            "--project",
            "fixture",
            "--account",
            "work",
            "--thread-id",
            "thread-one",
            "--history-home",
            store.to_str().unwrap(),
        ],
    );
    ok(&result);
    let row: Value = serde_json::from_slice(&result.stdout).unwrap();
    let argv = row.as_array().unwrap();
    assert_eq!(argv[0], "new-session");
    assert_eq!(argv[3], "ai-fixture-profile-work-resume-thread-one");
    let shell = argv.last().unwrap().as_str().unwrap();
    assert!(shell.contains("thread-one"));
    assert!(shell.contains("danger-full-access"));
    assert!(shell.contains("never"));
    assert!(shell.contains("--no-daemon"));
    assert!(shell.contains("sqlite_home"));
    assert!(shell.contains("old store"));
    assert!(shell.contains("NACRE_AI_HISTORY_HOME"));
}

#[test]
fn configure_preserves_private_auth_and_nested_settings_and_is_idempotent() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    let profile = home.join(".local/share/nacre/ai/accounts/work");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("auth.json"), "synthetic marker").unwrap();
    let original = "sandbox_mode=\"workspace-write\"\napproval_policy=\"on-request\"\nmodel=\"keep[model\"\n[profiles.special]\napproval_policy=\"on-request\"\n";
    fs::write(profile.join("config.toml"), original).unwrap();
    ok(&run(
        home,
        &["nacre-ai-chat", "configure", "--account", "work"],
    ));
    let first = fs::read_to_string(profile.join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&first).unwrap();
    assert_eq!(parsed["model"].as_str(), Some("keep[model"));
    assert_eq!(parsed["sandbox_mode"].as_str(), Some("danger-full-access"));
    assert_eq!(parsed["approval_policy"].as_str(), Some("never"));
    assert_eq!(
        parsed["profiles"]["special"]["approval_policy"].as_str(),
        Some("on-request")
    );
    assert_eq!(parsed["sqlite_home"].as_str(), home.join(".codex").to_str());
    ok(&run(
        home,
        &["nacre-ai-chat", "configure", "--account", "work"],
    ));
    assert_eq!(
        fs::read_to_string(profile.join("config.toml")).unwrap(),
        first
    );
    assert_eq!(
        fs::read_to_string(profile.join("auth.json")).unwrap(),
        "synthetic marker"
    );
}

#[test]
fn malformed_config_is_preserved_and_missing_claude_history_needs_no_sdk() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    fs::create_dir(home.join(".codex")).unwrap();
    fs::write(home.join(".codex/config.toml"), "[broken").unwrap();
    assert!(!run(home, &["nacre-ai-chat", "configure"]).status.success());
    assert_eq!(
        fs::read_to_string(home.join(".codex/config.toml")).unwrap(),
        "[broken"
    );
    let result = run(home, &["nacre-ai-claude", "list"]);
    ok(&result);
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        json!([])
    );
    assert!(!home.join(".local/share/nacre/ai/claude-sdk").exists());
}

#[test]
fn all_account_history_keeps_newest_id_and_legacy_store_source() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    let legacy = home.join(".local/share/nacre/ai/accounts/old");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("state_5.sqlite"), "synthetic marker").unwrap();
    fs::create_dir(home.join("chats")).unwrap();
    script(&home.join(".local/bin/tmux"), "#!/bin/sh\nexit 0\n");
    script(
        &home.join(".local/bin/codex"),
        r#"#!/usr/bin/python3
import json,sys
old = any('/accounts/old' in a for a in sys.argv)
for line in sys.stdin:
    q=json.loads(line)
    if 'id' not in q: continue
    if q['method']=='thread/list':
        assert q['params']['useStateDbOnly'] is True
        data=[{'id':'same','name':'New' if old else 'Old','updatedAt':2 if old else 1,'cwd':'/synthetic'}]
        if old: data.append({'id':'other','name':'Other','cwd':'/synthetic/other'})
        result={'data':data,'nextCursor':None}
    else: result={}
    print(json.dumps({'id':q['id'],'result':result}),flush=True)
"#,
    );
    let result = run(
        home,
        &[
            "nacre-ai-chat",
            "list",
            "--project",
            "fixture",
            "--path",
            home.join("chats").to_str().unwrap(),
            "--chat-directory",
            "--all-accounts",
        ],
    );
    ok(&result);
    let rows: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["id"], "same");
    assert_eq!(rows[0]["title"], "New");
    assert_eq!(rows[0]["account"], "old");
    assert_eq!(rows[0]["history_home"], legacy.to_str().unwrap());
    assert_eq!(rows[0]["shared_history"], false);
    assert_eq!(rows[1]["id"], "other");
}

#[test]
fn mcp_resolves_context_through_native_command_dispatch() {
    use std::io::Write;
    use std::process::Stdio;
    let temp = tempdir().unwrap();
    let home = temp.path();
    fs::create_dir_all(home.join(".config/nacre/ai")).unwrap();
    fs::write(
        home.join(".config/nacre/ai/projects.json"),
        json!({"projects":[{"id":"fixture","label":"Fixture","path":"/synthetic/repo"}]})
            .to_string(),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["nacre-ai-chat", "serve"])
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"resolve_context","arguments":{"query":"fixture"}}}
"#).unwrap();
    let output = child.wait_with_output().unwrap();
    ok(&output);
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(response["result"]["isError"], true, "{response}");
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("/synthetic/repo"));
}

#[test]
fn configure_honors_inherited_account_home_and_refuses_linked_locks() {
    use std::os::unix::fs::symlink;
    let temp = tempdir().unwrap();
    let home = temp.path();
    let profile = home.join("selected");
    fs::create_dir(&profile).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .args(["nacre-ai-chat", "configure"])
            .env_clear()
            .env("HOME", home)
            .env("CODEX_HOME", &profile)
            .output()
            .unwrap()
    };
    ok(&invoke());
    assert!(profile.join("config.toml").is_file());
    assert!(!home.join(".codex/config.toml").exists());
    fs::remove_file(profile.join("nacre-config.lock")).unwrap();
    let marker = home.join("private-marker");
    fs::write(&marker, "synthetic unchanged data").unwrap();
    symlink(&marker, profile.join("nacre-config.lock")).unwrap();
    assert!(!invoke().status.success());
    assert_eq!(
        fs::read_to_string(marker).unwrap(),
        "synthetic unchanged data"
    );
}

#[test]
fn missing_claude_command_does_not_launch_a_provider() {
    let temp = tempdir().unwrap();
    setup(temp.path());
    script(
        &temp.path().join(".local/bin/nacre-ai-skills"),
        "#!/bin/sh\ntouch \"$HOME/skills-ran\"\n",
    );
    script(
        &temp.path().join(".local/bin/claude"),
        "#!/bin/sh\ntouch \"$HOME/provider-ran\"\n",
    );
    let cases: &[&[&str]] = &[
        &["nacre-ai-claude"],
        &["nacre-ai-claude", "new", "resume"],
        &["nacre-ai-claude", "new", "--project", "_invalid"],
    ];
    for args in cases {
        assert!(!run(temp.path(), args).status.success());
        assert!(!temp.path().join("provider-ran").exists());
        assert!(!temp.path().join("skills-ran").exists());
        assert!(!temp.path().join(".local/share/nacre/ai/worktrees").exists());
    }
}

#[test]
fn claude_resume_attaches_exact_background_session_without_a_roster() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    let id = "11111111-1111-4111-8111-111111111111";
    script(
        &home.join(".local/bin/claude"),
        r#"#!/usr/bin/python3
import json,sys
if sys.argv[1:]==['agents','--json']:
    print(json.dumps([{'kind':'background','id':'fixture-background','sessionId':'11111111-1111-4111-8111-111111111111'}]))
else:
    print(json.dumps(sys.argv[1:]))
"#,
    );
    let result = run(
        home,
        &[
            "nacre-ai-claude",
            "resume",
            "--thread-id",
            id,
            "--path",
            home.to_str().unwrap(),
        ],
    );
    ok(&result);
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        json!(["attach", "fixture-background"])
    );
    assert!(!home.join(".claude/daemon/roster.json").exists());
}

#[test]
fn codex_resume_passes_inherited_history_to_an_existing_tmux_server() {
    let temp = tempdir().unwrap();
    let home = temp.path();
    setup(home);
    script(
        &home.join(".local/bin/tmux"),
        "#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n",
    );
    let history = home.join("inherited history");
    let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args([
            "nacre-ai-chat",
            "resume",
            "--project",
            "fixture",
            "--thread-id",
            "thread-one",
        ])
        .env_clear()
        .env("HOME", home)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", home.join(".local/bin").display()),
        )
        .env("NACRE_AI_HISTORY_HOME", &history)
        .output()
        .unwrap();
    ok(&output);
    let argv: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(argv
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_str()
        .unwrap()
        .contains(&format!("NACRE_AI_HISTORY_HOME={}", history.display())));
}
