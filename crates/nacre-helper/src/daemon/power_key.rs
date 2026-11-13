//! Holds logind's `handle-power-key` inhibitor for the daemon's lifetime so a
//! short power press only reaches the compositor's XF86PowerOff lock binding
//! (N0312; replaces the `systemd-inhibit … sleep infinity` unit).
use super::behaviours::proxy;
use std::time::Duration;

const LOGIN1: &str = "org.freedesktop.login1";

/// Returns the inhibitor pipe; dropping it releases the lock. Failures only
/// warn: the daemon must start without logind (tests, containers).
pub async fn inhibit() -> Option<zbus::zvariant::OwnedFd> {
    let taken = tokio::time::timeout(Duration::from_secs(3), async {
        let connection = zbus::Connection::system().await?;
        proxy(
            &connection,
            LOGIN1,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?
        .call(
            "Inhibit",
            &(
                "handle-power-key",
                "Nacre",
                "The compositor binds the power key to lock",
                "block",
            ),
        )
        .await
    })
    .await;
    match taken {
        Ok(Ok(fd)) => Some(fd),
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "Power-key inhibitor unavailable");
            None
        }
        Err(_) => {
            tracing::warn!("Power-key inhibitor timed out");
            None
        }
    }
}
