//! Compatibility command paths for nacre_shell; generation/publication use native libraries.
use crate::{
    classic_state::{self, Publisher},
    json, orient,
    paths::Paths,
    state, Result,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};
#[derive(Default, Clone)]
pub struct Arguments {
    pub command: String,
    pub action: String,
    pub flags: BTreeSet<String>,
    pub values: BTreeMap<String, String>,
    pub print: Option<String>,
    pub random: Option<String>,
    pub harmony: Option<bool>,
    pub background: Option<bool>,
}
impl Arguments {
    fn flag(&self, key: &str) -> bool {
        self.flags.contains(key)
    }
    fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}
#[derive(Debug)]
pub struct Usage {
    pub message: String,
    pub context: String,
}
impl fmt::Display for Usage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for Usage {}
fn usage_error<T>(context: &str, message: String) -> Result<T> {
    Err(Usage {
        message,
        context: context.into(),
    }
    .into())
}
pub fn usage(program: &str, context: &str) -> String {
    let parts: Vec<String> = match context {
        "scheme set" => vec![
            "[-h]".into(),
            "[--notify]".into(),
            "[-r]".into(),
            "[-n {default,dynamic}]".into(),
            "[-f {default,hard}]".into(),
            "[-m {dark,light}]".into(),
            format!("[-v {{{}}}]", orient_core::palette::VARIANTS.join(",")),
            "[--accent ACCENT]".into(),
            "[--auto-accent]".into(),
        ],
        "scheme get" => ["[-h]", "[-n]", "[-f]", "[-m]", "[-v]", "[--json]"]
            .map(str::to_owned)
            .to_vec(),
        "scheme list" => ["[-h]", "[-n]", "[-f]", "[-m]", "[-v]"]
            .map(str::to_owned)
            .to_vec(),
        "scheme" => vec!["[-h]".into(), "{list,get,set}".into(), "...".into()],
        "wallpaper" => vec![
            "[-h]".into(),
            "[-p [PRINT]".into(),
            "|".into(),
            "-r [RANDOM]".into(),
            "|".into(),
            "-f FILE]".into(),
            "[-n]".into(),
            "[-t THRESHOLD]".into(),
            "[-N]".into(),
            "[--accent ACCENT]".into(),
            "[--source SOURCE]".into(),
            "[--mode {dark,light}]".into(),
            "[--harmony | --no-harmony]".into(),
            format!(
                "[--personality {{{},source}}]",
                orient_core::palette::PERSONALITIES.join(",")
            ),
            "[--background-from-wallpaper | --no-background-from-wallpaper]".into(),
        ],
        _ => vec![
            "[-h]".into(),
            "[-v]".into(),
            "{scheme,wallpaper}".into(),
            "...".into(),
        ],
    };
    let name = if context.is_empty() {
        program.into()
    } else {
        format!("{program} {context}")
    };
    let prefix = format!("usage: {name} ");
    let indent = " ".repeat(prefix.len());
    let mut line = prefix.clone();
    let mut output = String::new();
    for part in parts {
        let occupied = line.len() > prefix.len();
        if occupied && line.len() + 1 + part.len() > 78 {
            output.push_str(&line);
            output.push('\n');
            line = indent.clone();
        }
        if line.len() > indent.len() {
            line.push(' ');
        }
        line.push_str(&part);
    }
    output.push_str(&line);
    output.push('\n');
    output
}
pub fn root_help(program: &str) -> String {
    format!("{}\nOrient: Nacre wallpaper and palette engine\n\npositional arguments:\n  {{scheme,wallpaper}}\n    scheme            manage the colour scheme\n    wallpaper         manage the wallpaper\n\noptions:\n  -h, --help          show this help message and exit\n  -v, --version       show program's version number and exit\n",usage(program,""))
}
#[derive(Debug)]
struct Help(String);
impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Help {}
fn help(program: &str, context: &str) -> String {
    let body = match context {
        "scheme" => include_str!("../resources/palette-cli-help/scheme.txt"),
        "scheme list" => include_str!("../resources/palette-cli-help/scheme-list.txt"),
        "scheme get" => include_str!("../resources/palette-cli-help/scheme-get.txt"),
        "scheme set" => include_str!("../resources/palette-cli-help/scheme-set.txt"),
        "wallpaper" => include_str!("../resources/palette-cli-help/wallpaper.txt"),
        _ => return root_help(program),
    };
    format!("{}\n{body}", usage(program, context))
}
pub fn parse(args: &[String]) -> Result<Arguments> {
    // argparse accepts adjacent short flags and attached values (for example
    // scheme get -nm, or scheme set -mdark). Normalize only its declared shorts.
    let mut normalized = Vec::new();
    for token in args {
        let mut tail = token.strip_prefix('-').unwrap_or("");
        if token.starts_with("--") || !tail.is_ascii() || tail.len() < 2 {
            normalized.push(token.clone());
            continue;
        }
        let readonly = args.first().is_some_and(|v| v == "scheme")
            && args.get(1).is_some_and(|v| v == "get" || v == "list");
        let wallpaper = args.first().is_some_and(|v| v == "wallpaper");
        while !tail.is_empty() {
            let (short, rest) = tail.split_at(1);
            let boolean = short == "h"
                || (readonly && "nfmv".contains(short))
                || (wallpaper && "nN".contains(short))
                || (!readonly && !wallpaper && short == "r");
            normalized.push(format!("-{short}"));
            if !boolean {
                if !rest.is_empty() {
                    normalized.push(rest.into());
                }
                break;
            }
            tail = rest;
        }
    }
    let args = normalized.as_slice();
    let Some(command) = args.first() else {
        return usage_error("", "the following arguments are required: command".into());
    };
    if !["scheme", "wallpaper"].contains(&command.as_str()) {
        return usage_error(
            "",
            format!(
                "argument command: invalid choice: '{}' (choose from 'scheme', 'wallpaper')",
                command
            ),
        );
    }
    let mut result = Arguments {
        command: command.clone(),
        ..Default::default()
    };
    let mut index = 1;
    if command == "scheme" {
        let Some(action) = args.get(index) else {
            return usage_error(
                "scheme",
                "the following arguments are required: action".into(),
            );
        };
        if ["-h", "--help"].contains(&action.as_str()) {
            return Err(Help("scheme".into()).into());
        }
        if !["list", "get", "set"].contains(&action.as_str()) {
            return usage_error(
                "scheme",
                format!(
                    "argument action: invalid choice: '{}' (choose from 'list', 'get', 'set')",
                    action
                ),
            );
        }
        result.action = action.clone();
        index += 1;
    }
    let context = if command == "scheme" {
        format!("scheme {}", result.action)
    } else {
        "wallpaper".into()
    };
    let mut selection = None;
    while index < args.len() {
        let token = &args[index];
        if ["-h", "--help"].contains(&token.as_str()) {
            return Err(Help(context.clone()).into());
        }
        let (option, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(a, b)| (a, Some(b.to_owned())));
        index += 1;
        let readonly = command == "scheme" && result.action != "set";
        let name = match option {
            "-n" => {
                if readonly && result.action == "list" {
                    "names"
                } else if readonly {
                    "name"
                } else if command == "wallpaper" {
                    "no-filter"
                } else {
                    "name"
                }
            }
            "-f" => {
                if readonly && result.action == "list" {
                    "flavours"
                } else if readonly {
                    "flavour"
                } else if command == "wallpaper" {
                    "file"
                } else {
                    "flavour"
                }
            }
            "-m" => {
                if readonly && result.action == "list" {
                    "modes"
                } else {
                    "mode"
                }
            }
            "-v" => {
                if readonly && result.action == "list" {
                    "variants"
                } else {
                    "variant"
                }
            }
            "-r" => "random",
            "-p" => "print",
            "-t" => "threshold",
            "-N" => "no-smart",
            value => value.strip_prefix("--").unwrap_or(value),
        };
        if [
            "harmony",
            "no-harmony",
            "background-from-wallpaper",
            "no-background-from-wallpaper",
        ]
        .contains(&name)
            && command == "wallpaper"
        {
            if name.contains("harmony") {
                result.harmony = Some(name == "harmony");
            } else {
                result.background = Some(name == "background-from-wallpaper");
            }
            continue;
        }
        let boolean = (readonly
            && if result.action == "list" {
                ["names", "flavours", "modes", "variants"].contains(&name)
            } else {
                ["name", "flavour", "mode", "variant", "json"].contains(&name)
            })
            || (command == "scheme"
                && result.action == "set"
                && ["notify", "random", "auto-accent"].contains(&name))
            || (command == "wallpaper" && ["no-filter", "no-smart"].contains(&name));
        if boolean {
            result.flags.insert(name.into());
            continue;
        }
        let optional = command == "wallpaper" && ["print", "random"].contains(&name);
        let valued = (command == "scheme"
            && result.action == "set"
            && ["name", "flavour", "mode", "variant", "accent"].contains(&name))
            || (command == "wallpaper"
                && [
                    "print",
                    "random",
                    "file",
                    "threshold",
                    "accent",
                    "source",
                    "mode",
                    "personality",
                ]
                .contains(&name));
        if !valued {
            return usage_error(&context, format!("unrecognized arguments: {token}"));
        }
        let value = if let Some(value) = inline {
            value
        } else if optional && args.get(index).is_none_or(|s| s.starts_with('-')) {
            String::new()
        } else {
            let Some(value) = args.get(index) else {
                return usage_error(
                    &context,
                    format!("argument {option}: expected one argument"),
                );
            };
            index += 1;
            value.clone()
        };
        let choices: Option<Vec<&str>> = match name {
            "name" => Some(vec!["default", "dynamic"]),
            "flavour" => Some(vec!["default", "hard"]),
            "mode" => Some(vec!["dark", "light"]),
            "variant" => Some(orient_core::palette::VARIANTS.to_vec()),
            "personality" => Some(
                orient_core::palette::PERSONALITIES
                    .into_iter()
                    .chain(["source"])
                    .collect(),
            ),
            _ => None,
        };
        if let Some(choices) = choices {
            if !choices.contains(&value.as_str()) {
                let argument = match name {
                    "mode" => {
                        if command == "scheme" {
                            "-m/--mode"
                        } else {
                            "--mode"
                        }
                    }
                    "name" => "-n/--name",
                    "flavour" => "-f/--flavour",
                    "variant" => "-v/--variant",
                    _ => "--personality",
                };
                return usage_error(
                    &context,
                    format!(
                        "argument {argument}: invalid choice: '{}' (choose from {})",
                        value,
                        choices
                            .iter()
                            .map(|s| format!("'{s}'"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
        let value = if name == "accent" {
            orient_core::colour::clean(&value).map_err(|_| Usage {
                context: context.clone(),
                message: format!("argument --accent: invalid clean value: '{value}'"),
            })?
        } else {
            value
        };
        if optional || name == "file" {
            if let Some(previous) = selection {
                return usage_error(
                    &context,
                    format!("argument {option}: not allowed with argument {previous}"),
                );
            }
            selection = Some(option);
        }
        if name == "print" {
            result.print = Some(value);
        } else if name == "random" {
            result.random = Some(value);
        } else {
            result.values.insert(name.into(), value);
        }
    }
    Ok(result)
}

pub struct Cli {
    pub paths: Paths,
    pub publisher: Publisher,
    pub publish: bool,
}
impl Cli {
    pub fn environment(publish: bool) -> Result<Self> {
        let publisher = Publisher::environment()?;
        Ok(Self {
            paths: publisher.paths.clone(),
            publisher,
            publish,
        })
    }
    pub fn current(&self) -> Result<Value> {
        let saved = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let mut data =
            json!({"name":"dynamic","mode":"dark","variant":"tonalspot","flavour":"default"});
        for (key, value) in saved.as_object().ok_or("Invalid current scheme")? {
            data[key] = value.clone();
        }
        Ok(data)
    }
    pub fn current_image(&self) -> Result<Option<String>> {
        for name in ["wallpaper/path.txt", "wallpaper/last.txt"] {
            let path = self.paths.state.join(name);
            if path.exists() {
                return Ok(Some(fs::read_to_string(path)?.trim().to_owned()));
            }
        }
        Ok(None)
    }
    fn personality(&self, args: &Arguments) -> Result<Value> {
        if let Some(value) = args.value("personality") {
            if !value.is_empty() {
                return Ok(json!(value));
            }
        }
        if let Some(harmony) = args.harmony {
            return Ok(json!(if harmony { "harmony" } else { "natural" }));
        }
        let preferences = state::read(&self.paths.config.join("wallpaper-picker.json"), json!({}))?;
        Ok(preferences
            .get("palettePersonality")
            .filter(|v| json::truthy(v))
            .cloned()
            .unwrap_or_else(|| {
                json!(if json::truthy(&preferences["paletteHarmony"]) {
                    "harmony"
                } else {
                    "natural"
                })
            }))
    }
    fn image_settings(&self, path: &Path, args: &Arguments) -> Result<Value> {
        use chrono::Timelike;
        let path = crate::paths::weak_canonical(path)?;
        let saved = state::read(&self.paths.config.join("colors.json"), json!({}))?;
        let choice = saved["wallpapers"]
            .get(path.to_string_lossy().as_ref())
            .cloned()
            .unwrap_or(json!({}));
        let mut overrides = choice.get("overrides").cloned().unwrap_or(json!({}));
        if args.value("accent").is_some() || args.flag("auto-accent") {
            overrides
                .as_object_mut()
                .ok_or("Invalid saved accent overrides")?
                .shift_remove("primary");
        }
        let preferences = state::read(&self.paths.config.join("wallpaper-picker.json"), json!({}))?;
        let background = args.background.map(|v| json!(v)).unwrap_or_else(|| {
            choice
                .get("backgroundFromWallpaper")
                .cloned()
                .unwrap_or_else(|| {
                    if choice["personality"] == "source" {
                        json!(true)
                    } else {
                        preferences["paletteBackgroundFromWallpaper"].clone()
                    }
                })
        });
        let personality = self.personality(args)?;
        let (personality, background) = orient_core::palette::background_policy(
            personality.as_str().ok_or("Expected personality string")?,
            &background,
        )?;
        Ok(
            json!({"personality":personality,"background_from_wallpaper":background,"overrides":overrides,"brightness":choice.get("brightness").cloned().unwrap_or(json!(0.0)),"time_tint":choice.get("timeTint").cloned().unwrap_or(json!(false)),"saturation":choice.get("saturation").cloned().unwrap_or(json!(1.0)),"hour":if json::truthy(&choice["tideAutomatic"]){json!(chrono::Local::now().hour())}else{choice.get("hour").cloned().unwrap_or(json!(12))}}),
        )
    }
    fn analysis_path(&self, path: &Path, args: &Arguments) -> Result<PathBuf> {
        if let Some(source) = args.value("source").filter(|s| !s.is_empty()) {
            return Ok(source.into());
        }
        let media = state::read(&self.paths.state.join("wallpaper/media.json"), json!({}))?;
        if media["poster"].as_str() == crate::paths::weak_canonical(path)?.to_str() {
            if let Some(source) = media["path"]
                .as_str()
                .map(Path::new)
                .filter(|path| path.is_file())
            {
                return Ok(source.to_owned());
            }
        }
        Ok(path.to_owned())
    }
    fn options(&self, path: &Path, args: &Arguments, data: &Value) -> Result<Value> {
        let config = state::read(&self.paths.config.join("cli.json"), json!({}))?;
        let key = crate::paths::weak_canonical(path)?;
        let key = key.to_string_lossy();
        let saved = state::read(&self.paths.config.join("colors.json"), json!({}))?;
        let choice = saved["wallpapers"]
            .get(key.as_ref())
            .cloned()
            .unwrap_or(json!({}));
        let accent = args
            .value("accent")
            .map(|v| json!(v))
            .or_else(|| choice.get("accent").filter(|v| json::truthy(v)).cloned())
            .or_else(|| config["orient"]["accents"].get(key.as_ref()).cloned())
            .unwrap_or(Value::Null);
        let mut options = json!({"mode":args.value("mode").map(|v|json!(v)).unwrap_or_else(||data["mode"].clone()),"variant":data["variant"],"flavour":data["flavour"],"accent":accent,"smart":false});
        for (key, value) in self
            .image_settings(path, args)?
            .as_object()
            .ok_or("Invalid image options")?
        {
            options[key] = value.clone();
        }
        orient::from_image(&self.analysis_path(path, args)?, &options)
    }
    fn write_json(&self, path: &Path, value: &Value) -> Result<()> {
        classic_state::atomic_write(path, &(json::dumps(value, true)? + "\n"))
    }
    fn save_scheme(&self, data: &Value) -> Result<()> {
        let colors = data["colours"].as_object().ok_or("Invalid scheme colors")?;
        orient_core::palette::validate(colors)?;
        self.write_json(&self.paths.state.join("scheme.json"), data)?;
        classic_state::atomic_write(
            &self.paths.state.join("scheme/current-mode.txt"),
            data["mode"].as_str().ok_or("Invalid scheme mode")?,
        )?;
        classic_state::atomic_write(
            &self.paths.state.join("scheme/current.txt"),
            &colors
                .iter()
                .map(|(key, value)| format!("{key} {}\n", value.as_str().unwrap_or("")))
                .collect::<String>(),
        )
    }
    pub fn scheme(&mut self, args: &Arguments) -> Result<Option<String>> {
        if args.action == "list" {
            let mut rows = Vec::new();
            let values = [
                ("names", vec!["default", "dynamic"]),
                ("flavours", vec!["default", "hard"]),
                ("modes", vec!["light", "dark"]),
                ("variants", orient_core::palette::VARIANTS.to_vec()),
            ];
            let selected = values.iter().any(|(name, _)| args.flag(name));
            for (name, values) in values {
                if !selected || args.flag(name) {
                    rows.extend(values);
                }
            }
            return Ok(Some(rows.join("\n") + "\n"));
        }
        let data = self.current()?;
        if args.action == "get" {
            let selected: Vec<_> = ["name", "flavour", "mode", "variant"]
                .into_iter()
                .filter(|name| args.flag(name))
                .collect();
            if !selected.is_empty() {
                return Ok(Some(
                    selected
                        .iter()
                        .map(|name| json::python_str(&data[*name]))
                        .collect::<Vec<_>>()
                        .join("\n")
                        + "\n",
                ));
            }
            if args.flag("json") {
                return Ok(Some(json::dumps(&data, false)? + "\n"));
            }
            let mut output = "Current scheme:\n".to_owned();
            for name in ["name", "flavour", "mode", "variant"] {
                let mut title = name.to_owned();
                title[0..1].make_ascii_uppercase();
                output.push_str(&format!("    {title}: {}\n", json::python_str(&data[name])));
            }
            if let Some(colors) = data["colours"].as_object() {
                for (key, value) in colors {
                    output.push_str(&format!("    {key}: {}\n", json::python_str(value)));
                }
            }
            return Ok(Some(output));
        }
        if args.value("accent").is_some() && args.flag("auto-accent") {
            return Err("Choose an explicit accent or automatic selection, not both".into());
        }
        let guard = classic_state::publication_lock(&self.paths)?;
        let mut data = self.current()?;
        for name in ["name", "flavour", "mode", "variant"] {
            if let Some(value) = args.value(name) {
                data[name] = json!(value);
            }
        }
        if args.flag("random") {
            data["flavour"] = json!(if uuid::Uuid::new_v4().as_bytes()[0] & 1 == 0 {
                "default"
            } else {
                "hard"
            });
        }
        let path = self.current_image()?;
        let generated = if data["name"] == "dynamic" {
            let path = path
                .as_deref()
                .filter(|v| !v.is_empty())
                .ok_or("Choose a wallpaper before selecting a dynamic palette")?;
            let path = Path::new(path);
            let resolved = crate::paths::weak_canonical(path)?;
            let resolved = resolved.to_string_lossy();
            let config_path = self.paths.config.join("cli.json");
            let mut config = state::read(&config_path, json!({}))?;
            let mut accents = config["orient"]
                .get("accents")
                .cloned()
                .unwrap_or(json!({}));
            if args.flag("auto-accent") {
                accents
                    .as_object_mut()
                    .ok_or("Invalid accent preferences")?
                    .shift_remove(resolved.as_ref());
            } else if let Some(accent) = args.value("accent") {
                accents[resolved.as_ref()] = json!(accent);
            }
            let mut options = json!({"mode":data["mode"],"variant":data["variant"],"flavour":data["flavour"],"accent":accents[resolved.as_ref()]});
            for (key, value) in self
                .image_settings(path, args)?
                .as_object()
                .ok_or("Invalid image settings")?
            {
                options[key] = value.clone();
            }
            let generated = orient::from_image(&self.analysis_path(path, args)?, &options)?;
            if args.value("accent").is_some() || args.flag("auto-accent") {
                let mut orient = config.get("orient").cloned().unwrap_or(json!({}));
                orient["accents"] = accents;
                config["orient"] = orient;
                self.write_json(&config_path, &config)?;
                let choices_path = self.paths.config.join("colors.json");
                if choices_path.exists() {
                    let mut saved = state::read(&choices_path, json!({}))?;
                    if let Some(choices) = saved["wallpapers"].as_object_mut() {
                        for (key, choice) in choices {
                            if key == resolved.as_ref()
                                || choice["poster"].as_str() == Some(resolved.as_ref())
                            {
                                if let Some(overrides) = choice["overrides"].as_object_mut() {
                                    overrides.shift_remove("primary");
                                } else {
                                    choice["overrides"] = json!({});
                                }
                                if args.flag("auto-accent") {
                                    choice
                                        .as_object_mut()
                                        .ok_or("Invalid saved choice")?
                                        .shift_remove("accent");
                                } else {
                                    choice["accent"] = json!(args.value("accent").unwrap());
                                }
                            }
                        }
                    }
                    self.write_json(&choices_path, &saved)?;
                }
            }
            generated
        } else {
            let mut value = orient_core::engine::default_palette(
                &json!({"mode":data["mode"],"variant":data["variant"],"flavour":data["flavour"]}),
            )?;
            value["engine"] = json!(orient::engine_id());
            value
        };
        self.save_scheme(&generated)?;
        if self.publish {
            self.publisher.apply_palette_held(None, true, &guard)?;
        }
        Ok(None)
    }
    fn suitable_images(
        &mut self,
        directory: &Path,
        threshold: f64,
        no_filter: bool,
    ) -> Result<Vec<PathBuf>> {
        let sizes = if no_filter {
            None
        } else {
            self.publisher
                .backend
                .query("monitors", std::time::Duration::from_secs(3), None)
                .ok()
                .and_then(|v| v.as_array().cloned())
                .filter(|v| !v.is_empty())
        };
        let dimensions = sizes.map(|items| {
            (
                items
                    .iter()
                    .filter_map(|m| m["width"].as_f64())
                    .fold(0.0, f64::max),
                items
                    .iter()
                    .filter_map(|m| m["height"].as_f64())
                    .fold(0.0, f64::max),
            )
        });
        suitable_images(directory, threshold, no_filter, dimensions)
    }
    pub fn wallpaper(&mut self, args: &Arguments) -> Result<Option<String>> {
        let data = self.current()?;
        if let Some(print) = &args.print {
            let path = if print.is_empty() {
                self.current_image()?
                    .filter(|v| !v.is_empty())
                    .ok_or("No current wallpaper")?
            } else {
                print.clone()
            };
            return Ok(Some(
                json::dumps(&self.options(Path::new(&path), args, &data)?, false)? + "\n",
            ));
        }
        let path = if let Some(random) = &args.random {
            let config = state::read(&self.paths.config.join("cli.json"), json!({}))?;
            let directory = if random.is_empty() {
                config["wallpaper"]["directory"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        self.paths
                            .home
                            .join("Pictures/Wallpapers")
                            .to_string_lossy()
                            .into_owned()
                    })
            } else {
                random.clone()
            };
            let threshold = args
                .value("threshold")
                .map(str::parse::<f64>)
                .transpose()?
                .unwrap_or(0.5);
            let choices = self.suitable_images(
                &crate::paths::expand(Path::new(&directory))?,
                threshold,
                args.flag("no-filter"),
            )?;
            if choices.is_empty() {
                return Err("No suitable local wallpapers found".into());
            }
            let entropy = u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into()?);
            choices[entropy as usize % choices.len()].clone()
        } else {
            PathBuf::from(
                args.value("file")
                    .map(str::to_owned)
                    .or(self.current_image()?)
                    .filter(|v| !v.is_empty())
                    .ok_or("Provide --file, --random or --print")?,
            )
        };
        let path = crate::paths::expand(&path)?;
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!(
                    "[Errno 2] No such file or directory: {}",
                    json::python_repr(&json!(path.to_string_lossy()))
                )
                .into())
            }
            Err(error) => return Err(error.into()),
        };
        let guard = classic_state::publication_lock(&self.paths)?;
        let data = self.current()?;
        let mut generated = self.options(&path, args, &data)?;
        let config = state::read(&self.paths.config.join("cli.json"), json!({}))?;
        let preferences = state::read(&self.paths.config.join("wallpaper-picker.json"), json!({}))?;
        if !args.flag("no-smart")
            && args.value("mode").is_none()
            && !matches!(preferences["paletteMode"].as_str(), Some("light" | "dark"))
        {
            let mut options = json!({"mode":data["mode"],"variant":data["variant"],"flavour":data["flavour"],"accent":generated["input"]["accent"],"smart":true});
            for (key, value) in self
                .image_settings(&path, args)?
                .as_object()
                .ok_or("Invalid image settings")?
            {
                options[key] = value.clone();
            }
            generated = orient::from_image(&self.analysis_path(&path, args)?, &options)?;
        }
        if data["name"] == "default" {
            generated = orient_core::engine::default_palette(
                &json!({"mode":generated["mode"],"variant":data["variant"],"flavour":data["flavour"]}),
            )?;
            generated["engine"] = json!(orient::engine_id());
        }
        let digest = if let Some(digest) = generated["source"]["digest"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            digest.to_owned()
        } else {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(fs::read(&path)?))
        };
        let thumbnail = self
            .paths
            .cache
            .join("wallpapers")
            .join(digest)
            .join("thumbnail.jpg");
        if !thumbnail.exists() {
            fs::create_dir_all(thumbnail.parent().ok_or("Invalid thumbnail directory")?)?;
            let source = crate::image_paint::thumbnail_source(&path, 640)?;
            let image = orient_decoder::thumbnail_rgb(
                &source,
                (640, 640),
                orient_decoder::Filter::Lanczos,
            )?;
            let temp = thumbnail.with_extension("tmp.jpg");
            fs::write(&temp, crate::image_paint::encode_jpeg(&image, 85)?)?;
            fs::rename(temp, &thumbnail)?;
        }
        self.save_scheme(&generated)?;
        classic_state::atomic_write(
            &self.paths.state.join("wallpaper/path.txt"),
            &path.to_string_lossy(),
        )?;
        classic_state::atomic_symlink(&self.paths.state.join("wallpaper/current"), &path)?;
        classic_state::atomic_symlink(
            &self.paths.state.join("wallpaper/thumbnail.jpg"),
            &thumbnail,
        )?;
        if let Some(hook) = config["wallpaper"]["postHook"]
            .as_str()
            .filter(|s| !s.is_empty())
        {
            let mut command = std::process::Command::new("/bin/sh");
            command
                .args(["-c", hook, "nacre-wallpaper"])
                .arg(&path)
                .env("WALLPAPER", &path)
                .env("NACRE_WALLPAPER", &path);
            crate::process::restore_child_signals(&mut command);
            let status = command.status()?;
            if !status.success() {
                return Err(format!("Wallpaper post hook exited with {status}").into());
            }
        }
        if self.publish {
            self.publisher
                .apply_palette_held(Some(&path), true, &guard)?;
        }
        Ok(None)
    }
    pub fn execute(&mut self, args: &Arguments) -> Result<Option<String>> {
        if args.command == "scheme" {
            self.scheme(args)
        } else {
            self.wallpaper(args)
        }
    }
}
pub fn run_with_program(args: &[String], program: &str, publish: bool) -> Result<i32> {
    use std::io::Write;
    if args == ["-h"] || args == ["--help"] {
        print!("{}", root_help(program));
        return Ok(0);
    }
    if args == ["-v"] || args == ["--version"] {
        println!("{}", orient::engine_id());
        return Ok(0);
    }
    let parsed = match parse(args) {
        Ok(args) => args,
        Err(error) => {
            if let Some(context) = error.downcast_ref::<Help>() {
                print!("{}", help(program, &context.0));
                return Ok(0);
            }
            if let Some(error) = error.downcast_ref::<Usage>() {
                let name = if error.context.is_empty() {
                    program.into()
                } else {
                    format!("{program} {}", error.context)
                };
                eprintln!(
                    "{}{name}: error: {}",
                    usage(program, &error.context),
                    error.message
                );
                return Ok(2);
            }
            return Err(error);
        }
    };
    let result = Cli::environment(publish)?.execute(&parsed);
    match result {
        Ok(Some(text)) => {
            std::io::stdout().write_all(text.as_bytes())?;
            Ok(0)
        }
        Ok(None) => Ok(0),
        Err(error) => {
            eprintln!("Orient: {error}");
            if parsed.flag("notify") {
                if let Err(error) = notify(&error.to_string()) {
                    eprintln!("Palette notification failed: {error}");
                }
            }
            Ok(1)
        }
    }
}
fn notify(message: &str) -> Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )?;
    let _: u32 = proxy.call(
        "Notify",
        &(
            "Orient",
            0u32,
            "",
            "Orient",
            message,
            Vec::<String>::new(),
            std::collections::HashMap::<String, zbus::zvariant::Value>::new(),
            -1i32,
        ),
    )?;
    Ok(())
}
pub fn run(args: &[String]) -> Result<i32> {
    if args.first().is_some_and(|s| s == "--publish") {
        run_with_program(&args[1..], "nacre_shell", true)
    } else {
        run_with_program(args, "nacre_shell", false)
    }
}

pub fn suitable_images(
    directory: &Path,
    threshold: f64,
    no_filter: bool,
    dimensions: Option<(f64, f64)>,
) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = walkdir::WalkDir::new(directory)
        .min_depth(1)
        .into_iter()
        .map(|entry| entry.map(|e| e.path().to_owned()).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.is_file()
            && path.extension().is_some_and(|s| {
                crate::wallpaper_media::IMAGES
                    .contains(&s.to_string_lossy().to_lowercase().as_str())
            })
    });
    paths.sort();
    if no_filter {
        return Ok(paths);
    }
    if !(0.0 < threshold && threshold <= 1.0) {
        return Err("Wallpaper size threshold must be above 0 and at most 1".into());
    }
    let (width, height) = dimensions.unwrap_or((1920.0, 1080.0));
    Ok(paths
        .into_iter()
        .filter(|path| {
            image::ImageReader::open(path)
                .ok()
                .and_then(|reader| reader.with_guessed_format().ok())
                .and_then(|reader| reader.into_dimensions().ok())
                .is_some_and(|(w, h)| {
                    f64::from(w) >= width * threshold && f64::from(h) >= height * threshold
                })
        })
        .collect())
}
