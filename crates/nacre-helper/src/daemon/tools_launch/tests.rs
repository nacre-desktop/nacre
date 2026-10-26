//! Fake programs in a temp folder that record their arguments; no real application starts.
use super::*;
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, time::Duration};

struct Fixture {
    dir: tempfile::TempDir,
    hub: Shared,
}
impl Fixture {
    fn new(programs: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        for program in programs {
            let path = dir.path().join(program);
            std::fs::write(
                &path,
                format!("#!/bin/sh\necho \"{program} $@\" >> '{}'\n", log.display()),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        set_env(Some(Env {
            path: vec![dir.path().to_path_buf()],
            engine: "/opt/nacre/nacre-helper".into(),
            tools: dir.path().join("tools"),
        }));
        std::fs::create_dir_all(dir.path().join("tools")).unwrap();
        let control = dir.path().join("tools/control.sh");
        std::fs::write(
            &control,
            format!("#!/bin/sh\necho \"control.sh $@\" >> '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o755)).unwrap();
        let hub = Hub::new("tools".into(), false, Duration::from_millis(1)).shared();
        hub.borrow_mut()
            .service("capabilities", "running", 0)
            .unwrap();
        hub.borrow_mut().updates_wake = Some(std::sync::Arc::new(tokio::sync::Notify::new()));
        Self { dir, hub }
    }
    async fn launch(&self, params: Value) -> Result<Value, Failure> {
        dispatch(self.hub.clone(), params).await
    }
    fn log(&self) -> String {
        // The program runs detached: wait for the line.
        for _ in 0..200 {
            if let Ok(text) = std::fs::read_to_string(self.dir.path().join("log")) {
                if text.ends_with('\n') {
                    return text;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        String::new()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        set_env(None);
    }
}
fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn each_tool_starts_exactly_its_command() {
    run(async {
        let f = Fixture::new(&[
            "pavucontrol",
            "nm-connection-editor",
            "kitty",
            "nacre",
            "nacre-ai",
            "pkexec",
            "powerprofilesctl",
            "systemctl",
            "nacre",
        ]);
        for (tool, line) in [
            ("audio-manager", "pavucontrol \n"),
            ("connection-editor", "nm-connection-editor \n"),
            ("setup", "kitty --title Nacre setup -- nacre setup\n"),
            (
                "ai-workspace",
                "kitty --class nacre-ai-dashboard --title Nacre AI settings -- nacre-ai settings\n",
            ),
            (
                "aur-review-policy",
                "kitty --hold --title AUR review policy provenance -- /opt/nacre/nacre-helper aur-review policy\n",
            ),
            (
                "power-service",
                "pkexec systemctl start power-profiles-daemon\n",
            ),
            (
                "updates",
                "control.sh updates\n",
            ),
        ] {
            std::fs::remove_file(f.dir.path().join("log")).ok();
            assert_eq!(
                f.launch(json!({"tool":tool})).await.unwrap(),
                json!({"launched":true})
            );
            assert_eq!(f.log(), line, "{tool}");
        }
    });
}

#[test]
fn only_named_tools_start_and_missing_programs_are_unavailable() {
    run(async {
        let f = Fixture::new(&["pavucontrol", "kitty", "pkexec", "systemctl"]);
        for params in [
            json!({"tool":"bash"}),
            json!({"tool":"setup","args":["-c","id"]}),
            json!({"tool":["setup"]}),
            json!({}),
            json!("setup"),
        ] {
            let error = f.launch(params.clone()).await.unwrap_err();
            assert_eq!(
                (error.code, error.dispatched),
                (ErrorCode::Validation, Some(false)),
                "{params}"
            );
        }
        // `nm-connection-editor` is absent; `setup` needs `nacre`; the service needs the tool.
        for tool in [
            "connection-editor",
            "setup",
            "ai-workspace",
            "power-service",
        ] {
            let error = f.launch(json!({"tool":tool})).await.unwrap_err();
            assert_eq!(
                (error.code, error.dispatched),
                (ErrorCode::Unavailable, Some(false)),
                "{tool}"
            );
        }
        assert!(!f.dir.path().join("log").exists(), "nothing was started");
        assert!(available_methods(&f.hub.borrow()).contains_key("tools.launch"));
        f.hub.borrow_mut().safe = true;
        let error = f.launch(json!({"tool":"audio-manager"})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert!(available_methods(&f.hub.borrow()).is_empty());
    });
}

#[test]
fn updater_exit_notifies_the_dashboard_refresh_service() {
    run(async {
        let f = Fixture::new(&[]);
        let wake = f.hub.borrow().updates_wake.clone().unwrap();
        f.launch(json!({"tool":"updates"})).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), wake.notified())
            .await
            .expect("completed updater wakes dashboard updates refresh");
        assert_eq!(f.log(), "control.sh updates\n");
    });
}
