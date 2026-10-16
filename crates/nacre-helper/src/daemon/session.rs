//! Bounded concurrent calls with independent reader/writer tasks on the local runtime.
use super::{
    hub::{failure, Shared},
    socket,
};
use futures_util::FutureExt;
use nacre_protocol::{framing, ErrorCode, Failure, Id, Message, Reply};
use serde_json::Value;
use std::{
    panic::AssertUnwindSafe,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncWrite, BufReader},
    net::{unix::OwnedReadHalf, UnixStream},
    sync::Notify,
    task::JoinSet,
};

struct Client {
    hub: Shared,
    id: u64,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.hub.borrow_mut().remove_client(self.id);
    }
}
pub(super) struct Route {
    pub(super) hub: Shared,
    pub(super) id: Id,
}
impl Drop for Route {
    fn drop(&mut self) {
        self.hub.borrow_mut().cancel_route(&self.id);
    }
}

async fn dispatch(
    hub: Shared,
    method: String,
    params: Value,
    timeout: Option<u32>,
) -> Result<Value, Failure> {
    match method.as_str() {
        "daemon.ping" => Ok(Value::Null),
        "daemon.status" => Ok(hub.borrow().status()),
        method @ ("session.check" | "session.ready" | "session.watch") if !hub.borrow().safe => {
            super::session_policy::dispatch(hub, method, params).await
        }
        method if method.starts_with("ui.") => {
            let (id, receiver) = hub.borrow_mut().forward(method, params, timeout)?;
            let _route = Route { hub, id };
            match receiver.await.map_err(|_| {
                failure(ErrorCode::Unavailable, "UI provider disconnected", true)
                    .dispatch_state(true)
            })? {
                Reply::Ok { ok } => Ok(ok),
                Reply::Err { err } => Err(err.dispatch_state(true)),
            }
        }
        _ => Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    }
}
async fn call(
    hub: Shared,
    id: Id,
    method: String,
    params: Value,
    timeout: Option<u32>,
) -> (Id, Reply) {
    let ms = timeout.unwrap_or_else(|| {
        hub.borrow()
            .methods()
            .get(&method)
            .map_or(1000, |m| m.timeout)
    });
    let ui = method.starts_with("ui.");
    let future = AssertUnwindSafe(dispatch(hub, method, params, timeout)).catch_unwind();
    let result = match tokio::time::timeout(Duration::from_millis(u64::from(ms)), future).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            tracing::error!("Call handler panicked");
            Err(failure(ErrorCode::Internal, "Call handler failed", false))
        }
        Err(_) => Err(if ui {
            failure(ErrorCode::Timeout, "Call deadline reached", true).dispatch_state(true)
        } else {
            failure(ErrorCode::Timeout, "Call deadline reached", true)
        }),
    };
    (
        id,
        match result {
            Ok(ok) => Reply::Ok { ok },
            Err(err) => Reply::Err { err },
        },
    )
}
async fn reader(stream: BufReader<OwnedReadHalf>, hub: Shared, id: u64) -> Result<(), Failure> {
    let mut reader = framing::Reader::new(stream);
    let mut calls = JoinSet::new();
    loop {
        tokio::select! {
            result=calls.join_next(),if !calls.is_empty()=>{
                let (call,reply)=result.ok_or_else(||failure(ErrorCode::Internal,"Call task disappeared",false))?.map_err(|_|failure(ErrorCode::Internal,"Call task failed",false))?;
                hub.borrow_mut().finish_call(id,call,reply)?;
            },
            message=reader.next()=>{
                let Some(message)=message? else {return Ok(());};
                let result=match message {
                    Message::Sub {topics}=>hub.borrow_mut().subscribe(id,&topics,false),
                    Message::Resync {topics}=>hub.borrow_mut().subscribe(id,&topics,true),
                    Message::Unsub {topics}=>{hub.borrow_mut().unsubscribe(id,&topics);Ok(())},
                    Message::Ping {id:ping}=>hub.borrow_mut().enqueue(id,Message::Pong {id:ping}),
                    Message::Pong {..}=>Ok(()),
                    Message::Register {provider:_,methods}=>hub.borrow_mut().register(id,methods),
                    Message::Ret {id:route,reply}=>hub.borrow_mut().answer(id,&route,reply),
                    Message::Call {id:call_id,m,p,timeout}=>{
                        let result=hub.borrow_mut().begin_call(id,&call_id);
                        match result {
                            Ok(())=>{calls.spawn_local(call(hub.clone(),call_id,m,p,timeout));Ok(())},
                            Err(err) if err.code==ErrorCode::Busy=>hub.borrow_mut().enqueue(id,Message::Ret {id:call_id,reply:Reply::Err {err}}),
                            Err(err)=>Err(err),
                        }
                    },
                    _=>Err(failure(ErrorCode::InvalidRequest,"Unexpected client message",false)),
                };
                if let Err(err)=result {hub.borrow_mut().enqueue(id,Message::Error {err})?;}
            },
        }
    }
}
async fn writer<W: AsyncWrite + Unpin>(
    mut stream: W,
    hub: Shared,
    id: u64,
    wake: Rc<Notify>,
) -> Result<(), Failure> {
    loop {
        if !hub.borrow().has_client(id) {
            return Ok(());
        }
        let (packet, deadline) = hub.borrow_mut().next(id, Instant::now());
        if let Some(packet) = packet {
            tokio::time::timeout(
                Duration::from_secs(10),
                framing::write(&mut stream, &packet.message),
            )
            .await
            .map_err(|_| failure(ErrorCode::Timeout, "Client stopped reading", false))??;
            hub.borrow_mut().sent(id, &packet, Instant::now());
        } else if let Some(deadline) = deadline {
            tokio::select! {_=wake.notified()=>{},_=tokio::time::sleep_until(deadline.into())=>{}}
        } else {
            wake.notified().await;
        }
    }
}

pub async fn serve(stream: UnixStream, hub: Shared) -> Result<(), Failure> {
    socket::check_uid(
        stream
            .peer_cred()
            .map_err(|_| failure(ErrorCode::PermissionDenied, "Cannot verify peer", false))?
            .uid(),
    )
    .map_err(|_| failure(ErrorCode::PermissionDenied, "Peer UID mismatch", false))?;
    let (read, mut write) = stream.into_split();
    let reservation = hub.borrow_mut().add_client();
    let (id, wake) = match reservation {
        Ok(client) => client,
        Err(err) => {
            tokio::time::timeout(
                Duration::from_secs(10),
                framing::write(&mut write, &Message::Error { err: err.clone() }),
            )
            .await
            .map_err(|_| failure(ErrorCode::Timeout, "Client stopped reading", false))??;
            return Err(err);
        }
    };
    let _client = Client {
        hub: hub.clone(),
        id,
    };
    let mut header = BufReader::new(read);
    let hello = tokio::time::timeout(Duration::from_secs(5), framing::read(&mut header))
        .await
        .map_err(|_| failure(ErrorCode::Timeout, "Hello deadline reached", false))??;
    let valid = match hello {
        Some(Message::Hello { proto, min, .. }) => {
            min.unwrap_or(proto) <= nacre_protocol::VERSION && proto >= nacre_protocol::MIN_VERSION
        }
        _ => false,
    };
    if !valid {
        let err = failure(
            ErrorCode::UnsupportedVersion,
            "No supported protocol version",
            false,
        );
        tokio::time::timeout(
            Duration::from_secs(10),
            framing::write(&mut write, &Message::Error { err: err.clone() }),
        )
        .await
        .map_err(|_| failure(ErrorCode::Timeout, "Client stopped reading", false))??;
        return Err(err);
    }
    let welcome = hub.borrow().welcome();
    tokio::time::timeout(
        Duration::from_secs(10),
        framing::write(&mut write, &welcome),
    )
    .await
    .map_err(|_| failure(ErrorCode::Timeout, "Client stopped reading", false))??;
    // Preserve bytes read ahead with hello; a pipelined sub/call belongs to this connection.
    let read = reader(header, hub.clone(), id);
    tokio::select! {result=read=>result,result=writer(write,hub,id,wake)=>result}
}

#[cfg(test)]
mod tests {
    use super::super::hub::Hub;
    use super::*;
    use serde_json::json;
    use tokio::io::AsyncWriteExt;
    use tokio::net::unix::OwnedWriteHalf;
    fn fixture() -> Shared {
        let mut hub = Hub::new(
            "00000000-0000-4000-8000-000000000001".into(),
            false,
            Duration::from_millis(16),
        );
        hub.publish("compositor", json!({"windows":{},"order":[]}))
            .unwrap();
        hub.shared()
    }
    fn transcript(name: &str) -> Vec<Message> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures");
        std::fs::read_to_string(root.join(format!("{name}.jsonl")))
            .unwrap()
            .lines()
            .map(|line| framing::decode(line.as_bytes()).unwrap())
            .collect()
    }
    async fn receive(reader: &mut BufReader<OwnedReadHalf>) -> Message {
        tokio::time::timeout(Duration::from_secs(10), framing::read(reader))
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }
    async fn connect(
        hub: Shared,
    ) -> (
        OwnedWriteHalf,
        BufReader<OwnedReadHalf>,
        tokio::task::JoinHandle<Result<(), Failure>>,
    ) {
        let (server, client) = UnixStream::pair().unwrap();
        let task = tokio::task::spawn_local(serve(server, hub));
        let (read, mut write) = client.into_split();
        let mut read = BufReader::new(read);
        framing::write(&mut write, &transcript("handshake")[0])
            .await
            .unwrap();
        assert_eq!(receive(&mut read).await, transcript("handshake")[1]);
        (write, read, task)
    }
    #[tokio::test(flavor = "current_thread")]
    async fn actual_session_replays_hello_pipeline_calls_and_resync_without_losing_read_ahead() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let hub = fixture();
                let (server, client) = UnixStream::pair().unwrap();
                let task = tokio::task::spawn_local(serve(server, hub.clone()));
                let (read, mut write) = client.into_split();
                let mut read = BufReader::new(read);
                let mut wire = framing::encode(&transcript("handshake")[0]).unwrap();
                wire.extend(
                    framing::encode(&Message::Call {
                        id: Id::Integer(7),
                        m: "daemon.ping".into(),
                        p: json!({}),
                        timeout: None,
                    })
                    .unwrap(),
                );
                write.write_all(&wire).await.unwrap();
                assert_eq!(receive(&mut read).await, transcript("handshake")[1]);
                assert_eq!(
                    receive(&mut read).await,
                    Message::Ret {
                        id: Id::Integer(7),
                        reply: Reply::Ok { ok: Value::Null }
                    }
                );
                let calls = transcript("calls");
                framing::write(&mut write, &calls[2]).await.unwrap();
                assert_eq!(receive(&mut read).await, calls[3]);
                framing::write(
                    &mut write,
                    &Message::Sub {
                        topics: vec!["compositor".into()],
                    },
                )
                .await
                .unwrap();
                assert_eq!(receive(&mut read).await, transcript("state")[1]);
                hub.borrow_mut()
                    .publish(
                        "compositor",
                        json!({"windows":{"w1":{"id":"w1"}},"order":["w1"]}),
                    )
                    .unwrap();
                framing::write(
                    &mut write,
                    &Message::Resync {
                        topics: vec!["compositor".into()],
                    },
                )
                .await
                .unwrap();
                loop {
                    if let Message::Snap { seq, data, .. } = receive(&mut read).await {
                        assert_eq!(seq, 1);
                        assert_eq!(data["order"], json!(["w1"]));
                        break;
                    }
                }
                drop(write);
                drop(read);
                assert!(task.await.unwrap().is_ok());
                assert_eq!(hub.borrow().status()["clients"], 0);
            })
            .await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn provider_calls_are_correlated_and_do_not_block_other_calls_or_survive_provider_loss() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let hub = fixture();
                let (mut provider, mut provider_read, provider_task) = connect(hub.clone()).await;
                let ui = transcript("ui");
                framing::write(&mut provider, &ui[0]).await.unwrap();
                framing::write(
                    &mut provider,
                    &Message::Ping {
                        id: Id::Text("registered".into()),
                    },
                )
                .await
                .unwrap();
                assert!(matches!(
                    receive(&mut provider_read).await,
                    Message::Pong { .. }
                ));
                let (server, client) = UnixStream::pair().unwrap();
                let caller_task = tokio::task::spawn_local(serve(server, hub.clone()));
                let (caller_read, mut caller) = client.into_split();
                let mut caller_read = BufReader::new(caller_read);
                framing::write(&mut caller, &transcript("handshake")[0])
                    .await
                    .unwrap();
                assert!(matches!(
                    receive(&mut caller_read).await,
                    Message::Welcome { .. }
                ));
                framing::write(&mut caller, &ui[1]).await.unwrap();
                assert_eq!(receive(&mut provider_read).await, ui[2]);
                framing::write(
                    &mut caller,
                    &Message::Call {
                        id: Id::Integer(2),
                        m: "daemon.ping".into(),
                        p: json!({}),
                        timeout: None,
                    },
                )
                .await
                .unwrap();
                assert_eq!(
                    receive(&mut caller_read).await,
                    Message::Ret {
                        id: Id::Integer(2),
                        reply: Reply::Ok { ok: Value::Null }
                    }
                );
                framing::write(&mut provider, &ui[3]).await.unwrap();
                assert_eq!(receive(&mut caller_read).await, ui[4]);
                framing::write(
                    &mut caller,
                    &Message::Call {
                        id: Id::Integer(3),
                        m: "ui.dispatch".into(),
                        p: json!({}),
                        timeout: None,
                    },
                )
                .await
                .unwrap();
                let forwarded = receive(&mut provider_read).await;
                let Message::Call { id: forwarded, .. } = forwarded else {
                    panic!("Missing provider call");
                };
                framing::write(
                    &mut provider,
                    &Message::Ret {
                        id: forwarded,
                        reply: Reply::Err {
                            err: failure(ErrorCode::Unavailable, "Synthetic failure", true)
                                .dispatch_state(false),
                        },
                    },
                )
                .await
                .unwrap();
                assert!(matches!(
                    receive(&mut caller_read).await,
                    Message::Ret {
                        reply: Reply::Err {
                            err: Failure {
                                dispatched: Some(true),
                                ..
                            }
                        },
                        ..
                    }
                ));
                framing::write(
                    &mut caller,
                    &Message::Call {
                        id: Id::Text("deadline".into()),
                        m: "ui.dispatch".into(),
                        p: json!({}),
                        timeout: Some(1),
                    },
                )
                .await
                .unwrap();
                assert!(matches!(
                    receive(&mut provider_read).await,
                    Message::Call { .. }
                ));
                assert_eq!(receive(&mut caller_read).await, transcript("calls")[5]);
                framing::write(
                    &mut caller,
                    &Message::Call {
                        id: Id::Text("lost".into()),
                        m: "ui.dispatch".into(),
                        p: json!({}),
                        timeout: None,
                    },
                )
                .await
                .unwrap();
                assert!(matches!(
                    receive(&mut provider_read).await,
                    Message::Call { .. }
                ));
                drop(provider);
                drop(provider_read);
                assert!(provider_task.await.unwrap().is_ok());
                assert!(matches!(
                    receive(&mut caller_read).await,
                    Message::Ret {
                        reply: Reply::Err {
                            err: Failure {
                                code: ErrorCode::Unavailable,
                                ..
                            }
                        },
                        ..
                    }
                ));
                drop(caller);
                drop(caller_read);
                assert!(caller_task.await.unwrap().is_ok());
            })
            .await;
    }
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn stalled_writer_is_disconnected_after_ten_seconds_without_polling() {
        let hub = fixture();
        let (id, wake) = hub.borrow_mut().add_client().unwrap();
        for n in 0..4 {
            hub.borrow_mut()
                .enqueue(
                    id,
                    Message::Ret {
                        id: Id::Integer(n),
                        reply: Reply::Ok {
                            ok: json!("x".repeat(500_000)),
                        },
                    },
                )
                .unwrap();
        }
        let (output, _unread) = tokio::io::duplex(64);
        assert_eq!(
            writer(output, hub, id, wake).await.unwrap_err().code,
            ErrorCode::Timeout
        );
    }
}
