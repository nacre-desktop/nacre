//! Developer-only inherited socket boundary, never installed.
use nacre_helper::{
    compositor::{
        niri_events::{EventSocket, Events, Outputs},
        niri_state::Error,
        niri_transport::Connection,
    },
    json, Result,
};
use serde_json::{json as value, Value};
use std::{
    io::{self, BufRead, Read, Write},
    os::{fd::FromRawFd, unix::net::UnixStream},
    time::Duration,
};

fn send(value: Value) -> Result<()> {
    println!("{}", json::encode(&value)?);
    io::stdout().flush()?;
    Ok(())
}
fn receive() -> Result<Value> {
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(serde_json::from_str(&line)?)
}
fn socket(fd: i32) -> Result<UnixStream> {
    if fd < 3 {
        return Err("Invalid inherited socket descriptor".into());
    }
    // The test process passes ownership of this descriptor via pass_fds.
    Ok(unsafe { UnixStream::from_raw_fd(fd) })
}
struct Metadata;
impl Outputs for Metadata {
    fn outputs(&mut self) -> std::result::Result<Value, Error> {
        let run = || -> Result<Value> {
            send(value!({"callback":"outputs"}))?;
            receive()
        };
        run().map_err(|error| Error {
            kind: "RuntimeError",
            message: error.to_string(),
        })
    }
}
fn run() -> Result<()> {
    let input: Value = serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
    match input["mode"].as_str() {
        Some("request") => {
            let mut connection = Connection::from_stream(socket(
                input["fd"].as_i64().ok_or("Missing descriptor")? as i32,
            )?);
            let result = connection.request_value(
                &input["request"],
                Duration::from_secs(10),
                4 * 1024 * 1024,
            );
            match result {
                Ok(result) => send(value!({"result":result})),
                Err(error) => send(value!({"error":error.message,"type":error.kind})),
            }
        }
        Some("handshake") => {
            let mut sockets = input["fds"].as_array().ok_or("Missing descriptors")?.iter();
            let mut selected = None;
            let result = EventSocket::connect_with(
                || {
                    let fd = sockets
                        .next()
                        .and_then(Value::as_i64)
                        .ok_or_else(|| Error {
                            kind: "RuntimeError",
                            message: "No handshake socket".into(),
                        })?;
                    selected = Some(fd);
                    socket(fd as i32).map_err(|error| Error {
                        kind: "RuntimeError",
                        message: error.to_string(),
                    })
                },
                input["hover"].as_bool().unwrap_or(true),
                4 * 1024 * 1024,
            );
            match result {
                Ok(socket) => {
                    let mut stream = socket.into_stream();
                    stream.get_mut().set_nonblocking(false)?;
                    stream
                        .get_mut()
                        .set_read_timeout(Some(Duration::from_secs(1)))?;
                    let mut remaining = Vec::new();
                    stream
                        .by_ref()
                        .take(4096)
                        .read_until(b'\n', &mut remaining)?;
                    send(value!({"selected_fd":selected,"remaining":remaining}))
                }
                Err(error) => send(value!({"error":error.message,"type":error.kind})),
            }
        }
        Some("events") => {
            let socket = EventSocket::from_stream(socket(
                input["fd"].as_i64().ok_or("Missing descriptor")? as i32,
            )?);
            let mut events = Events::new(socket, Metadata)?;
            loop {
                match events.next_value() {
                    Ok(event) => send(value!({"event":event}))?,
                    Err(error) => {
                        send(value!({"error":error.message,"type":error.kind}))?;
                        break;
                    }
                }
            }
            Ok(())
        }
        _ => Err("Unknown transport operation".into()),
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
