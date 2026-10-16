//! Deterministic exports. Schemas and Luau derive from the same Rust message types.
use crate::{Message, MIN_VERSION, VERSION};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn luau(schema: &Value) -> String {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or("any").into();
    }
    for keyword in ["oneOf", "anyOf"] {
        if let Some(items) = schema.get(keyword).and_then(Value::as_array) {
            let union = items
                .iter()
                .map(|s| format!("({})", luau(s)))
                .collect::<Vec<_>>()
                .join(" | ");
            if schema.get("properties").is_some() {
                let mut base = schema.clone();
                base.as_object_mut().expect("schema object").remove(keyword);
                return format!("({}) & ({union})", luau(&base));
            }
            return union;
        }
    }
    if let Some(items) = schema.get("allOf").and_then(Value::as_array) {
        return items
            .iter()
            .map(|s| format!("({})", luau(s)))
            .collect::<Vec<_>>()
            .join(" & ");
    }
    if let Some(value) = schema.get("const") {
        return value.to_string();
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        return types
            .iter()
            .map(|t| luau(&json!({"type":t})))
            .collect::<Vec<_>>()
            .join(" | ");
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("null") => "nil".into(),
        Some("boolean") => "boolean".into(),
        Some("number" | "integer") => "number".into(),
        Some("string") => "string".into(),
        Some("array") => format!(
            "{{{}}}",
            schema
                .get("items")
                .map(luau)
                .unwrap_or_else(|| "any".into())
        ),
        Some("object") => {
            let required = schema.get("required").and_then(Value::as_array);
            let mut fields = Vec::new();
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    let optional =
                        required.is_none_or(|keys| !keys.contains(&Value::String(name.clone())));
                    let value = luau(property);
                    fields.push(format!(
                        "[{}]: {}",
                        json!(name),
                        if optional {
                            format!("({value})?")
                        } else {
                            value
                        }
                    ));
                }
            }
            if let Some(additional) = schema.get("additionalProperties").filter(|v| v.is_object()) {
                fields.push(format!("[string]: {}", luau(additional)));
            }
            if fields.is_empty() {
                "{[string]: any}".into()
            } else {
                format!("{{ {} }}", fields.join(", "))
            }
        }
        _ => "any".into(),
    }
}
pub fn transcripts() -> BTreeMap<String, String> {
    let boot = "00000000-0000-4000-8000-000000000001";
    let welcome = json!({"t":"welcome","proto":VERSION,"min":MIN_VERSION,"boot":boot,"topics":{"compositor":1},"methods":crate::core_methods(),"frame_ms":16});
    let mut out = BTreeMap::new();
    let cases = vec![
        (
            "handshake",
            vec![
                json!({"t":"hello","proto":1,"min":1,"client":"fixture","futureOptional":true}),
                welcome.clone(),
            ],
        ),
        (
            "state",
            vec![
                welcome.clone(),
                json!({"t":"snap","topic":"compositor","seq":0,"data":{"windows":{},"order":[]}}),
                json!({"t":"patch","topic":"compositor","base":0,"seq":3,"data":{"windows":{"w1":{"id":"w1","focused":true}},"order":["w1"]}}),
                json!({"t":"patch","topic":"compositor","base":1,"seq":4,"data":{"windows":{"w1":{"focused":false}}}}),
                json!({"t":"resync","topics":["compositor"]}),
                json!({"t":"snap","topic":"compositor","seq":4,"data":{"windows":{"w1":{"id":"w1","focused":false}},"order":["w1"]}}),
            ],
        ),
        (
            "null-snapshot",
            vec![
                json!({"t":"snap","topic":"settings.desktop","seq":0,"data":{}}),
                json!({"t":"snap","topic":"settings.desktop","seq":1,"data":{"optional":null}}),
            ],
        ),
        (
            "calls",
            vec![
                json!({"t":"call","id":7,"m":"daemon.ping","p":{},"timeout":1000}),
                json!({"t":"ret","id":7,"ok":null}),
                json!({"t":"call","id":"missing","m":"example.missing","p":{}}),
                json!({"t":"ret","id":"missing","err":{"code":"unknown_method","message":"Unknown method","retry":false}}),
                json!({"t":"call","id":"deadline","m":"ui.dispatch","p":{},"timeout":1}),
                json!({"t":"ret","id":"deadline","err":{"code":"timeout","message":"Call deadline reached","retry":true,"dispatched":true}}),
            ],
        ),
        (
            "events",
            vec![
                json!({"t":"ev","topic":"notifications","name":"shown","seq":1,"data":{"id":"n1"}}),
                json!({"t":"lost","topic":"notifications","from":2,"to":5}),
                json!({"t":"ev","topic":"notifications","name":"shown","seq":6,"data":{"id":"n6"}}),
                json!({"t":"ping","id":"health"}),
                json!({"t":"pong","id":"health"}),
            ],
        ),
        (
            "restart",
            vec![
                welcome,
                json!({"t":"snap","topic":"compositor","seq":20,"data":{"windows":{},"order":[]}}),
                json!({"t":"welcome","proto":1,"min":1,"boot":"00000000-0000-4000-8000-000000000002","topics":{"compositor":1},"methods":{}}),
                json!({"t":"sub","topics":["compositor"]}),
                json!({"t":"snap","topic":"compositor","seq":0,"data":{"windows":{},"order":[]}}),
            ],
        ),
        (
            "ui",
            vec![
                json!({"t":"register","provider":"ui","methods":{"ui.dispatch":1}}),
                json!({"t":"call","id":1,"m":"ui.dispatch","p":{"target":"nacre","method":"launcher","args":[""]}}),
                json!({"t":"call","id":"provider-1","m":"ui.dispatch","p":{"target":"nacre","method":"launcher","args":[""]}}),
                json!({"t":"ret","id":"provider-1","ok":null}),
                json!({"t":"ret","id":1,"ok":null}),
            ],
        ),
    ];
    for (name, steps) in cases {
        let lines = steps
            .into_iter()
            .map(|step| {
                crate::framing::decode(&serde_json::to_vec(&step).expect("finite golden"))
                    .expect("valid golden");
                serde_json::to_string(&step).expect("finite golden") + "\n"
            })
            .collect();
        out.insert(format!("protocol/fixtures/{name}.jsonl"), lines);
    }
    out
}
pub fn generate() -> BTreeMap<String, String> {
    let schema = serde_json::to_value(schemars::schema_for!(Message)).expect("finite schema");
    let mut luau_types =
        String::from("-- Generated by nacre-protocol; do not edit. GPL-3.0-only.\n");
    if let Some(definitions) = schema.get("$defs").and_then(Value::as_object) {
        for (name, value) in definitions {
            luau_types.push_str(&format!("export type {name} = {}\n", luau(value)));
        }
    }
    luau_types.push_str(&format!("export type Message = {}\n", luau(&schema)));
    luau_types.push_str("return {}\n");
    let mut out = transcripts();
    out.insert(
        "protocol/schema/message.json".into(),
        serde_json::to_string_pretty(&schema).expect("finite schema") + "\n",
    );
    out.insert("protocol/luau/nacre.d.luau".into(), luau_types);
    out.extend(menu_exports());
    out.extend(widths_exports());
    out
}

fn widths_exports() -> BTreeMap<String, String> {
    use crate::widths::{WidthsConfigure, WidthsStatus};
    let mut out = BTreeMap::new();
    let mut types = String::from("-- Generated by nacre-protocol; do not edit. GPL-3.0-only.\n");
    for (name, filename, schema) in [
        (
            "WidthsConfigure",
            "widths-configure",
            schemars::schema_for!(WidthsConfigure),
        ),
        (
            "WidthsStatus",
            "widths-status",
            schemars::schema_for!(WidthsStatus),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("finite widths schema");
        types.push_str(&format!("export type {name} = {}\n", luau(&schema)));
        out.insert(
            format!("protocol/schema/{filename}.json"),
            serde_json::to_string_pretty(&schema).expect("finite widths schema") + "\n",
        );
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/widths.d.luau".into(), types);
    let request = WidthsConfigure {
        connection_id: "fixture-compositor".into(),
        display: ":99".into(),
        opening_width: 50.0,
    };
    request.validate().expect("valid fixture");
    let status = WidthsStatus {
        available: true,
        opening_width: 50.0,
        relaxed: 0,
    };
    let frames = [
        json!({"t":"call","id":60,"m":"niri.widths.configure","p":request}),
        json!({"t":"ret","id":60,"ok":status}),
        json!({"t":"snap","topic":"niri.widths","seq":0,"data":status}),
    ];
    out.insert(
        "protocol/fixtures/widths.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("finite fixture") + "\n")
            .collect(),
    );
    out
}

/// Service-specific launcher transcripts; the original v1 wire goldens stay byte-exact.
pub fn menu_transcripts() -> BTreeMap<String, String> {
    use crate::menu::{
        MenuActivate, MenuActivated, MenuItem, MenuQuery, MenuQueryResult, MenuView,
    };
    let item = |label: &str, kind: &str| MenuItem {
        name: label.into(),
        kind: kind.into(),
        description: Some("Public fixture result".into()),
        breadcrumb: Some("All apps".into()),
        icon: Some("application-x-executable".into()),
        emoji: None,
        brand: None,
        status: None,
        checked: None,
        reason: None,
        disabled: false,
        activation: Some(format!("fixture-{kind}")),
    };
    let items = (0..320)
        .map(|i| {
            let id = format!("app-{i:04}");
            (id, item(&format!("Application {i:04}"), "app"))
        })
        .collect::<BTreeMap<_, _>>();
    let result = MenuQueryResult {
        query: "".into(),
        mode: "apps".into(),
        route: "home".into(),
        catalog_rev: 1,
        order: items.keys().cloned().collect(),
        items,
        total: Some(320),
        conversion: None,
        message: None,
    };
    let query = MenuQuery {
        query: "".into(),
        mode: "apps".into(),
        route: Some("home".into()),
        limit: Some(320),
    };
    let mut cases = vec![(
        "menu-apps",
        vec![
            json!({"t":"call","id":1,"m":"menu.query","p":query,"timeout":1000}),
            json!({"t":"ret","id":1,"ok":result}),
        ],
    )];
    let empty = MenuQueryResult {
        query: "No fixture result".into(),
        mode: "search".into(),
        route: "home".into(),
        catalog_rev: 1,
        items: BTreeMap::new(),
        order: vec![],
        total: Some(0),
        conversion: None,
        message: None,
    };
    cases.push((
        "menu-empty",
        vec![
            json!({"t":"call","id":2,"m":"menu.query","p":{"query":empty.query,"mode":"search"}}),
            json!({"t":"ret","id":2,"ok":empty}),
        ],
    ));
    let mut steps = vec![];
    // Query interpretation remains the engine's job; these are shape examples.
    for (i, (query, kind, label)) in [
        ("a", "app", "Application 0000"),
        ("> fixture", "action", "Fixture action"),
        ("=1+1", "calculation", "2"),
        ("emoji fixture", "emoji", "Smiling face"),
        ("clipboard fixture", "clipboard", "Public clip"),
        (": fixture", "file", "Fixture document"),
        ("# fixture", "brain", "Fixture note"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("result-{i}");
        let mut entry = item(label, kind);
        if kind == "emoji" {
            entry.emoji = Some("☺".into());
        }
        let result = MenuQueryResult {
            query: query.into(),
            mode: "search".into(),
            route: "home".into(),
            catalog_rev: 1,
            items: BTreeMap::from([(id.clone(), entry)]),
            order: vec![id],
            total: Some(1),
            conversion: None,
            message: None,
        };
        steps.push(
            json!({"t":"call","id":10+i,"m":"menu.query","p":{"query":query,"mode":"search"}}),
        );
        steps.push(json!({"t":"ret","id":10+i,"ok":result}));
    }
    cases.push(("menu-results", steps));
    let activate = MenuActivate {
        id: "app-0000".into(),
        activation: "fixture-app".into(),
        catalog_rev: 1,
    };
    let navigation = MenuActivated {
        keep_open: true,
        view: Some(MenuView {
            query: Some(String::new()),
            mode: Some("apps".into()),
            route: Some("group.fixture".into()),
        }),
    };
    cases.push(("menu-activation",vec![json!({"t":"call","id":30,"m":"menu.activate","p":activate,"timeout":3000}),json!({"t":"ret","id":30,"ok":MenuActivated::default()}),json!({"t":"call","id":31,"m":"menu.activate","p":{"id":"group.fixture","activation":"fixture-group","catalog_rev":1}}),json!({"t":"ret","id":31,"ok":navigation}),json!({"t":"call","id":32,"m":"menu.activate","p":{"id":"app-0000","activation":"fixture-app","catalog_rev":0}}),json!({"t":"ret","id":32,"err":{"code":"stale_revision","message":"Catalog changed; query again","retry":true}}),json!({"t":"call","id":33,"m":"menu.query","p":{"query":"fixture unavailable","mode":"search"}}),json!({"t":"ret","id":33,"err":{"code":"unavailable","message":"Launcher service unavailable","retry":true}})]));
    cases.push(("menu-conversion",vec![
        json!({"t":"call","id":40,"m":"menu.query","p":{"query":"= 10 km to miles","mode":"search"}}),
        json!({"t":"ret","id":40,"ok":{"query":"= 10 km to miles","mode":"search","route":"home","catalog_rev":1,"items":{},"order":[],"total":0,"conversion":{"ok":true,"value":"10 km = 6.213711922 mi","description":"Enter to copy"}}}),
        json!({"t":"call","id":41,"m":"menu.query","p":{"query":": fixture","mode":"search"}}),
        json!({"t":"ret","id":41,"ok":{"query":": fixture","mode":"search","route":"home","catalog_rev":1,"items":{},"order":[],"total":0,"message":"Install fd to search files"}}),
    ]));
    cases
        .into_iter()
        .map(|(name, steps)| {
            let text = steps
                .into_iter()
                .map(|step| {
                    let bytes = serde_json::to_vec(&step).expect("finite menu golden");
                    crate::framing::decode(&bytes).expect("bounded menu golden");
                    String::from_utf8(bytes).expect("JSON UTF8") + "\n"
                })
                .collect();
            (format!("protocol/fixtures/{name}.jsonl"), text)
        })
        .collect()
}

fn menu_exports() -> BTreeMap<String, String> {
    use crate::menu::{MenuActivate, MenuActivated, MenuQuery, MenuQueryResult};
    let schemas = [
        ("MenuQuery", schemars::schema_for!(MenuQuery)),
        ("MenuQueryResult", schemars::schema_for!(MenuQueryResult)),
        ("MenuActivate", schemars::schema_for!(MenuActivate)),
        ("MenuActivated", schemars::schema_for!(MenuActivated)),
    ];
    let mut out = menu_transcripts();
    let mut definitions = BTreeMap::new();
    let mut roots = vec![];
    for (name, schema) in schemas {
        let schema = serde_json::to_value(schema).expect("finite menu schema");
        if let Some(defs) = schema.get("$defs").and_then(Value::as_object) {
            for (key, value) in defs {
                definitions.insert(key.clone(), value.clone());
            }
        }
        let filename = match name {
            "MenuQuery" => "menu-query",
            "MenuQueryResult" => "menu-query-result",
            "MenuActivate" => "menu-activate",
            _ => "menu-activated",
        };
        out.insert(
            format!("protocol/schema/{filename}.json"),
            serde_json::to_string_pretty(&schema).expect("finite menu schema") + "\n",
        );
        roots.push((name, schema));
    }
    let mut types = String::from("-- Generated by nacre-protocol; do not edit. GPL-3.0-only.\n");
    for (name, schema) in definitions {
        types.push_str(&format!("export type {name} = {}\n", luau(&schema)));
    }
    for (name, schema) in roots {
        types.push_str(&format!("export type {name} = {}\n", luau(&schema)));
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/menu.d.luau".into(), types);
    out
}
