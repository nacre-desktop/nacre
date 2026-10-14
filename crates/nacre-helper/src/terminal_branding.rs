//! Kitty logo registration and frame edits restricted to the original owned PTY.
use crate::{files, i18n::tr, json, paths::Paths, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json as value, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{
        ffi::OsStringExt,
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::OnceLock,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub parent: String,
    pub device: String,
    pub started: String,
}
#[derive(Clone, Debug)]
pub struct Terminal {
    pub uid: u32,
    pub character: bool,
    pub inode: u64,
    pub device: u64,
}
impl From<fs::Metadata> for Terminal {
    fn from(meta: fs::Metadata) -> Self {
        Self {
            uid: meta.uid(),
            character: meta.file_type().is_char_device(),
            inode: meta.ino(),
            device: meta.rdev(),
        }
    }
}
fn tty_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\A/dev/pts/\d+\z").expect("checked PTY pattern"))
}
pub fn identity_for_uid(pid: u32, proc: &Path, uid: u32) -> Result<Identity> {
    let directory = proc.join(pid.to_string());
    if fs::metadata(&directory)?.uid() != uid {
        return Err(tr(
            "helper.terminal-branding.other-user-process",
            "Other user process",
        )
        .into());
    }
    let text = fs::read_to_string(directory.join("stat"))?;
    let end = text.rfind(')').ok_or("Invalid terminal process stat")?;
    let fields: Vec<_> = text[end + 1..].split_whitespace().collect();
    if fields.len() < 20 {
        return Err("Incomplete terminal process stat".into());
    }
    Ok(Identity {
        parent: fields[1].into(),
        device: fields[4].into(),
        started: fields[19].into(),
    })
}
pub fn identity(pid: u32, proc: &Path) -> Result<Identity> {
    identity_for_uid(pid, proc, nix::unistd::getuid().as_raw())
}
pub fn packet(image: &Value, path: &Path, edit: bool) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let fields = if edit { "a=f,r=1,X=1" } else { "a=T,r=16" };
    let encoded = STANDARD.encode(path.as_os_str().as_bytes());
    format!(
        "\x1b_G{fields},t=f,f=100,i={},q=2;{encoded}\x1b\\",
        json::python_str(image)
    )
    .into_bytes()
}
pub fn register_at(
    paths: &Paths,
    proc: &Path,
    tty: &str,
    terminal: &Terminal,
    mut parent: u32,
    image: u32,
) -> Result<Option<Value>> {
    if !tty_pattern().is_match(tty)
        || terminal.uid != nix::unistd::getuid().as_raw()
        || !terminal.character
    {
        return Ok(None);
    }
    let mut candidate = None;
    for _ in 0..12 {
        let info = identity(parent, proc)?;
        let name = fs::read_to_string(proc.join(parent.to_string()).join("comm"))?;
        if name.trim() == "kitty" {
            break;
        }
        if matches!(name.trim(), "fish" | "bash" | "zsh")
            && info.device.parse::<u64>()? == terminal.device
        {
            candidate = Some((parent, info.started));
        }
        parent = info.parent.parse()?;
        if parent <= 1 {
            break;
        }
    }
    let Some((pid, started)) = candidate else {
        return Ok(None);
    };
    let record = value!({"pid":pid,"started":started,"tty":tty,"inode":terminal.inode,"device":terminal.device,"image":image.max(1)});
    let folder = paths.cache.join("terminal-logos");
    fs::create_dir_all(folder.parent().ok_or("Invalid terminal cache")?)?;
    match fs::DirBuilder::new().mode(0o700).create(&folder) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && folder.is_dir() => (),
        Err(error) => return Err(error.into()),
    }
    files::atomic(
        &folder.join(format!(
            "{}.json",
            tty.rsplit('/').next().ok_or("Invalid PTY")?
        )),
        json::encode(&record)?.as_bytes(),
    )?;
    Ok(Some(record))
}
pub fn register(paths: &Paths, proc: &Path) -> Result<Option<Value>> {
    let tty = rustix::termios::ttyname(std::io::stdin(), Vec::new())?;
    let tty = PathBuf::from(std::ffi::OsString::from_vec(tty.into_bytes()));
    let text = tty.to_str().ok_or("Invalid terminal path encoding")?;
    if !tty_pattern().is_match(text) {
        return Ok(None);
    }
    let terminal = Terminal::from(fs::metadata(&tty)?);
    let random = uuid::Uuid::new_v4();
    let image = u32::from_be_bytes(random.as_bytes()[..4].try_into()?);
    register_at(
        paths,
        proc,
        text,
        &terminal,
        nix::unistd::getppid().as_raw().try_into()?,
        image,
    )
}
fn integer(value: &Value) -> Result<u64> {
    if let Some(value) = value.as_u64() {
        return Ok(value);
    }
    if let Some(value) = value.as_bool() {
        return Ok(u64::from(value));
    }
    if let Some(value) = value.as_str() {
        return Ok(value.trim().parse()?);
    }
    if let Some(value) = value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0 && *value < u64::MAX as f64)
    {
        return Ok(value.trunc() as u64);
    }
    Err("Invalid terminal process ID".into())
}
fn refresh_record(paths: &Paths, proc: &Path, file: &Path) -> Result<()> {
    let record: Value = serde_json::from_slice(&fs::read(file)?)?;
    let tty = record["tty"].as_str().ok_or("Missing terminal path")?;
    if !tty_pattern().is_match(tty) {
        return Err(tr(
            "helper.terminal-branding.unexpected-terminal-path",
            "Unexpected terminal path",
        )
        .into());
    }
    let info = identity(integer(&record["pid"])?.try_into()?, proc)?;
    if record["started"] != info.started || record["device"] != info.device.parse::<u64>()? {
        return Err(tr(
            "helper.terminal-branding.terminal-shell-ended-or-process-id-reused",
            "Terminal shell ended or process id reused",
        )
        .into());
    }
    let flags =
        rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NOCTTY;
    let mut fd = fs::OpenOptions::new()
        .write(true)
        .custom_flags(flags.bits().try_into()?)
        .open(tty)?;
    let meta = fd.metadata()?;
    if !meta.file_type().is_char_device()
        || meta.uid() != nix::unistd::getuid().as_raw()
        || record["inode"] != meta.ino()
        || record["device"] != meta.rdev()
    {
        return Err(tr(
            "helper.terminal-branding.terminal-device-changed",
            "Terminal device changed",
        )
        .into());
    }
    let image = &record["image"];
    let number = image.as_u64().or_else(|| image.as_bool().map(u64::from));
    if !number.is_some_and(|number| number > 0 && number < 1u64 << 32) {
        return Err(tr(
            "helper.terminal-branding.invalid-image-id",
            "Invalid image id",
        )
        .into());
    }
    fd.write_all(&packet(
        image,
        &paths.share.join("branding/nacre.png"),
        true,
    ))?;
    Ok(())
}
pub fn refresh(paths: &Paths, proc: &Path) -> Result<()> {
    let folder = paths.cache.join("terminal-logos");
    if !folder.is_dir() {
        return Ok(());
    }
    let mut records: Vec<_> = fs::read_dir(&folder)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    records.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "json")
    });
    records.sort();
    for file in records.into_iter().take(256) {
        if let Err(error) = refresh_record(paths, proc, &file) {
            eprintln!("Terminal logo registration removed: {error}");
            match fs::remove_file(&file) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}
pub fn render_with<F: FnOnce() -> Result<Option<Value>>>(
    paths: &Paths,
    environment: &BTreeMap<String, String>,
    prepare: bool,
    registration: F,
) -> Result<Vec<u8>> {
    let image = paths.share.join("branding/nacre.png");
    let setting = |name| environment.get(name).map_or("", String::as_str);
    if setting("TERM") == "xterm-kitty"
        && setting("TMUX").is_empty()
        && setting("NO_COLOR").is_empty()
        && image.is_file()
    {
        let result: Result<Option<Vec<u8>>> = (|| {
            let Some(record) = registration()? else {
                return Ok(None);
            };
            let data = packet(&record["image"], &image, false);
            if prepare {
                let key = format!("{}-{}", setting("KITTY_PID"), setting("KITTY_WINDOW_ID"));
                if !Regex::new(r"\A\d+-\d+\z")?.is_match(&key) {
                    return Ok(Some(Vec::new()));
                }
                fs::write(
                    paths
                        .cache
                        .join("terminal-logos")
                        .join(format!("{key}.raw")),
                    data,
                )?;
                Ok(Some(Vec::new()))
            } else {
                Ok(Some(data))
            }
        })();
        match result {
            Ok(Some(data)) => return Ok(data),
            Ok(None) => (),
            Err(error) => eprintln!("Terminal logo registration failed; using text: {error}"),
        }
    }
    let fallback = paths.share.join("branding/nacre-text.txt");
    let text = if fallback.is_file() {
        fs::read_to_string(fallback)?
    } else {
        "Nacre".into()
    };
    Ok(format!("{}\n", text.trim_end()).into_bytes())
}
pub fn run(arguments: &[String]) -> Result<i32> {
    if arguments.first().is_some_and(|word| word == "--refresh") {
        let paths = if let Some(home) = arguments.get(1) {
            Paths::for_home(Path::new(home))?
        } else {
            Paths::environment()?
        };
        refresh(&paths, Path::new("/proc"))?;
    } else {
        let paths = Paths::environment()?;
        let bytes = render_with(
            &paths,
            &std::env::vars().collect(),
            arguments.iter().any(|word| word == "--prepare"),
            || register(&paths, Path::new("/proc")),
        )?;
        std::io::stdout().write_all(&bytes)?;
    }
    Ok(0)
}
