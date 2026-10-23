//! N0048: every registered shell route reaches the UI provider unchanged through `nacrectl ui`.
use nacre_helper::Result;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    process::Command,
    sync::{Arc, Mutex},
    thread,
};

fn registry() -> Result<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../nacre/shell-tools/shell-routes.json"
    );
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

#[test]
fn fake_provider_receives_every_registered_route_exactly() -> Result<()> {
    let registry = registry()?;
    let mut expected = Vec::new();
    for route in registry["routes"].as_array().ok_or("routes")? {
        for example in route["examples"].as_array().ok_or("examples")? {
            expected.push(json!({
                "target": route["target"],
                "method": route["method"],
                "args": example,
            }));
        }
    }
    assert!(expected.len() > 20);
    let home = tempfile::Builder::new().prefix("ur-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let folder = home.path().join("runtime/nacre");
    fs::create_dir_all(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let server = UnixListener::bind(&endpoint)?;
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let (count, recorded) = (expected.len(), seen.clone());
    let provider = thread::spawn(move || {
        for _ in 0..count {
            let (stream, _) = server.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let descriptor = json!({"v":1,"class":"ui","timeout":3000,"params":{"type":"object"},"result":{}});
            writeln!(
                reader.get_mut(),
                "{}",
                json!({"t":"welcome","proto":1,"min":1,"boot":"fixture","topics":{},"methods":{"ui.dispatch":descriptor}})
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let call: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(call["m"], "ui.dispatch");
            recorded.lock().unwrap().push(call["p"].clone());
            writeln!(
                reader.get_mut(),
                "{}",
                json!({"t":"ret","id":call["id"],"ok":null})
            )
            .unwrap();
        }
    });
    for want in &expected {
        let mut arguments = vec![
            "ctl".to_owned(),
            "ui".to_owned(),
            want["target"].as_str().ok_or("target")?.to_owned(),
            want["method"].as_str().ok_or("method")?.to_owned(),
        ];
        for argument in want["args"].as_array().ok_or("args")? {
            arguments.push(argument.as_str().ok_or("argument")?.to_owned());
        }
        let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", "")
            .env("XDG_RUNTIME_DIR", home.path().join("runtime"))
            .env("NACRE_DAEMON_SOCKET", &endpoint)
            .args(&arguments)
            .output()?;
        assert!(output.status.success(), "{arguments:?}");
        assert_eq!(String::from_utf8(output.stdout)?, "null\n", "{arguments:?}");
    }
    provider.join().map_err(|_| "Fake provider failed")?;
    assert_eq!(*seen.lock().unwrap(), expected);
    Ok(())
}
