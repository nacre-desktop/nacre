use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::tempdir;

fn helper() -> &'static str {
    env!("CARGO_BIN_EXE_nacre-helper")
}
fn invoke(home: &Path, args: &[&str], input: Option<&str>) -> std::process::Output {
    let mut command = Command::new(helper());
    command
        .args(args)
        .env("HOME", home)
        .env_remove("NACRE_BRAIN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn memory_capture_is_private_deduplicated_and_discovers_worlds() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let mut command = Command::new(helper());
    command
        .args([
            "nacre-ai-memory",
            "--title",
            "Migration evidence",
            "--source",
            "test fixture",
            "--world",
            "Public World",
            "--world",
            "Public World",
            "--topic",
            "AI, rust",
        ])
        .env("HOME", &home)
        .env("NACRE_BRAIN", temporary.path().join("vault"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Verified fixture body")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(first["worlds"], json!(["Public World"]));
    assert_eq!(first["topics"], json!(["AI  rust"]));
    assert!(Path::new(first["path"].as_str().unwrap()).is_file());
    let state = temporary.path().join("vault/.brain-state");
    // Python's historical json.dumps(..., ensure_ascii=False) fingerprint.
    assert!(state
        .join("capture-11da4d75e1e3be5adf1e1c289bfbfe1de32d5a8292ce17d479565b3ca7dcccf7.json")
        .is_file());
    assert_eq!(
        fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(state.join("capture.lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(temporary
        .path()
        .join("vault/wiki/entity-public-world.md")
        .is_file());

    let mut repeated = Command::new(helper());
    repeated
        .args([
            "nacre-ai-memory",
            "--title",
            "Migration evidence",
            "--source",
            "test fixture",
            "--world",
            "Public World",
            "--world",
            "Public World",
            "--topic",
            "AI, rust",
        ])
        .env("HOME", &home)
        .env("NACRE_BRAIN", temporary.path().join("vault"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = repeated.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Verified fixture body")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let second: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(first["path"], second["path"]);
    assert_eq!(
        fs::read_dir(temporary.path().join("vault/inbox"))
            .unwrap()
            .count(),
        1
    );
    let record =
        state.join("capture-11da4d75e1e3be5adf1e1c289bfbfe1de32d5a8292ce17d479565b3ca7dcccf7.json");
    let mut data: Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    assert_eq!(data["version"], 1);
    data["version"] = json!(999);
    let future = serde_json::to_vec(&data).unwrap();
    fs::write(&record, &future).unwrap();
    let mut child = repeated.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Verified fixture body")
        .unwrap();
    assert!(!child.wait_with_output().unwrap().status.success());
    assert_eq!(fs::read(record).unwrap(), future);
}

#[test]
fn raw_memory_metadata_and_symlinked_profile_are_rejected() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let output = invoke(
        &home,
        &[
            "nacre-ai-memory",
            "--title",
            "password",
            "--source",
            ":fixture-secret",
        ],
        Some("safe body"),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Possible credential"));

    let outside = temporary.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, home.join(".local")).unwrap();
    let output = invoke(
        &home,
        &["ai-core"],
        Some(r#"{"operation":"environment","agent":"codex","account":"fixture"}"#),
    );
    assert!(!output.status.success());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
fn ai_core_full_access_requires_a_json_boolean_and_repairs_only_fixture_profile_mode() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    let profile = home.join(".config/nacre/profile.json");
    fs::create_dir_all(profile.parent().unwrap()).unwrap();
    fs::write(&profile, r#"{"aiFullAccess":true}"#).unwrap();
    let output = invoke(&home, &["ai-core"], Some(r#"{"operation":"full-access"}"#));
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["full_access"],
        true
    );
    fs::write(&profile, r#"{"aiFullAccess":"true"}"#).unwrap();
    let output = invoke(&home, &["ai-core"], Some(r#"{"operation":"full-access"}"#));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["full_access"],
        false
    );
    fs::write(&profile, r#"{"version":999,"aiFullAccess":true}"#).unwrap();
    let output = invoke(&home, &["ai-core"], Some(r#"{"operation":"full-access"}"#));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["full_access"],
        false
    );

    let named_home = home.join(".local/share/nacre/ai/accounts/fixture");
    fs::create_dir_all(&named_home).unwrap();
    fs::set_permissions(&named_home, fs::Permissions::from_mode(0o755)).unwrap();
    let output = invoke(
        &home,
        &["ai-core"],
        Some(r#"{"operation":"environment","agent":"codex","account":"fixture"}"#),
    );
    assert!(output.status.success());
    assert_eq!(
        fs::metadata(&named_home).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(!named_home.join("auth.json").exists());
    let output = invoke(
        &home,
        &["ai-core"],
        Some(r#"{"operation":"environment","agent":"claude","account":"missing"}"#),
    );
    assert!(output.status.success());
    assert!(!home
        .join(".local/share/nacre/ai/claude-accounts/missing")
        .exists());
    let default_home = home.join(".claude");
    fs::create_dir(&default_home).unwrap();
    fs::set_permissions(&default_home, fs::Permissions::from_mode(0o755)).unwrap();
    let output = invoke(
        &home,
        &["ai-core"],
        Some(r#"{"operation":"environment","agent":"claude"}"#),
    );
    assert!(output.status.success());
    assert_eq!(
        fs::metadata(&default_home).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn usage_cache_capture_uses_private_mode_and_reads_only_fixture_data() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let input = r#"{"rate_limits":{"five_hour":{"used_percentage":12.5,"resets_at":1900000000}},"cost":{"total_cost_usd":0.25}}"#;
    let output = invoke(&home, &["nacre-ai-usage", "capture"], Some(input));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cache = home.join(".local/state/nacre/ai/usage/claude-default.json");
    assert_eq!(
        fs::metadata(&cache).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let output = invoke(
        &home,
        &["nacre-ai-usage", "read", "--agent", "claude"],
        None,
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| line.as_str().unwrap().contains("12.5%")));
}

#[test]
fn linked_usage_cache_and_oversized_capture_are_refused_without_writes() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    let outside = temporary.path().join("outside");
    let cache = home.join(".local/state/nacre/ai/usage");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, &cache).unwrap();
    let output = invoke(&home, &["nacre-ai-usage", "capture"], Some("{}"));
    assert!(!output.status.success());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    fs::remove_file(cache).unwrap();
    let oversized = " ".repeat(1_048_577);
    let output = invoke(&home, &["nacre-ai-usage", "capture"], Some(&oversized));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exceeds"));
}

#[test]
fn memory_refuses_a_linked_capture_lock_without_touching_its_target() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    let state = home.join("Documents/Nacre-Brain/.brain-state");
    fs::create_dir_all(&state).unwrap();
    let target = temporary.path().join("fixture-lock");
    fs::write(&target, "untouched fixture").unwrap();
    symlink(&target, state.join("capture.lock")).unwrap();
    let output = invoke(
        &home,
        &["nacre-ai-memory", "--title", "Fixture", "--source", "test"],
        Some("Fixture evidence."),
    );
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched fixture");
}

#[test]
fn memory_alias_preserves_curated_pages_and_does_not_register_projects() {
    let temporary = tempdir().unwrap();
    let home = temporary.path().join("home");
    let wiki = home.join("Documents/Nacre-Brain/wiki");
    fs::create_dir_all(&wiki).unwrap();
    let page = wiki.join("curated.md");
    let original = "# Subject\nName: Climbing\nAliases: Bouldering\n\nCurated evidence\n";
    fs::write(&page, original).unwrap();
    let registry = home.join(".config/nacre/ai/projects.json");
    fs::create_dir_all(registry.parent().unwrap()).unwrap();
    fs::write(&registry, "{\"projects\":[]}").unwrap();
    let output = invoke(
        &home,
        &[
            "nacre-ai-memory",
            "--title",
            "Fixture",
            "--source",
            "test",
            "--world",
            "Bouldering",
        ],
        Some("Fixture evidence."),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(page).unwrap(), original);
    assert_eq!(fs::read_dir(wiki).unwrap().count(), 1);
    assert_eq!(fs::read_to_string(registry).unwrap(), "{\"projects\":[]}");
}

#[test]
fn selected_accounts_allow_an_explicit_default_override_and_reject_invalid_types() {
    let temporary = tempdir().unwrap();
    let home = temporary.path();
    let settings = home.join(".config/nacre/ai/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r#"{"default_assistant":"claude","accounts":{"claude":"work"}}"#,
    )
    .unwrap();
    let output = invoke(
        home,
        &["ai-core"],
        Some(r#"{"operation":"default-assistant"}"#),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["agent"],
        "claude"
    );
    let output = invoke(
        home,
        &["ai-core"],
        Some(r#"{"operation":"effective-account","agent":"claude"}"#),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["account"],
        "work"
    );
    let output = invoke(
        home,
        &["ai-core"],
        Some(r#"{"operation":"effective-account","agent":"claude","override":""}"#),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["account"],
        ""
    );
    for invalid in [r#""../escape""#, "23", "true"] {
        let request =
            format!(r#"{{"operation":"effective-account","agent":"claude","override":{invalid}}}"#);
        assert!(!invoke(home, &["ai-core"], Some(&request)).status.success());
    }
    fs::write(&settings, r#"{"version":999,"accounts":{"claude":"work"}}"#).unwrap();
    assert!(!invoke(
        home,
        &["ai-core"],
        Some(r#"{"operation":"effective-account","agent":"claude"}"#)
    )
    .status
    .success());
}

#[test]
fn shared_history_uses_top_level_toml_and_never_logs_invalid_config_payloads() {
    let temporary = tempdir().unwrap();
    let home = temporary.path();
    let config = home.join(".codex/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let expected = home.join("history-store");
    fs::write(
        &config,
        format!(
            "sqlite_home = {}\n[profiles.custom]\nsqlite_home = '/keep'\n",
            serde_json::to_string(&expected).unwrap()
        ),
    )
    .unwrap();
    let request = r#"{"operation":"shared-history-home"}"#;
    let output = invoke(home, &["ai-core"], Some(request));
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["path"],
        expected.to_str().unwrap()
    );
    fs::write(&config, "token = \"fixture-sensitive-value").unwrap();
    let output = invoke(home, &["ai-core"], Some(request));
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-sensitive-value"));
    fs::remove_file(&config).unwrap();
    let target = home.join("fixture-configuration");
    fs::write(&target, "fixture-sensitive-value").unwrap();
    symlink(&target, &config).unwrap();
    let output = invoke(home, &["ai-core"], Some(request));
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-sensitive-value"));
    fs::remove_file(&config).unwrap();
    fs::write(&config, " ".repeat(1_048_577)).unwrap();
    assert!(!invoke(home, &["ai-core"], Some(request)).status.success());
}

#[test]
fn usage_provider_inherits_private_mask_and_selected_home_without_copying_auth() {
    let temporary = tempdir().unwrap();
    let home = temporary.path();
    let binary = home.join(".local/bin/codex");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        r#"#!/usr/bin/python3
import json, os, sys
with open(os.path.join(os.environ['HOME'], 'provider-fixture.json'), 'w') as out:
    json.dump({'home':os.environ['CODEX_HOME'],'args':sys.argv[1:]}, out)
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request: continue
    result = {}
    if request['method'] == 'account/rateLimits/read':
        result = {'rateLimits':{'primary':{'usedPercent':25,'windowDurationMins':60,'resetsAt':0}}}
    if request['method'] == 'account/usage/read':
        result = {'summary':{'lifetimeTokens':1500}}
    print(json.dumps({'id':request['id'],'result':result}), flush=True)
"#,
    )
    .unwrap();
    fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
    let history = home.join("old history store");
    let output = Command::new(helper())
        .args(["nacre-ai-usage", "--account", "work", "read"])
        .env("HOME", home)
        .env("NACRE_AI_HISTORY_HOME", &history)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row.as_str().unwrap().contains("1,500")));
    let record = home.join("provider-fixture.json");
    assert_eq!(
        fs::metadata(&record).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let data: Value = serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
    assert_eq!(
        data["args"],
        json!([
            "-c",
            format!("sqlite_home={}", serde_json::to_string(&history).unwrap()),
            "app-server",
            "--stdio"
        ])
    );
    assert_eq!(
        data["home"],
        home.join(".local/share/nacre/ai/accounts/work")
            .to_str()
            .unwrap()
    );
    assert!(!home
        .join(".local/share/nacre/ai/accounts/work/auth.json")
        .exists());
}

#[test]
fn environment_api_returns_deltas_without_inherited_secret_values() {
    let temporary = tempdir().unwrap();
    let mut command = Command::new(helper());
    command
        .args(["ai-core"])
        .env("HOME", temporary.path())
        .env("INHERITED_FIXTURE_SECRET", "fixture-sensitive-environment")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"operation":"environment","agent":"codex"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let delta: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(delta, json!({"set":{},"unset":[]}));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("fixture-sensitive-environment"));
}

#[test]
fn concurrent_account_initialization_publishes_one_private_configuration() {
    let temporary = tempdir().unwrap();
    let home = temporary.path();
    let mut children = Vec::new();
    for _ in 0..8 {
        let mut child = Command::new(helper())
            .arg("ai-core")
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"operation":"environment","agent":"codex","account":"work"}"#)
            .unwrap();
        children.push(child);
    }
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let profile = home.join(".local/share/nacre/ai/accounts/work");
    assert_eq!(
        fs::read_to_string(profile.join("config.toml")).unwrap(),
        "cli_auth_credentials_store = \"auto\"\n"
    );
    assert_eq!(
        fs::metadata(profile.join("config.toml"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(!profile.join("auth.json").exists());
}
