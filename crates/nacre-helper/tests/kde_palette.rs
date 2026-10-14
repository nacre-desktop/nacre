use nacre_helper::Result;
use serde_json::json;
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn empty_path_publisher_preserves_custom_settings_without_child_processes() -> Result<()> {
    let home = tempfile::tempdir()?;
    fs::create_dir(home.path().join(".config"))?;
    fs::write(
        home.path().join(".config/kdeglobals"),
        "[General]\nTerminalApplication=kitty\n",
    )?;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/kde-palette-reference.json"))?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["kde-palette", "--publish"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().ok_or("Missing stdin")?.write_all(json!({"home":home.path(),"colors":fixture["cases"][0]["colors"],"icon_theme":"ExampleIcons"}).to_string().as_bytes())?;
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(fs::read_to_string(home.path().join(".config/kdeglobals"))?
        .contains("TerminalApplication=kitty\n"));
    assert!(home
        .path()
        .join(".local/share/color-schemes/Nacre.colors")
        .is_file());
    Ok(())
}
#[test]
fn retired_module_empty_invocation_remains_silent() -> Result<()> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/kde-palette-cli-reference.json"))?;
    for case in fixture["cases"].as_array().ok_or("Missing CLI cases")? {
        let args: Vec<String> = serde_json::from_value(case["arguments"].clone())?;
        let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("kde-palette")
            .args(args)
            .env_clear()
            .output()?;
        assert_eq!(
            output.status.code(),
            case["exit"].as_i64().map(|value| value as i32)
        );
        assert_eq!(String::from_utf8(output.stdout)?, case["stdout"]);
        assert_eq!(String::from_utf8(output.stderr)?, case["stderr"]);
    }
    Ok(())
}
