//! Typed producer contract for the daemon notifications service.
use crate::core_services::*;
use crate::{Capability, Method, UI_CALL_TIMEOUT_MS};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoticeRevision {
    pub id: EntityId,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub state_seq: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    Expired,
    Dismissed,
    Application,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoticeClosed {
    pub id: EntityId,
    pub reason: CloseReason,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub state_seq: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoticeAction {
    pub id: EntityId,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    pub action: EntityId,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub state_seq: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DndChanged {
    pub enabled: bool,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub state_seq: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateChanged {
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub state_seq: u64,
}

/// Event payloads contain identifiers and state revisions only, never notice text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "name", content = "data", rename_all = "snake_case")]
pub enum NotificationEvent {
    Shown(NoticeRevision),
    Updated(NoticeRevision),
    Closed(NoticeClosed),
    Action(NoticeAction),
    DndChanged(DndChanged),
    HistoryChanged(StateChanged),
    PresentationChanged(StateChanged),
}

impl NotificationEvent {
    pub fn valid(&self) -> bool {
        match self {
            Self::Shown(value) | Self::Updated(value) => {
                value.id.valid()
                    && value.revision > 0
                    && crate::core_services::timestamp_valid(value.revision)
                    && crate::core_services::timestamp_valid(value.state_seq)
            }
            Self::Closed(value) => {
                value.id.valid() && crate::core_services::timestamp_valid(value.state_seq)
            }
            Self::Action(value) => {
                value.id.valid()
                    && value.action.valid()
                    && value.revision > 0
                    && crate::core_services::timestamp_valid(value.revision)
                    && crate::core_services::timestamp_valid(value.state_seq)
            }
            Self::DndChanged(value) => crate::core_services::timestamp_valid(value.state_seq),
            Self::HistoryChanged(value) | Self::PresentationChanged(value) => {
                crate::core_services::timestamp_valid(value.state_seq)
            }
        }
    }
}

/// Conditional registry entries. A daemon advertises these only with usable handlers.
pub fn method_descriptors() -> BTreeMap<String, Method> {
    [
        ("notifications.set_dnd", schema::<NotificationDnd>()),
        ("notifications.dismiss", schema::<NotificationDismiss>()),
        ("notifications.clear", schema::<NotificationClear>()),
        (
            "notifications.invoke_action",
            schema::<NotificationActionCall>(),
        ),
        ("notifications.set_hover", schema::<NotificationHover>()),
        (
            "notifications.set_inhibited",
            schema::<NotificationInhibited>(),
        ),
    ]
    .into_iter()
    .map(|(name, params)| {
        (
            name.into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: UI_CALL_TIMEOUT_MS,
                params,
                result: serde_json::to_value(schemars::schema_for!(CoreResult))
                    .expect("finite notifications result schema"),
            },
        )
    })
    .collect()
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("finite notifications parameter schema")
}

pub(crate) fn transcripts() -> BTreeMap<String, String> {
    use serde_json::json;
    let notice = json!({"app_name":"Fixture","summary":"Original","body":"Safe fixture text","urgency":"normal","resident":false,"transient":false,"created_ms":1800000000000_u64,"actions":{"items":{"a1":{"label":"Open"},"a2":{"label":"Snooze"}},"order":["a1","a2"]},"default_action":"a1","timeout_ms":5000,"remaining_ms":5000,"expires_ms":1800000005000_u64,"revision":1,"retained":true,"paused":false});
    let active = |id: &str, item: Value| {
        let mut items = serde_json::Map::new();
        items.insert(id.to_owned(), item);
        json!({"items":items,"order":[id]})
    };
    let empty = json!({"items":{},"order":[]});
    let snapshot = |active: Value, history: Value, extra: Value| {
        let mut state = json!({"health":{"available":true,"stale":false},"do_not_disturb":false,"active":active,"history":history,"popup_order":[],"inhibited":false,"history_count":1,"history_more":false,"history_storage":{"ready":true,"writable":true}});
        if let Some(fields) = extra.as_object() {
            for (key, value) in fields {
                state[key] = value.clone();
            }
        }
        state
    };
    let mut closed_notice = notice.clone();
    closed_notice["summary"] = json!("Replacement");
    closed_notice["created_ms"] = json!(1800000001000_u64);
    closed_notice["revision"] = json!(2);
    closed_notice["actions"] = empty.clone();
    closed_notice["default_action"] = Value::Null;
    closed_notice["paused"] = json!(false);
    closed_notice
        .as_object_mut()
        .unwrap()
        .remove("remaining_ms");
    closed_notice.as_object_mut().unwrap().remove("expires_ms");
    let mut frames = vec![
        json!({"t":"register","provider":"ui","methods":{"ui.dispatch":1}}),
        json!({"t":"snap","topic":"notifications","seq":0,"data":snapshot(active("notice-b1-g1-n1",notice.clone()),empty.clone(),json!({"popup_order":["notice-b1-g1-n1"]}))}),
        json!({"t":"patch","topic":"notifications","base":0,"seq":1,"data":{"active":{"items":{"notice-b1-g1-n1":{"summary":"Replacement","created_ms":1800000001000_u64,"revision":2,"actions":{"items":{"a1":null,"a2":{"label":"Open updated"}},"order":["a2"]},"default_action":"a2"}},"order":["notice-b1-g1-n1"]}}}),
        json!({"t":"ev","topic":"notifications","name":"updated","seq":1,"data":{"id":"notice-b1-g1-n1","revision":2,"state_seq":1}}),
        call(
            19,
            "notifications.invoke_action",
            json!({"id":"notice-b1-g1-n1","revision":1,"action":"a1"}),
        ),
        error_ret(19, "stale_revision", true),
        call(
            11,
            "notifications.set_hover",
            json!({"id":"notice-b1-g1-n1","revision":2,"hovered":true}),
        ),
        json!({"t":"patch","topic":"notifications","base":1,"seq":2,"data":{"active":{"items":{"notice-b1-g1-n1":{"remaining_ms":3000,"paused":true,"expires_ms":null}},"order":["notice-b1-g1-n1"]}}}),
        json!({"t":"ev","topic":"notifications","name":"presentation_changed","seq":2,"data":{"state_seq":2}}),
        ret(11),
        call(
            12,
            "notifications.set_hover",
            json!({"id":"notice-b1-g1-n1","revision":2,"hovered":false}),
        ),
        json!({"t":"patch","topic":"notifications","base":2,"seq":3,"data":{"active":{"items":{"notice-b1-g1-n1":{"remaining_ms":3000,"paused":false,"expires_ms":1800000005000_u64}},"order":["notice-b1-g1-n1"]}}}),
        json!({"t":"ev","topic":"notifications","name":"presentation_changed","seq":3,"data":{"state_seq":3}}),
        ret(12),
        call(20, "notifications.set_dnd", json!({"enabled":true})),
        json!({"t":"patch","topic":"notifications","base":3,"seq":4,"data":{"do_not_disturb":true,"inhibited":true,"popup_order":[]}}),
        json!({"t":"ev","topic":"notifications","name":"dnd_changed","seq":4,"data":{"enabled":true,"state_seq":4}}),
        ret(20),
        call(21, "notifications.set_dnd", json!({"enabled":false})),
        json!({"t":"patch","topic":"notifications","base":4,"seq":5,"data":{"do_not_disturb":false,"inhibited":false,"popup_order":[]}}),
        json!({"t":"ev","topic":"notifications","name":"dnd_changed","seq":5,"data":{"enabled":false,"state_seq":5}}),
        ret(21),
        json!({"t":"register","provider":"ui","methods":{"ui.dispatch":1}}),
        call(22, "notifications.set_inhibited", json!({"inhibited":true})),
        json!({"t":"patch","topic":"notifications","base":5,"seq":6,"data":{"inhibited":true,"popup_order":[]}}),
        json!({"t":"ev","topic":"notifications","name":"presentation_changed","seq":6,"data":{"state_seq":6}}),
        ret(22),
        call(
            23,
            "notifications.set_inhibited",
            json!({"inhibited":false}),
        ),
        json!({"t":"patch","topic":"notifications","base":6,"seq":7,"data":{"inhibited":false,"popup_order":[]}}),
        json!({"t":"ev","topic":"notifications","name":"presentation_changed","seq":7,"data":{"state_seq":7}}),
        ret(23),
        json!({"t":"call","id":1,"m":"notifications.invoke_action","p":{"id":"notice-b1-g1-n1","revision":2,"action":"a2"},"timeout":3000}),
        json!({"t":"patch","topic":"notifications","base":7,"seq":8,"data":{"active":{"items":{"notice-b1-g1-n1":null},"order":[]},"history":{"items":{"notice-b1-g1-n1":closed_notice},"order":["notice-b1-g1-n1"]},"popup_order":[],"history_count":1}}),
        json!({"t":"ev","topic":"notifications","name":"action","seq":8,"data":{"id":"notice-b1-g1-n1","revision":2,"action":"a2","state_seq":8}}),
        json!({"t":"ev","topic":"notifications","name":"closed","seq":9,"data":{"id":"notice-b1-g1-n1","reason":"dismissed","state_seq":8}}),
        json!({"t":"ret","id":1,"ok":{"changed":true}}),
        call(
            2,
            "notifications.dismiss",
            json!({"id":"notice-b1-g1-n1","revision":2,"forget":true}),
        ),
        json!({"t":"patch","topic":"notifications","base":8,"seq":9,"data":{"history":{"items":{"notice-b1-g1-n1":null},"order":[]},"history_count":0}}),
        json!({"t":"ev","topic":"notifications","name":"history_changed","seq":10,"data":{"state_seq":9}}),
        ret(2),
    ];
    let replacement_popup = {
        let mut n = notice.clone();
        n["summary"] = json!("Transient arrival");
        n["revision"] = json!(1);
        n["transient"] = json!(true);
        n["retained"] = json!(false);
        n
    };
    frames.extend([
        json!({"t":"patch","topic":"notifications","base":9,"seq":10,"data":{"active":{"items":{"notice-b1-g1-n2":replacement_popup},"order":["notice-b1-g1-n2"]},"history_count":0,"popup_order":["notice-b1-g1-n2"]}}),
        json!({"t":"ev","topic":"notifications","name":"shown","seq":11,"data":{"id":"notice-b1-g1-n2","revision":1,"state_seq":10}}),
        json!({"t":"patch","topic":"notifications","base":10,"seq":11,"data":{"active":{"items":{"notice-b1-g1-n2":null},"order":[]},"history_count":0,"popup_order":[]}}),
        json!({"t":"ev","topic":"notifications","name":"closed","seq":12,"data":{"id":"notice-b1-g1-n2","reason":"expired","state_seq":11}}),
    ]);

    let controls_state = snapshot(
        active("n1", notice.clone()),
        empty.clone(),
        json!({"popup_order":["n1"]}),
    );
    let mut controls_closed_notice = notice.clone();
    controls_closed_notice["actions"] = empty.clone();
    controls_closed_notice["default_action"] = Value::Null;
    controls_closed_notice
        .as_object_mut()
        .unwrap()
        .remove("remaining_ms");
    controls_closed_notice
        .as_object_mut()
        .unwrap()
        .remove("expires_ms");
    let controls = vec![
        json!({"t":"snap","topic":"notifications","seq":0,"data":controls_state}),
        call(1, "notifications.set_dnd", json!({"enabled":false})),
        ret_noop(1),
        call(2, "notifications.dismiss", json!({"id":"n1","revision":1})),
        json!({"t":"patch","topic":"notifications","base":0,"seq":1,"data":{"active":{"items":{"n1":null},"order":[]},"history":{"items":{"n1":controls_closed_notice},"order":["n1"]},"popup_order":[],"history_count":1}}),
        json!({"t":"ev","topic":"notifications","name":"closed","seq":1,"data":{"id":"n1","reason":"dismissed","state_seq":1}}),
        ret(2),
        call(3, "notifications.clear", json!({"scope":"history"})),
        json!({"t":"patch","topic":"notifications","base":1,"seq":2,"data":{"history":{"items":{"n1":null},"order":[]},"history_count":0}}),
        json!({"t":"ev","topic":"notifications","name":"history_changed","seq":2,"data":{"state_seq":2}}),
        ret(3),
        call(30, "notifications.clear", json!({"scope":"retained"})),
        ret_noop(30),
        call(31, "notifications.clear", json!({"scope":"popups"})),
        ret_noop(31),
        call(
            4,
            "notifications.invoke_action",
            json!({"id":"n1","revision":1,"action":"a1"}),
        ),
        error_ret(4, "stale_revision", true),
        call(
            5,
            "notifications.set_hover",
            json!({"id":"n1","revision":1,"hovered":true}),
        ),
        error_ret(5, "permission_denied", false),
        call(6, "notifications.set_inhibited", json!({"inhibited":true})),
        error_ret(6, "permission_denied", false),
    ];
    let errors = vec![
        call(1, "notifications.set_dnd", json!({"enabled":"bad"})),
        error_ret(1, "validation", false),
        call(
            2,
            "notifications.dismiss",
            json!({"id":"stale","revision":2}),
        ),
        error_ret(2, "stale_revision", true),
        call(3, "notifications.clear", json!({"scope":"retained"})),
        error_ret(3, "permission_denied", false),
        call(
            4,
            "notifications.invoke_action",
            json!({"id":"n1","revision":1,"action":"a1"}),
        ),
        error_ret(4, "unavailable", true),
        call(
            5,
            "notifications.set_hover",
            json!({"id":"n1","revision":1,"hovered":true}),
        ),
        error_ret(5, "busy", true),
        call(6, "notifications.set_inhibited", json!({"inhibited":true})),
        error_ret(6, "timeout", true),
        json!({"t":"call","id":7,"m":"notifications.dismiss","p":{"id":"n".repeat(8193),"revision":1}}),
        error_ret(7, "too_large", false),
        json!({"t":"call","id":8,"m":"notifications.clear","p":{"scope":"history"}}),
        error_ret(8, "io", true),
        json!({"t":"call","id":9,"m":"notifications.clear","p":{"scope":"popups"}}),
        error_ret(9, "internal", false),
    ];
    let unknown_history = json!({"health":{"available":true,"stale":false},"do_not_disturb":false,"active":empty,"history":empty,"popup_order":[],"inhibited":false,"history_storage":{"ready":false,"writable":false}});
    let mut restored_notice = notice.clone();
    restored_notice["actions"] = empty.clone();
    restored_notice["default_action"] = Value::Null;
    restored_notice["paused"] = json!(false);
    restored_notice
        .as_object_mut()
        .unwrap()
        .remove("remaining_ms");
    restored_notice
        .as_object_mut()
        .unwrap()
        .remove("expires_ms");
    let restored_id = "notice-b2-g2-restored1";
    let mut window_items = serde_json::Map::new();
    let mut window_order = Vec::new();
    for index in 0..64 {
        let id = format!("notice-b1-g1-history{index}");
        window_items.insert(id.clone(), restored_notice.clone());
        window_order.push(id);
    }
    let window = json!({"items":window_items,"order":window_order});
    let restored_history = json!({"items":{restored_id:restored_notice},"order":[restored_id]});
    let old_state = snapshot(
        active("notice-b1-g1-n1", notice.clone()),
        empty.clone(),
        json!({"popup_order":["notice-b1-g1-n1"]}),
    );
    let methods = method_descriptors();
    let loss_restart = vec![
        json!({"t":"welcome","proto":1,"min":1,"boot":"b1","topics":{"notifications":1},"methods":methods}),
        json!({"t":"sub","topics":["notifications"]}),
        json!({"t":"snap","topic":"notifications","seq":0,"data":old_state}),
        json!({"t":"patch","topic":"notifications","base":0,"seq":3,"data":{"active":{"items":{"notice-b1-g1-n1":{"summary":"Coalesced update","revision":2,"created_ms":1800000001000_u64,"actions":{"items":{"a1":null,"a2":{"label":"Open updated"}},"order":["a2"]},"default_action":"a2"}}}}}}),
        json!({"t":"patch","topic":"notifications","base":1,"seq":4,"data":{"obsolete":true}}),
        json!({"t":"lost","topic":"notifications","from":3,"to":7}),
        json!({"t":"resync","topics":["notifications"]}),
        json!({"t":"welcome","proto":1,"min":1,"boot":"b2","topics":{"notifications":1},"methods":method_descriptors()}),
        json!({"t":"sub","topics":["notifications"]}),
        json!({"t":"snap","topic":"notifications","seq":0,"data":snapshot(empty.clone(),restored_history,json!({"popup_order":[],"history_count":1,"history_more":false}))}),
    ];
    let cases = [
        ("lifecycle", frames),
        ("controls", controls),
        ("errors", errors),
        ("loss-restart", loss_restart),
        (
            "ownership",
            vec![json!({"t":"welcome","proto":1,"min":1,"boot":"b1","topics":{},"methods":{}})],
        ),
        (
            "storage",
            vec![
                json!({"t":"snap","topic":"notifications","seq":0,"data":unknown_history}),
                json!({"t":"patch","topic":"notifications","base":0,"seq":1,"data":{"history_storage":{"ready":true,"writable":false,"error":{"code":"io","message":"History storage unavailable","retry":true}},"history_count":0,"history_more":false}}),
                json!({"t":"patch","topic":"notifications","base":1,"seq":2,"data":{"history_storage":{"writable":true,"error":null},"history":window,"history_count":65,"history_more":true}}),
            ],
        ),
    ];
    cases
        .into_iter()
        .map(|(name, frames)| {
            let content = frames
                .into_iter()
                .map(|frame| serde_json::to_string(&frame).unwrap())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            (
                format!("protocol/fixtures/notifications-{name}.jsonl"),
                content,
            )
        })
        .collect()
}

fn call(id: u64, method: &str, params: Value) -> Value {
    serde_json::json!({"t":"call","id":id,"m":method,"p":params,"timeout":3000})
}
fn ret(id: u64) -> Value {
    serde_json::json!({"t":"ret","id":id,"ok":{"changed":true}})
}
fn ret_noop(id: u64) -> Value {
    serde_json::json!({"t":"ret","id":id,"ok":{"changed":false}})
}
fn error_ret(id: u64, code: &str, retry: bool) -> Value {
    serde_json::json!({"t":"ret","id":id,"err":{"code":code,"message":"Request failed","retry":retry}})
}
