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

struct ChildGuard {
    child: std::process::Child,
    finished: bool,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(self.child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = self.child.wait();
        }
    }
}

fn run_dashboard(
    events: &[&[u8]],
    terminate: bool,
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
    let terminal_probe = slave.try_clone()?;
    let original_mode = nix::sys::termios::tcgetattr(&terminal_probe)?;
    let child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["nacre-ai"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave))
        .spawn()?;
    let mut child = ChildGuard {
        child,
        finished: false,
    };
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
    if terminate {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(child.child.id() as i32),
            nix::sys::signal::Signal::SIGTERM,
        )?;
    }
    for (index, event) in events.iter().enumerate() {
        let before = frames.load(Ordering::SeqCst);
        writer.write_all(event)?;
        writer.flush()?;
        if index + 1 < events.len() {
            // Each complete key/mouse event redraws the menu. Waiting for that
            // redraw keeps the next event from being mistaken for a sequence tail.
            wait_for(before + 1)?;
        }
    }
    let status = child
        .child
        .wait_timeout(Duration::from_secs(60))?
        .ok_or_else(|| std::io::Error::other("dashboard did not close after selecting Close"))?;
    child.finished = true;
    let restored_mode = nix::sys::termios::tcgetattr(&terminal_probe)?;
    assert_eq!(restored_mode.local_flags, original_mode.local_flags);
    assert_eq!(restored_mode.input_flags, original_mode.input_flags);
    drop(terminal_probe);
    let output = output
        .join()
        .map_err(|_| std::io::Error::other("PTY reader panicked"))??;
    Ok((status, String::from_utf8_lossy(&output).into_owned()))
}

#[test]
fn root_menu_renders_and_keyboard_navigation_closes_on_the_final_action(
) -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(&[b"\x1b[B", b"\x1bOB", b"jjj", b"\r"], false)?;
    assert!(status.success());
    assert!(output.contains("\x1b[?1049h"));
    assert!(output.contains("New chat"));
    assert!(output.contains("Close"));
    assert!(output.contains("\x1b[?1049l"));
    Ok(())
}

#[test]
fn root_escape_cancels_only_the_current_selection() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(&[b"\x1b", b"jjjjj", b"\r"], false)?;
    assert!(status.success());
    assert!(output.matches("WORKSPACE").count() >= 2, "{output:?}");
    assert!(output.contains("\x1b[?1049l"));
    Ok(())
}

#[test]
fn root_menu_accepts_page_down_and_kitty_enter() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(&[b"\x1b[6~", b"\x1b[13;1u"], false)?;
    assert!(status.success());
    assert!(output.contains("\x1b[?1000h\x1b[?1006h"));
    assert!(output.contains("\x1b[?1006l\x1b[?1000l"));
    Ok(())
}

#[test]
fn root_menu_home_and_page_up_change_selection() -> Result<(), Box<dyn std::error::Error>> {
    let (status, _) = run_dashboard(&[b"\x1b[8~", b"\x1b[H", b"jjjjj", b"\r"], false)?;
    assert!(status.success());
    let (status, _) = run_dashboard(&[b"\x1b[8~", b"\x1b[5~", b"jjjjj", b"\r"], false)?;
    assert!(status.success());
    Ok(())
}

#[test]
fn root_menu_wheel_moves_selection() -> Result<(), Box<dyn std::error::Error>> {
    let wheel_down = b"\x1b[<65;15;13M";
    let (status, _) = run_dashboard(
        &[
            b"\x1b[H", wheel_down, wheel_down, wheel_down, wheel_down, wheel_down, b"\r",
        ],
        false,
    )?;
    assert!(status.success());
    Ok(())
}

#[test]
fn root_menu_accepts_keypad_enter() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(&[b"\x1b[8~", b"\x1b[57414u"], false)?;
    assert!(status.success());
    assert!(output.contains("\x1b[?1049l"));
    Ok(())
}

#[test]
fn root_menu_mouse_click_selects_a_row() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(&[b"\x1b[<0;15;18M"], false)?;
    assert!(status.success());
    assert!(output.contains("\x1b[?1049l"));
    Ok(())
}

#[test]
fn signal_exit_restores_terminal_modes() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = run_dashboard(&[], true)?;
    assert!(!status.success());
    assert!(output.contains("\x1b[?1006l\x1b[?1000l"));
    assert!(output.contains("\x1b[?25h\x1b[?1049l"));
    Ok(())
}
