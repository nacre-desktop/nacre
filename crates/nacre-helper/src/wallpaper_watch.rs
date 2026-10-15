//! Debounced inotify subscriptions; the idle path blocks without polling scans.
use crate::{json, paths::Paths, wallpaper_media, Result};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    fs::inotify,
    io::Errno,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::Write,
    mem::MaybeUninit,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MASK: u32 = 0x8 | 0x40 | 0x80 | 0x100 | 0x200 | 0x400 | 0x800;
const ISDIR: u32 = 0x40000000;
const IGNORED: u32 = 0x8000;
#[derive(Debug)]
pub struct FlavourType(pub &'static str);
impl std::fmt::Display for FlavourType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}' object has no attribute 'get'", self.0)
    }
}
impl std::error::Error for FlavourType {}
pub fn flavour(path: &Path) -> Result<[Value; 3]> {
    let defaults = [json!("default"), json!("dark"), json!("tonalspot")];
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(defaults),
        Err(error) => {
            eprintln!("Wallpaper scheme read: {error}");
            return Ok(defaults);
        }
    };
    let data: Value = match serde_json::from_slice(&raw) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("Wallpaper scheme JSON: {error}");
            return Ok(defaults);
        }
    };
    let Some(data) = data.as_object() else {
        return Err(FlavourType(match data {
            Value::Array(_) => "list",
            Value::String(_) => "str",
            Value::Null => "NoneType",
            Value::Bool(_) => "bool",
            Value::Number(number) if number.is_f64() => "float",
            _ => "int",
        })
        .into());
    };
    Ok(std::array::from_fn(|index| {
        data.get(["flavour", "mode", "variant"][index])
            .cloned()
            .unwrap_or_else(|| defaults[index].clone())
    }))
}
fn parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_owned()
}
struct Watcher {
    root: PathBuf,
    scheme: PathBuf,
    engine: PathBuf,
    config: PathBuf,
    fd: OwnedFd,
    paths: BTreeMap<i32, PathBuf>,
}
impl Watcher {
    fn rescan(&mut self) -> Result<()> {
        let mut wanted = BTreeSet::from([
            self.root.clone(),
            parent(&self.scheme),
            parent(&self.engine),
            parent(&self.root),
            self.config.clone(),
        ]);
        if self.root.is_dir() {
            for entry in walkdir::WalkDir::new(&self.root).follow_links(false) {
                let entry = entry?;
                if entry.file_type().is_dir() && !entry.path().is_symlink() {
                    wanted.insert(entry.into_path());
                }
            }
        }
        let removed = self
            .paths
            .iter()
            .filter(|(_, path)| !wanted.contains(*path))
            .map(|(wd, _)| *wd)
            .collect::<Vec<_>>();
        for wd in removed {
            match inotify::remove_watch(&self.fd, wd) {
                Ok(()) | Err(Errno::INVAL) => (),
                Err(error) => return Err(error.into()),
            }
            self.paths.remove(&wd);
        }
        let known = self.paths.values().cloned().collect::<BTreeSet<_>>();
        for path in wanted.difference(&known) {
            match inotify::add_watch(&self.fd, path, inotify::WatchFlags::from_bits_retain(MASK)) {
                Ok(wd) => {
                    self.paths.insert(wd, path.clone());
                }
                Err(Errno::NOENT) => (), // Optional preferences/runtime directories may not exist yet.
                Err(error) => {
                    return Err(format!("Cannot watch {}: {error}", path.display()).into())
                }
            }
        }
        Ok(())
    }
    fn run(&mut self) -> Result<()> {
        self.rescan()?;
        let mut previous = flavour(&self.scheme)?;
        let mut deadline: Option<Instant> = None;
        let mut buffer = vec![MaybeUninit::uninit(); 262144];
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        loop {
            let timeout = deadline
                .map(|time| Timespec::try_from(time.saturating_duration_since(Instant::now())))
                .transpose()?;
            let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
            match poll(&mut fds, timeout.as_ref()) {
                Ok(_) => (),
                Err(Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            if fds[0]
                .revents()
                .intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
            {
                return Err("Wallpaper notification descriptor closed".into());
            }
            if fds[0].revents().contains(PollFlags::IN) {
                let events = {
                    let mut reader = inotify::Reader::new(&self.fd, &mut buffer);
                    let mut events = Vec::new();
                    loop {
                        let event = reader.next()?;
                        let name = OsString::from(Path::new(std::ffi::OsStr::from_bytes(
                            event
                                .file_name()
                                .map_or(b"".as_slice(), |name| name.to_bytes()),
                        )));
                        events.push((event.wd(), event.events().bits(), name));
                        if reader.is_buffer_empty() {
                            break;
                        }
                    }
                    events
                };
                let mut dirty = false;
                for (wd, mask, name) in events {
                    if mask & 0x4000 != 0 {
                        eprintln!("Wallpaper event queue overflow; rebuilding subscriptions");
                        dirty = true;
                        continue;
                    }
                    if mask & IGNORED != 0 {
                        self.paths.remove(&wd);
                        continue;
                    }
                    let Some(path) = self.paths.get(&wd).cloned() else {
                        continue;
                    };
                    if path == parent(&self.engine) {
                        dirty |= Some(name.as_os_str()) == self.engine.file_name();
                    } else if path == parent(&self.scheme) {
                        if Some(name.as_os_str()) == self.scheme.file_name()
                            && mask & (0x8 | 0x40 | 0x80 | 0x200 | 0x400 | 0x800) != 0
                        {
                            // IN_CREATE precedes the writer's content. Read only
                            // after close/move/delete, so valid writes are never
                            // reported as malformed while still in progress.
                            let current = flavour(&self.scheme)?;
                            dirty |= current != previous;
                            previous = current;
                        }
                    } else if path == self.config {
                        dirty |= name == "cli.json" || name == "wallpaper-picker.json";
                    } else if path == parent(&self.root) {
                        if Some(name.as_os_str()) == self.root.file_name() {
                            dirty = true;
                            self.rescan()?;
                        }
                    } else {
                        let suffix = Path::new(&name)
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase();
                        if mask & ISDIR != 0
                            || wallpaper_media::IMAGES.contains(&suffix.as_str())
                            || wallpaper_media::VIDEOS.contains(&suffix.as_str())
                        {
                            if mask & ISDIR != 0 {
                                self.rescan()?;
                            }
                            if mask & (0x8 | 0x40 | 0x80 | 0x200 | 0x400 | 0x800) != 0 {
                                dirty = true;
                            }
                        }
                    }
                }
                if dirty {
                    deadline = Some(Instant::now() + Duration::from_millis(500));
                }
            }
            if deadline.is_some_and(|time| Instant::now() >= time) {
                self.rescan()?;
                writeln!(output, "{}", json::dumps(&json!({"changed":true}), false)?)?;
                output.flush()?;
                deadline = None;
            }
        }
    }
}
const USAGE: &str = "usage: wallpaper-watch.py [-h] [--root ROOT] [--scheme SCHEME]\n                          [--engine ENGINE]\n";
fn argument_value(value: &str) -> bool {
    if !value.starts_with('-') {
        return true;
    }
    static NEGATIVE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    NEGATIVE
        .get_or_init(|| regex::Regex::new(r"^-\.?\d").expect("fixed argparse numeric prefix"))
        .is_match(value)
}
pub fn run(args: &[String]) -> Result<i32> {
    let paths = Paths::environment()?;
    let mut root = paths.home.join("Pictures/Wallpapers");
    let mut scheme = paths.state.join("scheme.json");
    let mut engine = paths.share.join("shell/bin/nacre_shell");
    let mut index = 0;
    let mut unknown = Vec::new();
    while index < args.len() {
        let token = &args[index];
        index += 1;
        if token == "--" {
            unknown.extend_from_slice(&args[index - 1..]);
            break;
        }
        let (key, inline) = token
            .split_once('=')
            .map_or((token.as_str(), None), |(key, value)| {
                (key, Some(value.to_owned()))
            });
        let key = if key.starts_with("--") {
            ["--root", "--scheme", "--engine", "--help"]
                .into_iter()
                .find(|candidate| candidate.starts_with(key))
                .unwrap_or(key)
        } else {
            key
        };
        if key == "--help" || key == "-h" {
            if let Some(value) = inline {
                eprintln!("{USAGE}wallpaper-watch.py: error: argument -h/--help: ignored explicit argument {}", json::python_repr(&json!(value)));
                return Ok(2);
            }
            print!("{USAGE}\noptions:\n  -h, --help       show this help message and exit\n  --root ROOT\n  --scheme SCHEME\n  --engine ENGINE\n");
            return Ok(0);
        }
        let target = match key {
            "--root" => &mut root,
            "--scheme" => &mut scheme,
            "--engine" => &mut engine,
            _ => {
                unknown.push(token.clone());
                continue;
            }
        };
        let value = if let Some(value) = inline {
            value
        } else {
            let value = args.get(index).filter(|value| argument_value(value));
            let Some(value) = value else {
                eprintln!(
                    "{USAGE}wallpaper-watch.py: error: argument {key}: expected one argument"
                );
                return Ok(2);
            };
            index += 1;
            value.clone()
        };
        *target = if value.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(value)
        };
    }
    if !unknown.is_empty() {
        eprintln!(
            "{USAGE}wallpaper-watch.py: error: unrecognized arguments: {}",
            unknown.join(" ")
        );
        return Ok(2);
    }
    fs::create_dir_all(&root)?;
    fs::create_dir_all(parent(&scheme))?;
    Watcher {
        root,
        scheme,
        engine,
        config: paths.config,
        fd: inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)?,
        paths: BTreeMap::new(),
    }
    .run()?;
    Ok(0)
}
