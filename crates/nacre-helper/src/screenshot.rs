//! Region and full-screen capture helpers. Region capture intentionally writes
//! PNG bytes to the clipboard without saving a file.
use crate::{i18n::tr, paths::Paths, Result};
use chrono::Local;
use std::{
    fs,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Command, ExitStatus, Output, Stdio},
};

fn create_private_output(folder: &Path) -> Result<std::path::PathBuf> {
    fs::create_dir_all(folder)?;
    let path = folder.join(format!(
        "screenshot_{}.png",
        Local::now().format("%Y%m%d_%H%M%S_%6f")
    ));
    let _file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    Ok(path)
}

fn notification_args(message: &str) -> Vec<String> {
    vec![
        "-a".into(),
        "Nacre Capture".into(),
        "-i".into(),
        "nacre-notifications".into(),
        "Screenshot".into(),
        message.into(),
    ]
}
fn notify(message: &str) {
    let _ = Command::new("notify-send")
        .args(notification_args(message))
        .status();
}

trait CaptureRunner {
    fn run(&mut self, args: &[String], input: Option<&[u8]>) -> Result<Output>;
}
fn called_process_error(args: &[String], status: &ExitStatus) -> crate::Error {
    let command = args
        .iter()
        .map(|arg| format!("'{}'", arg.replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Command '[{command}]' returned non-zero exit status {}.",
        status.code().unwrap_or(1)
    )
    .into()
}
struct NativeRunner;
impl CaptureRunner for NativeRunner {
    fn run(&mut self, args: &[String], input: Option<&[u8]>) -> Result<Output> {
        use std::io::Write;
        let Some(program) = args.first() else {
            return Err("Missing screenshot command".into());
        };
        let mut command = Command::new(program);
        command.args(&args[1..]);
        if let Some(bytes) = input {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .ok_or("Could not open clipboard input")?
                .write_all(bytes)?;
            return Ok(child.wait_with_output()?);
        }
        Ok(command.output()?)
    }
}

fn capture_with(
    mode: &str,
    paths: &Paths,
    runner: &mut impl CaptureRunner,
) -> Result<Option<String>> {
    match mode {
        "region" => {
            let selection = runner.run(&["slurp".into()], None)?;
            if !selection.status.success() {
                return Ok(None);
            }
            let geometry = String::from_utf8_lossy(&selection.stdout).trim().to_owned();
            if geometry.is_empty() {
                return Ok(None);
            }
            let args = vec!["grim".into(), "-g".into(), geometry, "-".into()];
            let image = runner.run(&args, None)?;
            if !image.status.success() {
                return Err(called_process_error(&args, &image.status));
            }
            let copy_args = vec!["wl-copy".into(), "--type".into(), "image/png".into()];
            let copied = runner.run(&copy_args, Some(&image.stdout))?;
            if !copied.status.success() {
                return Err(called_process_error(&copy_args, &copied.status));
            }
            Ok(Some(tr(
                "helper.screenshot.copied-selection-to-clipboard",
                "Copied selection to clipboard",
            )))
        }
        "full" => {
            let path = create_private_output(&paths.screenshots())?;
            let args = vec!["grim".into(), path.to_string_lossy().into_owned()];
            let image = runner.run(&args, None)?;
            if !image.status.success() {
                return Err(called_process_error(&args, &image.status));
            }
            let bytes = fs::read(&path)?;
            let copy_args = vec!["wl-copy".into(), "--type".into(), "image/png".into()];
            let copied = runner.run(&copy_args, Some(&bytes))?;
            if !copied.status.success() {
                return Err(called_process_error(&copy_args, &copied.status));
            }
            Ok(Some(tr(
                "helper.screenshot.saved-to-pictures-screenshots-and-copied-to-clipboard",
                "Saved to Pictures/Screenshots and copied to clipboard",
            )))
        }
        _ => Err(tr(
            "helper.screenshot.unknown-screenshot-mode",
            "Unknown screenshot mode",
        )
        .into()),
    }
}
pub fn capture(mode: &str, paths: &Paths) -> Result<Option<String>> {
    capture_with(mode, paths, &mut NativeRunner)
}

pub fn run(args: &[String]) -> Result<i32> {
    let [mode] = args else {
        return Err("Expected region or full".into());
    };
    match capture(mode, &Paths::environment()?) {
        Ok(Some(message)) => {
            notify(&message);
            Ok(0)
        }
        Ok(None) => Ok(0),
        Err(error) => {
            let message = tr("capture.notification.failed", "Capture failed: {error}");
            notify(&message.replace("{error}", &error.to_string()));
            let _ = error;
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, os::unix::process::ExitStatusExt};
    struct Fixture {
        outputs: VecDeque<Output>,
        commands: Vec<Vec<String>>,
        inputs: Vec<Option<Vec<u8>>>,
    }
    impl Fixture {
        fn new(outputs: impl IntoIterator<Item = (i32, Vec<u8>)>) -> Self {
            Self {
                outputs: outputs
                    .into_iter()
                    .map(|(code, stdout)| Output {
                        status: std::process::ExitStatus::from_raw(code << 8),
                        stdout,
                        stderr: Vec::new(),
                    })
                    .collect(),
                commands: Vec::new(),
                inputs: Vec::new(),
            }
        }
    }
    impl CaptureRunner for Fixture {
        fn run(&mut self, args: &[String], input: Option<&[u8]>) -> Result<Output> {
            self.commands.push(args.to_vec());
            self.inputs.push(input.map(<[u8]>::to_vec));
            let output = self
                .outputs
                .pop_front()
                .ok_or("Missing screenshot command fixture")?;
            if args.first().is_some_and(|program| program == "grim")
                && args.len() == 2
                && args[1] != "-"
                && output.status.success()
            {
                assert_eq!(fs::metadata(&args[1])?.permissions().mode() & 0o777, 0o600);
                fs::write(&args[1], &output.stdout)?;
            }
            Ok(output)
        }
    }
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn unknown_mode_is_rejected() {
        assert_eq!(
            capture("other", &Paths::new(Path::new("/tmp/nacre-test")))
                .unwrap_err()
                .to_string(),
            "Unknown screenshot mode"
        );
    }
    #[test]
    fn command_failure_matches_python_called_process_error() {
        let args = vec!["grim".into(), "-g".into(), "0,0 800x600".into(), "-".into()];
        let status = std::process::ExitStatus::from_raw(1 << 8);
        assert_eq!(
            called_process_error(&args, &status).to_string(),
            "Command '['grim', '-g', '0,0 800x600', '-']' returned non-zero exit status 1."
        );
    }
    #[test]
    fn notification_identifies_nacre_capture() {
        let args = notification_args("Saved");
        assert_eq!(
            args[args.iter().position(|arg| arg == "-a").unwrap() + 1],
            "Nacre Capture"
        );
        assert_eq!(
            args[args.iter().position(|arg| arg == "-i").unwrap() + 1],
            "nacre-notifications"
        );
    }
    #[test]
    fn cancelled_selection_never_captures_or_clears_clipboard() {
        for code in [1, 0] {
            let mut runner = Fixture::new([(code, Vec::new())]);
            assert!(
                capture_with("region", &Paths::new("/tmp/nacre-test"), &mut runner)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(runner.commands, vec![vec!["slurp".to_owned()]]);
        }
    }
    #[test]
    fn selected_region_is_copied_as_png_bytes() {
        let mut runner = Fixture::new([
            (0, b"10,20 400x300\n".to_vec()),
            (0, b"png".to_vec()),
            (0, Vec::new()),
        ]);
        assert!(
            capture_with("region", &Paths::new("/tmp/nacre-test"), &mut runner)
                .unwrap()
                .unwrap()
                .contains("selection")
        );
        assert_eq!(
            runner.commands[1],
            vec!["grim", "-g", "10,20 400x300", "-"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            runner.commands[2],
            vec!["wl-copy", "--type", "image/png"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(runner.inputs[2], Some(b"png".to_vec()));
    }
    #[test]
    fn full_capture_creates_unique_private_files_before_grim_writes() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path());
        let expected = root.path().join("Pictures/Screenshots");
        let mut names = Vec::new();
        for _ in 0..2 {
            let mut runner = Fixture::new([(0, b"png".to_vec()), (0, Vec::new())]);
            capture_with("full", &paths, &mut runner).unwrap();
            names.push(runner.commands[0][1].clone());
            assert_eq!(
                runner.commands[1],
                vec!["wl-copy", "--type", "image/png"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            );
            assert_eq!(runner.inputs[1], Some(b"png".to_vec()));
        }
        assert_ne!(names[0], names[1]);
        assert!(names
            .iter()
            .all(|name| Path::new(name).parent() == Some(expected.as_path())));
    }
    #[test]
    fn full_capture_destination_is_private_before_image_write() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let path = create_private_output(folder.path()).unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
