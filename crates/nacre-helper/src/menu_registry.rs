//! Existing package, web application and reminder actions; no shell evaluation.
use crate::{files, i18n::tr, json, menu, paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

#[derive(Debug)]
pub struct ReminderTypeError(&'static str);
impl std::fmt::Display for ReminderTypeError {
    fn fmt(&self, format: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            format,
            "float() argument must be a string or a real number, not '{}'",
            self.0
        )
    }
}
impl std::error::Error for ReminderTypeError {}

fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|root| root.join(name))
        .find(|path| {
            path.is_file()
                && fs::metadata(path)
                    .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
        })
}

pub fn package_command(action: &str, value: &str) -> Result<Vec<String>> {
    package_with(action, value, which)
}
fn package_with(
    action: &str,
    value: &str,
    mut find: impl FnMut(&str) -> Option<PathBuf>,
) -> Result<Vec<String>> {
    let value = menu::package_name(value)?;
    let install = action == "install";
    let mut command = if let Some(manager) = find("paru").or_else(|| find("yay")) {
        let mut args = vec![
            manager.to_string_lossy().into_owned(),
            if install { "-S" } else { "-Rns" }.into(),
        ];
        if install {
            args.push("--needed".into());
            if manager.file_name().is_some_and(|name| name == "paru") {
                args.push("--skipreview".into());
            }
        }
        args
    } else {
        if find("pacman").is_none() {
            return Err(tr(
                "helper.menu-backend.package-management-requires-an-arch-compatible-system",
                "Package management requires an Arch-compatible system",
            )
            .into());
        }
        let mut args = vec![
            "pkexec".into(),
            "pacman".into(),
            if install { "-S" } else { "-Rns" }.into(),
        ];
        if install {
            args.push("--needed".into());
        }
        args
    };
    command.extend(["--".into(), value.into()]);
    Ok(command)
}

pub(crate) fn registry_lock(paths: &Paths, name: &str) -> Result<nix::fcntl::Flock<fs::File>> {
    let folder = paths.state.join("menu");
    fs::create_dir_all(folder.parent().ok_or("Missing menu state parent")?)?;
    match fs::DirBuilder::new().mode(0o700).create(&folder) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && folder.is_dir() => {}
        Err(error) => return Err(error.into()),
    }
    files::lock(&folder.join(format!("{name}.lock")))
}
fn unlink(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
pub(crate) fn invalidate(paths: &Paths) -> Result<()> {
    unlink(&paths.cache.join("menu/state.json"))
}
fn identifier(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn rows(value: Value) -> Result<Vec<Value>> {
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "Invalid menu registry response".into())
}
fn trimmed(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

// Match urllib.parse's validation boundary, not browser URL canonicalization.
// The saved URL is retained verbatim; no DNS/port lookup or normalization runs.
fn web_url_valid(value: &str) -> Result<bool> {
    let value = value.trim_start_matches(' ');
    let (scheme, tail) = match value.split_once(':') {
        Some((scheme, tail))
            if scheme
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && scheme
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"+-.".contains(&c)) =>
        {
            (scheme.to_ascii_lowercase(), tail)
        }
        _ => (String::new(), value),
    };
    let authority = tail
        .strip_prefix("//")
        .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or(""));
    let Some(authority) = authority else {
        return Ok(false);
    };
    let host_port = authority.rsplit('@').next().unwrap_or("");
    if authority.contains('[') != authority.contains(']') {
        return Err("Invalid IPv6 URL".into());
    }
    if authority.contains('[') && authority.contains(']') {
        let hostname = if let Some((before, rest)) = host_port.split_once('[') {
            if !before.is_empty() {
                return Err("Invalid IPv6 URL".into());
            }
            let (hostname, port) = rest.split_once(']').ok_or("Invalid IPv6 URL")?;
            if !port.is_empty() && !port.starts_with(':') {
                return Err("Invalid IPv6 URL".into());
            }
            hostname
        } else {
            host_port.split(':').next().unwrap_or("")
        };
        if hostname.starts_with(['v', 'V']) {
            if !regex::Regex::new(r"\A[vV][a-fA-F0-9]+\..+\z")?.is_match(hostname) {
                return Err("IPvFuture address is invalid".into());
            }
        } else {
            if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
                return Err("An IPv4 address cannot be in brackets".into());
            }
            let (address, scope) = hostname
                .split_once('%')
                .map_or((hostname, None), |(address, scope)| (address, Some(scope)));
            if address.parse::<std::net::Ipv6Addr>().is_err()
                || scope.is_some_and(|scope| scope.is_empty() || scope.contains('%'))
            {
                let repr = json::python_str(&json!([hostname]));
                return Err(format!(
                    "{} does not appear to be an IPv4 or IPv6 address",
                    &repr[1..repr.len() - 1]
                )
                .into());
            }
        }
    }
    if !authority.is_ascii() {
        let stripped: String = authority.chars().filter(|c| !"@:#?".contains(*c)).collect();
        let normalized: String = stripped.nfkc().collect();
        if stripped != normalized && normalized.contains(['/', '?', '#', '@', ':']) {
            return Err(format!(
                "netloc '{authority}' contains invalid characters under NFKC normalization"
            )
            .into());
        }
    }
    let hostname = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    let credentials = authority
        .rsplit_once('@')
        .map(|(credentials, _)| credentials);
    Ok(matches!(scheme.as_str(), "http" | "https")
        && !hostname.is_empty()
        && credentials.is_none_or(|credentials| credentials.is_empty() || credentials == ":"))
}

pub fn create_webapp(label: &Value, url: &Value) -> Result<Value> {
    create_webapp_with(
        &Paths::environment()?,
        label,
        url,
        &Uuid::new_v4().simple().to_string(),
        menu::webapps,
    )
}
fn create_webapp_with(
    paths: &Paths,
    label: &Value,
    url: &Value,
    ident: &str,
    read: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    let _guard = registry_lock(paths, "webapps")?;
    let label = label
        .as_str()
        .filter(|label| {
            (1..=80).contains(&trimmed(label).chars().count()) && !label.chars().any(|c| c < ' ')
        })
        .ok_or_else(|| {
            tr(
                "helper.menu-backend.choose-a-short-application-name",
                "Choose a short application name",
            )
        })?;
    let url = url
        .as_str()
        .filter(|url| url.chars().count() <= 4096 && !url.chars().any(|c| c < ' '))
        .ok_or_else(|| {
            tr(
                "helper.menu-backend.enter-a-valid-website-url",
                "Enter a valid website URL",
            )
        })?;
    if !web_url_valid(url)? {
        return Err(tr(
            "helper.menu-backend.use-an-http-or-https-website-url-without-credentials",
            "Use an HTTP or HTTPS website URL without credentials",
        )
        .into());
    }
    let mut entries = rows(read()?)?;
    entries.push(json!({"id":ident,"label":trimmed(label),"url":url}));
    files::json_write(&paths.state.join("menu/webapps.json"), &entries)?;
    let folder = paths.applications();
    fs::create_dir_all(&folder)?;
    let display = trimmed(label).replace('\\', "\\\\");
    let body = format!("[Desktop Entry]\nType=Application\nName={display}\nIcon=web-browser\nExec=nacre-menu webapp-launch {ident}\nCategories=Network;\nTerminal=false\n");
    files::atomic(
        &folder.join(format!("nacre-web-{ident}.desktop")),
        body.as_bytes(),
    )?;
    invalidate(paths)?;
    Ok(
        json!({"message":tr("helper.menu-backend.web-app-added-to-all-apps", "Web app added to All apps")}),
    )
}

pub fn remove_webapp(ident: &str) -> Result<Value> {
    remove_webapp_with(&Paths::environment()?, ident, menu::webapps)
}
fn remove_webapp_with(
    paths: &Paths,
    ident: &str,
    read: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    let _guard = registry_lock(paths, "webapps")?;
    if !identifier(ident) {
        return Err(tr("helper.menu-backend.invalid-web-app", "Invalid web app").into());
    }
    let mut entries = rows(read()?)?;
    if !entries.iter().any(|row| row["id"] == ident) {
        return Err(tr(
            "helper.menu-backend.web-app-no-longer-exists",
            "Web app no longer exists",
        )
        .into());
    }
    entries.retain(|row| row["id"] != ident);
    files::json_write(&paths.state.join("menu/webapps.json"), &entries)?;
    unlink(
        &paths
            .applications()
            .join(format!("nacre-web-{ident}.desktop")),
    )?;
    invalidate(paths)?;
    Ok(json!({"message":tr("helper.menu-backend.web-app-removed", "Web app removed")}))
}

#[derive(Debug)]
struct TimerRequest {
    unit: String,
    seconds: f64,
    executable: PathBuf,
    argv: Vec<String>,
}

fn duration(value: &Value) -> Result<f64> {
    match value {
        Value::Number(value) => value
            .as_f64()
            .ok_or_else(|| "Invalid reminder duration".into()),
        Value::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
        Value::String(value) => {
            use unicode_general_category::{get_general_category, GeneralCategory};
            let mapped: String = value
                .chars()
                .map(|c| {
                    if get_general_category(c) == GeneralCategory::DecimalNumber {
                        let code = c as u32;
                        let mut start = code;
                        while char::from_u32(start - 1).is_some_and(|c| {
                            get_general_category(c) == GeneralCategory::DecimalNumber
                        }) {
                            start -= 1;
                        }
                        char::from_digit((code - start) % 10, 10).expect("decimal digit")
                    } else if !value.is_ascii() && c.is_whitespace() {
                        ' '
                    } else {
                        c
                    }
                })
                .collect();
            let text = mapped.trim();
            let pattern = r"(?i)\A[+-]?(?:inf(?:inity)?|nan|(?:(?:[0-9](?:_?[0-9])*(?:\.(?:[0-9](?:_?[0-9])*)?)?|\.[0-9](?:_?[0-9])*)(?:e[+-]?[0-9](?:_?[0-9])*)?))\z";
            if regex::Regex::new(pattern)?.is_match(text) {
                if let Ok(number) = text
                    .replace('_', "")
                    .to_ascii_lowercase()
                    .replace("infinity", "inf")
                    .parse()
                {
                    return Ok(number);
                }
            }
            let repr = json::python_str(&json!([value]));
            Err(format!(
                "could not convert string to float: {}",
                &repr[1..repr.len() - 1]
            )
            .into())
        }
        Value::Null => Err(ReminderTypeError("NoneType").into()),
        Value::Array(_) => Err(ReminderTypeError("list").into()),
        Value::Object(_) => Err(ReminderTypeError("dict").into()),
    }
}

fn start_timer(request: &TimerRequest) -> Result<()> {
    start_timer_at(request, None)
}
pub(crate) fn description_word(word: &str) -> String {
    if !word.is_empty()
        && !word
            .chars()
            .any(|c| c.is_ascii_control() || c == ' ' || "\"\\`$*?['()<>|&;!".contains(c))
    {
        return word.into();
    }
    let mut result = String::from("\"");
    for c in word.chars() {
        match c {
            '\u{7}' => result.push_str("\\a"),
            '\u{8}' => result.push_str("\\b"),
            '\t' => result.push_str("\\t"),
            '\n' => result.push_str("\\n"),
            '\u{b}' => result.push_str("\\v"),
            '\u{c}' => result.push_str("\\f"),
            '\r' => result.push_str("\\r"),
            c if c.is_ascii_control() => result.push_str(&format!("\\{:03o}", c as u32)),
            c if "\"\\`$".contains(c) => {
                result.push('\\');
                result.push(c);
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
fn start_timer_at(request: &TimerRequest, address: Option<&str>) -> Result<()> {
    let executable = std::path::absolute(&request.executable)?;
    let executable = executable
        .to_str()
        .ok_or("Notification executable path is not UTF-8")?;
    let mut argv = request.argv.clone();
    *argv.first_mut().ok_or("Missing notification command")? = executable.into();
    let description = format!(
        "[systemd-run] {}",
        argv.iter()
            .map(|word| description_word(word))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let timer: Vec<(&str, OwnedValue)> = vec![
        (
            "Description",
            OwnedValue::try_from(zbus::zvariant::Value::new(description.clone()))?,
        ),
        (
            "CollectMode",
            OwnedValue::try_from(zbus::zvariant::Value::new("inactive-or-failed"))?,
        ),
        ("AddRef", OwnedValue::from(true)),
        (
            "TimersMonotonic",
            OwnedValue::try_from(zbus::zvariant::Value::new(vec![(
                "OnActiveSec",
                (request.seconds * 1_000_000.0).round() as u64,
            )]))?,
        ),
        ("AccuracyUSec", OwnedValue::from(1_000_000u64)),
        ("RemainAfterElapse", OwnedValue::from(false)),
    ];
    let service: Vec<(&str, OwnedValue)> = vec![
        (
            "Description",
            OwnedValue::try_from(zbus::zvariant::Value::new(description))?,
        ),
        (
            "CollectMode",
            OwnedValue::try_from(zbus::zvariant::Value::new("inactive-or-failed"))?,
        ),
        ("AddRef", OwnedValue::from(true)),
        (
            "ExecStart",
            OwnedValue::try_from(zbus::zvariant::Value::new(vec![(executable, argv, false)]))?,
        ),
    ];
    systemd_job_at(
        "StartTransientUnit",
        &(
            format!("{}.timer", request.unit),
            "fail",
            timer,
            vec![(format!("{}.service", request.unit), service)],
        ),
        address,
    )
}
fn stop_timer(unit: &str) -> Result<()> {
    systemd_job("StopUnit", &(unit, "replace"))
}
pub(crate) fn systemd_job<B: serde::Serialize + zbus::zvariant::DynamicType>(
    method: &str,
    body: &B,
) -> Result<()> {
    systemd_job_at(method, body, None)
}
fn systemd_job_at<B: serde::Serialize + zbus::zvariant::DynamicType>(
    method: &str,
    body: &B,
    address: Option<&str>,
) -> Result<()> {
    use futures_lite::StreamExt;
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::session()?,
            };
            let connection = builder
                .method_timeout(Duration::from_secs(5))
                .build()
                .await?;
            let manager = zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
            )
            .await?;
            let mut completed = manager.receive_signal("JobRemoved").await?;
            let _: () = manager.call("Subscribe", &()).await?;
            let job: OwnedObjectPath = manager.call(method, body).await?;
            while let Some(message) = completed.next().await {
                let (_, path, _, result): (u32, OwnedObjectPath, String, String) =
                    message.body().deserialize()?;
                if path == job {
                    return if result == "done" {
                        Ok(())
                    } else {
                        Err(format!("Systemd job failed: {result}").into())
                    };
                }
            }
            Err("Systemd job event stream closed".into())
        },
        async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            Err("Systemd job timed out after 5 seconds".into())
        },
    ))
}
pub fn set_reminder(minutes: &Value, message: &Value) -> Result<Value> {
    reminder_with(
        &Paths::environment()?,
        minutes,
        message,
        &Uuid::new_v4().simple().to_string(),
        || Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64()),
        which,
        menu::reminders,
        start_timer,
    )
}
#[allow(clippy::too_many_arguments)]
fn reminder_with(
    paths: &Paths,
    minutes: &Value,
    message: &Value,
    ident: &str,
    now: impl FnOnce() -> Result<f64>,
    mut find: impl FnMut(&str) -> Option<PathBuf>,
    read: impl FnOnce() -> Result<Value>,
    mut start: impl FnMut(&TimerRequest) -> Result<()>,
) -> Result<Value> {
    let _guard = registry_lock(paths, "reminders")?;
    let minutes = duration(minutes)?;
    let message = message.as_str().filter(|message| {
        (1..=240).contains(&trimmed(message).chars().count()) && !message.contains('\0')
    });
    if !(0.1..=1440.0).contains(&minutes) || message.is_none() {
        return Err(tr(
            "helper.menu-backend.use-a-duration-between-0-1-and-1440-minutes-and-a-short-message",
            "Use a duration between 0.1 and 1440 minutes and a short message",
        )
        .into());
    }
    let executable = find("notify-send").ok_or_else(|| {
        tr(
            "helper.menu-backend.desktop-notifications-are-unavailable",
            "Desktop notifications are unavailable",
        )
    })?;
    let message = trimmed(message.ok_or("Missing reminder message")?);
    let unit = format!("nacre-reminder-{ident}");
    let request = TimerRequest {
        unit: unit.clone(),
        seconds: minutes * 60.0,
        executable,
        argv: vec![
            "notify-send".into(),
            "-a".into(),
            "Nacre Reminders".into(),
            "-i".into(),
            "nacre-notifications".into(),
            tr("reminder.notification.title", "Reminder"),
            message.into(),
        ],
    };
    start(&request)?;
    let mut entries = rows(read()?)?;
    entries.push(json!({"unit":unit+".timer","label":message,"due":now()?+request.seconds}));
    files::json_write(&paths.state.join("menu/reminders.json"), &entries)?;
    invalidate(paths)?;
    Ok(json!({"message":tr("helper.menu-backend.reminder-set", "Reminder set")}))
}
pub fn cancel_reminder(unit: &str) -> Result<Value> {
    cancel_with(&Paths::environment()?, unit, menu::reminders, stop_timer)
}
fn cancel_with(
    paths: &Paths,
    unit: &str,
    mut read: impl FnMut() -> Result<Value>,
    mut stop: impl FnMut(&str) -> Result<()>,
) -> Result<Value> {
    let _guard = registry_lock(paths, "reminders")?;
    let ident = unit
        .strip_prefix("nacre-reminder-")
        .and_then(|value| value.strip_suffix(".timer"));
    if !ident.is_some_and(identifier) || !rows(read()?)?.iter().any(|row| row["unit"] == unit) {
        return Err(tr(
            "helper.menu-backend.unknown-nacre-reminder",
            "Unknown Nacre reminder",
        )
        .into());
    }
    stop(unit)?;
    let mut entries = rows(read()?)?;
    entries.retain(|row| row["unit"] != unit);
    files::json_write(&paths.state.join("menu/reminders.json"), &entries)?;
    invalidate(paths)?;
    Ok(json!({"message":tr("helper.menu-backend.reminder-canceled", "Reminder canceled")}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicBool, AtomicU8, Ordering},
            Arc, Mutex,
        },
    };

    type Properties = Vec<(String, OwnedValue)>;
    struct BusCall {
        unit: String,
        mode: String,
        properties: Properties,
        auxiliary: Vec<(String, Properties)>,
    }
    struct SystemdMock {
        subscribed: Arc<AtomicBool>,
        result: Arc<AtomicU8>,
        calls: Arc<Mutex<Vec<BusCall>>>,
    }
    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl SystemdMock {
        fn subscribe(&self) {
            self.subscribed.store(true, Ordering::SeqCst);
        }
        async fn start_transient_unit(
            &self,
            unit: String,
            mode: String,
            properties: Properties,
            auxiliary: Vec<(String, Properties)>,
            #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            if !self.subscribed.load(Ordering::SeqCst) {
                return Err(zbus::fdo::Error::Failed("Missing job subscription".into()));
            }
            self.calls
                .lock()
                .map_err(|_| zbus::fdo::Error::Failed("Fixture lock failed".into()))?
                .push(BusCall {
                    unit: unit.clone(),
                    mode,
                    properties,
                    auxiliary,
                });
            let job = OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/7")
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
            // Emit before the method reply: subscribing after StartTransientUnit
            // would miss completion. Unrelated failed jobs must be ignored.
            Self::job_removed(
                &emitter,
                6,
                zbus::zvariant::ObjectPath::try_from("/org/freedesktop/systemd1/job/6")
                    .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?,
                "unrelated.service",
                "failed",
            )
            .await?;
            match self.result.load(Ordering::SeqCst) {
                0 => Self::job_removed(&emitter, 7, job.as_ref(), &unit, "done").await?,
                1 => Self::job_removed(&emitter, 7, job.as_ref(), &unit, "failed").await?,
                _ => {}
            }
            Ok(job)
        }
        async fn stop_unit(
            &self,
            unit: String,
            mode: String,
            #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            self.start_transient_unit(unit, mode, vec![], vec![], emitter)
                .await
        }
        #[zbus(signal)]
        async fn job_removed(
            emitter: &zbus::object_server::SignalEmitter<'_>,
            id: u32,
            job: zbus::zvariant::ObjectPath<'_>,
            unit: &str,
            result: &str,
        ) -> zbus::Result<()>;
    }
    struct BusDaemon(Child);
    impl Drop for BusDaemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn private_systemd_bus_waits_for_matching_jobs_and_surfaces_failure() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let mut daemon = BusDaemon(
            Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--print-address",
                    &format!(
                        "--address=unix:path={}",
                        folder.path().join("bus").display()
                    ),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let mut address = String::new();
        BufReader::new(daemon.0.stdout.take().ok_or("Missing fixture address")?)
            .read_line(&mut address)?;
        let address = address.trim();
        let subscribed = Arc::new(AtomicBool::new(false));
        let result = Arc::new(AtomicU8::new(0));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let _server = async_io::block_on(
            zbus::connection::Builder::address(address)?
                .name("org.freedesktop.systemd1")?
                .serve_at(
                    "/org/freedesktop/systemd1",
                    SystemdMock {
                        subscribed: subscribed.clone(),
                        result: result.clone(),
                        calls: calls.clone(),
                    },
                )?
                .build(),
        )?;
        if std::env::var_os("NACRE_FREEZE_SYSTEMD_REFERENCE").is_some() {
            // Developer-only reference capture through a private manager.
            // Neither this daemon nor this object starts a real unit.
            subscribed.store(true, Ordering::SeqCst);
            let probe_message =
                std::env::var("NACRE_FREEZE_SYSTEMD_MESSAGE").unwrap_or_else(|_| "Break".into());
            let output = Command::new("systemd-run")
                .env_clear()
                .env("PATH", "/usr/bin")
                .env("HOME", folder.path())
                .env("DBUS_SESSION_BUS_ADDRESS", address)
                .args([
                    "--user",
                    "--collect",
                    "--no-block",
                    "--quiet",
                    "--unit=nacre-reminder-00000000000000000000000000000000",
                    "--on-active=6.0s",
                    "--timer-property=AccuracySec=1s",
                    "notify-send",
                    "-a",
                    "Nacre Reminders",
                    "-i",
                    "nacre-notifications",
                    "Reminder",
                    &probe_message,
                ])
                .output()?;
            let observed = calls.lock().map_err(|_| "Fixture lock failed")?;
            if let Some(call) = observed.first() {
                println!(
                    "OLD_SYSTEMD_PROPERTIES {}",
                    serde_json::to_string(
                        &json!({"unit":call.unit,"mode":call.mode,"properties":call.properties,"auxiliary":call.auxiliary})
                    )?
                );
            }
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(());
        }
        let mut request = TimerRequest {
            unit: "nacre-reminder-00000000000000000000000000000000".into(),
            seconds: 6.0,
            executable: "/usr/bin/notify-send".into(),
            argv: vec![
                "notify-send".into(),
                "-a".into(),
                "Nacre Reminders".into(),
                "-i".into(),
                "nacre-notifications".into(),
                "Reminder".into(),
                "Break".into(),
            ],
        };
        let reference: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/menu-systemd-reference.json"
        )))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing systemd references")?
        {
            *request
                .argv
                .last_mut()
                .ok_or("Missing notification message")? = case["message"]
                .as_str()
                .ok_or("Missing reference message")?
                .into();
            start_timer_at(&request, Some(address))?;
            let observed = calls.lock().map_err(|_| "Fixture lock failed")?;
            let call = observed.last().ok_or("Missing systemd call")?;
            assert_eq!(
                serde_json::to_value(
                    json!({"unit":call.unit,"mode":call.mode,"properties":call.properties,"auxiliary":call.auxiliary})
                )?,
                case["call"],
                "Full legacy transient properties"
            );
        }
        {
            let observed = calls.lock().map_err(|_| "Fixture lock failed")?;
            let call = &observed[0];
            assert_eq!(call.unit, format!("{}.timer", request.unit));
            assert_eq!(call.mode, "fail");
            assert_eq!(call.auxiliary[0].0, format!("{}.service", request.unit));
            let property = |name: &str| -> Result<OwnedValue> {
                Ok(call
                    .properties
                    .iter()
                    .find(|(key, _)| key == name)
                    .ok_or("Missing timer property")?
                    .1
                    .try_clone()?)
            };
            let schedule: Vec<(String, u64)> = property("TimersMonotonic")?.try_into()?;
            assert_eq!(schedule, vec![("OnActiveSec".into(), 6_000_000)]);
            assert_eq!(u64::try_from(property("AccuracyUSec")?)?, 1_000_000);
            assert!(!bool::try_from(property("RemainAfterElapse")?)?);
            let mut argv = request.argv.clone();
            argv[0] = "/usr/bin/notify-send".into();
            argv[6] = "Break".into();
            assert_eq!(
                String::try_from(property("CollectMode")?)?,
                "inactive-or-failed"
            );
            let exec = call.auxiliary[0]
                .1
                .iter()
                .find(|(name, _)| name == "ExecStart")
                .ok_or("Missing execution property")?
                .1
                .try_clone()?;
            let exec: Vec<(String, Vec<String>, bool)> = exec.try_into()?;
            assert_eq!(exec, vec![("/usr/bin/notify-send".into(), argv, false)]);
        }
        subscribed.store(false, Ordering::SeqCst);
        systemd_job_at(
            "StopUnit",
            &(format!("{}.timer", request.unit), "replace"),
            Some(address),
        )?;
        result.store(1, Ordering::SeqCst);
        let error =
            start_timer_at(&request, Some(address)).expect_err("Accepted failed job must fail");
        assert_eq!(error.to_string(), "Systemd job failed: failed");
        result.store(2, Ordering::SeqCst);
        let error =
            start_timer_at(&request, Some(address)).expect_err("Missing completion must time out");
        assert_eq!(error.to_string(), "Systemd job timed out after 5 seconds");
        Ok(())
    }

    fn snapshot(home: &Path) -> Result<Value> {
        fn visit(
            folder: &Path,
            home: &Path,
            files: &mut serde_json::Map<String, Value>,
        ) -> Result<()> {
            for item in fs::read_dir(folder)? {
                let path = item?.path();
                if path.is_dir() {
                    visit(&path, home, files)?;
                } else {
                    files.insert(path.strip_prefix(home)?.to_string_lossy().into_owned(), json!({"text":fs::read_to_string(&path)?,"mode":format!("0o{:o}",fs::metadata(path)?.mode() & 0o777)}));
                }
            }
            Ok(())
        }
        let mut files = serde_json::Map::new();
        visit(home, home, &mut files)?;
        Ok(Value::Object(files))
    }

    #[test]
    fn frozen_package_webapp_and_reminder_actions() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/menu-registry-reference.json"
        )))?;
        let ident = reference["uuid"].as_str().ok_or("Missing frozen UUID")?;
        let now = reference["time"].as_f64().ok_or("Missing frozen time")?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing frozen cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            for (name, text) in case["seed"].as_object().ok_or("Missing seed")? {
                let path = home.path().join(name);
                fs::create_dir_all(path.parent().ok_or("Missing fixture parent")?)?;
                fs::write(&path, text.as_str().ok_or("Invalid fixture text")?)?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            }
            let find = |name: &str| case["tools"][name].as_str().map(PathBuf::from);
            let read = |name: &str| -> Result<Value> {
                let file = paths.state.join(format!("menu/{name}.json"));
                if file.exists() {
                    Ok(serde_json::from_slice(&fs::read(file)?)?)
                } else {
                    Ok(json!([]))
                }
            };
            let mut calls = Vec::new();
            let args = &case["args"];
            let result = match case["operation"].as_str().ok_or("Missing operation")? {
                "package_command" => package_with(
                    args[0].as_str().ok_or("Missing action")?,
                    args[1].as_str().ok_or("Missing package")?,
                    find,
                )
                .map(|value| json!(value)),
                "create_webapp" => {
                    create_webapp_with(&paths, &args[0], &args[1], ident, || read("webapps"))
                }
                "remove_webapp" => remove_webapp_with(
                    &paths,
                    args[0].as_str().ok_or("Missing identifier")?,
                    || read("webapps"),
                ),
                "set_reminder" => reminder_with(
                    &paths,
                    &args[0],
                    &args[1],
                    ident,
                    || Ok(now),
                    find,
                    || read("reminders"),
                    |request| {
                        // The transport changed to D-Bus. Preserve the frozen
                        // schedule and notification argv; wire calls have their
                        // own isolated bus test rather than launching a unit here.
                        let mut argv = vec![
                            "systemd-run".into(),
                            "--user".into(),
                            "--collect".into(),
                            format!("--unit={}", request.unit),
                            format!("--on-active={}s", json::python_str(&json!(request.seconds))),
                            "--timer-property=AccuracySec=1s".into(),
                        ];
                        argv.extend(request.argv.clone());
                        calls.push(json!({"argv":argv,"timeout":5,"check":true}));
                        assert_eq!(
                            request.executable,
                            PathBuf::from("/fixture/bin/notify-send")
                        );
                        Ok(())
                    },
                ),
                "cancel_reminder" => cancel_with(
                    &paths,
                    args[0].as_str().ok_or("Missing unit")?,
                    || read("reminders"),
                    |unit| {
                        calls.push(json!({"argv":["systemctl","--user","stop",unit],"timeout":5,"check":true}));
                        Ok(())
                    },
                ),
                _ => return Err("Unsupported frozen operation".into()),
            };
            let outcome = match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":"ValueError","message":error.to_string()}),
            };
            assert_eq!(outcome, case["outcome"], "{}: result", case["name"]);
            let files = snapshot(home.path())?;
            assert_eq!(files, case["files"], "{}: files/modes", case["name"]);
            let mut deleted: Vec<_> = case["seed"]
                .as_object()
                .ok_or("Missing seed")?
                .keys()
                .filter(|name| files.get(*name).is_none())
                .cloned()
                .collect();
            deleted.sort();
            assert_eq!(
                json!(deleted),
                case["deleted"],
                "{}: deleted files",
                case["name"]
            );
            assert_eq!(
                json!(calls),
                case["calls"],
                "{}: action semantics",
                case["name"]
            );
        }
        Ok(())
    }

    #[test]
    fn rejected_timer_keeps_state_and_cache_unchanged() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let cache = paths.cache.join("menu/state.json");
        fs::create_dir_all(cache.parent().ok_or("Missing fixture cache parent")?)?;
        fs::write(&cache, "existing cache")?;
        let result = reminder_with(
            &paths,
            &json!(1),
            &json!("Break"),
            "00000000000000000000000000000000",
            || Err("Clock must not be read after rejected job".into()),
            |_| Some("/fixture/bin/notify-send".into()),
            || Err("Registry must not be read after rejected job".into()),
            |_| Err("Systemd job failed: failed".into()),
        );
        assert_eq!(
            result.expect_err("Rejected job must fail").to_string(),
            "Systemd job failed: failed"
        );
        assert!(!paths.state.join("menu/reminders.json").exists());
        assert_eq!(fs::read_to_string(cache)?, "existing cache");
        assert_eq!(fs::read(paths.state.join("menu/reminders.lock"))?, b"");
        Ok(())
    }
}
