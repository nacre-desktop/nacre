//! Exercise the native dashboard's alternate-screen navigation with a private PTY.
use nix::pty::{openpty, Winsize};
use std::{
    fs::File,
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

fn run_dashboard(
    keys: &[u8],
) -> Result<(std::process::ExitStatus, String), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let window = Winsize {
        ws_row: 30,
        ws_col: 90,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pair = openpty(Some(&window), None)?;
    let reader = File::from(pair.master);
    let mut writer = reader.try_clone()?;
    let slave = File::from(pair.slave);
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["nacre-ai"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave))
        .spawn()?;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let frames = Arc::new(AtomicUsize::new(0));
    let seen = frames.clone();
    let output = thread::spawn(move || {
        let mut output = Vec::new();
        let mut reader = reader;
        let mut chunk = [0; 4096];
        let mut sent_ready = false;
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => {
                    output.extend_from_slice(&chunk[..count]);
                    seen.store(
                        output.windows(9).filter(|w| *w == b"WORKSPACE").count(),
                        Ordering::SeqCst,
                    );
                    if !sent_ready && output.windows(6).any(|window| window == b"?1049h") {
                        let _ = ready_tx.send(());
                        sent_ready = true;
                    }
                }
                Err(error) if error.raw_os_error() == Some(nix::libc::EIO) => break,
                Err(error) => return Err(error),
            }
        }
        Ok::<_, std::io::Error>(output)
    });
    // Waits are event-driven; the caps only turn a hang into a failure.
    let wait_for = |count: usize| {
        let until = Instant::now() + Duration::from_secs(60);
        while frames.load(Ordering::SeqCst) < count {
            if Instant::now() > until {
                return Err(std::io::Error::other("dashboard did not render"));
            }
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    };
    ready_rx.recv_timeout(Duration::from_secs(60))?;
    wait_for(1)?;
    for key in keys {
        let before = frames.load(Ordering::SeqCst);
        writer.write_all(std::slice::from_ref(key))?;
        writer.flush()?;
        if *key == 27 {
            // The dashboard reads a lone Esc only after its escape timeout, and
            // takes a following byte as part of a sequence. Wait for the redraw
            // that answers the Esc instead of guessing with a sleep.
            wait_for(before + 1)?;
        }
    }
    let status = child
        .wait_timeout(Duration::from_secs(60))?
        .ok_or_else(|| std::io::Error::other("dashboard did not close after selecting Close"))?;
    let output = output
        .join()
        .map_err(|_| std::io::Error::other("PTY reader panicked"))??;
    Ok((status, String::from_utf8_lossy(&output).into_owned()))
}

#[test]
fn root_menu_renders_and_keyboard_navigation_closes_on_the_final_action(
) -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(b"jjjjj\r")?;
    assert!(status.success());
    assert!(output.contains("\x1b[?1049h"));
    assert!(output.contains("New chat"));
    assert!(output.contains("Close"));
    assert!(output.contains("\x1b[?1049l"));
    Ok(())
}

#[test]
fn root_escape_cancels_only_the_current_selection() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(b"\x1bjjjjj\r")?;
    assert!(status.success());
    assert!(output.matches("WORKSPACE").count() >= 2, "{output:?}");
    assert!(output.contains("\x1b[?1049l"));
    Ok(())
}
