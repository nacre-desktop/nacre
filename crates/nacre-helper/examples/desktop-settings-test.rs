//! Developer-only callback transport for the retained Python assertions.
use nacre_helper::{
    desktop_settings::{self as desktop, Effect, Runtime, Settings},
    json as wire,
    paths::Paths,
    Result,
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    path::Path,
};
fn read() -> Result<Value> {
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Err("Test provider disconnected".into());
    }
    Ok(serde_json::from_str(&line)?)
}
fn callback(value: Value) -> Result<Value> {
    println!("{}", serde_json::to_string(&json!({"callback":value}))?);
    io::stdout().flush()?;
    let response = read()?;
    if let Some(error) = response["error"].as_str() {
        Err(error.to_owned().into())
    } else {
        Ok(response["result"].clone())
    }
}
struct Provider {
    input: Value,
}
impl Runtime for Provider {
    fn compositor(&mut self, operation: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        match operation {
            "backend_name" => Ok(self.input["backend"].clone()),
            "capabilities" => Ok(self.input["capabilities"].clone()),
            "desktop_option_keys" => Ok(self.input["options"].clone()),
            "config_path" => Ok(self.input["config"].clone()),
            _ => callback(
                json!({"kind":"compositor","operation":operation,"args":args,"kwargs":kwargs}),
            ),
        }
    }
    fn monitors(&mut self) -> Result<Value> {
        if self.input["monitor_override"] == true {
            callback(json!({"kind":"monitor_state"}))
        } else {
            let all = self.compositor("query", &[json!("all_monitors")], &json!({"timeout":8}))?;
            desktop::monitor_records(&all)
        }
    }
    fn write(&mut self, path: &Path, text: &str) -> Result<()> {
        callback(json!({"kind":"write","path":path,"text":text}))?;
        Ok(())
    }
    fn effect(&mut self, effect: Effect<'_>) -> Result<()> {
        let event = match effect {
            Effect::Font(family) => json!({"kind":"font","family":family}),
            Effect::Workflow {
                action,
                name,
                roles,
            } => json!({"kind":"workflow","action":action,"name":name,"roles":roles}),
            Effect::Lock => json!({"kind":"lock"}),
            Effect::Gradients => json!({"kind":"gradients"}),
            Effect::Rollback { token, environment } => {
                json!({"kind":"rollback","token":token,"environment":environment})
            }
        };
        callback(event)?;
        Ok(())
    }
    fn sleep(&mut self, seconds: u64) {
        if let Err(error) = callback(json!({"kind":"sleep","seconds":seconds})) {
            eprintln!("{error}");
        }
    }
}
fn run() -> Result<Value> {
    let request = read()?;
    let operation = request["operation"].as_str().ok_or("Missing operation")?;
    if operation == "native-rollback" {
        let paths = Paths::environment()?;
        let mut backend = nacre_helper::compositor::backend::Backend::new(
            paths.clone(),
            std::env::vars().collect(),
        )?;
        let mut native = desktop::Native::new(paths, &mut backend);
        native.effect(Effect::Rollback {
            token: "0123456789abcdef0123456789abcdef",
            environment: &[],
        })?;
        return Ok(Value::Null);
    }
    if operation == "metadata" {
        return Ok(
            json!({"defaults":desktop::defaults(),"inputKeys":desktop::INPUT_KEYS,"presetKeys":desktop::preset_keys()}),
        );
    }
    if operation == "validate" {
        return desktop::validate(
            request["key"].as_str().ok_or("Missing key")?,
            &request["value"],
            request["developer"] == true,
        );
    }
    if operation == "display_plan" {
        return desktop::display_plan(
            request["monitors"].as_array().ok_or("Missing monitors")?,
            request["primary"].as_str().ok_or("Missing primary")?,
            request["mode"].as_str().ok_or("Missing mode")?,
        );
    }
    let paths = Paths::for_home(Path::new(request["home"].as_str().ok_or("Missing home")?))?;
    let mut settings = Settings::new(
        paths,
        Provider {
            input: request.clone(),
        },
    )?;
    settings.state = request["state"].as_str().ok_or("Missing state")?.into();
    settings.config = request["config"].as_str().ok_or("Missing config")?.into();
    settings.pending = request["pending"].as_str().ok_or("Missing pending")?.into();
    settings.defaults = request["defaults"].clone();
    match operation {
        "load" => settings.load(),
        "monitors" => settings.monitors(),
        "persist" => {
            settings.persist(&request["data"])?;
            Ok(Value::Null)
        }
        "revert" => {
            settings.revert(request["token"].as_str())?;
            Ok(Value::Null)
        }
        "main" => {
            let arguments: Vec<String> = serde_json::from_value(request["args"].clone())?;
            let value = settings.execute(&arguments[0], &arguments[1..])?;
            println!(
                "{}",
                serde_json::to_string(&json!({"print":wire::dumps(&value,false)?}))?
            );
            Ok(Value::Null)
        }
        _ => Err("Unknown test operation".into()),
    }
}
fn main() {
    let value = match run() {
        Ok(result) => json!({"result":result}),
        Err(error) => json!({"error":error.to_string()}),
    };
    println!("{}", serde_json::to_string(&value).unwrap());
}
