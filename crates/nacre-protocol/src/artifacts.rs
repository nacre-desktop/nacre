//! Deterministic exports. Schemas and Luau derive from the same Rust message types.
use crate::{Message, MIN_VERSION, VERSION};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn luau(schema: &Value) -> String {
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
            .map(|kind| {
                let mut branch = schema.clone();
                branch["type"] = kind.clone();
                luau(&branch)
            })
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
            let mut values = Vec::new();
            if let Some(additional) = schema.get("additionalProperties").filter(|v| v.is_object()) {
                values.push(luau(additional));
            }
            if let Some(patterns) = schema.get("patternProperties").and_then(Value::as_object) {
                values.extend(patterns.values().map(luau));
            }
            if !values.is_empty() {
                fields.push(format!("[string]: {}", values.join(" | ")));
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
                json!({"t":"ret","id":"deadline","err":{"code":"timeout","message":"UI call timed out; action may still complete","retry":false,"dispatched":true}}),
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
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
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
    out.extend(helper_exports());
    out.extend(network_exports());
    out.extend(wallpaper_exports());
    out.extend(wallpaper_active_exports());
    out.extend(crate::core_artifacts::generate());
    out.extend(compositor_exports());
    out.extend(crate::settings::exports());
    out.extend(crate::shell_actions::exports());
    out.extend(crate::bar_services::exports());
    out.extend(crate::pill_controls::exports());
    out.extend(crate::ai_chat::exports());
    out.extend(crate::dashboard_extras::exports());
    out.extend(crate::i18n::exports());
    out.extend(crate::osd::exports());
    out.extend(crate::control_center::exports());
    out.extend(crate::system_settings::exports());
    out.extend(crate::displays::exports());
    out.extend(crate::settings_asks::exports());
    out.extend(crate::colors::exports());
    out.extend(crate::weather_photo::exports());
    out
}

fn compositor_exports() -> BTreeMap<String, String> {
    use crate::compositor::{NiriAction, NiriConnection, NiriSource};
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, filename, schema) in [
        (
            "NiriSource",
            "niri-source",
            serde_json::to_value(schemars::schema_for!(NiriSource)),
        ),
        (
            "NiriConnection",
            "niri-connection",
            serde_json::to_value(schemars::schema_for!(NiriConnection)),
        ),
        (
            "NiriAction",
            "niri-action",
            serde_json::to_value(schemars::schema_for!(NiriAction)),
        ),
    ] {
        let schema = schema.expect("finite compositor schema");
        types.push_str(&format!("export type {name} = {}\n", luau(&schema)));
        out.insert(
            format!("protocol/schema/{filename}.json"),
            serde_json::to_string_pretty(&schema).expect("compositor schema") + "\n",
        );
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/compositor.d.luau".into(), types);
    let frames = [
        json!({"t":"call","id":70,"m":"niri.source","p":{"socket":"/private/niri.sock"}}),
        json!({"t":"ret","id":70,"ok":{"connection_id":"1:2"}}),
        json!({"t":"call","id":71,"m":"niri.action","p":{"connection_id":"1:2","argv":["action","reload"]}}),
        json!({"t":"ret","id":71,"ok":{"id":null,"ok":true}}),
        json!({"t":"call","id":72,"m":"niri.action","p":{"connection_id":"1:old","argv":["action","reload"]}}),
        json!({"t":"ret","id":72,"err":{"code":"permission_denied","message":"Compositor session mismatch","retry":false,"dispatched":false}}),
    ];
    out.insert(
        "protocol/fixtures/compositor-actions.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("compositor fixture") + "\n")
            .collect(),
    );
    out
}

fn wallpaper_exports() -> BTreeMap<String, String> {
    let schema = serde_json::to_value(schemars::schema_for!(crate::wallpaper::WallpaperWatch))
        .expect("finite wallpaper watch schema");
    let key = "0".repeat(64);
    let frames = [
        json!({"t":"snap","topic":"wallpaper.watch","seq":0,"data":{"available":false,"generations":{}}}),
        json!({"t":"patch","topic":"wallpaper.watch","base":0,"seq":1,"data":{"available":true,"generations":{key.clone():0}}}),
        json!({"t":"patch","topic":"wallpaper.watch","base":1,"seq":3,"data":{"generations":{key:2}}}),
        json!({"t":"patch","topic":"wallpaper.watch","base":3,"seq":4,"data":{"available":false,"error":"Wallpaper monitoring is unavailable"}}),
    ];
    BTreeMap::from([
        ("protocol/schema/wallpaper-watch.json".into(), serde_json::to_string_pretty(&schema).expect("wallpaper schema") + "\n"),
        ("protocol/luau/wallpaper.d.luau".into(), format!("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\nexport type WallpaperWatch = {}\nreturn {{}}\n", luau(&schema))),
        ("protocol/fixtures/wallpaper.jsonl".into(), frames.iter().map(|frame| serde_json::to_string(frame).expect("wallpaper fixture") + "\n").collect()),
    ])
}

fn wallpaper_active_exports() -> BTreeMap<String, String> {
    use crate::wallpaper::{
        ActiveWallpaper, PaletteMode, Presentation, WallpaperActivate, WallpaperActive,
        WallpaperCatalogue,
        WallpaperFileIdentity,
    };
    let active_schema =
        serde_json::to_value(schemars::schema_for!(WallpaperActive)).expect("active schema");
    let image_schema =
        serde_json::to_value(schemars::schema_for!(ActiveWallpaper)).expect("image schema");
    let request_schema =
        serde_json::to_value(schemars::schema_for!(WallpaperActivate)).expect("request schema");
    let catalogue_schema = serde_json::to_value(schemars::schema_for!(WallpaperCatalogue))
        .expect("catalogue schema");
    let image = json!({"revision":1,"palette_revision":1,"path":"/home/user/Pictures/wallpaper.png","file_identity":{"device":"2049","inode":"123456","size":4096,"mtime_sec":1700000000,"mtime_nsec":1,"ctime_sec":1700000000,"ctime_nsec":1},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#101820","onSurface":"#f0f4f8","primary":"#22aa88"}}});
    let mut output = BTreeMap::new();
    for (name, schema) in [
        ("wallpaper-active", &active_schema),
        ("wallpaper-active-image", &image_schema),
        ("wallpaper-activate", &request_schema),
    ] {
        output.insert(
            format!("protocol/schema/{name}.json"),
            serde_json::to_string_pretty(&schema).expect("active wallpaper schema") + "\n",
        );
    }
    let luau = format!("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\nexport type WallpaperActive = {}\nexport type ActiveWallpaper = {}\nexport type WallpaperPalette = {}\nexport type WallpaperFileIdentity = {}\nexport type Presentation = {}\nexport type PaletteMode = {}\nexport type WallpaperActivate = {}\nreturn {{}}\n", luau(&active_schema), luau(&image_schema), luau(&serde_json::to_value(schemars::schema_for!(crate::wallpaper::WallpaperPalette)).expect("palette schema")), luau(&serde_json::to_value(schemars::schema_for!(WallpaperFileIdentity)).expect("identity schema")), luau(&serde_json::to_value(schemars::schema_for!(Presentation)).expect("presentation schema")), luau(&serde_json::to_value(schemars::schema_for!(PaletteMode)).expect("palette mode schema")), luau(&request_schema));
    output.insert("protocol/luau/wallpaper-active.d.luau".into(), luau);
    output.insert(
        "protocol/schema/wallpaper-catalogue.json".into(),
        serde_json::to_string_pretty(&catalogue_schema).expect("catalogue schema") + "\n",
    );
    output.insert(
        "protocol/luau/wallpaper-catalogue.d.luau".into(),
        format!("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\nexport type WallpaperCatalogue = {}\nreturn {{}}\n", luau(&catalogue_schema)),
    );
    let catalogue = json!({"entries":[{"id":"/pictures/lake.png","kind":"image","title":"Lake","thumbnail":"/cache/nacre/wallpaper-media/abc-thumbnail.jpg","active":true},{"id":"/videos/clip.mp4","kind":"video","title":"Clip","thumbnail":"","active":false}]});
    let selection = json!({"revision":0,"id":"/pictures/lake.png"});
    output.insert(
        "protocol/fixtures/wallpaper-catalogue.jsonl".into(),
        [
            json!({"t":"call","id":1,"m":"wallpaper.catalogue","p":{}}),
            json!({"t":"ret","id":1,"ok":catalogue}),
            json!({"t":"call","id":2,"m":"wallpaper.activate","p":selection}),
            json!({"t":"ret","id":2,"ok":{"revision":1,"palette_revision":1,"path":"/cache/nacre/wallpaper-media/lake.png","file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}}}),
        ].iter().map(|frame| serde_json::to_string(frame).expect("wallpaper catalogue transcript") + "\n").collect(),
    );
    let out_state = json!({"available":true,"stale":false,"revision":1,"current":image,"outputs":{"DP-1":image},"error":null});
    let second = json!({"revision":2,"palette_revision":2,"path":"/home/user/Pictures/second.png","file_identity":{"device":"2049","inode":"123457","size":8192,"mtime_sec":1700000001,"mtime_nsec":2,"ctime_sec":1700000001,"ctime_nsec":2},"presentation":"crop","palette":{"mode":"light","colours":{"background":"#ffffff","onSurface":"#111111","primary":"#2255aa"}},"motion":{"kind":"video","path":"/home/user/Videos/clip.mp4","file_identity":{"device":"2049","inode":"123458","size":402653184,"mtime_sec":1700000001,"mtime_nsec":3,"ctime_sec":1700000001,"ctime_nsec":3}}});
    let frames = [
        json!({"t":"snap","topic":"wallpaper.active","seq":0,"data":{"available":false,"stale":false,"revision":0,"current":null,"outputs":{}}}),
        json!({"t":"patch","topic":"wallpaper.active","base":0,"seq":1,"data":out_state.clone()}),
        json!({"t":"call","id":1,"m":"wallpaper.activate","p":{"revision":1}}),
        json!({"t":"ret","id":1,"ok":image.clone()}),
        json!({"t":"patch","topic":"wallpaper.active","base":1,"seq":2,"data":{"revision":2,"current":second.clone(),"outputs":{"DP-1":second}}}),
        json!({"t":"call","id":2,"m":"wallpaper.activate","p":{"revision":1}}),
        json!({"t":"ret","id":2,"err":{"code":"stale_revision","message":"Wallpaper changed; refresh active state","retry":true}}),
        json!({"t":"patch","topic":"wallpaper.active","base":2,"seq":3,"data":{"available":false,"stale":true,"error":"Wallpaper source temporarily unavailable"}}),
        json!({"t":"call","id":3,"m":"wallpaper.activate","p":{"revision":2}}),
        json!({"t":"ret","id":3,"err":{"code":"stale_revision","message":"Wallpaper unavailable; refresh active state","retry":true}}),
        json!({"t":"patch","topic":"wallpaper.active","base":3,"seq":4,"data":{"available":true,"stale":false,"error":null}}),
        json!({"t":"patch","topic":"wallpaper.active","base":4,"seq":5,"data":{"outputs":{}}}),
    ];
    output.insert(
        "protocol/fixtures/wallpaper-active.jsonl".into(),
        frames
            .iter()
            .map(|f| serde_json::to_string(f).expect("wallpaper active fixture") + "\n")
            .collect(),
    );
    output
}

fn network_exports() -> BTreeMap<String, String> {
    let schema = serde_json::to_value(schemars::schema_for!(crate::network::NetworkWatch))
        .expect("finite network watch schema");
    let frames = [
        json!({"t":"snap","topic":"network.watch","seq":0,"data":{"available":false,"generation":0}}),
        json!({"t":"patch","topic":"network.watch","base":0,"seq":1,"data":{"available":true,"generation":1}}),
        json!({"t":"patch","topic":"network.watch","base":1,"seq":3,"data":{"generation":3}}),
        json!({"t":"patch","topic":"network.watch","base":3,"seq":4,"data":{"available":false,"error":"Network monitoring is unavailable"}}),
    ];
    BTreeMap::from([
        ("protocol/schema/network-watch.json".into(), serde_json::to_string_pretty(&schema).expect("finite network watch schema") + "\n"),
        ("protocol/luau/network.d.luau".into(), format!("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\nexport type NetworkWatch = {}\nreturn {{}}\n", luau(&schema))),
        ("protocol/fixtures/network.jsonl".into(), frames.iter().map(|frame| serde_json::to_string(frame).expect("network fixture") + "\n").collect()),
    ])
}

fn widths_exports() -> BTreeMap<String, String> {
    use crate::widths::{WidthsConfigure, WidthsStatus};
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
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
        presentation: None,
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
        filter: None,
        category: None,
        presentation: None,
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
    let palette_order = [
        (
            "nacre-menu:nacre-action:wallpaper:",
            "Open wallpaper picker",
        ),
        ("nacre-menu:nacre-action:calculator:", "Calculator"),
        ("nacre-menu:nacre-action:colors:", "Open Nacre Colors"),
    ];
    let palette_rows = palette_order
        .iter()
        .map(|(id, name)| ((*id).to_owned(), item(name, "action")))
        .collect::<BTreeMap<_, _>>();
    let palette = MenuQueryResult {
        query: String::new(),
        mode: "palette".into(),
        route: "palette".into(),
        catalog_rev: 1,
        order: palette_order
            .iter()
            .map(|(id, _)| (*id).to_owned())
            .collect(),
        items: palette_rows,
        total: Some(3),
        conversion: None,
        message: None,
    };
    cases.push((
        "menu-palette",
        vec![
            json!({"t":"call","id":3,"m":"menu.query","p":{"query":"","mode":"palette","route":"palette"}}),
            json!({"t":"ret","id":3,"ok":palette}),
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

/// New provider examples supplement, and never replace, the original menu goldens.
pub fn menu_action_transcripts() -> BTreeMap<String, String> {
    use crate::menu::{MenuUiActivate, UI_ACTIVATE};
    let selected = |row: Value| {
        let params = json!({"row":row,"catalog_rev":7});
        serde_json::from_value::<MenuUiActivate>(params.clone())
            .expect("typed selected action")
            .validate()
            .expect("bounded selected action");
        params
    };
    let copy = selected(
        json!({"id":"calculation","kind":"calculation","name":"2","operation":"copy","value":"2","disabled":false,"icon":"calculate"}),
    );
    let confirm = selected(
        json!({"id":"power.reboot","kind":"action","name":"Restart","operation":"power","value":"reboot","disabled":false,"confirm":"Restart the computer?"}),
    );
    let clipboard = selected(
        json!({"id":"actions.clipboard.fixture","kind":"clipboard","name":"Public clip","operation":"copy-clip","value":"fixture-image-1","disabled":false}),
    );
    let command = selected(
        json!({"id":"actions.fixture-command","kind":"action","name":"Literal arguments","operation":"command","arguments":["fixture-app","space & literal","$(literal)"],"directory":"/fixture","disabled":false}),
    );
    let unknown = selected(
        json!({"id":"actions.future","kind":"action","name":"Future operation","operation":"future.fixture","value":"fixture","disabled":false}),
    );
    let frames = vec![
        json!({"t":"register","provider":"ui","methods":{"ui.dispatch":1,"ui.menu.activate":1}}),
        json!({"t":"call","id":70,"m":"menu.query","p":{"query":"=1+1","mode":"search"}}),
        json!({"t":"ret","id":70,"ok":{"query":"=1+1","mode":"search","route":"home","catalog_rev":7,"items":{"nacre-menu:calculation":{"name":"2","kind":"calculation","description":"Enter to copy the result","disabled":false,"activation":"fixture-calculation-7"}},"order":["nacre-menu:calculation"],"total":1,"conversion":{"ok":true,"value":"2","error":""}}}),
        json!({"t":"call","id":71,"m":"menu.activate","p":{"id":"nacre-menu:calculation","activation":"fixture-calculation-7","catalog_rev":7}}),
        json!({"t":"call","id":"provider-1","m":UI_ACTIVATE,"p":copy}),
        json!({"t":"ret","id":"provider-1","ok":{"keep_open":false}}),
        json!({"t":"ret","id":71,"ok":{"keep_open":false}}),
        json!({"t":"call","id":72,"m":UI_ACTIVATE,"p":command}),
        json!({"t":"ret","id":72,"err":{"code":"permission_denied","message":"Use menu.activate with an engine-issued ticket","retry":false,"dispatched":false}}),
        json!({"t":"call","id":73,"m":"menu.activate","p":{"id":"nacre-menu:power.reboot","activation":"fixture-power-7","catalog_rev":7,"confirmed":true,"row":{"operation":"copy","value":"Untrusted replacement"}}}),
        json!({"t":"call","id":"provider-2","m":UI_ACTIVATE,"p":confirm}),
        json!({"t":"ret","id":"provider-2","ok":{"keep_open":true}}),
        json!({"t":"ret","id":73,"ok":{"keep_open":true}}),
        json!({"t":"call","id":"provider-3","m":UI_ACTIVATE,"p":clipboard}),
        json!({"t":"ret","id":"provider-3","ok":{"keep_open":false}}),
        json!({"t":"call","id":"provider-4","m":UI_ACTIVATE,"p":command}),
        json!({"t":"ret","id":"provider-4","ok":{"keep_open":false}}),
        json!({"t":"call","id":"provider-5","m":UI_ACTIVATE,"p":unknown}),
        json!({"t":"ret","id":"provider-5","err":{"code":"unavailable","message":"Launcher action is not supported by this UI","retry":false}}),
    ];
    let trace = frames
        .iter()
        .map(|frame| {
            let bytes = serde_json::to_vec(frame).expect("finite action golden");
            crate::framing::decode(&bytes).expect("bounded action golden");
            String::from_utf8(bytes).expect("JSON UTF8") + "\n"
        })
        .collect();
    BTreeMap::from([("protocol/fixtures/menu-ui-actions.jsonl".into(), trace)])
}

fn menu_exports() -> BTreeMap<String, String> {
    use crate::menu::{
        MenuActivate, MenuActivated, MenuGroups, MenuGroupsResult, MenuQuery, MenuQueryResult,
        MenuUiActivate,
    };
    let schemas = [
        ("MenuQuery", schemars::schema_for!(MenuQuery)),
        ("MenuQueryResult", schemars::schema_for!(MenuQueryResult)),
        ("MenuActivate", schemars::schema_for!(MenuActivate)),
        ("MenuActivated", schemars::schema_for!(MenuActivated)),
        ("MenuUiActivate", schemars::schema_for!(MenuUiActivate)),
        ("MenuGroups", schemars::schema_for!(MenuGroups)),
        ("MenuGroupsResult", schemars::schema_for!(MenuGroupsResult)),
    ];
    let mut out = menu_transcripts();
    out.extend(menu_action_transcripts());
    let groups = crate::menu::MenuGroupsResult {
        catalog_rev: 1,
        groups: ["home", "style"]
            .into_iter()
            .map(|id| crate::menu::MenuGroup {
                id: id.into(),
                label: id[..1].to_uppercase() + &id[1..],
                icon: if id == "home" { "home" } else { "palette" }.into(),
            })
            .collect(),
    };
    groups.validate().expect("bounded menu groups golden");
    out.insert(
        "protocol/fixtures/menu-groups.jsonl".into(),
        [
            json!({"t":"call","id":50,"m":"menu.groups","p":{}}),
            json!({"t":"ret","id":50,"ok":groups}),
        ]
        .iter()
        .map(|frame| serde_json::to_string(frame).expect("menu groups golden") + "\n")
        .collect(),
    );
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
            "MenuUiActivate" => "menu-ui-activate",
            "MenuGroups" => "menu-groups",
            "MenuGroupsResult" => "menu-groups-result",
            _ => "menu-activated",
        };
        out.insert(
            format!("protocol/schema/{filename}.json"),
            serde_json::to_string_pretty(&schema).expect("finite menu schema") + "\n",
        );
        roots.push((name, schema));
    }
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
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

fn helper_exports() -> BTreeMap<String, String> {
    use crate::helpers::{
        HelperCall, HelperReply, HelperRequest, HelperResult, HelperTransfer, HelperTransferRequest,
    };
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, file, schema) in [
        (
            "HelperRequest",
            "helper-request",
            schemars::schema_for!(HelperRequest),
        ),
        (
            "HelperResult",
            "helper-result",
            schemars::schema_for!(HelperResult),
        ),
        (
            "HelperCall",
            "helper-call",
            schemars::schema_for!(HelperCall),
        ),
        (
            "HelperReply",
            "helper-reply",
            schemars::schema_for!(HelperReply),
        ),
        (
            "HelperTransfer",
            "helper-transfer",
            schemars::schema_for!(HelperTransfer),
        ),
        (
            "HelperTransferRequest",
            "helper-transfer-request",
            schemars::schema_for!(HelperTransferRequest),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("helper schema");
        types.push_str(&format!("export type {name} = {}\n", luau(&schema)));
        out.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(&schema).expect("helper schema") + "\n",
        );
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/helpers.d.luau".into(), types);
    let frames = [
        json!({"t":"call","id":70,"m":"helper.execute","p":{"request":{"id":1,"tool":"notification-history","arguments":["load"],"input":null}}}),
        json!({"t":"ret","id":70,"ok":{"code":0,"output":"[]\n"}}),
        json!({"t":"call","id":71,"m":"helper.execute","p":{"file":"packet-fixture"}}),
        json!({"t":"ret","id":71,"ok":{"file":"packet-response"}}),
    ];
    out.insert(
        "protocol/fixtures/helpers.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("helper fixture") + "\n")
            .collect(),
    );
    let leases = [
        json!({"t":"call","id":73,"m":"helper.transfer","p":{"reserve":true}}),
        json!({"t":"ret","id":73,"ok":{"directory":"/private/lease","file":"packet-input"}}),
        json!({"t":"call","id":74,"m":"helper.execute","p":{"file":"packet-input"}}),
        json!({"t":"ret","id":74,"ok":{"file":"packet-output"}}),
        json!({"t":"call","id":75,"m":"helper.transfer","p":{"release":["packet-input","packet-output"]}}),
        json!({"t":"ret","id":75,"ok":{"directory":"/private/lease"}}),
    ];
    out.insert(
        "protocol/fixtures/helper-transfer.jsonl".into(),
        leases
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("lease fixture") + "\n")
            .collect(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::luau;
    use serde_json::json;

    #[test]
    fn nullable_collections_retain_item_and_property_types() {
        assert_eq!(
            luau(&json!({"type":["array","null"],
                "items":{"$ref":"#/$defs/EntityId"}})),
            "{EntityId} | nil"
        );
        assert_eq!(
            luau(&json!({"type":["object","null"],
                "properties":{"ready":{"type":"boolean"}},"required":["ready"]})),
            "{ [\"ready\"]: boolean } | nil"
        );
    }
}
