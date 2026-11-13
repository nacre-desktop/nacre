// SPDX-License-Identifier: GPL-3.0-only
//! N0217: the Brain app's private socket on the on-demand worker. One JSON line per request
//! (`{"id","op","params"}`) answered by `{"id","status","body"}`; the worker pushes
//! `{"event":"revision","revision"}` when the graph is rebuilt. Ops map 1:1 to the `/api/*` routes
//! and reuse their handlers and validation. An open connection counts as worker activity.
use crate::brain_http::{Activity, Request};
use crate::{brain_launch::percent_encode, brain_web, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LINE_LIMIT: usize = 64 * 1024;
const CLIENT_LIMIT: usize = 8;

pub fn path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    dir.is_absolute().then(|| dir.join("nacre/brain.sock"))
}

/// The HTTP request an op stands for; `None` for an unknown op.
pub(crate) fn request(op: &str, params: &Value) -> Option<Request> {
    let get = |path: &str| {
        let query = params
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                let v = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => (*b as u8).to_string(),
                    _ => return None,
                };
                Some(format!("{}={}", percent_encode(k), percent_encode(&v)))
            })
            .collect::<Vec<_>>()
            .join("&");
        Request {
            method: "GET".into(),
            path: path.into(),
            query,
            headers: Default::default(),
            body: Vec::new(),
        }
    };
    let post = |path: &str| Request {
        method: "POST".into(),
        path: path.into(),
        query: String::new(),
        headers: Default::default(),
        body: params.to_string().into_bytes(),
    };
    Some(match op {
        "brain" | "summary" | "wiki" | "loops" | "graph" | "search" | "note" => {
            get(&format!("/api/{op}"))
        }
        "wiki.page" => get("/api/wiki/page"),
        "ask" if params.get("question").is_some() => post("/api/ask"),
        "ask" => get("/api/ask"),
        "ask.settings" => post("/api/ask/settings"),
        "action" => post("/api/action"),
        _ => return None,
    })
}

fn reply(line: &str) -> Value {
    let call: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return json!({"id":null,"status":400,"body":{"error":"Invalid request"}}),
    };
    let id = call["id"].clone();
    let Some(request) = call["op"]
        .as_str()
        .and_then(|op| request(op, &call["params"]))
    else {
        return json!({"id":id,"status":404,"body":{"error":"Unknown op"}});
    };
    let response = brain_web::route(request);
    let mut body = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
    if call["op"] == "brain" {
        body = summary(body);
    }
    json!({"id":id,"status":response.status,"body":body})
}

/// `/api/brain` carries every note's body, metadata and all links for the browser; the app
/// reads titles, dates and worlds only (bodies come from `wiki.page`/`note` on demand), so the
/// reply stays small however large the vault is.
fn summary(mut body: Value) -> Value {
    let mut nodes = body["nodes"].take();
    for node in nodes.as_array_mut().into_iter().flatten() {
        if let Some(node) = node.as_object_mut() {
            node.remove("text");
            node.remove("meta");
        }
    }
    json!({"nodes":nodes,"worlds":body["worlds"].take(),"noteCount":body["noteCount"],
        "reviewCount":body["reviewCount"]})
}

fn serve_client(stream: UnixStream, activity: &Activity) -> Result<()> {
    let peer = nix::sys::socket::getsockopt(&stream, nix::sys::socket::sockopt::PeerCredentials)?;
    if peer.uid() != nix::unistd::getuid().as_raw() {
        return Err("Brain socket peer is another user".into());
    }
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let send = |writer: &Mutex<UnixStream>, value: &Value| {
        writeln!(writer.lock().unwrap_or_else(|e| e.into_inner()), "{value}")
    };
    let mut reader = BufReader::new(stream).take(LINE_LIMIT as u64);
    let mut sent_revision = brain_web::graph_revision();
    let mut line = String::new();
    loop {
        // A read timeout keeps the partial line (the event tick below runs between reads).
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(()),
            Ok(_) if !line.ends_with('\n') => return Err("Brain socket line too long".into()),
            Ok(_) => {
                reader.set_limit(LINE_LIMIT as u64);
                activity.touch();
                // Each request on its own thread, so a slow Ask never holds up page loads.
                let (writer, line) = (Arc::clone(&writer), std::mem::take(&mut line));
                std::thread::spawn(move || send(&writer, &reply(line.trim_end())));
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(error) => return Err(error.into()),
        }
        let revision = brain_web::graph_revision();
        if revision != sent_revision {
            sent_revision = revision;
            send(
                &writer,
                &json!({"event":"revision","revision":sent_revision}),
            )?;
        }
    }
}

/// Bind the app socket (0600, same-user peers only) and serve it until the worker exits.
pub fn listen(path: &PathBuf, activity: Arc<Activity>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    std::thread::Builder::new()
        .name("brain-socket".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if activity
                    .active
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                    >= CLIENT_LIMIT
                {
                    activity
                        .active
                        .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                    continue;
                }
                let activity = Arc::clone(&activity);
                std::thread::spawn(move || {
                    if let Err(error) = serve_client(stream, &activity) {
                        eprintln!("BRAIN_SOCKET_ERROR {error}");
                    }
                    activity
                        .active
                        .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                    activity.touch();
                });
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ops_map_to_the_http_routes_with_encoded_queries() {
        let search = request("search", &json!({"q":"a b&c","limit":5,"archive":true})).unwrap();
        assert_eq!(
            (search.method.as_str(), search.path.as_str()),
            ("GET", "/api/search")
        );
        assert_eq!(search.query, "q=a%20b%26c&limit=5&archive=1");
        let page = request("wiki.page", &json!({"id":"note:x"})).unwrap();
        assert_eq!(page.path, "/api/wiki/page");
        let ask = request("ask", &json!({"question":"why"})).unwrap();
        assert_eq!(
            (ask.method.as_str(), ask.path.as_str()),
            ("POST", "/api/ask")
        );
        assert_eq!(ask.body, br#"{"question":"why"}"#);
        assert_eq!(request("ask", &json!({})).unwrap().method, "GET");
        assert_eq!(
            request("action", &json!({"name":"capture"})).unwrap().path,
            "/api/action"
        );
        assert!(request("health", &json!({})).is_none());
        assert!(request("navigation", &json!({})).is_none());
    }
    #[test]
    fn the_brain_summary_drops_bodies_and_links() {
        let full = json!({"nodes":[{"id":"n","label":"N","text":"long body","meta":{"a":1}}],
            "links":[{"source":"n","target":"m"}],"worlds":[{"label":"W"}],"noteCount":1,"reviewCount":0,"discovery":{}});
        assert_eq!(
            summary(full),
            json!({"nodes":[{"id":"n","label":"N"}],"worlds":[{"label":"W"}],"noteCount":1,"reviewCount":0})
        );
    }
    #[test]
    fn malformed_and_unknown_requests_get_error_replies() {
        assert_eq!(reply("nope")["status"], 400);
        let unknown = reply(r#"{"id":7,"op":"shutdown","params":{}}"#);
        assert_eq!(
            (unknown["id"].as_u64(), unknown["status"].as_u64()),
            (Some(7), Some(404))
        );
    }
}
