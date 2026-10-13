//! Match NacrePaths.qml's absolute-XDG rule; installation paths remain HOME-relative.
use crate::Result;
use std::{
    env,
    ffi::OsStr,
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
        Self::with_xdg(home.as_ref(), None, None, None)
    }
    fn with_xdg(
        home: &Path,
        config: Option<&OsStr>,
        state: Option<&OsStr>,
        cache: Option<&OsStr>,
    ) -> Self {
        let home = home.to_path_buf();
        let root = |value: Option<&OsStr>, fallback: &str| {
            value
                .map(Path::new)
                .filter(|path| path.is_absolute())
                .map_or_else(|| home.join(fallback), Path::to_path_buf)
                .join("nacre")
        };
        Self {
            config: root(config, ".config"),
            state: root(state, ".local/state"),
            cache: root(cache, ".cache"),
            share: home.join(".local/share/nacre"),
            home,
        }
    }
    pub fn environment() -> Result<Self> {
        let home = env::var_os("HOME").ok_or("Missing home directory")?;
        Ok(Self::with_xdg(
            Path::new(&home),
            env::var_os("XDG_CONFIG_HOME").as_deref(),
            env::var_os("XDG_STATE_HOME").as_deref(),
            env::var_os("XDG_CACHE_HOME").as_deref(),
        ))
    }
    pub fn tools(&self) -> PathBuf {
        self.share.join("shell/tools")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_xdg_roots_match_qml() {
        let paths = Paths::with_xdg(
            Path::new("/case/home"),
            Some(OsStr::new("/case/config")),
            Some(OsStr::new("/case/state/")),
            Some(OsStr::new("/case/cache")),
        );
        assert_eq!(paths.config, Path::new("/case/config/nacre"));
        assert_eq!(paths.state, Path::new("/case/state/nacre"));
        assert_eq!(paths.cache, Path::new("/case/cache/nacre"));
        assert_eq!(
            paths.tools(),
            Path::new("/case/home/.local/share/nacre/shell/tools")
        );
    }

    #[test]
    fn unset_empty_and_relative_xdg_roots_fall_back() {
        for value in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("relative")),
            Some(OsStr::new("~/custom")),
        ] {
            let paths = Paths::with_xdg(Path::new("/case/home"), value, value, value);
            let defaults = Paths::new("/case/home");
            assert_eq!(paths.config, defaults.config);
            assert_eq!(paths.state, defaults.state);
            assert_eq!(paths.cache, defaults.cache);
        }
    }
}
