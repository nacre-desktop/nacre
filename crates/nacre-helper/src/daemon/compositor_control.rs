//! Shared native request transport for compositor observation and session policy.
use crate::{compositor::backend::Backend, paths::Paths, Result};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Control {
    backend: Arc<Mutex<Option<Backend>>>,
    paths: Paths,
    environment: BTreeMap<String, String>,
}
impl Control {
    pub async fn niri_action(
        &self,
        identity: String,
        argv: Vec<String>,
    ) -> Result<serde_json::Value> {
        let path = self
            .environment
            .get("NIRI_SOCKET")
            .cloned()
            .ok_or("Missing Niri socket")?;
        self.with(move |backend| {
            // Check again after waiting for the shared request connection.
            if super::niri_events::identity(std::path::Path::new(&path))? != identity {
                return Err("Compositor session mismatch".into());
            }
            Ok(crate::compositor::cli::action_response(
                backend,
                &serde_json::json!({"argv":argv}),
            ))
        })
        .await
    }
    /// Verify both socket identities without opening another compositor connection.
    pub async fn niri_identity(&self, selected: Option<String>) -> Result<String> {
        let environment = self.environment.clone();
        tokio::task::spawn_blocking(move || {
            if crate::compositor::backend::select(&environment)? != "niri" {
                return Err("Selected compositor is not niri".into());
            }
            let path = environment
                .get("NIRI_SOCKET")
                .ok_or("Missing Niri socket")?;
            let identity = super::niri_events::identity(std::path::Path::new(path))?;
            if let Some(selected) = selected {
                if super::niri_events::identity(std::path::Path::new(&selected))? != identity {
                    return Err("Compositor session mismatch".into());
                }
            }
            Ok(identity)
        })
        .await?
    }
    pub fn new(paths: Paths, environment: BTreeMap<String, String>) -> Self {
        Self {
            backend: Arc::new(Mutex::new(None)),
            paths,
            environment,
        }
    }
    /// Wait for transport ownership asynchronously, occupying a blocking worker
    /// only while the existing synchronous native implementation does its work.
    pub async fn with<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Backend) -> Result<T> + Send + 'static,
    {
        let mut backend = self.backend.clone().lock_owned().await;
        let paths = self.paths.clone();
        let environment = self.environment.clone();
        tokio::task::spawn_blocking(move || {
            if backend.is_none() {
                *backend = Some(Backend::new(paths, environment)?);
            }
            operation(
                backend
                    .as_mut()
                    .ok_or("Compositor transport is unavailable")?,
            )
        })
        .await?
    }
}
