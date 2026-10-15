//! A crash notice must stay on the injected bus, nice 10, with no service activation.
use nacre_helper::Result;
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
use wait_timeout::ChildExt;
use zbus::{
    message::{Flags, Header},
    zvariant::OwnedValue,
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[derive(Debug)]
struct Notice {
    app: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    no_auto_start: bool,
}
struct Server {
    seen: Arc<Mutex<Vec<Notice>>>,
    ready: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    #[allow(clippy::too_many_arguments)] // The freedesktop Notify wire signature has eight arguments.
    fn notify(
        &self,
        app_name: String,
        _replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
        #[zbus(header)] header: Header<'_>,
    ) -> u32 {
        self.seen.lock().unwrap().push(Notice {
            app: app_name,
            summary,
            body,
            actions,
            no_auto_start: header.primary().flags().contains(Flags::NoAutoStart),
        });
        self.ready.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        42
    }
}
#[test]
fn native_notice_uses_only_private_bus_and_lowers_its_own_priority() -> Result<()> {
    let home = tempfile::Builder::new().prefix("nn-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let config = home.path().join("bus.xml");
    fs::write(&config,format!("<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",home.path().join("bus").display()))?;
    let mut bus = Process(
        Command::new("/usr/bin/dbus-daemon")
            .arg("--nofork")
            .arg("--print-address")
            .arg("--config-file")
            .arg(config)
            .env_clear()
            .env("HOME", home.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
        .read_line(&mut address)?;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (ready, received) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let _server = zbus::blocking::connection::Builder::address(address.trim())?
        .method_timeout(Duration::from_secs(5))
        .name("org.freedesktop.Notifications")?
        .serve_at(
            "/org/freedesktop/Notifications",
            Server {
                seen: seen.clone(),
                ready,
                release: Mutex::new(resume),
            },
        )?
        .build()?;
    let mut worker = Process(
        Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .args(["daemon-notice", &std::process::id().to_string()])
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", "")
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}", home.path().join("absent-system").display()),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    received.recv_timeout(Duration::from_secs(5))?;
    let stat = fs::read_to_string(format!("/proc/{}/stat", worker.0.id()))?;
    let fields: Vec<_> = stat[stat.rfind(')').ok_or("Invalid worker stat")? + 2..]
        .split_whitespace()
        .collect();
    assert!(fields[16].parse::<i32>()? >= 10);
    release.send(())?;
    assert!(worker
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Notice worker did not exit")?
        .success());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].app, "Nacre");
    assert_eq!(seen[0].summary, "Nacre safe mode");
    assert!(seen[0].body.contains("nacrectl status"));
    assert!(seen[0].actions.is_empty());
    assert!(seen[0].no_auto_start);
    Ok(())
}
