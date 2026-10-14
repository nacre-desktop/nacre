//! Cached RGB login images and data-only publication; no greeter/authentication code.
use crate::{i18n::tr, json, paths::Paths, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
pub const PUBLIC: &str = "/var/lib/nacre/login/appearance";
pub const ROLES: [(&str, &str); 9] = [
    ("surface", "surface"),
    ("raised", "surfaceContainer"),
    ("primary", "primary"),
    ("secondary", "secondary"),
    ("tertiary", "tertiary"),
    ("highlight", "primaryFixed"),
    ("text", "onSurface"),
    ("muted", "onSurfaceVariant"),
    ("error", "error"),
];
#[derive(Debug)]
pub struct LoginError {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for LoginError {}
fn fail<T>(kind: &'static str, message: String) -> Result<T> {
    Err(LoginError { kind, message }.into())
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid appearance output")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::Builder::new()
        .prefix(&format!(
            ".{}",
            path.file_name().ok_or("Invalid output")?.to_string_lossy()
        ))
        .tempfile_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    Ok(())
}
fn private_directory(path: &Path, chmod: bool) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("Invalid appearance directory")?)?;
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => (),
        Err(error) => return Err(error.into()),
    }
    if chmod {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn expanded(path: &Path) -> Result<PathBuf> {
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
fn weak_canonical(path: &Path) -> Result<PathBuf> {
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
pub fn prepare(paths: &Paths, wallpaper: &Path, cache: Option<&Path>) -> Result<(PathBuf, String)> {
    let image = expanded(wallpaper)?;
    let image = match fs::canonicalize(&image) {
        Ok(image) => image,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return fail(
                "FileNotFoundError",
                format!(
                    "[Errno 2] No such file or directory: {}",
                    json::python_repr(&Value::String(image.to_string_lossy().into_owned()))
                ),
            )
        }
        Err(error) => return Err(error.into()),
    };
    let meta = fs::metadata(&image)?;
    let cache = cache
        .map(Path::to_owned)
        .unwrap_or_else(|| paths.cache.join("login-images"));
    private_directory(&cache, false)?;
    let nanos = i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec());
    let stamp = format!("{}:{nanos}:{}:login-image-v1", image.display(), meta.len());
    let target = cache.join(format!("{:x}.png", Sha256::digest(stamp.as_bytes())));
    if !target.exists() {
        let bytes = orient_decoder::rgb_preview_png(&image, (2560, 1600), 10.0)?;
        atomic(&target, &bytes)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
    }
    Ok((target, stamp))
}
#[derive(Clone, Debug)]
pub struct Policy {
    pub base_home: PathBuf,
    pub system_public: PathBuf,
    pub unix_home: PathBuf,
}
impl Policy {
    pub fn environment() -> Result<Self> {
        Ok(Self {
            base_home: Paths::environment()?.home,
            system_public: PathBuf::from(PUBLIC),
            unix_home: nix::unistd::User::from_uid(nix::unistd::getuid())?
                .ok_or("Unix account is unavailable")?
                .dir,
        })
    }
}
pub fn publish(
    paths: &Paths,
    colors: &Value,
    wallpaper: &Path,
    preview: Option<&Path>,
    public: Option<&Path>,
    policy: &Policy,
) -> Result<PathBuf> {
    let canonical_preview = paths.state.join("shell/login-preview");
    let preview = preview.unwrap_or(&canonical_preview);
    let public = public.unwrap_or_else(|| Path::new(PUBLIC));
    let mut values = Vec::new();
    for (key, role) in ROLES {
        let Some(color) = colors.get(role) else {
            return fail("KeyError", json::python_repr(&Value::String(role.into())));
        };
        let color = color.as_str().ok_or("Invalid login palette value")?;
        values.push((key, format!("#{}", color.trim_start_matches('#'))));
    }
    if values.iter().any(|(_, value)| {
        value.len() != 7 || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return fail(
            "ValueError",
            tr(
                "helper.login-appearance.invalid-login-palette",
                "Invalid login palette",
            ),
        );
    }
    private_directory(preview, true)?;
    let canonical = weak_canonical(preview)? == weak_canonical(&canonical_preview)?;
    let cache = if canonical {
        paths.cache.join("login-images")
    } else {
        preview.join("image-cache")
    };
    let (cached, stamp) = prepare(paths, wallpaper, Some(&cache))?;
    let background = preview.join("background.png");
    let old_stamp = preview.join("image-stamp.txt");
    if !background.exists() || !old_stamp.exists() || fs::read_to_string(&old_stamp)? != stamp {
        atomic(&background, &fs::read(cached)?)?;
        atomic(&old_stamp, stamp.as_bytes())?;
    }
    let mut destinations = vec![preview];
    if canonical
        && weak_canonical(&paths.home)? == weak_canonical(&policy.base_home)?
        && (public != policy.system_public
            || weak_canonical(&paths.home)? == weak_canonical(&policy.unix_home)?)
        && public.is_dir()
        && fs::metadata(public)?.uid() == nix::unistd::getuid().as_raw()
        && nix::unistd::access(public, nix::unistd::AccessFlags::W_OK).is_ok()
    {
        destinations.push(public);
    }
    for destination in destinations {
        if destination != preview {
            atomic(&destination.join("background.png"), &fs::read(&background)?)?;
        }
        let mut text = String::from("[General]\n");
        for (key, value) in &values {
            text.push_str(&format!("{key} = {value}\n"));
        }
        text.push_str(&format!(
            "background = {}\n\n",
            destination.join("background.png").display()
        ));
        atomic(&destination.join("theme.conf"), text.as_bytes())?;
    }
    Ok(preview.to_owned())
}
pub fn run(arguments: &[String]) -> Result<i32> {
    if arguments.first().is_some_and(|word| word == "--prepare") {
        let image = arguments.get(1).ok_or("Expected wallpaper path")?;
        let paths = Paths::environment()?;
        let result = prepare(&paths, Path::new(image), arguments.get(2).map(Path::new))?;
        println!(
            "{}",
            json::encode(&serde_json::json!([result.0, result.1]))?
        );
        return Ok(0);
    }
    if arguments.first().is_some_and(|word| word == "--publish") {
        let mut text = String::new();
        std::io::stdin()
            .take(16 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        if text.len() > 16 * 1024 * 1024 {
            return Err("Login appearance request is too large".into());
        }
        let request: Value = serde_json::from_str(&text)?;
        let paths = if let Some(home) = request["home"].as_str() {
            Paths::for_home(Path::new(home))?
        } else {
            Paths::environment()?
        };
        publish(
            &paths,
            &request["colors"],
            Path::new(request["wallpaper"].as_str().ok_or("Missing wallpaper")?),
            request["preview"].as_str().map(Path::new),
            request["public"].as_str().map(Path::new),
            &Policy::environment()?,
        )?;
    } else {
        let paths = Paths::environment()?;
        let scheme = crate::state::read(&paths.state.join("scheme.json"), Value::Null)?;
        let wallpaper = fs::read_to_string(paths.state.join("wallpaper/last.txt"))?;
        publish(
            &paths,
            &scheme["colours"],
            Path::new(wallpaper.trim()),
            None,
            None,
            &Policy::environment()?,
        )?;
    }
    Ok(0)
}
