//! Bounded, newline-delimited Codex app-server requests for the usage wrapper.
use crate::Result;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

const MAX_RESPONSE: usize = 4 * 1024 * 1024;

pub struct Peer {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    buffer: Vec<u8>,
    deadline: Instant,
    sequence: u64,
}

impl Drop for Peer {
    fn drop(&mut self) {
        // Only the exact process created by this Peer is signalled and reaped.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Peer {
    pub fn start(mut command: Command, timeout: Duration) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process::restore_child_signals(&mut command);
        let mut child = command.spawn()?;
        let streams = (|| -> Result<_> {
            let input = child.stdin.take().ok_or("Missing Codex input")?;
            let output = child.stdout.take().ok_or("Missing Codex output")?;
            for fd in [
                &input as &dyn std::os::fd::AsFd,
                &output as &dyn std::os::fd::AsFd,
            ] {
                let flags = OFlag::from_bits_truncate(fcntl(fd.as_fd(), FcntlArg::F_GETFL)?);
                fcntl(fd.as_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
            }
            Ok((input, output))
        })();
        let (input, output) = match streams {
            Ok(streams) => streams,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            child,
            input,
            output,
            buffer: Vec::new(),
            deadline: Instant::now() + timeout,
            sequence: 0,
        })
    }

    fn check_deadline(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            return Err("Reading Codex usage timed out".into());
        }
        Ok(())
    }

    fn send(&mut self, value: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        let mut offset = 0;
        while offset < bytes.len() {
            self.check_deadline()?;
            match self.input.write(&bytes[offset..]) {
                Ok(0) => return Err("Codex closed the connection".into()),
                Ok(count) => offset += count,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        if params.as_object().is_some_and(|value| value.is_empty()) {
            self.send(json!({"method":method}))
        } else {
            self.send(json!({"method":method,"params":params}))
        }
    }

    fn call(&mut self, method: &str) -> Result<Value> {
        let params = if method == "initialize" {
            json!({"clientInfo":{"name":"nacre_workflow","version":"1.0","title":"Nacre AI"}})
        } else {
            json!({})
        };
        self.call_with_params(method, params)
    }

    pub fn call_with_params(&mut self, method: &str, params: Value) -> Result<Value> {
        self.sequence += 1;
        let identity = self.sequence;
        self.send(json!({"id":identity,"method":method,"params":params}))?;
        loop {
            self.check_deadline()?;
            while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=end).collect();
                let value: Value = serde_json::from_slice(&line)?;
                if value.get("id").and_then(Value::as_u64) != Some(identity) {
                    continue;
                }
                // Provider error messages can contain sensitive request details.
                if value.get("error").is_some() {
                    return Err("Codex usage request failed".into());
                }
                return value
                    .get("result")
                    .cloned()
                    .ok_or_else(|| "Invalid Codex usage response".into());
            }
            let mut chunk = [0u8; 65536];
            match self.output.read(&mut chunk) {
                Ok(0) => return Err("Codex closed the connection".into()),
                Ok(count) => {
                    if self.buffer.len() + count > MAX_RESPONSE {
                        return Err("Codex response exceeds the workflow limit".into());
                    }
                    self.buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

/// Read quota and optional older-client activity, then close the owned peer.
pub fn usage(command: Command, timeout: Duration) -> Result<(Value, Value)> {
    let mut peer = Peer::start(command, timeout)?;
    peer.call("initialize")?;
    peer.notify("initialized", json!({}))?;
    let rates = peer.call("account/rateLimits/read")?;
    let activity = peer.call("account/usage/read").unwrap_or(Value::Null);
    Ok((rates, activity))
}

/// Forward status-line input without letting a non-reading delegate block capture.
pub fn delegate(command: &str, input: &[u8], timeout: Duration) -> Result<()> {
    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut command_builder = Command::new("sh");
    command_builder.args(["-c", command]).stdin(Stdio::piped());
    crate::process::restore_child_signals(&mut command_builder);
    let mut child = Owned(command_builder.spawn()?);
    let mut pipe = child.0.stdin.take().ok_or("Missing delegate input")?;
    let flags = OFlag::from_bits_truncate(fcntl(&pipe, FcntlArg::F_GETFL)?);
    fcntl(&pipe, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    let deadline = Instant::now() + timeout;
    let mut offset = 0;
    while offset < input.len() {
        if Instant::now() >= deadline {
            return Err("Status-line delegate timed out".into());
        }
        match pipe.write(&input[offset..]) {
            Ok(0) => break,
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
    drop(pipe);
    loop {
        if child.0.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("Status-line delegate timed out".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(script: &str) -> Command {
        let mut command = Command::new("python3");
        command.args(["-c", script]);
        // No fixture inherits a provider home, keyring or authentication.
        command.env_clear().env("PATH", "/usr/bin:/bin");
        command
    }

    #[test]
    fn ndjson_handshake_ignores_notifications_and_tolerates_old_activity_api() {
        let command = fixture(
            r#"
import json, sys
def read(): return json.loads(sys.stdin.readline())
def send(v): print(json.dumps(v), flush=True)
request = read()
assert request['method'] == 'initialize'
send({'id':request['id'], 'result':{}})
assert read() == {'method':'initialized'}
request = read()
assert request['method'] == 'account/rateLimits/read'
send({'method':'notification'})
send({'id':request['id'], 'result':{'rateLimits':{'primary':{'usedPercent':25}}}})
request = read()
assert request['method'] == 'account/usage/read'
send({'id':request['id'], 'error':{'message':'fixture sensitive detail'}})
sys.stdin.read()
"#,
        );
        let (rates, activity) = usage(command, Duration::from_secs(2)).unwrap();
        assert_eq!(rates["rateLimits"]["primary"]["usedPercent"], 25);
        assert!(activity.is_null());
    }

    #[test]
    fn persistent_nonresponding_provider_is_bounded() {
        let start = Instant::now();
        let result = usage(
            fixture("import time; time.sleep(10)"),
            Duration::from_millis(50),
        );
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let result = usage(
            fixture("import sys; sys.stdout.write('x' * (5*1024*1024)); sys.stdout.flush()"),
            Duration::from_secs(2),
        );
        assert!(result.unwrap_err().to_string().contains("exceeds"));
    }

    #[test]
    fn delegate_deadline_includes_a_full_input_pipe() {
        let start = Instant::now();
        let result = delegate(
            "exec sleep 10",
            &vec![b'x'; 1024 * 1024],
            Duration::from_millis(50),
        );
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
