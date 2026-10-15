//! Developer-only module-operation boundary. Not installed with nacre-helper.
use nacre_helper::{
    dashboard_data, dashboard_media, json, launcher_preferences, menu, notification_history,
    paths::Paths, Result,
};
use serde::Deserialize;
use serde_json::{json as value, Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default)]
    args: Vec<Value>,
    #[serde(default)]
    kwargs: Map<String, Value>,
}
impl Request {
    fn argument(&self, index: usize, key: &str) -> Result<&Value> {
        self.args
            .get(index)
            .or_else(|| self.kwargs.get(key))
            .ok_or_else(|| format!("Missing argument: {key}").into())
    }
    fn text(&self, index: usize, key: &str) -> Result<&str> {
        self.argument(index, key)?
            .as_str()
            .ok_or_else(|| format!("Expected text: {key}").into())
    }
    fn path(&self, index: usize, key: &str, fallback: PathBuf) -> Result<PathBuf> {
        match self.args.get(index).or_else(|| self.kwargs.get(key)) {
            None => Ok(fallback),
            Some(v) => v
                .get("$path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| "Expected a tagged Path".into()),
        }
    }
}
fn call(helper: &str, operation: &str, request: &Request) -> Result<Value> {
    if helper == "lock-config" && operation == "output_width" {
        // The historical module never exported this name: preserve missing-attribute failure.
        eprintln!("AttributeError: module has no attribute 'output_width'");
        std::process::exit(1);
    }
    let paths = Paths::environment()?;
    if helper == "shell-supervisor" {
        let root = paths.share.join("shell");
        return match operation {
            "restore" => Ok(value!(nacre_helper::shell_supervisor::restore(
                &root,
                &chrono::Local::now().format("%Y%m%dT%H%M%S").to_string()
            )?)),
            "health" => {
                let environment = request
                    .args
                    .first()
                    .or_else(|| request.kwargs.get("env"))
                    .cloned()
                    .unwrap_or(value!({}));
                Ok(value!(nacre_helper::shell_supervisor::health(
                    &root,
                    &serde_json::from_value(environment)?
                )?))
            }
            _ => Err("Unknown supervisor operation".into()),
        };
    }
    if helper == "nacre_shell" && operation == "from_image" {
        let mut options = request.kwargs.clone();
        if let Some(path) = options
            .get("cache_dir")
            .and_then(|v| v.get("$path"))
            .cloned()
        {
            options.insert("cache_dir".into(), path);
        }
        return nacre_helper::orient::from_image(
            Path::new(request.text(0, "path")?),
            &Value::Object(options),
        );
    }

    if matches!(
        helper,
        "classic-state"
            | "lock-info"
            | "lock-prepare"
            | "branding"
            | "file-icons"
            | "lock-config"
            | "lock-dashboard"
            | "palette_engine"
            | "colors"
            | "wallpaper-media"
    ) {
        return nacre_helper::palette_api::call(
            helper,
            operation,
            &nacre_helper::palette_api::Request {
                args: request.args.clone(),
                kwargs: request.kwargs.clone(),
            },
        );
    }
    match (helper, operation) {
        ("wallpaper-watch", "flavour") => {
            match nacre_helper::wallpaper_watch::flavour(&request.path(
                0,
                "path",
                PathBuf::new(),
            )?) {
                Ok(values) => Ok(value!(values)),
                Err(error) if error.is::<nacre_helper::wallpaper_watch::FlavourType>() => {
                    eprintln!("AttributeError: {error}");
                    std::process::exit(1);
                }
                Err(error) => Err(error),
            }
        }

        ("demo-wallpapers", "collection") => {
            nacre_helper::demo_wallpapers::collection(&paths.assets().join("demo-wallpapers"))
        }
        ("login-appearance", "prepare") => {
            let wallpaper = match request.argument(0, "wallpaper")? {
                Value::String(path) => PathBuf::from(path),
                value => PathBuf::from(
                    value
                        .get("$path")
                        .and_then(Value::as_str)
                        .ok_or("Expected wallpaper path")?,
                ),
            };
            let cache = request
                .args
                .get(1)
                .or_else(|| request.kwargs.get("cache"))
                .and_then(|v| v.get("$path"))
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let (path, stamp) =
                nacre_helper::login_appearance::prepare(&paths, &wallpaper, cache.as_deref())?;
            Ok(value!([path, stamp]))
        }

        ("kde-palette", "read") => {
            nacre_helper::kde_palette::read(&request.path(0, "path", PathBuf::new())?)
        }
        ("kde-palette", "save") => {
            nacre_helper::kde_palette::save(
                &request.path(0, "path", PathBuf::new())?,
                request.text(1, "text")?,
            )?;
            Ok(Value::Null)
        }

        ("compositor-backends-niri-widths", "check_session") => Ok(value!(
            nacre_helper::niri_widths::runtime::check_session(&std::env::vars().collect())?
        )),
        ("diagnostics", "hardware") => {
            nacre_helper::diagnostics::hardware(&request.path(0, "root", PathBuf::from("/sys"))?)
        }
        ("diagnostics", "redact") => nacre_helper::diagnostics::redact(
            request.argument(0, "value")?,
            &request
                .args
                .get(1)
                .or_else(|| request.kwargs.get("identities"))
                .and_then(Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        ),
        ("dashboard-data", "validate") => dashboard_data::validate(request.argument(0, "data")?),
        ("dashboard-data", "focus_action") => {
            let now = request
                .kwargs
                .get("now")
                .filter(|v| !v.is_null())
                .map(|v| v.as_f64().ok_or("Expected numeric now"))
                .transpose()?
                .unwrap_or(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64());
            dashboard_data::focus_action(
                request.text(0, "kind")?,
                request
                    .args
                    .get(1)
                    .or_else(|| request.kwargs.get("minutes"))
                    .unwrap_or(&value!(25)),
                request
                    .args
                    .get(2)
                    .or_else(|| request.kwargs.get("dnd"))
                    .unwrap_or(&value!(false)),
                now,
            )
        }
        ("dashboard-data", "calendar") => {
            let now = match request.args.get(1).or_else(|| request.kwargs.get("now")) {
                None | Some(Value::Null) => chrono::Utc::now(),
                Some(v) => chrono::DateTime::parse_from_rfc3339(
                    v.get("$datetime")
                        .and_then(Value::as_str)
                        .ok_or("Expected tagged datetime")?,
                )?
                .with_timezone(&chrono::Utc),
            };
            Ok(dashboard_data::calendar(request.text(0, "path")?, now))
        }
        ("dashboard-media", "lyrics") => Ok(dashboard_media::lyrics(request.argument(0, "text")?)),
        ("dashboard-media", "track_data") => {
            dashboard_media::track_data(request.argument(0, "meta")?)
        }
        ("launcher-preferences", "load") => {
            launcher_preferences::load(&paths.config.join("launcher.json"))
        }
        ("launcher-preferences", "change") => launcher_preferences::change(
            &paths.config.join("launcher.json"),
            request.text(0, "action")?,
            request.argument(1, "value")?,
        ),
        ("notification-history", "load" | "save") => {
            let policy: Value = serde_json::from_slice(&fs::read(
                paths.assets().join("notification-policy.json"),
            )?)?;
            let index = usize::from(operation == "save");
            let path = request.path(
                index,
                "path",
                paths.state.join("shell/notifications/history.json"),
            )?;
            if operation == "save" {
                notification_history::save(&path, request.argument(0, "rows")?, &policy)?;
                Ok(Value::Null)
            } else {
                notification_history::load(&path, &policy)
            }
        }
        ("menu-backend", "package_name") => {
            Ok(value!(menu::package_name(request.text(0, "value")?)?))
        }
        ("menu-backend", "search_files") => menu::search_files(request.text(0, "query")?),
        ("menu-backend", "convert_units") => menu::convert_units(request.text(0, "expression")?),
        ("menu-backend", "installed") => menu::installed(),
        ("menu-backend", "fonts") => menu::fonts(),
        ("menu-backend", "webapps") => menu::webapps(),
        ("menu-backend", "reminders") => menu::reminders(),
        _ => Err("Unsupported native module operation".into()),
    }
}
fn main() {
    let result = (|| -> Result<Value> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        if arguments.len() != 3 {
            return Err("Expected helper, operation and JSON request".into());
        }
        let helper = Path::new(&arguments[0])
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("Invalid helper name")?;
        let helper = helper.strip_suffix(".py").unwrap_or(helper);
        call(helper, &arguments[1], &serde_json::from_str(&arguments[2])?)
    })();
    let (reply, code) = match result {
        Ok(v) => (v, 0),
        Err(error) if error.is::<nacre_helper::wallpaper_media::ToolFailure>() => {
            // The original developer API did not catch CalledProcessError.
            eprintln!("{error}");
            std::process::exit(1);
        }
        Err(error) => {
            let kind = if let Some(error) =
                error.downcast_ref::<nacre_helper::login_appearance::LoginError>()
            {
                error.kind
            } else if error.downcast_ref::<std::io::Error>().is_some() {
                "OSError"
            } else if error
                .downcast_ref::<dashboard_data::DashboardTypeError>()
                .is_some()
                || error.is::<nacre_helper::menu_registry::ReminderTypeError>()
            {
                "TypeError"
            } else if error.downcast_ref::<dashboard_media::Attribute>().is_some()
                || error.is::<dashboard_data::DashboardAttribute>()
            {
                "AttributeError"
            } else {
                "ValueError"
            };
            (value!({"error":error.to_string(),"type":kind}), 1)
        }
    };
    match json::encode(&reply) {
        Ok(encoded) => println!("{encoded}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    std::process::exit(code);
}
