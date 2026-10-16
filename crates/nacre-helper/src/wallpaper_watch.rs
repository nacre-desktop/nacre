//! Debounced inotify subscriptions; the idle path blocks without polling scans.
use crate::{json, paths::Paths, wallpaper_media, Result};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::{AsFd, OwnedFd},
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
    engines: Vec<PathBuf>,
    config: PathBuf,
    fd: OwnedFd,
    paths: BTreeMap<i32, PathBuf>,
}
impl Watcher {
    fn rescan(&mut self) -> Result<()> {
        let mut wanted = BTreeSet::from([
            self.root.clone(),
            parent(&self.scheme),
            parent(&self.root),
            self.config.clone(),
        ]);
        wanted.extend(self.engines.iter().map(|engine| parent(engine)));
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
    fn read_events(
        fd: &OwnedFd,
        buffer: &mut [MaybeUninit<u8>],
    ) -> std::io::Result<Vec<(i32, u32, OsString)>> {
        let events = {
            let mut reader = inotify::Reader::new(fd, buffer);
            let mut events = Vec::new();
            loop {
                let event = reader.next().map_err(std::io::Error::from)?;
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
        Ok(events)
    }
    fn apply(
        &mut self,
        events: Vec<(i32, u32, OsString)>,
        previous: &mut [Value; 3],
    ) -> Result<u8> {
        let mut dirty = 0;
        let all = (1u8 << self.engines.len()) - 1;
        for (wd, mask, name) in events {
            if mask & 0x4000 != 0 {
                eprintln!("Wallpaper event queue overflow; rebuilding subscriptions");
                dirty |= all;
                continue;
            }
            if mask & IGNORED != 0 {
                self.paths.remove(&wd);
                continue;
            }
            let Some(path) = self.paths.get(&wd).cloned() else {
                continue;
            };
            if self.engines.iter().any(|engine| path == parent(engine)) {
                for (index, engine) in self.engines.iter().enumerate() {
                    if path == parent(engine) && Some(name.as_os_str()) == engine.file_name() {
                        dirty |= 1 << index;
                    }
                }
            } else if path == parent(&self.scheme) {
                if Some(name.as_os_str()) == self.scheme.file_name()
                    && mask & (0x8 | 0x40 | 0x80 | 0x200 | 0x400 | 0x800) != 0
                {
                    // IN_CREATE precedes the writer's content. Read only
                    // after close/move/delete, so valid writes are never
                    // reported as malformed while still in progress.
                    let current = flavour(&self.scheme)?;
                    if current != *previous {
                        dirty |= all;
                    }
                    *previous = current;
                }
            } else if path == self.config {
                if name == "cli.json" || name == "wallpaper-picker.json" {
                    dirty |= all;
                }
            } else if path == parent(&self.root) {
                if Some(name.as_os_str()) == self.root.file_name() {
                    dirty |= all;
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
                        dirty |= all;
                    }
                }
            }
        }
        Ok(dirty)
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
                let events = Self::read_events(&self.fd, &mut buffer)?;
                let dirty = self.apply(events, &mut previous)?;
                if dirty != 0 {
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
/// Same event parser/filter as the standalone command, registered on the daemon
/// runtime. Distinct engine paths have independent debounce deadlines; common
/// library/scheme changes notify every source. No timer is armed while idle.
#[allow(unsafe_code)]
pub(crate) async fn observe(
    paths: &Paths,
    engines: Vec<PathBuf>,
    mut publish: impl FnMut(Option<u8>) -> Result<()>,
) -> Result<()> {
    if engines.is_empty() || engines.len() > 3 {
        return Err("Invalid wallpaper source count".into());
    }
    let root = paths.home.join("Pictures/Wallpapers");
    let scheme = paths.state.join("scheme.json");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(parent(&scheme))?;
    let mut watcher = Watcher {
        root,
        scheme,
        engines,
        config: paths.config.clone(),
        fd: inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)?,
        paths: BTreeMap::new(),
    };
    watcher.rescan()?;
    let mut previous = flavour(&watcher.scheme)?;
    // The duplicate refers to the same inotify source. Only this task reads it;
    // Watcher retains its original descriptor for add/remove operations.
    let descriptor = watcher.fd.as_fd().try_clone_to_owned()?;
    // SAFETY: AsyncFd owns this descriptor throughout its registration.
    let descriptor = unsafe { tokio::io::unix::AsyncFd::register(descriptor) }?;
    let mut buffer = vec![MaybeUninit::uninit(); 262144];
    let mut deadlines = vec![None; watcher.engines.len()];
    publish(None)?;
    loop {
        let next = deadlines.iter().flatten().copied().min();
        let timeout = async {
            match next {
                Some(next) => tokio::time::sleep_until(next).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            ready = descriptor.readable() => {
                let mut ready = ready?;
                let events = match ready.try_io(|fd| Watcher::read_events(fd.get_ref(), &mut buffer)) {
                    Ok(events) => events?,
                    Err(_) => continue,
                };
                drop(ready);
                let dirty = watcher.apply(events, &mut previous)?;
                for (index, deadline) in deadlines.iter_mut().enumerate() {
                    if dirty & (1 << index) != 0 {
                        *deadline = Some(tokio::time::Instant::now() + Duration::from_millis(500));
                    }
                }
            },
            _ = timeout => {
                let now = tokio::time::Instant::now();
                let mut changed = 0;
                for (index, deadline) in deadlines.iter_mut().enumerate() {
                    if deadline.is_some_and(|deadline| deadline <= now) {
                        *deadline = None;
                        changed |= 1 << index;
                    }
                }
                watcher.rescan()?;
                publish(Some(changed))?;
            },
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
    if let Some(code) = crate::daemon::wallpaper::watch(&paths, &root, &scheme, &engine)? {
        return Ok(code);
    }
    fs::create_dir_all(&root)?;
    fs::create_dir_all(parent(&scheme))?;
    Watcher {
        root,
        scheme,
        engines: vec![engine],
        config: paths.config,
        fd: inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)?,
        paths: BTreeMap::new(),
    }
    .run()?;
    Ok(0)
}
