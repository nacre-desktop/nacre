//! Inert hardware/timer provider for the unchanged original controller fixtures.
use nacre_helper::{
    compositor::backend::Backend,
    desktop_settings::{Effect, Runtime, Settings},
    json as wire,
    paths::Paths,
    Result,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
struct Fixture {
    backend: Backend,
    home: PathBuf,
    monitors: Value,
    effects: Vec<Value>,
    failure: String,
    failed: bool,
}
fn original_schema() -> nacre_helper::state::Schema {
    serde_json::from_str(include_str!(
        "../tests/fixtures/desktop-settings-state-schema-input.json"
    ))
    .expect("frozen original state schema")
}
impl Fixture {
    fn record(&mut self, operation: &str, args: Value, kwargs: Value) {
        self.effects
            .push(json!({"operation":operation,"args":args,"kwargs":kwargs}));
    }
}
impl Runtime for Fixture {
    fn read(&mut self, path: &Path, fallback: Value) -> Result<Value> {
        if path.file_name().is_some_and(|name| name == "desktop.json") {
            nacre_helper::state::read_as(path, Some(&original_schema()), fallback)
        } else {
            nacre_helper::state::read(path, fallback)
        }
    }
    fn compositor(&mut self, operation: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        match operation {
            "query" => Ok(self.monitors.clone()),
            "set_desktop_options" | "configure_monitors" | "apply_config" | "reload" => {
                self.record(operation, json!(args), kwargs.clone());
                Ok(Value::Null)
            }
            "ensure_desktop_loader" => {
                self.record(operation, json!([{"$path":self.home}]), kwargs.clone());
                Ok(Value::Null)
            }
            _ => Ok(self.backend.module_call(operation, args, kwargs)?),
        }
    }
    fn effect(&mut self, effect: Effect<'_>) -> Result<()> {
        match effect {
            Effect::Font(family) => self.record("terminal-font", json!([family]), json!({})),
            Effect::Workflow { .. } => (), // The original fixture has no saved profile.
            Effect::Rollback { token, environment } => {
                let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap();
                let mut command = vec![
                    "systemd-run".to_owned(),
                    "--user".into(),
                    "--collect".into(),
                    "--quiet".into(),
                    format!("--unit=nacre-display-revert-{token}"),
                ];
                command.extend(environment.iter().map(|value| format!("--setenv={value}")));
                command.extend([
                    std::env::var("NACRE_REFERENCE_PYTHON").unwrap_or("{python}".into()),
                    root.join("nacre/shell-tools/desktop-settings.py")
                        .to_string_lossy()
                        .into_owned(),
                    "rollback-after".into(),
                    token.to_owned(),
                ]);
                self.record("systemd-run", json!([command]), json!({}));
            }
            _ => return Err("Unscripted fixture effect".into()),
        }
        Ok(())
    }
    fn write(&mut self, path: &Path, text: &str) -> Result<()> {
        if !self.failed
            && ((self.failure == "mono-font-state"
                && path.file_name().is_some_and(|v| v == "desktop.json"))
                || (self.failure == "display-config"
                    && path
                        .file_name()
                        .is_some_and(|v| v == "desktop.lua" || v == "desktop-niri.kdl")))
        {
            self.failed = true;
            return Err("fixture write failure".into());
        }
        if path.file_name().is_some_and(|name| name == "desktop.json") {
            nacre_helper::state::write_text_as(path, &original_schema(), text)
        } else {
            nacre_helper::classic_state::atomic_write(path, text)
        }
    }
    fn token(&mut self) -> String {
        "0123456789abcdef0123456789abcdef".into()
    }
    fn sleep(&mut self, seconds: u64) {
        self.record("sleep", json!([seconds]), json!({}));
    }
}
fn run() -> Result<()> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input: Value = serde_json::from_str(&raw)?;
    let paths = Paths::environment()?;
    let backend = Backend::new(paths.clone(), std::env::vars().collect())?;
    let monitors = json!([
        {"name":"DP-1","width":2560,"height":1440,"refreshRate":120,"scale":1.25,"x":0,"y":0,"transform":0,"disabled":false,"mirrorOf":"none","availableModes":["2560x1440@120Hz"]},
        {"name":"DP-2","width":1920,"height":1080,"refreshRate":120,"scale":1,"x":0,"y":0,"transform":0,"disabled":false,"mirrorOf":"none","availableModes":["1920x1080@120Hz"]}
    ]);
    let fixture = Fixture {
        backend,
        home: paths.home.clone(),
        monitors: monitors.clone(),
        effects: Vec::new(),
        failure: input["failure"].as_str().unwrap_or("").into(),
        failed: false,
    };
    let mut settings = Settings::new(paths, fixture)?;
    // These immutable scenarios were captured with the original 51 defaults.
    // New Niri preferences have their own additive reference, not new old outputs.
    let reference: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/desktop-settings-validation-reference.json"
    ))?;
    settings.defaults = reference["defaults"].clone();
    // The frozen Python provider explicitly injected this configuration route.
    settings.config = settings
        .state
        .with_file_name(if settings.backend == "niri" {
            "desktop-niri.kdl"
        } else {
            "desktop.lua"
        });
    fs::create_dir_all(settings.state.parent().unwrap())?;
    let mut saved = settings.defaults.as_object().unwrap().clone();
    for key in nacre_helper::desktop_settings::INPUT_KEYS {
        saved.shift_remove(*key);
    }
    for (key,value) in json!({"gapsOut":27,"lockMedia":false,"lockWeather":false,"lockNotifications":false,"lockNotificationContents":true,"weatherLocation":"Fixture city","weatherFahrenheit":true}).as_object().unwrap(){saved.insert(key.clone(),value.clone());}
    if settings.runtime.failure == "mono-font-state" {
        saved.insert("monoFont".into(), json!("Old Mono"));
    }
    fs::write(&settings.state, wire::dumps(&saved, false)?)?;
    let args: Vec<String> = serde_json::from_value(input["args"].clone())?;
    if ["confirm", "revert", "rollback-after"].contains(&args[0].as_str()) {
        fs::write(
            &settings.pending,
            wire::dumps(
                &json!({"token":"fixed-token","previous":[],"monitors":monitors}),
                false,
            )?,
        )?;
    }
    let output = settings.execute(&args[0], &args[1..])?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"stdout":format!("{}\n",wire::dumps(&output,false)?),"exit":0,"effects":settings.runtime.effects})
        )?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
