//! Explicit, terminal-visible development environment plans. Discovery never installs.
use crate::{i18n::tr, paths::Paths, process, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, IsTerminal, Write},
    path::Path,
    process::Command,
    time::Duration,
};

fn profiles() -> Value {
    serde_json::from_str(include_str!(
        "../../../nacre/shell-tools/development-environments.json"
    ))
    .expect("checked development catalog")
}
fn tool(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|folder| {
        let path = folder.join(name);
        path.is_file() && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    })
}
fn inspect(program: &str, args: &[&str], home: &Path) -> Option<Value> {
    let args = args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    // Discovery runs from HOME, not an arbitrary project with its own mise config.
    let mut command = Command::new(program);
    command.args(&args).current_dir(home);
    command.env("HOME", home);
    for (key, relative) in [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_STATE_HOME", ".local/state"),
    ] {
        if std::env::var_os("HOME").is_none_or(|owner| Path::new(&owner) != home) {
            command.env(key, home.join(relative));
        }
    }
    let output = process::capture(command, None, Duration::from_secs(3));
    output
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
}
fn installed_tools(data: &Value) -> BTreeSet<String> {
    data.as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .filter(|(_, records)| {
            records
                .as_array()
                .is_some_and(|records| records.iter().any(|record| record["installed"] == true))
        })
        .map(|(name, _)| name.clone())
        .collect()
}
pub fn snapshot(paths: &Paths) -> Value {
    let mise = tool("mise");
    let global = if mise {
        inspect(
            "mise",
            &["ls", "--global", "--installed", "--json"],
            &paths.home,
        )
    } else {
        None
    };
    let installed = installed_tools(global.as_ref().unwrap_or(&Value::Null));
    let docker = tool("docker");
    let daemon = docker
        && inspect(
            "docker",
            &["info", "--format", "{{json .ServerVersion}}"],
            &paths.home,
        )
        .is_some();
    let terminal = tool("kitty");
    let mut rows = profiles().as_array().unwrap().clone();
    for row in &mut rows {
        let database = row["kind"] == "database";
        let ready = if database {
            daemon && tool("systemd-ask-password")
        } else {
            mise && global.is_some()
        };
        let owned = if database {
            let name = format!("nacre-dev-{}", row["id"].as_str().unwrap());
            inspect(
                "docker",
                &["inspect", "--format", "{{json .Config.Labels}}", &name],
                &paths.home,
            )
            .is_some_and(|labels| labels["io.nacre.development"] == row["id"])
        } else if let Some(framework) = row["framework"].as_str() {
            match framework {
                "phoenix" => archives(paths).is_ok_and(|a| !a.is_empty()),
                "rails" => global
                    .as_ref()
                    .and_then(|g| g["ruby"].as_array())
                    .is_some_and(|records| {
                        records.iter().any(|record| {
                            if record["installed"] != true {
                                return false;
                            }
                            let path = record["install_path"]
                                .as_str()
                                .map(std::path::PathBuf::from)
                                .filter(|p| p.is_absolute())
                                .or_else(|| {
                                    let version = record["version"].as_str()?;
                                    if !version
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                                    {
                                        return None;
                                    }
                                    let data = std::env::var_os("XDG_DATA_HOME")
                                        .map(std::path::PathBuf::from)
                                        .filter(|p| p.is_absolute())
                                        .unwrap_or_else(|| paths.home.join(".local/share"));
                                    Some(data.join("mise/installs/ruby").join(version))
                                });
                            path.is_some_and(|p| p.join("bin/rails").is_file())
                        })
                    }),
                "laravel" => [
                    ".config/composer/vendor/bin/laravel",
                    ".composer/vendor/bin/laravel",
                ]
                .iter()
                .any(|p| paths.home.join(p).is_file()),
                _ => false,
            }
        } else {
            row["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| installed.contains(t.as_str().unwrap().split('@').next().unwrap()))
        };
        row["installed"] = json!(owned);
        row["available"] = json!(ready && terminal);
        row["reason"] = json!(if !terminal {
            tr(
                "software.development.requires-terminal",
                "Requires Kitty for a visible terminal",
            )
        } else if database && !daemon {
            tr(
                "software.development.requires-docker",
                "Requires a running Docker daemon",
            )
        } else if database && !tool("systemd-ask-password") {
            tr(
                "software.development.requires-password",
                "Requires systemd-ask-password for database credentials",
            )
        } else if !database && !mise {
            tr(
                "software.development.requires-mise",
                "Install mise to manage development environments",
            )
        } else if !ready {
            tr(
                "software.development.discovery-error",
                "Could not inspect development tools",
            )
        } else {
            String::new()
        });
    }
    json!({"profiles":rows,"mise":mise,"terminal":terminal})
}
fn archives(paths: &Paths) -> Result<Vec<String>> {
    let directory = paths.home.join(".mix/archives");
    if !directory.exists() {
        return Ok(vec![]);
    }
    Ok(fs::read_dir(directory)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            (name.starts_with("phx_new-")
                && name.len() < 100
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)))
            .then_some(name)
        })
        .collect())
}
fn command(args: &[&str]) -> Value {
    json!({"arguments":args})
}
pub fn plan(paths: &Paths, operation: &str, id: &str) -> Result<Value> {
    if !matches!(operation, "install" | "remove") {
        return Err("Unknown environment operation".into());
    }
    let profile = profiles()
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .cloned()
        .ok_or("Unknown development environment")?;
    let mut steps = vec![];
    if profile["kind"] == "database" {
        let name = format!("nacre-dev-{id}");
        if operation == "remove" {
            let container = inspect(
                "docker",
                &[
                    "inspect",
                    "--format",
                    "{\"id\":{{json .Id}},\"labels\":{{json .Config.Labels}}}",
                    &name,
                ],
                &paths.home,
            )
            .ok_or("Database container was not found")?;
            if container["labels"]["io.nacre.development"] != id {
                return Err("Container is not owned by Nacre; preserved".into());
            }
            let container_id = container["id"]
                .as_str()
                .filter(|id| id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or("Invalid container identity")?;
            steps.push(command(&["docker", "rm", "--force", container_id]));
        } else {
            let port = profile["port"].as_u64().ok_or("Missing database port")?;
            let mount = format!(
                "type=volume,source={name},target={}",
                profile["mount"].as_str().unwrap()
            );
            let mut args = vec![
                "docker".into(),
                "run".into(),
                "--detach".into(),
                "--name".into(),
                name,
                "--label".into(),
                format!("io.nacre.development={id}"),
                "--publish".into(),
                format!("127.0.0.1:{port}:{port}"),
                "--mount".into(),
                mount,
            ];
            let secret = profile["passwordEnv"].as_str().unwrap_or("");
            if !secret.is_empty() {
                args.extend(["--env".into(), secret.into()])
            }
            args.push(profile["image"].as_str().unwrap().into());
            if let Some(startup) = profile["startup"].as_array() {
                args.extend(startup.iter().map(|a| a.as_str().unwrap().to_owned()));
            }
            steps.push(json!({"arguments":args,"passwordEnv":secret}));
        }
    } else if operation == "install" {
        for entry in profile["tools"].as_array().unwrap() {
            steps.push(command(&[
                "mise",
                "use",
                "--global",
                entry.as_str().unwrap(),
            ]));
        }
        match profile["framework"].as_str() {
            Some("rails") => steps.push(command(&[
                "mise",
                "exec",
                "ruby",
                "--",
                "gem",
                "install",
                "rails",
                "--no-document",
            ])),
            Some("phoenix") => {
                steps.push(command(&[
                    "mise",
                    "exec",
                    "elixir",
                    "--",
                    "mix",
                    "local.hex",
                    "--force",
                ]));
                steps.push(command(&[
                    "mise",
                    "exec",
                    "elixir",
                    "--",
                    "mix",
                    "local.rebar",
                    "--force",
                ]));
                steps.push(command(&[
                    "mise",
                    "exec",
                    "elixir",
                    "--",
                    "mix",
                    "archive.install",
                    "hex",
                    "phx_new",
                    "--force",
                ]));
            }
            Some("laravel") => steps.push(command(&[
                "mise",
                "exec",
                "github:nunomaduro/static-php-builds",
                "--",
                "composer",
                "global",
                "require",
                "laravel/installer",
            ])),
            _ => {}
        }
    } else {
        match profile["framework"].as_str() {
            Some("rails") => steps.push(command(&[
                "mise",
                "exec",
                "ruby",
                "--",
                "gem",
                "uninstall",
                "rails",
                "--all",
                "--executables",
                "--ignore-dependencies",
            ])),
            Some("laravel") => steps.push(command(&[
                "mise",
                "exec",
                "github:nunomaduro/static-php-builds",
                "--",
                "composer",
                "global",
                "remove",
                "laravel/installer",
            ])),
            Some("phoenix") => {
                for archive in archives(paths)? {
                    steps.push(command(&[
                        "mise",
                        "exec",
                        "elixir",
                        "--",
                        "mix",
                        "archive.uninstall",
                        &archive,
                        "--force",
                    ]));
                }
            }
            _ => {
                for entry in profile["tools"].as_array().unwrap() {
                    let name = entry.as_str().unwrap().split('@').next().unwrap();
                    steps.push(command(&["mise", "unuse", "--global", name]));
                }
            }
        }
    }
    Ok(
        json!({"id":id,"operation":operation,"name":profile["name"],"steps":steps,"dataRetained":profile["kind"]=="database"&&operation=="remove","requiresRoot":false}),
    )
}
pub fn run(paths: &Paths, operation: &str, id: &str) -> Result<i32> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Development changes require a visible interactive terminal".into());
    }
    let profile = snapshot(paths)["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .cloned()
        .ok_or("Unknown development environment")?;
    if profile["available"] != true {
        return Err(profile["reason"]
            .as_str()
            .unwrap_or("Environment is unavailable")
            .into());
    }
    let plan = plan(paths, operation, id)?;
    println!("{}: {}", operation, plan["name"].as_str().unwrap());
    for step in plan["steps"].as_array().unwrap() {
        println!("{}", serde_json::to_string(&step["arguments"])?)
    }
    if plan["dataRetained"] == true {
        println!(
            "{}",
            tr(
                "software.development.volume-retained",
                "Database volumes are retained; only the managed container is removed."
            )
        );
    }
    print!(
        "{} ",
        tr("software.development.confirm", "Type yes to continue:")
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim() != "yes" {
        return Ok(0);
    }
    let mut code = 0;
    for step in plan["steps"].as_array().unwrap() {
        let args = step["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect::<Vec<_>>();
        let mut child = Command::new(args[0]);
        child.args(&args[1..]).current_dir(&paths.home);
        // Mise manages only the chosen global tools; framework removals retain runtimes.
        child.env("MISE_RUBY_COMPILE", "false");
        if let Some(variable) = step["passwordEnv"].as_str().filter(|s| !s.is_empty()) {
            let result = Command::new("systemd-ask-password")
                .arg("Password for the local development database")
                .output()?;
            if !result.status.success() {
                return Err("Password entry cancelled".into());
            }
            let password = String::from_utf8(result.stdout)?;
            let password = password.trim_end();
            if password.is_empty() {
                return Err("Database password cannot be empty".into());
            }
            child.env(variable, password);
        }
        code = child.status()?.code().unwrap_or(1);
        if code != 0 {
            break;
        }
    }
    println!(
        "{}",
        tr(
            "software.development.finished",
            "Finished. Press Enter to close this terminal."
        )
    );
    answer.clear();
    io::stdin().read_line(&mut answer)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_removal_and_framework_removal_preserve_other_tools() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::new(temp.path());
        assert_eq!(
            plan(&paths, "remove", "node")?["steps"][0]["arguments"],
            json!(["mise", "unuse", "--global", "node"])
        );
        let rails = plan(&paths, "remove", "rails")?;
        assert_eq!(rails["steps"].as_array().unwrap().len(), 1);
        assert_eq!(rails["steps"][0]["arguments"][5], "uninstall");
        let laravel = plan(&paths, "remove", "laravel")?;
        assert_eq!(laravel["steps"][0]["arguments"][6], "remove");
        assert!(plan(&paths, "install", "$(evil)").is_err());
        assert!(plan(&paths, "--force", "node").is_err());
        Ok(())
    }
    #[test]
    fn database_is_loopback_only_and_password_is_not_in_arguments() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::new(temp.path());
        let p = plan(&paths, "install", "postgres")?;
        let args = serde_json::to_string(&p["steps"][0]["arguments"])?;
        assert!(args.contains("127.0.0.1:5432:5432"));
        assert!(args.contains("io.nacre.development=postgres"));
        assert_eq!(p["steps"][0]["passwordEnv"], "POSTGRES_PASSWORD");
        assert!(!args.contains("trust"));
        assert!(!args.contains("PASSWORD="));
        Ok(())
    }
    #[test]
    fn discovery_ignores_missing_requests() {
        assert_eq!(
            installed_tools(&json!({"node":[{"installed":false}],"python":[{"installed":true}]})),
            BTreeSet::from(["python".into()])
        );
    }
}
