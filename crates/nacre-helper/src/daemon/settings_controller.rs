//! Adapt the frozen desktop controller to the daemon's cached desktop document.
use crate::{
    desktop_settings::{Effect, Native, Runtime, Settings},
    paths::Paths,
    Result,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

struct Cached<'a> {
    native: Native<'a>,
    path: PathBuf,
    data: Value,
    exists: bool,
    written: bool,
}

impl Runtime for Cached<'_> {
    fn compositor(&mut self, operation: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        self.native.compositor(operation, args, kwargs)
    }
    fn effect(&mut self, effect: Effect<'_>) -> Result<()> {
        self.native.effect_with_desktop(effect, Some(&self.data))
    }
    fn read(&mut self, path: &Path, fallback: Value) -> Result<Value> {
        if path == self.path {
            Ok(self.data.clone())
        } else {
            self.native.read(path, fallback)
        }
    }
    fn exists(&self, path: &Path) -> bool {
        if path == self.path {
            self.exists
        } else {
            self.native.exists(path)
        }
    }
    fn write(&mut self, path: &Path, text: &str) -> Result<()> {
        if path == self.path {
            self.data = serde_json::from_str(text)?;
            self.exists = true;
            self.written = true;
            Ok(())
        } else {
            // In particular the display-pending journal uses the established
            // synchronous, fsynced versioned writer before rollback is armed.
            self.native.write(path, text)
        }
    }
}

pub async fn execute(
    control: super::compositor_control::Control,
    paths: Paths,
    data: Value,
    exists: bool,
    arguments: Vec<String>,
) -> Result<(Result<Value>, Option<Value>)> {
    control
        .with(move |backend| {
            let runtime = Cached {
                path: paths.config.join("desktop.json"),
                native: Native::new(paths.clone(), backend),
                data,
                exists,
                written: false,
            };
            let mut controller = Settings::new(paths, runtime)?;
            let result = controller.execute(&arguments[0], &arguments[1..]);
            let written = controller
                .runtime
                .written
                .then_some(controller.runtime.data);
            Ok((result, written))
        })
        .await
}

fn changed_expectations(
    before: &Value,
    after: &Value,
) -> Result<Vec<crate::state::FieldExpectation>> {
    fn visit(
        before: Option<&Value>,
        after: Option<&Value>,
        path: &mut Vec<String>,
        output: &mut Vec<crate::state::FieldExpectation>,
    ) -> Result<()> {
        if before == after {
            return Ok(());
        }
        if let (Some(old), Some(new)) = (
            before.and_then(Value::as_object),
            after.and_then(Value::as_object),
        ) {
            let keys = old
                .keys()
                .chain(new.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                path.push(key.clone());
                visit(old.get(&key), new.get(&key), path, output)?;
                path.pop();
            }
            return Ok(());
        }
        if path.is_empty() {
            return Err("Cannot safely determine desktop settings fields".into());
        }
        output.push(crate::state::FieldExpectation {
            path: path.clone(),
            value: after.cloned(),
        });
        Ok(())
    }
    let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
        return Err("Cannot safely determine desktop settings fields".into());
    };
    let keys = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut expected = Vec::new();
    for key in keys {
        visit(
            before.get(&key),
            after.get(&key),
            &mut vec![key],
            &mut expected,
        )?;
    }
    Ok(expected)
}

fn receipt_matches(result: &Value, accepted: &Value) -> bool {
    result.get("rev").and_then(Value::as_u64).is_some() && result.get("rev") == accepted.get("rev")
}

/// Native command compatibility. Once a call is dispatched, errors never fall
/// back to a second writer: an interrupted reply may already have applied it.
pub fn client(action: &str, arguments: &[String], recovery: bool) -> Result<Option<Value>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let mut connection = match super::client::connect().await {
            Ok(connection) => connection,
            Err(super::client::ConnectError::Absent) if !explicit || recovery => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !connection.methods.contains_key("settings.desktop") {
            return Err("Daemon desktop settings service is unavailable".into());
        }
        let store = connection
            .call(
                "settings.get",
                serde_json::json!({"store":"desktop"}),
                Some(5000),
            )
            .await?;
        let mut argv = vec![action.to_owned()];
        argv.extend_from_slice(arguments);
        let result = connection
            .call(
                "settings.desktop",
                serde_json::json!({"arguments":argv,"rev":store["rev"]}),
                Some(30000),
            )
            .await?;
        if action != "state" {
            let accepted = connection
                .call(
                    "settings.get",
                    serde_json::json!({"store":"desktop"}),
                    Some(5000),
                )
                .await?;
            if !receipt_matches(&result, &accepted) {
                return Err(
                    "Desktop settings changed before the operation could be confirmed".into(),
                );
            }
            let expected = if action == "set" {
                let key = arguments.first().ok_or("Missing desktop setting key")?;
                vec![crate::state::FieldExpectation::from_data(
                    &accepted["data"],
                    vec![key.clone()],
                )]
            } else {
                changed_expectations(&store["data"], &accepted["data"])?
            };
            crate::state::wait_store(&mut connection, "desktop", accepted, &expected).await?;
        }
        Ok(Some(result))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{classic_state, compositor::backend::Backend};
    use serde_json::json;

    #[test]
    fn settings_controller_receipt_race_is_conflict() {
        assert!(receipt_matches(
            &json!({"rev":7}),
            &json!({"rev":7,"data":{}})
        ));
        assert!(!receipt_matches(
            &json!({"rev":7}),
            &json!({"rev":8,"data":{}})
        ));
        assert!(!receipt_matches(&json!({}), &json!({"rev":8,"data":{}})));
    }

    #[test]
    fn controller_expectations_track_changed_paths_and_deleted_values() {
        let before = json!({"changed":false,"nested":{"left":1,"keep":2},"array":[1]});
        let accepted = json!({"changed":true,"nested":{"keep":2},"array":[1,2]});
        let expected = changed_expectations(&before, &accepted).unwrap();
        assert!(crate::state::expectations_match(&accepted, &expected));
        let unrelated =
            json!({"changed":true,"nested":{"keep":2,"other":9},"array":[1,2],"new":true});
        assert!(crate::state::expectations_match(&unrelated, &expected));
        let overwritten = json!({"changed":false,"nested":{"keep":2},"array":[1,2]});
        assert!(!crate::state::expectations_match(&overwritten, &expected));
    }
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        thread,
        time::Duration,
    };

    #[test]
    fn projection_uses_accepted_cache_before_the_quiet_disk_write() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        fs::create_dir_all(&paths.state)?;
        fs::create_dir_all(paths.tools())?;
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
        for name in [
            "reference-style.json",
            "compositor_backends/niri/palette.kdl.in",
        ] {
            fs::create_dir_all(
                paths
                    .tools()
                    .join(name)
                    .parent()
                    .ok_or("Missing asset directory")?,
            )?;
            fs::copy(assets.join(name), paths.tools().join(name))?;
        }
        fs::write(paths.tools().join("nacre-helper"), b"fixture asset marker")?;
        let scheme: Value =
            serde_json::from_slice(&fs::read(assets.join("reference-style.json"))?)?;
        fs::write(
            paths.state.join("scheme.json"),
            serde_json::to_vec(&scheme)?,
        )?;
        let desktop = paths.config.join("desktop.json");
        fs::write(&desktop, br#"{"interfaceGradients":true}"#)?;
        let socket = home.path().join("niri.sock");
        let listener = UnixListener::bind(&socket)?;
        let peer = thread::spawn(move || -> Result<()> {
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            assert!(line.contains("LoadConfigFile"), "{line}");
            writeln!(reader.get_mut(), "{}", json!({"Ok":"Handled"}))?;
            Ok(())
        });
        let mut backend = Backend::new(
            paths.clone(),
            [
                ("NACRE_COMPOSITOR".into(), "niri".into()),
                ("NIRI_SOCKET".into(), socket.to_string_lossy().into_owned()),
            ]
            .into(),
        )?;
        let palette = backend.config_path("palette")?;
        let mut cached = Cached {
            native: Native::new(paths.clone(), &mut backend),
            path: desktop.clone(),
            data: json!({"interfaceGradients":false}),
            exists: true,
            written: true,
        };
        cached.effect(Effect::Gradients)?;
        peer.join().unwrap()?;
        let mut colors = classic_state::validated_colors(&scheme, &assets)?;
        colors.insert("secondary".into(), colors["primary"].clone());
        let expected =
            classic_state::render_template(&assets, "niri", "compositor-palette", &colors)?;
        assert_eq!(fs::read_to_string(palette)?, expected);
        assert_eq!(fs::read(desktop)?, br#"{"interfaceGradients":true}"#);
        Ok(())
    }
}
