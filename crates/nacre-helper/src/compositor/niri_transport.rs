//! Persistent, bounded niri IPC. Failed requests are surfaced, never replayed.
use super::{
    niri_actions::Transport,
    niri_state::{Error, State},
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ValueError",
        message: message.into(),
    }
}
fn io_error(error: std::io::Error) -> Error {
    Error {
        kind: if matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ) {
            "TimeoutError"
        } else {
            "OSError"
        },
        message: error.to_string(),
    }
}

pub struct Connection {
    path: PathBuf,
    stream: Option<BufReader<UnixStream>>,
}
impl Connection {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            stream: None,
        }
    }
    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            path: PathBuf::new(),
            stream: Some(BufReader::new(stream)),
        }
    }
    pub fn connected(&self) -> bool {
        self.stream.is_some()
    }
    pub fn invalidate(&mut self) {
        self.stream = None;
    }
    fn stream(&mut self) -> Result<&mut BufReader<UnixStream>> {
        if self.stream.is_none() {
            if self.path.as_os_str().is_empty() {
                return Err(Error {
                    kind: "RuntimeError",
                    message: "NIRI_SOCKET is unavailable".into(),
                });
            }
            self.stream = Some(BufReader::new(
                UnixStream::connect(&self.path).map_err(io_error)?,
            ));
        }
        self.stream
            .as_mut()
            .ok_or_else(|| invalid("Niri connection is unavailable"))
    }
    pub fn request(
        &mut self,
        request: &niri_ipc::Request,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<Value> {
        let value = serde_json::to_value(request).map_err(|e| invalid(e.to_string()))?;
        self.request_value(&value, timeout, max_bytes)
    }
    pub fn request_value(
        &mut self,
        request: &Value,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<Value> {
        if timeout.is_zero() || max_bytes == 0 || max_bytes > 4 * 1024 * 1024 {
            return Err(invalid("Invalid niri IPC bounds"));
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| invalid("Invalid niri IPC deadline"))?;
        let wire = crate::json::encode(request).map_err(|e| invalid(e.to_string()))? + "\n";
        let result = (|| {
            let stream = self.stream()?;
            stream
                .get_mut()
                .set_write_timeout(Some(timeout))
                .map_err(io_error)?;
            let mut pending = wire.as_bytes();
            while !pending.is_empty() {
                let remaining = remaining(deadline)?;
                stream
                    .get_mut()
                    .set_write_timeout(Some(remaining))
                    .map_err(io_error)?;
                let written = stream.get_mut().write(pending).map_err(io_error)?;
                if written == 0 {
                    return Err(invalid("Truncated or oversized niri reply"));
                }
                pending = &pending[written..];
            }
            let line = read_line(
                stream,
                deadline,
                max_bytes,
                "Truncated or oversized niri reply",
            )?;
            let text = std::str::from_utf8(&line).map_err(|error| Error {
                kind: "UnicodeDecodeError",
                message: error.to_string(),
            })?;
            let response: Value = crate::json::parse_value(text).map_err(|error| Error {
                kind: "JSONDecodeError",
                message: error.to_string(),
            })?;
            let object = response
                .as_object()
                .filter(|v| v.len() == 1)
                .ok_or_else(|| invalid("Malformed niri reply"))?;
            if let Some(error) = object.get("Err") {
                return Err(Error {
                    kind: "RuntimeError",
                    message: format!(
                        "niri IPC: {}",
                        error
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| error.to_string())
                    ),
                });
            }
            object
                .get("Ok")
                .cloned()
                .ok_or_else(|| invalid("Missing niri reply status"))
        })();
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    pub fn response(&mut self, name: &str, timeout: Duration, max_bytes: usize) -> Result<Value> {
        let request = match name {
            "Outputs" => niri_ipc::Request::Outputs,
            "Workspaces" => niri_ipc::Request::Workspaces,
            "Windows" => niri_ipc::Request::Windows,
            _ => return Err(invalid("Unknown niri snapshot request")),
        };
        let response = self.request(&request, timeout, max_bytes)?;
        let result = response
            .get(name)
            .cloned()
            .ok_or_else(|| invalid("Unexpected niri response"))?;
        if name == "Outputs"
            && !result
                .as_object()
                .is_some_and(|v| v.values().all(Value::is_object))
        {
            return Err(invalid("Malformed niri output records"));
        }
        Ok(result)
    }
}
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|v| !v.is_zero())
        .ok_or_else(|| Error {
            kind: "TimeoutError",
            message: "Niri IPC deadline exceeded".into(),
        })
}
pub(super) fn read_line(
    stream: &mut BufReader<UnixStream>,
    deadline: Instant,
    maximum: usize,
    message: &str,
) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        stream
            .get_mut()
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        let available = stream.fill_buf().map_err(io_error)?;
        if available.is_empty() {
            return Err(invalid(message));
        }
        let amount = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if amount > maximum.saturating_sub(line.len()) {
            return Err(invalid(message));
        }
        let complete = available[amount - 1] == b'\n';
        line.extend_from_slice(&available[..amount]);
        stream.consume(amount);
        if complete {
            return Ok(line);
        }
    }
}
impl Transport for Connection {
    fn snapshot(&mut self, timeout: f64, max_bytes: usize) -> Result<Value> {
        let timeout = duration(timeout)?;
        let outputs = self
            .response("Outputs", timeout, max_bytes)?
            .as_object()
            .cloned()
            .ok_or_else(|| invalid("Malformed niri output records"))?;
        let mut state = State::new(outputs);
        state.apply(&json!({"WorkspacesChanged":{"workspaces":self.response("Workspaces",timeout,max_bytes)?}}))?;
        state.apply(
            &json!({"WindowsChanged":{"windows":self.response("Windows",timeout,max_bytes)?}}),
        )?;
        state.snapshot()
    }
    fn request(&mut self, value: Value, timeout: f64, max_bytes: usize) -> Result<Value> {
        self.request_value(&value, duration(timeout)?, max_bytes)
    }
}
fn duration(seconds: f64) -> Result<Duration> {
    Duration::try_from_secs_f64(seconds)
        .ok()
        .filter(|v| !v.is_zero())
        .ok_or_else(|| invalid("Invalid niri IPC timeout"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Read, os::unix::net::UnixListener, thread};
    #[test]
    fn frozen_request_bytes_and_reply_contracts() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-transport-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let (client, server) = UnixStream::pair()?;
            let response = case["request"]["reply"]
                .as_str()
                .ok_or("Missing reply")?
                .as_bytes()
                .to_vec();
            let worker = thread::spawn(move || -> std::io::Result<Vec<u8>> {
                let mut server = BufReader::new(server);
                let mut wire = Vec::new();
                server.read_until(b'\n', &mut wire)?;
                let split = response.len().min(4);
                server.get_mut().write_all(&response[..split])?;
                server.get_mut().write_all(&response[split..])?;
                Ok(wire)
            });
            let mut connection = Connection::from_stream(client);
            let result = connection.request_value(
                &case["request"]["request"],
                Duration::from_secs(1),
                case["request"]["max_bytes"]
                    .as_u64()
                    .ok_or("Missing maximum")? as usize,
            );
            let (value, exit) = match result {
                Ok(value) => {
                    assert!(connection.connected());
                    (value, 0)
                }
                Err(error) => {
                    assert!(!connection.connected());
                    (json!({"error":error.message,"type":error.kind}), 1)
                }
            };
            assert_eq!(
                crate::json::encode(&value)? + "\n",
                case["observed"]["stdout"]
                    .as_str()
                    .ok_or("Missing stdout")?,
                "{}",
                case["request"]["name"]
            );
            assert_eq!(json!(exit), case["observed"]["exit"]);
            let wire = worker.join().map_err(|_| "Peer panicked")??;
            let hex = wire.iter().map(|b| format!("{b:02x}")).collect::<String>();
            assert_eq!(
                hex,
                case["observed"]["wire_hex"]
                    .as_str()
                    .ok_or("Missing wire")?
            );
        }
        Ok(())
    }
    #[test]
    fn multiple_requests_share_one_connection_and_keep_buffered_frames() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        let path = home.path().join("ipc.sock");
        let listener = UnixListener::bind(&path)?;
        let worker = thread::spawn(move || -> std::io::Result<Vec<Value>> {
            let (stream, _) = listener.accept()?;
            let mut stream = BufReader::new(stream);
            let mut requests = Vec::new();
            for _ in 0..3 {
                let mut line = String::new();
                stream.read_line(&mut line)?;
                requests.push(serde_json::from_str(&line).map_err(std::io::Error::other)?);
                stream.get_mut().write_all(b"{\"Ok\":\"Handled\"}\n")?;
            }
            Ok(requests)
        });
        let mut connection = Connection::new(&path);
        for request in [
            json!("Version"),
            json!("Windows"),
            json!({"Action":{"CloseWindow":{"id":1}}}),
        ] {
            assert_eq!(
                connection.request_value(&request, Duration::from_secs(1), 128)?,
                "Handled"
            );
        }
        let requests = worker.join().map_err(|_| "Peer panicked")??;
        assert_eq!(requests.len(), 3);
        assert!(connection.connected());
        drop(connection);
        fs::remove_file(path)?;
        Ok(())
    }
    #[test]
    fn partial_reads_obey_total_deadline_and_never_replay_action() -> crate::Result<()> {
        let (client, mut server) = UnixStream::pair()?;
        let worker = thread::spawn(move || -> std::io::Result<Vec<u8>> {
            server.set_read_timeout(Some(Duration::from_secs(1)))?;
            let mut request = Vec::new();
            let mut byte = [0];
            while server.read(&mut byte)? != 0 {
                request.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            server.write_all(b"{")?;
            thread::sleep(Duration::from_millis(80));
            let mut remainder = Vec::new();
            server.read_to_end(&mut remainder)?;
            assert!(remainder.is_empty());
            Ok(request)
        });
        let mut connection = Connection::from_stream(client);
        let error = connection
            .request_value(
                &json!({"Action":{"CloseWindow":{"id":1}}}),
                Duration::from_millis(20),
                128,
            )
            .expect_err("Partial response must expire");
        assert_eq!(error.kind, "TimeoutError");
        assert!(!connection.connected());
        let wire = worker.join().map_err(|_| "Peer panicked")??;
        assert_eq!(wire.iter().filter(|b| **b == b'\n').count(), 1);
        Ok(())
    }
    #[test]
    fn snapshot_reads_all_native_data_on_one_persistent_connection() -> crate::Result<()> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../nacre/shell-tools/tests/fixtures/niri-event-stream.json"
        ))?;
        let expected = fixture.clone();
        let (client, server) = UnixStream::pair()?;
        let worker = thread::spawn(move || -> std::io::Result<()> {
            let mut server = BufReader::new(server);
            for (name, result) in [
                ("Outputs", fixture["outputs"].clone()),
                (
                    "Workspaces",
                    fixture["events"][0]["WorkspacesChanged"]["workspaces"].clone(),
                ),
                (
                    "Windows",
                    fixture["events"][1]["WindowsChanged"]["windows"].clone(),
                ),
            ] {
                let mut line = String::new();
                server.read_line(&mut line)?;
                assert_eq!(
                    serde_json::from_str::<Value>(&line).map_err(std::io::Error::other)?,
                    json!(name)
                );
                let mut payload = serde_json::Map::new();
                payload.insert(name.into(), result);
                let wire =
                    serde_json::to_vec(&json!({"Ok":payload})).map_err(std::io::Error::other)?;
                server.get_mut().write_all(&wire)?;
                server.get_mut().write_all(b"\n")?;
            }
            Ok(())
        });
        let mut connection = Connection::from_stream(client);
        let actual = Transport::snapshot(&mut connection, 1.0, 4 * 1024 * 1024)?;
        let mut state = State::new(
            expected["outputs"]
                .as_object()
                .ok_or("Missing outputs")?
                .clone(),
        );
        for event in expected["events"]
            .as_array()
            .ok_or("Missing events")?
            .iter()
            .take(2)
        {
            state.apply(event)?;
        }
        assert_eq!(
            crate::json::encode(&actual)?,
            crate::json::encode(&state.snapshot()?)?
        );
        assert!(connection.connected());
        worker.join().map_err(|_| "Peer panicked")??;
        Ok(())
    }
}
