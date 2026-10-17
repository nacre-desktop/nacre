//! The actual stdio bridge on private sockets: byte parity, strict framing,
//! bounded shutdown and no replay. No compositor, bus or ambient session.
use nacre_helper::Result;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

struct Process(Child, PathBuf);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn fixture() -> Result<(tempfile::TempDir, UnixListener, Process)> {
    let home = tempfile::Builder::new().prefix("na-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let socket = home.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let log = home.path().join("attach.log");
    let child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["ctl", "attach"])
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_RUNTIME_DIR", home.path())
        .env("NACRE_DAEMON_SOCKET", socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(&log)?)
        .spawn()?;
    Ok((home, listener, Process(child, log)))
}
fn accept(listener: &UnixListener) -> Result<UnixStream> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;
                return Ok(stream);
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn stopped(child: &mut Process, success: bool) -> Result<()> {
    // wait_timeout closes Child.stdin. Retain our writer separately so this
    // assertion cannot turn a protocol error/daemon loss into a clean UI EOF.
    let input = child.0.stdin.take();
    let status = child
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Attach retained a closed connection or pipe")?;
    drop(input);
    assert_eq!(
        status.success(),
        success,
        "Attach stderr: {}",
        fs::read_to_string(&child.1)?
    );
    Ok(())
}

#[test]
fn actual_bridge_preserves_wire_extensions_and_exits_on_each_pipe_and_daemon_loss() -> Result<()> {
    let (_home, listener, mut child) = fixture()?;
    let mut server = BufReader::new(accept(&listener)?);
    let mut input = child.0.stdin.take().ok_or("Missing input")?;
    let mut output = BufReader::new(child.0.stdout.take().ok_or("Missing output")?);
    let hello = b" {\"t\":\"hello\",\"proto\":1,\"client\":\"qml\",\"future\":\"caf\xc3\xa9\"}\r\n";
    input.write_all(&hello[..hello.len() - 4])?;
    input.write_all(&hello[hello.len() - 4..])?;
    let mut bytes = Vec::new();
    server.read_until(b'\n', &mut bytes)?;
    assert_eq!(bytes, hello);
    let welcome = b"{\"t\":\"welcome\",\"proto\":1,\"min\":1,\"boot\":\"fixture\",\"topics\":{},\"methods\":{},\"future\":{\"value\":3}}\n";
    server.get_mut().write_all(welcome)?;
    bytes.clear();
    output.read_until(b'\n', &mut bytes)?;
    assert_eq!(bytes, welcome);
    // No new message should be required to notice the UI closing stdout.
    drop(output);
    stopped(&mut child, true)?;
    drop(input);

    let (_home, listener, mut child) = fixture()?;
    let _server = accept(&listener)?;
    drop(child.0.stdin.take());
    stopped(&mut child, true)?;

    let (_home, listener, mut child) = fixture()?;
    let mut server = BufReader::new(accept(&listener)?);
    child
        .0
        .stdin
        .as_mut()
        .ok_or("Missing input")?
        .write_all(b"{\"t\":\"call\",\"id\":7,\"m\":\"ui.dispatch\",\"p\":{}}\n")?;
    bytes.clear();
    server.read_until(b'\n', &mut bytes)?;
    assert!(!bytes.is_empty());
    drop(server);
    // Keep stdin open: a dispatched action with a lost reply must end the link,
    // never reconnect or fall back to another executor.
    stopped(&mut child, false)?;
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    Ok(())
}

#[test]
fn invalid_bytes_duplicates_and_oversized_unterminated_lines_never_cross_bridge() -> Result<()> {
    let mut oversized = b"{\"t\":\"ping\",\"id\":1,\"future\":\"".to_vec();
    oversized.resize(nacre_protocol::MAX_FRAME + 1, b'a');
    let cases = [
        b"{\"t\":\"ping\",\"id\":\"\xff\"}\n".to_vec(),
        b"{\"t\":\"ping\",\"id\":1,\"id\":2}\n".to_vec(),
        b"{\"t\":\"ping\",\"id\":9007199254740992}\n".to_vec(),
        oversized,
    ];
    for (case, bytes) in cases.iter().enumerate() {
        for from_ui in [true, false] {
            eprintln!("Malformed frame case {case}, from UI: {from_ui}");
            let (_home, listener, mut child) = fixture()?;
            let mut server = accept(&listener)?;
            if from_ui {
                child
                    .0
                    .stdin
                    .as_mut()
                    .ok_or("Missing input")?
                    .write_all(bytes)?;
            } else {
                server.write_all(bytes)?;
            }
            // Both writing endpoints remain open: size rejection cannot wait
            // for a newline, EOF, another daemon event or the read deadline.
            stopped(&mut child, false)?;
            let mut crossed = Vec::new();
            if from_ui {
                server.read_to_end(&mut crossed)?;
            } else {
                child
                    .0
                    .stdout
                    .as_mut()
                    .ok_or("Missing output")?
                    .read_to_end(&mut crossed)?;
            }
            assert!(crossed.is_empty(), "Invalid frame reached its destination");
        }
    }
    let (_home, listener, mut child) = fixture()?;
    let mut server = accept(&listener)?;
    child
        .0
        .stdin
        .as_mut()
        .ok_or("Missing input")?
        .write_all(b"{\"t\":\"ping\"")?;
    drop(child.0.stdin.take());
    stopped(&mut child, false)?;
    let mut crossed = Vec::new();
    server.read_to_end(&mut crossed)?;
    assert!(crossed.is_empty());
    Ok(())
}
