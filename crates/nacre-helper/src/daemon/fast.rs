//! Independent one-shot request lane. It never enters the producer runtime
//! before acknowledging a bounded hardware call.
use super::socket;
use nacre_protocol::{framing, ErrorCode, Failure, Message, Reply, MAX_FRAME};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

pub(super) struct Server {
    stop: Arc<AtomicBool>,
    path: PathBuf,
    thread: Option<JoinHandle<crate::Result<()>>>,
}

impl Server {
    pub(super) fn start(
        mut listener: socket::FastListener,
        requests: mpsc::Sender<Message>,
        audio_steps: SyncSender<super::pill_controls::AudioStepCommand>,
        safe_mode: bool,
        audio_available: bool,
    ) -> crate::Result<Self> {
        let path = listener.path.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("nacre-fast-daemon".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let listener = listener.take()?;
                run(
                    listener,
                    thread_stop,
                    requests,
                    audio_steps,
                    safe_mode,
                    audio_available,
                )
            })?;
        Ok(Self {
            stop,
            path,
            thread: Some(thread),
        })
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake the blocking accept without signalling or restarting any other
        // process. This short-lived connection is owned by this server.
        let _ = UnixStream::connect(&self.path);
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "Fast request lane stopped with an error"),
                Err(_) => tracing::warn!("Fast request lane panicked"),
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    listener: UnixListener,
    stop: Arc<AtomicBool>,
    requests: mpsc::Sender<Message>,
    audio_steps: SyncSender<super::pill_controls::AudioStepCommand>,
    safe_mode: bool,
    audio_available: bool,
) -> crate::Result<()> {
    loop {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
        let response = handle(
            &mut stream,
            &requests,
            &audio_steps,
            safe_mode,
            audio_available,
        );
        if let Err(error) = response {
            tracing::debug!(%error, "One-shot daemon request failed");
        }
    }
    Ok(())
}

fn handle(
    stream: &mut UnixStream,
    requests: &mpsc::Sender<Message>,
    audio_steps: &SyncSender<super::pill_controls::AudioStepCommand>,
    safe_mode: bool,
    audio_available: bool,
) -> crate::Result<()> {
    let started = Instant::now();
    let peer = nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::PeerCredentials)?;
    socket::check_uid(peer.uid())?;
    let frame = read_frame(stream)?;
    let message = framing::decode(&frame)?;
    let (id, reply, method) = match message {
        Message::FastCall { id, m, .. } if m == "daemon.ping" => (
            id,
            Reply::Ok {
                ok: serde_json::Value::Null,
            },
            m,
        ),
        request @ Message::FastCall { .. } => {
            let Message::FastCall { id, m, .. } = &request else {
                unreachable!()
            };
            let id = id.clone();
            let method = m.clone();
            let reject = if safe_mode && m != "daemon.ping" {
                Some(Failure::new(
                    ErrorCode::PermissionDenied,
                    "Hardware actions are disabled in safe mode",
                    false,
                ))
            } else if m == "audio.step" && !audio_available {
                Some(Failure::new(
                    ErrorCode::Unavailable,
                    "Audio backend is unavailable",
                    false,
                ))
            } else {
                None
            };
            if let Some(err) = reject {
                (id, Reply::Err { err }, method)
            } else if method == "audio.step" {
                let Message::FastCall { p, .. } = &request else {
                    unreachable!()
                };
                let step =
                    serde_json::from_value::<nacre_protocol::pill_controls::VolumeStep>(p.clone())
                        .map_err(|_| {
                            Failure::new(ErrorCode::Validation, "Invalid audio step", false)
                        })?;
                let delta = match step.direction {
                    nacre_protocol::pill_controls::VolumeDirection::Up => 0.05,
                    nacre_protocol::pill_controls::VolumeDirection::Down => -0.05,
                };
                match audio_steps.try_send(super::pill_controls::AudioStepCommand::Apply(
                    super::pill_controls::FastAudioStep {
                        delta,
                        key_ms: step.key_ms,
                    },
                )) {
                    Ok(()) => (
                        id,
                        Reply::Ok {
                            ok: json!({"changed":true}),
                        },
                        method,
                    ),
                    Err(std::sync::mpsc::TrySendError::Full(_)) => (
                        id,
                        Reply::Err {
                            err: Failure::new(ErrorCode::Busy, "Audio step queue is full", true),
                        },
                        method,
                    ),
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => (
                        id,
                        Reply::Err {
                            err: Failure::new(
                                ErrorCode::Unavailable,
                                "Audio step worker is stopping",
                                true,
                            ),
                        },
                        method,
                    ),
                }
            } else {
                match requests.try_send(request) {
                    Ok(()) => (
                        id,
                        Reply::Ok {
                            ok: json!({"changed":true}),
                        },
                        method,
                    ),
                    Err(mpsc::error::TrySendError::Full(_)) => (
                        id,
                        Reply::Err {
                            err: Failure::new(ErrorCode::Busy, "Fast request queue is full", true),
                        },
                        method,
                    ),
                    Err(mpsc::error::TrySendError::Closed(_)) => (
                        id,
                        Reply::Err {
                            err: Failure::new(
                                ErrorCode::Unavailable,
                                "Fast request lane is stopping",
                                true,
                            ),
                        },
                        method,
                    ),
                }
            }
        }
        _ => {
            write_message(
                stream,
                &Message::Error {
                    err: Failure::new(ErrorCode::InvalidRequest, "Expected one-shot call", false),
                },
            )?;
            return Ok(());
        }
    };
    write_message(stream, &Message::Ret { id, reply })?;
    if std::env::var_os("NACRE_FAST_TRACE").is_some() {
        tracing::info!(
            method,
            server_us = started.elapsed().as_micros(),
            "FAST_CALL"
        );
    }
    Ok(())
}

fn read_frame(stream: &mut UnixStream) -> crate::Result<Vec<u8>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut frame = Vec::with_capacity(256);
    let bytes = reader
        .by_ref()
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut frame)?;
    if bytes == 0 || bytes > MAX_FRAME || frame.last() != Some(&b'\n') {
        return Err("Invalid one-shot request frame".into());
    }
    frame.pop();
    Ok(frame)
}

fn write_message(stream: &mut UnixStream, message: &Message) -> crate::Result<()> {
    stream.write_all(&framing::encode(message)?)?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nacre_protocol::Id;
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::{fs::PermissionsExt, net::UnixListener as StdListener},
    };

    fn exchange(path: &std::path::Path, request: &Message) -> Message {
        let mut stream = UnixStream::connect(path).unwrap();
        let peer = getsockopt(&stream, PeerCredentials).unwrap();
        assert_eq!(peer.uid(), nix::unistd::getuid().as_raw());
        stream
            .write_all(&framing::encode(request).unwrap())
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        framing::decode(line.trim_end().as_bytes()).unwrap()
    }

    #[test]
    fn one_shot_ping_is_direct_and_hardware_calls_are_queued() -> crate::Result<()> {
        let temp = tempfile::tempdir()?;
        let directory = temp.path().join("nacre");
        std::fs::create_dir(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let main_path = directory.join("daemon.sock");
        let _main = StdListener::bind(&main_path)?;
        std::fs::set_permissions(&main_path, std::fs::Permissions::from_mode(0o600))?;
        let fast_path = socket::fast_path_for(&main_path)?;
        let listener = socket::FastListener::acquire(&main_path)?;
        let (send, mut receive) = mpsc::channel(4);
        let (audio_send, audio_receive) = std::sync::mpsc::sync_channel(4);
        let server = Server::start(listener, send, audio_send, false, true)?;

        assert_eq!(
            exchange(
                &fast_path,
                &Message::FastCall {
                    proto: nacre_protocol::VERSION,
                    id: Id::Integer(1),
                    m: "daemon.ping".into(),
                    p: json!({}),
                },
            ),
            Message::Ret {
                id: Id::Integer(1),
                reply: Reply::Ok {
                    ok: serde_json::Value::Null
                },
            }
        );
        let queued = Message::FastCall {
            proto: nacre_protocol::VERSION,
            id: Id::Integer(2),
            m: "brightness.adjust".into(),
            p: json!({"delta":5}),
        };
        assert_eq!(
            exchange(&fast_path, &queued),
            Message::Ret {
                id: Id::Integer(2),
                reply: Reply::Ok {
                    ok: json!({"changed":true})
                },
            }
        );
        assert_eq!(receive.blocking_recv(), Some(queued));
        let audio = Message::FastCall {
            proto: nacre_protocol::VERSION,
            id: Id::Integer(3),
            m: "audio.step".into(),
            p: json!({"direction":"up","key_ms":99}),
        };
        assert_eq!(
            exchange(&fast_path, &audio),
            Message::Ret {
                id: Id::Integer(3),
                reply: Reply::Ok {
                    ok: json!({"changed":true})
                },
            }
        );
        let Ok(crate::daemon::pill_controls::AudioStepCommand::Apply(queued)) = audio_receive.try_recv()
        else {
            panic!("queued audio step");
        };
        assert_eq!(queued.delta, 0.05);
        assert_eq!(queued.key_ms, Some(99));
        drop(server);
        assert!(!fast_path.exists());
        Ok(())
    }
}
