//! Narrow native module calls for remaining Python callers, retired in later waves.
use crate::{
    branding,
    classic_state::{self, Publisher},
    colors::Colors,
    json, lock_config, lock_dashboard,
    paths::Paths,
    wallpaper_media::Media,
    Result,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(default)]
    pub args: Vec<Value>,
    #[serde(default)]
    pub kwargs: Map<String, Value>,
}
impl Request {
    fn value(&self, index: usize, key: &str) -> Option<&Value> {
        self.args.get(index).or_else(|| self.kwargs.get(key))
    }
    fn argument(&self, index: usize, key: &str) -> Result<&Value> {
        self.value(index, key)
            .ok_or_else(|| format!("Missing {key} argument").into())
    }
    fn path(&self, index: usize, key: &str) -> Result<PathBuf> {
        let value = self.argument(index, key)?;
        value
            .as_str()
            .or_else(|| value["$path"].as_str())
            .map(PathBuf::from)
            .ok_or_else(|| "Expected path argument".into())
    }
    fn text(&self, index: usize, key: &str) -> Result<&str> {
        self.argument(index, key)?
            .as_str()
            .ok_or_else(|| format!("Expected {key} string").into())
    }
    fn optional_path(&self, index: usize, key: &str) -> Result<Option<PathBuf>> {
        if self.value(index, key).is_none_or(Value::is_null) {
            Ok(None)
        } else {
            self.path(index, key).map(Some)
        }
    }
    fn boolean(&self, index: usize, key: &str, default: bool) -> Result<bool> {
        self.value(index, key).map_or(Ok(default), |v| {
            v.as_bool()
                .ok_or_else(|| format!("Expected {key} boolean").into())
        })
    }
}
fn publisher(paths: Paths) -> Result<Publisher> {
    let backend =
        crate::compositor::backend::Backend::new(paths.clone(), std::env::vars().collect())?;
    Ok(Publisher {
        assets: paths.assets(),
        paths,
        backend,
        system_icons: "/usr/share/icons".into(),
        monitors: None,
        now_ms: None,
    })
}
fn home(request: &Request, index: usize, key: &str) -> Result<Paths> {
    if request.value(index, key).is_none_or(Value::is_null) {
        Paths::environment()
    } else {
        Paths::for_home(&request.path(index, key)?)
    }
}
pub fn call(module: &str, operation: &str, request: &Request) -> Result<Value> {
    let paths = Paths::environment()?;
    let assets = paths.assets();
    match (module, operation) {
        ("classic-state", "publish_compositor_palette") => {
            let publisher = publisher(home(request, 0, "home")?)?;
            Ok(json!(publisher.publish_compositor_palette(
                request
                    .argument(1, "colors")?
                    .as_object()
                    .ok_or("Expected compositor colors")?
            )?))
        }
        ("classic-state", "DEMO_PREFERENCES") => Ok(json!(classic_state::DEMO_PREFERENCES)),
        ("classic-state", "atomic_write") => {
            classic_state::atomic_write(&request.path(0, "path")?, request.text(1, "text")?)?;
            Ok(Value::Null)
        }
        ("classic-state", "atomic_symlink") => {
            classic_state::atomic_symlink(&request.path(0, "path")?, &request.path(1, "target")?)?;
            Ok(Value::Null)
        }
        ("classic-state", "validated_colors") => Ok(json!(classic_state::validated_colors(
            request.argument(0, "data")?,
            &assets
        )?)),
        ("classic-state", "luminance") => {
            Ok(json!(classic_state::luminance(request.text(0, "value")?)?))
        }
        ("classic-state", "readable") => Ok(json!(classic_state::readable(
            request.text(0, "value")?,
            request.text(1, "background")?,
            request
                .value(2, "minimum")
                .and_then(Value::as_f64)
                .unwrap_or(4.5)
        )?)),
        ("classic-state", "_reload_desktop_consumers") => {
            if request.boolean(3, "live", true)? {
                let mut publisher = publisher(paths)?;
                let connection = zbus::blocking::Connection::session()?;
                publisher.reload_desktop_consumers(
                    request.argument(0, "data")?,
                    request.text(1, "icon_theme")?,
                    request.text(2, "lua")?,
                    Some(&connection),
                )?;
            }
            Ok(Value::Null)
        }
        ("classic-state", "_publish_lock_login") => {
            let mut publisher = publisher(home(request, 0, "home")?)?;
            publisher.publish_lock_login(
                request
                    .argument(2, "colors")?
                    .as_object()
                    .ok_or("Expected lock colors")?,
                &request.path(3, "wallpaper")?.to_string_lossy(),
            )?;
            Ok(Value::Null)
        }
        ("classic-state", "lock_path") => {
            let paths = home(request, 0, "home")?;
            Ok(json!(crate::paths::weak_canonical(
                &paths.state.join("palette-commit.lock")
            )?))
        }
        ("classic-state", "apply_palette") => {
            let mut publisher = publisher(home(request, 0, "home")?)?;
            publisher.apply_palette(
                request.optional_path(1, "wallpaper")?.as_deref(),
                request.boolean(2, "live", true)?,
            )?;
            Ok(Value::Null)
        }
        ("classic-state", "commit_prepared") => {
            let mut publisher = publisher(home(request, 0, "home")?)?;
            Ok(json!(publisher.commit_prepared(
                &request.path(1, "wallpaper")?,
                request.argument(2, "data")?,
                &request.path(3, "thumbnail")?,
                request.boolean(4, "live", true)?,
                request.boolean(5, "allow_mode_change", false)?
            )?))
        }
        ("classic-state", "restore_demo") => {
            let mut publisher = publisher(home(request, 0, "home")?)?;
            publisher.restore_demo(
                request.argument(1, "snapshot")?,
                request.boolean(2, "live", true)?,
            )
        }
        ("palette_engine", "render_values") => {
            let mapping = |index, key| -> Result<Option<&Map<String, Value>>> {
                request
                    .value(index, key)
                    .filter(|v| !v.is_null())
                    .map(|v| {
                        v.as_object()
                            .ok_or_else(|| format!("Expected {key} mapping").into())
                    })
                    .transpose()
            };
            Ok(json!(orient_core::template_engine::render_values(
                request.text(0, "template")?,
                request
                    .argument(1, "values")?
                    .as_object()
                    .ok_or("Expected color values")?,
                mapping(2, "schemes")?,
                request.value(3, "image").and_then(Value::as_str),
                mapping(4, "custom")?,
                mapping(5, "data")?,
            )?))
        }
        ("lock-dashboard", "rgb") => Ok(json!(lock_dashboard::rgb(request.text(0, "value")?)?)),
        ("lock-dashboard", "layout") => Ok(json!(lock_dashboard::layout(
            request.argument(0, "monitor")?
        ))),
        ("branding", "refresh_terminal_menus") => {
            branding::refresh_terminal_menus(
                &home(request, 0, "home")?,
                &request.path(1, "proc")?,
            )?;
            Ok(Value::Null)
        }
        ("branding", "master") => Ok(json!(branding::master(&assets)?)),
        ("branding", "prepare") => {
            branding::prepare(
                request
                    .argument(0, "colors")?
                    .as_object()
                    .ok_or("Expected branding colors")?,
                &home(request, 1, "home")?,
                &assets,
            )?;
            Ok(Value::Null)
        }
        ("branding", "raster") => {
            use base64::Engine;
            let image = branding::raster(
                request.text(0, "text")?,
                request
                    .value(1, "size")
                    .and_then(Value::as_u64)
                    .unwrap_or(512) as u32,
            )?;
            Ok(json!(base64::engine::general_purpose::STANDARD
                .encode(orient_decoder::encode_rgba_png(&image, 6)?)))
        }
        ("branding", "cached_png") => {
            use base64::Engine;
            Ok(json!(base64::engine::general_purpose::STANDARD.encode(
                branding::cached_png(
                    request.text(0, "text")?,
                    &home(request, 1, "home")?,
                    request
                        .value(2, "size")
                        .and_then(Value::as_u64)
                        .unwrap_or(512) as u32
                )?
            )))
        }
        ("branding", "templates") => Ok(json!(branding::templates(&assets)?)),
        ("branding", "asset_names") => Ok(json!(branding::asset_names())),
        ("branding", "role_colors") => Ok(json!(branding::role_colors(
            request
                .argument(0, "colors")?
                .as_object()
                .ok_or("Expected branding colors")?
        )?)),
        ("branding", "paint_svg") => Ok(json!(branding::paint_svg(
            request.text(0, "text")?,
            request
                .argument(1, "roles")?
                .as_object()
                .ok_or("Expected branding roles")?
        )?)),
        ("branding", "svg") => {
            let patterns = branding::templates(&assets)?;
            let variant = request
                .value(2, "variant")
                .and_then(Value::as_str)
                .unwrap_or("full");
            let text = patterns[variant]
                .as_str()
                .ok_or("Unknown branding variant")?;
            if request.boolean(0, "template", true)? {
                Ok(json!(text))
            } else {
                let colors = request
                    .argument(1, "colors")?
                    .as_object()
                    .ok_or("Expected branding colors")?;
                Ok(json!(branding::paint_svg(
                    text,
                    &branding::role_colors(colors)?
                )?))
            }
        }
        ("branding", "luminance") => {
            Ok(json!(classic_state::luminance(request.text(0, "value")?)?))
        }
        ("branding", "publish") => {
            branding::publish(
                request
                    .argument(0, "colors")?
                    .as_object()
                    .ok_or("Expected branding colors")?,
                &home(request, 1, "home")?,
                &assets,
            )?;
            Ok(Value::Null)
        }
        ("branding", "text_logo") => {
            let paths = request
                .optional_path(1, "home")?
                .map(|p| Paths::for_home(&p))
                .transpose()?;
            Ok(json!(branding::text_logo(
                &assets,
                request
                    .value(0, "variant")
                    .and_then(Value::as_str)
                    .unwrap_or("symbolic"),
                paths.as_ref()
            )?))
        }
        ("file-icons", "family") => {
            Ok(json!(crate::file_icons::family(request.text(0, "color")?)?))
        }
        ("file-icons", "theme") => Ok(json!(crate::file_icons::theme(
            &home(request, 0, "home")?,
            Path::new("/usr/share/icons"),
            request.text(1, "primary")?,
            request.text(2, "mode")?
        )?)),
        ("file-icons", "overlay_index") => {
            let pairs = request
                .argument(0, "directories")?
                .as_array()
                .ok_or("Expected icon directories")?
                .iter()
                .map(|p| {
                    Ok((
                        p[0].as_str().ok_or("Invalid icon directory")?.into(),
                        p[1].as_u64().ok_or("Invalid icon size")? as u32,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!(crate::file_icons::overlay_index(
                &pairs,
                request.text(1, "fallback")?
            )))
        }
        ("file-icons", "overlay_link") => {
            crate::file_icons::overlay_link(
                &request.path(0, "path")?,
                &request.path(1, "source")?,
            )?;
            Ok(Value::Null)
        }
        ("lock-config", "block") => {
            let options = request
                .value(2, "options")
                .and_then(Value::as_object)
                .unwrap_or(&request.kwargs);
            let values = options
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect::<Vec<_>>();
            Ok(json!(lock_config::block(
                request.text(0, "widget")?,
                request.text(1, "monitor")?,
                &values
            )))
        }
        ("lock-config", "panel_key") => Ok(json!(lock_config::panel_key(
            request.argument(0, "monitor")?
        )?)),
        ("lock-config", "panel_scale") => Ok(json!(lock_config::panel_scale(
            request.argument(0, "monitor")?
        ))),
        ("lock-config", "render") => {
            let ready = request.optional_path(5, "ready")?;
            let artwork = request.optional_path(6, "artwork")?;
            let colors = request
                .argument(0, "colors")?
                .as_object()
                .ok_or("Expected lock colors")?;
            let monitors = request
                .value(4, "monitors")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(json!(lock_config::render(
                &assets,
                &paths,
                colors,
                request.text(1, "wallpaper")?,
                request.argument(2, "preferences")?,
                &request.path(3, "helper")?,
                &monitors,
                ready.as_deref(),
                artwork.as_deref()
            )?))
        }
        ("lock-config", "prepare_config") => {
            let paths = home(request, 4, "home")?;
            let mut backend = crate::compositor::backend::Backend::new(
                paths.clone(),
                std::env::vars().collect(),
            )?;
            let monitors = lock_config::monitor_layout(&paths, &mut backend)?;
            Ok(json!(lock_config::prepare_config(
                &paths,
                &assets,
                request
                    .argument(0, "colors")?
                    .as_object()
                    .ok_or("Expected lock colors")?,
                &request.path(1, "wallpaper")?,
                request.argument(2, "preferences")?,
                &request.path(3, "helper")?,
                &monitors
            )?))
        }
        ("lock-config", "publish") => {
            let paths = home(request, 0, "home")?;
            let mut backend = crate::compositor::backend::Backend::new(
                paths.clone(),
                std::env::vars().collect(),
            )?;
            lock_config::publish(&paths, &mut backend, None)?;
            Ok(Value::Null)
        }
        ("lock-dashboard", "publish") => lock_dashboard::publish(
            request.argument(0, "data")?,
            request
                .argument(1, "colors")?
                .as_object()
                .ok_or("Expected lock colors")?,
            &request.path(2, "wallpaper")?,
            request.optional_path(3, "artwork")?.as_deref(),
            &home(request, 4, "home")?,
            request
                .argument(5, "monitors")?
                .as_array()
                .ok_or("Expected lock monitors")?,
        ),
        ("lock-prepare", "atomic") => {
            crate::lock_prepare::atomic(
                &request.path(0, "path")?,
                request.text(1, "text")?.as_bytes(),
            )?;
            Ok(Value::Null)
        }
        ("lock-prepare", "prepare") => {
            let mut info = crate::lock_info::Info::new(home(request, 1, "home")?);
            crate::lock_prepare::prepare(
                request.argument(0, "data")?,
                &mut info,
                &assets,
                &mut crate::lock_dashboard::Context::default(),
            )?;
            Ok(Value::Null)
        }
        ("lock-info", operation) => {
            let mut info = crate::lock_info::Info::new(paths.clone());
            match operation {
                "plain" => info
                    .plain(
                        request.argument(0, "value")?,
                        request
                            .value(1, "width")
                            .and_then(Value::as_f64)
                            .unwrap_or(288.0),
                        request
                            .value(2, "lines")
                            .and_then(Value::as_u64)
                            .unwrap_or(2) as usize,
                        request
                            .value(3, "font")
                            .and_then(Value::as_u64)
                            .unwrap_or(14) as u32,
                    )
                    .map(|value| json!(value)),
                "label" => info
                    .label(
                        request.text(0, "kind")?,
                        request.argument(1, "data")?,
                        request.argument(2, "settings")?,
                    )
                    .map(|value| json!(value)),
                "artwork" => info
                    .artwork(request.argument(0, "data")?)
                    .map(|value| json!(value)),
                "snapshot" => info.snapshot(),
                "ipc" => info
                    .ipc(request.text(0, "target")?, request.text(1, "action")?)
                    .map(|value| json!(value)),
                _ => Err("Unsupported native lock information call".into()),
            }
        }
        ("wallpaper-media", operation) => {
            let media = Media::new(paths.clone(), assets.clone())?;
            match operation {
                "session" => Media::session(),
                "wallpaper_mode" => Ok(json!(Media::wallpaper_mode(&request.path(0, "poster")?)?)),
                "select" | "theme" | "prepared_theme" | "warm" | "demo_start" | "demo_restore"
                | "demo_scenes" | "pick" => {
                    let mut cli = crate::palette_cli::Cli::environment(true)?;
                    match operation {
                        "select" => media.select(&mut cli, &request.path(0, "path")?),
                        "theme" => media.theme(&mut cli, request.argument(0, "value")?),
                        "prepared_theme" => Ok(json!(media.prepared_theme(
                            &mut cli.publisher,
                            &request.path(0, "poster")?,
                            request.boolean(1, "live", true)?
                        )?)),
                        "warm" => media.warm(&mut cli, request.boolean(0, "include_demo", true)?),
                        "demo_start" => media.demo_start(),
                        "demo_restore" => media.demo_restore(
                            &mut cli,
                            request.value(0, "token").and_then(Value::as_str),
                        ),
                        "demo_scenes" => media.demo_scenes(),
                        _ => media.pick(),
                    }
                }

                "import_files" => media.import_files(request.argument(0, "paths")?),
                "settings" => media.settings(),
                "media_state" => media.media_state(),
                "palette_presets" => media.palette_presets(),
                "preference" => media.preference(request.argument(0, "value")?),
                "previews" => media.previews(&request.path(0, "poster")?),
                "describe" => media.describe(&request.path(0, "path")?),
                "catalog" => media.catalog(),
                "palette_marker" => Ok(json!(media.palette_marker(
                    &request.path(0, "poster")?,
                    request.text(1, "flavour")?,
                    request
                        .value(2, "harmony")
                        .filter(|v| !v.is_null())
                        .map(|v| v.as_bool().ok_or("Expected harmony boolean"))
                        .transpose()?,
                    request.optional_path(3, "source")?.as_deref(),
                    request.value(4, "mode").and_then(Value::as_str),
                    request.value(5, "personality").and_then(Value::as_str)
                )?)),
                _ => Err("Unsupported native media module call".into()),
            }
        }
        ("colors", operation) => {
            let mut app = Colors::environment()?;
            match operation {
                "validate_request" => app.validate_request(request.argument(0, "request")?),
                "stage" => app.stage(request.argument(0, "request")?),
                "record" => app.record(request.argument(0, "identity")?),
                "stored" => app.stored(),
                "catalog" => app.catalog(),
                "favorite" => app.favorite(
                    request.argument(0, "identity")?,
                    request.value(1, "name").unwrap_or(&json!("")),
                ),
                "apply" => app.apply(
                    request.argument(0, "identity")?,
                    request.boolean(1, "live", true)?,
                ),
                "export" => app.export(request.argument(0, "identity")?),
                "card" => app.card(request.argument(0, "identity")?),
                _ => Err("Unsupported native Colors module call".into()),
            }
        }
        _ => Err("Unsupported native palette module operation".into()),
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    use std::io::Read;
    if args.len() != 2 && args.len() != 3 {
        return Err("Expected palette module, operation and JSON request".into());
    }
    let mut input = String::new();
    if args.len() == 3 {
        input = args[2].clone();
    } else {
        std::io::stdin()
            .take(16 * 1024 * 1024 + 1)
            .read_to_string(&mut input)?;
        if input.len() > 16 * 1024 * 1024 {
            return Err("Palette module request is too large".into());
        }
    }
    let result = serde_json::from_str::<Request>(&input)
        .map_err(Into::into)
        .and_then(|request| call(&args[0], &args[1], &request));
    let (value, code) = match result {
        Ok(value) => (json!({"result":value}), 0),
        Err(error) if error.is::<crate::wallpaper_media::ToolFailure>() => {
            let failure = error
                .downcast_ref::<crate::wallpaper_media::ToolFailure>()
                .unwrap();
            (
                json!({"error":failure.to_string(), "type":"CalledProcessError", "command":failure.command, "code":failure.code, "stderr":failure.stderr}),
                1,
            )
        }
        Err(error) => {
            let kind = if let Some(error) = error.downcast_ref::<crate::colors::ColorError>() {
                error.kind
            } else if error.is::<std::io::Error>() {
                "OSError"
            } else {
                "ValueError"
            };
            (json!({"error":error.to_string(),"type":kind}), 1)
        }
    };
    writeln!(std::io::stdout(), "{}", json::dumps(&value, false)?)?;
    Ok(code)
}
