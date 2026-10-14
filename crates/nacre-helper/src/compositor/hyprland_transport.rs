//! Hyprland control socket framing, bounded total deadline and reply size.
use super::{hyprland::Transport, niri_state::Error};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub fn exchange(
    path: &Path,
    command: &[u8],
    timeout: Duration,
    maximum: usize,
) -> crate::Result<Vec<u8>> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("Invalid compositor timeout")?;
    let mut stream = UnixStream::connect(path)?;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or("Compositor IPC deadline exceeded")
    };
    let mut pending = command;
    while !pending.is_empty() {
        stream.set_write_timeout(Some(remaining()?))?;
        let count = stream.write(pending)?;
        if count == 0 {
            return Err("Compositor socket write failed".into());
        }
        pending = &pending[count..];
    }
    stream.shutdown(Shutdown::Write)?;
    let mut result = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        stream.set_read_timeout(Some(remaining()?))?;
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if result.len() + count > maximum {
            return Err("Compositor response exceeded its size limit".into());
        }
        result.extend_from_slice(&buffer[..count]);
    }
    Ok(result)
}
pub struct Socket {
    pub folder: PathBuf,
}
impl Socket {
    pub fn new(runtime: &Path, signature: &str) -> crate::Result<Self> {
        if signature.is_empty()
            || signature.contains('/')
            || signature.contains('\0')
            || signature == "."
            || signature == ".."
        {
            return Err("Invalid Hyprland instance signature".into());
        }
        Ok(Self {
            folder: runtime.join("hypr").join(signature),
        })
    }
}
pub fn instances(runtime: &Path) -> crate::Result<Value> {
    use nix::sys::signal::kill;
    let root = runtime.join("hypr");
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(json!([])),
        Err(error) => return Err(error.into()),
    };
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let id = entry.file_name();
        let Some(id) = id.to_str() else {
            continue;
        };
        let (Some(first), Some(last)) = (id.find('_'), id.rfind('_')) else {
            continue;
        };
        if last <= first {
            continue;
        }
        let Ok(time) = id[first + 1..last].parse::<u64>() else {
            continue;
        };
        let text = match std::fs::read_to_string(entry.path().join("hyprland.lock")) {
            Ok(text) => text,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        let mut lines = text.lines();
        let Some(pid) = lines
            .next()
            .and_then(|pid| pid.parse::<u32>().ok())
            .filter(|pid| *pid <= i32::MAX as u32)
        else {
            continue;
        };
        let Some(display) = lines.next() else {
            continue;
        };
        if lines.next().is_some_and(|line| !line.is_empty()) {
            continue;
        }
        if matches!(
            kill(nix::unistd::Pid::from_raw(pid as i32), None),
            Err(nix::errno::Errno::ESRCH)
        ) {
            continue;
        }
        result.push(json!({"instance":id,"time":time,"pid":pid,"wl_socket":display}));
    }
    result.sort_by_key(|value| value["time"].as_u64());
    Ok(Value::Array(result))
}
impl Transport for Socket {
    fn run(
        &mut self,
        args: Vec<String>,
        timeout: f64,
        check: bool,
    ) -> std::result::Result<Value, Error> {
        let execute = || -> crate::Result<Value> {
            let timeout = Duration::try_from_secs_f64(timeout)?;
            let json = args.iter().any(|argument| argument == "-j");
            let arguments = args
                .iter()
                .filter(|argument| argument.as_str() != "-j")
                .cloned()
                .collect::<Vec<_>>();
            if arguments.is_empty() {
                return Err("A compositor command is required".into());
            }
            if arguments[0] == "instances" {
                let runtime = self
                    .folder
                    .parent()
                    .and_then(Path::parent)
                    .ok_or("Invalid runtime path")?;
                return Ok(
                    json!({"code":0,"stdout":crate::json::encode(&instances(runtime)?)?+"\n","stderr":""}),
                );
            }
            let (path, command) = if arguments[0] == "hyprsunset" {
                (
                    self.folder.join(".hyprsunset.sock"),
                    arguments[1..].join(" "),
                )
            } else {
                (
                    self.folder.join(".socket.sock"),
                    format!("{}/{}", if json { "j" } else { "" }, arguments.join(" ")),
                )
            };
            let bytes = exchange(&path, command.as_bytes(), timeout, 4 * 1024 * 1024)?;
            let reply = String::from_utf8(bytes)?;
            let code = if reply.starts_with("error:") { 7 } else { 0 };
            if code != 0 && check {
                return Err(format!("Hyprland IPC: {}", reply.trim()).into());
            }
            Ok(json!({"code":code,"stdout":reply+"\n","stderr":""}))
        };
        execute().map_err(|error| Error {
            kind: "RuntimeError",
            message: error.to_string(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_discovery_reads_private_locks_and_orders_live_instances() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        for (name, lock) in [
            (
                "hash_200_tail",
                format!("{}\nfixture-1\n", std::process::id()),
            ),
            (
                "hash_100_tail",
                format!("{}\nfixture-0\n", std::process::id()),
            ),
            ("invalid", "bad\n".into()),
        ] {
            let folder = home.path().join("hypr").join(name);
            std::fs::create_dir_all(&folder)?;
            std::fs::write(folder.join("hyprland.lock"), lock)?;
        }
        let values = instances(home.path())?;
        assert_eq!(values.as_array().ok_or("Missing instances")?.len(), 2);
        assert_eq!(values[0]["instance"], "hash_100_tail");
        let mut socket = Socket::new(home.path(), "fixture")?;
        let reply = socket.run(vec!["instances".into(), "-j".into()], 1.0, true)?;
        assert_eq!(
            crate::json::parse_value(reply["stdout"].as_str().ok_or("Missing stdout")?)?,
            values
        );
        Ok(())
    }
    #[test]
    fn private_socket_commands_partial_replies_errors_and_limits() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        let folder = home.path().join("hypr/fixture");
        std::fs::create_dir_all(&folder)?;
        let listener = std::os::unix::net::UnixListener::bind(folder.join(".socket.sock"))?;
        let peer = std::thread::spawn(move || -> crate::Result<()> {
            for (command, reply) in [
                ("j/clients", "[]"),
                ("/eval hl.config({})", "ok"),
                ("/reload", "error: rejected"),
            ] {
                let (mut socket, _) = listener.accept()?;
                let mut input = String::new();
                socket.read_to_string(&mut input)?;
                assert_eq!(input, command);
                let split = reply.len() / 2;
                socket.write_all(&reply.as_bytes()[..split])?;
                socket.write_all(&reply.as_bytes()[split..])?;
            }
            Ok(())
        });
        let mut socket = Socket::new(home.path(), "fixture")?;
        assert_eq!(
            socket.run(vec!["clients".into(), "-j".into()], 1.0, true)?["stdout"],
            "[]\n"
        );
        assert_eq!(
            socket.run(vec!["eval".into(), "hl.config({})".into()], 1.0, true)?["stdout"],
            "ok\n"
        );
        assert_eq!(socket.run(vec!["reload".into()], 1.0, false)?["code"], 7);
        peer.join()
            .map_err(|_| "Hyprland socket fixture panicked")??;
        Ok(())
    }
    #[test]
    fn socket_reply_limit_and_total_deadline_are_enforced() -> crate::Result<()> {
        for oversized in [true, false] {
            let home = tempfile::tempdir()?;
            let path = home.path().join("fixture.sock");
            let listener = std::os::unix::net::UnixListener::bind(&path)?;
            let peer = std::thread::spawn(move || -> crate::Result<()> {
                let (mut socket, _) = listener.accept()?;
                let mut command = String::new();
                socket.read_to_string(&mut command)?;
                if oversized {
                    socket.write_all(&[b'x'; 65])?;
                } else {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(())
            });
            let error = exchange(&path, b"/version", Duration::from_millis(20), 64)
                .expect_err("Invalid peer reply accepted");
            if oversized {
                assert_eq!(
                    error.to_string(),
                    "Compositor response exceeded its size limit"
                );
            }
            peer.join()
                .map_err(|_| "Hyprland bounds fixture panicked")??;
        }
        Ok(())
    }
}
