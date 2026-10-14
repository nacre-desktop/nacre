//! Blocking Hyprland event stream; initial snapshot then semantic events until EOF.
use super::{
    hyprland::{self, Backend, Transport},
    niri_state::Error,
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read},
    os::unix::net::UnixStream,
    path::Path,
};
type Result<T> = std::result::Result<T, Error>;
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
pub trait Queries {
    fn query(&mut self, name: &str) -> Result<Value>;
}
impl<T: Transport> Queries for Backend<T> {
    fn query(&mut self, name: &str) -> Result<Value> {
        Backend::query(self, name, 10.0, None)
    }
}
pub struct Events {
    stream: BufReader<UnixStream>,
    initial: Option<Value>,
    ended: bool,
}
impl Events {
    pub fn next_before(&mut self, deadline: std::time::Instant) -> Result<Option<Value>> {
        let timeout = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|time| !time.is_zero());
        let Some(timeout) = timeout else {
            return Ok(None);
        };
        self.stream
            .get_mut()
            .set_read_timeout(Some(timeout))
            .map_err(|error| Error {
                kind: "OSError",
                message: error.to_string(),
            })?;
        match self.next_value() {
            Err(error) if error.kind == "TimeoutError" => Ok(None),
            result => result,
        }
    }
    pub fn connect(path: &Path, backend: &mut impl Queries) -> Result<Self> {
        let stream = UnixStream::connect(path).map_err(|error| Error {
            kind: "OSError",
            message: error.to_string(),
        })?;
        Self::new(stream, backend)
    }
    pub fn new(stream: UnixStream, backend: &mut impl Queries) -> Result<Self> {
        stream.set_nonblocking(false).map_err(|error| Error {
            kind: "OSError",
            message: error.to_string(),
        })?;
        let initial = json!({"type":"snapshot","monitors":backend.query("monitors")?,"workspaces":backend.query("workspaces")?,"windows":backend.query("windows")?,"activeWindow":backend.query("active_window")?});
        Ok(Self {
            stream: BufReader::new(stream),
            initial: Some(initial),
            ended: false,
        })
    }
    pub fn next_value(&mut self) -> Result<Option<Value>> {
        if let Some(value) = self.initial.take() {
            return Ok(Some(value));
        }
        if self.ended {
            return Ok(None);
        }
        let result = (|| {
            let mut bytes = Vec::new();
            let count = (&mut self.stream)
                .take(4 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut bytes)
                .map_err(io_error)?;
            if count == 0 {
                self.ended = true;
                return Ok(None);
            }
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(Error {
                    kind: "ValueError",
                    message: "Hyprland event exceeded its size limit".into(),
                });
            }
            let line = std::str::from_utf8(&bytes).map_err(|error| Error {
                kind: "UnicodeDecodeError",
                message: error.to_string(),
            })?;
            Ok(Some(hyprland::parse_event(line.trim_end_matches('\n'))))
        })();
        if result.is_err() {
            self.ended = true;
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn bounded_event_wait_exits_at_deadline_without_polling() -> crate::Result<()> {
        struct Empty;
        impl Queries for Empty {
            fn query(&mut self, name: &str) -> Result<Value> {
                Ok(if name == "active_window" {
                    json!({})
                } else {
                    json!([])
                })
            }
        }
        let (client, _server) = UnixStream::pair()?;
        let mut events = Events::new(client, &mut Empty)?;
        assert!(events.next_value()?.is_some());
        let began = std::time::Instant::now();
        assert!(events
            .next_before(began + std::time::Duration::from_millis(20))?
            .is_none());
        assert!(began.elapsed() < std::time::Duration::from_secs(1));
        Ok(())
    }
    #[test]
    fn frozen_python_stream_emissions_queries_and_clean_eof() -> crate::Result<()> {
        struct Fixture {
            snapshot: Value,
            queries: Vec<String>,
        }
        impl Queries for Fixture {
            fn query(&mut self, name: &str) -> Result<Value> {
                self.queries.push(name.into());
                Ok(self.snapshot[name].clone())
            }
        }
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hyprland-events-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing stream cases")?
        {
            let input = &case["request"];
            let (client, mut server) = UnixStream::pair()?;
            let lines = input["lines"]
                .as_array()
                .ok_or("Missing lines")?
                .iter()
                .map(|line| line.as_str().ok_or("Malformed line"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let wire = lines.join("\n")
                + if !lines.is_empty() && input["trailing_newline"] == true {
                    "\n"
                } else {
                    ""
                };
            server.write_all(wire.as_bytes())?;
            drop(server);
            let mut backend = Fixture {
                snapshot: input["snapshot"].clone(),
                queries: vec![],
            };
            let mut events = Events::new(client, &mut backend)?;
            let mut emitted = Vec::new();
            while let Some(value) = events.next_value()? {
                emitted.push(crate::json::encode(&value)? + "\n");
            }
            assert_eq!(
                json!(emitted),
                case["observed"]["emitted"],
                "{}",
                input["name"]
            );
            assert_eq!(json!(backend.queries), case["observed"]["queries"]);
            assert!(events.next_value()?.is_none());
        }
        Ok(())
    }
}
