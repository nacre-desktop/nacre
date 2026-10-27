//! Native nacrectl entry point; no shell/interpreter between bindings and the link.
use super::socket;
use crate::Result;
use nacre_protocol::{framing, ErrorCode, Id, Message, Method, Mirror, Reply, Update};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader as StdBufReader, Read, Write},
    os::unix::net::UnixStream as StdUnixStream,
    time::Duration,
};
use tokio::{
    io::BufReader,
    net::{
        unix::{OwnedReadHalf, OwnedWriteHalf},
        UnixStream,
    },
};
pub struct Client {
    pub reader: framing::Reader<BufReader<OwnedReadHalf>>,
    pub writer: OwnedWriteHalf,
    pub welcome: Message,
    pub methods: BTreeMap<String, Method>,
}
#[derive(Debug)]
pub enum ConnectError {
    Absent,
    Failed(crate::Error),
}
impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Absent => f.write_str("Nacre daemon is unavailable"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for ConnectError {}
fn absent(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}
pub async fn connect() -> std::result::Result<Client, ConnectError> {
    let path = socket::path().map_err(ConnectError::Failed)?;
    connect_at(&path, "nacrectl").await
}
pub(crate) async fn connect_at(
    path: &std::path::Path,
    name: &str,
) -> std::result::Result<Client, ConnectError> {
    let stream = transport(path).await?;
    let (read, mut writer) = stream.into_split();
    let mut reader = framing::Reader::new(BufReader::new(read));
    let hello = Message::Hello {
        proto: nacre_protocol::VERSION,
        min: Some(nacre_protocol::MIN_VERSION),
        client: name.into(),
    };
    let welcome = tokio::time::timeout(Duration::from_secs(5), async {
        framing::write(&mut writer, &hello).await?;
        reader.next().await?.ok_or_else(|| {
            nacre_protocol::Failure::new(ErrorCode::Unavailable, "Daemon handshake ended", true)
        })
    })
    .await
    .map_err(|_| {
        ConnectError::Failed(
            nacre_protocol::Failure::new(ErrorCode::Timeout, "Daemon handshake timed out", true)
                .into(),
        )
    })?
    .map_err(|error| ConnectError::Failed(error.into()))?;
    let methods = match &welcome {
        Message::Welcome {
            proto,
            min,
            methods,
            ..
        } if *proto <= nacre_protocol::VERSION && *min <= nacre_protocol::VERSION => {
            methods.clone()
        }
        Message::Error { err } => return Err(ConnectError::Failed(err.clone().into())),
        _ => return Err(ConnectError::Failed("Unexpected daemon handshake".into())),
    };
    Ok(Client {
        reader,
        writer,
        welcome,
        methods,
    })
}
/// Checked transport without a handshake, for the persistent UI stdio bridge.
/// The UI owns hello, subscriptions, call IDs and reconnect state.
pub(crate) async fn transport(
    path: &std::path::Path,
) -> std::result::Result<UnixStream, ConnectError> {
    if let Err(error) = socket::validate_endpoint(path) {
        return Err(if error.downcast_ref::<io::Error>().is_some_and(absent) {
            ConnectError::Absent
        } else {
            ConnectError::Failed(error)
        });
    }
    let stream = tokio::time::timeout(Duration::from_secs(2), UnixStream::connect(path))
        .await
        .map_err(|_| {
            ConnectError::Failed(
                nacre_protocol::Failure::new(
                    ErrorCode::Timeout,
                    "Daemon connection timed out",
                    true,
                )
                .into(),
            )
        })?
        .map_err(|error| {
            if absent(&error) {
                ConnectError::Absent
            } else {
                ConnectError::Failed(error.into())
            }
        })?;
    socket::check_uid(
        stream
            .peer_cred()
            .map_err(|error| ConnectError::Failed(error.into()))?
            .uid(),
    )
    .map_err(ConnectError::Failed)?;
    Ok(stream)
}
impl Client {
    pub(crate) fn control(&self) -> Result<std::os::unix::net::UnixStream> {
        use std::os::fd::AsFd;
        Ok(self.writer.as_ref().as_fd().try_clone_to_owned()?.into())
    }
    pub async fn call(
        &mut self,
        method: &str,
        params: Value,
        timeout: Option<u32>,
    ) -> Result<Value> {
        let milliseconds =
            timeout.unwrap_or_else(|| self.methods.get(method).map_or(1000, |m| m.timeout));
        let request = Message::Call {
            id: Id::Integer(1),
            m: method.into(),
            p: params,
            timeout,
        };
        tokio::time::timeout(Duration::from_millis(u64::from(milliseconds)), async {
            framing::write(&mut self.writer, &request).await?;
            match self.reader.next().await?.ok_or("Daemon disconnected")? {
                Message::Ret {
                    id: Id::Integer(1),
                    reply: Reply::Ok { ok },
                } => Ok(ok),
                Message::Ret {
                    id: Id::Integer(1),
                    reply: Reply::Err { err },
                } => Err(err.into()),
                Message::Error { err } => Err(err.into()),
                _ => Err("Unexpected daemon reply".into()),
            }
        })
        .await
        .map_err(|_| "Daemon call timed out; the action may already have completed")?
    }
}
fn print(message: &Message) -> Result<()> {
    let bytes = framing::encode(message)?;
    let mut output = io::stdout().lock();
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}
async fn watch(topic: &str) -> Result<i32> {
    tokio::select! {
        result = watch_messages(topic) => result,
        closed = output_closed() => { closed?; Ok(0) },
    }
}
pub(crate) async fn output_closed() -> Result<()> {
    let output = io::stdout();
    if rustix::fs::fstat(&output)?.st_mode & nix::libc::S_IFMT != nix::libc::S_IFIFO {
        std::future::pending::<()>().await;
        return Ok(());
    }
    // Reopening our pipe gives the observer independent nonblocking flags;
    // duplicating stdout would change the flags used by ordinary output writes.
    let descriptor = match rustix::fs::open(
        "/proc/self/fd/1",
        rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(rustix::io::Errno::NXIO) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    // Linux reports EPOLLERR when the last reader closes. No timer polls it.
    let observer = tokio::net::unix::pipe::Sender::from_owned_fd(descriptor)?;
    let _closed = observer.ready(tokio::io::Interest::ERROR).await?;
    Ok(())
}
async fn watch_messages(topic: &str) -> Result<i32> {
    let mut mirror = Mirror::default();
    let mut retry = 50u64;
    loop {
        let result = async {
            let mut client = connect().await?;
            mirror.receive(&client.welcome);
            print(&client.welcome)?;
            framing::write(
                &mut client.writer,
                &Message::Sub {
                    topics: vec![topic.into()],
                },
            )
            .await?;
            while let Some(message) = client.reader.next().await? {
                match &message {
                    Message::Error { err } if !err.retry => {
                        return Err(crate::Error::from(err.clone()))
                    }
                    Message::Lost { topic, .. } => {
                        framing::write(
                            &mut client.writer,
                            &Message::Resync {
                                topics: vec![topic.clone()],
                            },
                        )
                        .await?
                    }
                    _ => {}
                }
                if let Update::Resync(topic) = mirror.receive(&message) {
                    framing::write(
                        &mut client.writer,
                        &Message::Resync {
                            topics: vec![topic],
                        },
                    )
                    .await?;
                    continue;
                }
                if matches!(message, Message::Snap { .. }) {
                    retry = 50;
                }
                print(&message)?;
            }
            Err::<(), crate::Error>("Daemon disconnected".into())
        }
        .await;
        if let Err(error) = result {
            // Socket I/O is already a typed Failure. A bare I/O error here is stdout.
            if let Some(output) = error.downcast_ref::<io::Error>() {
                if output.kind() == io::ErrorKind::BrokenPipe {
                    return Ok(0);
                }
                return Err(error);
            }
            if let Some(ConnectError::Failed(inner)) = error.downcast_ref::<ConnectError>() {
                if inner
                    .downcast_ref::<nacre_protocol::Failure>()
                    .is_none_or(|error| !error.retry)
                {
                    return Err(error);
                }
            }
            if error
                .downcast_ref::<nacre_protocol::Failure>()
                .is_some_and(|error| !error.retry)
            {
                return Err(error);
            }
            eprintln!("nacrectl: daemon disconnected; reconnecting");
        }
        tokio::time::sleep(Duration::from_millis(retry)).await;
        retry = (retry * 2).min(2000);
    }
}
/// How a provider-neutral route call made from a helper process ended (N0048).
#[derive(Debug, PartialEq)]
pub enum UiReply {
    /// The registered UI answered.
    Ok(Value),
    /// A UI is registered and refused the call or could not do it.
    Refused,
    /// No daemon, or the daemon proved that no UI is registered.
    NoProvider,
}
/// `nacrectl ui TARGET METHOD ARGS` for a helper that must not care which shell is running.
pub fn ui_call(target: &str, method: &str, args: &[&str]) -> UiReply {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return UiReply::NoProvider;
    };
    runtime.block_on(async {
        let Ok(mut client) = connect().await else {
            return UiReply::NoProvider;
        };
        match client
            .call(
                "ui.dispatch",
                json!({"target":target,"method":method,"args":args}),
                None,
            )
            .await
        {
            Ok(value) => UiReply::Ok(value),
            Err(error)
                if error
                    .downcast_ref::<nacre_protocol::Failure>()
                    .is_some_and(|f| {
                        f.code == ErrorCode::Unavailable && f.dispatched == Some(false)
                    }) =>
            {
                UiReply::NoProvider
            }
            Err(_) => UiReply::Refused,
        }
    })
}
/// The daemon's own answer to a media key, for shells that do not host the QML `mpris` target:
/// the active `dashboard.player` entity gets `player.{play_pause,next,previous}`.
pub fn player_key(method: &str) -> Result<()> {
    if !["player.play_pause", "player.next", "player.previous"].contains(&method) {
        return Err("Unknown player key".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut client = connect().await?;
        framing::write(
            &mut client.writer,
            &Message::Sub {
                topics: vec!["dashboard.player".into()],
            },
        )
        .await?;
        let data = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match client.reader.next().await?.ok_or("Daemon disconnected")? {
                    Message::Snap { topic, data, .. } if topic == "dashboard.player" => {
                        return Ok::<Value, crate::Error>(data)
                    }
                    Message::Error { err } => return Err(err.into()),
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| "No media player state")??;
        let players = &data["players"];
        let id = data["active"]
            .as_str()
            .or_else(|| players["order"][0].as_str())
            .ok_or("No media player")?;
        client.call(method, json!({"player":id}), None).await?;
        Ok(())
    })
}
/// The only remaining `quickshell ipc` caller for shell routes (N0048). It runs when the
/// daemon is absent or no UI provider is registered, which today means the QML shell
/// is the only provider that can exist. Expiry: delete this function and the
/// `fallback` flag in `run` in the Group F QML deletion pass.
fn legacy_ui(arguments: &[String]) -> Result<i32> {
    use std::os::unix::process::CommandExt;
    let paths = crate::paths::Paths::environment()?;
    let binary = paths.share.join("shell/bin/qs");
    // `nacre settings PAGE` is provider-neutral; the legacy QML handler is `settingsApp open`.
    let arguments = match arguments {
        [target, method, page] if target == "nacre" && method == "settings" => {
            vec!["settingsApp".to_owned(), "open".to_owned(), page.clone()]
        }
        _ => arguments.to_vec(),
    };
    Err(std::process::Command::new(binary)
        .args(["-c", "nacre", "ipc", "call"])
        .args(arguments)
        .exec()
        .into())
}
fn usage() -> i32 {
    println!("nacrectl status\nnacrectl call [--timeout MS] METHOD [JSON]\nnacrectl call --list\nnacrectl key-step METHOD JSON\nnacrectl brain open|capture\nnacrectl watch TOPIC\nnacrectl attach\nnacrectl ui TARGET METHOD [ARGS...]");
    0
}
pub fn run(arguments: &[String]) -> Result<i32> {
    if arguments.is_empty() || matches!(arguments,[flag] if matches!(flag.as_str(),"--help"|"help"))
    {
        return Ok(usage());
    }
    // Hardware key repeats and a tiny allowlist of cheap calls use a single
    // self-contained frame before creating a Tokio runtime or loading the
    // daemon's method catalogue.
    if arguments
        .first()
        .is_some_and(|command| command == "key-step")
    {
        return match key_step(&arguments[1..]) {
            Ok(()) => Ok(0),
            Err(KeyStepError::DaemonAbsent) => Ok(69),
            Err(KeyStepError::Failed(error)) => Err(error),
        };
    }
    if let Some((method, params)) = fast_cli_call(arguments)? {
        let path = socket::path()?;
        match fast_call_at(&path, &method, params) {
            Ok(Some(value)) => {
                println!("{}", serde_json::to_string(&value)?);
                return Ok(0);
            }
            Ok(None) => {} // Older daemon: preserve the persistent protocol fallback.
            Err(FastCallError::DaemonAbsent) => {
                return Err("Nacre daemon is unavailable".into());
            }
            Err(FastCallError::Failed(error)) => return Err(error),
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    if matches!(arguments, [command] if command == "attach") {
        return runtime.block_on(super::attach::run());
    }
    if let [command, topic] = arguments {
        if command == "watch" {
            let probe = Message::Sub {
                topics: vec![topic.clone()],
            };
            probe.validate()?;
            return runtime.block_on(watch(topic));
        }
    }
    let (method, params, timeout, list) = match arguments {
        [command] if command == "status" => ("daemon.status".to_owned(), json!({}), None, false),
        [command, action]
            if command == "brain" && ["open", "capture"].contains(&action.as_str()) =>
        {
            (format!("brain.{action}"), json!({}), None, false)
        }
        [command, flag] if command == "call" && flag == "--list" => {
            (String::new(), json!({}), None, true)
        }
        [command, target, method, rest @ ..] if command == "ui" => (
            "ui.dispatch".to_owned(),
            json!({"target":target,"method":method,"args":rest}),
            None,
            false,
        ),
        [command, rest @ ..] if command == "call" => {
            let (rest, timeout) = match rest {
                [flag, ms, tail @ ..] if flag == "--timeout" => (tail, Some(ms.parse::<u32>()?)),
                _ => (rest, None),
            };
            match rest {
                [method] => (method.clone(), json!({}), timeout, false),
                [method, params] => (
                    method.clone(),
                    serde_json::from_str(params)?,
                    timeout,
                    false,
                ),
                _ => return Err("Expected call METHOD [JSON]".into()),
            }
        }
        _ => {
            return Err(
                "Expected status, call, brain, watch, attach or ui; use nacrectl --help".into(),
            )
        }
    };
    if !list {
        Message::Call {
            id: Id::Integer(1),
            m: method.clone(),
            p: params.clone(),
            timeout,
        }
        .validate()?;
    }
    let client = runtime.block_on(connect());
    let fallback = matches!(arguments.first().map(String::as_str), Some("ui"))
        && std::env::var_os("NACRE_DAEMON_SOCKET").is_none();
    match client {
        Err(ConnectError::Absent) if fallback => {
            drop(runtime);
            legacy_ui(&arguments[1..])
        }
        Err(error) => Err(error.into()),
        Ok(mut client) => {
            let result = if list {
                serde_json::to_value(&client.methods)?
            } else {
                match runtime.block_on(client.call(&method, params, timeout)) {
                    Ok(value) => value,
                    Err(error)
                        if fallback
                            && error.downcast_ref::<nacre_protocol::Failure>().is_some_and(
                                |error| {
                                    error.code == ErrorCode::Unavailable
                                        && error.dispatched == Some(false)
                                },
                            ) =>
                    {
                        drop(client);
                        drop(runtime);
                        return legacy_ui(&arguments[1..]);
                    }
                    Err(error) => return Err(error),
                }
            };
            println!("{}", serde_json::to_string(&result)?);
            Ok(0)
        }
    }
}

enum KeyStepError {
    DaemonAbsent,
    Failed(crate::Error),
}

fn key_step(arguments: &[String]) -> std::result::Result<(), KeyStepError> {
    let path = socket::path().map_err(KeyStepError::Failed)?;
    key_step_at(arguments, &path)
}

fn key_step_at(
    arguments: &[String],
    path: &std::path::Path,
) -> std::result::Result<(), KeyStepError> {
    let fail = |error: crate::Error| KeyStepError::Failed(error);
    let [method, params] = arguments else {
        return Err(fail("Expected key-step METHOD JSON".into()));
    };
    if !matches!(
        method.as_str(),
        "audio.step" | "brightness.adjust" | "keyboard_light.adjust"
    ) {
        return Err(fail("Unsupported key-step method".into()));
    }
    let params: Value = serde_json::from_str(params).map_err(|error| fail(error.into()))?;
    match fast_call_at(path, method, params.clone()).map_err(|error| match error {
        FastCallError::DaemonAbsent => KeyStepError::DaemonAbsent,
        FastCallError::Failed(error) => KeyStepError::Failed(error),
    })? {
        Some(_) => Ok(()),
        None => legacy_key_step_at(method, params, path),
    }
}

fn legacy_key_step_at(
    method: &str,
    params: Value,
    path: &std::path::Path,
) -> std::result::Result<(), KeyStepError> {
    let fail = |error: crate::Error| KeyStepError::Failed(error);
    let call = Message::Call {
        id: Id::Integer(1),
        m: method.to_owned(),
        p: params,
        timeout: None,
    };
    let hello = Message::Hello {
        proto: nacre_protocol::VERSION,
        min: Some(nacre_protocol::MIN_VERSION),
        client: "nacrectl-key-compat".into(),
    };
    socket::validate_endpoint(path).map_err(|error| {
        if error.downcast_ref::<std::io::Error>().is_some_and(absent) {
            KeyStepError::DaemonAbsent
        } else {
            fail(error)
        }
    })?;
    let mut stream = StdUnixStream::connect(path).map_err(|error| {
        if absent(&error) {
            KeyStepError::DaemonAbsent
        } else {
            fail(error.into())
        }
    })?;
    let peer = nix::sys::socket::getsockopt(&stream, nix::sys::socket::sockopt::PeerCredentials)
        .map_err(|error| fail(error.into()))?;
    socket::check_uid(peer.uid()).map_err(fail)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| fail(error.into()))?;
    let mut request = framing::encode(&hello).map_err(|error| fail(error.into()))?;
    request.extend(framing::encode(&call).map_err(|error| fail(error.into()))?);
    stream
        .write_all(&request)
        .map_err(|error| fail(error.into()))?;
    let mut reader = StdBufReader::new(stream);
    let mut frame = Vec::with_capacity(256);
    for index in 0..2 {
        frame.clear();
        let bytes = reader
            .by_ref()
            .take((nacre_protocol::MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut frame)
            .map_err(|error| fail(error.into()))?;
        if bytes == 0 || bytes > nacre_protocol::MAX_FRAME || frame.last() != Some(&b'\n') {
            return Err(fail("Daemon returned an invalid key-step response".into()));
        }
        if index == 1 {
            let reply: Value =
                serde_json::from_slice(&frame).map_err(|error| fail(error.into()))?;
            if reply.get("t") != Some(&Value::String("ret".into()))
                || reply.get("ok").is_none()
                || reply.get("err").is_some()
            {
                return Err(fail("Daemon rejected key-step request".into()));
            }
        }
    }
    Ok(())
}

enum FastCallError {
    DaemonAbsent,
    Failed(crate::Error),
}

fn fast_call_at(
    main: &std::path::Path,
    method: &str,
    params: Value,
) -> std::result::Result<Option<Value>, FastCallError> {
    let fail = |error: crate::Error| FastCallError::Failed(error);
    socket::validate_endpoint(main).map_err(|error| {
        if error.downcast_ref::<std::io::Error>().is_some_and(absent) {
            FastCallError::DaemonAbsent
        } else {
            fail(error)
        }
    })?;
    let fast = socket::fast_path_for(main).map_err(FastCallError::Failed)?;
    if let Err(error) = socket::validate_endpoint(&fast) {
        if error.downcast_ref::<std::io::Error>().is_some_and(absent) {
            return Ok(None);
        }
        return Err(fail(error));
    }
    let mut stream = match std::os::unix::net::UnixStream::connect(&fast) {
        Ok(stream) => stream,
        Err(error) if absent(&error) => return Ok(None),
        Err(error) => return Err(fail(error.into())),
    };
    let peer = nix::sys::socket::getsockopt(&stream, nix::sys::socket::sockopt::PeerCredentials)
        .map_err(|error| fail(error.into()))?;
    socket::check_uid(peer.uid()).map_err(fail)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .and_then(|_| stream.set_write_timeout(Some(Duration::from_secs(3))))
        .map_err(|error| fail(error.into()))?;
    let request = Message::FastCall {
        proto: nacre_protocol::VERSION,
        id: Id::Integer(1),
        m: method.to_owned(),
        p: params,
    };
    let bytes = framing::encode(&request).map_err(|error| fail(error.into()))?;
    stream
        .write_all(&bytes)
        .map_err(|error| fail(error.into()))?;
    let mut reader = StdBufReader::new(stream);
    let mut frame = Vec::with_capacity(256);
    let bytes = reader
        .by_ref()
        .take((nacre_protocol::MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut frame)
        .map_err(|error| fail(error.into()))?;
    if bytes == 0 || bytes > nacre_protocol::MAX_FRAME || frame.last() != Some(&b'\n') {
        return Err(fail("Daemon returned an invalid one-shot response".into()));
    }
    match framing::decode(&frame).map_err(|error| fail(error.into()))? {
        Message::Ret {
            id: Id::Integer(1),
            reply: Reply::Ok { ok },
        } => Ok(Some(ok)),
        Message::Ret {
            id: Id::Integer(1),
            reply: Reply::Err { err },
        }
        | Message::Error { err } => Err(fail(err.into())),
        _ => Err(fail(
            "Daemon returned a mismatched one-shot response".into(),
        )),
    }
}

fn fast_cli_call(
    arguments: &[String],
) -> std::result::Result<Option<(String, Value)>, crate::Error> {
    let [command, rest @ ..] = arguments else {
        return Ok(None);
    };
    if command != "call" {
        return Ok(None);
    }
    let (rest, timeout) = match rest {
        [flag, ms, tail @ ..] if flag == "--timeout" => (tail, Some(ms.parse::<u32>()?)),
        _ => (rest, None),
    };
    if timeout.is_some() {
        return Ok(None);
    }
    let (method, params) = match rest {
        [method] => (method.clone(), json!({})),
        [method, params] => (method.clone(), serde_json::from_str(params)?),
        _ => return Ok(None),
    };
    if matches!(
        method.as_str(),
        "daemon.ping" | "audio.step" | "brightness.adjust" | "keyboard_light.adjust"
    ) {
        Ok(Some((method, params)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod key_step_tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::fs::PermissionsExt,
        os::unix::net::UnixListener,
    };

    #[test]
    fn key_step_sends_one_validated_frame_without_the_welcome() -> Result<()> {
        let home = tempfile::tempdir()?;
        let parent = home.path().join("nacre");
        std::fs::create_dir(&parent)?;
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))?;
        let path = parent.join("daemon.sock");
        let _primary = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let fast_path = socket::fast_path_for(&path)?;
        let listener = UnixListener::bind(&fast_path)?;
        std::fs::set_permissions(&fast_path, std::fs::Permissions::from_mode(0o600))?;
        let server = std::thread::spawn(move || -> std::io::Result<Value> {
            let (stream, _) = listener.accept()?;
            let mut reader = BufReader::new(stream);
            let mut call = String::new();
            reader.read_line(&mut call)?;
            reader
                .get_mut()
                .write_all(b"{\"t\":\"ret\",\"id\":1,\"ok\":{\"changed\":true}}\n")?;
            Ok(serde_json::from_str(&call).unwrap())
        });
        key_step_at(
            &[
                "audio.step".into(),
                r#"{"direction":"up","key_ms":123}"#.into(),
            ],
            &path,
        )
        .map_err(|error| match error {
            KeyStepError::DaemonAbsent => "test daemon unexpectedly absent".into(),
            KeyStepError::Failed(error) => error,
        })?;
        let call = server.join().expect("mock daemon thread")?;
        assert_eq!(call["t"], "fastcall");
        assert_eq!(call["m"], "audio.step");
        assert_eq!(call["p"]["key_ms"], 123);
        assert!(key_step_at(&["daemon.status".into(), "{}".into()], &path).is_err());
        assert!(matches!(
            key_step_at(
                &[
                    "audio.step".into(),
                    r#"{"direction":"up","key_ms":123}"#.into(),
                ],
                &parent.join("missing.sock"),
            ),
            Err(KeyStepError::DaemonAbsent)
        ));
        Ok(())
    }

    #[test]
    fn key_step_uses_legacy_protocol_only_when_the_fast_sidecar_is_absent() -> Result<()> {
        let home = tempfile::tempdir()?;
        let parent = home.path().join("nacre");
        std::fs::create_dir(&parent)?;
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))?;
        let path = parent.join("daemon.sock");
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            let (stream, _) = listener.accept()?;
            let mut reader = BufReader::new(stream);
            let mut hello = String::new();
            let mut call = String::new();
            reader.read_line(&mut hello)?;
            reader.read_line(&mut call)?;
            reader
                .get_mut()
                .write_all(b"{\"t\":\"welcome\"}\n{\"t\":\"ret\",\"id\":1,\"ok\":{}}\n")?;
            Ok(())
        });
        key_step_at(
            &["audio.step".into(), r#"{"direction":"up"}"#.into()],
            &path,
        )
        .map_err(|error| match error {
            KeyStepError::DaemonAbsent => "test daemon unexpectedly absent".into(),
            KeyStepError::Failed(error) => error,
        })?;
        server.join().expect("mock compatibility server")?;
        Ok(())
    }
}
