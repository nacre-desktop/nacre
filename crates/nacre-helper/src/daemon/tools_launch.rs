//! N0051: `tools.launch {tool}` starts one of a closed set of programs for the Settings pages: the
//! audio routing manager, the connection editor, `nacre setup` (add-on install), the AI workspace,
//! the AUR review policy view and the power-profiles service. The QML pages ran exactly these
//! commands; the caller picks a name and nothing else reaches a command line. The program is
//! started detached and the call returns `launched:true`; what happens next is the program's own.
use super::hub::{failure, Hub, Shared};
use nacre_protocol::{
    settings_asks::{Tool, ToolsLaunch},
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::OsString,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Stdio,
};

pub const METHOD: &str = "tools.launch";

#[derive(Clone)]
pub struct Env {
    /// Where programs are looked up (`PATH`).
    pub path: Vec<PathBuf>,
    /// The daemon's own executable, which hosts `aur-review policy`.
    pub engine: PathBuf,
}
impl Env {
    fn production() -> Self {
        Self {
            path: std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default(),
            engine: std::env::current_exe().unwrap_or_default(),
        }
    }
    fn find(&self, program: &str) -> Option<PathBuf> {
        if program.contains('/') {
            return Path::new(program).is_file().then(|| program.into());
        }
        self.path
            .iter()
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    }
}
thread_local! {
    static OVERRIDE: RefCell<Option<Env>> = const { RefCell::new(None) };
}
#[cfg(test)]
fn set_env(env: Option<Env>) {
    OVERRIDE.with(|slot| *slot.borrow_mut() = env);
}
fn env() -> Env {
    OVERRIDE
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(Env::production)
}

/// The command line of each tool: the program to start, its arguments and the other programs it
/// needs. Terminal tools run in `kitty`, as the QML pages did.
fn command(tool: Tool, engine: &Path) -> (&'static str, Vec<OsString>, Vec<&'static str>) {
    let args = |items: &[&str]| items.iter().map(OsString::from).collect::<Vec<_>>();
    match tool {
        Tool::AudioManager => ("pavucontrol", vec![], vec![]),
        Tool::ConnectionEditor => ("nm-connection-editor", vec![], vec![]),
        Tool::Setup => (
            "kitty",
            args(&["--title", "Nacre setup", "--", "nacre", "setup"]),
            vec!["nacre"],
        ),
        Tool::AiWorkspace => (
            "kitty",
            args(&[
                "--class",
                "nacre-ai-dashboard",
                "--title",
                "Nacre AI settings",
                "--",
                "nacre-ai",
                "settings",
            ]),
            vec!["nacre-ai"],
        ),
        Tool::AurReviewPolicy => {
            let mut all = args(&["--hold", "--title", "AUR review policy provenance", "--"]);
            all.extend([engine.into(), "aur-review".into(), "policy".into()]);
            ("kitty", all, vec![])
        }
        Tool::PowerService => (
            "pkexec",
            args(&["systemctl", "start", "power-profiles-daemon"]),
            // Installed means the service has something to start.
            vec!["powerprofilesctl", "systemctl"],
        ),
        Tool::Updates => (
            "kitty",
            args(&[
                "--class",
                "nacre-control",
                "--title",
                "System updates",
                "--",
                "nacre",
                "updates",
            ]),
            vec!["nacre"],
        ),
    }
}

pub fn handles(method: &str) -> bool {
    method == METHOD
}
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let mut methods = nacre_protocol::settings_asks::methods();
    methods.retain(|name, _| handles(name));
    if !super::capabilities::pages_up(hub) {
        methods.clear();
    }
    methods
}
fn refused(message: &str, code: ErrorCode) -> Failure {
    failure(code, message, code == ErrorCode::Unavailable).dispatch_state(false)
}

pub async fn dispatch(hub: Shared, params: Value) -> Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(refused(
            "Tools cannot be started in safe mode",
            ErrorCode::PermissionDenied,
        ));
    }
    let request: ToolsLaunch = serde_json::from_value(params)
        .map_err(|_| refused("Invalid tool request", ErrorCode::Validation))?;
    let env = env();
    let (program, args, needs) = command(request.tool, &env.engine);
    let missing = || refused("The tool is not installed", ErrorCode::Unavailable);
    let path = env.find(program).ok_or_else(missing)?;
    if needs.iter().any(|other| env.find(other).is_none()) {
        return Err(missing());
    }
    let mut child = std::process::Command::new(path);
    child
        .args(args)
        .env_remove("NOTIFY_SOCKET")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = child.spawn().map_err(|_| {
        failure(ErrorCode::Io, "The tool could not be started", true).dispatch_state(false)
    })?;
    // The program belongs to the session now; reaping it keeps the daemon free of zombies.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(json!({"launched":true}))
}

#[cfg(test)]
mod tests;
