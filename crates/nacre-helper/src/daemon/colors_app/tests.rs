//! Fakes only: a scripted engine, a temp HOME with generated images and a real settings service
//! on temp paths. Nothing touches the owner's wallpapers, configs or session.
use super::*;
use crate::daemon::{hub::Hub, settings};
use std::os::unix::fs::PermissionsExt;
use tokio::task::LocalSet;

const ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Fixture {
    dir: tempfile::TempDir,
    hub: Shared,
}
fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(LocalSet::new().run_until(future))
}
/// A smooth gradient (small PNGs) or, with `noisy`, pseudo-random pixels (PNGs near raw size).
fn jpeg(path: &Path, width: u32, height: u32) {
    picture(path, width, height, false);
}
fn picture(path: &Path, width: u32, height: u32, noisy: bool) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_fn(width, height, |x, y| {
        let noise = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) >> 7;
        image::Rgb([
            (x * 255 / width) as u8,
            (y * 255 / height) as u8,
            if noisy {
                noise as u8
            } else {
                ((x + y) / 8) as u8
            },
        ])
    })
    .save(path)
    .unwrap();
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let root = dir.path();
        let d = root.display();
        let paths = Paths::new(root.join("home"));
        std::fs::create_dir_all(paths.state.join("wallpaper")).unwrap();
        std::fs::create_dir_all(&paths.config).unwrap();
        std::fs::write(
            root.join("engine"),
            format!(
                r#"#!/bin/sh
d='{d}'
echo "$@" >> $d/engine.log
case "$1 $2" in
 "colors catalog") cat $d/catalog.json;;
 "colors preview") read line; echo "$line" >> $d/preview.log
   if [ -f $d/slow-once ]; then rm $d/slow-once; sleep 6; fi
   if [ -f $d/refuse ]; then printf 'Could not read that image %0400d\n' 0 >&2; exit 1; fi
   cat $d/preview.json;;
 "colors apply") read line; echo "$line" > $d/apply.log; [ -f $d/hold ] && sleep 0.6
   echo '{{"applied":true,"name":"lake"}}';;
 "colors export") read line; echo "$line" > $d/export.log
   echo '{{"directory":"/x","files":["/x/palette.json"]}}';;
 "app-theme "*) echo "$@" > $d/app-theme.log; [ -f $d/refuse ] && exit 1; exit 0;;
esac
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(root.join("engine"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
        let service = settings::Service::load(hub.clone(), paths.clone(), None)
            .await
            .unwrap();
        hub.borrow_mut().settings = Some(service);
        let fixture = Self { dir, hub };
        set_env(Some(Env {
            engine: fixture.path("engine"),
            paths,
            nice: false,
        }));
        fixture
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn cache(&self) -> PathBuf {
        self.path("home/.cache/nacre")
    }
    fn write(&self, name: &str, value: &Value) {
        std::fs::write(self.path(name), value.to_string()).unwrap();
    }
    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_default()
    }
    async fn call(&self, method: &str, params: Value) -> std::result::Result<Value, Failure> {
        dispatch(self.hub.clone(), method, params).await
    }
}
fn png_size(path: &str) -> (u32, u32, u64) {
    let (w, h) = image::image_dimensions(path).unwrap();
    (w, h, std::fs::metadata(path).unwrap().len())
}
fn roles() -> Value {
    json!({"primary":"3a6ea5","secondary":"7a8fa8","tertiary":"a58a6e","surface":"10141a","onSurface":"e4e8ee"})
}
fn raw_preview(thumbnail: &str) -> Value {
    json!({"id":ID,"request":{"image":"/w/a.png","mode":"dark","compare":true},"palette":{"colours":roles(),
      "accessibility":{"textPasses":true,"minimumTextContrast":7.1,"warnings":["a / b are close"],
        "text":[{"foreground":"e4e8ee","background":"10141a","ratio":14.2,"passes":true}],
        "simulated":{"protanopia":roles(),"deuteranopia":roles(),"tritanopia":roles()},"distinctions":[]}},
      "comparisons":[{"personality":"natural","mode":"dark","colours":roles(),"minimumContrast":7.1}],
      "imageColors":[{"hex":"3a6ea5","coverage":0.42,"score":1.0,"members":[]}],"otherCoverage":0.12,
      "thumbnail":thumbnail,"image":"/w/a.png","name":"a","favorite":false,"frames":1})
}

#[test]
fn catalog_lists_entries_with_small_private_pngs_and_guards_sources() {
    run(async {
        let f = Fixture::new().await;
        let inside = f.cache().join("wallpapers/a-thumbnail.jpg");
        jpeg(&inside, 640, 400);
        // A source outside the user's cache is never read, a missing one is a placeholder.
        let outside = f.path("elsewhere.jpg");
        jpeg(&outside, 64, 40);
        let mut entries = vec![
            json!({"path":"/w/a.png","name":"a","dynamic":false,"thumbnail":inside,"families":["x"],"colors":[1]}),
            json!({"path":"/w/b.png","name":"b","dynamic":true,"thumbnail":outside}),
            json!({"path":"/w/c.png","name":"c","dynamic":false,"thumbnail":null}),
            json!({"path":"/w/d.png","name":"d","dynamic":false,"thumbnail":f.cache().join("wallpapers/none.jpg")}),
        ];
        for index in 0..600 {
            entries.push(json!({"path":format!("/w/n{index}.png"),"name":"n","dynamic":false,"thumbnail":null}));
        }
        f.write(
            "catalog.json",
            &json!({"current":"/w/a.png","entries":entries,"favorites":[{"id":"f"}],"history":[],
                "choices":{"/w/a.png":{"mode":"dark"},"/gone.png":{"mode":"light"}},"exports":"/x"}),
        );
        let reply = f.call("colors.catalog", json!({})).await.unwrap();
        let catalog: ColorsCatalog = serde_json::from_value(reply.clone()).unwrap();
        assert_eq!(catalog.current, "/w/a.png");
        assert_eq!(catalog.entries.len(), MAX_ENTRIES);
        assert!(reply["entries"][0].get("families").is_none() && reply.get("exports").is_none());
        assert!(catalog.entries[1].dynamic);
        assert_eq!(catalog.favorites.len(), 1);
        assert_eq!(catalog.choices.keys().collect::<Vec<_>>(), ["/w/a.png"]);
        let first = &catalog.entries[0].thumbnail;
        let root = f.cache().join("shell/colors-thumbs");
        assert_eq!(Path::new(first).parent().unwrap(), root);
        let (w, h, bytes) = png_size(first);
        assert!(
            w == 256 && (150..=170).contains(&h) && bytes <= 256 * 1024,
            "{w}x{h} {bytes}"
        );
        for entry in &catalog.entries[1..4] {
            assert_eq!(entry.thumbnail, "");
        }
        let meta = std::fs::metadata(&root).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        let files: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".png"))
            .collect();
        assert_eq!(files.len(), 1);
        assert!(f.text("engine.log").contains("colors catalog"));
        // Unknown parameters are refused before anything runs.
        let error = f.call("colors.catalog", json!({"x":1})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
    });
}

#[test]
fn preview_flattens_the_palette_and_keeps_one_poster() {
    run(async {
        let f = Fixture::new().await;
        let source = f.cache().join("wallpapers/a-preview.jpg");
        jpeg(&source, 1600, 1000);
        f.write("preview.json", &raw_preview(source.to_str().unwrap()));
        let request = json!({"image":"/w/a.png","mode":"dark","compare":true});
        let reply = f
            .call("colors.preview", json!({"request":request}))
            .await
            .unwrap();
        let result: ColorsPreviewResult = serde_json::from_value(reply.clone()).unwrap();
        assert_eq!(result.id, ID);
        assert_eq!(result.colours["primary"], "3a6ea5");
        assert!(result
            .audit
            .simulated
            .keys()
            .eq(["deutan", "protan", "tritan"]));
        assert_eq!(result.audit.text[0].ratio, 14.2);
        assert_eq!(result.comparisons.len(), 1);
        assert_eq!(result.image_colors.len(), 1);
        assert!(reply["imageColors"][0].get("score").is_none());
        assert!(reply.get("palette").is_none());
        let (w, h, bytes) = png_size(&result.thumbnail);
        assert_eq!((w, h), (1280, 800));
        assert!(bytes <= 2 * 1024 * 1024);
        // The engine got exactly the request object on one line.
        let sent: Value = serde_json::from_str(f.text("preview.log").trim()).unwrap();
        assert_eq!(sent, request);
        // A second preview keeps only its own poster.
        let other = f.cache().join("wallpapers/b-preview.jpg");
        jpeg(&other, 800, 500);
        f.write("preview.json", &raw_preview(other.to_str().unwrap()));
        let second: ColorsPreviewResult = serde_json::from_value(
            f.call("colors.preview", json!({"request":request}))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_ne!(second.thumbnail, result.thumbnail);
        let root = f.cache().join("shell/colors-preview");
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".png"))
                .count(),
            1
        );
        // A poster outside the cache is a placeholder, not an error.
        let outside = f.path("outside.jpg");
        jpeg(&outside, 100, 100);
        f.write("preview.json", &raw_preview(outside.to_str().unwrap()));
        let third: ColorsPreviewResult = serde_json::from_value(
            f.call("colors.preview", json!({"request":request}))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(third.thumbnail, "");
        // A poster whose PNG would pass 2 MiB at 1280 px is stepped down until it fits.
        let busy = f.cache().join("wallpapers/c-preview.jpg");
        picture(&busy, 1600, 1000, true);
        f.write("preview.json", &raw_preview(busy.to_str().unwrap()));
        let fourth: ColorsPreviewResult = serde_json::from_value(
            f.call("colors.preview", json!({"request":request}))
                .await
                .unwrap(),
        )
        .unwrap();
        let (w, _, bytes) = png_size(&fourth.thumbnail);
        assert!(w < 1280 && bytes <= 2 * 1024 * 1024, "{w} {bytes}");
    });
}

#[test]
fn preview_refusals_are_short_validation_errors_and_bad_requests_never_start_the_engine() {
    run(async {
        let f = Fixture::new().await;
        std::fs::write(f.path("refuse"), "").unwrap();
        let error = f
            .call("colors.preview", json!({"request":{"image":"/w/a.png"}}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        assert!(error.message.starts_with("Could not read that image"));
        assert!(error.message.chars().count() <= 240);
        let before = f.text("engine.log");
        for bad in [
            json!({"request":{"image":"rel.png"}}),
            json!({"request":{}}),
            json!({"request":{"image":"/a"},"extra":1}),
            json!({"request":"x"}),
        ] {
            let error = f.call("colors.preview", bad).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation);
        }
        assert_eq!(f.text("engine.log"), before);
    });
}

#[test]
fn the_newest_preview_wins_and_only_one_runs_at_a_time() {
    run(async {
        let f = Fixture::new().await;
        let source = f.cache().join("wallpapers/a-preview.jpg");
        jpeg(&source, 400, 250);
        f.write("preview.json", &raw_preview(source.to_str().unwrap()));
        std::fs::write(f.path("slow-once"), "").unwrap();
        let hub = f.hub.clone();
        let old = tokio::task::spawn_local(dispatch(
            hub.clone(),
            "colors.preview",
            json!({"request":{"image":"/w/a.png","hour":1}}),
        ));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(f.text("preview.log").contains("\"hour\":1"));
        let started = std::time::Instant::now();
        let new = f
            .call(
                "colors.preview",
                json!({"request":{"image":"/w/a.png","hour":2}}),
            )
            .await;
        assert!(new.is_ok(), "{new:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let error = old.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        assert_eq!(f.text("preview.log").lines().count(), 2);
    });
}

#[test]
fn apply_and_export_send_only_the_id_and_run_one_action_at_a_time() {
    run(async {
        let f = Fixture::new().await;
        let applied = f.call("colors.apply", json!({"id":ID})).await.unwrap();
        assert_eq!(applied, json!({"applied":true,"name":"lake"}));
        assert_eq!(f.text("apply.log").trim(), format!("{{\"id\":\"{ID}\"}}"));
        let exported = f.call("colors.export", json!({"id":ID})).await.unwrap();
        assert_eq!(exported["directory"], "/x");
        assert_eq!(exported["files"][0], "/x/palette.json");
        for bad in ["", "../x", &ID.to_uppercase(), &ID[..63]] {
            let error = f.call("colors.apply", json!({"id":bad})).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation);
        }
        assert_eq!(f.text("apply.log").lines().count(), 1);
        std::fs::write(f.path("hold"), "").unwrap();
        let hub = f.hub.clone();
        let first = tokio::task::spawn_local(dispatch(hub, "colors.apply", json!({"id":ID})));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let error = f.call("colors.export", json!({"id":ID})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        assert!(first.await.unwrap().is_ok());
        std::fs::remove_file(f.path("hold")).unwrap();
        // The lane is free again.
        f.call("colors.export", json!({"id":ID})).await.unwrap();
    });
}

#[test]
fn targets_come_from_the_published_file_and_set_target_runs_the_app_theme_command() {
    run(async {
        let f = Fixture::new().await;
        let state = f.path("home/.local/state/nacre");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("theme-targets.json"),
            json!({"version":1,"targets":[
                {"id":"gtk3","labelId":"settings.publisher.gtk3","status":"current"},
                {"id":"kitty","labelId":"missing.label","status":"tool-missing"},
                {"id":"rofi","labelId":"x","status":"paused"},
                {"id":"lock","status":"surprise"}]})
            .to_string(),
        )
        .unwrap();
        let reply = f.call("colors.targets", json!({})).await.unwrap();
        let targets: ColorsTargets = serde_json::from_value(reply).unwrap();
        let status: Vec<_> = targets
            .targets
            .iter()
            .map(|t| (t.id.as_str(), t.status))
            .collect();
        assert_eq!(
            status,
            [
                ("gtk3", TargetStatus::Current),
                ("kitty", TargetStatus::ToolMissing),
                ("rofi", TargetStatus::Paused),
                ("lock", TargetStatus::NotPublished)
            ]
        );
        assert_eq!(targets.targets[3].label, "lock");
        let changed = f
            .call("colors.set_target", json!({"id":"kitty","enabled":false}))
            .await
            .unwrap();
        assert_eq!(changed, json!({"changed":true}));
        assert_eq!(f.text("app-theme.log").trim(), "app-theme kitty false");
        // Unknown and malformed ids never reach the engine.
        for id in ["nope", "../kitty", "Kitty", ""] {
            let error = f
                .call("colors.set_target", json!({"id":id,"enabled":true}))
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation, "{id}");
        }
        assert_eq!(f.text("app-theme.log").trim(), "app-theme kitty false");
        std::fs::write(f.path("refuse"), "").unwrap();
        let error = f
            .call("colors.set_target", json!({"id":"gtk3","enabled":true}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Io);
    });
}

#[test]
fn methods_need_the_settings_service_and_never_run_in_safe_mode() {
    run(async {
        let f = Fixture::new().await;
        let names: Vec<_> = available_methods(&f.hub.borrow()).into_keys().collect();
        assert_eq!(names.len(), 6);
        assert!(names.iter().all(|n| handles(n)));
        let bare = Hub::new("bare".into(), false, Duration::from_millis(1)).shared();
        assert!(available_methods(&bare.borrow()).is_empty());
        let error = dispatch(bare, "colors.catalog", json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        let safe = Hub::new("safe".into(), true, Duration::from_millis(1)).shared();
        assert!(available_methods(&safe.borrow()).is_empty());
        for (method, params) in [
            ("colors.catalog", json!({})),
            ("colors.apply", json!({"id":ID})),
            ("colors.set_target", json!({"id":"gtk3","enabled":true})),
        ] {
            let error = dispatch(safe.clone(), method, params).await.unwrap_err();
            assert_eq!(
                (error.code, error.dispatched),
                (ErrorCode::PermissionDenied, Some(false))
            );
        }
        assert!(f.text("engine.log").is_empty());
    });
}
