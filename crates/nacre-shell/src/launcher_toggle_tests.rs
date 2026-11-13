// SPDX-License-Identifier: GPL-3.0-only
//! Focused coverage for the launcher's optimistic toggle lifecycle.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use tokio::sync::Notify;

const ID: &str = "style.window-shadows";

struct ToggleSource {
    publisher: source::Publisher,
    checked: AtomicBool,
    fail: AtomicBool,
    started: mpsc::Sender<()>,
    release: Notify,
}

impl ToggleSource {
    fn new(started: mpsc::Sender<()>) -> Self {
        Self {
            publisher: source::Publisher::new("toggle-fixture".into(), json!({})).unwrap(),
            checked: AtomicBool::new(false),
            fail: AtomicBool::new(false),
            started,
            release: Notify::new(),
        }
    }

    fn query_result(&self, params: &Json) -> Json {
        let checked = self.checked.load(Ordering::SeqCst);
        let status = if checked { "On" } else { "Off" };
        let mode = params["mode"].as_str().unwrap_or("apps");
        let route = params["route"].as_str().unwrap_or("apps");
        json!({
            "query": params["query"].as_str().unwrap_or(""),
            "mode": mode,
            "route": route,
            "catalog_rev": self.publisher.snapshot().revision,
            "total": 1,
            "order": [ID],
            "items": { ID: {
                "name":"Window shadows", "kind":"action", "status":status,
                "checked":checked, "disabled":false, "activation":"toggle-ticket",
                "presentation":{"toggle":true}
            }}
        })
    }
}

impl Source for ToggleSource {
    fn snapshot(&self) -> Arc<source::Snapshot> {
        self.publisher.snapshot()
    }

    fn subscribe(&self, callback: source::Callback) -> source::Result<source::Subscription> {
        self.publisher.subscribe(callback)
    }

    fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
        Box::pin(async move {
            match method.as_str() {
                "menu.query" => Ok(self.query_result(&params)),
                "menu.groups" => Ok(json!({"catalog_rev":self.publisher.snapshot().revision,"groups":[]})),
                "menu.preferences" => Ok(json!({"favorites":[],"hidden":[],"recent":[],"usage":{}})),
                "menu.activate" => {
                    self.started.send(()).map_err(|_| source::Error::new("internal", "test receiver closed", false))?;
                    self.release.notified().await;
                    if self.fail.load(Ordering::SeqCst) {
                        Err(source::Error::new("unavailable", "fixture toggle failed", true))
                    } else {
                        self.checked.store(true, Ordering::SeqCst);
                        self.publisher.publish(json!({"checked":true}))?;
                        Ok(json!({"keep_open":true}))
                    }
                }
                _ => Err(source::Error::new("unavailable", "unexpected fixture method", false)),
            }
        })
    }

    fn reconnect(&self) -> source::Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }

    fn stop(&self) -> source::Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
}

fn prepared_menu(fail: bool) -> (Menu, Arc<ToggleSource>, mpsc::Receiver<()>) {
    let (started_tx, started_rx) = mpsc::channel();
    let source = Arc::new(ToggleSource::new(started_tx));
    source.fail.store(fail, Ordering::SeqCst);
    let menu = Menu::new(Some(source.clone())).unwrap();
    let stamp = source.snapshot();
    let request = (String::new(), "apps".into(), String::new());
    {
        let mut view = menu.view.borrow_mut();
        view.request = Some(request.clone());
        view.result_request = Some(request);
        view.stamp = Some((stamp.epoch.clone(), stamp.revision, stamp.connected));
        view.result_stamp = Some((stamp.epoch.clone(), stamp.revision, stamp.connected));
        view.rows = Arc::new(vec![json!({
            "id":ID,"name":"Window shadows","kind":"action","toggle":true,
            "checked":false,"status":"Off","disabled":false
        })]);
        view.actions.insert(ID.into(), "toggle-ticket".into());
        view.catalog_rev = stamp.revision;
        view.total = 1;
    }
    (menu, source, started_rx)
}

fn stores() -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("launcher.mode".into(), Value::Text("apps".into())),
            ("launcher.search.text".into(), Value::Text(String::new())),
            ("launcher.selection".into(), Value::Text(ID.into())),
            ("launcher.engine_route".into(), Value::Text(String::new())),
            ("launcher.scroll.y".into(), Value::Number(0.)),
        ]))
        .unwrap()
}

fn first_row(stores: &Stores) -> &Json {
    match stores.get("launcher.apps") {
        Some(Value::List(rows)) => &rows[0],
        _ => panic!("launcher rows missing"),
    }
}

fn project_until(menu: &Menu, stores: &mut Stores, checked: bool, error: bool) -> BTreeMap<String, Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let delta = menu.project(stores).expect("launcher projection");
        *stores = stores.reconcile(&delta).expect("stores reconcile");
        stores.commit(delta.clone()).expect("stores commit");
        let rows = match stores.get("launcher.apps") {
            Some(Value::List(rows)) => rows,
            _ => continue,
        };
        if rows.first().is_some_and(|row| row["checked"] == checked)
            && delta.get("launcher.error").is_some_and(|value| {
                matches!(value, Value::Text(message) if message.is_empty() != error)
            })
            && delta.get("launcher.pending") != Some(&Value::Bool(true))
        {
            return delta;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("launcher toggle projection did not settle")
}

fn wait_for_toggle_reply(menu: &Menu) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if menu.pending_toggle.borrow().as_ref().is_some_and(|pending| {
            pending.completion.lock().unwrap().is_some()
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("toggle request did not complete")
}

#[test]
fn toggle_projects_optimistically_blocks_duplicates_then_confirms_from_query() {
    let (menu, source, started) = prepared_menu(false);
    let mut stores = stores();

    assert!(menu.toggle(ID).unwrap());
    started.recv_timeout(Duration::from_secs(2)).expect("toggle dispatch started");
    let optimistic = menu.project(&stores).unwrap();
    let rows = match &optimistic["launcher.apps"] {
        Value::List(rows) => rows,
        _ => panic!("launcher rows missing"),
    };
    assert_eq!(rows[0]["checked"], true);
    assert_eq!(rows[0]["status"], "On");
    assert_eq!(menu.toggle(ID).unwrap_err().code, "busy");

    source.release.notify_one();
    wait_for_toggle_reply(&menu);
    let confirmed = project_until(&menu, &mut stores, true, false);
    assert_eq!(first_row(&stores)["checked"], true);
    assert_eq!(confirmed.get("launcher.error"), Some(&Value::Text(String::new())));
}

#[test]
fn failed_toggle_rolls_back_and_surfaces_daemon_error() {
    let (menu, source, started) = prepared_menu(true);
    let mut stores = stores();

    assert!(menu.toggle(ID).unwrap());
    started.recv_timeout(Duration::from_secs(2)).expect("toggle dispatch started");
    let optimistic = menu.project(&stores).unwrap();
    let optimistic_rows = match &optimistic["launcher.apps"] {
        Value::List(rows) => rows,
        _ => panic!("launcher rows missing"),
    };
    assert_eq!(optimistic_rows[0]["checked"], true);

    source.release.notify_one();
    wait_for_toggle_reply(&menu);
    let failure = menu.project(&stores).unwrap();
    stores = stores.reconcile(&failure).unwrap();
    stores.commit(failure.clone()).unwrap();
    assert_eq!(first_row(&stores)["checked"], false);
    assert_eq!(failure.get("launcher.error"), Some(&Value::Text("fixture toggle failed".into())));
}

#[test]
fn toggle_returns_false_for_regular_rows() {
    let (menu, _source, _started) = prepared_menu(false);
    menu.view.borrow_mut().rows = Arc::new(vec![json!({"id":ID,"checked":false,"status":"Off"})]);
    assert!(!menu.toggle(ID).unwrap());
}
