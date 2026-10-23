//! Public Colors launcher; opens the app through the provider-neutral `nacrectl ui colorsApp open` route (N0048).
use crate::{paths, process, Result};
use serde_json::json;
use std::{path::Path, time::Duration};
pub fn run(args: &[String]) -> Result<i32> {
    if args == ["-h"] || args == ["--help"] {
        print!("usage: nacre-colors [-h] [image]\n\nOpen Nacre Colors; opening never applies a palette.\n\npositional arguments:\n  image\n\noptions:\n  -h, --help  show this help message and exit\n");
        return Ok(0);
    }
    if args.len() > 1 {
        eprintln!(
            "usage: nacre-colors [-h] [image]\nnacre-colors: error: unrecognized arguments: {}",
            args[1..].join(" ")
        );
        return Ok(2);
    }
    let result = (|| -> Result<()> {
        let path = if let Some(path) = args.first().filter(|s| !s.is_empty()) {
            let path = paths::expand(Path::new(path))?;
            match path.canonicalize() {
                Ok(path) => path.to_string_lossy().into_owned(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(format!(
                        "[Errno 2] No such file or directory: {}",
                        crate::json::python_repr(&json!(path.to_string_lossy()))
                    )
                    .into())
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            String::new()
        };
        let command = ["ui", "colorsApp", "open"]
            .map(str::to_owned)
            .into_iter()
            .chain([path])
            .collect::<Vec<_>>();
        let call = |timeout| -> Result<bool> {
            match process::run("nacrectl", &command, None, timeout) {
                Ok(output) => Ok(output.status.success()),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    Err("[Errno 2] No such file or directory: 'nacrectl'".into())
                }
                Err(error) => Err(error),
            }
        };
        if call(Duration::from_secs(3))? {
            return Ok(());
        }
        let connection = zbus::blocking::Connection::session()?;
        let manager = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
        )?;
        let _: zbus::zvariant::OwnedObjectPath =
            manager.call("StartUnit", &("nacre-shell.service", "replace"))?;
        // Quickshell target registration has no external readiness event. Retain
        // the existing bounded startup retry while the service loads its QML.
        for _ in 0..32 {
            std::thread::sleep(Duration::from_millis(250));
            if call(Duration::from_secs(2))? {
                return Ok(());
            }
        }
        Err("Colors could not connect to the desktop service".into())
    })();
    match result {
        Ok(()) => Ok(0),
        Err(error) => {
            eprintln!("nacre-colors: {error}");
            Ok(1)
        }
    }
}
