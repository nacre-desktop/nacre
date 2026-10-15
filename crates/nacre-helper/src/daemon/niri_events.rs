//! One-release native event compatibility client; the daemon owns the niri stream.
use super::{client, socket};
use crate::Result;
use nacre_protocol::{framing, Message, Mirror, Update};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
    },
    path::Path,
    time::Duration,
};

pub const TOPIC: &str = "compositor.events";

pub fn identity(path: &Path) -> Result<String> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.file_type().is_socket() {
        return Err("Selected compositor endpoint is not a socket".into());
    }
    Ok(format!("{:x}:{:x}", metadata.dev(), metadata.ino()))
}

fn rows(data: &Value, collection: &str, order: &str) -> Result<Vec<Value>> {
    let values = data[collection]
        .as_object()
        .ok_or("Invalid daemon compositor collection")?;
    let order = data[order]
        .as_array()
        .ok_or("Invalid daemon compositor order")?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for id in order {
        let id = id.as_str().ok_or("Invalid daemon compositor ID")?;
        if !seen.insert(id) {
            return Err("Duplicate daemon compositor ID".into());
        }
        result.push(
            values
                .get(id)
                .ok_or("Missing daemon compositor entity")?
                .clone(),
        );
    }
    if result.len() != values.len() {
        return Err("Unordered daemon compositor entity".into());
    }
    Ok(result)
}

/// Restore the existing command's JSON shape without maintaining a second reducer.
pub fn snapshot(data: &Value) -> Result<Value> {
    if data["backend"] != "niri" || data["available"] != true {
        return Err("Niri daemon state is unavailable".into());
    }
    let selected = |collection: &str, key: &str| {
        data["focus"][key]
            .as_str()
            .and_then(|id| data[collection].get(id))
            .cloned()
            .unwrap_or(Value::Null)
    };
    Ok(json!({
        "type":"snapshot", "backend":"niri", "capabilities":data["capabilities"],
        "monitors":rows(data,"outputs","outputOrder")?,
        "workspaces":rows(data,"workspaces","workspaceOrder")?,
        "windows":rows(data,"windows","windowOrder")?,
        "activeWindow":selected("windows","windowId"),
        "activeWorkspace":selected("workspaces","workspaceId"),
        "focusedMonitor":selected("outputs","outputId"),
        "overviewOpen":data["overviewOpen"],
        "overviewHoverSupported":data["overviewHoverSupported"],
        "overviewHoveredWindowId":data["overviewHoveredWindowId"],
        "configFailed":data["configFailed"]
    }))
}

pub struct Source {
    runtime: tokio::runtime::Runtime,
    inner: Inner,
}
struct Inner {
    client: client::Client,
    mirror: Mirror,
    identity: String,
    identified: bool,
    awaiting: BTreeSet<String>,
}
impl Source {
    pub fn connect(environment: &BTreeMap<String, String>, niri: &Path) -> Result<Option<Self>> {
        let selected = environment.get("NACRE_DAEMON_SOCKET");
        let runtime = environment.get("XDG_RUNTIME_DIR");
        if selected.is_none() && runtime.is_none_or(|path| !Path::new(path).is_absolute()) {
            return Ok(None);
        }
        let path = socket::path_from(selected.map(OsStr::new), runtime.map(OsStr::new))?;
        if let Err(error) = socket::validate_endpoint(&path) {
            if selected.is_none()
                && error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                return Ok(None);
            }
            return Err(error);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(2)
            .build()?;
        let client = match runtime.block_on(client::connect_at(&path, "niri-events")) {
            Ok(client) => client,
            Err(client::ConnectError::Absent) if selected.is_none() => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !matches!(&client.welcome, Message::Welcome {topics,..} if topics.get(TOPIC)==Some(&1)) {
            if selected.is_none() {
                return Ok(None);
            }
            return Err("Daemon compositor event lane is unavailable".into());
        }
        let mut inner = Inner {
            client,
            mirror: Mirror::default(),
            identity: identity(niri)?,
            identified: false,
            awaiting: BTreeSet::new(),
        };
        inner.mirror.receive(&inner.client.welcome);
        runtime.block_on(async {
            framing::write(
                &mut inner.client.writer,
                &Message::Sub {
                    topics: vec!["compositor".into(), TOPIC.into()],
                },
            )
            .await
        })?;
        Ok(Some(Self { runtime, inner }))
    }
    pub fn control(&self) -> Result<UnixStream> {
        self.inner.client.control()
    }
    pub fn next_value(&mut self) -> Result<Value> {
        if self.inner.identified {
            self.runtime.block_on(self.inner.next_value())
        } else {
            self.runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(5), self.inner.next_value())
                    .await
                    .map_err(|_| "Daemon compositor startup timed out")?
            })
        }
    }
}
impl Inner {
    async fn next_value(&mut self) -> Result<Value> {
        loop {
            let message = self
                .client
                .reader
                .next()
                .await?
                .ok_or("Daemon compositor stream ended")?;
            match &message {
                Message::Error { err } => return Err(err.clone().into()),
                Message::Lost { topic, .. } if topic == TOPIC => {
                    eprintln!("Compositor event history overflowed; refreshing state");
                    self.awaiting.extend(["compositor".into(), TOPIC.into()]);
                    framing::write(
                        &mut self.client.writer,
                        &Message::Resync {
                            topics: vec!["compositor".into(), TOPIC.into()],
                        },
                    )
                    .await?;
                    continue;
                }
                Message::Ev {
                    topic, name, data, ..
                } if topic == TOPIC && name == "niri" => {
                    if !self.identified || !data.is_object() || !data["type"].is_string() {
                        return Err("Invalid daemon compositor event".into());
                    }
                    if !self.awaiting.is_empty() {
                        continue;
                    }
                    return Ok(data.clone());
                }
                _ => {}
            }
            let was_resyncing = !self.awaiting.is_empty();
            let update = self.mirror.receive(&message);
            if let Update::Resync(_) = update {
                eprintln!("Compositor state chain changed; refreshing state");
                self.awaiting.extend(["compositor".into(), TOPIC.into()]);
                framing::write(
                    &mut self.client.writer,
                    &Message::Resync {
                        topics: vec!["compositor".into(), TOPIC.into()],
                    },
                )
                .await?;
                continue;
            }
            if let Message::Snap { topic, .. } = &message {
                self.awaiting.remove(topic);
            }
            let was_identified = self.identified;
            if let Some((_, metadata)) = self.mirror.topics.get(TOPIC) {
                if metadata["available"] == true {
                    if metadata["backend"] != "niri" || metadata["connectionId"] != self.identity {
                        return Err("Daemon belongs to a different compositor session".into());
                    }
                    self.identified = true;
                }
            }
            if let Some((_, data)) = self.mirror.topics.get("compositor") {
                if was_identified
                    && data["available"] == false
                    && matches!(update,Update::Changed(ref name) if name=="compositor")
                {
                    return Err("Niri daemon service is unavailable".into());
                }
                if self.identified
                    && self.awaiting.is_empty()
                    && data["available"] == true
                    && (!was_identified
                        || was_resyncing
                        || matches!(update,Update::Changed(ref name) if name=="compositor"))
                {
                    return snapshot(data);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        os::unix::{fs::PermissionsExt, net::UnixListener},
        thread,
    };

    fn model() -> Value {
        super::super::compositor::project(&json!({"monitors":[],"workspaces":[],"windows":[],"activeWindow":null,"activeWorkspace":null,"focusedMonitor":null}),"niri",&json!({"names":[],"currentIndex":null,"activeName":""})).unwrap()
    }
    fn write(stream: &mut UnixStream, value: Value) {
        writeln!(stream, "{value}").unwrap();
    }
    fn read(stream: &mut BufReader<UnixStream>) -> Value {
        let mut line = String::new();
        stream.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn fixture(root: &Path) -> Result<(UnixListener, UnixListener, BTreeMap<String, String>)> {
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let niri = UnixListener::bind(root.join("niri.sock"))?;
        let server = UnixListener::bind(root.join("daemon.sock"))?;
        fs::set_permissions(root.join("daemon.sock"), fs::Permissions::from_mode(0o600))?;
        Ok((
            niri,
            server,
            BTreeMap::from([(
                "NACRE_DAEMON_SOCKET".into(),
                root.join("daemon.sock").to_string_lossy().into_owned(),
            )]),
        ))
    }
    fn accept(server: UnixListener) -> BufReader<UnixStream> {
        let mut stream = BufReader::new(server.accept().unwrap().0);
        stream
            .get_ref()
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        assert_eq!(read(&mut stream)["client"], "niri-events");
        write(
            stream.get_mut(),
            json!({"t":"welcome","proto":1,"min":1,"boot":"fixture","topics":{"compositor":1,"compositor.events":1},"methods":{}}),
        );
        assert_eq!(read(&mut stream)["t"], "sub");
        stream
    }
    fn snap(stream: &mut UnixStream, seq: u64, data: Value, identity: &str) {
        write(
            stream,
            json!({"t":"snap","topic":"compositor","seq":seq,"data":data}),
        );
        write(
            stream,
            json!({"t":"snap","topic":TOPIC,"seq":0,"data":{"backend":"niri","available":true,"connectionId":identity}}),
        );
    }

    #[test]
    fn private_proxy_replays_semantic_events_and_resyncs_before_releasing_more() -> Result<()> {
        let root = tempfile::Builder::new().prefix("np-").tempdir()?;
        let (_niri, server, environment) = fixture(root.path())?;
        let identity = identity(&root.path().join("niri.sock"))?;
        let worker = thread::spawn(move || {
            let mut stream = accept(server);
            snap(stream.get_mut(), 0, model(), &identity);
            write(
                stream.get_mut(),
                json!({"t":"ev","topic":TOPIC,"name":"niri","seq":1,"data":{"type":"configuration-reloaded","failed":false}}),
            );
            write(
                stream.get_mut(),
                json!({"t":"patch","topic":"compositor","base":90,"seq":91,"data":{"configFailed":"must-not-publish"}}),
            );
            write(
                stream.get_mut(),
                json!({"t":"ev","topic":TOPIC,"name":"niri","seq":2,"data":{"type":"must-not-release-before-resync"}}),
            );
            assert_eq!(read(&mut stream)["t"], "resync");
            let mut next = model();
            next["configFailed"] = json!(true);
            snap(stream.get_mut(), 3, next, &identity);
            write(
                stream.get_mut(),
                json!({"t":"ev","topic":TOPIC,"name":"niri","seq":3,"data":{"type":"configuration-reloaded","failed":true}}),
            );
        });
        let mut source = Source::connect(&environment, &root.path().join("niri.sock"))?
            .ok_or("Missing proxy")?;
        assert_eq!(source.next_value()?["type"], "snapshot");
        assert_eq!(
            source.next_value()?,
            json!({"type":"configuration-reloaded","failed":false})
        );
        let next = source.next_value()?;
        assert_eq!(next["type"], "snapshot");
        assert_eq!(next["configFailed"], true);
        assert_eq!(
            source.next_value()?,
            json!({"type":"configuration-reloaded","failed":true})
        );
        worker.join().map_err(|_| "Proxy fixture failed")?;
        Ok(())
    }
    #[test]
    fn mismatched_session_and_explicit_missing_socket_never_open_a_native_stream() -> Result<()> {
        let root = tempfile::Builder::new().prefix("np-").tempdir()?;
        let (niri, server, environment) = fixture(root.path())?;
        let worker = thread::spawn(move || {
            let mut stream = accept(server);
            snap(stream.get_mut(), 0, model(), "another-session");
        });
        let mut source = Source::connect(&environment, &root.path().join("niri.sock"))?
            .ok_or("Missing proxy")?;
        assert!(source
            .next_value()
            .unwrap_err()
            .to_string()
            .contains("different compositor session"));
        niri.set_nonblocking(true)?;
        assert_eq!(
            niri.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        worker.join().map_err(|_| "Proxy fixture failed")?;
        assert!(Source::connect(
            &BTreeMap::from([(
                "NACRE_DAEMON_SOCKET".into(),
                root.path()
                    .join("absent.sock")
                    .to_string_lossy()
                    .into_owned()
            )]),
            &root.path().join("niri.sock")
        )
        .is_err());
        assert!(Source::connect(&BTreeMap::new(), &root.path().join("niri.sock"))?.is_none());
        Ok(())
    }
    #[test]
    fn previous_daemon_capability_falls_back_only_for_the_default_endpoint() -> Result<()> {
        for explicit in [false, true] {
            let root = tempfile::Builder::new().prefix("np-").tempdir()?;
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
            let directory = root.path().join("nacre");
            fs::create_dir(&directory)?;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            let endpoint = directory.join("daemon.sock");
            let server = UnixListener::bind(&endpoint)?;
            fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
            let niri = UnixListener::bind(root.path().join("niri.sock"))?;
            let environment = if explicit {
                BTreeMap::from([(
                    "NACRE_DAEMON_SOCKET".into(),
                    endpoint.to_string_lossy().into_owned(),
                )])
            } else {
                BTreeMap::from([(
                    "XDG_RUNTIME_DIR".into(),
                    root.path().to_string_lossy().into_owned(),
                )])
            };
            let worker = thread::spawn(move || {
                let mut stream = BufReader::new(server.accept().unwrap().0);
                stream
                    .get_ref()
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                assert_eq!(read(&mut stream)["t"], "hello");
                write(
                    stream.get_mut(),
                    json!({"t":"welcome","proto":1,"min":1,"boot":"previous","topics":{"compositor":1},"methods":{}}),
                );
                assert_eq!(stream.read_line(&mut String::new()).unwrap(), 0);
            });
            let connected = Source::connect(&environment, &root.path().join("niri.sock"));
            if explicit {
                assert!(connected.is_err());
            } else {
                assert!(connected?.is_none());
            }
            worker.join().map_err(|_| "Capability fixture failed")?;
            niri.set_nonblocking(true)?;
            assert_eq!(
                niri.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
        Ok(())
    }

    #[test]
    fn frozen_snapshot_round_trip_reports_only_explicit_projection_differences() -> Result<()> {
        fn clear_unresolved_active_windows(value: &mut Value, windows: &BTreeSet<String>) {
            match value {
                Value::Object(object) => {
                    if object
                        .get("activeWindowId")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !windows.contains(id))
                    {
                        object.insert("activeWindowId".into(), Value::Null);
                    }
                    for child in object.values_mut() {
                        clear_unresolved_active_windows(child, windows);
                    }
                }
                Value::Array(array) => {
                    for child in array {
                        clear_unresolved_active_windows(child, windows);
                    }
                }
                _ => {}
            }
        }
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-state-reference.json"
        ))?;
        let mut differences = 0;
        let mut count = 0;
        for (case_index, case) in reference["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .enumerate()
        {
            for (index, observation) in case["observations"]
                .as_array()
                .ok_or("Missing observations")?
                .iter()
                .enumerate()
            {
                let original = &observation["snapshot"];
                let projected = super::super::compositor::project(
                    original,
                    "niri",
                    &json!({"names":[],"currentIndex":null,"activeName":""}),
                )?;
                let restored = snapshot(&projected)?;
                let mut expected = original.clone();
                // These frozen cases deliberately omit all outputs or disable
                // DP-2. The daemon must hide their dangling desktop entities.
                match case["request"]["name"].as_str() {
                    Some("outputs-absent-never-invent-geometry") => {
                        assert_eq!(expected["monitors"], json!([]));
                        expected["workspaces"] = json!([]);
                        expected["windows"] = json!([]);
                        expected["activeWindow"] = Value::Null;
                        expected["activeWorkspace"] = Value::Null;
                    }
                    Some("disabled-output-never-invent-geometry") => {
                        assert_eq!(expected["monitors"].as_array().unwrap().len(), 1);
                        assert_eq!(expected["monitors"][0]["name"], "DP-1");
                        expected["workspaces"]
                            .as_array_mut()
                            .unwrap()
                            .retain(|workspace| {
                                ["niri-workspace:101", "niri-workspace:102"]
                                    .contains(&workspace["id"].as_str().unwrap())
                            });
                        expected["windows"]
                            .as_array_mut()
                            .unwrap()
                            .retain(|window| window["windowId"] == "niri-window:1");
                        expected["activeWindow"] = Value::Null;
                        expected["activeWorkspace"] = Value::Null;
                    }
                    _ => {}
                }
                // Startup and window-close observations retain stale active
                // window references in Python. Only those references clear.
                let windows = expected["windows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|window| window["windowId"].as_str().unwrap().to_owned())
                    .collect();
                clear_unresolved_active_windows(&mut expected, &windows);
                assert_eq!(restored, expected, "case {case_index}, observation {index}");
                differences += usize::from(restored != *original);
                count += 1;
            }
        }
        assert_eq!(count, 79);
        assert_eq!(differences, 25);
        Ok(())
    }
}
