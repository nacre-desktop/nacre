//! Bounded, loopback-only HTTP transport for the Brain web worker.

use crate::brain_auth::BrowserAuth;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CLIENT_LIMIT: usize = 32;
const HEADER_LIMIT: usize = 16 * 1024;
const BODY_LIMIT: usize = 4096;
const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
const IO_DEADLINE: Duration = Duration::from_secs(5);
const CONNECTION_DEADLINE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub mime: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, mime: impl Into<String>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            mime: mime.into(),
            headers: Vec::new(),
            body: body.into(),
        }
    }
    fn error(status: u16) -> Self {
        Self::new(status, "application/json", b"{}".to_vec())
    }
}

pub fn serve<F>(
    listener: TcpListener,
    state: &Path,
    idle_timeout: Duration,
    callback: F,
) -> io::Result<()>
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    serve_inner(listener, state, idle_timeout, CONNECTION_DEADLINE, callback)
}

fn serve_inner<F>(
    listener: TcpListener,
    state: &Path,
    idle_timeout: Duration,
    connection_deadline: Duration,
    callback: F,
) -> io::Result<()>
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    let address = listener.local_addr()?;
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Brain HTTP must bind to loopback",
        ));
    }
    listener.set_nonblocking(true)?;
    let port = address.port();
    let auth = Arc::new(Mutex::new(BrowserAuth::open(state, port)?));
    let callback = Arc::new(callback);
    let active = Arc::new(AtomicUsize::new(0));
    let mut last_activity = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                last_activity = Instant::now();
                if !peer.ip().is_loopback() {
                    drop(stream);
                    continue;
                }
                if active.fetch_add(1, Ordering::AcqRel) >= CLIENT_LIMIT {
                    active.fetch_sub(1, Ordering::AcqRel);
                    drop(stream);
                    continue;
                }
                let watchdog_stream = match stream.try_clone() {
                    Ok(value) => value,
                    Err(_) => {
                        active.fetch_sub(1, Ordering::AcqRel);
                        drop(stream);
                        continue;
                    }
                };
                let (done_tx, done_rx) = mpsc::channel();
                std::thread::spawn(move || {
                    watch_connection(watchdog_stream, connection_deadline, done_rx)
                });
                let auth = Arc::clone(&auth);
                let callback = Arc::clone(&callback);
                let active = Arc::clone(&active);
                std::thread::spawn(move || {
                    let _guard = ClientGuard {
                        active,
                        done: done_tx,
                    };
                    let mut stream = stream;
                    if let Ok(Some(request)) = read_request(&mut stream) {
                        let authorized = match auth.lock() {
                            Ok(mut auth) => authorize(request, port, &mut auth),
                            Err(_) => Err(Response::error(500)),
                        };
                        let response = match authorized {
                            Ok(mut request) => {
                                let proof = request.headers.remove("x-nacre-internal-proof");
                                let mut response = callback(request);
                                if let Some(proof) = proof {
                                    response
                                        .headers
                                        .push(("X-Nacre-Brain-Proof".into(), proof));
                                }
                                response
                            }
                            Err(response) => response,
                        };
                        let _ = write_response(&mut stream, response);
                    }
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if active.load(Ordering::Acquire) == 0 && last_activity.elapsed() >= idle_timeout {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

struct ClientGuard {
    active: Arc<AtomicUsize>,
    done: mpsc::Sender<()>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
        let _ = self.done.send(());
    }
}

fn watch_connection(stream: TcpStream, deadline: Duration, done: mpsc::Receiver<()>) {
    if done.recv_timeout(deadline).is_err() {
        let _ = stream.shutdown(Shutdown::Both);
    }
}

fn authorize(request: Request, port: u16, auth: &mut BrowserAuth) -> Result<Request, Response> {
    if request.method == "INVALID" {
        return Err(Response::error(400));
    }
    let host = request
        .headers
        .get("host")
        .map(String::as_str)
        .unwrap_or("");
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        return Err(Response::error(403));
    }
    let mut request = request;
    request.headers.remove("x-nacre-internal-proof");
    if request.method == "GET" && request.path == "/api/health" {
        if let Some(nonce) = request.headers.get("x-nacre-brain-nonce") {
            let Some(proof) = auth.readiness_proof(nonce) else {
                return Err(Response::error(400));
            };
            request
                .headers
                .insert("x-nacre-internal-proof".into(), proof);
        }
    }
    let origin = request.headers.get("origin");
    let referer = request.headers.get("referer");
    let canonical = format!("http://127.0.0.1:{port}");
    if request.method == "GET" && request.path == "/" {
        if let Some(token) = query_value(&request.query, "token") {
            if !auth.consume_bootstrap(&token) {
                return Err(Response::error(403));
            }
            let handoff = query_value(&request.query, "handoff").as_deref() == Some("1");
            let mut response = Response::new(303, "text/plain", Vec::new());
            response.headers.push((
                "Set-Cookie".into(),
                format!(
                    "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
                    auth.cookie_name(),
                    auth.cookie_value(),
                    auth.cookie_max_age()
                ),
            ));
            response.headers.push((
                "Location".into(),
                format!(
                    "{}#nacre-auth={}",
                    if handoff { "/auth-complete" } else { "/" },
                    auth.bearer_value()
                ),
            ));
            response
                .headers
                .push(("Referrer-Policy".into(), "no-referrer".into()));
            response
                .headers
                .push(("Cache-Control".into(), "no-store".into()));
            return Err(response);
        }
    }
    if request.method == "POST" {
        let cookie_ok = request.headers.get("cookie").is_some_and(|value| {
            cookie_from(value, &auth.cookie_name()).is_some_and(|cookie| auth.cookie_valid(cookie))
        });
        let bearer_ok = request
            .headers
            .get("x-nacre-brain-authorization")
            .is_some_and(|value| auth.bearer_valid(value));
        if !cookie_ok
            || !bearer_ok
            || origin.map(String::as_str) != Some(canonical.as_str())
            || request.headers.get("content-type").map(String::as_str) != Some("application/json")
            || request
                .headers
                .get("sec-fetch-site")
                .is_some_and(|value| value != "same-origin")
        {
            return Err(Response::error(403));
        }
    } else if request.path.starts_with("/api/") && request.path != "/api/health" {
        let cookie_ok = request.headers.get("cookie").is_some_and(|value| {
            cookie_from(value, &auth.cookie_name()).is_some_and(|cookie| auth.cookie_valid(cookie))
        });
        let bearer_ok = request
            .headers
            .get("x-nacre-brain-authorization")
            .is_some_and(|value| auth.bearer_valid(value));
        let fetch_ok =
            request.headers.get("sec-fetch-site").map(String::as_str) == Some("same-origin");
        let origin_ok = origin.is_none_or(|value| value == &canonical);
        let referer_ok = referer.is_none_or(|value| {
            value.starts_with(&(canonical.clone() + "/")) || value == &canonical
        });
        if !cookie_ok || !bearer_ok || !fetch_ok || !origin_ok || !referer_ok {
            return Err(Response::error(403));
        }
    }
    if request.method == "POST"
        && request
            .headers
            .get("content-type")
            .is_some_and(|value| value.starts_with("application/json"))
        && !matches!(
            serde_json::from_slice::<Value>(&request.body),
            Ok(Value::Object(_))
        )
    {
        return Err(Response::error(400));
    }
    Ok(request)
}

fn cookie_from<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    let mut found = None;
    for item in header.split(';') {
        let (key, value) = item.trim().split_once('=')?;
        if key == name {
            if found.is_some() {
                return None;
            }
            found = Some(value);
        }
    }
    found
}

fn query_value(query: &str, wanted: &str) -> Option<String> {
    let mut result = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if decode_component(key)? == wanted {
            if result.is_some() {
                return None;
            }
            result = Some(decode_component(value)?);
        }
    }
    result
}

fn decode_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                output.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            b'%' => return None,
            value => {
                output.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<Request>> {
    let start = Instant::now();
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0_u8; 1024];
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(None),
            Ok(count) => {
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(index) = bytes.windows(4).position(|item| item == b"\r\n\r\n") {
                    break index + 4;
                }
                if bytes.len() > HEADER_LIMIT {
                    return Ok(Some(bad_request()));
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                if start.elapsed() >= IO_DEADLINE {
                    return Ok(Some(bad_request()));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
        if start.elapsed() >= IO_DEADLINE {
            return Ok(Some(bad_request()));
        }
    };
    if header_end > HEADER_LIMIT {
        return Ok(Some(bad_request()));
    }
    let header_text = match std::str::from_utf8(&bytes[..header_end - 4]) {
        Ok(value) => value.to_owned(),
        Err(_) => return Ok(Some(bad_request())),
    };
    let mut lines = header_text.split("\r\n");
    let Some(first) = lines.next() else {
        return Ok(Some(bad_request()));
    };
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(Some(bad_request()));
    };
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") || !matches!(method, "GET" | "POST" | "HEAD") {
        return Ok(Some(bad_request()));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if !path.starts_with('/') || path.starts_with("//") {
        return Ok(Some(bad_request()));
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Ok(Some(bad_request()));
        };
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
        {
            return Ok(Some(bad_request()));
        }
        let name = name.to_ascii_lowercase();
        if headers.insert(name, value.trim().to_owned()).is_some() {
            return Ok(Some(bad_request()));
        }
    }
    if headers.contains_key("transfer-encoding") || !headers.contains_key("host") {
        return Ok(Some(bad_request()));
    }
    let length = match headers.get("content-length") {
        Some(value) if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            match value.parse::<usize>() {
                Ok(value) if value <= BODY_LIMIT => value,
                _ => return Ok(Some(bad_request())),
            }
        }
        Some(_) => return Ok(Some(bad_request())),
        None => 0,
    };
    if method == "POST" && length == 0 {
        return Ok(Some(bad_request()));
    }
    bytes.drain(..header_end);
    let mut body = bytes;
    while body.len() < length {
        let mut chunk = vec![0_u8; (length - body.len()).min(8192)];
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(Some(bad_request())),
            Ok(count) => body.extend_from_slice(&chunk[..count]),
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                if start.elapsed() >= IO_DEADLINE {
                    return Ok(Some(bad_request()));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
        if start.elapsed() >= IO_DEADLINE {
            return Ok(Some(bad_request()));
        }
    }
    if body.len() != length {
        return Ok(Some(bad_request()));
    }
    body.truncate(length);
    Ok(Some(Request {
        method: method.to_owned(),
        path: path.to_owned(),
        query: query.to_owned(),
        headers,
        body,
    }))
}

fn bad_request() -> Request {
    Request {
        method: "INVALID".into(),
        path: "/".into(),
        query: String::new(),
        headers: BTreeMap::new(),
        body: Vec::new(),
    }
}

fn write_response(stream: &mut TcpStream, mut response: Response) -> io::Result<()> {
    stream.set_write_timeout(Some(Duration::from_millis(250)))?;
    if response.body.len() > RESPONSE_LIMIT
        || response.mime.chars().any(|ch| ch == '\r' || ch == '\n')
    {
        response = Response::error(500);
    }
    let reason = match response.status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Response",
    };
    let mut head = format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'\r\n", response.status, reason, response.mime, response.body.len());
    for (name, value) in response.headers {
        if name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !value.chars().any(|ch| ch == '\r' || ch == '\n')
        {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    head.push_str("\r\n");
    if head.len() > HEADER_LIMIT {
        return write_response(stream, Response::error(500));
    }
    let start = Instant::now();
    for chunk in [head.as_bytes(), response.body.as_slice()] {
        let mut remaining = chunk;
        while !remaining.is_empty() {
            if start.elapsed() >= IO_DEADLINE {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "response write deadline",
                ));
            }
            match stream.write(remaining) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "client stopped reading",
                    ))
                }
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::TimedOut =>
                {
                    continue
                }
                Err(error) => return Err(error),
            }
        }
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_api_rejects_cross_port_navigation_even_with_host_wide_cookie() {
        let dir = tempfile::tempdir().unwrap();
        let victim = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let attacker = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = victim.local_addr().unwrap().port();
        let attacker_port = attacker.local_addr().unwrap().port();
        let mut auth = BrowserAuth::open(dir.path(), port).unwrap();
        let cookie = format!("{}={}", auth.cookie_name(), auth.cookie_value());
        let bearer = auth.bearer_value().to_owned();
        let token = auth.bootstrap_token().unwrap().to_owned();
        assert!(auth.consume_bootstrap(&token));
        let mut headers = BTreeMap::new();
        headers.insert("host".into(), format!("127.0.0.1:{port}"));
        headers.insert("cookie".into(), cookie);
        headers.insert("x-nacre-brain-authorization".into(), bearer.clone());
        headers.insert("sec-fetch-site".into(), "same-site".into());
        let response = authorize(
            Request {
                method: "GET".into(),
                path: "/api/brain".into(),
                query: String::new(),
                headers,
                body: Vec::new(),
            },
            port,
            &mut auth,
        );
        assert_eq!(response.unwrap_err().status, 403);
        let mut headers = BTreeMap::new();
        headers.insert("host".into(), format!("127.0.0.1:{port}"));
        headers.insert(
            "cookie".into(),
            format!("{}={}", auth.cookie_name(), auth.cookie_value()),
        );
        headers.insert("x-nacre-brain-authorization".into(), bearer);
        headers.insert("sec-fetch-site".into(), "same-site".into());
        headers.insert("origin".into(), format!("http://127.0.0.1:{attacker_port}"));
        headers.insert(
            "referer".into(),
            format!("http://127.0.0.1:{attacker_port}/"),
        );
        let response = authorize(
            Request {
                method: "GET".into(),
                path: "/api/brain".into(),
                query: String::new(),
                headers,
                body: Vec::new(),
            },
            port,
            &mut auth,
        );
        assert_eq!(response.unwrap_err().status, 403);
        drop(victim);
        drop(attacker);
    }

    #[test]
    fn browser_auth_gates_preserve_get_post_and_json_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let port = 17843;
        let mut auth = BrowserAuth::open(dir.path(), port).unwrap();
        let cookie = format!("{}={}", auth.cookie_name(), auth.cookie_value());
        let bearer = auth.bearer_value().to_owned();
        let canonical = format!("http://127.0.0.1:{port}");
        let make = |method: &str, path: &str, body: &[u8]| {
            let mut headers = BTreeMap::new();
            headers.insert("host".into(), format!("127.0.0.1:{port}"));
            headers.insert("cookie".into(), cookie.clone());
            headers.insert("x-nacre-brain-authorization".into(), bearer.clone());
            headers.insert("origin".into(), canonical.clone());
            headers.insert("sec-fetch-site".into(), "same-origin".into());
            headers.insert("content-type".into(), "application/json".into());
            Request {
                method: method.into(),
                path: path.into(),
                query: String::new(),
                headers,
                body: body.to_vec(),
            }
        };
        let mut missing_cookie = make("GET", "/api/brain", &[]);
        missing_cookie.headers.remove("cookie");
        assert_eq!(
            authorize(missing_cookie, port, &mut auth)
                .unwrap_err()
                .status,
            403
        );
        assert_eq!(
            authorize(make("GET", "/api/brain", &[]), port, &mut auth)
                .unwrap()
                .path,
            "/api/brain"
        );
        let mut stolen_cookie = make("GET", "/api/brain", &[]);
        stolen_cookie.headers.remove("x-nacre-brain-authorization");
        assert_eq!(
            authorize(stolen_cookie, port, &mut auth)
                .unwrap_err()
                .status,
            403
        );
        let mut wrong_bearer = make("GET", "/api/brain", &[]);
        wrong_bearer
            .headers
            .insert("x-nacre-brain-authorization".into(), "stolen".into());
        assert_eq!(
            authorize(wrong_bearer, port, &mut auth).unwrap_err().status,
            403
        );

        let invalid_json = [b"[]".as_slice(), b"null", b"42", b"\"text\""];
        for body in invalid_json {
            assert_eq!(
                authorize(make("POST", "/api/action", body), port, &mut auth)
                    .unwrap_err()
                    .status,
                400
            );
        }
        assert!(authorize(
            make("POST", "/api/action", br#"{"name":"capture"}"#),
            port,
            &mut auth
        )
        .is_ok());

        for (name, value) in [
            ("cookie", "nacre_brain_bad=wrong"),
            ("x-nacre-brain-authorization", "stolen"),
            ("origin", "http://evil.invalid"),
            ("host", "evil.invalid"),
            ("content-type", "application/x-www-form-urlencoded"),
        ] {
            let mut request = make("POST", "/api/action", br#"{"name":"capture"}"#);
            request.headers.insert(name.into(), value.into());
            assert_eq!(
                authorize(request, port, &mut auth).unwrap_err().status,
                403,
                "{name}"
            );
        }
    }

    #[test]
    fn health_readiness_proof_is_optional_and_preserves_public_body() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_path_buf();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let auth = BrowserAuth::open(&state, port).unwrap();
        let nonce = crate::brain_auth::readiness_challenge().unwrap();
        let expected_proof = auth.readiness_proof(&nonce).unwrap();
        let server = std::thread::spawn(move || {
            serve(listener, &state, Duration::from_millis(400), |_request| {
                Response::new(
                    200,
                    "application/json",
                    br#"{"ready":true,"authVersion":1}"#.to_vec(),
                )
            })
            .unwrap()
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            client,
            "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Nacre-Brain-Nonce: {nonce}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.contains(&format!("X-Nacre-Brain-Proof: {expected_proof}\r\n")));
        assert!(response.ends_with("{\"ready\":true,\"authVersion\":1}"));

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            client,
            "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(!response.contains("X-Nacre-Brain-Proof:"));
        assert!(response.ends_with("{\"ready\":true,\"authVersion\":1}"));

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            client,
            "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Nacre-Brain-Nonce: invalid\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        server.join().unwrap();
    }

    #[test]
    fn bootstrap_redirect_delivers_bearer_only_in_origin_fragment() {
        let dir = tempfile::tempdir().unwrap();
        let port = 17843;
        let mut auth = BrowserAuth::open(dir.path(), port).unwrap();
        let token = auth.bootstrap_token().unwrap().to_owned();
        let bearer = auth.bearer_value().to_owned();
        let mut headers = BTreeMap::new();
        headers.insert("host".into(), format!("127.0.0.1:{port}"));
        let result = authorize(
            Request {
                method: "GET".into(),
                path: "/".into(),
                query: format!("token={token}&handoff=1"),
                headers,
                body: Vec::new(),
            },
            port,
            &mut auth,
        );
        let response = result.unwrap_err();
        assert_eq!(response.status, 303);
        assert!(response
            .headers
            .iter()
            .any(|(name, value)| name == "Location"
                && value == &format!("/auth-complete#nacre-auth={bearer}")));
        assert!(response
            .headers
            .iter()
            .any(|(name, value)| name == "Set-Cookie" && value.contains("HttpOnly")));
    }

    #[test]
    fn refuses_non_loopback_listener_and_caps_responses() {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        assert!(serve(listener, dir.path(), Duration::ZERO, |_request| {
            Response::new(200, "text/plain", Vec::new())
        })
        .is_err());

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        write_response(
            &mut server,
            Response::new(200, "text/plain", vec![b'x'; RESPONSE_LIMIT + 1]),
        )
        .unwrap();
        drop(server);
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        assert!(response.starts_with(b"HTTP/1.1 500 Internal Server Error"));
    }

    #[test]
    fn real_loopback_ports_reject_same_site_navigation_with_host_wide_cookie() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_path_buf();
        let victim = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let attacker = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = victim.local_addr().unwrap().port();
        let attacker_port = attacker.local_addr().unwrap().port();
        let auth = BrowserAuth::open(&state, port).unwrap();
        let cookie = format!("{}={}", auth.cookie_name(), auth.cookie_value());
        let server = std::thread::spawn(move || {
            serve(victim, &state, Duration::from_millis(300), |_request| {
                Response::new(200, "application/json", b"{}".to_vec())
            })
            .unwrap()
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(client, "GET /api/brain HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\nSec-Fetch-Site: same-site\r\nOrigin: http://127.0.0.1:{attacker_port}\r\nReferer: http://127.0.0.1:{attacker_port}/\r\nConnection: close\r\n\r\n").unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        assert!(response.starts_with(b"HTTP/1.1 403 Forbidden"));
        server.join().unwrap();
        drop(attacker);
    }

    fn wait_for_server(port: u16) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            client
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            write!(
                client,
                "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
            )
            .unwrap();
            let mut byte = [0u8; 1];
            if matches!(client.read(&mut byte), Ok(1)) {
                return;
            }
            assert!(Instant::now() < deadline, "fixture server did not start");
        }
    }

    #[test]
    fn server_caps_simultaneous_clients() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_path_buf();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            serve(listener, &state, Duration::from_millis(200), |_request| {
                Response::new(200, "text/plain", b"ok".to_vec())
            })
            .unwrap()
        });
        wait_for_server(port);
        let mut clients = Vec::new();
        for _ in 0..CLIENT_LIMIT {
            let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            client.write_all(b"G").unwrap();
            clients.push(client);
        }
        std::thread::sleep(Duration::from_millis(300));
        let mut excess = TcpStream::connect(("127.0.0.1", port)).unwrap();
        excess
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0_u8; 1];
        match excess.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("excess client was not dropped: {result:?}"),
        }
        drop(excess);
        drop(clients);
        server.join().unwrap();
    }

    #[test]
    fn connection_watchdog_closes_client_while_route_callback_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_path_buf();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let server = std::thread::spawn(move || {
            serve_inner(
                listener,
                &state,
                Duration::from_millis(200),
                Duration::from_secs(1),
                move |_request| {
                    let _ = started_tx.send(());
                    let _ = release_rx.lock().unwrap().recv();
                    Response::new(200, "text/plain", b"done".to_vec())
                },
            )
            .unwrap()
        });
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            client,
            "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        started_rx.recv_timeout(Duration::from_secs(15)).unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        assert!(response.is_empty());
        release_tx.send(()).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn rejects_transfer_encoding_and_duplicate_security_headers() {
        for raw in [
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nTransfer-Encoding: chunked\r\n\r\n".as_slice(),
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1:1\r\nOrigin: a\r\nOrigin: b\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1:1\r\nHost: localhost:1\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1:1\r\nCookie: a=b\r\nCookie: c=d\r\n\r\n",
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\n{}",
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: -1\r\n\r\n",
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 0\r\n\r\n",
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 4097\r\n\r\n",
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: invalid\r\n\r\n",
        ] {
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            client.write_all(raw).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            assert_eq!(read_request(&mut server).unwrap().unwrap().method, "INVALID");
        }
    }

    #[test]
    fn header_and_body_deadlines_close_incomplete_requests() {
        let mut handles = Vec::new();
        let mut clients = Vec::new();
        for raw in [
            b"GET / HTTP/1.1\r\nHost: partial".as_slice(),
            b"POST /api/action HTTP/1.1\r\nHost: 127.0.0.1:1\r\nContent-Length: 2\r\n\r\n{",
        ] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            client.write_all(raw).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            clients.push(client);
            handles.push(std::thread::spawn(move || {
                let start = Instant::now();
                let request = read_request(&mut server).unwrap().unwrap();
                assert_eq!(request.method, "INVALID");
                assert!(start.elapsed() >= IO_DEADLINE);
                assert!(start.elapsed() < IO_DEADLINE + Duration::from_secs(3));
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        drop(clients);
    }
    #[test]
    fn response_write_deadline_bounds_a_client_that_never_reads() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let start = Instant::now();
        assert!(write_response(
            &mut server,
            Response::new(200, "application/octet-stream", vec![b'x'; RESPONSE_LIMIT])
        )
        .is_err());
        assert!(start.elapsed() < IO_DEADLINE + Duration::from_secs(2));
    }
}
