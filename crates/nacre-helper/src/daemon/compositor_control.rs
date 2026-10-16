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
