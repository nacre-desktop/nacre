//! Rare crash notification work stays outside the persistent service's runtime.
use crate::Result;
use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};
use tokio::{process::Command, sync::Semaphore};
#[derive(Clone)]
pub struct Workers {
    slots: Arc<Semaphore>,
}
impl Default for Workers {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(2)),
        }
    }
}
impl Workers {
    pub async fn safe_notice(&self) -> Result<()> {
        let _permit = self.slots.clone().acquire_owned().await?;
        // Keep the daemon's exact image even if an upgrade replaced its pathname.
        let mut command = Command::new("/proc/self/exe");
        command
            .args(["daemon-notice", &std::process::id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
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
        let mut child = command.spawn()?;
        let result = tokio::time::timeout(Duration::from_secs(6), child.wait()).await;
        match result {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(Ok(_)) => Err("Safe-mode notification service is unavailable".into()),
            Ok(Err(error)) => Err(error.into()),
            Err(_) => {
                child.kill().await?;
                Err("Safe-mode notification worker timed out".into())
            }
        }
    }
}
pub fn notice(arguments: &[String]) -> Result<i32> {
    let result = (|| -> Result<()> {
        let [parent] = arguments else {
            return Err("Expected daemon parent PID".into());
        };
        let parent = parent.parse::<u32>()?;
        rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::TERM))?;
        if rustix::process::getppid().map(|pid| pid.as_raw_nonzero().get() as u32) != Some(parent) {
            return Err("Daemon parent ended".into());
        }
        if rustix::process::getpriority_process(None)? < 10 {
            rustix::process::setpriority_process(None, 10)?;
        }
        let connection = zbus::blocking::connection::Builder::session()?
            .method_timeout(Duration::from_secs(5))
            .build()?;
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )?;
        let id:Option<u32>=proxy.call_with_flags("Notify",zbus::proxy::MethodFlags::NoAutoStart.into(),&("Nacre",0u32,"dialog-warning","Nacre safe mode","Some desktop services are paused after repeated crashes. Run nacrectl status for details.",Vec::<String>::new(),HashMap::<String,zvariant::Value<'static>>::new(),0i32))?;
        id.ok_or("Missing notification acknowledgement")?;
        Ok(())
    })();
    // The parent reports this outcome. Do not copy D-Bus addresses or raw errors into crash logs.
    Ok(if result.is_ok() { 0 } else { 1 })
}
