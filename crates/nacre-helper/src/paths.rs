//! Preserve the helpers' existing HOME-relative state and installation paths.
use crate::Result;
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub share: PathBuf,
}
impl Paths {
    pub fn new(home: impl AsRef<Path>) -> Self {
        let home = home.as_ref().to_path_buf();
        Self {
            config: home.join(".config/nacre"),
            state: home.join(".local/state/nacre"),
            cache: home.join(".cache/nacre"),
            share: home.join(".local/share/nacre"),
            home,
        }
    }
    pub fn environment() -> Result<Self> {
        Ok(Self::new(
            env::var_os("HOME").ok_or("Missing home directory")?,
        ))
    }
    pub fn tools(&self) -> PathBuf {
        self.share.join("shell/tools")
    }
}
