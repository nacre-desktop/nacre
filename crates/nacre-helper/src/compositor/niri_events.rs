//! Event-driven niri snapshots and semantic deltas, with negotiated hover stream.
use super::{
    niri_state::{Error, State},
    niri_transport::{self, Connection},
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Error>;
const MAX_BYTES: usize = 4 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ValueError",
        message: message.into(),
    }
}
fn io_error(error: std::io::Error) -> Error {
    Error {
        kind: "OSError",
        message: error.to_string(),
    }
}
fn parse(bytes: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(bytes).map_err(|e| Error {
        kind: "UnicodeDecodeError",
        message: e.to_string(),
    })?;
    crate::json::parse_value(text).map_err(|e| Error {
        kind: "JSONDecodeError",
        message: e.to_string(),
    })
}

pub struct EventSocket {
    stream: BufReader<UnixStream>,
}
impl EventSocket {
    pub fn read_before(&mut self, deadline: Instant) -> Result<Option<Value>> {
        let Some(timeout) = deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
        else {
            return Ok(None);
        };
        self.stream
            .get_mut()
            .set_nonblocking(false)
            .map_err(io_error)?;
        let read = niri_transport::read_line(
            &mut self.stream,
            Instant::now() + timeout,
            MAX_BYTES,
            "Truncated or oversized niri event",
        );
        match read {
            Ok(bytes) => parse(&bytes).map(Some),
            Err(error) if error.kind == "TimeoutError" => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn connect(path: &Path, hover: bool) -> Result<Self> {
        let requests: &[&str] = if hover {
            &["NacreOverviewHoverStream", "EventStream"]
        } else {
            &["EventStream"]
        };
        Self::connect_requests(
            || UnixStream::connect(path).map_err(io_error),
            requests,
            MAX_BYTES,
        )
    }
    pub fn connect_collections(path: &Path, hover: bool) -> Result<Self> {
        Self::connect_collections_with(|| UnixStream::connect(path).map_err(io_error), hover)
    }
    fn connect_collections_with(
        open: impl FnMut() -> Result<UnixStream>,
        hover: bool,
    ) -> Result<Self> {
        let mut requests = Vec::new();
        if hover {
            requests.push("NacreOverviewHoverStreamWithCollections");
        }
        requests.push("EventStreamWithCollections");
        requests.push("EventStream");
        Self::connect_requests(open, &requests, MAX_BYTES)
    }
    pub fn connect_edge_triggers(path: &Path) -> Result<Self> {
        Self::connect_requests(
            || UnixStream::connect(path).map_err(io_error),
            &["NacreEdgeTriggerStream"],
            MAX_BYTES,
        )
    }
    pub fn connect_with(
        mut open: impl FnMut() -> Result<UnixStream>,
        hover: bool,
        maximum: usize,
    ) -> Result<Self> {
        let requests: &[&str] = if hover {
            &["NacreOverviewHoverStream", "EventStream"]
        } else {
            &["EventStream"]
        };
        Self::connect_requests(&mut open, requests, maximum)
    }
    fn connect_requests(
        mut open: impl FnMut() -> Result<UnixStream>,
        requests: &[&str],
        maximum: usize,
    ) -> Result<Self> {
        for request in requests {
            let mut stream = BufReader::new(open()?);
            stream
                .get_mut()
                .set_write_timeout(Some(Duration::from_secs(10)))
                .map_err(io_error)?;
            let wire = crate::json::encode(request).map_err(|e| invalid(e.to_string()))? + "\n";
            stream
                .get_mut()
                .write_all(wire.as_bytes())
                .map_err(io_error)?;
            let acknowledgement = niri_transport::read_line(
                &mut stream,
                Instant::now() + Duration::from_secs(10),
                maximum,
                "Invalid event stream acknowledgement",
            )?;
            let reply = parse(&acknowledgement)?;
            if reply == json!({"Ok":"Handled"}) {
                stream.get_mut().set_read_timeout(None).map_err(io_error)?;
                stream.get_mut().set_nonblocking(true).map_err(io_error)?;
                return Ok(Self { stream });
            }
            if *request == "EventStream" {
                return Err(Error {
                    kind: "RuntimeError",
                    message: "niri event stream was rejected".into(),
                });
            }
            // A valid rejection of the optional extension falls back once to
            // stock. Malformed frames/JSON propagate before reaching here.
        }
        Err(Error {
            kind: "RuntimeError",
            message: "niri event stream was rejected".into(),
        })
    }
    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            stream: BufReader::new(stream),
        }
    }
    pub fn into_stream(self) -> BufReader<UnixStream> {
        self.stream
    }
    pub fn next_event(&mut self) -> Result<Value> {
        self.stream
            .get_mut()
            .set_nonblocking(false)
            .map_err(io_error)?;
        self.stream
            .get_mut()
            .set_read_timeout(None)
            .map_err(io_error)?;
        let mut line = Vec::new();
        let mut limited = (&mut self.stream).take((MAX_BYTES + 1) as u64);
        let count = limited.read_until(b'\n', &mut line).map_err(io_error)?;
        if count == 0 {
            return Err(Error {
                kind: "ConnectionError",
                message: "niri event stream disconnected".into(),
            });
        }
        if count > MAX_BYTES || line.last() != Some(&b'\n') {
            return Err(invalid("Truncated or oversized niri event"));
        }
        parse(&line)
    }
}

/// Validate and normalize the fork's independent edge event stream.
pub fn edge_trigger(value: &Value) -> Result<Option<Value>> {
    let object = value
        .as_object()
        .filter(|object| object.len() == 1)
        .ok_or_else(|| invalid("Malformed niri edge event"))?;
    let (variant, event_type) = if object.contains_key("EdgeTriggered") {
        ("EdgeTriggered", "edge-triggered")
    } else if object.contains_key("EdgeLeft") {
        ("EdgeLeft", "edge-left")
    } else {
        return Ok(None);
    };
    let payload = object
        .get(variant)
        .and_then(Value::as_object)
        .filter(|payload| payload.len() == 3)
        .ok_or_else(|| invalid("Malformed niri edge event"))?;
    let output = payload
        .get("output")
        .and_then(Value::as_str)
        .filter(|output| {
            !output.is_empty() && output.len() <= 256 && !output.chars().any(char::is_control)
        })
        .ok_or_else(|| invalid("Malformed niri edge output"))?;
    let edge = match payload.get("edge").and_then(Value::as_str) {
        Some("Left") => "left",
        Some("Right") => "right",
        Some("Top") => "top",
        Some("Bottom") => "bottom",
        _ => return Err(invalid("Malformed niri edge name")),
    };
    let position = payload
        .get("position")
        .and_then(Value::as_array)
        .filter(|position| position.len() == 2)
        .and_then(|position| {
            Some([
                position[0].as_f64().filter(|value| value.is_finite())?,
                position[1].as_f64().filter(|value| value.is_finite())?,
            ])
        })
        .ok_or_else(|| invalid("Malformed niri edge position"))?;
    Ok(Some(json!({
        "type":event_type,
        "output":output,
        "edge":edge,
        "position":position
    })))
}

pub trait Outputs {
    fn outputs(&mut self) -> Result<Value>;
}
impl Outputs for Connection {
    fn outputs(&mut self) -> Result<Value> {
        self.response("Outputs", Duration::from_secs(10), MAX_BYTES)
    }
}
pub struct Events<T: Outputs> {
    socket: Option<EventSocket>,
    daemon: Option<crate::daemon::niri_events::Source>,
    pub state: State,
    outputs: T,
    pending: VecDeque<Value>,
    failed: bool,
}
impl<T: Outputs> Events<T> {
    /// A duplicate descriptor lets an owning observer interrupt a blocked read.
    pub(crate) fn control(&self) -> Result<UnixStream> {
        if let Some(daemon) = &self.daemon {
            return daemon.control().map_err(|error| invalid(error.to_string()));
        }
        self.socket
            .as_ref()
            .ok_or_else(|| invalid("niri event stream disconnected"))?
            .stream
            .get_ref()
            .try_clone()
            .map_err(io_error)
    }
    pub fn new(socket: EventSocket, mut outputs: T) -> Result<Self> {
        let initial = outputs
            .outputs()?
            .as_object()
            .cloned()
            .ok_or_else(|| invalid("Malformed niri output records"))?;
        Ok(Self {
            socket: Some(socket),
            daemon: None,
            state: State::new(initial),
            outputs,
            pending: VecDeque::new(),
            failed: false,
        })
    }
    pub(crate) fn from_daemon(daemon: crate::daemon::niri_events::Source, outputs: T) -> Self {
        Self {
            socket: None,
            daemon: Some(daemon),
            state: State::new(serde_json::Map::new()),
            outputs,
            pending: VecDeque::new(),
            failed: false,
        }
    }
    pub fn next_value(&mut self) -> Result<Value> {
        if self.failed {
            return Err(Error {
                kind: "ConnectionError",
                message: "niri event stream disconnected".into(),
            });
        }
        if let Some(daemon) = &mut self.daemon {
            return daemon
                .next_value()
                .map_err(|error| invalid(error.to_string()));
        }
        let result = self.read_next();
        if result.is_err() {
            self.socket = None;
            self.failed = true;
        }
        result
    }
    fn read_next(&mut self) -> Result<Value> {
        loop {
            if let Some(value) = self.pending.pop_front() {
                return Ok(value);
            }
            let event = self
                .socket
                .as_mut()
                .ok_or_else(|| invalid("Niri event socket is unavailable"))?
                .next_event()?;
            let normalized = self.state.apply(&event)?;
            let mut outputs_changed = false;
            if event.get("WorkspacesChanged").is_some() || event.get("ConfigLoaded").is_some() {
                let outputs = self
                    .outputs
                    .outputs()?
                    .as_object()
                    .cloned()
                    .ok_or_else(|| invalid("Malformed niri output records"))?;
                outputs_changed = outputs != self.state.outputs;
                self.state.outputs = outputs;
            }
            if self.state.ready.len() == 2
                && self.state.ready.contains("window")
                && self.state.ready.contains("workspace")
                && normalized["type"] != "other"
            {
                self.pending.push_back(self.state.snapshot()?);
                self.pending.push_back(normalized.clone());
                if let Some(changes) = normalized.get("changes").and_then(Value::as_array) {
                    for kind in changes.iter().skip(1) {
                        self.pending
                            .push_back(json!({"type":kind,"windowId":normalized["windowId"]}));
                    }
                }
                if outputs_changed {
                    self.pending.push_back(json!({"type":"outputs-changed"}));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    struct FixtureOutputs {
        values: Vec<Value>,
        queries: Vec<Value>,
    }
    impl Outputs for FixtureOutputs {
        fn outputs(&mut self) -> Result<Value> {
            let index = self.queries.len().min(self.values.len() - 1);
            self.queries.push(json!({"name":"Outputs","kwargs":{}}));
            Ok(self.values[index].clone())
        }
    }
    #[test]
    fn native_edge_stream_handshake_and_event_shape_are_validated() -> crate::Result<()> {
        let (client, mut server) = UnixStream::pair()?;
        let peer = thread::spawn(move || -> std::io::Result<()> {
            let mut reader = BufReader::new(server.try_clone()?);
            let mut request = String::new();
            reader.read_line(&mut request)?;
            assert_eq!(request, "\"NacreEdgeTriggerStream\"\n");
            writeln!(server, "{{\"Ok\":\"Handled\"}}")?;
            writeln!(server, "{{\"WorkspacesChanged\":{{\"workspaces\":[]}}}}")?;
            writeln!(
                server,
                "{{\"OverviewOpenedOrClosed\":{{\"is_open\":true}}}}"
            )?;
            writeln!(server, "{{\"EdgeTriggered\":{{\"output\":\"DP-1\",\"edge\":\"Left\",\"position\":[4.0,720.0]}}}}")?;
            writeln!(server, "{{\"EdgeLeft\":{{\"output\":\"DP-1\",\"edge\":\"Left\",\"position\":[8.0,720.0]}}}}")
        });
        let mut socket = EventSocket::connect_requests(
            || client.try_clone().map_err(io_error),
            &["NacreEdgeTriggerStream"],
            MAX_BYTES,
        )?;
        let standard = socket.next_event()?;
        assert_eq!(edge_trigger(&standard)?, None);
        let standard = socket.next_event()?;
        assert_eq!(edge_trigger(&standard)?, None);
        let wire = socket.next_event()?;
        assert_eq!(
            edge_trigger(&wire)?,
            Some(
                json!({"type":"edge-triggered","output":"DP-1","edge":"left","position":[4.0,720.0]})
            )
        );
        let wire = socket.next_event()?;
        assert_eq!(
            edge_trigger(&wire)?,
            Some(json!({"type":"edge-left","output":"DP-1","edge":"left","position":[8.0,720.0]}))
        );
        assert!(edge_trigger(&json!({"EdgeTriggered": {
            "output":"DP-1", "edge":"Left", "position":[0.0,"bad"]
        }}))
        .is_err());
        peer.join().map_err(|_| "Edge event fixture panicked")??;
        Ok(())
    }
    #[test]
    fn all_frozen_event_emissions_output_refreshes_and_disconnects() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-events-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let (client, mut server) = UnixStream::pair()?;
            let mut wire = Vec::new();
            for event in input["events"].as_array().ok_or("Missing events")? {
                wire.extend_from_slice((crate::json::encode(event)? + "\n").as_bytes());
            }
            wire.extend_from_slice(
                input
                    .get("tail")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .as_bytes(),
            );
            server.write_all(&wire)?;
            drop(server);
            let outputs = FixtureOutputs {
                values: input["outputs"]
                    .as_array()
                    .ok_or("Missing outputs")?
                    .clone(),
                queries: vec![],
            };
            let mut events = Events::new(EventSocket::from_stream(client), outputs)?;
            let mut emitted = Vec::new();
            let error = loop {
                match events.next_value() {
                    Ok(value) => emitted.push(crate::json::encode(&value)? + "\n"),
                    Err(error) => break json!({"type":error.kind,"error":error.message}),
                }
            };
            let observed = &case["observed"];
            assert_eq!(json!(emitted), observed["emitted"], "{}", input["name"]);
            assert_eq!(error, observed["error"], "{}", input["name"]);
            assert_eq!(
                json!(events.outputs.queries),
                observed["queries"],
                "{}",
                input["name"]
            );
            assert!(events.socket.is_none());
        }
        assert_eq!(
            reference["cases"].as_array().ok_or("Missing cases")?.len(),
            11
        );
        Ok(())
    }
    #[test]
    fn frozen_hover_negotiation_fallback_and_buffered_first_event() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-handshake-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let mut clients = VecDeque::new();
            let mut peers = Vec::new();
            for reply in input["replies"].as_array().ok_or("Missing replies")? {
                let (client, mut server) = UnixStream::pair()?;
                server.write_all(reply.as_str().ok_or("Invalid reply")?.as_bytes())?;
                server.shutdown(std::net::Shutdown::Write)?;
                clients.push_back(client);
                peers.push(server);
            }
            let mut opened = 0;
            let result = EventSocket::connect_with(
                || {
                    opened += 1;
                    clients
                        .pop_front()
                        .ok_or_else(|| invalid("Unexpected event connection"))
                },
                input["hover"].as_bool().ok_or("Missing hover")?,
                input["max_bytes"].as_u64().ok_or("Missing bound")? as usize,
            );
            let observed = &case["observed"];
            match result {
                Ok(mut socket) => {
                    assert_eq!(json!(opened - 1), observed["selected"]);
                    assert!(observed["error"].is_null());
                    let next = socket.next_event()?;
                    let expected = bytes(
                        observed["remaining_hex"]
                            .as_str()
                            .ok_or("Missing event bytes")?,
                    )?;
                    assert_eq!(next, serde_json::from_slice::<Value>(&expected)?);
                }
                Err(error) => {
                    assert!(observed["selected"].is_null());
                    assert_eq!(
                        json!({"type":error.kind,"error":error.message}),
                        observed["error"],
                        "{}",
                        input["name"]
                    );
                }
            }
            for (index, mut peer) in peers.into_iter().enumerate() {
                let mut wire = Vec::new();
                peer.read_to_end(&mut wire)?;
                let hex = wire.iter().map(|b| format!("{b:02x}")).collect::<String>();
                assert_eq!(json!(hex), observed["wire_hex"][index]);
            }
        }
        Ok(())
    }
    #[test]
    fn disabled_native_hover_rejects_extension_then_uses_stock_event_stream() -> crate::Result<()> {
        let (hover_client, mut hover_server) = UnixStream::pair()?;
        hover_server.write_all(b"{\"Err\":\"hover disabled\"}\n")?;
        hover_server.shutdown(std::net::Shutdown::Write)?;
        let (stock_client, mut stock_server) = UnixStream::pair()?;
        stock_server.write_all(b"{\"Ok\":\"Handled\"}\n")?;
        stock_server.shutdown(std::net::Shutdown::Write)?;
        let mut clients = VecDeque::from([hover_client, stock_client]);
        let mut socket = EventSocket::connect_with(
            || {
                clients
                    .pop_front()
                    .ok_or_else(|| invalid("Unexpected event connection"))
            },
            true,
            MAX_BYTES,
        )?;
        assert!(socket.next_event().is_err());
        drop(socket);
        let mut hover_request = Vec::new();
        hover_server.read_to_end(&mut hover_request)?;
        let mut stock_request = Vec::new();
        stock_server.read_to_end(&mut stock_request)?;
        assert_eq!(
            serde_json::from_slice::<Value>(&hover_request)?,
            json!("NacreOverviewHoverStream")
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&stock_request)?,
            json!("EventStream")
        );
        Ok(())
    }
    #[test]
    fn collection_stream_negotiates_combined_hover_and_reads_collection_events() -> crate::Result<()> {
        let mut clients = VecDeque::new();
        let mut peers = Vec::new();
        for reply in [
            "{\"Err\":\"combined stream unavailable\"}\n",
            "{\"Ok\":\"Handled\"}\n{\"CollectionsChanged\":{\"collections\":[{\"id\":17,\"idx\":1,\"name\":\"work\",\"output\":\"DP-1\",\"is_active\":true,\"workspace_ids\":[4]}]}}\n",
        ] {
            let (client, mut server) = UnixStream::pair()?;
            server.write_all(reply.as_bytes())?;
            clients.push_back(client);
            peers.push(server);
        }
        let mut socket = EventSocket::connect_collections_with(
            || clients.pop_front().ok_or_else(|| invalid("Unexpected stream open")),
            true,
        )?;
        let first = socket.next_event()?;
        assert_eq!(first["CollectionsChanged"]["collections"][0]["name"], "work");
        let mut wires = Vec::new();
        for mut peer in peers {
            let mut bytes = Vec::new();
            peer.read_to_end(&mut bytes)?;
            wires.push(String::from_utf8(bytes)?);
        }
        assert_eq!(serde_json::from_str::<Value>(wires[0].trim())?, json!("NacreOverviewHoverStreamWithCollections"));
        assert_eq!(serde_json::from_str::<Value>(wires[1].lines().next().ok_or("missing request")?)?, json!("EventStreamWithCollections"));
        Ok(())
    }
    fn bytes(hex: &str) -> crate::Result<Vec<u8>> {
        hex.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
            .collect()
    }
    #[test]
    fn event_loop_waits_for_signal_without_idle_polling() -> crate::Result<()> {
        let (client, mut server) = UnixStream::pair()?;
        let worker = thread::spawn(move || -> std::io::Result<()> {
            thread::sleep(Duration::from_millis(25));
            server.write_all(b"{\"FutureEvent\":null}\n")
        });
        let mut socket = EventSocket::from_stream(client);
        assert_eq!(socket.next_event()?, json!({"FutureEvent":null}));
        worker.join().map_err(|_| "Peer panicked")??;
        Ok(())
    }
}
