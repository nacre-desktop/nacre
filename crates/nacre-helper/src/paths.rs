//! Match NacrePaths.qml's absolute-XDG rule; installation paths remain HOME-relative.
use crate::Result;
use std::{
    env,
    ffi::OsStr,
    fs,
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
    /// Explicit fixture homes use defaults; the current home honors absolute XDG roots.
    pub fn for_home(home: &Path) -> Result<Self> {
        if env::var_os("HOME").is_some_and(|current| Path::new(&current) == home) {
            Self::environment()
        } else {
            Ok(Self::new(home))
        }
    }
    /// Resolve read-only helper assets in a source checkout or installed tree.
    pub fn assets(&self) -> PathBuf {
        if let Some(folder) = env::var_os("NACRE_HELPER_TOOLS") {
            return PathBuf::from(folder);
        }
        let installed = self.tools();
        if installed.join("nacre-helper").is_file() {
            return installed;
        }
        let source = env::current_dir()
            .unwrap_or_default()
            .join("nacre/shell-tools");
        if source.join("notification-policy.json").is_file() {
            source
        } else {
            self.tools()
        }
    }
    pub fn tools(&self) -> PathBuf {
        self.share.join("shell/tools")
    }
    pub fn applications(&self) -> PathBuf {
        self.home.join(".local/share/applications")
    }
    pub fn recordings(&self) -> PathBuf {
        self.home.join("Videos/Recordings")
    }
    pub fn screenshots(&self) -> PathBuf {
        self.home.join("Pictures/Screenshots")
    }
}

pub fn expand(path: &Path) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    if text == "~" || text.starts_with("~/") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| {
                nix::unistd::User::from_uid(nix::unistd::getuid())
                    .ok()
                    .flatten()
                    .map(|user| user.dir)
            })
            .ok_or("Could not determine home directory.")?;
        return Ok(home.join(text.strip_prefix("~/").unwrap_or("")));
    }
    if let Some(rest) = text.strip_prefix('~') {
        let (name, tail) = rest.split_once('/').unwrap_or((rest, ""));
        let user =
            nix::unistd::User::from_name(name)?.ok_or("Could not determine home directory.")?;
        return Ok(user.dir.join(tail));
    }
    Ok(path.to_owned())
}
pub fn weak_canonical(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut parent = absolute.as_path();
    let mut tails = Vec::new();
    loop {
        match fs::canonicalize(parent) {
            Ok(mut root) => {
                for tail in tails.iter().rev() {
                    root.push(tail);
                }
                return Ok(root);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tails.push(
                    parent
                        .file_name()
                        .ok_or("Invalid appearance path")?
                        .to_owned(),
                );
                parent = parent.parent().ok_or("Invalid appearance root")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (digit(bytes[index + 1]), digit(bytes[index + 2])) {
                result.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        result.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
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
