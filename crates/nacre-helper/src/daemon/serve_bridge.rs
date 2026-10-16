//! One persistent link keeps the legacy JSON-lines interface for one release.
use super::client::{self, ConnectError};
use crate::{
    service::{self, Request},
    Result,
};
use nacre_protocol::{
    framing,
    helpers::{HelperCall, HelperReply, HelperTransfer},
    Id, Message, Reply,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

type Output = Arc<Mutex<io::Stdout>>;
type Active = Arc<Mutex<HashSet<u64>>>;
enum Input {
    Request(Request),
    End(i32),
    Error(crate::Error),
}
fn response(output: &Output, id: u64, code: i32, value: String) -> Result<()> {
    service::send(
        output,
        json!({"id":id,"code":code,"output":value}),
        service::Limits::default().response,
    )
}
fn read_input(
    output: Output,
    active: Active,
    queue: tokio::sync::mpsc::Sender<Input>,
) -> Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let limits = service::Limits::default();
    loop {
        let mut bytes = Vec::new();
        if (&mut input)
            .take(limits.request + 1)
            .read_until(b'\n', &mut bytes)?
            == 0
        {
            queue
                .blocking_send(Input::End(0))
                .map_err(|_| "Helper bridge ended")?;
            return Ok(());
        }
        if bytes.len() as u64 > limits.request {
            service::send(
                &output,
                json!({"fatal":"Helper request is too large"}),
                limits.response,
            )?;
            queue
                .blocking_send(Input::End(1))
                .map_err(|_| "Helper bridge ended")?;
            return Ok(());
        }
        let value = serde_json::from_slice::<Value>(&bytes);
        let id = value
            .as_ref()
            .ok()
            .and_then(|v| v.get("id"))
            .filter(|v| v.is_i64() || v.is_u64())
            .cloned()
            .unwrap_or(json!(0));
        let request = value
            .map_err(|e| -> crate::Error { service::parse_error(e, &bytes).into() })
            .and_then(|v| service::validate(&v));
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                service::send(
                    &output,
                    json!({"id":id,"code":1,"output":service::error_output(&error.to_string())}),
                    limits.response,
                )?;
                continue;
            }
        };
        {
            let mut active = active.lock().map_err(|_| "Helper active lock failed")?;
            if active.contains(&request.id) {
                service::send(
                    &output,
                    json!({"fatal":"Duplicate helper request ID"}),
                    limits.response,
                )?;
                drop(active);
                queue
                    .blocking_send(Input::End(1))
                    .map_err(|_| "Helper bridge ended")?;
                return Ok(());
            }
            if active.len() >= limits.pending {
                response(
                    &output,
                    request.id,
                    1,
                    service::error_output("Helper queue is full"),
                )?;
                continue;
            }
            active.insert(request.id);
        }
        queue
            .blocking_send(Input::Request(request))
            .map_err(|_| "Helper bridge ended")?;
    }
}
struct Pending {
    _file: Option<tempfile::NamedTempFile>,
    deadline: Instant,
}
fn packet(directory: &Path, request: Request) -> Result<(Value, Option<tempfile::NamedTempFile>)> {
    let bytes = serde_json::to_vec(&request)?;
    if bytes.len() <= 384 * 1024 {
        return Ok((serde_json::to_value(HelperCall::Inline { request })?, None));
    }
    let mut file = tempfile::Builder::new()
        .prefix("packet-")
        .tempfile_in(directory)?;
    file.write_all(&bytes)?;
    file.flush()?;
    let name = file
        .path()
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("Invalid helper packet name")?
        .into();
    Ok((
        serde_json::to_value(HelperCall::File { file: name })?,
        Some(file),
    ))
}
fn reply(directory: &Path, value: Value) -> Result<nacre_protocol::helpers::HelperResult> {
    match serde_json::from_value::<HelperReply>(value)? {
        HelperReply::Inline(result) => Ok(result),
        HelperReply::File { file } => {
            if !file.starts_with("packet-")
                || file.len() > 96
                || !file
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                return Err("Invalid helper response path".into());
            }
            let path = directory.join(file);
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NOFOLLOW)
                .open(&path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file()
                || metadata.uid() != nix::unistd::getuid().as_raw()
                || metadata.mode() & 0o777 != 0o600
                || metadata.nlink() != 1
                || metadata.len() > 16 * 1024 * 1024
            {
                return Err("Unsafe helper response file".into());
            }
            let result = serde_json::from_reader(file.take(16 * 1024 * 1024 + 1));
            std::fs::remove_file(path)?;
            Ok(result?)
        }
    }
}
async fn run_link(mut client: client::Client) -> Result<i32> {
    let transfer: HelperTransfer = serde_json::from_value(
        client
            .call("helper.transfer", json!({}), Some(1000))
            .await?,
    )?;
    let directory = PathBuf::from(transfer.directory);
    let endpoint = super::socket::path()?;
    let metadata = std::fs::symlink_metadata(&directory)?;
    if directory.parent() != endpoint.parent()
        || !directory
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("helper-packets-"))
        || !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err("Unsafe helper transfer directory".into());
    }
    client.methods.clear();
    client.welcome = Message::Ping { id: Id::Integer(0) };
    let output = Arc::new(Mutex::new(io::stdout()));
    service::send(
        &output,
        json!({"ready":true,"version":1}),
        service::Limits::default().response,
    )?;
    let active = Arc::new(Mutex::new(HashSet::new()));
    let (sender, mut requests) = tokio::sync::mpsc::channel(64);
    let input_output = output.clone();
    let input_active = active.clone();
    // A dedicated stdin reader cannot block the socket runtime or retain a
    // Tokio blocking task forever when a disconnected bridge exits.
    std::thread::Builder::new()
        .name("nacre-helper-input".into())
        .spawn(move || {
            if let Err(error) = read_input(input_output, input_active, sender.clone()) {
                if sender.blocking_send(Input::Error(error)).is_err() {
                    eprintln!("Nacre helper input failed after bridge shutdown");
                }
            }
        })?;
    let mut pending = HashMap::<u64, Pending>::new();
    let mut ending = None;
    loop {
        if pending.is_empty() {
            if let Some(code) = ending {
                return Ok(code);
            }
        }
        let deadline = pending
            .values()
            .map(|v| v.deadline)
            .min()
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(120));
        tokio::select! {
            request=requests.recv(),if ending.is_none()=>match request {
                Some(Input::Request(request))=>{
                    let id=request.id;let (params,file)=packet(&directory,request)?;
                    framing::write(&mut client.writer,&Message::Call{id:Id::Integer(id as i64),m:"helper.execute".into(),p:params,timeout:Some(120000)}).await?;
                    pending.insert(id,Pending{_file:file,deadline:Instant::now()+Duration::from_secs(121)});
                },
                Some(Input::End(code))=>ending=Some(code),
                Some(Input::Error(error))=>return Err(error),
                None=>return Err("Helper input reader ended".into()),
            },
            incoming=client.reader.next()=>{
                let message=incoming?.ok_or("Daemon disconnected; helper actions may already have completed")?;
                let Message::Ret{id:Id::Integer(id),reply:result}=message else {return Err("Unexpected helper daemon message".into());};
                let id=u64::try_from(id).map_err(|_|"Invalid helper response ID")?;
                let Some(_pending)=pending.remove(&id) else {return Err("Unexpected helper daemon response ID".into());};
                match result {
                    Reply::Ok{ok}=>{let value=reply(&directory,ok)?;response(&output,id,value.code,value.output)?;},
                    Reply::Err{err}=>response(&output,id,1,service::error_output(&err.message))?,
                }
                active.lock().map_err(|_|"Helper active lock failed")?.remove(&id);
            },
            _=tokio::time::sleep_until(deadline.into()),if !pending.is_empty()=>return Err("Helper daemon deadline reached; actions may already have completed".into()),
        }
    }
}
/// Only a missing default/older endpoint permits standalone operation. No
/// dispatched action is retried after a deadline, lost reply or daemon crash.
pub fn run() -> Result<Option<i32>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let mut client = match client::connect().await {
            Ok(c) => c,
            Err(ConnectError::Absent) if !explicit => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if !client.methods.contains_key("helper.execute") {
            let status = client.call("daemon.status", json!({}), Some(1000)).await?;
            if !explicit && status["safe"] != true {
                return Ok(None);
            }
            return Err("Daemon helper lanes are unavailable".into());
        }
        run_link(client).await.map(Some)
    })
}
