//! Bounded reusable provider workers. Ranking never starts a process.
use crate::Result;
use nacre_protocol::{ErrorCode, Failure};
use serde_json::{json, Value};
use std::{
    io::{BufRead, Read, Write},
    process::Stdio,
    rc::Rc,
    time::{Duration, Instant},
};
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};
// Legacy hosted responses are 8 MiB and their JSON string needs escaping.
const LIMIT: usize = 16 * 1024 * 1024;
struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    used: Instant,
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            // The child created this process group; provider subprocesses inherit it.
            match nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            ) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                Err(_) => tracing::warn!("Launcher worker group could not be stopped"),
            }
        }
    }
}
#[derive(Clone)]
pub struct Pool {
    slots: [Rc<Mutex<Option<Worker>>>; 2],
    changed: Rc<tokio::sync::Notify>,
    #[cfg(test)]
    test_requests: Option<mpsc::UnboundedSender<TestRequest>>,
}
#[cfg(test)]
pub(crate) struct TestRequest {
    pub(crate) action: String,
    pub(crate) query: String,
    pub(crate) reply: tokio::sync::oneshot::Sender<std::result::Result<Value, Failure>>,
}
impl Default for Pool {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| Rc::new(Mutex::new(None))),
            changed: Rc::new(tokio::sync::Notify::new()),
            #[cfg(test)]
            test_requests: None,
        }
    }
}
fn failure(code: ErrorCode, message: &str, retry: bool) -> Failure {
    Failure::new(code, message, retry)
}
impl Pool {
    #[cfg(test)]
    pub(crate) fn with_test_requests(requests: mpsc::UnboundedSender<TestRequest>) -> Self {
        Self {
            test_requests: Some(requests),
            ..Self::default()
        }
    }

    pub async fn reap_idle(&self) -> Result<()> {
        #[cfg(test)]
        if self.test_requests.is_some() {
            return std::future::pending().await;
        }
        loop {
            let deadline = self
                .slots
                .iter()
                .filter_map(|slot| {
                    slot.try_lock()
                        .ok()
                        .and_then(|slot| slot.as_ref().map(|w| w.used + Duration::from_secs(31)))
                })
                .min();
            if let Some(deadline) = deadline {
                tokio::select! {_=self.changed.notified()=>continue,_=tokio::time::sleep_until(deadline.into())=>{}}
                for slot in &self.slots {
                    if let Ok(mut slot) = slot.try_lock() {
                        if slot
                            .as_ref()
                            .is_some_and(|w| w.used.elapsed() >= Duration::from_secs(31))
                        {
                            if let Some(mut worker) = slot.take() {
                                if worker.child.try_wait()?.is_none() {
                                    worker.child.kill().await?;
                                }
                            }
                        }
                    }
                }
                return Ok(());
            } else {
                self.changed.notified().await;
            }
        }
    }
    pub async fn request(&self, action: &str, query: &str) -> std::result::Result<Value, Failure> {
        #[cfg(test)]
        if let Some(requests) = &self.test_requests {
            let (reply, result) = tokio::sync::oneshot::channel();
            requests
                .send(TestRequest {
                    action: action.to_owned(),
                    query: query.to_owned(),
                    reply,
                })
                .map_err(|_| failure(ErrorCode::Unavailable, "Test provider ended", false))?;
            return result
                .await
                .map_err(|_| failure(ErrorCode::Unavailable, "Test provider ended", false))?;
        }
        let slot = self
            .slots
            .iter()
            .find_map(|s| s.try_lock().ok())
            .ok_or_else(|| {
                failure(
                    ErrorCode::Busy,
                    "Launcher provider concurrency limit reached",
                    true,
                )
            })?;
        self.request_in(slot, action, query).await
    }
    /// Hosted lanes queue fairly behind the same two providers as menu queries.
    pub async fn hosted(
        &self,
        request: &crate::service::Request,
    ) -> std::result::Result<Value, Failure> {
        let slot = if let Some(slot) = self.slots.iter().find_map(|s| s.try_lock().ok()) {
            slot
        } else {
            tokio::select! { slot=self.slots[0].lock()=>slot, slot=self.slots[1].lock()=>slot }
        };
        let request = serde_json::to_string(request)
            .map_err(|_| failure(ErrorCode::InvalidRequest, "Invalid helper request", false))?;
        self.request_in(slot, "helper", &request).await
    }
    /// One coalesced metadata refresh waits for its lane instead of taking both
    /// worker slots away from interactive queries. Its whole deadline includes waiting.
    pub async fn refresh(&self) -> std::result::Result<Value, Failure> {
        #[cfg(test)]
        if self.test_requests.is_some() {
            return Ok(json!({}));
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            let slot = self.slots[0].lock().await;
            self.request_in(slot, "load", "").await
        })
        .await
        .map_err(|_| {
            failure(
                ErrorCode::Timeout,
                "Launcher metadata refresh timed out",
                true,
            )
        })?
    }
    async fn request_in(
        &self,
        mut slot: tokio::sync::MutexGuard<'_, Option<Worker>>,
        action: &str,
        query: &str,
    ) -> std::result::Result<Value, Failure> {
        if let Some(worker) = slot.as_mut() {
            if worker
                .child
                .try_wait()
                .map_err(|_| failure(ErrorCode::Internal, "Cannot observe launcher worker", true))?
                .is_some()
            {
                *slot = None;
            }
        }
        if slot.is_none() {
            let mut command = Command::new("/proc/self/exe");
            command
                .args(["daemon-menu-worker", &std::process::id().to_string()])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .process_group(0);
            for name in [
                "NOTIFY_SOCKET",
                "WATCHDOG_USEC",
                "WATCHDOG_PID",
                "LISTEN_PID",
                "LISTEN_FDS",
                "LISTEN_FDNAMES",
            ] {
                command.env_remove(name);
            }
            let mut child = command.spawn().map_err(|_| {
                failure(
                    ErrorCode::Unavailable,
                    "Cannot start launcher provider",
                    true,
                )
            })?;
            let input = child
                .stdin
                .take()
                .ok_or_else(|| failure(ErrorCode::Internal, "Missing provider input", false))?;
            let output =
                BufReader::new(child.stdout.take().ok_or_else(|| {
                    failure(ErrorCode::Internal, "Missing provider output", false)
                })?);
            *slot = Some(Worker {
                child,
                input,
                output,
                used: Instant::now(),
            });
        }
        // Taking ownership makes cancellation kill the worker instead of leaving
        // a half-read response for the next request on this slot.
        let mut worker = slot.take().unwrap();
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            let mut packet = serde_json::to_vec(&json!({"action":action,"query":query}))?;
            packet.push(b'\n');
            worker.input.write_all(&packet).await?;
            worker.input.flush().await?;
            let mut bytes = Vec::new();
            (&mut worker.output)
                .take((LIMIT + 1) as u64)
                .read_until(b'\n', &mut bytes)
                .await?;
            if bytes.is_empty() || bytes.len() > LIMIT || bytes.last() != Some(&b'\n') {
                return Err::<Value, crate::Error>("Invalid launcher provider response".into());
            }
            Ok(serde_json::from_slice(&bytes)?)
        })
        .await;
        match result {
            Ok(Ok(value)) => {
                worker.used = Instant::now();
                *slot = Some(worker);
                self.changed.notify_one();
                if let Some(message) = value["error"].as_str() {
                    Err(failure(ErrorCode::Unavailable, message, true))
                } else {
                    Ok(value["ok"].clone())
                }
            }
            _ => {
                drop(worker);
                Err(failure(
                    ErrorCode::Timeout,
                    "Launcher provider failed or timed out",
                    true,
                ))
            }
        }
    }
}
fn execute(action: &str, query: &str, inherited_tokio_threads: bool) -> Result<Value> {
    match action {
        "helper" => {
            let request = crate::service::validate(&serde_json::from_str(query)?)?;
            // Preferences are routed to the engine by the parent, never recursively
            // connected back to the daemon from this worker.
            if request.tool == "launcher-preferences" {
                return Err("Preferences must use the daemon writer".into());
            }
            Ok(serde_json::to_value(crate::service::result(&request))?)
        }
        "launch" => launch_application(&serde_json::from_str(query)?, inherited_tokio_threads),
        "load" => Ok(
            json!({"snapshot":crate::menu::snapshot(false)?,"library":crate::software_library::execute(&["load".into()],None)?,"workflows":crate::launcher_workflows::execute(&["load".into()],None)?}),
        ),
        "files" | "packages" | "convert" | "ownership" => {
            crate::menu::execute(&[action.into(), query.into()], None)?
                .ok_or_else(|| "Missing launcher provider response".into())
        }
        "emoji" => crate::launcher_workflows::execute(&[action.into(), query.into()], None),
        _ => Err("Unsupported launcher provider".into()),
    }
}
pub fn run(arguments: &[String], inherited_tokio_threads: bool) -> Result<i32> {
    // Keep provider diagnostics visible without leaking a panic's payload.
    std::panic::set_hook(Box::new(|_| {
        eprintln!("Nacre launcher worker panicked; payload omitted")
    }));
    let [parent] = arguments else {
        return Err("Expected daemon parent PID".into());
    };
    let parent = parent.parse::<u32>()?;
    rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::TERM))?;
    if rustix::process::getppid().map(|p| p.as_raw_nonzero().get() as u32) != Some(parent) {
        return Err("Daemon parent ended".into());
    }
    if rustix::process::getpriority_process(None)? < 10 {
        rustix::process::setpriority_process(None, 10)?;
    }
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    loop {
        let mut fds = [rustix::event::PollFd::new(
            &stdin,
            rustix::event::PollFlags::IN,
        )];
        let timeout = rustix::event::Timespec::try_from(Duration::from_secs(30))?;
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Ok(0),
            Err(rustix::io::Errno::INTR) => continue,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        let mut bytes = Vec::new();
        input
            .by_ref()
            .take((LIMIT + 1) as u64)
            .read_until(b'\n', &mut bytes)?;
        if bytes.is_empty() {
            return Ok(0);
        }
        if bytes.len() > LIMIT || bytes.last() != Some(&b'\n') {
            return Err("Launcher worker input exceeds its limit".into());
        }
        let result = (|| -> Result<Value> {
            let input: Value = serde_json::from_slice(&bytes)?;
            execute(
                input["action"].as_str().ok_or("Missing provider action")?,
                input["query"].as_str().ok_or("Missing provider query")?,
                inherited_tokio_threads,
            )
        })();
        let packet = match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error.to_string().chars().take(240).collect::<String>()}),
        };
        let mut bytes = serde_json::to_vec(&packet)?;
        if bytes.len() > LIMIT {
            bytes =
                serde_json::to_vec(&json!({"error":"Launcher provider output exceeds its limit"}))?;
        }
        output.write_all(&bytes)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
}

/// Launch through the user manager directly; never run a shell, env or systemd-run.
/// The owner supplies catalog data, not client-provided command text.
fn launch_application(entry: &Value, inherited_tokio_threads: bool) -> Result<Value> {
    use std::{os::unix::fs::PermissionsExt, path::Path};
    use zbus::zvariant::{OwnedValue, Value as Variant};
    let args = entry["command"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("Application has no command")?;
    let mut argv = args
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.contains('\0'))
                .map(str::to_owned)
                .ok_or("Invalid application argument")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if entry["runInTerminal"] == true {
        argv.splice(0..0, ["kitty".into(), "--".into()]);
    }
    let executable = if argv[0].contains('/') {
        std::path::absolute(&argv[0])?
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(&argv[0]))
            .find(|p| {
                p.is_file()
                    && std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
            })
            .ok_or("Application command is unavailable")?
    };
    let executable = executable
        .to_str()
        .ok_or("Invalid executable path")?
        .to_owned();
    argv[0] = executable.clone();
    const REMOVE: &[&str] = &[
        "LD_LIBRARY_PATH",
        "QML_IMPORT_PATH",
        "QML2_IMPORT_PATH",
        "QT_PLUGIN_PATH",
        "NACRE_LIB_DIR",
        "NOTIFY_SOCKET",
        "WATCHDOG_USEC",
        "WATCHDOG_PID",
        "LISTEN_PID",
        "LISTEN_FDS",
        "LISTEN_FDNAMES",
    ];
    let environment: Vec<String> = std::env::vars()
        .filter(|(key, _)| {
            !REMOVE.contains(&key.as_str())
                && (inherited_tokio_threads || key != "TOKIO_WORKER_THREADS")
        })
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let mut properties: Vec<(&str, OwnedValue)> = vec![
        (
            "Description",
            OwnedValue::try_from(Variant::new("Nacre application"))?,
        ),
        ("Type", OwnedValue::try_from(Variant::new("exec"))?),
        ("Slice", OwnedValue::try_from(Variant::new("app.slice"))?),
        (
            "CollectMode",
            OwnedValue::try_from(Variant::new("inactive-or-failed"))?,
        ),
        (
            "ExecStart",
            OwnedValue::try_from(Variant::new(vec![(executable, argv, false)]))?,
        ),
        (
            "Environment",
            OwnedValue::try_from(Variant::new(environment))?,
        ),
        (
            "UnsetEnvironment",
            OwnedValue::try_from(Variant::new(REMOVE.to_vec()))?,
        ),
    ];
    if let Some(directory) = entry["workingDirectory"].as_str().filter(|s| !s.is_empty()) {
        let directory = std::path::absolute(Path::new(directory))?;
        properties.push((
            "WorkingDirectory",
            OwnedValue::try_from(Variant::new(
                directory.to_str().ok_or("Invalid application directory")?,
            ))?,
        ));
    }
    let name = format!("nacre-app-{}.service", uuid::Uuid::new_v4());
    crate::menu_registry::systemd_job(
        "StartTransientUnit",
        &(
            name,
            "fail",
            properties,
            Vec::<(String, Vec<(String, OwnedValue)>)>::new(),
        ),
    )?;
    Ok(json!({"keep_open":false}))
}
