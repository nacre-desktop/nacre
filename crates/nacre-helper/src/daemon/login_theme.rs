//! N0049: the privileged login-theme install as an explicit method. It runs the flow the Nacre
//! README documents, `pkexec /usr/bin/python3 login-install.py --user NAME --theme DIR`, so polkit
//! asks the session's authentication agent and the install runs as root exactly as `nacre/
//! shell-tools/login-install.py` writes it (backup manifest first, no SDDM restart). On success the
//! helper's own `login-appearance` publishes the colours and wallpaper for the greeter.
//!
//! Nothing a caller sends reaches the command line: the method takes no parameters, and the paths
//! are the daemon's own tool and theme folders. Command output is parsed for the manifest path and
//! never relayed.
use super::hub::{failure, Hub, Shared};
use crate::{paths::Paths, process};
use nacre_protocol::{
    pill_controls::PillEmpty,
    system_settings::{methods, PROMPT_TIMEOUT_MS},
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const METHOD: &str = "login.install_theme";
const MANIFEST_PREFIX: &str = "Rollback manifest: ";
const MAX_MANIFEST: usize = 4096;
/// pkexec: the authentication dialog was dismissed (126) or authorisation failed (127).
const DISMISSED: i32 = 126;
const NOT_AUTHORIZED: i32 = 127;

#[derive(Clone)]
pub struct Env {
    pub pkexec: String,
    pub python: String,
    /// `login-install.py` in the helper tools folder.
    pub installer: PathBuf,
    /// The SDDM theme sources the installer copies (`Main.qml` marks a real one).
    pub theme: PathBuf,
    pub user: String,
    /// The daemon's own executable, which hosts `login-appearance`.
    pub engine: PathBuf,
    pub timeout: Duration,
}
impl Env {
    fn production() -> Option<Self> {
        let paths = Paths::environment().ok()?;
        let installer = paths.assets().join("login-install.py");
        // The installer's own default: the `login` folder beside its parent.
        let theme = installer
            .canonicalize()
            .ok()?
            .parent()?
            .parent()?
            .join("login");
        let user = nix::unistd::User::from_uid(nix::unistd::getuid())
            .ok()??
            .name;
        Some(Self {
            pkexec: "pkexec".into(),
            python: "/usr/bin/python3".into(),
            installer,
            theme,
            user,
            engine: std::env::current_exe().ok()?,
            timeout: Duration::from_millis(u64::from(PROMPT_TIMEOUT_MS) - 5_000),
        })
    }
    fn present(&self) -> bool {
        let program = |name: &str| {
            if name.contains('/') {
                std::path::Path::new(name).is_file()
            } else {
                std::env::var_os("PATH").is_some_and(|path| {
                    std::env::split_paths(&path).any(|d| d.join(name).is_file())
                })
            }
        };
        program(&self.pkexec)
            && program(&self.python)
            && self.installer.is_file()
            && self.theme.join("Main.qml").is_file()
            && !self.user.is_empty()
            && !self.user.starts_with('-')
    }
}
thread_local! {
    static OVERRIDE: RefCell<Option<Env>> = const { RefCell::new(None) };
}
/// One install at a time. It is released when the install itself ends, not when a caller gives up.
static RUNNING: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
fn set_env(env: Option<Env>) {
    OVERRIDE.with(|slot| *slot.borrow_mut() = env);
}
fn env() -> Option<Env> {
    OVERRIDE
        .with(|slot| slot.borrow().clone())
        .or_else(Env::production)
}

pub fn handles(method: &str) -> bool {
    method == METHOD
}
/// Advertised only where the installer, the theme sources, `pkexec` and python exist.
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let mut all = methods();
    all.retain(|name, _| name == METHOD);
    if hub.safe || !env().is_some_and(|env| env.present()) {
        all.clear();
    }
    all
}

fn unavailable() -> Failure {
    failure(
        ErrorCode::Unavailable,
        "The login theme cannot be installed here",
        true,
    )
    .dispatch_state(false)
}
/// The path after the marker, if it is one plain absolute path.
fn manifest(stdout: &str) -> Option<String> {
    let path = stdout
        .lines()
        .find_map(|line| line.split_once(MANIFEST_PREFIX))?
        .1
        .trim();
    (path.starts_with('/') && path.len() <= MAX_MANIFEST && !path.chars().any(char::is_control))
        .then(|| path.to_owned())
}
fn command(program: &str, args: &[&std::ffi::OsStr]) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.args(args).env_remove("NOTIFY_SOCKET");
    command
}
/// The blocking part: pkexec and then the appearance publication.
fn install(env: &Env) -> Result<Value, Failure> {
    use std::ffi::OsStr;
    let output = process::capture(
        command(
            &env.pkexec,
            &[
                OsStr::new(&env.python),
                env.installer.as_os_str(),
                OsStr::new("--user"),
                OsStr::new(&env.user),
                OsStr::new("--theme"),
                env.theme.as_os_str(),
            ],
        ),
        None,
        env.timeout,
    )
    .map_err(|_| {
        failure(
            ErrorCode::Io,
            "The login theme install did not finish",
            false,
        )
        .dispatch_state(true)
    })?;
    match output.status.code() {
        Some(0) => {}
        Some(DISMISSED | NOT_AUTHORIZED) => {
            return Err(failure(
                ErrorCode::PermissionDenied,
                "Administrator authentication was not given",
                true,
            )
            .dispatch_state(false))
        }
        _ => {
            return Err(
                failure(ErrorCode::Io, "The login theme install failed", false)
                    .dispatch_state(true),
            )
        }
    }
    let stdout = String::from_utf8_lossy(output.stdout.get(..64 * 1024).unwrap_or(&output.stdout));
    let published = process::capture(
        command(
            &env.engine.to_string_lossy(),
            &[OsStr::new("login-appearance")],
        ),
        None,
        Duration::from_secs(60),
    )
    .is_ok_and(|output| output.status.success());
    let mut reply = json!({"installed":true,"appearance_published":published});
    if let Some(path) = manifest(&stdout) {
        reply["manifest"] = json!(path);
    }
    Ok(reply)
}

pub async fn dispatch(hub: Shared, params: Value) -> Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "The login theme install is disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    serde_json::from_value::<PillEmpty>(params).map_err(|_| {
        failure(ErrorCode::Validation, "Invalid login theme request", false).dispatch_state(false)
    })?;
    let env = env().filter(Env::present).ok_or_else(unavailable)?;
    if RUNNING.swap(true, Ordering::AcqRel) {
        return Err(
            failure(ErrorCode::Busy, "The login theme install is running", true)
                .dispatch_state(false),
        );
    }
    tokio::task::spawn_blocking(move || {
        struct Release;
        impl Drop for Release {
            fn drop(&mut self) {
                RUNNING.store(false, Ordering::Release);
            }
        }
        let _release = Release;
        install(&env)
    })
    .await
    .map_err(|_| {
        failure(ErrorCode::Internal, "The login theme install failed", false).dispatch_state(true)
    })?
}

#[cfg(test)]
mod tests;
