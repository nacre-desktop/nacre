//! Narrow provider adapter: a revision belongs to one daemon boot. Failed or
//! ambiguous mutations never fall back to a competing disk writer.
use crate::{
    daemon::{client, settings_validation},
    paths::Paths,
    state, Result,
};
use nacre_protocol::Message;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct Document {
    pub data: Value,
    path: PathBuf,
    store: &'static str,
    revision: Option<(String, u64)>,
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?)
}

async fn connection() -> Result<Option<client::Client>> {
    if std::env::var_os("NACRE_DAEMON_SOCKET").is_none()
        && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
    match client::connect().await {
        Ok(client) => Ok(Some(client)),
        Err(client::ConnectError::Absent) if std::env::var_os("NACRE_DAEMON_SOCKET").is_none() => {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn boot(client: &client::Client) -> Result<&str> {
    match &client.welcome {
        Message::Welcome { boot, .. } => Ok(boot),
        _ => Err("Missing daemon boot identity".into()),
    }
}

fn provider_expectations(store: &str, accepted: &Value) -> Vec<state::FieldExpectation> {
    let mut expected = Vec::new();
    for key in settings_validation::provider_keys(store) {
        if store == "colors" && key == "appThemes" {
            for theme in [
                "gtk3", "gtk4", "qt", "kde", "kitty", "rofi", "lock", "login",
                "chromium", "discord", "spotify",
            ] {
                expected.push(state::FieldExpectation::from_data(
                    &accepted["data"],
                    vec!["appThemes".into(), theme.into()],
                ));
            }
        } else {
            expected.push(state::FieldExpectation::from_data(
                &accepted["data"],
                vec![key],
            ));
        }
    }
    expected
}

impl Document {
    pub fn is_hosted(&self) -> bool {
        self.revision.is_some()
    }

    pub fn open(path: &Path, store: &'static str, fallback: Value) -> Result<Self> {
        if ![
            "colors",
            "wallpaper-picker",
            "dashboard",
            "cli",
            "apps",
            "launcher-workflows",
        ]
        .contains(&store)
        {
            return Err("Store has no native provider adapter".into());
        }
        let remote = runtime()?.block_on(async {
            // Developer callers can supply disposable Paths independently of
            // HOME. Never probe an ambient desktop for a different path.
            if std::env::var_os("NACRE_DAEMON_SOCKET").is_none()
                && state::settings_path_in(&Paths::environment()?, store)? != path
            {
                return Ok(None);
            }
            let Some(mut client) = connection().await? else {
                return Ok(None);
            };
            if state::settings_path_in(&Paths::environment()?, store)? != path {
                return Err("Provider path does not match daemon settings".into());
            }
            let snapshot = client
                .call("settings.get", json!({"store":store}), Some(5000))
                .await?;
            if snapshot.get("error").is_some_and(|v| !v.is_null()) {
                return Err("Resolve the settings file error before using its provider".into());
            }
            let revision = snapshot["rev"]
                .as_u64()
                .ok_or("Missing settings revision")?;
            Ok::<_, crate::Error>(Some((
                boot(&client)?.to_owned(),
                revision,
                snapshot["data"].clone(),
            )))
        })?;
        let (data, revision) = match remote {
            Some((boot, revision, data)) => {
                let data = if let (Some(defaults), Some(saved)) =
                    (fallback.as_object(), data.as_object())
                {
                    let mut merged = defaults.clone();
                    merged.extend(saved.clone());
                    Value::Object(merged)
                } else {
                    data
                };
                (data, Some((boot, revision)))
            }
            None => (state::read(path, fallback)?, None),
        };
        Ok(Self {
            data,
            path: path.into(),
            store,
            revision,
        })
    }

    /// Returns false only for an operation that started without a daemon.
    /// The caller then retains its exact existing standalone serializer.
    pub fn save(&mut self, value: &Value) -> Result<bool> {
        let Some((expected_boot, revision)) = &self.revision else {
            return Ok(false);
        };
        let mut value = value.clone();
        value
            .as_object_mut()
            .ok_or("Invalid provider document")?
            .insert("version".into(), json!(state::by_id(self.store)?.version));
        let snapshot = runtime()?.block_on(async {
            let mut client = connection()
                .await?
                .ok_or("Daemon ended before provider write")?;
            if boot(&client)? != expected_boot {
                return Err("Daemon restarted; refresh before writing settings".into());
            }
            if state::settings_path_in(&Paths::environment()?, self.store)? != self.path {
                return Err("Provider path does not match daemon settings".into());
            }
            let accepted = client
                .call(
                    "settings.provider",
                    json!({"store":self.store,"value":value,"rev":revision}),
                    Some(30000),
                )
                .await?;
            let expected = provider_expectations(self.store, &accepted);
            state::wait_store(&mut client, self.store, accepted, &expected).await
        })?;
        self.revision = Some((
            expected_boot.clone(),
            snapshot["rev"]
                .as_u64()
                .ok_or("Missing settings revision")?,
        ));
        self.data = snapshot["data"].clone();
        Ok(true)
    }
}

pub fn read(path: &Path, store: &'static str, fallback: Value) -> Result<Value> {
    Ok(Document::open(path, store, fallback)?.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_expectations_cover_omissions_but_ignore_unknown_color_themes() {
        let accepted = json!({
            "data": {
                "favorites": [],
                "appThemes": {"gtk3": true, "chromium": true, "futureTheme": "kept"},
                "futureField": {"kept": true}
            }
        });
        let expected = provider_expectations("colors", &accepted);
        assert!(state::expectations_match(&accepted["data"], &expected));

        let mut future_changed = accepted["data"].clone();
        future_changed["appThemes"]["futureTheme"] = json!("changed");
        future_changed["futureField"]["kept"] = json!(false);
        assert!(state::expectations_match(&future_changed, &expected));

        let mut deleted_field_restored = accepted["data"].clone();
        deleted_field_restored["history"] = json!(null);
        assert!(!state::expectations_match(
            &deleted_field_restored,
            &expected
        ));

        let mut owned_theme_changed = accepted["data"].clone();
        owned_theme_changed["appThemes"]["chromium"] = json!(false);
        assert!(!state::expectations_match(&owned_theme_changed, &expected));
    }
}
