use nacre_protocol::{
    artifacts, framing,
    wallpaper::{
        ActiveWallpaper, MotionKind, WallpaperActivate, WallpaperActive, WallpaperCatalogueRequest,
        WallpaperFileIdentity, WallpaperMotion,
    },
    Message, Mirror, Update, MAX_SAFE_INTEGER,
};
use serde_json::json;

#[test]
fn active_wallpaper_transcript_replays_and_validates_last_good_state() {
    let mut mirror = Mirror::default();
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/wallpaper-active.jsonl")
        .unwrap();
    for line in fixture.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        let update = mirror.receive(&message);
        assert!(!matches!(update, Update::Resync(_)), "{line}");
        if let Message::Snap { topic, .. } | Message::Patch { topic, .. } = &message {
            if topic == "wallpaper.active" {
                let state: WallpaperActive =
                    serde_json::from_value(mirror.topics[topic].1.clone()).unwrap();
                state.validate().unwrap();
            }
        }
    }
    let (seq, _) = &mirror.topics["wallpaper.active"];
    assert_eq!(*seq, 5);
}

#[test]
fn active_wallpaper_bounds_and_pairing_are_enforced() {
    let mut state: WallpaperActive = serde_json::from_value(json!({
        "available":true,"stale":false,"revision":1,
        "current":{"revision":1,"palette_revision":1,"path":"/home/user/wall.png","file_identity":{"device":"2049","inode":"123456","size":4096,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}},
        "outputs":{"DP-1":{"revision":1,"palette_revision":1,"path":"/home/user/wall.png","file_identity":{"device":"2049","inode":"123456","size":4096,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}}}
    })).unwrap();
    state.validate().unwrap();
    assert!(!state.previous_available);
    let mut stale_ready = state.clone();
    stale_ready.available = false;
    stale_ready.stale = true;
    stale_ready.previous_available = true;
    assert!(stale_ready.validate().is_err());
    let mut previous_without_current = state.clone();
    previous_without_current.current = None;
    previous_without_current.outputs.clear();
    previous_without_current.revision = 0;
    previous_without_current.available = false;
    previous_without_current.previous_available = true;
    assert!(previous_without_current.validate().is_err());
    let mut unavailable = state.clone();
    unavailable.available = false;
    unavailable.stale = true;
    unavailable.validate().unwrap();
    unavailable.stale = false;
    assert!(unavailable.validate().is_err());
    let mut empty = state.clone();
    empty.available = false;
    empty.current = None;
    empty.outputs.clear();
    empty.revision = 0;
    empty.validate().unwrap();
    empty.revision = 1;
    assert!(empty.validate().is_err());
    state.current.as_mut().unwrap().palette_revision = 2;
    assert!(state.validate().is_err());
    state.current.as_mut().unwrap().palette_revision = 1;
    state.current.as_mut().unwrap().path = "/home/user/../secret.png".into();
    assert!(state.validate().is_err());
    state.current.as_mut().unwrap().path = "https://example.test/wall.png".into();
    assert!(state.validate().is_err());
    state.current.as_mut().unwrap().path = "/home/user/wall.png".into();
    state.revision = MAX_SAFE_INTEGER + 1;
    assert!(state.validate().is_err());
    state.revision = 1;
    let mut mismatched = state.current.clone().unwrap();
    mismatched.path = "/home/user/other.png".into();
    state.outputs.insert("bad".into(), mismatched);
    assert!(state.validate().is_err());
    state.outputs.remove("bad");
    state
        .current
        .as_mut()
        .unwrap()
        .palette
        .colours
        .remove("primary");
    assert!(state.validate().is_err());
    state
        .current
        .as_mut()
        .unwrap()
        .palette
        .colours
        .insert("primary".into(), "22aa88".into());
    assert!(state.validate().is_err());
}

#[test]
fn activation_revision_is_positive_and_safe() {
    assert!(WallpaperActivate {
        revision: 1,
        id: None
    }
    .validate()
    .is_ok());
    assert!(WallpaperActivate {
        revision: 0,
        id: None
    }
    .validate()
    .is_err());
    assert!(WallpaperActivate {
        revision: MAX_SAFE_INTEGER + 1,
        id: None,
    }
    .validate()
    .is_err());
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/wallpaper-active.json",
        "protocol/schema/wallpaper-active-image.json",
        "protocol/schema/wallpaper-activate.json",
        "protocol/luau/wallpaper-active.d.luau",
        "protocol/fixtures/wallpaper-active.jsonl",
        "protocol/schema/wallpaper-catalogue.json",
        "protocol/schema/wallpaper-catalogue-request.json",
        "protocol/luau/wallpaper-catalogue.d.luau",
        "protocol/fixtures/wallpaper-catalogue.jsonl",
    ] {
        assert!(
            generated.contains_key(path),
            "missing generated contract artifact {path}"
        );
    }
    let luau = &generated["protocol/luau/wallpaper-active.d.luau"];
    for declaration in [
        "export type WallpaperFileIdentity =",
        "export type Presentation =",
        "export type PaletteMode =",
    ] {
        assert!(
            luau.contains(declaration),
            "missing Luau declaration {declaration}"
        );
    }
    assert!(generated["protocol/luau/wallpaper-catalogue.d.luau"]
        .contains("export type WallpaperCatalogueRequest ="));
    assert!(generated["protocol/luau/wallpaper-catalogue.d.luau"]
        .contains("export type WallpaperCatalogueEntry ="));
    assert!(generated["protocol/luau/wallpaper-catalogue.d.luau"]
        .contains("export type WallpaperVideoPredecode ="));
    assert!(luau.contains("export type WallpaperPalette ="));
    let catalogue_luau = &generated["protocol/luau/wallpaper-catalogue.d.luau"];
    assert!(catalogue_luau.contains("export type WallpaperCataloguePalette ="));
    let catalogue_schema: serde_json::Value =
        serde_json::from_str(&generated["protocol/schema/wallpaper-catalogue.json"]).unwrap();
    assert_eq!(
        catalogue_schema["$defs"]["WallpaperCataloguePalette"]["properties"]["colours"]
            ["maxProperties"],
        64
    );
    let active_schema: serde_json::Value =
        serde_json::from_str(&generated["protocol/schema/wallpaper-active-image.json"]).unwrap();
    assert_eq!(
        active_schema["$defs"]["WallpaperPalette"]["properties"]["colours"]["maxProperties"],
        128
    );
    assert!(
        catalogue_schema["$defs"]["WallpaperCatalogueEntry"]["properties"]
            .get("palette")
            .is_some()
    );
    let request_schema: serde_json::Value =
        serde_json::from_str(&generated["protocol/schema/wallpaper-catalogue-request.json"])
            .unwrap();
    assert_eq!(
        request_schema["properties"]["ids"]["items"]["pattern"],
        r"^/(?!.*(?:^|/)\.\.(?:/|$))(?!.*://)[^\u0000]*$"
    );
}

#[test]
fn catalogue_request_distinguishes_legacy_metadata_and_hydration_calls() {
    let legacy: WallpaperCatalogueRequest = serde_json::from_value(json!({})).unwrap();
    let metadata: WallpaperCatalogueRequest = serde_json::from_value(json!({"ids":[]})).unwrap();
    let hydration: WallpaperCatalogueRequest =
        serde_json::from_value(json!({"ids":["/pictures/lake.png"]})).unwrap();
    assert_eq!(legacy.ids, None);
    assert_eq!(metadata.ids, Some(vec![]));
    assert_eq!(hydration.ids, Some(vec!["/pictures/lake.png".into()]));
    assert!(
        legacy.validate().is_ok() && metadata.validate().is_ok() && hydration.validate().is_ok()
    );
}

#[test]
fn file_identity_is_decimal_and_bounded() {
    let mut identity = WallpaperFileIdentity {
        device: "0".into(),
        inode: "18446744073709551615".into(),
        size: 134_217_728,
        mtime_sec: -1,
        mtime_nsec: 999_999_999,
        ctime_sec: 1,
        ctime_nsec: 0,
    };
    identity.validate().unwrap();
    identity.device = "01".into();
    assert!(identity.validate().is_err());
    identity.device = "1".into();
    identity.inode = "18446744073709551616".into();
    assert!(identity.validate().is_err());
    identity.inode = "1".into();
    identity.size = MAX_SAFE_INTEGER;
    identity.validate().unwrap();
    identity.size += 1;
    assert!(identity.validate().is_err());
}

#[test]
fn motion_is_optional_validated_and_not_capped_at_the_poster_size() {
    let mut pair: ActiveWallpaper = serde_json::from_value(json!({
        "revision":1,"palette_revision":1,"path":"/home/user/poster.png","file_identity":{"device":"2049","inode":"1","size":4096,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}
    })).unwrap();
    assert!(pair.motion.is_none());
    pair.validate().unwrap();
    assert!(!serde_json::to_string(&pair).unwrap().contains("motion"));
    let mut motion: WallpaperMotion = serde_json::from_value(json!({
        "kind":"video","path":"/home/user/clip.mp4","file_identity":{"device":"2049","inode":"2","size":402653184,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0}
    })).unwrap();
    pair.motion = Some(motion.clone());
    pair.validate().unwrap();
    motion.kind = MotionKind::Animation;
    pair.motion = Some(motion.clone());
    pair.validate().unwrap();
    for bad in [
        "clip.mp4",
        "/home/user/../clip.mp4",
        "https://example.test/clip.mp4",
    ] {
        pair.motion = Some(WallpaperMotion {
            path: bad.into(),
            ..motion.clone()
        });
        assert!(pair.validate().is_err(), "{bad}");
    }
    pair.motion = None;
    pair.file_identity.size = 134_217_729;
    assert!(pair.validate().is_err(), "poster stays capped");
    assert!(serde_json::from_value::<WallpaperMotion>(
        json!({"kind":"audio","path":"/a","file_identity":motion.file_identity})
    )
    .is_err());
}
