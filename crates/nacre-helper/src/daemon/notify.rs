//! sd_notify without libsystemd. Also supports abstract Unix datagram endpoints.
use crate::Result;
use std::{
    ffi::OsStr,
    os::{
        linux::net::SocketAddrExt,
        unix::{
            ffi::OsStrExt,
            net::{SocketAddr, UnixDatagram},
        },
    },
    path::Path,
    time::Duration,
};
pub async fn send_to(destination: &OsStr, message: &str) -> Result<()> {
    let bytes = destination.as_bytes();
    let address = if bytes.first() == Some(&b'@') {
        SocketAddr::from_abstract_name(&bytes[1..])?
    } else {
        SocketAddr::from_pathname(Path::new(destination))?
    };
    let socket = UnixDatagram::unbound()?;
    socket.set_nonblocking(true)?;
    socket.connect_addr(&address)?;
    let socket = tokio::net::UnixDatagram::from_std(socket)?;
    tokio::time::timeout(Duration::from_secs(1), socket.send(message.as_bytes())).await??;
    Ok(())
}
pub async fn ready() -> Result<()> {
    if let Some(socket) = std::env::var_os("NOTIFY_SOCKET") {
        send_to(&socket, "READY=1\nSTATUS=Nacre daemon ready").await?;
    }
    Ok(())
}
pub async fn watchdog() -> Result<()> {
    let Some(socket) = std::env::var_os("NOTIFY_SOCKET") else {
        std::future::pending::<()>().await;
        return Ok(());
    };
    if std::env::var("WATCHDOG_PID")
        .ok()
        .is_some_and(|pid| pid != std::process::id().to_string())
    {
        std::future::pending::<()>().await;
        return Ok(());
    }
    let Some(period) = std::env::var("WATCHDOG_USEC")
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .filter(|n| *n > 0)
    else {
        std::future::pending::<()>().await;
        return Ok(());
    };
    let period = Duration::from_micros((period / 2).max(1));
    loop {
        tokio::time::sleep(period).await;
        send_to(&socket, "WATCHDOG=1").await?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn private_datagram_gets_ready_and_watchdog() -> Result<()> {
        let home = tempfile::tempdir()?;
        let path = home.path().join("notify");
        let server = UnixDatagram::bind(&path)?;
        server.set_read_timeout(Some(Duration::from_secs(1)))?;
        send_to(path.as_os_str(), "READY=1").await?;
        let mut bytes = [0; 64];
        let n = server.recv(&mut bytes)?;
        assert_eq!(&bytes[..n], b"READY=1");
        send_to(path.as_os_str(), "WATCHDOG=1").await?;
        let n = server.recv(&mut bytes)?;
        assert_eq!(&bytes[..n], b"WATCHDOG=1");
        Ok(())
    }
}
