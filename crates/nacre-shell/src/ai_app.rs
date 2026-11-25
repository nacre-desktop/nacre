// SPDX-License-Identifier: GPL-3.0-only
//! N0248 Nacre AI app (design review D8): a Bead window with a rail (Chats, Projects, Accounts,
//! Usage, Brain) over the provider-neutral N0319 API. The layout is `shell/ai/*.luau`; this
//! adapter only projects the `ai.providers`, `ai.projects`, `ai.accounts` and `ai.usage` topics
//! and the `ai.chats.list` reply into `ai.*` stores and turns the `ai.do` host action into
//! daemon calls. No provider is named here: every provider comes from `ai.providers`.
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{self, Source, Subscription},
};
use nacre_protocol::ai_app::{AccountStatus, Accounts, ChatState, Chats, Projects, Providers};
use nacre_protocol::dashboard_extras::AiUsageState;
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
    time::Duration,
};

pub const PAGES: [&str; 4] = ["chats", "projects", "accounts", "usage"];
const TOPICS: [&str; 4] = ["ai.providers", "ai.projects", "ai.accounts", "ai.usage"];
const DAY_MS: u64 = 86_400_000;

#[derive(Default)]
struct Calls {
    chats: Option<CallTicket>,
    /// The last `ai.chats.list` result; `None` until one arrives.
    listed: Option<Json>,
    /// The first list was requested; a failed one is asked again by the next refresh (`stale`).
    asked: bool,
    /// A list is requested again after a start/resume/refresh finishes.
    stale: bool,
    pending: Vec<CallTicket>,
    message: String,
    /// The daemon boot the list was asked under.
    boot: Option<String>,
}

impl Calls {
    /// A finished call: the note shows its error and clears on the next success.
    fn settle(&mut self, reply: source::Result<Json>) {
        match reply {
            Ok(_) => self.message.clear(),
            Err(error) => self.message = error.message,
        }
    }
    /// A restarted daemon (new boot) invalidates the listed chats.
    fn see_boot(&mut self, boot: Option<String>) {
        if boot != self.boot {
            self.stale |= self.boot.is_some();
            self.boot = boot;
        }
    }
}

pub struct AiApp {
    client: Option<Client>,
    sources: Vec<(&'static str, Arc<dyn Source>)>,
    subscriptions: RefCell<Vec<Subscription>>,
    wake: RefCell<Option<CallWake>>,
    calls: RefCell<Calls>,
    viewport: Cell<(u32, u32)>,
    now: fn() -> u64,
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn parse<T: serde::de::DeserializeOwned>(data: &Json) -> Option<T> {
    serde_json::from_value(data.clone()).ok()
}
fn stored<'a>(stores: &'a Stores, key: &str) -> &'a str {
    match stores.get(key) {
        Some(Value::Text(s)) => s,
        _ => "",
    }
}

/// Providers in registry order; the chosen one is the stored id when listed, else the first.
pub(crate) fn providers(data: &Json, chosen: &str) -> Vec<Json> {
    let Some(list) = parse::<Providers>(data).filter(|p| p.validate().is_ok()) else {
        return Vec::new();
    };
    let pick = list
        .providers
        .iter()
        .find(|p| p.id == chosen && p.capabilities.chat)
        .or_else(|| list.providers.iter().find(|p| p.capabilities.chat))
        .map(|p| p.id.clone())
        .unwrap_or_default();
    list.providers
        .iter()
        .map(|p| {
            json!({"id":p.id,"label":p.label,"selected":p.id==pick,"chat":p.capabilities.chat,
                "accounts":p.capabilities.accounts,"usage":p.capabilities.usage})
        })
        .collect()
}

/// Projects as settings rows; the chosen one is the stored id when listed, else the first.
pub(crate) fn projects(data: &Json, chosen: &str) -> Vec<Json> {
    let Some(list) = parse::<Projects>(data).filter(|p| p.validate().is_ok()) else {
        return Vec::new();
    };
    let pick = list
        .projects
        .iter()
        .find(|p| p.id == chosen)
        .or(list.projects.first())
        .map(|p| p.id.clone())
        .unwrap_or_default();
    list.projects
        .iter()
        .map(|p| {
            let path = match &p.host {
                Some(host) => format!("{host}:{}", p.path),
                None => p.path.clone(),
            };
            json!({"id":p.id,"label":p.label,"path":path,"remote":p.host.is_some(),"selected":p.id==pick})
        })
        .collect()
}

/// Accounts grouped under their provider (registry order), each with a status label.
pub(crate) fn accounts(data: &Json, providers: &[Json]) -> Vec<Json> {
    let Some(list) = parse::<Accounts>(data).filter(|a| a.validate().is_ok()) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for provider in providers {
        let id = provider["id"].as_str().unwrap_or_default();
        let label = provider["label"].as_str().unwrap_or(id);
        for (n, a) in list
            .accounts
            .iter()
            .filter(|a| a.provider == id)
            .enumerate()
        {
            let (status, tone) = match a.status {
                AccountStatus::SignedIn => ("bead.panel.ai-signed-in", "success"),
                AccountStatus::SignedOut => ("bead.panel.ai-signed-out", "error"),
                AccountStatus::Missing => ("bead.panel.ai-missing", "error"),
                AccountStatus::Unknown => ("bead.panel.ai-unchecked", "onSurfaceVariant"),
            };
            rows.push(
                json!({"id":format!("{id}:{}",a.id),"group":if n==0 {label} else {""},
                "label":a.label,"active":a.active,"status":tr(status, &[]),"tone":tone}),
            );
        }
    }
    rows
}

/// `ai.usage` as rows: one ring row per structured window, text lines for a provider without
/// windows. Shared with the dashboard's AI usage page; nothing is estimated.
pub(crate) fn usage_rows(s: &AiUsageState, now: u64) -> Vec<Json> {
    use crate::dashboard::line;
    let mut rows = Vec::new();
    for id in &s.providers.order {
        let Some(p) = s.providers.items.get(id) else {
            continue;
        };
        let name = line(&p.name, 32);
        if p.windows.is_empty() {
            rows.push(
                json!({"id":id.0,"label":name,"value":"","detail":"","provider":id.0,"head":true}),
            );
            for (n, l) in p.lines.iter().enumerate() {
                rows.push(json!({"id":format!("{}-{n}", id.0),"label":line(l,96),"value":"","detail":"","provider":id.0}));
            }
        }
        for (n, w) in p.windows.iter().enumerate() {
            let resets = w
                .resets_ms
                .filter(|t| *t > now)
                .map(|t| {
                    tr(
                        "bead.dashboard.ai-usage-resets",
                        &[("value", &crate::notice_service::time_str(t - now))],
                    )
                })
                .unwrap_or_default();
            rows.push(json!({"id":format!("{}-w{n}", id.0),"label":format!("{name} · {}", line(&w.label,24)),
                "value":format!("{:.0}%", w.used_percent),"detail":resets,"provider":id.0,"percent":w.used_percent}));
        }
    }
    rows.truncate(14);
    rows
}

/// `ai.chats.list` rows grouped Running, Today, Earlier (a heading on each group's first row).
/// The provider chip shows the registry id, as the sidebar's rows do.
pub(crate) fn chat_rows(reply: &Json, now: u64) -> Vec<Json> {
    let Some(list) = parse::<Chats>(reply) else {
        return Vec::new();
    };
    // Owner 2026-11-25: every chat with an open session is Running, working or waiting for input.
    let open =
        |c: &nacre_protocol::ai_app::Chat| c.state == ChatState::Running || c.session.is_some();
    let group = |c: &nacre_protocol::ai_app::Chat| {
        let updated = (c.updated_at.max(0.) * 1000.) as u64;
        if open(c) {
            0
        } else if now.saturating_sub(updated) < DAY_MS {
            1
        } else {
            2
        }
    };
    let mut chats: Vec<_> = list
        .chats
        .iter()
        .take(nacre_protocol::ai_app::MAX_CHATS)
        .collect();
    chats.sort_by_key(|c| group(c));
    let headings = [
        "bead.panel.ai-running",
        "bead.panel.ai-today",
        "bead.panel.ai-earlier",
    ];
    let mut last = None;
    chats
        .into_iter()
        .map(|c| {
            let g = group(c);
            let heading = (last != Some(g)).then(|| tr(headings[g], &[]));
            last = Some(g);
            let updated = (c.updated_at.max(0.) * 1000.) as u64;
            let age = crate::notice_service::time_str(now.saturating_sub(updated));
            let project = match &c.host {
                Some(host) => format!("{host}:{}", c.project),
                None => c.project.clone(),
            };
            json!({"key":c.id,"title":c.title,"provider":c.provider,"project":project,"age":age,
                "running":open(c),"working":c.state==ChatState::Running,"resumable":c.resumable,"group":heading.unwrap_or_default()})
        })
        .collect()
}

impl AiApp {
    pub fn new(client: Option<Client>) -> Self {
        let sources = client
            .as_ref()
            .map(|c| {
                TOPICS
                    .iter()
                    .filter_map(|name| c.view_topic(name).ok().map(|t| (*name, t)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            client,
            sources,
            subscriptions: Default::default(),
            wake: Default::default(),
            calls: Default::default(),
            viewport: Cell::new((1120, 760)),
            now: now_ms,
        }
    }
    /// Fixture topics and a fixed clock (tests and previews): no daemon call is made.
    #[cfg(test)]
    pub fn with_fixture(
        mut self,
        sources: Vec<(&'static str, Arc<dyn Source>)>,
        chats: Option<Json>,
        now: fn() -> u64,
    ) -> Self {
        self.sources = sources;
        self.calls.borrow_mut().listed = chats;
        self.now = now;
        self
    }
    pub fn set_viewport(&self, width: u32, height: u32) {
        self.viewport.set((width, height));
    }
    /// Topic changes and finished calls project again.
    pub fn subscribe(&self, wake: CallWake) {
        let mut subscriptions = self.subscriptions.borrow_mut();
        for (_, source) in &self.sources {
            let changed = wake.clone();
            if let Ok(s) = source.subscribe(Arc::new(move |_| changed())) {
                subscriptions.push(s);
            }
        }
        *self.wake.borrow_mut() = Some(wake);
    }
    fn data(&self, topic: &str) -> Json {
        self.sources
            .iter()
            .find(|(name, _)| *name == topic)
            .map(|(_, s)| (*s.snapshot().data).clone())
            .unwrap_or(Json::Null)
    }
    fn ready_for(&self, method: &str) -> Option<(String, Duration)> {
        let client = self.client.as_ref()?;
        let status = client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        client.snapshot().connected.then_some(())?;
        Some((
            status.boot.clone()?,
            Duration::from_millis(u64::from(advertised.timeout).clamp(1, 120_000)),
        ))
    }
    fn begin(&self, method: &str, params: Json) -> Option<CallTicket> {
        let (boot, timeout) = self.ready_for(method)?;
        self.client
            .as_ref()?
            .begin_call(
                method.into(),
                params,
                timeout,
                CallGuard { boot, version: 1 },
                self.wake.borrow().clone(),
            )
            .ok()
    }
    /// Collect finished calls and keep one `ai.chats.list` in flight while the list is stale.
    fn poll(&self) {
        let mut guard = self.calls.borrow_mut();
        let calls = &mut *guard;
        if let Some(reply) = calls.chats.as_mut().and_then(CallTicket::try_take) {
            calls.chats = None;
            if let Ok(listed) = &reply {
                calls.listed = Some(listed.clone());
            }
            calls.settle(reply);
        }
        let mut finished = false;
        let mut replies = Vec::new();
        calls.pending.retain_mut(|ticket| match ticket.try_take() {
            None => true,
            Some(reply) => {
                replies.push(reply);
                false
            }
        });
        for reply in replies {
            finished = true;
            calls.settle(reply);
        }
        calls.stale |= finished;
        calls.see_boot(self.client.as_ref().and_then(|c| c.status().boot.clone()));
        if calls.chats.is_none() && (calls.stale || !calls.asked) {
            calls.chats = self.begin("ai.chats.list", json!({}));
            calls.asked |= calls.chats.is_some();
            calls.stale &= calls.chats.is_none();
        }
    }
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        self.poll();
        let now = (self.now)();
        let providers = providers(&self.data("ai.providers"), stored(stores, "ai.provider"));
        let projects = projects(&self.data("ai.projects"), stored(stores, "ai.project"));
        let accounts = accounts(&self.data("ai.accounts"), &providers);
        let usage = parse::<AiUsageState>(&self.data("ai.usage"))
            .filter(|s| s.validate().is_ok())
            .map(|s| usage_rows(&s, now))
            .unwrap_or_default();
        let calls = self.calls.borrow();
        let chats = calls
            .listed
            .as_ref()
            .map(|reply| chat_rows(reply, now))
            .unwrap_or_default();
        let connected = self.client.as_ref().is_some_and(|c| c.snapshot().connected)
            || (cfg!(test) && !providers.is_empty());
        let (width, _) = self.viewport.get();
        let flag = |name: &str, on: bool| (format!("ai.can.{name}"), Value::Bool(on));
        BTreeMap::from([
            ("ai.win_w".into(), Value::Number(f64::from(width))),
            ("ai.connected".into(), Value::Bool(connected)),
            ("ai.loaded".into(), Value::Bool(calls.listed.is_some())),
            ("ai.message".into(), text(&calls.message)),
            flag(
                "start",
                cfg!(test) || self.ready_for("ai.chats.start").is_some(),
            ),
            flag(
                "resume",
                cfg!(test) || self.ready_for("ai.chats.resume").is_some(),
            ),
            flag(
                "refresh",
                cfg!(test) || self.ready_for("ai.projects.refresh").is_some(),
            ),
            ("ai.providers".into(), Value::list(providers)),
            ("ai.projects".into(), Value::list(projects)),
            ("ai.accounts".into(), Value::list(accounts)),
            ("ai.usage".into(), Value::list(usage)),
            ("ai.chats".into(), Value::list(chats)),
        ])
    }
    /// The host action `ai.do`, argument `op:payload`: daemon calls only (the rail page and the
    /// provider and project choice are Luau view state). A payload is an id sent as a method
    /// parameter; it never becomes a command, path or URL.
    pub fn act(&self, arg: &str, stores: &Stores) {
        let (op, payload) = arg.split_once(':').unwrap_or((arg, ""));
        let call = |method: &str, params: Json| {
            if let Some(ticket) = self.begin(method, params) {
                self.calls.borrow_mut().pending.push(ticket);
            }
        };
        match op {
            "start" => {
                let pick = |rows: Vec<Json>| {
                    rows.into_iter()
                        .find(|r| r["selected"] == true)
                        .and_then(|r| r["id"].as_str().map(str::to_owned))
                };
                let provider = pick(providers(
                    &self.data("ai.providers"),
                    stored(stores, "ai.provider"),
                ));
                let project = pick(projects(
                    &self.data("ai.projects"),
                    stored(stores, "ai.project"),
                ));
                if let (Some(provider), Some(project)) = (provider, project) {
                    call(
                        "ai.chats.start",
                        json!({"provider":provider,"project":project}),
                    );
                }
            }
            "resume" if !payload.is_empty() && payload.len() <= 128 => {
                call("ai.chats.resume", json!({"id":payload}));
            }
            "refresh" => match payload {
                "projects" => call("ai.projects.refresh", json!({})),
                "accounts" => call("ai.accounts.refresh", json!({})),
                "usage" => call("ai.usage.refresh", json!({})),
                _ => self.calls.borrow_mut().stale = true,
            },
            "brain" => {
                // The window route can wait on its socket, so it runs off the UI thread.
                std::thread::spawn(|| {
                    let _ = crate::app::route(0, "brain", "open", &[]);
                });
            }
            "settings" => call("ai.launcher.action", json!({"action":"settings"})),
            _ => {}
        }
    }
}

/// The Nacre AI window as a framework `WindowSpec`, sized like the Brain window from its
/// behaviour defaults and remembering its size.
pub fn window_spec(
    root: std::path::PathBuf,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    client: Option<Client>,
    trace: bool,
) -> Result<bead::wayland::WindowSpec, Box<dyn std::error::Error>> {
    let behavior = root.join("ai/behavior.luau");
    let schema = bead::ui::script::Script::load(&std::fs::read(&behavior)?, &behavior)?.schema()?;
    let number = |key: &str| -> Result<u32, Box<dyn std::error::Error>> {
        match schema.get(key) {
            Some(Value::Number(value)) if value.is_finite() && *value > 0. && *value <= 4096. => {
                Ok(*value as u32)
            }
            _ => Err(format!("invalid Nacre AI window dimension: {key}").into()),
        }
    };
    let state_path = crate::state::shell_dir()
        .ok()
        .map(|state| state.join("ai-window.json"));
    let min_size = (number("window.min_width")?, number("window.min_height")?);
    let size = state_path
        .as_deref()
        .and_then(crate::widgets::restored_window_size)
        .map(|(w, h)| (w.max(min_size.0), h.max(min_size.1)))
        .unwrap_or((number("window.width")?, number("window.height")?));
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.motion = None;
    fallback.primitives.clear();
    fallback.offset = (0., 0.);
    fallback.width = size.0 as f32;
    fallback.bounds = bead::render::Rect::new(0., 0., size.0 as f32, size.1 as f32);
    let cold = fallback.clone();
    let mut spec = bead::wayland::WindowSpec::new(
        "nacre-ai",
        tr("bead.panel.chat-title", &[]),
        size,
        fallback,
        Arc::new(move |_| {
            let mut session = bead::ui::Session::new(
                root.clone(),
                "ai/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            session.set_viewport(size.0, size.1);
            crate::reload::Runtime::new(session)
                .with_trace(trace)
                .with_ai_app(AiApp::new(client.clone()))
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    );
    spec.min_size = min_size;
    spec.on_resize =
        state_path.map(|path| crate::widgets::window_size_saver(path, "nacre-ai-size-save"));
    // `nacre ai [chats|projects|accounts|usage]` opens (or deep-links) a rail page.
    spec.model = Arc::new(|args| {
        args.first()
            .and_then(Json::as_str)
            .filter(|page| PAGES.contains(page))
            .map(|page| {
                BTreeMap::from([
                    ("ai.page".into(), text(page)),
                    ("ai.scroll".into(), Value::Number(0.)),
                ])
            })
            .unwrap_or_default()
    });
    Ok(spec)
}

#[cfg(test)]
#[path = "ai_app_tests.rs"]
mod tests;
