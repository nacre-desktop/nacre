//! Developer-only mapping for historical module globals and publication tests.
use nacre_helper::{login_appearance as login, paths::Paths, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
fn main() {
    let result = (|| -> Result<Value> {
        let request: Value =
            serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
        let text = |name| request[name].as_str().ok_or("Missing text argument");
        let paths = Paths::new(text("home")?);
        match text("operation")? {
            "prepare" => {
                let (path, stamp) = login::prepare(
                    &paths,
                    Path::new(text("wallpaper")?),
                    request["cache"].as_str().map(Path::new),
                )?;
                Ok(json!([path, stamp]))
            }
            "publish" => {
                let context = &request["policy"];
                let policy = login::Policy {
                    base_home: PathBuf::from(
                        context["base_home"].as_str().ok_or("Missing base home")?,
                    ),
                    system_public: PathBuf::from(
                        context["system_public"]
                            .as_str()
                            .ok_or("Missing system appearance path")?,
                    ),
                    unix_home: nix::unistd::User::from_uid(nix::unistd::getuid())?
                        .ok_or("Missing Unix account")?
                        .dir,
                };
                Ok(json!(login::publish(
                    &paths,
                    &request["colors"],
                    Path::new(text("wallpaper")?),
                    request["preview"].as_str().map(Path::new),
                    request["public"].as_str().map(Path::new),
                    &policy
                )?))
            }
            _ => Err("Unknown login appearance operation".into()),
        }
    })();
    let (response, code) = match result {
        Ok(value) => (json!({"result":value}), 0),
        Err(error) => {
            let kind = error
                .downcast_ref::<login::LoginError>()
                .map_or("OSError", |error| error.kind);
            (json!({"error":error.to_string(),"type":kind}), 1)
        }
    };
    println!(
        "{}",
        nacre_helper::json::encode(&response).expect("Serializable reply")
    );
    std::process::exit(code);
}
