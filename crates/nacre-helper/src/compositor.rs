//! Native compositor boundaries; compositor IDs stay opaque.
pub mod backend;
pub mod cli;
pub mod hyprland;
pub mod hyprland_events;
pub mod hyprland_runtime;
pub mod hyprland_transport;
pub mod niri;
pub mod niri_actions;
pub mod niri_config;
pub mod niri_desktop;
pub mod niri_events;
pub mod niri_night_light;
pub mod niri_state;
pub mod niri_transport;
pub(crate) mod wayland_registry;
use crate::Result;
use niri_ipc::{Action, Request};
use serde_json::Value;
use std::{
    env,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};

pub enum CaptureWindow {
    Hyprland(Value),
    Niri(Option<u64>),
}

enum Connection {
    Hyprland(PathBuf),
    Niri(niri_transport::Connection),
}

pub struct Compositor(Connection);
impl Compositor {
    pub fn connect() -> Result<Self> {
        let environment = env::vars().collect();
        let backend = backend::select(&environment)?;
        let connection = if backend == "niri" {
            let path = env::var_os("NIRI_SOCKET")
                .filter(|v| !v.is_empty())
                .ok_or("NIRI_SOCKET is unavailable")?;
            let stream = UnixStream::connect(path)?;
            Connection::Niri(niri_transport::Connection::from_stream(stream))
        } else {
            let runtime = env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is unavailable")?;
            let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")?;
            if signature.is_empty()
                || signature.contains('/')
                || signature.contains('\0')
                || signature == "."
                || signature == ".."
            {
                return Err("Invalid Hyprland instance signature".into());
            }
            Connection::Hyprland(
                PathBuf::from(runtime)
                    .join("hypr")
                    .join(signature)
                    .join(".socket.sock"),
            )
        };
        Ok(Self(connection))
    }
    pub fn active_window(&mut self) -> Result<CaptureWindow> {
        match &mut self.0 {
            Connection::Niri(stream) => {
                let reply = niri_request(stream, Request::FocusedWindow)?;
                let window = reply
                    .get("FocusedWindow")
                    .ok_or("Malformed compositor query response")?;
                let id = if window.is_null() {
                    None
                } else {
                    Some(
                        window
                            .get("id")
                            .and_then(Value::as_u64)
                            .ok_or("Malformed native window ID")?,
                    )
                };
                Ok(CaptureWindow::Niri(id))
            }
            Connection::Hyprland(path) => {
                let bytes = hyprland_request(path, b"j/activewindow", Duration::from_secs(5))?;
                let value: Value = serde_json::from_slice(&bytes)?;
                if !value.is_object() {
                    return Err("Malformed compositor query response".into());
                }
                Ok(CaptureWindow::Hyprland(value))
            }
        }
    }
    pub fn focused_output_name(&mut self) -> Result<String> {
        let output = match &mut self.0 {
            Connection::Niri(stream) => {
                let reply = niri_request(stream, Request::FocusedOutput)?;
                reply
                    .get("FocusedOutput")
                    .cloned()
                    .ok_or("Malformed compositor query response")?
            }
            Connection::Hyprland(path) => {
                let bytes = hyprland_request(path, b"j/monitors", Duration::from_secs(5))?;
                let monitors: Value = serde_json::from_slice(&bytes)?;
                monitors
                    .as_array()
                    .ok_or("Malformed compositor query response")?
                    .iter()
                    .find(|monitor| monitor["focused"] == true)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
        };
        output
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .ok_or_else(|| "No focused output available".into())
    }
    pub fn diagnostic(&mut self, name: &str, timeout: Duration) -> Result<String> {
        match &mut self.0 {
            Connection::Hyprland(path) => {
                let command = match name {
                    "runningVersion" => b"/version".as_slice(),
                    "allMonitors" => b"j/monitors all".as_slice(),
                    "configErrors" => b"/configerrors".as_slice(),
                    _ => return Err("Unknown compositor diagnostic".into()),
                };
                Ok(String::from_utf8(hyprland_request(
                    path, command, timeout,
                )?)?)
            }
            Connection::Niri(stream) => {
                let (request, field) = match name {
                    "runningVersion" => (Request::Version, "Version"),
                    "allMonitors" => (Request::Outputs, "Outputs"),
                    _ => return Err("Unknown compositor diagnostic".into()),
                };
                let result = niri_request_with_timeout(stream, request, timeout)?;
                crate::json::pretty(
                    result
                        .get(field)
                        .ok_or("Malformed compositor diagnostic reply")?,
                )
            }
        }
    }
    pub fn screenshot_window(&mut self, path: &Path, id: u64) -> Result<()> {
        if !path.is_absolute() {
            return Err("Screenshot path must be absolute".into());
        }
        let Connection::Niri(stream) = &mut self.0 else {
            return Err("Native window capture is unavailable".into());
        };
        let reply = niri_request(
            stream,
            Request::Action(Action::ScreenshotWindow {
                id: Some(id),
                write_to_disk: true,
                show_pointer: false,
                path: Some(
                    path.to_str()
                        .ok_or("Screenshot path is not UTF-8")?
                        .to_owned(),
                ),
            }),
        )?;
        if reply == "Handled" {
            Ok(())
        } else {
            Err("Malformed compositor action response".into())
        }
    }
}

fn hyprland_request(path: &Path, command: &[u8], timeout: Duration) -> Result<Vec<u8>> {
    hyprland_transport::exchange(path, command, timeout, 2_097_152)
}

fn niri_request(stream: &mut niri_transport::Connection, request: Request) -> Result<Value> {
    niri_request_with_timeout(stream, request, Duration::from_secs(5))
}
fn niri_request_with_timeout(
    stream: &mut niri_transport::Connection,
    request: Request,
    timeout: Duration,
) -> Result<Value> {
    stream
        .request(&request, timeout, 2_097_152)
        .map_err(|error| {
            // Capture's established stderr predates the neutral backend's prefix.
            // Keep that public contract while sharing framing and connection limits.
            if error.kind == "RuntimeError" {
                if let Some(message) = error.message.strip_prefix("niri IPC: ") {
                    return message.to_owned().into();
                }
            }
            error.into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::Shutdown,
        os::unix::net::UnixListener,
        thread,
    };
    #[test]
    fn private_hyprland_diagnostics_use_exact_socket_commands() -> Result<()> {
        let home = tempfile::tempdir()?;
        let path = home.path().join("compositor.sock");
        let listener = UnixListener::bind(&path)?;
        let server = thread::spawn(move || -> Result<()> {
            for (command, reply) in [
                ("/version", "fixture version"),
                ("j/monitors all", "[]"),
                ("/configerrors", "bad option"),
            ] {
                let (mut socket, _) = listener.accept()?;
                socket.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut request = String::new();
                socket.read_to_string(&mut request)?;
                assert_eq!(request, command);
                socket.write_all(reply.as_bytes())?;
            }
            Ok(())
        });
        let mut compositor = Compositor(Connection::Hyprland(path));
        for (name, expected) in [
            ("runningVersion", "fixture version"),
            ("allMonitors", "[]"),
            ("configErrors", "bad option"),
        ] {
            assert_eq!(
                compositor.diagnostic(name, Duration::from_secs(1))?,
                expected
            );
        }
        server
            .join()
            .map_err(|_| "Diagnostic socket fixture panicked")??;
        Ok(())
    }
    #[test]
    fn private_niri_diagnostics_reuse_the_connection_and_bound_unknown_replies() -> Result<()> {
        let (client, server) = UnixStream::pair()?;
        let server = thread::spawn(move || -> Result<()> {
            let mut stream = BufReader::new(server);
            for (name, reply) in [
                ("Version", r#"{"Ok":{"Version":"fixture version"}}"#),
                ("Outputs", r#"{"Ok":{"Outputs":{}}}"#),
            ] {
                let mut line = String::new();
                stream.read_line(&mut line)?;
                assert_eq!(
                    serde_json::from_str::<Value>(&line)?,
                    Value::String(name.into())
                );
                stream
                    .get_mut()
                    .write_all(format!("{reply}\n").as_bytes())?;
            }
            Ok(())
        });
        let mut compositor = Compositor(Connection::Niri(niri_transport::Connection::from_stream(
            client,
        )));
        assert_eq!(
            compositor.diagnostic("runningVersion", Duration::from_secs(1))?,
            "\"fixture version\""
        );
        assert_eq!(
            compositor.diagnostic("allMonitors", Duration::from_secs(1))?,
            "{}"
        );
        assert!(compositor
            .diagnostic("unsupported", Duration::from_secs(1))
            .is_err());
        server
            .join()
            .map_err(|_| "Diagnostic socket fixture panicked")??;
        Ok(())
    }
    #[test]
    fn niri_capture_reuses_one_socket_and_preserves_native_errors() -> Result<()> {
        let (client, server) = UnixStream::pair()?;
        client.set_read_timeout(Some(Duration::from_secs(1)))?;
        let server = thread::spawn(move || -> Result<()> {
            let mut stream = BufReader::new(server);
            let mut line = String::new();
            stream.read_line(&mut line)?;
            assert!(matches!(
                serde_json::from_str::<Request>(&line)?,
                Request::FocusedWindow
            ));
            stream
                .get_mut()
                .write_all(b"{\"Ok\":{\"FocusedWindow\":{\"id\":42}}}\n")?;
            line.clear();
            stream.read_line(&mut line)?;
            match serde_json::from_str::<Request>(&line)? {
                Request::Action(Action::ScreenshotWindow {
                    id,
                    path,
                    write_to_disk,
                    show_pointer,
                }) => {
                    assert_eq!(id, Some(42));
                    assert_eq!(path.as_deref(), Some("/capture.png"));
                    assert!(write_to_disk);
                    assert!(!show_pointer);
                }
                other => panic!("Unexpected request: {other:?}"),
            }
            stream
                .get_mut()
                .write_all(b"{\"Err\":\"capture rejected\"}\n")?;
            Ok(())
        });
        let mut compositor = Compositor(Connection::Niri(niri_transport::Connection::from_stream(
            client,
        )));
        assert!(matches!(
            compositor.active_window()?,
            CaptureWindow::Niri(Some(42))
        ));
        assert_eq!(
            compositor
                .screenshot_window(Path::new("/capture.png"), 42)
                .expect_err("Native failure must surface")
                .to_string(),
            "capture rejected"
        );
        server.join().map_err(|_| "IPC fixture panicked")??;
        Ok(())
    }
    #[test]
    fn focused_output_names_use_native_queries_and_reject_missing_outputs() -> Result<()> {
        let (client, server) = UnixStream::pair()?;
        let server = thread::spawn(move || -> Result<()> {
            let mut stream = BufReader::new(server);
            for reply in [
                r#"{"Ok":{"FocusedOutput":{"name":"eDP-1"}}}"#,
                r#"{"Ok":{"FocusedOutput":null}}"#,
            ] {
                let mut line = String::new();
                stream.read_line(&mut line)?;
                assert!(matches!(
                    serde_json::from_str::<Request>(&line)?,
                    Request::FocusedOutput
                ));
                writeln!(stream.get_mut(), "{reply}")?;
            }
            Ok(())
        });
        let mut compositor = Compositor(Connection::Niri(niri_transport::Connection::from_stream(
            client,
        )));
        assert_eq!(compositor.focused_output_name()?, "eDP-1");
        assert!(compositor.focused_output_name().is_err());
        server.join().map_err(|_| "IPC fixture panicked")??;

        let home = tempfile::tempdir()?;
        let path = home.path().join("monitors.sock");
        let listener = UnixListener::bind(&path)?;
        let server = thread::spawn(move || -> Result<()> {
            for reply in [
                r#"[{"name":"eDP-1","focused":false},{"name":"DP-2","focused":true}]"#,
                "[]",
            ] {
                let (mut socket, _) = listener.accept()?;
                let mut command = String::new();
                socket.read_to_string(&mut command)?;
                assert_eq!(command, "j/monitors");
                socket.write_all(reply.as_bytes())?;
            }
            Ok(())
        });
        let mut compositor = Compositor(Connection::Hyprland(path));
        assert_eq!(compositor.focused_output_name()?, "DP-2");
        assert!(compositor.focused_output_name().is_err());
        server.join().map_err(|_| "IPC fixture panicked")??;
        Ok(())
    }
    #[test]
    fn hyprland_capture_reads_its_socket_and_rejects_bad_shapes() -> Result<()> {
        let home = tempfile::tempdir()?;
        let path = home.path().join("compositor.sock");
        let listener = UnixListener::bind(&path)?;
        let server = thread::spawn(move || -> Result<()> {
            for reply in [
                b"{\"at\":[-20,30],\"size\":[200,100]}".as_slice(),
                b"[]".as_slice(),
            ] {
                let (mut socket, _) = listener.accept()?;
                socket.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut command = String::new();
                socket.read_to_string(&mut command)?;
                assert_eq!(command, "j/activewindow");
                socket.write_all(reply)?;
            }
            Ok(())
        });
        let mut compositor = Compositor(Connection::Hyprland(path));
        assert!(matches!(
            compositor.active_window()?,
            CaptureWindow::Hyprland(_)
        ));
        assert_eq!(
            compositor
                .active_window()
                .err()
                .ok_or("Invalid shape accepted")?
                .to_string(),
            "Malformed compositor query response"
        );
        server.join().map_err(|_| "IPC fixture panicked")??;
        Ok(())
    }
    #[test]
    fn niri_rejects_truncation_and_oversized_lines() -> Result<()> {
        for reply in [b"{\"Ok\":\"Handled\"}".to_vec(), vec![b'x'; 2_097_153]] {
            let (client, mut server) = UnixStream::pair()?;
            client.set_read_timeout(Some(Duration::from_secs(1)))?;
            let task = thread::spawn(move || -> Result<()> {
                let mut request = String::new();
                BufReader::new(server.try_clone()?).read_line(&mut request)?;
                server.write_all(&reply)?;
                server.shutdown(Shutdown::Write)?;
                Ok(())
            });
            let error = niri_request(
                &mut niri_transport::Connection::from_stream(client),
                Request::FocusedWindow,
            )
            .expect_err("Invalid reply accepted");
            assert_eq!(error.to_string(), "Truncated or oversized niri reply");
            task.join().map_err(|_| "IPC fixture panicked")??;
        }
        Ok(())
    }
}
