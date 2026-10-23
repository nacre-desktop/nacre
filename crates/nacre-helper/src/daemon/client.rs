//! Native nacrectl entry point; no shell/interpreter between bindings and the link.
use super::socket;
use crate::Result;
use nacre_protocol::{framing, ErrorCode, Id, Message, Method, Mirror, Reply, Update};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Write},
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
    println!("nacrectl status\nnacrectl call [--timeout MS] METHOD [JSON]\nnacrectl call --list\nnacrectl brain open|capture\nnacrectl watch TOPIC\nnacrectl attach\nnacrectl ui TARGET METHOD [ARGS...]");
    0
}
pub fn run(arguments: &[String]) -> Result<i32> {
    if arguments.is_empty() || matches!(arguments,[flag] if matches!(flag.as_str(),"--help"|"help"))
    {
        return Ok(usage());
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
