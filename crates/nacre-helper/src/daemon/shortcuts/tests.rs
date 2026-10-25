//! A fake reader stands in for the compositor; nothing is queried.
use super::*;
use std::sync::Mutex;
static RUN_LOCK: Mutex<()> = Mutex::new(());

fn hub() -> Shared {
    Hub::new("shortcuts".into(), false, Duration::from_millis(1)).shared()
}
fn state(hub: &Shared) -> ShortcutsState {
    serde_json::from_value(hub.borrow().snapshot(SHORTCUTS_TOPIC).unwrap()).unwrap()
}

#[test]
fn rows_become_bounded_display_text_in_order() {
    let rows = json!([
        {"key":"SUPER+SPACE","description":"Launcher","submap":"","menu":""},
        {"key":"","description":"no key is skipped"},
        {"key":"SUPER+PERIOD","description":"Emoji\nlist","submap":"resize","menu":"actions.emoji"},
        {"key":"K".repeat(100),"description":"d".repeat(400)},
        "not a row",
    ]);
    let all = items(&rows);
    assert_eq!(
        all.order.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        ["k0", "k1", "k2"]
    );
    let launcher = &all.items[&EntityId("k0".into())];
    assert_eq!(
        (launcher.submap.clone(), launcher.menu.clone()),
        (None, None)
    );
    let emoji = &all.items[&EntityId("k1".into())];
    assert_eq!(emoji.description, "Emoji list");
    assert_eq!(emoji.submap.as_deref(), Some("resize"));
    assert_eq!(emoji.menu.as_deref(), Some("actions.emoji"));
    let long = &all.items[&EntityId("k2".into())];
    assert_eq!(
        (long.key.chars().count(), long.description.chars().count()),
        (64, 256)
    );
    let many: Vec<Value> = (0..600)
        .map(|n| json!({"key":format!("F{n}"),"description":"x"}))
        .collect();
    assert_eq!(items(&Value::Array(many)).order.len(), MAX_SHORTCUTS);
    let state = ShortcutsState {
        health: health(true, false),
        items: all,
    };
    state.validate().unwrap();
}

fn runtime<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::task::LocalSet::new()
                .run_until(tokio::time::timeout(Duration::from_secs(30), future))
                .await
        })
        .unwrap()
}
async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out: {what}");
}

#[test]
fn the_topic_waits_for_the_compositor_reads_on_refresh_and_keeps_the_last_good_list() {
    let _guard = RUN_LOCK.lock().unwrap();
    runtime(async {
        let hub = hub();
        initial(&hub).unwrap();
        assert!(!state(&hub).health.available);
        // (rows to answer, reads so far, fail the next read)
        let script = Arc::new(Mutex::new((
            json!([{"key":"SUPER+A","description":"First"}]),
            0usize,
            false,
        )));
        let fake = script.clone();
        let env = Env {
            files: Arc::new(|| Ok(vec![])),
            read: Arc::new(move || {
                let mut script = fake.lock().unwrap();
                script.1 += 1;
                if script.2 {
                    Err("compositor gone".into())
                } else {
                    Ok(script.0.clone())
                }
            }),
        };
        hub.borrow_mut()
            .publish("compositor", json!({"available":false}))
            .unwrap();
        let task = tokio::task::spawn_local(run_with(hub.clone(), env));
        until("running", || {
            hub.borrow().services.contains_key("shortcuts")
        })
        .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            script.lock().unwrap().1,
            0,
            "nothing is read before the compositor is up"
        );

        hub.borrow_mut()
            .publish("compositor", json!({"available":true}))
            .unwrap();
        until("first read", || state(&hub).items.order.len() == 1).await;
        assert!(state(&hub).health.available && !state(&hub).health.stale);

        // Compositor topic churn alone does not read again.
        hub.borrow_mut()
            .publish("compositor", json!({"available":true,"windows":{"w":1}}))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(script.lock().unwrap().1, 1);

        script.lock().unwrap().0 = json!([
            {"key":"SUPER+A","description":"First"},
            {"key":"SUPER+B","description":"Second"}
        ]);
        assert_eq!(
            dispatch(&hub.borrow(), json!({})).unwrap(),
            json!({"requested":true})
        );
        until("refresh", || state(&hub).items.order.len() == 2).await;
        assert_eq!(script.lock().unwrap().1, 2);

        // A failed read keeps the last good list, visibly stale.
        script.lock().unwrap().2 = true;
        dispatch(&hub.borrow(), json!({})).unwrap();
        until("stale", || state(&hub).health.stale).await;
        assert_eq!(state(&hub).items.order.len(), 2);
        assert!(state(&hub).health.error.is_some());
        script.lock().unwrap().2 = false;
        dispatch(&hub.borrow(), json!({})).unwrap();
        until("recovered", || !state(&hub).health.stale).await;

        assert!(available_methods(&hub.borrow()).contains_key("shortcuts.refresh"));
        assert_eq!(
            dispatch(&hub.borrow(), json!({"x":1})).unwrap_err().code,
            ErrorCode::Validation
        );
        hub.borrow_mut().safe = true;
        assert!(available_methods(&hub.borrow()).is_empty());
        assert_eq!(
            dispatch(&hub.borrow(), json!({})).unwrap_err().code,
            ErrorCode::Unavailable
        );
        task.abort();
    });
}

#[test]
fn config_edits_replacements_and_missing_file_creation_refresh_the_topic() {
    let _guard = RUN_LOCK.lock().unwrap();
    runtime(async {
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join("config.kdl");
        let custom_dir = home.path().join("custom");
        std::fs::create_dir(&custom_dir).unwrap();
        let include = custom_dir.join("custom.kdl");
        std::fs::write(&config, "include optional=true \"custom/custom.kdl\"\n").unwrap();
        let hub = hub();
        initial(&hub).unwrap();
        hub.borrow_mut()
            .publish("compositor", json!({"available":true}))
            .unwrap();
        let read_home = home.path().to_path_buf();
        let read_config = config.clone();
        let watched = config.clone();
        let watch_home = home.path().to_path_buf();
        let env = Env {
            read: Arc::new(move || {
                let paths = Paths::new(&read_home);
                let mut backend = crate::compositor::backend::Backend::new(
                    paths.clone(),
                    BTreeMap::from([
                        ("NACRE_COMPOSITOR".into(), "niri".into()),
                        (
                            "NIRI_CONFIG".into(),
                            read_config.to_string_lossy().into_owned(),
                        ),
                    ]),
                )?;
                crate::desktop_extras::bindings_from(&mut backend, &paths)
            }),
            files: Arc::new(move || {
                crate::compositor::niri::Backend::new(
                    Paths::new(&watch_home),
                    BTreeMap::from([(
                        "NIRI_CONFIG".into(),
                        watched.to_string_lossy().into_owned(),
                    )]),
                )
                .binding_source_files()
                .map_err(Into::into)
            }),
        };
        let task = tokio::task::spawn_local(run_with(hub.clone(), env));
        until("empty initial read", || state(&hub).health.available).await;
        std::fs::write(
            &include,
            "binds { Super+A hotkey-overlay-title=\"First\" { close-window; }; }\n",
        )
        .unwrap();
        until("missing include created", || {
            state(&hub).items.order.len() == 1
        })
        .await;
        std::fs::write(&include, "binds { Super+B { close-window; }; }\n").unwrap();
        until("in-place edit", || {
            state(&hub).items.items.values().any(|r| r.key == "SUPER+B")
        })
        .await;
        let replacement = home.path().join("replacement.kdl");
        std::fs::write(&replacement, "binds { Super+C { close-window; }; }\n").unwrap();
        std::fs::rename(replacement, &include).unwrap();
        until("atomic replacement", || {
            state(&hub).items.items.values().any(|r| r.key == "SUPER+C")
        })
        .await;
        std::fs::write(&include, "binds {").unwrap();
        until("invalid included file is stale", || {
            state(&hub).health.stale
        })
        .await;
        std::fs::write(&include, "binds { Super+D { close-window; }; }\n").unwrap();
        until("invalid included file fixed", || {
            !state(&hub).health.stale
                && state(&hub).items.items.values().any(|r| r.key == "SUPER+D")
        })
        .await;
        task.abort();
    });
}
