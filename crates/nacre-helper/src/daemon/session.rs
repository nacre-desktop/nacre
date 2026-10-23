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
    client: u64,
    method: String,
    params: Value,
    timeout: Option<u32>,
    transfer: super::helpers::Transfer,
) -> Result<Value, Failure> {
    match method.as_str() {
        method if method.starts_with("notifications.") && !hub.borrow().safe => {
            super::notifications::dispatch(hub, client, method, params, timeout).await
        }
        method if method.starts_with("player.") && !hub.borrow().safe => {
            super::dashboard_player::dispatch(hub, method, params).await
        }
        "daemon.ping" => Ok(Value::Null),
        method @ ("brain.open" | "brain.capture") if !hub.borrow().safe => {
            super::brain::dispatch(hub, method, params)
        }
        method @ ("tray.activate"
        | "tray.secondary_activate"
        | "tray.context_menu"
        | "tray.scroll"
        | "brightness.set"
        | "brightness.adjust"
        | "keyboard_light.adjust") => super::bar_services::dispatch(hub, method, params).await,
        method if super::control_center::handles(method) => {
            super::control_center::dispatch(hub, method, params).await
        }
        method @ ("audio.adjust_volume" | "audio.toggle_mute" | "bluetooth.set_enabled") => {
            super::pill_controls::dispatch(hub, method, params).await
        }
        method if method.starts_with("ai.chat.") && !hub.borrow().safe => {
            super::ai_chat::dispatch(hub, method, params).await
        }
        "i18n.catalog" if !hub.borrow().safe => super::i18n::catalog(&hub, params),
        method if super::dashboard_extras::handles(method) && !hub.borrow().safe => {
            super::dashboard_extras::dispatch(hub, method, params).await
        }
        "wallpaper.activate" if !hub.borrow().safe => {
            super::active_wallpaper::activate(hub, params)
        }
        method @ ("workspace.activate" | "workspace.scroll" | "window.focus") => {
            super::shell_actions::compositor(hub, method, params).await
        }
        "session.request" => super::shell_actions::session(hub, params).await,
        method @ ("network.set_wifi" | "network.connect" | "network.disconnect"
        | "network.forget") => super::network::dispatch(hub, method, params, timeout).await,
        method @ ("bluetooth.pair" | "bluetooth.pair.respond" | "bluetooth.pair.cancel")
            if !hub.borrow().safe =>
        {
            super::bluetooth::dispatch(hub, method, params).await
        }
        method @ ("settings.get" | "settings.set" | "settings.publish" | "settings.provider"
        | "settings.desktop") => super::settings::dispatch(hub, method, params).await,
        method @ ("niri.source" | "niri.action") => {
            super::niri_actions::dispatch(hub, method, params).await
        }
        method @ ("helper.execute" | "helper.transfer")
            if !hub.borrow().safe && hub.borrow().services.contains_key("helper-lanes") =>
        {
            super::helpers::dispatch(hub, transfer, method, params).await
        }
        "daemon.status" => Ok(hub.borrow().status()),
        method @ ("menu.query" | "menu.groups" | "menu.activate" | "menu.preferences")
            if !hub.borrow().safe =>
        {
            super::menu::dispatch(hub, method, params, timeout).await
        }
        method @ ("session.check" | "session.ready" | "session.watch") if !hub.borrow().safe => {
            super::session_policy::dispatch(hub, method, params).await
        }
        "niri.widths.configure" if !hub.borrow().safe => super::widths::dispatch(hub, params).await,
        nacre_protocol::menu::UI_ACTIVATE => Err(failure(
            ErrorCode::PermissionDenied,
            "Use menu.activate with an engine-issued ticket",
            false,
        )
        .dispatch_state(false)),
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
    client: u64,
    id: Id,
    method: String,
    params: Value,
    timeout: Option<u32>,
    transfer: super::helpers::Transfer,
) -> (Id, Reply) {
    let started = Instant::now();
    let ms = timeout.unwrap_or_else(|| {
        hub.borrow()
            .methods()
            .get(&method)
            .map_or(1000, |m| m.timeout)
    });
    let ui = method.starts_with("ui.");
    let network = matches!(
        method.as_str(),
        "network.set_wifi" | "network.connect" | "network.disconnect" | "network.forget"
    );
    let helper = matches!(
        method.as_str(),
        "helper.execute"
            | "network.set_wifi"
            | "network.connect"
            | "network.disconnect"
            | "network.forget"
            | "bluetooth.pair"
            | "bluetooth.pair.respond"
            | "bluetooth.pair.cancel"
            | "niri.action"
            | "settings.set"
            | "settings.publish"
            | "settings.provider"
            | "settings.desktop"
            | "workspace.activate"
            | "workspace.scroll"
            | "window.focus"
            | "session.request"
            | "tray.activate"
            | "tray.secondary_activate"
            | "tray.context_menu"
            | "tray.scroll"
            | "brightness.set"
            | "brightness.adjust"
            | "keyboard_light.adjust"
            | "audio.adjust_volume"
            | "audio.toggle_mute"
            | "bluetooth.set_enabled"
    );
    let helper = helper || super::control_center::handles(&method);
    let future = AssertUnwindSafe(dispatch(
        hub,
        client,
        method.clone(),
        params,
        timeout,
        transfer,
    ))
    .catch_unwind();
    let result = match tokio::time::timeout(Duration::from_millis(u64::from(ms)), future).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
            tracing::error!("Call handler panicked");
            Err(if helper {
                failure(ErrorCode::Internal, "Call handler failed", false).dispatch_state(true)
            } else {
                failure(ErrorCode::Internal, "Call handler failed", false)
            })
        }
        Err(_) => {
            if ui {
                tracing::info!(
                    call = ?id,
                    method,
                    elapsed_ms = started.elapsed().as_millis(),
                    deadline_ms = ms,
                    "UI call timed out; action may still complete"
                );
            }
            Err(if network {
                failure(
                    ErrorCode::Unavailable,
                    "Network operation unavailable or outcome uncertain",
                    true,
                )
                .dispatch_state(true)
            } else if helper {
                failure(ErrorCode::Timeout, "Call deadline reached", false).dispatch_state(true)
            } else if ui {
                failure(
                    ErrorCode::Timeout,
                    "UI call timed out; action may still complete",
                    false,
                )
                .dispatch_state(true)
            } else {
                failure(ErrorCode::Timeout, "Call deadline reached", true)
            })
        }
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
    let transfer = super::helpers::Transfer::default();
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
                            Ok(())=>{calls.spawn_local(call(hub.clone(),id,call_id,m,p,timeout,transfer.clone()));Ok(())},
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

    #[tokio::test(flavor = "current_thread")]
    async fn semantic_mutations_refuse_safe_mode_and_unconfirmed_session_actions() {
        let hub = fixture();
        for action in ["logout", "suspend", "reboot", "shutdown"] {
            let error = dispatch(
                hub.clone(),
                0,
                "session.request".into(),
                json!({"action":action}),
                None,
                super::super::helpers::Transfer::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::PermissionDenied);
            assert_eq!(error.dispatched, Some(false));
        }
        hub.borrow_mut().safe = true;
        for (method, params) in [
            (
                "workspace.activate",
                json!({"output":"DP-1","workspace":"1"}),
            ),
            (
                "workspace.scroll",
                json!({"output":"DP-1","direction":"next"}),
            ),
            ("window.focus", json!({"output":"DP-1","window":"1"})),
            (
                "session.request",
                json!({"action":"shutdown","confirmed":true}),
            ),
            ("tray.activate", json!({"item":"sni:old"})),
            ("brightness.adjust", json!({"output":"DP-1","delta":5})),
            ("keyboard_light.adjust", json!({"step":"up"})),
            ("audio.toggle_mute", json!({})),
            ("bluetooth.set_enabled", json!({"enabled":true})),
            ("audio.set_volume", json!({"target":"sink","value":0.5})),
            ("audio.set_mute", json!({"target":"sink","muted":true})),
            ("keyboard_light.set", json!({"level":1})),
            ("power.set_profile", json!({"profile":"balanced"})),
            ("airplane.set", json!({"enabled":true})),
            ("night_light.set", json!({"enabled":true})),
            ("stay_awake.set", json!({"enabled":true})),
            ("recorder.start", json!({"mode":"region"})),
            ("recorder.stop", json!({})),
            ("screenshot.capture", json!({"mode":"region"})),
            ("color_picker.pick", json!({})),
            ("wallpaper.next", json!({})),
            ("appearance.set_mode", json!({"mode":"dark"})),
        ] {
            let error = dispatch(
                hub.clone(),
                0,
                method.into(),
                params,
                None,
                super::super::helpers::Transfer::default(),
            )
            .await
            .unwrap_err();
            assert!(
                matches!(
                    error.code,
                    ErrorCode::PermissionDenied | ErrorCode::Unavailable
                ),
                "{method}"
            );
            assert_ne!(error.dispatched, Some(true), "{method}");
            assert!(!hub.borrow().methods().contains_key(method), "{method}");
        }
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
    async fn selected_menu_rows_cannot_be_supplied_by_a_public_caller() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let hub = fixture();
                let (mut caller, mut reader, caller_task) = connect(hub.clone()).await;
                let (provider, _) = hub.borrow_mut().add_client().unwrap();
                hub.borrow_mut()
                    .register(
                        provider,
                        std::collections::BTreeMap::from([
                            ("ui.dispatch".into(), 1),
                            (nacre_protocol::menu::UI_ACTIVATE.into(), 1),
                        ]),
                    )
                    .unwrap();
                assert!(hub.borrow().methods().contains_key("ui.dispatch"));
                assert!(!hub
                    .borrow()
                    .methods()
                    .contains_key(nacre_protocol::menu::UI_ACTIVATE));
                let selected = json!({"row":{"id":"fixture","kind":"action","name":"Fixture","operation":"command","arguments":["fixture-app","$(literal)"],"disabled":false},"catalog_rev":7});
                framing::write(
                    &mut caller,
                    &Message::Call {
                        id: Id::Integer(88),
                        m: nacre_protocol::menu::UI_ACTIVATE.into(),
                        p: selected.clone(),
                        timeout: Some(1000),
                    },
                )
                .await
                .unwrap();
                let Message::Ret { id, reply: Reply::Err { err } } = receive(&mut reader).await else {
                    panic!("Direct selected-row call was not refused");
                };
                assert_eq!(id, Id::Integer(88));
                assert_eq!(err.code, ErrorCode::PermissionDenied);
                assert_eq!(err.dispatched, Some(false));
                assert!(!err.retry);
                assert!(hub.borrow_mut().next(provider, Instant::now()).0.is_none());
                // The same route remains usable internally after engine ticket
                // validation. The provider ACK is independent of caller I/O.
                let (route, reply) = hub
                    .borrow_mut()
                    .forward(nacre_protocol::menu::UI_ACTIVATE, selected.clone(), None)
                    .unwrap();
                let packet = hub.borrow_mut().next(provider, Instant::now()).0.unwrap();
                assert!(matches!(packet.message,Message::Call{ref id,ref m,ref p,..}
                    if id==&route && m==nacre_protocol::menu::UI_ACTIVATE && p==&selected));
                hub.borrow_mut()
                    .answer(provider, &route, Reply::Ok { ok: json!({"keep_open":false}) })
                    .unwrap();
                assert!(matches!(reply.await.unwrap(), Reply::Ok { .. }));
                caller.shutdown().await.unwrap();
                caller_task.abort();
            })
            .await;
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
                let Message::Call { id: late_route, .. } = receive(&mut provider_read).await else {
                    panic!("Timed out UI call was not forwarded exactly once");
                };
                assert_eq!(receive(&mut caller_read).await, transcript("calls")[5]);
                framing::write(
                    &mut provider,
                    &Message::Ret {
                        id: late_route,
                        reply: Reply::Ok { ok: json!({}) },
                    },
                )
                .await
                .unwrap();
                framing::write(
                    &mut provider,
                    &Message::Ping {
                        id: Id::Text("after-late-reply".into()),
                    },
                )
                .await
                .unwrap();
                assert_eq!(
                    receive(&mut provider_read).await,
                    Message::Pong {
                        id: Id::Text("after-late-reply".into()),
                    }
                );
                framing::write(
                    &mut caller,
                    &Message::Call {
                        id: Id::Text("after-late-reply".into()),
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
                        id: Id::Text("after-late-reply".into()),
                        reply: Reply::Ok { ok: Value::Null },
                    }
                );
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
