//! B0084: the lock-screen media buttons work with any UI provider. QML hosts the `mpris` target; a
//! provider without it (Bead) refuses, and the daemon's `dashboard.player` projection answers instead.
use nacre_helper::Result;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    process::Command,
    thread,
};

fn welcome(methods: Value) -> Value {
    json!({"t":"welcome","proto":1,"min":1,"boot":"fixture","topics":{},"methods":methods})
}
fn descriptor(class: &str) -> Value {
    json!({"v":1,"class":class,"timeout":3000,"params":{"type":"object"},"result":{}})
}
/// Serves one client: hello, then `script` over the connection; returns the lines the client sent.
fn serve(server: &UnixListener, methods: Value, script: &[Value]) -> Vec<Value> {
    let (stream, _) = server.accept().unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    writeln!(reader.get_mut(), "{}", welcome(methods)).unwrap();
    let mut seen = Vec::new();
    for reply in script {
        line.clear();
        reader.read_line(&mut line).unwrap();
        let sent: Value = serde_json::from_str(&line).unwrap();
        let mut reply = reply.clone();
        if reply["t"] == "ret" {
            reply["id"] = sent["id"].clone();
        }
        seen.push(sent);
        writeln!(reader.get_mut(), "{reply}").unwrap();
    }
    seen
}
fn run(kind: &str, refuse: bool) -> Result<Vec<Value>> {
    let home = tempfile::Builder::new().prefix("lm-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let folder = home.path().join("runtime/nacre");
    fs::create_dir_all(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let server = UnixListener::bind(&endpoint)?;
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
    let daemon = thread::spawn(move || {
        let ui = json!({"ui.dispatch": descriptor("ui")});
        if !refuse {
            return serve(&server, ui, &[json!({"t":"ret","ok":null})]);
        }
        let mut seen = serve(
            &server,
            ui,
            &[json!({"t":"ret","err":{"code":"unavailable","message":"no mpris","retry":false}})],
        );
        let methods = json!({"player.play_pause": descriptor("desktop"), "player.next": descriptor("desktop"), "player.previous": descriptor("desktop")});
        let snap = json!({"t":"snap","topic":"dashboard.player","seq":1,"data":{"health":{"available":true,"stale":false},"active":"player-2","players":{"items":{},"order":["player-1","player-2"]}}});
        seen.extend(serve(
            &server,
            methods,
            &[snap, json!({"t":"ret","ok":{}})],
        ));
        seen
    });
    let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .arg("lock-info")
        .arg(kind)
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "")
        .env("NACRE_LANGUAGE", "en")
        .env("XDG_RUNTIME_DIR", home.path().join("runtime"))
        .env("NACRE_DAEMON_SOCKET", &endpoint)
        .output()?;
    let seen = daemon.join().map_err(|_| "fake daemon failed")?;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    Ok(seen)
}

#[test]
fn a_provider_that_hosts_mpris_gets_the_key_and_nothing_else_is_called() -> Result<()> {
    for (kind, action) in [("toggle", "playPause"), ("next", "next"), ("previous", "previous")] {
        let seen = run(kind, false)?;
        assert_eq!(seen.len(), 1, "{kind}");
        assert_eq!(seen[0]["m"], "ui.dispatch");
        assert_eq!(
            seen[0]["p"],
            json!({"target":"mpris","method":action,"args":[]}),
            "{kind}"
        );
    }
    Ok(())
}

#[test]
fn a_provider_without_mpris_leaves_the_key_to_the_daemon_player_projection() -> Result<()> {
    for (kind, method) in [
        ("toggle", "player.play_pause"),
        ("next", "player.next"),
        ("previous", "player.previous"),
    ] {
        let seen = run(kind, true)?;
        assert_eq!(seen[0]["m"], "ui.dispatch", "{kind}");
        assert_eq!(seen[1]["t"], "sub");
        assert_eq!(seen[1]["topics"], json!(["dashboard.player"]));
        assert_eq!(seen[2]["m"], method, "{kind}");
        assert_eq!(seen[2]["p"], json!({"player":"player-2"}), "{kind}");
    }
    Ok(())
}
