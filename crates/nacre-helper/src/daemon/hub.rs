//! One owner for topic state and bounded per-client delivery. No timer polls.
use nacre_protocol::{
    patch::{self, Diff},
    ErrorCode, Failure, Id, Message, Method, Reply, MAX_CALLS, MAX_CLIENTS, MAX_EVENTS,
    MAX_SAFE_INTEGER, MAX_SUBSCRIPTIONS,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, oneshot, Notify};

const QUEUE_BYTES: usize = 2 * 1024 * 1024;
const BASELINE_BYTES: usize = 8 * 1024 * 1024;
const TOPIC_BYTES: usize = 32 * 1024 * 1024;
pub type Shared = Rc<std::cell::RefCell<Hub>>;
#[derive(Clone, Debug)]
pub enum Notice {
    Changed {
        topic: String,
        available: Option<bool>,
    },
    Event {
        topic: String,
        name: String,
        data: Value,
    },
    Resync(String),
}
pub fn failure(code: ErrorCode, message: &str, retry: bool) -> Failure {
    Failure::new(code, message, retry)
}
struct Topic {
    seq: u64,
    data: Value,
    bytes: usize,
}
struct Subscription {
    delivered: Option<Rc<Topic>>,
    considered: Option<u64>,
    generation: u64,
    force: bool,
    next: Instant,
}
struct Client {
    pub wake: Rc<Notify>,
    topics: BTreeMap<String, Subscription>,
    generation: u64,
    events: VecDeque<Event>,
    event_bytes: usize,
    lost: BTreeMap<String, (u64, u64)>,
    replies: VecDeque<(Message, usize)>,
    reply_bytes: usize,
    inflight: BTreeSet<Id>,
}
struct Event {
    message: Message,
    bytes: usize,
    after: Vec<(String, u64)>,
}
struct PendingRoute {
    provider: u64,
    reply: oneshot::Sender<Reply>,
    method: String,
    started: Instant,
    deadline_ms: u32,
}
pub struct Packet {
    pub message: Message,
    commit: Option<(String, Rc<Topic>, u64)>,
}
pub struct Hub {
    pub boot: String,
    pub safe: bool,
    pub frame: Duration,
    topics: BTreeMap<String, Rc<Topic>>,
    clients: BTreeMap<u64, Client>,
    next_client: u64,
    event_seq: BTreeMap<String, u64>,
    pub services: BTreeMap<String, Value>,
    provider: Option<(u64, BTreeMap<String, u16>)>,
    routes: BTreeMap<Id, PendingRoute>,
    // One contiguous issued range per provider keeps retired-ID memory bounded.
    // An issued ID without an active route has completed or been canceled;
    // accepting its late/duplicate reply cannot affect another call.
    issued_provider_routes: Option<(u64, u64, u64)>,
    next_route: u64,
    internal: broadcast::Sender<Notice>,
    /// Lazy producers wait on "has any client subscribed to this topic".
    interest: BTreeMap<String, tokio::sync::watch::Sender<bool>>,
    pub network: Option<super::network::Requests>,
    pub bluetooth: Option<Rc<super::bluetooth::Service>>,
    pub notifications: Option<super::notifications::Requests>,
    pub calendar: Option<Rc<Notify>>,
    pub player: Option<super::dashboard_player::Requests>,
    pub session: Option<super::session_policy::Requests>,
    pub menu: Option<super::menu::Requests>,
    pub dashboard: Option<super::dashboard_extras::Requests>,
    pub widths: Option<super::widths::Requests>,
    pub providers: super::menu_worker::Pool,
    pub helper_lanes: [Rc<tokio::sync::Mutex<()>>; 6],
    pub niri_width_source: super::widths::Source,
    pub compositor: Option<super::compositor_control::Control>,
    pub settings: Option<Rc<super::settings::Service>>,
    pub ai_chat: Option<Rc<super::ai_chat::Host>>,
}
impl Hub {
    pub fn new(boot: String, safe: bool, frame: Duration) -> Self {
        let (internal, _) = broadcast::channel(MAX_EVENTS);
        Self {
            boot,
            safe,
            frame,
            topics: BTreeMap::new(),
            clients: BTreeMap::new(),
            next_client: 0,
            event_seq: BTreeMap::new(),
            services: BTreeMap::new(),
            provider: None,
            routes: BTreeMap::new(),
            issued_provider_routes: None,
            next_route: 0,
            internal,
            interest: BTreeMap::new(),
            network: None,
            bluetooth: None,
            notifications: None,
            calendar: None,
            player: None,
            session: None,
            menu: None,
            dashboard: None,
            widths: None,
            providers: super::menu_worker::Pool::default(),
            helper_lanes: std::array::from_fn(|_| Rc::new(tokio::sync::Mutex::new(()))),
            niri_width_source: super::widths::Source::default(),
            compositor: None,
            settings: None,
            ai_chat: None,
        }
    }
    pub fn shared(self) -> Shared {
        Rc::new(std::cell::RefCell::new(self))
    }
    /// True while at least one connected client subscribes to `topic`.
    pub fn watch_interest(&mut self, topic: &str) -> tokio::sync::watch::Receiver<bool> {
        let wanted = self.clients.values().any(|c| c.topics.contains_key(topic));
        self.interest
            .entry(topic.to_owned())
            .or_insert_with(|| tokio::sync::watch::channel(wanted).0)
            .subscribe()
    }
    fn refresh_interest(&self) {
        for (topic, sender) in &self.interest {
            let wanted = self.clients.values().any(|c| c.topics.contains_key(topic));
            sender.send_if_modified(|current| std::mem::replace(current, wanted) != wanted);
        }
    }
    pub fn observe(&self) -> broadcast::Receiver<Notice> {
        self.internal.subscribe()
    }
    fn notify(&self, notice: Notice) {
        if self.internal.receiver_count() > 0 && self.internal.send(notice).is_err() {
            tracing::debug!("Internal topic observers ended");
        }
    }
    pub fn snapshot(&self, name: &str) -> Option<Value> {
        self.topics.get(name).map(|topic| topic.data.clone())
    }
    /// Synchronous service checks should not copy an entire desktop snapshot.
    pub(crate) fn snapshot_ref(&self, name: &str) -> Option<&Value> {
        self.topics.get(name).map(|topic| &topic.data)
    }
    pub fn publish(&mut self, topic: &str, data: Value) -> Result<(), Failure> {
        self.publish_state(topic, data, false)
    }
    /// Some processed transitions (resident actions/inhibitors) change private
    /// state only. Allocate their state revision before publishing the event.
    pub fn publish_transition(&mut self, topic: &str, data: Value) -> Result<(), Failure> {
        self.publish_state(topic, data, true)
    }
    pub fn topic_seq(&self, topic: &str) -> Option<u64> {
        self.topics.get(topic).map(|value| value.seq)
    }
    pub fn is_ui_provider(&self, id: u64) -> bool {
        self.provider
            .as_ref()
            .is_some_and(|provider| provider.0 == id)
    }
    fn publish_state(&mut self, topic: &str, data: Value, force: bool) -> Result<(), Failure> {
        if !self.topics.contains_key(topic) && self.topics.len() >= MAX_SUBSCRIPTIONS {
            return Err(failure(ErrorCode::Busy, "Topic limit reached", true));
        }
        let bytes = serde_json::to_vec(&data)
            .map_err(|_| failure(ErrorCode::Internal, "Topic encoding failed", false))?
            .len();
        let old = self.topics.get(topic);
        if !force && old.is_some_and(|old| old.data == data) {
            return Ok(());
        }
        let seq = old.map_or(Ok(0), |old| {
            old.seq
                .checked_add(1)
                .filter(|s| *s <= MAX_SAFE_INTEGER)
                .ok_or_else(|| failure(ErrorCode::Internal, "Topic sequence exhausted", false))
        })?;
        nacre_protocol::framing::encode(&Message::Snap {
            topic: topic.into(),
            seq,
            data: data.clone(),
        })?;
        if self.topics.values().map(|t| t.bytes).sum::<usize>() - old.map_or(0, |t| t.bytes) + bytes
            > TOPIC_BYTES
        {
            return Err(failure(
                ErrorCode::TooLarge,
                "Topic memory limit reached",
                false,
            ));
        }
        self.topics
            .insert(topic.into(), Rc::new(Topic { seq, data, bytes }));
        let over: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| {
                client
                    .topics
                    .iter()
                    .map(|(name, subscription)| {
                        let current = self.topics.get(name).map_or(0, |topic| topic.bytes);
                        current.max(
                            subscription
                                .delivered
                                .as_ref()
                                .map_or(0, |topic| topic.bytes),
                        )
                    })
                    .sum::<usize>()
                    > BASELINE_BYTES
            })
            .map(|(id, _)| *id)
            .collect();
        for id in over {
            tracing::warn!("Client exceeded snapshot memory limit");
            self.remove_client(id);
        }
        for client in self.clients.values() {
            if client.topics.contains_key(topic) {
                client.wake.notify_one();
            }
        }
        self.notify(Notice::Changed {
            topic: topic.into(),
            available: self.topics[topic].data["available"].as_bool(),
        });
        Ok(())
    }
    pub fn add_client(&mut self) -> Result<(u64, Rc<Notify>), Failure> {
        if self.clients.len() >= MAX_CLIENTS {
            return Err(failure(ErrorCode::Busy, "Client limit reached", true));
        }
        self.next_client = self
            .next_client
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::Internal, "Client sequence exhausted", false))?;
        let wake = Rc::new(Notify::new());
        self.clients.insert(
            self.next_client,
            Client {
                wake: wake.clone(),
                topics: BTreeMap::new(),
                generation: 0,
                events: VecDeque::new(),
                event_bytes: 0,
                lost: BTreeMap::new(),
                replies: VecDeque::new(),
                reply_bytes: 0,
                inflight: BTreeSet::new(),
            },
        );
        Ok((self.next_client, wake))
    }
    pub fn remove_client(&mut self, id: u64) {
        if self.is_ui_provider(id) {
            if let Some(notifications) = &self.notifications {
                notifications.disconnected(id);
            }
        }
        if let Some(client) = self.clients.remove(&id) {
            client.wake.notify_one();
        }
        self.refresh_interest();
        if self.provider.as_ref().is_some_and(|p| p.0 == id) {
            self.provider = None;
        }
        self.routes.retain(|_, route| route.provider != id);
        if self
            .issued_provider_routes
            .is_some_and(|(provider, _, _)| provider == id)
        {
            self.issued_provider_routes = None;
        }
    }
    pub fn has_client(&self, id: u64) -> bool {
        self.clients.contains_key(&id)
    }
    fn fixed_methods(&self) -> BTreeMap<String, Method> {
        let mut methods = nacre_protocol::core_methods();
        methods.extend(super::bar_services::available_methods(self));
        if !self.safe {
            methods.extend(super::pill_controls::methods());
        }
        if self.settings.is_some() {
            methods.extend(super::settings::methods());
        }
        methods.extend(super::shell_actions::available_methods(self));
        if self.compositor.is_some() && self.topics.contains_key(super::niri_events::TOPIC) {
            methods.extend(super::niri_actions::methods());
        }
        if !self.safe && self.services.contains_key("session-watch") {
            methods.extend(super::session_policy::methods());
        }
        if !self.safe && self.services.contains_key("helper-lanes") {
            methods.extend(super::helpers::methods());
        }
        if !self.safe && self.services.contains_key("menu") {
            methods.extend(super::menu::methods());
        }
        if !self.safe && self.services.contains_key("niri-widths") {
            methods.extend(super::widths::methods());
        }
        if !self.safe {
            if self.snapshot_ref(super::active_wallpaper::TOPIC).is_some() {
                methods.extend(super::active_wallpaper::methods());
            }
            if self.services.contains_key("ai-chat") {
                methods.extend(super::ai_chat::methods());
            }
            if self.settings.is_some() && self.topics.contains_key(nacre_protocol::i18n::TOPIC) {
                methods.extend(super::i18n::methods());
            }
            if self.dashboard.is_some() {
                methods.extend(super::dashboard_extras::methods());
            }
            if self
                .services
                .get("dashboard-player")
                .is_some_and(|s| s["status"] == "running")
            {
                methods.extend(super::dashboard_player::methods());
            }
            methods.extend(super::control_center::available_methods(self));
            methods.extend(super::time_settings::available_methods(self));
            methods.extend(super::login_theme::available_methods(self));
            methods.extend(super::capabilities::available_methods(self));
            methods.extend(super::shortcuts::available_methods(self));
            methods.extend(super::system_status::available_methods(self));
            if self.calendar.is_some() {
                methods.extend(super::calendar::methods());
            }
            if self.snapshot_ref(super::updates::TOPIC).is_some() {
                methods.extend(super::updates::methods());
            }
            if self.snapshot_ref(super::weather::TOPIC).is_some() {
                methods.extend(super::weather::methods());
            }
            methods.extend(super::tools_launch::available_methods(self));
            methods.extend(super::colors_app::available_methods(self));
            if self.snapshot_ref(super::brain::TOPIC).is_some() {
                methods.extend(super::brain::methods());
            }
            if let Some(network) = &self.network {
                methods.extend(network.methods());
            }
            if let Some(bluetooth) = &self.bluetooth {
                methods.extend(bluetooth.methods());
            }
            if let Some(notifications) = &self.notifications {
                methods.extend(notifications.methods());
            }
        }
        methods
    }
    pub fn methods(&self) -> BTreeMap<String, Method> {
        use nacre_protocol::Capability;
        let mut methods = self.fixed_methods();
        if let Some((_, provider)) = &self.provider {
            for (name, version) in provider {
                // Selected action rows travel only from menu.activate to the
                // provider; this is not a public arbitrary-row execution API.
                if name == nacre_protocol::menu::UI_ACTIVATE {
                    continue;
                }
                methods.insert(
                    name.clone(),
                    Method {
                        v: *version,
                        class: Capability::Ui,
                        timeout: nacre_protocol::UI_CALL_TIMEOUT_MS,
                        params: json!({"type":"object"}),
                        result: json!({}),
                    },
                );
            }
        }
        methods
    }
    pub fn welcome(&self) -> Message {
        Message::Welcome {
            proto: nacre_protocol::VERSION,
            min: nacre_protocol::MIN_VERSION,
            boot: self.boot.clone(),
            topics: self
                .topics
                .keys()
                .filter(|key| {
                    key.as_str() != super::notifications::TOPIC
                        || self
                            .notifications
                            .as_ref()
                            .is_some_and(|service| service.available())
                })
                .map(|key| (key.clone(), 1))
                .collect(),
            methods: self.methods(),
            frame_ms: Some(self.frame.as_millis() as u32),
        }
    }
    pub fn status(&self) -> Value {
        json!({"boot":self.boot,"safe":self.safe,"clients":self.clients.len(),"services":self.services,"topics":self.topics.keys().cloned().collect::<Vec<_>>()})
    }
    pub fn service(&mut self, name: &str, status: &str, failures: usize) -> Result<(), Failure> {
        self.services
            .insert(name.into(), json!({"status":status,"failures":failures}));
        self.publish("daemon", self.status())
    }
    pub fn subscribe(&mut self, id: u64, topics: &[String], resync: bool) -> Result<(), Failure> {
        if topics.iter().any(|topic| !self.topics.contains_key(topic)) {
            return Err(failure(ErrorCode::UnknownTopic, "Unknown topic", false));
        }
        let client = self
            .clients
            .get_mut(&id)
            .ok_or_else(|| failure(ErrorCode::Unavailable, "Client disconnected", true))?;
        if resync
            && topics
                .iter()
                .any(|topic| !client.topics.contains_key(topic))
        {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Resync requires a subscription",
                false,
            ));
        }
        let count = client
            .topics
            .keys()
            .chain(topics)
            .collect::<BTreeSet<_>>()
            .len();
        if count > MAX_SUBSCRIPTIONS {
            return Err(failure(ErrorCode::Busy, "Subscription limit reached", true));
        }
        if topics
            .iter()
            .filter_map(|name| self.topics.get(name))
            .map(|t| t.bytes)
            .sum::<usize>()
            + client
                .topics
                .iter()
                .filter(|(name, _)| !topics.contains(name))
                .filter_map(|(name, _)| self.topics.get(name))
                .map(|t| t.bytes)
                .sum::<usize>()
            > BASELINE_BYTES
        {
            return Err(failure(
                ErrorCode::Busy,
                "Subscription memory limit reached",
                true,
            ));
        }
        for topic in topics {
            client.generation += 1;
            client.topics.insert(
                topic.clone(),
                Subscription {
                    delivered: None,
                    considered: None,
                    generation: client.generation,
                    force: true,
                    next: Instant::now(),
                },
            );
        }
        client.wake.notify_one();
        self.refresh_interest();
        Ok(())
    }
    pub fn unsubscribe(&mut self, id: u64, topics: &[String]) {
        if let Some(client) = self.clients.get_mut(&id) {
            for topic in topics {
                client.topics.remove(topic);
                client.lost.remove(topic);
            }
            client.events.retain(
                |event| !matches!(&event.message,Message::Ev {topic,..} if topics.contains(topic)),
            );
            client.event_bytes = client.events.iter().map(|event| event.bytes).sum();
        }
        self.refresh_interest();
    }
    pub fn enqueue(&mut self, id: u64, message: Message) -> Result<(), Failure> {
        let bytes = nacre_protocol::framing::encode(&message)?.len();
        let client = self
            .clients
            .get_mut(&id)
            .ok_or_else(|| failure(ErrorCode::Unavailable, "Client disconnected", true))?;
        if client.replies.len() >= MAX_CALLS || client.reply_bytes + bytes > QUEUE_BYTES {
            return Err(failure(ErrorCode::Busy, "Reply queue limit reached", true));
        }
        client.reply_bytes += bytes;
        client.replies.push_back((message, bytes));
        client.wake.notify_one();
        Ok(())
    }
    pub fn begin_call(&mut self, id: u64, call: &Id) -> Result<(), Failure> {
        let client = self
            .clients
            .get_mut(&id)
            .ok_or_else(|| failure(ErrorCode::Unavailable, "Client disconnected", true))?;
        if client.inflight.contains(call) {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Duplicate in-flight call ID",
                false,
            ));
        }
        if client.inflight.len() >= MAX_CALLS {
            return Err(failure(ErrorCode::Busy, "Call limit reached", true));
        }
        client.inflight.insert(call.clone());
        Ok(())
    }
    pub fn finish_call(&mut self, id: u64, call: Id, reply: Reply) -> Result<(), Failure> {
        if let Some(client) = self.clients.get_mut(&id) {
            client.inflight.remove(&call);
        }
        let message = Message::Ret {
            id: call.clone(),
            reply,
        };
        if let Err(error) = nacre_protocol::framing::encode(&message) {
            return self.enqueue(
                id,
                Message::Ret {
                    id: call,
                    reply: Reply::Err { err: error },
                },
            );
        }
        self.enqueue(id, message)
    }
    pub fn event(&mut self, topic: &str, name: &str, data: Value) -> Result<(), Failure> {
        self.event_after(topic, name, data, None)
    }
    /// Deliver an event only after its subscribed state dependencies are current.
    pub fn event_after(
        &mut self,
        topic: &str,
        name: &str,
        data: Value,
        state: Option<&str>,
    ) -> Result<(), Failure> {
        let mut after = Vec::new();
        for dependency in std::iter::once(topic).chain(state.filter(|state| *state != topic)) {
            if let Some(value) = self.topics.get(dependency) {
                after.push((dependency.to_owned(), value.seq));
            }
        }
        let seq = self
            .event_seq
            .get(topic)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|n| *n <= MAX_SAFE_INTEGER)
            .ok_or_else(|| failure(ErrorCode::Internal, "Event sequence exhausted", false))?;
        let message = Message::Ev {
            topic: topic.into(),
            name: name.into(),
            seq,
            data,
        };
        let bytes = nacre_protocol::framing::encode(&message)?.len();
        self.event_seq.insert(topic.into(), seq);
        if let Message::Ev {
            topic, name, data, ..
        } = &message
        {
            // Internal observers receive small deltas, or an explicit resync
            // marker for large events. The 256-slot ring stays byte-bounded.
            if bytes <= 1024 {
                self.notify(Notice::Event {
                    topic: topic.clone(),
                    name: name.clone(),
                    data: data.clone(),
                });
            } else {
                self.notify(Notice::Resync(topic.clone()));
            }
        }
        for client in self
            .clients
            .values_mut()
            .filter(|client| client.topics.contains_key(topic))
        {
            client.events.push_back(Event {
                message: message.clone(),
                bytes,
                after: after.clone(),
            });
            client.event_bytes += bytes;
            while client.events.len() > MAX_EVENTS || client.event_bytes > QUEUE_BYTES {
                if let Some(Event {
                    message: Message::Ev { topic, seq, .. },
                    bytes,
                    ..
                }) = client.events.pop_front()
                {
                    client.event_bytes -= bytes;
                    client
                        .lost
                        .entry(topic)
                        .and_modify(|range| range.1 = seq)
                        .or_insert((seq, seq));
                }
            }
            client.wake.notify_one();
        }
        Ok(())
    }
    pub fn next(&mut self, id: u64, now: Instant) -> (Option<Packet>, Option<Instant>) {
        let Some(client) = self.clients.get_mut(&id) else {
            return (None, None);
        };
        if let Some((message, bytes)) = client.replies.pop_front() {
            client.reply_bytes -= bytes;
            return (
                Some(Packet {
                    message,
                    commit: None,
                }),
                None,
            );
        }
        let mut deadline = None;
        for (name, subscription) in &mut client.topics {
            let Some(topic) = self.topics.get(name) else {
                continue;
            };
            if !subscription.force && subscription.considered == Some(topic.seq) {
                continue;
            }
            if now < subscription.next {
                deadline =
                    Some(deadline.map_or(subscription.next, |d: Instant| d.min(subscription.next)));
                continue;
            }
            let message = match &subscription.delivered {
                Some(delivered) if !subscription.force => {
                    match patch::diff(&delivered.data, &topic.data) {
                        Diff::Unchanged => {
                            subscription.considered = Some(topic.seq);
                            continue;
                        }
                        Diff::Patch(data) => Message::Patch {
                            topic: name.clone(),
                            base: delivered.seq,
                            seq: topic.seq,
                            data,
                        },
                        Diff::Snapshot(data) => Message::Snap {
                            topic: name.clone(),
                            seq: topic.seq,
                            data,
                        },
                    }
                }
                _ => Message::Snap {
                    topic: name.clone(),
                    seq: topic.seq,
                    data: topic.data.clone(),
                },
            };
            subscription.force = false;
            return (
                Some(Packet {
                    message,
                    commit: Some((name.clone(), topic.clone(), subscription.generation)),
                }),
                deadline,
            );
        }
        if let Some((topic, (from, to))) = client.lost.pop_first() {
            return (
                Some(Packet {
                    message: Message::Lost { topic, from, to },
                    commit: None,
                }),
                deadline,
            );
        }
        let ready = client.events.front().is_some_and(|event| {
            event.after.iter().all(|(topic, seq)| {
                client.topics.get(topic).is_none_or(|subscription| {
                    !subscription.force
                        && subscription.considered.is_some_and(|known| known >= *seq)
                })
            })
        });
        if ready {
            if let Some(event) = client.events.pop_front() {
                client.event_bytes -= event.bytes;
                return (
                    Some(Packet {
                        message: event.message,
                        commit: None,
                    }),
                    deadline,
                );
            }
        }
        (None, deadline)
    }
    pub fn sent(&mut self, id: u64, packet: &Packet, now: Instant) {
        if let Some((name, topic, generation)) = &packet.commit {
            if let Some(subscription) = self
                .clients
                .get_mut(&id)
                .and_then(|client| client.topics.get_mut(name))
            {
                if subscription.generation == *generation {
                    subscription.delivered = Some(topic.clone());
                    subscription.considered = Some(topic.seq);
                    subscription.next = now + self.frame;
                }
            }
        }
    }
    pub fn register(&mut self, id: u64, methods: BTreeMap<String, u16>) -> Result<(), Failure> {
        Message::Register {
            provider: "ui".into(),
            methods: methods.clone(),
        }
        .validate()?;
        if self
            .fixed_methods()
            .keys()
            .chain(methods.keys())
            .collect::<BTreeSet<_>>()
            .len()
            > 512
        {
            return Err(failure(
                ErrorCode::Busy,
                "Method registry limit reached",
                true,
            ));
        }
        if self
            .provider
            .as_ref()
            .is_some_and(|provider| provider.0 != id)
        {
            return Err(failure(
                ErrorCode::Busy,
                "UI provider is already registered",
                true,
            ));
        }
        self.provider = Some((id, methods));
        Ok(())
    }
    pub fn forward(
        &mut self,
        method: &str,
        params: Value,
        timeout: Option<u32>,
    ) -> Result<(Id, oneshot::Receiver<Reply>), Failure> {
        let provider = self
            .provider
            .as_ref()
            .filter(|(_, methods)| methods.contains_key(method))
            .map(|p| p.0)
            .ok_or_else(|| {
                failure(ErrorCode::Unavailable, "UI provider is unavailable", true)
                    .dispatch_state(false)
            })?;
        if self.routes.len() >= MAX_CALLS {
            return Err(failure(ErrorCode::Busy, "UI call limit reached", true));
        }
        self.next_route = self
            .next_route
            .checked_add(1)
            .ok_or_else(|| failure(ErrorCode::Internal, "UI call sequence exhausted", false))?;
        let id = Id::Text(format!("provider-{}", self.next_route));
        let (sender, receiver) = oneshot::channel();
        let deadline_ms = timeout.unwrap_or_else(|| {
            self.methods()
                .get(method)
                .map_or(1000, |registered| registered.timeout)
        });
        if let Err(error) = self.enqueue(
            provider,
            Message::Call {
                id: id.clone(),
                m: method.into(),
                p: params,
                timeout,
            },
        ) {
            self.next_route -= 1;
            return Err(error);
        }
        let number = self.next_route;
        self.issued_provider_routes = Some(match self.issued_provider_routes {
            Some((issued_provider, first, last)) if issued_provider == provider => {
                (provider, first, last.max(number))
            }
            _ => (provider, number, number),
        });
        self.routes.insert(
            id.clone(),
            PendingRoute {
                provider,
                reply: sender,
                method: method.to_owned(),
                started: Instant::now(),
                deadline_ms,
            },
        );
        Ok((id, receiver))
    }
    pub fn answer(&mut self, provider: u64, id: &Id, reply: Reply) -> Result<(), Failure> {
        if let Some(route) = self.routes.get(id) {
            if route.provider != provider {
                return Err(failure(
                    ErrorCode::InvalidRequest,
                    "Unknown provider reply",
                    false,
                ));
            }
        } else if self
            .issued_provider_routes
            .is_some_and(|(issued_provider, first, last)| {
                issued_provider == provider
                    && self
                        .provider_route_number(id)
                        .is_some_and(|number| (first..=last).contains(&number))
            })
        {
            tracing::debug!(route = ?id, "Dropping late UI provider reply for retired route");
            return Ok(());
        } else {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Unknown provider reply",
                false,
            ));
        }
        if let Some(route) = self.routes.remove(id) {
            if route.reply.send(reply).is_err() {
                tracing::debug!(route = ?id, "UI result arrived after caller deadline");
            }
        }
        Ok(())
    }
    pub fn cancel_route(&mut self, id: &Id) {
        if let Some(route) = self.routes.remove(id) {
            tracing::info!(route = ?id, provider = route.provider, method = %route.method, elapsed_ms = route.started.elapsed().as_millis(), deadline_ms = route.deadline_ms, "UI call timed out or was canceled before reply; action may still complete");
        }
    }
    fn provider_route_number(&self, id: &Id) -> Option<u64> {
        match id {
            Id::Text(value) => {
                let number: u64 = value.strip_prefix("provider-")?.parse().ok()?;
                (value == &format!("provider-{number}")).then_some(number)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Guard: every method any module can advertise must pass the protocol's Welcome validation
    /// (timeouts 1..=120000 ms). Backend-dependent methods (network, notifications, bluetooth)
    /// are covered by their own fixtures that validate the full Welcome.
    #[test]
    fn every_static_method_set_passes_welcome_validation() {
        let mut methods = nacre_protocol::core_methods();
        for set in [
            super::super::pill_controls::methods(),
            super::super::settings::methods(),
            super::super::shell_actions::methods(),
            super::super::niri_actions::methods(),
            super::super::session_policy::methods(),
            super::super::helpers::methods(),
            super::super::menu::methods(),
            super::super::widths::methods(),
            super::super::active_wallpaper::methods(),
            super::super::ai_chat::methods(),
            super::super::dashboard_extras::methods(),
            super::super::dashboard_player::methods(),
            super::super::brain::methods(),
            super::super::i18n::methods(),
            nacre_protocol::system_settings::methods(),
        ] {
            methods.extend(set);
        }
        let welcome = Message::Welcome {
            proto: nacre_protocol::VERSION,
            min: nacre_protocol::MIN_VERSION,
            boot: "guard".into(),
            topics: BTreeMap::new(),
            methods: methods.clone(),
            frame_ms: Some(16),
        };
        welcome.validate().unwrap_or_else(|_| {
            let bad: Vec<_> = methods
                .iter()
                .filter(|(_, m)| !(1..=120_000).contains(&m.timeout))
                .map(|(n, m)| (n.clone(), m.timeout))
                .collect();
            panic!("welcome invalid; out-of-range timeouts: {bad:?}")
        });
    }
    #[test]
    fn internal_observers_get_changes_and_bounded_events_without_client_slots() {
        let mut hub = Hub::new("fixture".into(), false, Duration::from_millis(16));
        let mut observer = hub.observe();
        hub.publish("compositor", json!({"available":true}))
            .unwrap();
        assert!(matches!(
            observer.try_recv().unwrap(),
            Notice::Changed {
                available: Some(true),
                ..
            }
        ));
        hub.event("compositor", "outputs-changed", json!({}))
            .unwrap();
        assert!(
            matches!(observer.try_recv().unwrap(),Notice::Event{ref name,..} if name=="outputs-changed")
        );
        hub.event("compositor", "large", json!({"value":"x".repeat(2048)}))
            .unwrap();
        assert!(
            matches!(observer.try_recv().unwrap(),Notice::Resync(ref name) if name=="compositor")
        );
        assert_eq!(hub.status()["clients"], 0);
        for version in 0..300 {
            hub.publish("compositor", json!({"available":true,"revision":version}))
                .unwrap();
        }
        assert!(matches!(
            observer.try_recv(),
            Err(broadcast::error::TryRecvError::Lagged(_))
        ));
    }
    fn fixture() -> (Hub, u64, Instant) {
        let mut hub = Hub::new("fixture".into(), false, Duration::from_millis(16));
        hub.publish("x", json!({"v":0})).unwrap();
        let (id, _) = hub.add_client().unwrap();
        hub.subscribe(id, &["x".into()], false).unwrap();
        (hub, id, Instant::now())
    }
    fn send(hub: &mut Hub, id: u64, at: Instant) -> Message {
        let packet = hub.next(id, at).0.expect("packet");
        hub.sent(id, &packet, at);
        packet.message
    }
    #[test]
    fn coalescing_uses_only_the_last_successfully_written_client_baseline() {
        let (mut hub, id, now) = fixture();
        assert!(matches!(
            send(&mut hub, id, now),
            Message::Snap { seq: 0, .. }
        ));
        hub.publish("x", json!({"v":1})).unwrap();
        hub.publish("x", json!({"v":2})).unwrap();
        assert!(hub.next(id, now).0.is_none());
        assert!(matches!(
            send(&mut hub, id, now + Duration::from_millis(16)),
            Message::Patch {
                base: 0,
                seq: 2,
                ..
            }
        ));
    }
    #[test]
    fn change_back_before_delivery_does_not_advance_the_clients_wire_sequence() {
        let (mut hub, id, now) = fixture();
        send(&mut hub, id, now);
        hub.publish("x", json!({"v":1})).unwrap();
        hub.publish("x", json!({"v":0})).unwrap();
        assert!(hub.next(id, now + Duration::from_millis(16)).0.is_none());
        assert!(hub.next(id, now + Duration::from_millis(16)).1.is_none());
        hub.publish("x", json!({"v":3})).unwrap();
        assert!(matches!(
            send(&mut hub, id, now + Duration::from_millis(16)),
            Message::Patch {
                base: 0,
                seq: 3,
                ..
            }
        ));
    }
    #[test]
    fn resubscribe_inflight_does_not_commit_an_obsolete_subscription_generation() {
        let (mut hub, id, now) = fixture();
        let packet = hub.next(id, now).0.unwrap();
        hub.unsubscribe(id, &["x".into()]);
        hub.subscribe(id, &["x".into()], false).unwrap();
        hub.sent(id, &packet, now);
        assert!(matches!(
            send(&mut hub, id, Instant::now()),
            Message::Snap { .. }
        ));
        assert_eq!(
            hub.subscribe(id, &["missing".into()], true)
                .unwrap_err()
                .code,
            ErrorCode::UnknownTopic
        );
    }
    #[test]
    fn two_clients_get_independent_patches_and_explicit_resync_gets_snapshot() {
        let (mut hub, first, now) = fixture();
        send(&mut hub, first, now);
        hub.publish("x", json!({"v":1})).unwrap();
        let (second, _) = hub.add_client().unwrap();
        hub.subscribe(second, &["x".into()], false).unwrap();
        let later = Instant::now();
        send(&mut hub, second, later);
        hub.publish("x", json!({"v":2})).unwrap();
        assert!(matches!(
            send(&mut hub, first, later + Duration::from_millis(16)),
            Message::Patch {
                base: 0,
                seq: 2,
                ..
            }
        ));
        assert!(matches!(
            send(&mut hub, second, later + Duration::from_millis(16)),
            Message::Patch {
                base: 1,
                seq: 2,
                ..
            }
        ));
        hub.subscribe(first, &["x".into()], true).unwrap();
        assert!(matches!(
            send(&mut hub, first, later + Duration::from_millis(16)),
            Message::Snap { seq: 2, .. }
        ));
    }
    #[test]
    fn events_overflow_with_lost_before_the_surviving_events_and_keep_latest() {
        let (mut hub, id, now) = fixture();
        send(&mut hub, id, now);
        for n in 1..=300 {
            hub.event("x", "example", json!({"id":n})).unwrap();
        }
        assert_eq!(hub.clients[&id].events.len(), 256);
        assert!(matches!(
            send(&mut hub, id, now),
            Message::Lost {
                from: 1,
                to: 44,
                ..
            }
        ));
        assert!(matches!(
            send(&mut hub, id, now),
            Message::Ev { seq: 45, .. }
        ));
        hub.unsubscribe(id, &["x".into()]);
        assert_eq!(hub.clients[&id].event_bytes, 0);
    }
    #[test]
    fn semantic_events_wait_for_successfully_delivered_state_without_blocking_replies() {
        let (mut hub, id, now) = fixture();
        send(&mut hub, id, now);
        hub.publish("events", json!({})).unwrap();
        hub.subscribe(id, &["events".into()], false).unwrap();
        let now = Instant::now();
        send(&mut hub, id, now);
        hub.publish("x", json!({"v":1})).unwrap();
        hub.event_after("events", "changed", json!({}), Some("x"))
            .unwrap();
        assert!(hub.next(id, now + Duration::from_millis(1)).0.is_none());
        hub.enqueue(id, Message::Pong { id: Id::Integer(7) })
            .unwrap();
        assert!(matches!(send(&mut hub, id, now), Message::Pong { .. }));
        let at = now + Duration::from_millis(16);
        let packet = hub.next(id, at).0.unwrap();
        assert!(matches!(packet.message,Message::Patch {ref topic,..} if topic=="x"));
        // Merely constructing an outgoing patch must not release its event.
        assert!(matches!(
            hub.next(id, at).0.unwrap().message,
            Message::Patch { .. }
        ));
        hub.sent(id, &packet, at);
        assert!(matches!(send(&mut hub, id, at), Message::Ev { .. }));
        hub.publish("x", json!({"v":2})).unwrap();
        hub.publish("x", json!({"v":1})).unwrap();
        hub.event_after("events", "unchanged", json!({}), Some("x"))
            .unwrap();
        // A coalesced return to the delivered value needs no redundant patch.
        assert!(matches!(
            send(&mut hub, id, at + Duration::from_millis(16)),
            Message::Ev { .. }
        ));
    }
    #[test]
    fn oversized_state_and_duplicate_calls_and_second_ui_provider_are_rejected() {
        let (mut hub, id, _) = fixture();
        assert_eq!(
            hub.publish("x", json!({"v":"x".repeat(nacre_protocol::MAX_FRAME)}))
                .unwrap_err()
                .code,
            ErrorCode::TooLarge
        );
        hub.begin_call(id, &Id::Integer(1)).unwrap();
        assert_eq!(
            hub.begin_call(id, &Id::Integer(1)).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        hub.register(id, BTreeMap::from([("ui.dispatch".into(), 1)]))
            .unwrap();
        let (other, _) = hub.add_client().unwrap();
        assert_eq!(
            hub.register(other, BTreeMap::new()).unwrap_err().code,
            ErrorCode::Busy
        );
        assert_eq!(hub.provider.as_ref().unwrap().0, id);
    }
    #[test]
    fn ui_routes_use_independent_ids_and_disconnect_drops_waiters() {
        let (mut hub, id, now) = fixture();
        hub.register(id, BTreeMap::from([("ui.dispatch".into(), 1)]))
            .unwrap();
        let (route, mut receiver) = hub.forward("ui.dispatch", json!({}), Some(100)).unwrap();
        assert!(matches!(send(&mut hub, id, now), Message::Call { .. }));
        hub.answer(id, &route, Reply::Ok { ok: Value::Null })
            .unwrap();
        assert_eq!(receiver.try_recv().unwrap(), Reply::Ok { ok: Value::Null });
        let (_, mut receiver) = hub.forward("ui.dispatch", json!({}), Some(100)).unwrap();
        hub.remove_client(id);
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn retired_ui_routes_drop_late_replies_only_from_their_provider_and_are_bounded() {
        let (mut hub, provider, _) = fixture();
        let (other, _) = hub.add_client().unwrap();
        hub.register(provider, BTreeMap::from([("ui.dispatch".into(), 1)]))
            .unwrap();

        let (route, _receiver) = hub.forward("ui.dispatch", json!({}), None).unwrap();
        assert!(matches!(
            hub.next(provider, Instant::now()).0.unwrap().message,
            Message::Call { ref id, .. } if id == &route
        ));
        assert_eq!(
            hub.answer(other, &route, Reply::Ok { ok: Value::Null })
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
        hub.cancel_route(&route);
        assert!(hub
            .answer(provider, &route, Reply::Ok { ok: Value::Null })
            .is_ok());
        assert_eq!(
            hub.answer(other, &route, Reply::Ok { ok: Value::Null })
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            hub.answer(
                provider,
                &Id::Text("unknown".into()),
                Reply::Ok { ok: Value::Null }
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidRequest
        );

        let mut latest = None;
        for _ in 0..1000 {
            let (route, _) = hub.forward("ui.dispatch", json!({}), None).unwrap();
            assert!(matches!(
                hub.next(provider, Instant::now()).0.unwrap().message,
                Message::Call { ref id, .. } if id == &route
            ));
            latest = Some(route.clone());
            hub.cancel_route(&route);
        }
        assert!(hub.routes.is_empty());
        assert!(hub
            .answer(provider, &route, Reply::Ok { ok: Value::Null })
            .is_ok());
        assert!(hub
            .answer(provider, &latest.unwrap(), Reply::Ok { ok: Value::Null })
            .is_ok());
        for malformed in ["provider-01", "provider-999999"] {
            assert_eq!(
                hub.answer(
                    provider,
                    &Id::Text(malformed.into()),
                    Reply::Ok { ok: Value::Null }
                )
                .unwrap_err()
                .code,
                ErrorCode::InvalidRequest
            );
        }
        hub.remove_client(provider);
        assert!(hub.issued_provider_routes.is_none());
        hub.register(other, BTreeMap::from([("ui.dispatch".into(), 1)]))
            .unwrap();
        let _ = hub.forward("ui.dispatch", json!({}), None).unwrap();
        assert_eq!(
            hub.answer(other, &route, Reply::Ok { ok: Value::Null })
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
    #[test]
    fn failed_provider_enqueue_does_not_issue_or_burn_a_route_id() {
        let (mut hub, provider, _) = fixture();
        hub.register(provider, BTreeMap::from([("ui.dispatch".into(), 1)]))
            .unwrap();
        for sequence in 0..MAX_CALLS {
            hub.enqueue(
                provider,
                Message::Ping {
                    id: Id::Text(format!("fill-{sequence}")),
                },
            )
            .unwrap();
        }
        let error = match hub.forward("ui.dispatch", json!({}), None) {
            Err(error) => error,
            Ok(_) => panic!("Forward unexpectedly succeeded with a full provider queue"),
        };
        assert_eq!(error.code, ErrorCode::Busy);
        assert!(hub.issued_provider_routes.is_none());
        for _ in 0..MAX_CALLS {
            hub.next(provider, Instant::now()).0.unwrap();
        }
        assert_eq!(
            hub.forward("ui.dispatch", json!({}), None).unwrap().0,
            Id::Text("provider-1".into())
        );
    }
    #[test]
    fn registry_and_topic_caps_keep_welcome_encodable_and_large_calls_get_typed_errors() {
        let (mut hub, id, now) = fixture();
        for n in 0..127 {
            hub.publish(&format!("topic-{n}"), json!({})).unwrap();
        }
        assert_eq!(
            hub.publish("too-many", json!({})).unwrap_err().code,
            ErrorCode::Busy
        );
        let methods = (0..512).map(|n| (format!("ui.method{n}"), 1)).collect();
        assert_eq!(hub.register(id, methods).unwrap_err().code, ErrorCode::Busy);
        assert!(nacre_protocol::framing::encode(&hub.welcome()).is_ok());
        hub.begin_call(id, &Id::Integer(9)).unwrap();
        hub.finish_call(
            id,
            Id::Integer(9),
            Reply::Ok {
                ok: json!("x".repeat(nacre_protocol::MAX_FRAME)),
            },
        )
        .unwrap();
        assert!(matches!(
            send(&mut hub, id, now),
            Message::Ret {
                reply: Reply::Err {
                    err: Failure {
                        code: ErrorCode::TooLarge,
                        ..
                    }
                },
                ..
            }
        ));
    }
}
