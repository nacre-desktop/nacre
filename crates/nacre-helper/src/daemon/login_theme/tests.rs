//! Fakes only: `pkexec` and the helper engine are scripts, the theme is a temp folder. The real
//! pkexec, polkit, SDDM files and /usr/share are never touched.
use super::*;
use crate::daemon::hub::Hub;
use std::os::unix::fs::PermissionsExt;

struct Fixture {
    dir: tempfile::TempDir,
}
fn script(path: &std::path::Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().display().to_string();
        std::fs::create_dir_all(dir.path().join("login")).unwrap();
        std::fs::write(dir.path().join("login/Main.qml"), "").unwrap();
        std::fs::write(dir.path().join("login-install.py"), "").unwrap();
        std::fs::write(dir.path().join("python3"), "").unwrap();
        script(
            &dir.path().join("pkexec"),
            &format!(
                "echo \"$@\" >> '{d}/pkexec.log'\nwhile [ -f '{d}/hold' ]; do sleep 0.05; done\n[ -f '{d}/dismiss' ] && exit 126\n[ -f '{d}/fail' ] && {{ echo SECRETOUT; exit 1; }}\n[ -f '{d}/quiet' ] && exit 0\necho 'Nacre login theme installed. Rollback manifest: /var/lib/nacre/login/backups/20261023T100000000000Z/manifest.json'\n"
            ),
        );
        script(
            &dir.path().join("engine"),
            &format!(
                "echo \"$@\" >> '{d}/engine.log'\n[ -f '{d}/noappearance' ] && exit 1\nexit 0\n"
            ),
        );
        let fixture = Self { dir };
        set_env(Some(fixture.env()));
        fixture
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn env(&self) -> Env {
        Env {
            pkexec: self.path("pkexec").display().to_string(),
            python: self.path("python3").display().to_string(),
            installer: self.path("login-install.py"),
            theme: self.path("login"),
            user: "alice".into(),
            engine: self.path("engine"),
            timeout: Duration::from_secs(10),
        }
    }
    fn log(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_default()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        set_env(None);
    }
}
fn hub() -> Shared {
    Hub::new("login".into(), false, Duration::from_millis(1)).shared()
}
fn local<F: std::future::Future<Output = ()>>(f: F) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::task::LocalSet::new()
            .run_until(tokio::time::timeout(Duration::from_secs(60), f))
            .await
            .unwrap();
    });
}

#[test]
fn the_install_runs_the_documented_pkexec_flow_and_reports_the_result() {
    local(async {
        let f = Fixture::new();
        let hub = hub();
        assert!(available_methods(&hub.borrow()).contains_key(METHOD));
        let reply = dispatch(hub.clone(), json!({})).await.unwrap();
        assert_eq!(
            reply,
            json!({"installed":true,"appearance_published":true,"manifest":"/var/lib/nacre/login/backups/20261023T100000000000Z/manifest.json"})
        );
        assert_eq!(
            f.log("pkexec.log").trim(),
            format!(
                "{} {} --user alice --theme {}",
                f.path("python3").display(),
                f.path("login-install.py").display(),
                f.path("login").display()
            )
        );
        assert_eq!(f.log("engine.log").trim(), "login-appearance");

        // No manifest line, and a failed appearance publication, are reported as they are.
        std::fs::write(f.path("quiet"), "").unwrap();
        std::fs::write(f.path("noappearance"), "").unwrap();
        let reply = dispatch(hub.clone(), json!({})).await.unwrap();
        assert_eq!(
            reply,
            json!({"installed":true,"appearance_published":false})
        );
        std::fs::remove_file(f.path("quiet")).unwrap();
        std::fs::remove_file(f.path("noappearance")).unwrap();

        // A dismissed prompt is a permission error and publishes nothing.
        std::fs::write(f.path("dismiss"), "").unwrap();
        let before = f.log("engine.log");
        let error = dispatch(hub.clone(), json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.dispatched, Some(false));
        assert_eq!(f.log("engine.log"), before);
        std::fs::remove_file(f.path("dismiss")).unwrap();

        // A failed install is an I/O error that never echoes the installer's output.
        std::fs::write(f.path("fail"), "").unwrap();
        let error = dispatch(hub.clone(), json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Io);
        assert!(!format!("{error:?}").contains("SECRETOUT"));
        std::fs::remove_file(f.path("fail")).unwrap();

        // Only one install at a time, and a caller that gives up does not free the slot early.
        std::fs::write(f.path("hold"), "").unwrap();
        let first = tokio::task::spawn_local(dispatch(hub.clone(), json!({})));
        tokio::time::sleep(Duration::from_millis(300)).await;
        let error = dispatch(hub.clone(), json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        std::fs::remove_file(f.path("hold")).unwrap();
        assert!(first.await.unwrap().is_ok());
        assert!(dispatch(hub, json!({})).await.is_ok());
    });
}

#[test]
fn parameters_and_missing_pieces_are_refused_before_anything_runs() {
    local(async {
        let f = Fixture::new();
        let hub = hub();
        let error = dispatch(hub.clone(), json!({"user":"root"}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        // Without the theme sources the method is neither advertised nor runnable.
        std::fs::remove_file(f.path("login/Main.qml")).unwrap();
        assert!(available_methods(&hub.borrow()).is_empty());
        let error = dispatch(hub, json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert_eq!(f.log("pkexec.log"), "");
    });
}

#[test]
fn safe_mode_refuses_and_advertises_nothing() {
    local(async {
        let f = Fixture::new();
        let hub = Hub::new("safe".into(), true, Duration::from_millis(1)).shared();
        assert!(available_methods(&hub.borrow()).is_empty());
        let error = dispatch(hub, json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(f.log("pkexec.log"), "");
    });
}

#[test]
fn only_a_plain_absolute_manifest_path_is_relayed() {
    assert_eq!(
        manifest("x\nNacre login theme installed. Rollback manifest: /a/b.json\n").as_deref(),
        Some("/a/b.json")
    );
    for bad in [
        "Rollback manifest: relative.json",
        "Rollback manifest: /a\u{1b}[0m",
        "nothing",
    ] {
        assert_eq!(manifest(bad), None, "{bad:?}");
    }
}
