//! Nacre AI app topics and methods (N0319): `ai.providers`, `ai.projects`,
//! `ai.accounts`, the cross-provider chat list, chat start/resume and the two
//! Brain hooks. Built over `ai_providers::registry()` only; provider history is
//! the same `collect_history` the TUI uses, read in a blocking task on request
//! and never while idle. Nothing published here comes from a credential.
use super::hub::{failure, Shared};
use crate::{ai_dashboard, ai_providers, brain_data, brain_search};
use nacre_protocol::{
    ai_app::{
        Account, AccountStatus, Accounts, BrainHit, BrainHits, BrainNote, BrainSearch, Chat,
        ChatRef, ChatStart, ChatState, Chats, ChatsPage, Empty, Project, Projects, Providers,
        ACCOUNTS_TOPIC, MAX_ACCOUNTS, MAX_PROJECTS, PROJECTS_TOPIC, PROVIDERS_TOPIC,
    },
    ErrorCode, Failure, Method,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const COLLECT: Duration = Duration::from_secs(55);
const CHECK: Duration = Duration::from_secs(55);
const BRAIN: Duration = Duration::from_secs(25);

thread_local! {
    /// The last chat collection, by id; `ai.chats.resume` opens from it.
    static RECORDS: RefCell<(Vec<Chat>, BTreeMap<String, Value>)> = RefCell::default();
}
static COLLECTING: AtomicBool = AtomicBool::new(false);

pub fn methods() -> BTreeMap<String, Method> {
    nacre_protocol::ai_app::methods()
}
pub fn handles(method: &str) -> bool {
    nacre_protocol::ai_app::handles(method)
}

fn publish<T: Serialize>(hub: &Shared, topic: &str, state: &T) -> Result<bool, Failure> {
    let value = serde_json::to_value(state)
        .map_err(|_| failure(ErrorCode::Internal, "AI app state encoding failed", false))?;
    let changed = hub.borrow().snapshot(topic).as_ref() != Some(&value);
    hub.borrow_mut().publish(topic, value)?;
    Ok(changed)
}
fn unavailable(message: &str) -> Failure {
    failure(ErrorCode::Unavailable, message, true).dispatch_state(false)
}
fn invalid(message: &str) -> Failure {
    failure(ErrorCode::Validation, message, false).dispatch_state(false)
}
fn parse<T: DeserializeOwned>(params: Value) -> Result<T, Failure> {
    serde_json::from_value(params).map_err(|_| invalid("Invalid AI app request"))
}
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    limit: Duration,
    message: &str,
) -> Result<T, Failure> {
    match tokio::time::timeout(limit, tokio::task::spawn_blocking(work)).await {
        Ok(Ok(value)) => Ok(value),
        _ => Err(unavailable(message)),
    }
}

pub fn providers_state() -> Providers {
    Providers {
        providers: ai_providers::registry().iter().map(|p| p.info()).collect(),
    }
}
pub fn projects_state(registry: &[Value]) -> Projects {
    let text = |value: &Value, max: usize| -> String {
        value
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(max)
            .collect()
    };
    Projects {
        projects: registry
            .iter()
            .filter(|p| p["id"].as_str().is_some_and(|id| !id.is_empty()))
            .take(MAX_PROJECTS)
            .map(|p| Project {
                id: text(&p["id"], 128),
                label: if p["label"].as_str().is_some_and(|l| !l.is_empty()) {
                    text(&p["label"], 128)
                } else {
                    text(&p["id"], 128)
                },
                path: text(&p["path"], 4096),
                host: p["host"].as_str().map(|_| text(&p["host"], 253)),
            })
            .collect(),
    }
}
/// Profiles of every provider; `check` runs the login commands (bounded, one
/// at a time), otherwise the status stays as it was in `previous`.
pub fn accounts_state(
    providers: &[&dyn ai_providers::Provider],
    previous: &Accounts,
    check: bool,
) -> Accounts {
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let mut accounts = Vec::new();
    for provider in providers {
        let info = provider.info();
        let active = provider.active_profile().unwrap_or_default();
        for profile in provider.profiles() {
            if accounts.len() >= MAX_ACCOUNTS {
                break;
            }
            let id = if profile.is_empty() {
                "default".to_owned()
            } else {
                profile.clone()
            };
            let old = previous
                .accounts
                .iter()
                .find(|a| a.provider == info.id && a.id == id);
            let (status, checked_ms) = if check {
                (provider.login_status(&profile), Some(now))
            } else {
                old.map_or((AccountStatus::Unknown, None), |a| (a.status, a.checked_ms))
            };
            accounts.push(Account {
                provider: info.id.clone(),
                label: if profile.is_empty() {
                    "Default".into()
                } else {
                    profile.clone()
                },
                id,
                active: profile == active,
                status,
                checked_ms,
            });
        }
    }
    Accounts { accounts }
}
/// One list row from a history record, as `collect_history` returns them.
pub fn chat_row(item: &Value) -> Chat {
    let text = |value: &Value, max: usize| -> String {
        value
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(max)
            .collect()
    };
    let host = item["project"]["host"]
        .as_str()
        .map(|_| text(&item["project"]["host"], 253));
    Chat {
        id: super::ai_chat::launcher_key(item),
        provider: text(&item["agent"], 48),
        project: text(&item["project"]["id"], 128),
        resumable: host.is_none(),
        host,
        title: text(&item["title"], 256),
        state: if item["state"] == "running" {
            ChatState::Running
        } else {
            ChatState::Saved
        },
        updated_at: item["updated_at"]
            .as_f64()
            .filter(|t| t.is_finite() && *t >= 0.0)
            .unwrap_or(0.0),
        account: match item["account"].as_str() {
            Some(name) if !name.is_empty() => text(&item["account"], 64),
            _ => "Default".into(),
        },
        session: item["session"]
            .as_str()
            .map(|_| text(&item["session"], 128)),
    }
}
fn page(chats: &[Chat], offset: usize, limit: usize, warning: String) -> Chats {
    Chats {
        chats: chats.iter().skip(offset).take(limit).cloned().collect(),
        total: chats.len().min(100_000) as u32,
        warning: warning
            .chars()
            .filter(|c| !c.is_control())
            .take(2048)
            .collect(),
    }
}

pub fn initial(hub: &Shared) -> Result<(), Failure> {
    publish(hub, PROVIDERS_TOPIC, &providers_state())?;
    let registry = ai_dashboard::configured_projects().unwrap_or_default();
    publish(hub, PROJECTS_TOPIC, &projects_state(&registry))?;
    let empty = Accounts { accounts: vec![] };
    publish(
        hub,
        ACCOUNTS_TOPIC,
        &accounts_state(ai_providers::registry(), &empty, false),
    )?;
    Ok(())
}

fn previous_accounts(hub: &Shared) -> Accounts {
    hub.borrow()
        .snapshot(ACCOUNTS_TOPIC)
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(Accounts { accounts: vec![] })
}

async fn collect(cancel_message: &str) -> Result<(Vec<Value>, String), Failure> {
    if COLLECTING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(
            failure(ErrorCode::Busy, "AI chat history is being read", true).dispatch_state(false),
        );
    }
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            COLLECTING.store(false, Ordering::Release);
        }
    }
    let _release = Release;
    let (rows, _, errors) = blocking(
        || ai_dashboard::collect_history(None, None, || false),
        COLLECT,
        cancel_message,
    )
    .await?
    .map_err(|_| unavailable(cancel_message))?;
    Ok((rows, errors.join("; ")))
}

pub async fn dispatch(hub: Shared, method: &str, params: Value) -> Result<Value, Failure> {
    fn checked<T>(
        parsed: Result<T, Failure>,
        validate: impl FnOnce(&T) -> Result<(), Failure>,
    ) -> Result<T, Failure> {
        let value = parsed?;
        validate(&value).map_err(|f| f.dispatch_state(false))?;
        Ok(value)
    }
    match method {
        "ai.projects.refresh" => {
            checked(parse::<Empty>(params), |_| Ok(()))?;
            let registry = blocking(
                ai_dashboard::configured_projects,
                Duration::from_secs(4),
                "Could not read AI projects",
            )
            .await?
            .map_err(|_| unavailable("Could not read AI projects"))?;
            let changed = publish(&hub, PROJECTS_TOPIC, &projects_state(&registry))?;
            Ok(json!({"changed":changed}))
        }
        "ai.accounts.refresh" => {
            checked(parse::<Empty>(params), |_| Ok(()))?;
            let previous = previous_accounts(&hub);
            let state = blocking(
                move || accounts_state(ai_providers::registry(), &previous, true),
                CHECK,
                "Could not check AI accounts",
            )
            .await?;
            let changed = publish(&hub, ACCOUNTS_TOPIC, &state)?;
            Ok(json!({"changed":changed}))
        }
        "ai.chats.list" => {
            let page_request = checked(parse::<ChatsPage>(params), ChatsPage::validate)?;
            let (offset, limit) = (page_request.offset as usize, page_request.limit as usize);
            let warning = if offset == 0 {
                let (rows, warning) = collect("Could not read AI chat history").await?;
                let chats: Vec<Chat> = rows.iter().map(chat_row).collect();
                let by_id = chats.iter().map(|c| c.id.clone()).zip(rows).collect();
                RECORDS.with(|r| *r.borrow_mut() = (chats, by_id));
                warning
            } else {
                String::new()
            };
            let list = RECORDS.with(|r| page(&r.borrow().0, offset, limit, warning));
            list.validate().map_err(|f| f.dispatch_state(false))?;
            serde_json::to_value(list).map_err(|_| unavailable("Could not encode AI chats"))
        }
        "ai.chats.start" => {
            let request = checked(parse::<ChatStart>(params), ChatStart::validate)?;
            let provider = ai_providers::find(&request.provider)
                .ok_or_else(|| invalid("Unknown AI provider"))?;
            let registry = blocking(
                ai_dashboard::configured_projects,
                Duration::from_secs(4),
                "Could not read AI projects",
            )
            .await?
            .map_err(|_| unavailable("Could not read AI projects"))?;
            let project = registry
                .into_iter()
                .find(|p| p["id"].as_str() == Some(request.project.as_str()))
                .ok_or_else(|| invalid("Project is not registered"))?;
            // `default` names the provider's own home, which the launch paths spell "".
            let account = request
                .account
                .map(|a| if a == "default" { String::new() } else { a });
            let thread_id = provider
                .info()
                .capabilities
                .caller_session_id
                .then(|| uuid::Uuid::new_v4().to_string());
            let id = thread_id.as_deref().map(|thread| {
                let resolved = account.clone().or_else(|| provider.active_profile().ok());
                super::ai_chat::launcher_key(&json!({
                    "id":thread, "agent":request.provider, "account":resolved.unwrap_or_default(),
                    "project":{"id":project["id"],"host":project["host"],"path":project["path"]}
                }))
            });
            let project_id = request.project.clone();
            blocking(
                move || provider.start(&project_id, account.as_deref(), thread_id.as_deref()),
                Duration::from_secs(5),
                "Could not start AI chat",
            )
            .await?
            .map_err(|_| unavailable("Could not start AI chat"))?;
            Ok(match id {
                Some(id) => json!({"started":true,"id":id}),
                None => json!({"started":true}),
            })
        }
        "ai.chats.resume" => {
            let request = checked(parse::<ChatRef>(params), ChatRef::validate)?;
            let (item, unique_title) = RECORDS
                .with(|r| {
                    let records = r.borrow();
                    let (chats, by_id) = &*records;
                    let item = by_id.get(&request.id)?.clone();
                    let unique = chats.iter().filter(|c| c.title == item["title"]).count() == 1;
                    Some((item, unique))
                })
                .ok_or_else(|| invalid("Chat is not in the last list; call ai.chats.list first"))?;
            super::ai_chat::open_launcher_record(hub, item, request.id, unique_title).await
        }
        "brain.search" => {
            let request = checked(parse::<BrainSearch>(params), BrainSearch::validate)?;
            let found = blocking(
                move || {
                    let root = brain_data::root_path()?;
                    let options = brain_search::Options {
                        limit: request.limit as usize,
                        ..Default::default()
                    };
                    brain_search::search(&root, &request.query, &options)
                },
                BRAIN,
                "Brain search is unavailable",
            )
            .await?
            .map_err(|_| unavailable("Brain search is unavailable"))?;
            let hits = BrainHits {
                results: found["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|hit| {
                        let text = |key: &str, max: usize| -> String {
                            hit[key]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(max)
                                .collect()
                        };
                        let path = text("path", 4096);
                        (!path.is_empty()).then(|| BrainHit {
                            title: text("title", 256),
                            path,
                            line: hit["line"].as_u64().unwrap_or(0).min(100_000_000),
                            snippet: text("snippet", 1024),
                        })
                    })
                    .take(request.limit as usize)
                    .collect(),
            };
            hits.validate().map_err(|f| f.dispatch_state(false))?;
            serde_json::to_value(hits).map_err(|_| unavailable("Could not encode Brain results"))
        }
        "brain.note" => {
            let request = checked(parse::<BrainNote>(params), BrainNote::validate)?;
            let path = blocking(
                move || {
                    let text = request.text.trim();
                    let (title, body) = text.split_once('\n').unwrap_or((text, text));
                    let written = brain_data::write_note(
                        "inbox",
                        title.trim(),
                        &request.source,
                        "reported",
                        body.trim(),
                    )?;
                    let root = brain_data::configured_root_path()?;
                    Ok::<_, crate::Error>(
                        written
                            .strip_prefix(&root)
                            .unwrap_or(&written)
                            .to_string_lossy()
                            .into_owned(),
                    )
                },
                BRAIN,
                "Brain note could not be saved",
            )
            .await?
            .map_err(|_| unavailable("Brain note could not be saved"))?;
            Ok(json!({"path":path}))
        }
        _ => Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl ai_providers::Provider for Fake {
        fn info(&self) -> nacre_protocol::ai_app::ProviderInfo {
            nacre_protocol::ai_app::ProviderInfo {
                id: "fake".into(),
                label: "Fake".into(),
                kind: nacre_protocol::ai_app::ProviderKind::Local,
                capabilities: Default::default(),
            }
        }
        fn profiles(&self) -> Vec<String> {
            vec![String::new(), "work".into()]
        }
        fn active_profile(&self) -> crate::Result<String> {
            Ok("work".into())
        }
        fn login_status(&self, account: &str) -> AccountStatus {
            if account.is_empty() {
                AccountStatus::SignedOut
            } else {
                AccountStatus::SignedIn
            }
        }
        fn chats(&self, _: &Value, _: &str) -> crate::Result<Vec<Value>> {
            Ok(vec![])
        }
        fn start(&self, _: &str, _: Option<&str>, _: Option<&str>) -> crate::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn accounts_keep_unknown_until_checked_then_carry_the_exit_status() {
        let providers: [&dyn ai_providers::Provider; 1] = [&Fake];
        let empty = Accounts { accounts: vec![] };
        let unchecked = accounts_state(&providers, &empty, false);
        assert_eq!(unchecked.accounts.len(), 2);
        assert_eq!(unchecked.accounts[0].id, "default");
        assert_eq!(unchecked.accounts[0].label, "Default");
        assert!(!unchecked.accounts[0].active && unchecked.accounts[1].active);
        assert!(unchecked
            .accounts
            .iter()
            .all(|a| a.status == AccountStatus::Unknown && a.checked_ms.is_none()));
        let checked = accounts_state(&providers, &unchecked, true);
        assert_eq!(checked.accounts[0].status, AccountStatus::SignedOut);
        assert_eq!(checked.accounts[1].status, AccountStatus::SignedIn);
        assert!(checked.accounts[1].checked_ms.is_some());
        // An unchecked rebuild keeps the last known status.
        let kept = accounts_state(&providers, &checked, false);
        assert_eq!(kept.accounts[1].status, AccountStatus::SignedIn);
        checked.validate().unwrap();
    }

    #[test]
    fn chat_rows_map_history_records_and_remote_rows_are_not_resumable() {
        let local = json!({"id":"t1","agent":"codex","account":"","title":"Local\u{7}","state":"running",
            "session":"nacre-task-1","updated_at":1800000000.5,
            "project":{"id":"nacre","path":"/home/u/nacre"}});
        let remote = json!({"id":"t2","agent":"claude","account":"work","title":"Remote","state":"saved",
            "session":null,"updated_at":-1,"project":{"id":"nb","host":"dev-1","path":"/srv/nb"}});
        let rows = [chat_row(&local), chat_row(&remote)];
        assert_eq!(rows[0].id, super::super::ai_chat::launcher_key(&local));
        assert_eq!(rows[0].title, "Local");
        assert_eq!(rows[0].account, "Default");
        assert_eq!(rows[0].session.as_deref(), Some("nacre-task-1"));
        assert!(rows[0].resumable && rows[0].state == ChatState::Running);
        assert_eq!(rows[1].host.as_deref(), Some("dev-1"));
        assert!(!rows[1].resumable && rows[1].session.is_none());
        assert_eq!(rows[1].updated_at, 0.0);
        let list = page(&rows, 1, 5, String::new());
        assert_eq!((list.chats.len(), list.total), (1, 2));
        list.validate().unwrap();
        // The id a Claude start returns is the id the list reports for that record.
        let started = super::super::ai_chat::launcher_key(
            &json!({"id":"t2","agent":"claude","account":"work",
            "project":{"id":"nb","host":"dev-1","path":"/srv/nb"}}),
        );
        assert_eq!(started, rows[1].id);
    }

    #[test]
    fn projects_state_keeps_registry_shape_and_optional_host() {
        let state = projects_state(&[
            json!({"id":"general-chat","label":"General chat","path":"/home/u/chats","chat_directory":true}),
            json!({"id":"nb","path":"/srv/nb","host":"dev-1","build_host":"dev-1"}),
            json!({"label":"no id"}),
        ]);
        assert_eq!(state.projects.len(), 2);
        assert_eq!(state.projects[1].label, "nb");
        assert_eq!(state.projects[1].host.as_deref(), Some("dev-1"));
        assert!(state.projects[0].host.is_none());
        state.validate().unwrap();
        let registry = providers_state();
        assert_eq!(registry.providers.len(), 2);
        assert!(registry.providers[1].capabilities.caller_session_id);
        assert!(!registry.providers[0].capabilities.caller_session_id);
        registry.validate().unwrap();
    }
}
