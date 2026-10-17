//! Strict byte bridge for QML: Qt's SplitParser cannot bound an unfinished line
//! or distinguish invalid UTF-8 from replacement characters. No state or replay.
use super::{client, socket};
use crate::Result;
use nacre_protocol::{framing, ErrorCode, Failure};
use rustix::fs::{Mode, OFlags};
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    net::unix::pipe::{Receiver, Sender},
};

async fn forward<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(read: R, mut write: W) -> Result<()> {
    let mut reader = framing::Reader::new(BufReader::new(read));
    while let Some(bytes) = reader.next_raw().await? {
        tokio::time::timeout(Duration::from_secs(10), async {
            write.write_all(bytes).await?;
            write.write_all(b"\n").await?;
            write.flush().await
        })
        .await
        .map_err(|_| Failure::new(ErrorCode::Timeout, "Attach output stalled", true))??;
    }
    Ok(())
}

async fn bridge() -> Result<i32> {
    // Reopen rather than dup: nonblocking flags must not affect the parent's
    // copies. Checked pipe adapters need no resident stdin reader thread.
    let input = Receiver::from_owned_fd(rustix::fs::open(
        "/proc/self/fd/0",
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?)?;
    let output = Sender::from_owned_fd(rustix::fs::open(
        "/proc/self/fd/1",
        OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?)?;
    let stream = client::transport(&socket::path()?).await?;
    let (read, write) = stream.into_split();
    tokio::select! {
        result = forward(input, write) => { result?; Ok(0) },
        result = forward(read, output) => {
            result?;
            Err(Failure::new(ErrorCode::Unavailable,
                "Daemon disconnected; pending actions may already have completed", true).into())
        },
    }
}

pub async fn run() -> Result<i32> {
    tokio::select! {
        closed = client::output_closed() => { closed?; Ok(0) },
        result = bridge() => result,
    }
}
