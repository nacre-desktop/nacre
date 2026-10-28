//! Read-only projection of the existing writer's atomic presentation record.
use super::hub::{failure, Notice, Shared};
use crate::{paths::Paths, Result};
use nacre_protocol::{
    wallpaper::{
        ActiveWallpaper, MotionKind, Presentation, WallpaperActivate, WallpaperActive,
        WallpaperFileIdentity, WallpaperMotion, WallpaperPalette,
    },
    Capability, ErrorCode, Failure, Method, MAX_SAFE_INTEGER,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs::File,
    io::Read,
    path::{Component, Path},
};

pub const TOPIC: &str = "wallpaper.active";
const PALETTE_PREWARM_BATCH: usize = 32;
const PALETTE_PREWARM_QUEUE: usize = 128;
static SELECTION_GENERATION: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static SELECTION_TASK: std::cell::RefCell<Option<tokio::task::JoinHandle<()>>> = const { std::cell::RefCell::new(None) };
}

#[derive(Default)]
struct PalettePrewarmQueue {
    ids: VecDeque<String>,
    active: HashSet<String>,
    running: bool,
}
static PALETTE_PREWARM: OnceLock<Mutex<PalettePrewarmQueue>> = OnceLock::new();

fn enqueue_palette_prewarm(ids: &[String]) {
    let queue = PALETTE_PREWARM.get_or_init(|| Mutex::new(PalettePrewarmQueue::default()));
    let start = {
        let Ok(mut state) = queue.lock() else {
            return;
        };
        for id in ids.iter().take(PALETTE_PREWARM_QUEUE) {
            if state.ids.len() >= PALETTE_PREWARM_QUEUE {
                break;
            }
            if !state.active.contains(id) && !state.ids.contains(id) {
                state.ids.push_back(id.clone());
            }
        }
        if state.running || state.ids.is_empty() {
            false
        } else {
            state.running = true;
            true
        }
    };
    if start {
        tokio::spawn(async move { run_palette_prewarm().await });
    }
}

async fn run_palette_prewarm() {
    loop {
        let batch = {
            let queue = PALETTE_PREWARM.get_or_init(|| Mutex::new(PalettePrewarmQueue::default()));
            let Ok(mut state) = queue.lock() else {
                return;
            };
            let count = state.ids.len().min(PALETTE_PREWARM_BATCH);
            let batch = state.ids.drain(..count).collect::<Vec<_>>();
            if batch.is_empty() {
                state.running = false;
                return;
            }
            state.active.extend(batch.iter().cloned());
            batch
        };
        if let Ok(engine) = std::env::current_exe() {
            let engine = engine.to_string_lossy().into_owned();
            let mut owned = vec![
                "-n".to_owned(),
                "10".to_owned(),
                "--".to_owned(),
                engine,
                "wallpaper-media".to_owned(),
                "prewarm".to_owned(),
                "--catalogue-ids".to_owned(),
            ];
            owned.extend(batch.iter().cloned());
            let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
            if super::behaviours::helper(
                Path::new("nice"),
                &args,
                64 * 1024,
                std::time::Duration::from_secs(120),
            )
            .await
            .is_none_or(|(success, _)| !success)
            {
                eprintln!("Wallpaper palette prewarm failed");
            }
        } else {
            eprintln!("Wallpaper palette prewarm could not resolve the helper executable");
        }
        if let Ok(mut state) = PALETTE_PREWARM
            .get_or_init(|| Mutex::new(PalettePrewarmQueue::default()))
            .lock()
        {
            for id in batch {
                state.active.remove(&id);
            }
        }
    }
}

thread_local! {
    static SELECTION_BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static ASYNC_SELECTION_BUSY: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

pub(super) struct SelectionGuard {
    refresh_readiness: Option<Shared>,
}
impl SelectionGuard {
    pub(super) fn acquire() -> std::result::Result<Self, Failure> {
        if ASYNC_SELECTION_BUSY.with(|busy| busy.get().is_some())
            || SELECTION_BUSY.with(|busy| busy.replace(true))
        {
            Err(failure(ErrorCode::Busy, "Wallpaper selection is busy", true).dispatch_state(false))
        } else {
            Ok(Self {
                refresh_readiness: None,
            })
        }
    }
    pub(super) fn acquire_tracked(hub: &Shared) -> std::result::Result<Self, Failure> {
        let mut selection = Self::acquire()?;
        selection.refresh_readiness = Some(hub.clone());
        refresh_previous_availability(hub)
            .map_err(|_| failure(ErrorCode::Unavailable, "Wallpaper is unavailable", true))?;
        Ok(selection)
    }
    pub(super) fn is_busy() -> bool {
        SELECTION_BUSY.with(|busy| busy.get())
            || ASYNC_SELECTION_BUSY.with(|busy| busy.get().is_some())
    }
}
struct AsyncSelectionGuard {
    generation: u64,
    hub: Shared,
}
impl Drop for AsyncSelectionGuard {
    fn drop(&mut self) {
        if SELECTION_GENERATION.load(Ordering::Acquire) == self.generation {
            ASYNC_SELECTION_BUSY.with(|busy| {
                if busy.get() == Some(self.generation) {
                    busy.set(None);
                }
            });
            if let Err(error) = refresh_previous_availability(&self.hub) {
                eprintln!("Wallpaper selection availability refresh failed: {error}");
            }
        }
    }
}
impl Drop for SelectionGuard {
    fn drop(&mut self) {
        SELECTION_BUSY.with(|busy| busy.set(false));
        if let Some(hub) = &self.refresh_readiness {
            let _ = refresh_previous_availability(hub);
        }
    }
}
const RECORD_BYTES: u64 = 128 * 1024;
const IMAGE_BYTES: u64 = nacre_protocol::wallpaper::POSTER_BYTES;
/// Videos are large; the identity carries the exact size, so only sanity-bound it.
const MOTION_BYTES: u64 = 64 * 1024 * 1024 * 1024;

fn catalogue_helper_args(engine: &str, ids: Option<&[String]>) -> Vec<String> {
    let mut args = vec![
        "-n".to_owned(),
        "10".to_owned(),
        "--".to_owned(),
        engine.to_owned(),
        "wallpaper-media".to_owned(),
        "catalog".to_owned(),
    ];
    if let Some(ids) = ids {
        args.push("--catalogue-ids".to_owned());
        args.extend(ids.iter().cloned());
    }
    args
}

fn catalogue_entry(
    row: &Value,
    current: Option<&ActiveWallpaper>,
    cache: &Path,
    requested_ids: Option<&[String]>,
) -> Option<nacre_protocol::wallpaper::WallpaperCatalogueEntry> {
    use nacre_protocol::wallpaper::{WallpaperCatalogueEntry, WallpaperKind};
    let id = row["path"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 4096)?;
    let title = row["name"].as_str().unwrap_or("");
    let poster = row["poster"].as_str().unwrap_or("");
    let thumb = row["thumbnail"].as_str().unwrap_or("");
    let preview_source = row["preview"].as_str().unwrap_or("");
    let thumbnail = std::fs::canonicalize(thumb)
        .ok()
        .filter(|path| {
            path.starts_with(cache)
                && std::fs::metadata(path)
                    .is_ok_and(|meta| meta.is_file() && meta.len() <= 256 * 1024)
        })
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let preview = std::fs::canonicalize(preview_source)
        .ok()
        .filter(|path| {
            path.starts_with(cache)
                && std::fs::metadata(path)
                    .is_ok_and(|meta| meta.is_file() && meta.len() <= 2 * 1024 * 1024)
        })
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dynamic = row["dynamic"] == true;
    let animated = row["animated"] == true;
    let kind = if dynamic && !animated {
        WallpaperKind::Video
    } else {
        WallpaperKind::Image
    };
    let active = current.is_some_and(|wallpaper| {
        wallpaper.path == poster
            || wallpaper
                .motion
                .as_ref()
                .is_some_and(|motion| motion.path == id)
    });
    let video_predecode = requested_ids
        .filter(|ids| ids.iter().any(|requested| requested.as_str() == id))
        .filter(|_| kind == WallpaperKind::Video)
        .and_then(|_| video_predecode(id, poster, cache))
        .map(Box::new);
    let mut bounded_title = String::new();
    for character in title.chars() {
        if bounded_title.len() + character.len_utf8() > 512 {
            break;
        }
        bounded_title.push(character);
    }
    Some(WallpaperCatalogueEntry {
        id: id.into(),
        kind,
        animated,
        title: bounded_title,
        thumbnail,
        preview,
        active,
        palette: serde_json::from_value(row["palette"].clone()).ok(),
        video_predecode,
    })
}

fn video_predecode(
    source_path: &str,
    poster_path: &str,
    cache: &Path,
) -> Option<nacre_protocol::wallpaper::WallpaperVideoPredecode> {
    if source_path == poster_path || !Path::new(poster_path).starts_with(cache) {
        return None;
    }
    let source = private_file(Path::new(source_path), MOTION_BYTES).ok()?;
    let poster = private_file(Path::new(poster_path), IMAGE_BYTES).ok()?;
    let source_identity = WallpaperFileIdentity::from_metadata(&source.metadata().ok()?);
    let poster_identity = WallpaperFileIdentity::from_metadata(&poster.metadata().ok()?);
    if source_identity.validate().is_err() || poster_identity.validate().is_err() {
        return None;
    }
    let mut signature = [0; 8];
    if poster.read_at(&mut signature, 0).ok() != Some(signature.len())
        || signature != *b"\x89PNG\r\n\x1a\n"
        || !source_identity.matches(&source.metadata().ok()?)
        || !poster_identity.matches(&poster.metadata().ok()?)
    {
        return None;
    }
    Some(nacre_protocol::wallpaper::WallpaperVideoPredecode {
        poster_path: poster_path.into(),
        poster_file_identity: poster_identity,
        source_file_identity: source_identity,
    })
}

/// Resolve through directory descriptors, refusing symlinks at every component.
/// Only user-owned regular files below safe ancestors are eligible for disclosure.
pub(super) fn private_file(path: &Path, limit: u64) -> Result<File> {
    use rustix::fs::{open, openat, Mode, OFlags};
    let parts: Vec<_> = path
        .components()
        .filter(|p| !matches!(p, Component::RootDir))
        .collect();
    if !path.is_absolute()
        || parts.is_empty()
        || parts.iter().any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("Invalid private wallpaper path".into());
    }
    let mut directory = open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let uid = nix::unistd::getuid().as_raw();
    let check = |file: &File, regular: bool| -> Result<()> {
        let metadata = file.metadata()?;
        let root_sticky_directory =
            !regular && metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
        if (metadata.uid() != uid && (regular || metadata.uid() != 0))
            || (metadata.mode() & 0o022 != 0 && !root_sticky_directory)
            || (regular && (!metadata.is_file() || metadata.len() > limit))
        {
            return Err("Unsafe private wallpaper file".into());
        }
        Ok(())
    };
    check(&File::from(directory.try_clone()?), false)?;
    for (index, part) in parts.iter().enumerate() {
        let Component::Normal(part) = part else {
            unreachable!()
        };
        let last = index + 1 == parts.len();
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | OFlags::NONBLOCK
            | if last {
                OFlags::empty()
            } else {
                OFlags::DIRECTORY
            };
        let file = File::from(openat(&directory, *part, flags, Mode::empty())?);
        check(&file, last)?;
        if last {
            return Ok(file);
        }
        directory = file.into();
    }
    Err("Missing private wallpaper file".into())
}

/// The playable source of a video or animated-GIF wallpaper, taken from the private
/// selection record only when it still describes the published poster. Any problem
/// simply leaves the still wallpaper without `motion`.
enum MotionRead {
    None,
    Pending,
    Active(WallpaperMotion),
}

fn read_motion(paths: &Paths, poster: &str) -> MotionRead {
    let Ok(file) = private_file(&paths.state.join("wallpaper/media.json"), RECORD_BYTES) else {
        return MotionRead::None;
    };
    let mut bytes = Vec::new();
    if file.take(RECORD_BYTES + 1).read_to_end(&mut bytes).is_err() {
        return MotionRead::None;
    }
    if bytes.len() as u64 > RECORD_BYTES {
        return MotionRead::None;
    }
    let Ok(media) = serde_json::from_slice::<Value>(&bytes) else {
        return MotionRead::None;
    };
    let (Some(media_poster), Some(path), Some(dynamic), Some(animated)) = (
        media["poster"].as_str(),
        media["path"].as_str(),
        media["dynamic"].as_bool(),
        media["animated"].as_bool(),
    ) else {
        return MotionRead::None;
    };
    if !dynamic && (animated || path != media_poster)
        || dynamic && (path == media_poster || !media_poster.ends_with(".png"))
    {
        return MotionRead::None;
    }
    let Ok(poster_file) = private_file(Path::new(media_poster), IMAGE_BYTES) else {
        return MotionRead::None;
    };
    let Ok(source_file) = private_file(
        Path::new(path),
        if dynamic { MOTION_BYTES } else { IMAGE_BYTES },
    ) else {
        return MotionRead::None;
    };
    let Ok(poster_meta) = poster_file.metadata() else {
        return MotionRead::None;
    };
    let Ok(source_meta) = source_file.metadata() else {
        return MotionRead::None;
    };
    let poster_identity = WallpaperFileIdentity::from_metadata(&poster_meta);
    let source_identity = WallpaperFileIdentity::from_metadata(&source_meta);
    if poster_identity.validate().is_err() || source_identity.validate().is_err() {
        return MotionRead::None;
    }
    if dynamic {
        let mut signature = [0; 8];
        let signature_read = poster_file.read_at(&mut signature, 0).ok();
        let (Ok(poster_after), Ok(source_after)) = (poster_file.metadata(), source_file.metadata())
        else {
            return MotionRead::None;
        };
        if signature_read != Some(signature.len())
            || signature != *b"\x89PNG\r\n\x1a\n"
            || !poster_identity.matches(&poster_after)
            || !source_identity.matches(&source_after)
        {
            return MotionRead::None;
        }
    }
    if media_poster != poster {
        return MotionRead::Pending;
    }
    if !dynamic {
        return MotionRead::None;
    }
    let motion = WallpaperMotion {
        kind: if animated {
            MotionKind::Animation
        } else {
            MotionKind::Video
        },
        path: path.into(),
        file_identity: source_identity,
    };
    if motion.is_valid() {
        MotionRead::Active(motion)
    } else {
        MotionRead::None
    }
}

#[cfg(test)]
fn read_pair(
    paths: &Paths,
    watched: &mut Option<std::path::PathBuf>,
) -> Result<(ActiveWallpaper, String)> {
    read_pair_with_previous(paths, watched, None)
}

fn read_pair_with_previous(
    paths: &Paths,
    watched: &mut Option<std::path::PathBuf>,
    previous: Option<&ActiveWallpaper>,
) -> Result<(ActiveWallpaper, String)> {
    let file = private_file(&paths.state.join("presentation.json"), RECORD_BYTES)?;
    let mut bytes = Vec::new();
    file.take(RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > RECORD_BYTES {
        return Err("Oversized wallpaper presentation".into());
    }
    let record: Value = serde_json::from_slice(&bytes)?;
    let path = record["poster"]
        .as_str()
        .ok_or("Missing wallpaper poster")?;
    if path.len() > 4096
        || !Path::new(path).is_absolute()
        || path.contains('\0')
        || path.contains("://")
        || Path::new(path)
            .components()
            .any(|p| matches!(p, Component::ParentDir))
    {
        return Err("Invalid wallpaper poster".into());
    }
    *watched = Some(path.into());
    let image = private_file(Path::new(path), IMAGE_BYTES)?;
    let metadata = image.metadata()?;
    let identity = WallpaperFileIdentity::from_metadata(&metadata);
    let reader = image::ImageReader::new(std::io::BufReader::new(image.try_clone()?))
        .with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
                | image::ImageFormat::Gif
                | image::ImageFormat::Tiff
        )
    ) {
        return Err("Unsupported static wallpaper".into());
    }
    let (width, height) = reader.into_dimensions()?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 100_000_000 {
        return Err("Wallpaper dimensions exceed limit".into());
    }
    if identity != WallpaperFileIdentity::from_metadata(&image.metadata()?) {
        return Err("Wallpaper changed while reading".into());
    }
    let colours: BTreeMap<String, String> = record["colours"]
        .as_object()
        .ok_or("Missing palette")?
        .iter()
        .map(|(key, value)| {
            let text = value.as_str().ok_or("Invalid palette colour")?;
            Ok((
                key.clone(),
                format!(
                    "#{}",
                    text.strip_prefix('#').unwrap_or(text).to_ascii_lowercase()
                ),
            ))
        })
        .collect::<Result<_>>()?;
    // Same required roles as the QML source of truth, embedded at compile time.
    let reference: Value = serde_json::from_str(include_str!(
        "../../../../nacre/shell-tools/reference-style.json"
    ))?;
    if reference["colours"]
        .as_object()
        .ok_or("Missing reference palette")?
        .keys()
        .any(|key| !colours.contains_key(key))
    {
        return Err("Incomplete wallpaper palette".into());
    }
    let mut pair = ActiveWallpaper {
        motion: None,
        revision: 1,
        palette_revision: 1,
        path: path.into(),
        file_identity: identity,
        presentation: Presentation::Crop,
        palette: WallpaperPalette {
            mode: serde_json::from_value(record["mode"].clone())?,
            colours,
        },
    };
    match read_motion(paths, path) {
        MotionRead::Active(motion) => pair.motion = Some(motion),
        MotionRead::Pending => pair.motion = retain_previous_motion(previous, &pair),
        MotionRead::None => {}
    }
    pair.validate()?;
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&pair)?));
    Ok((pair, fingerprint))
}

fn retain_previous_motion(
    previous: Option<&ActiveWallpaper>,
    current: &ActiveWallpaper,
) -> Option<WallpaperMotion> {
    let previous = previous?;
    if previous.path != current.path
        || previous.file_identity != current.file_identity
        || previous.palette != current.palette
        || previous.presentation != current.presentation
    {
        return None;
    }
    let motion = previous.motion.as_ref()?;
    let source = private_file(Path::new(&motion.path), MOTION_BYTES).ok()?;
    motion
        .file_identity
        .matches(&source.metadata().ok()?)
        .then(|| motion.clone())
}

/// Return QML's `actualCurrent` source path rather than the daemon topic's still poster path.
fn selected_path(paths: &Paths, pair: &ActiveWallpaper) -> Option<String> {
    let media_file = private_file(&paths.state.join("wallpaper/media.json"), RECORD_BYTES).ok();
    let selected = if let Some(file) = media_file {
        let mut bytes = Vec::new();
        file.take(RECORD_BYTES + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > RECORD_BYTES {
            return None;
        }
        let media: Value = serde_json::from_slice(&bytes).ok()?;
        if media["poster"].as_str() != Some(pair.path.as_str()) {
            // The selection record can lead presentation.json during a commit. Do not
            // treat that new selection as history until its poster is actually active.
            return None;
        }
        media["path"].as_str().map(str::to_owned)
    } else {
        private_file(&paths.state.join("wallpaper/last.txt"), 4097)
            .ok()
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(4097).read_to_end(&mut bytes).ok()?;
                if bytes.len() > 4096 {
                    return None;
                }
                String::from_utf8(bytes)
                    .ok()
                    .map(|path| path.trim().to_owned())
            })
    };
    selected
        .or_else(|| pair.motion.as_ref().map(|motion| motion.path.clone()))
        .or_else(|| Some(pair.path.clone()))
        .filter(|path| {
            !path.is_empty()
                && path.len() <= 4096
                && path.starts_with('/')
                && !path.contains('\0')
                && !path.contains("://")
                && !Path::new(path)
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
        })
}

pub fn initial(hub: &Shared) -> Result<()> {
    hub.borrow_mut().publish(
        TOPIC,
        serde_json::to_value(WallpaperActive {
            available: false,
            stale: false,
            previous_available: false,
            revision: 0,
            current: None,
            outputs: BTreeMap::new(),
            error: None,
        })?,
    )?;
    Ok(())
}

/// Update readiness from the existing behavior worker's configured wallpaper pool and
/// session watcher; history itself remains private to the Hub.
pub(super) fn set_rotation_readiness(hub: &Shared, pool_ready: bool, sleeping: bool) -> Result<()> {
    hub.borrow_mut()
        .wallpaper_history
        .set_readiness(pool_ready, sleeping);
    refresh_previous_availability(hub)
}

pub(super) fn update_pool_readiness(hub: &Shared, pool_ready: bool) -> Result<()> {
    let sleeping = hub.borrow().wallpaper_history.sleeping();
    set_rotation_readiness(hub, pool_ready, sleeping)
}

/// Recheck daemon-owned rotation gates after an async manual-selection lookup.
/// This intentionally ignores SelectionGuard's own busy bit; the caller owns it.
pub(super) fn selection_environment_ready(hub: &Shared) -> bool {
    let hub = hub.borrow();
    hub.settings.is_some()
        && hub.wallpaper_history.pool_ready()
        && !hub.wallpaper_history.sleeping()
}

pub(super) fn refresh_previous_availability(hub: &Shared) -> Result<()> {
    let Some(data) = hub.borrow().snapshot(TOPIC) else {
        return Ok(());
    };
    let mut state: WallpaperActive = serde_json::from_value(data)?;
    let (pool_ready, sleeping, has_settings, committed_source) = {
        let hub = hub.borrow();
        (
            hub.wallpaper_history.pool_ready(),
            hub.wallpaper_history.sleeping(),
            hub.settings.is_some(),
            hub.wallpaper_history.current().is_some() && hub.wallpaper_history.selection_stable(),
        )
    };
    state.previous_available = previous_available(
        &state,
        has_settings,
        committed_source,
        pool_ready,
        sleeping,
        SelectionGuard::is_busy(),
    );
    state.validate()?;
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

fn previous_available(
    state: &WallpaperActive,
    has_settings: bool,
    committed_source: bool,
    pool_ready: bool,
    sleeping: bool,
    busy: bool,
) -> bool {
    state.available
        && !state.stale
        && state.current.is_some()
        && has_settings
        && committed_source
        && pool_ready
        && !sleeping
        && !busy
}

pub(super) fn previous_target(hub: &Shared, pool: &[String], current: &str) -> Option<String> {
    hub.borrow_mut().wallpaper_history.previous(pool, current)
}

pub(super) fn previous_failed(hub: &Shared, target: &str) {
    hub.borrow_mut().wallpaper_history.selection_failed(target);
}

pub(super) fn committed_source(hub: &Shared) -> Option<String> {
    let hub = hub.borrow();
    hub.wallpaper_history
        .selection_stable()
        .then(|| hub.wallpaper_history.current().map(str::to_owned))
        .flatten()
}

pub(super) fn mark_source_unstable(hub: &Shared) -> Result<()> {
    hub.borrow_mut()
        .wallpaper_history
        .set_selection_stable(false);
    refresh_previous_availability(hub)
}

#[derive(Default)]
pub struct Producer {
    fingerprint: Option<String>,
    pub watched: Option<std::path::PathBuf>,
}
impl Producer {
    pub fn refresh(&mut self, hub: &Shared, paths: &Paths) -> Result<()> {
        let previous = hub
            .borrow()
            .snapshot(TOPIC)
            .ok_or("Missing wallpaper topic")?;
        let mut state: WallpaperActive = serde_json::from_value(previous)?;
        let pair = read_pair_with_previous(paths, &mut self.watched, state.current.as_ref());
        let compositor = hub.borrow().snapshot("compositor");
        let outputs = compositor
            .as_ref()
            .filter(|v| v["available"] == true)
            .and_then(|v| v["outputs"].as_object())
            .filter(|outputs| {
                outputs.len() <= 32
                    && outputs
                        .keys()
                        .all(|key| !key.is_empty() && key.len() <= 256)
            });
        let Some(outputs) = outputs else {
            state.available = false;
            state.stale = state.current.is_some();
            state.previous_available = false;
            hub.borrow_mut()
                .wallpaper_history
                .set_selection_stable(false);
            state.error = Some("Wallpaper outputs are unavailable".into());
            state.validate()?;
            hub.borrow_mut()
                .publish(TOPIC, serde_json::to_value(state)?)?;
            return Ok(());
        };
        match pair {
            Ok((mut pair, fingerprint)) => {
                if self.fingerprint.as_ref() != Some(&fingerprint) {
                    let previous_pair = state.current.clone().map(|mut current| {
                        current.revision = 1;
                        current.palette_revision = 1;
                        current
                    });
                    if previous_pair.as_ref() != Some(&pair) {
                        state.revision = state
                            .revision
                            .checked_add(1)
                            .filter(|v| *v <= MAX_SAFE_INTEGER)
                            .ok_or("Wallpaper revision exhausted")?;
                    }
                    self.fingerprint = Some(fingerprint);
                }
                let selected = selected_path(paths, &pair);
                pair.revision = state.revision;
                pair.palette_revision = state.revision;
                state.current = Some(pair);
                if let Some(selected) = selected.as_deref() {
                    hub.borrow_mut().wallpaper_history.committed(Some(selected));
                    hub.borrow_mut()
                        .wallpaper_history
                        .set_selection_stable(true);
                } else {
                    hub.borrow_mut()
                        .wallpaper_history
                        .set_selection_stable(false);
                }
                state.available = true;
                state.stale = false;
                state.error = None;
            }
            Err(_) => {
                state.available = false;
                state.stale = state.current.is_some();
                state.previous_available = false;
                hub.borrow_mut()
                    .wallpaper_history
                    .set_selection_stable(false);
                state.error = Some("Wallpaper presentation is unavailable".into());
            }
        }
        state.outputs.clear();
        if let Some(pair) = &state.current {
            state
                .outputs
                .extend(outputs.keys().map(|key| (key.clone(), pair.clone())));
        }
        let (pool_ready, sleeping, has_settings, committed_source) = {
            let hub = hub.borrow();
            (
                hub.wallpaper_history.pool_ready(),
                hub.wallpaper_history.sleeping(),
                hub.settings.is_some(),
                hub.wallpaper_history.current().is_some()
                    && hub.wallpaper_history.selection_stable(),
            )
        };
        state.previous_available = previous_available(
            &state,
            has_settings,
            committed_source,
            pool_ready,
            sleeping,
            SelectionGuard::is_busy(),
        );
        state.validate()?;
        hub.borrow_mut()
            .publish(TOPIC, serde_json::to_value(state)?)?;
        Ok(())
    }
}

pub fn unavailable(hub: &Shared) -> Result<()> {
    let mut state: WallpaperActive = serde_json::from_value(
        hub.borrow()
            .snapshot(TOPIC)
            .ok_or("Missing wallpaper topic")?,
    )?;
    state.available = false;
    state.stale = state.current.is_some();
    state.previous_available = false;
    hub.borrow_mut()
        .wallpaper_history
        .set_selection_stable(false);
    state.error = Some("Wallpaper monitoring is unavailable".into());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

pub async fn outputs(
    hub: Shared,
    paths: Paths,
    producer: std::rc::Rc<std::cell::RefCell<Producer>>,
) -> Result<()> {
    let mut events = hub.borrow().observe();
    loop {
        match events.recv().await {
            Ok(Notice::Changed { topic, .. }) if topic == "compositor" => {
                producer.borrow_mut().refresh(&hub, &paths)?
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                producer.borrow_mut().refresh(&hub, &paths)?
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err("Wallpaper observer closed".into())
            }
            _ => {}
        }
    }
}

pub fn methods() -> BTreeMap<String, Method> {
    BTreeMap::from([
        (
            "wallpaper.activate".into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 30_000,
                params: serde_json::from_str(include_str!(
                    "../../../../protocol/schema/wallpaper-activate.json"
                ))
                .expect("wallpaper request schema"),
                result: serde_json::from_str(include_str!(
                    "../../../../protocol/schema/wallpaper-active-image.json"
                ))
                .expect("wallpaper result schema"),
            },
        ),
        (
            "wallpaper.catalogue".into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 30_000,
                params: serde_json::from_str(include_str!(
                    "../../../../protocol/schema/wallpaper-catalogue-request.json"
                ))
                .expect("wallpaper catalogue request schema"),
                result: serde_json::from_str(include_str!(
                    "../../../../protocol/schema/wallpaper-catalogue.json"
                ))
                .expect("wallpaper catalogue schema"),
            },
        ),
    ])
}

fn activation_state(
    hub: &Shared,
    params: Value,
) -> std::result::Result<(WallpaperActivate, WallpaperActive), Failure> {
    let request: WallpaperActivate = serde_json::from_value(params)
        .map_err(|_| failure(ErrorCode::Validation, "Invalid wallpaper activation", false))?;
    request.validate()?;
    let state: WallpaperActive = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Wallpaper is unavailable", true))?;
    let selecting = request.id.is_some();
    if state.stale || (!state.available && !(selecting && state.current.is_none())) {
        return Err(failure(
            ErrorCode::Unavailable,
            "Wallpaper is unavailable",
            true,
        ));
    }
    if request.revision != state.revision {
        return Err(failure(
            ErrorCode::StaleRevision,
            "Wallpaper changed; refresh active state",
            true,
        ));
    }
    Ok((request, state))
}

#[cfg(test)]
fn select_media(engine: &Path, id: &str) -> bool {
    let Ok(output) = crate::process::run_bytes(
        "nice",
        &[
            "-n".into(),
            "10".into(),
            engine.to_string_lossy().into_owned(),
            "wallpaper-media".into(),
            "select".into(),
            id.into(),
        ],
        None,
        std::time::Duration::from_secs(15),
    ) else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|entry| entry["error"].is_null() && entry["path"] == id)
}

fn valid_catalogue_selection(path: &Path, library: &Path) -> bool {
    if !path.is_absolute() || path.as_os_str().len() > 4096 {
        return false;
    }
    let Some(suffix) = path.extension().and_then(|suffix| suffix.to_str()) else {
        return false;
    };
    let suffix = suffix.to_ascii_lowercase();
    let Some(canonical) = std::fs::canonicalize(path).ok() else {
        return false;
    };
    if canonical != path
        || !canonical.starts_with(library)
        || (!crate::wallpaper_media::IMAGES.contains(&suffix.as_str())
            && !crate::wallpaper_media::VIDEOS.contains(&suffix.as_str()))
    {
        return false;
    }
    let limit = if crate::wallpaper_media::VIDEOS.contains(&suffix.as_str()) {
        MOTION_BYTES
    } else {
        IMAGE_BYTES
    };
    private_file(&canonical, limit).is_ok_and(|file| {
        file.metadata().is_ok_and(|metadata| {
            let identity = WallpaperFileIdentity::from_metadata(&metadata);
            identity.validate().is_ok() && identity.matches(&metadata)
        })
    })
}

pub fn activate(hub: Shared, params: Value) -> std::result::Result<Value, Failure> {
    let (request, state) = activation_state(&hub, params)?;
    if request.id.is_some() {
        return Err(failure(
            ErrorCode::Validation,
            "Catalogue selection requires async dispatch",
            false,
        ));
    }
    Ok(serde_json::to_value(state.current).expect("finite wallpaper pair"))
}

pub async fn activate_request(hub: Shared, params: Value) -> std::result::Result<Value, Failure> {
    let (request, state) = activation_state(&hub, params)?;
    if let Some(id) = request.id {
        let paths = crate::paths::Paths::environment()
            .map_err(|_| failure(ErrorCode::Unavailable, "Wallpaper is unavailable", true))?;
        // Validate the requested library member directly. The hot path never
        // constructs a catalogue or walks the wallpaper tree.
        let media =
            crate::wallpaper_media::Media::new(paths.clone(), paths.assets()).map_err(|_| {
                failure(
                    ErrorCode::Unavailable,
                    "Wallpaper catalogue is unavailable",
                    true,
                )
            })?;
        let library = std::fs::canonicalize(&media.library).unwrap_or(media.library.clone());
        let known = valid_catalogue_selection(Path::new(&id), &library);
        if !known {
            return Err(failure(
                ErrorCode::Validation,
                "Unknown wallpaper catalogue id",
                false,
            ));
        }
        let latest: WallpaperActive = hub
            .borrow()
            .snapshot(TOPIC)
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or_else(|| failure(ErrorCode::Unavailable, "Wallpaper is unavailable", true))?;
        if latest.stale
            || (!latest.available && latest.current.is_some())
            || latest.revision != request.revision
        {
            return Err(failure(
                ErrorCode::StaleRevision,
                "Wallpaper changed; refresh active state",
                true,
            ));
        }
        let engine = std::env::current_exe()
            .map_err(|_| failure(ErrorCode::Unavailable, "Wallpaper is unavailable", true))?;
        let generation = SELECTION_GENERATION
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let generation_file = paths.state.join("wallpaper/current-selection-generation");
        // N0089: never take the publication flock here. This runs on the daemon's single event
        // loop, and a running `wallpaper-media select` child holds that lock while it waits
        // for this same loop (B0171 nested repro: every selection after the first stalled for
        // seconds with the daemon parked in flock). The rename in `atomic` is already atomic,
        // and the children re-check the generation under the lock before they publish.
        crate::files::atomic(&generation_file, generation.to_string().as_bytes()).map_err(
            |_| {
                failure(
                    ErrorCode::Unavailable,
                    "Wallpaper selection could not be queued",
                    true,
                )
            },
        )?;
        ASYNC_SELECTION_BUSY.with(|busy| busy.set(Some(generation)));
        if let Err(error) = refresh_previous_availability(&hub) {
            eprintln!("Wallpaper selection availability refresh failed: {error}");
        }
        SELECTION_TASK.with(|slot| {
            let previous = slot.borrow_mut().take();
            if let Some(previous) = &previous {
                previous.abort();
            }
            let worker_hub = hub.clone();
            let worker_paths = paths.clone();
            *slot.borrow_mut() = Some(tokio::task::spawn_local(async move {
                let _selection_guard = AsyncSelectionGuard {
                    generation,
                    hub: worker_hub.clone(),
                };
                if let Some(previous) = previous {
                    let _ = previous.await;
                }
                if SELECTION_GENERATION.load(Ordering::Acquire) != generation {
                    return;
                }
                use tokio::process::Command;
                let child = Command::new("nice")
                    .args(["-n", "10", "--"])
                    .arg(engine)
                    .args(["wallpaper-media", "select"])
                    .arg(&id)
                    .env("NACRE_WALLPAPER_GENERATION", generation.to_string())
                    .kill_on_drop(true)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .output()
                    .await;
                let selected = child.is_ok_and(|output| output.status.success());
                if selected
                    && SELECTION_GENERATION.load(Ordering::Acquire) == generation
                    && Producer::default()
                        .refresh(&worker_hub, &worker_paths)
                        .is_err()
                {
                    eprintln!("Wallpaper selection refresh failed");
                }
            }));
        });
        return Ok(serde_json::to_value(state.current).expect("finite wallpaper pair"));
    }
    Ok(serde_json::to_value(state.current).expect("finite wallpaper pair"))
}

pub async fn catalogue(
    hub: &Shared,
    params: Value,
) -> std::result::Result<nacre_protocol::wallpaper::WallpaperCatalogue, Failure> {
    use nacre_protocol::wallpaper::{WallpaperCatalogue, WallpaperCatalogueRequest};
    let request: WallpaperCatalogueRequest = serde_json::from_value(params).map_err(|_| {
        failure(
            ErrorCode::Validation,
            "Invalid wallpaper catalogue request",
            false,
        )
    })?;
    request.validate()?;
    let paths = crate::paths::Paths::environment().map_err(|_| {
        failure(
            ErrorCode::Unavailable,
            "Wallpaper catalogue is unavailable",
            true,
        )
    })?;
    let engine = std::env::current_exe().map_err(|_| {
        failure(
            ErrorCode::Unavailable,
            "Wallpaper catalogue is unavailable",
            true,
        )
    })?;
    let engine_arg = engine.to_string_lossy().into_owned();
    let engine = std::path::PathBuf::from("nice");
    let helper_args = catalogue_helper_args(&engine_arg, request.ids.as_deref());
    let helper_args = helper_args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = super::behaviours::helper(
        &engine,
        &helper_args,
        8 * 1024 * 1024,
        std::time::Duration::from_secs(12),
    )
    .await
    .ok_or_else(|| {
        failure(
            ErrorCode::Unavailable,
            "Wallpaper catalogue is unavailable",
            true,
        )
    })?;
    let (success, bytes) = output;
    if !success {
        return Err(failure(
            ErrorCode::Unavailable,
            "Wallpaper catalogue is unavailable",
            true,
        ));
    }
    let raw: Value = serde_json::from_slice(&bytes).map_err(|_| {
        failure(
            ErrorCode::Unavailable,
            "Wallpaper catalogue is unavailable",
            true,
        )
    })?;
    let rows = raw["entries"]
        .as_array()
        .filter(|rows| rows.len() <= 2048)
        .ok_or_else(|| {
            failure(
                ErrorCode::Unavailable,
                "Wallpaper catalogue is unavailable",
                true,
            )
        })?;
    if request.ids.as_ref().is_some_and(|ids| {
        ids.iter()
            .any(|id| !rows.iter().any(|row| row["path"].as_str() == Some(id)))
    }) {
        return Err(failure(
            ErrorCode::Validation,
            "Unknown wallpaper catalogue id",
            false,
        ));
    }
    let active: Option<WallpaperActive> = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|value| serde_json::from_value(value).ok());
    let current = active.and_then(|state| state.current);
    let cache = paths.cache.join("wallpaper-media");
    let cache = std::fs::canonicalize(&cache).unwrap_or(cache);
    let entries = rows
        .iter()
        .filter_map(|row| catalogue_entry(row, current.as_ref(), &cache, request.ids.as_deref()))
        .collect();
    let catalogue = WallpaperCatalogue { entries };
    catalogue.validate().map_err(|_| {
        failure(
            ErrorCode::Unavailable,
            "Wallpaper catalogue is invalid",
            true,
        )
    })?;
    if let Some(ids) = request.ids.as_deref() {
        enqueue_palette_prewarm(ids);
    }
    Ok(catalogue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        time::Duration,
    };

    fn fixture() -> Result<(tempfile::TempDir, Paths, Shared)> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.state)?;
        let image = paths.home.join("wall.png");
        image::DynamicImage::new_rgb8(2, 2).save(&image)?;
        let mut record: Value = serde_json::from_str(include_str!(
            "../../../../nacre/shell-tools/reference-style.json"
        ))?;
        record["poster"] = json!(image);
        record["mode"] = json!("dark");
        fs::write(
            paths.state.join("presentation.json"),
            serde_json::to_vec(&record)?,
        )?;
        let hub =
            super::super::hub::Hub::new("test".into(), false, Duration::from_millis(16)).shared();
        hub.borrow_mut().publish(
            "compositor",
            json!({"available":true,"outputs":{"DP-1":{},"DP-2":{}}}),
        )?;
        initial(&hub)?;
        Ok((home, paths, hub))
    }

    #[test]
    fn selection_invokes_existing_media_selector_and_requires_matching_source() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir()?;
        let engine = directory.path().join("fake-engine");
        fs::write(
            &engine,
            "#!/bin/sh\n[ \"$1\" = wallpaper-media ] && [ \"$2\" = select ] || exit 2\nprintf '{\"error\":null,\"path\":\"%s\"}\\n' \"$3\"\n",
        )?;
        fs::set_permissions(&engine, fs::Permissions::from_mode(0o700))?;
        assert!(select_media(&engine, "/pictures/lake.png"));
        fs::write(
            &engine,
            "#!/bin/sh\nprintf '{\"error\":null,\"path\":\"/pictures/other.png\"}\\n'\n",
        )?;
        assert!(!select_media(&engine, "/pictures/lake.png"));
        Ok(())
    }
    #[test]
    fn selection_action_admission_is_single_flight_and_releases_on_drop() {
        let guard = SelectionGuard::acquire().unwrap();
        assert!(SelectionGuard::acquire().is_err());
        assert!(SelectionGuard::is_busy());
        drop(guard);
        assert!(!SelectionGuard::is_busy());
        assert!(SelectionGuard::acquire().is_ok());
    }

    #[test]
    fn direct_selection_validation_checks_library_membership_and_file_identity() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let library = paths.home.join("Pictures/Wallpapers");
        fs::create_dir_all(&library)?;
        let image_path = library.join("still.png");
        image::DynamicImage::new_rgb8(4, 4).save(&image_path)?;
        let video_path = library.join("clip.mp4");
        fs::write(&video_path, b"metadata-only video fixture")?;
        let outside_path = paths.home.join("outside.png");
        image::DynamicImage::new_rgb8(4, 4).save(&outside_path)?;
        symlink(&outside_path, library.join("escape.png"))?;
        let canonical_library = library.canonicalize()?;
        assert!(valid_catalogue_selection(&image_path, &canonical_library));
        assert!(valid_catalogue_selection(&video_path, &canonical_library));
        assert!(!valid_catalogue_selection(
            &outside_path,
            &canonical_library
        ));
        assert!(!valid_catalogue_selection(
            &library.join("escape.png"),
            &canonical_library
        ));
        assert!(!valid_catalogue_selection(
            &library.join("missing.png"),
            &canonical_library
        ));
        assert!(!valid_catalogue_selection(
            Path::new("/../../etc/passwd"),
            &canonical_library
        ));
        assert!(WallpaperActivate {
            revision: 1,
            id: Some("/../../etc/passwd".into()),
        }
        .validate()
        .is_err());
        Ok(())
    }

    #[test]
    fn previous_availability_requires_live_multi_item_ready_state() -> Result<()> {
        let (_home, paths, hub) = fixture()?;
        let mut producer = Producer::default();
        producer.refresh(&hub, &paths)?;
        let state: WallpaperActive = serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap())?;
        assert!(previous_available(&state, true, true, true, false, false));
        assert!(
            !previous_available(&state, true, true, false, false, false),
            "singleton pool"
        );
        assert!(
            !previous_available(&state, true, false, true, false, false),
            "source not committed"
        );
        assert!(
            !previous_available(&state, true, true, true, true, false),
            "locked or sleeping"
        );
        assert!(
            !previous_available(&state, true, true, true, false, true),
            "selection busy"
        );
        assert!(
            !previous_available(&state, false, true, true, false, false),
            "settings disconnected"
        );
        let mut unavailable_state = state;
        unavailable_state.available = false;
        unavailable_state.stale = true;
        assert!(!previous_available(
            &unavailable_state,
            true,
            true,
            true,
            false,
            false
        ));
        Ok(())
    }

    #[test]
    fn catalogue_helper_args_preserve_legacy_and_targeted_hydration_modes() {
        let base = vec![
            "-n".to_owned(),
            "10".to_owned(),
            "--".to_owned(),
            "/bin/nacre-helper".to_owned(),
            "wallpaper-media".to_owned(),
            "catalog".to_owned(),
        ];
        assert_eq!(catalogue_helper_args("/bin/nacre-helper", None), base);
        let ids: Vec<String> = vec![];
        let mut metadata = base.clone();
        metadata.push("--catalogue-ids".to_owned());
        assert_eq!(
            catalogue_helper_args("/bin/nacre-helper", Some(&ids)),
            metadata
        );
        let ids = vec!["/pictures/a.png".to_owned(), "/videos/b.mp4".to_owned()];
        let mut selected = metadata;
        selected.extend(ids);
        assert_eq!(
            catalogue_helper_args(
                "/bin/nacre-helper",
                Some(&["/pictures/a.png".to_owned(), "/videos/b.mp4".to_owned(),])
            ),
            selected
        );
    }

    #[test]
    fn catalogue_keeps_animated_images_in_image_kind_and_marks_active_source() {
        use nacre_protocol::wallpaper::WallpaperKind;
        let row = json!({
            "path":"/pictures/animated.gif",
            "poster":"/cache/wallpaper-media/animated.png",
            "name":"Animated",
            "dynamic":true,
            "animated":true,
            "thumbnail":"",
            "preview":""
        });
        let current: ActiveWallpaper = serde_json::from_value(json!({
            "revision":1,
            "palette_revision":1,
            "path":"/cache/wallpaper-media/animated.png",
            "file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
            "presentation":"crop",
            "palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}},
            "motion":{"kind":"animation","path":"/pictures/animated.gif","file_identity":{"device":"1","inode":"3","size":5,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0}}
        }))
        .unwrap();
        let entry = catalogue_entry(&row, Some(&current), Path::new("/nonexistent"), None)
            .expect("valid catalogue row");
        assert_eq!(entry.kind, WallpaperKind::Image);
        assert!(entry.animated);
        assert!(entry.active);
        assert!(entry.thumbnail.is_empty());
        let video = catalogue_entry(
            &json!({"path":"/videos/clip.mp4","poster":"/cache/clip.png","name":"Clip","dynamic":true,"animated":false,"thumbnail":"","preview":""}),
            None,
            Path::new("/nonexistent"),
            None,
        )
        .expect("valid video row");
        assert_eq!(video.kind, WallpaperKind::Video);
        assert!(!video.animated);
        assert!(video.video_predecode.is_none());
    }

    #[test]
    fn requested_video_catalogue_includes_only_a_secure_cached_poster() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let library = home.path().join("Videos");
        let cache = paths.cache.join("wallpaper-media");
        fs::create_dir_all(&library)?;
        fs::create_dir_all(&cache)?;
        let source = library.join("clip.mp4");
        fs::write(&source, b"video source")?;
        let poster = cache.join("clip.png");
        image::DynamicImage::new_rgb8(4, 2).save(&poster)?;
        let id = source.to_string_lossy().into_owned();
        let poster_path = poster.to_string_lossy().into_owned();
        let row = json!({
            "path":id,
            "poster":poster_path,
            "name":"Clip",
            "dynamic":true,
            "animated":false,
            "thumbnail":"",
            "preview":""
        });
        let cache = fs::canonicalize(cache)?;
        let metadata_only = catalogue_entry(&row, None, &cache, Some(&[])).unwrap();
        assert!(metadata_only.video_predecode.is_none());
        let other_ids = vec![home
            .path()
            .join("Videos/other.mp4")
            .to_string_lossy()
            .into_owned()];
        let unrequested = catalogue_entry(&row, None, &cache, Some(&other_ids)).unwrap();
        assert!(unrequested.video_predecode.is_none());
        let requested_ids = vec![id];
        let requested = catalogue_entry(&row, None, &cache, Some(&requested_ids)).unwrap();
        let predecode = requested.video_predecode.unwrap();
        assert_eq!(predecode.poster_path, poster_path);
        assert!(predecode
            .poster_file_identity
            .matches(&fs::metadata(&poster)?));
        assert!(predecode
            .source_file_identity
            .matches(&fs::metadata(&source)?));
        Ok(())
    }

    #[test]
    fn revisions_pair_palette_output_hotplug_stale_recovery_and_activation() -> Result<()> {
        let (_home, paths, hub) = fixture()?;
        let mut producer = Producer::default();
        producer.refresh(&hub, &paths)?;
        let first = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(first["outputs"].as_object().unwrap().len(), 2);
        assert_eq!(first["current"]["presentation"], "crop");
        assert_eq!(first["revision"], 1);
        assert_eq!(
            activate(hub.clone(), json!({"revision":1}))?["palette_revision"],
            1
        );
        producer.refresh(&hub, &paths)?;
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["revision"], 1);
        let file = paths.state.join("presentation.json");
        let valid = fs::read(&file)?;
        fs::write(&file, "bad JSON")?;
        producer.refresh(&hub, &paths)?;
        let stale = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(stale["current"], first["current"]);
        assert_eq!(stale["stale"], true);
        assert_eq!(
            activate(hub.clone(), json!({"revision":1}))
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
        fs::write(&file, &valid)?;
        producer.refresh(&hub, &paths)?;
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["revision"], 1);
        let mut changed: Value = serde_json::from_slice(&valid)?;
        changed["colours"]["background"] = json!("123456");
        fs::write(&file, serde_json::to_vec(&changed)?)?;
        producer.refresh(&hub, &paths)?;
        assert_eq!(
            activate(hub.clone(), json!({"revision":1}))
                .unwrap_err()
                .code,
            ErrorCode::StaleRevision
        );
        assert_eq!(
            activate(hub.clone(), json!({"revision":2}))?["palette"]["colours"]["background"],
            "#123456"
        );
        let last_assignments = hub.borrow().snapshot(TOPIC).unwrap()["outputs"].clone();
        hub.borrow_mut()
            .publish("compositor", json!({"available":false,"outputs":{}}))?;
        producer.refresh(&hub, &paths)?;
        let disconnected = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(disconnected["outputs"], last_assignments);
        assert_eq!(disconnected["available"], false);
        assert_eq!(disconnected["stale"], true);
        hub.borrow_mut()
            .publish("compositor", json!({"available":true,"outputs":{}}))?;
        producer.refresh(&hub, &paths)?;
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["outputs"], json!({}));
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["revision"], 2);
        unavailable(&hub)?;
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["stale"], true);
        Producer::default().refresh(&hub, &paths)?;
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["revision"], 2);
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["stale"], false);
        Ok(())
    }

    #[test]
    fn history_commits_only_after_the_selected_poster_is_published() -> Result<()> {
        let (_home, paths, hub) = fixture()?;
        let wallpaper_dir = paths.state.join("wallpaper");
        fs::create_dir_all(&wallpaper_dir)?;
        fs::write(wallpaper_dir.join("last.txt"), "/pictures/first.png")?;
        let mut producer = Producer::default();
        producer.refresh(&hub, &paths)?;
        assert_eq!(
            hub.borrow().wallpaper_history.current(),
            Some("/pictures/first.png")
        );
        assert!(hub.borrow().wallpaper_history.entries().is_empty());

        let second = paths.home.join("second.png");
        image::DynamicImage::new_rgb8(3, 3).save(&second)?;
        let mut presentation: Value =
            serde_json::from_slice(&fs::read(paths.state.join("presentation.json"))?)?;
        fs::write(
            wallpaper_dir.join("media.json"),
            json!({"path":"/pictures/second.png","poster":second,"dynamic":false}).to_string(),
        )?;
        producer.refresh(&hub, &paths)?;
        assert_eq!(
            hub.borrow().wallpaper_history.current(),
            Some("/pictures/first.png"),
            "a newer media record cannot commit before its poster is presented"
        );
        assert!(hub.borrow().wallpaper_history.entries().is_empty());
        assert!(!hub.borrow().wallpaper_history.selection_stable());

        presentation["poster"] = json!(second);
        fs::write(
            paths.state.join("presentation.json"),
            serde_json::to_vec(&presentation)?,
        )?;
        producer.refresh(&hub, &paths)?;
        assert_eq!(
            hub.borrow().wallpaper_history.current(),
            Some("/pictures/second.png")
        );
        assert!(hub.borrow().wallpaper_history.selection_stable());
        assert_eq!(
            hub.borrow()
                .wallpaper_history
                .entries()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["/pictures/first.png"]
        );

        presentation["poster"] = json!(paths.home.join("missing.png"));
        fs::write(
            paths.state.join("presentation.json"),
            serde_json::to_vec(&presentation)?,
        )?;
        fs::write(wallpaper_dir.join("last.txt"), "/pictures/failed.png")?;
        assert!(producer.refresh(&hub, &paths).is_ok());
        assert_eq!(
            hub.borrow().wallpaper_history.current(),
            Some("/pictures/second.png")
        );
        assert_eq!(
            hub.borrow()
                .wallpaper_history
                .entries()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["/pictures/first.png"]
        );
        Ok(())
    }

    #[test]
    fn missing_optional_source_records_validated_static_presentation() -> Result<()> {
        let (_home, paths, hub) = fixture()?;
        assert!(!paths.state.join("wallpaper/media.json").exists());
        assert!(!paths.state.join("wallpaper/last.txt").exists());

        Producer::default().refresh(&hub, &paths)?;

        let pair = hub.borrow().snapshot(TOPIC).unwrap();
        let source = pair["current"]["path"].as_str().unwrap();
        assert_eq!(
            hub.borrow().wallpaper_history.current(),
            Some(source),
            "a validated static presentation remains the source fallback"
        );
        assert!(hub.borrow().wallpaper_history.selection_stable());
        Ok(())
    }

    #[test]
    fn private_paths_reject_symlinks_escapes_permissions_special_files_and_limits() -> Result<()> {
        let (home, paths, _hub) = fixture()?;
        let image = home.path().join("wall.png");
        assert!(private_file(&image, IMAGE_BYTES).is_ok());
        let alias = home.path().join("alias.png");
        symlink(&image, &alias)?;
        assert!(private_file(&alias, IMAGE_BYTES).is_err());
        let escape = home.path().join("escape");
        symlink("/etc", &escape)?;
        assert!(private_file(&escape.join("passwd"), IMAGE_BYTES).is_err());
        assert!(private_file(&home.path().join("../wall.png"), IMAGE_BYTES).is_err());
        assert!(private_file(&image, 1).is_err());
        fs::set_permissions(&image, fs::Permissions::from_mode(0o666))?;
        assert!(private_file(&image, IMAGE_BYTES).is_err());
        fs::set_permissions(&image, fs::Permissions::from_mode(0o600))?;
        if nix::unistd::getuid().is_root() {
            nix::unistd::chown(&image, Some(nix::unistd::Uid::from_raw(1000)), None)?;
            assert!(private_file(&image, IMAGE_BYTES).is_err());
            nix::unistd::chown(&image, Some(nix::unistd::getuid()), None)?;
        }
        let fifo = home.path().join("fifo");
        nix::unistd::mkfifo(
            &fifo,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )?;
        assert!(private_file(&fifo, IMAGE_BYTES).is_err());
        fs::write(&image, "not an image")?;
        assert!(read_pair(&paths, &mut None).is_err());
        Ok(())
    }

    #[test]
    fn consumer_identity_rejects_replaced_and_mutated_files() -> Result<()> {
        let (home, paths, _hub) = fixture()?;
        let (pair, _) = read_pair(&paths, &mut None)?;
        let image = private_file(Path::new(&pair.path), IMAGE_BYTES)?;
        assert!(pair.file_identity.matches(&image.metadata()?));
        let replacement = home.path().join("replacement.png");
        image::DynamicImage::new_rgb8(3, 3).save(&replacement)?;
        fs::rename(&replacement, &pair.path)?;
        let reopened = private_file(Path::new(&pair.path), IMAGE_BYTES)?;
        assert!(!pair.file_identity.matches(&reopened.metadata()?));
        let before_write = WallpaperFileIdentity::from_metadata(&reopened.metadata()?);
        fs::write(&pair.path, "changed")?;
        assert!(!before_write.matches(&reopened.metadata()?));
        Ok(())
    }

    #[test]
    fn user_owned_images_outside_home_are_supported_but_unsafe_parents_are_not() -> Result<()> {
        let (_home, paths, _hub) = fixture()?;
        let external = tempfile::tempdir()?;
        let record_path = paths.state.join("presentation.json");
        let mut record: Value = serde_json::from_slice(&fs::read(&record_path)?)?;
        for extension in ["png", "gif", "tiff"] {
            let image = external.path().join(format!("wall.{extension}"));
            image::DynamicImage::new_rgb8(2, 2).save(&image)?;
            record["poster"] = json!(image);
            fs::write(&record_path, serde_json::to_vec(&record)?)?;
            assert!(read_pair(&paths, &mut None).is_ok(), "{extension}");
        }
        fs::set_permissions(external.path(), fs::Permissions::from_mode(0o777))?;
        assert!(read_pair(&paths, &mut None).is_err());
        fs::set_permissions(external.path(), fs::Permissions::from_mode(0o700))?;
        Ok(())
    }

    #[test]
    fn video_and_animation_publish_motion_and_stills_do_not() -> Result<()> {
        let (home, paths, hub) = fixture()?;
        let poster = home.path().join("wall.png");
        let clip = home.path().join("clip.mp4");
        fs::write(&clip, b"not a real video, only metadata is read")?;
        let media = paths.state.join("wallpaper/media.json");
        fs::create_dir_all(media.parent().unwrap())?;
        let write = |poster: &Path, dynamic: bool, animated: bool, path: &Path| -> Result<()> {
            fs::write(
                &media,
                serde_json::to_vec(
                    &json!({"path":path,"poster":poster,"dynamic":dynamic,"animated":animated}),
                )?,
            )?;
            Ok(())
        };
        let mut producer = Producer::default();
        // Still wallpaper: no motion member at all.
        write(&poster, false, false, &poster)?;
        producer.refresh(&hub, &paths)?;
        let still = hub.borrow().snapshot(TOPIC).unwrap();
        assert!(still["current"].get("motion").is_none());
        assert_eq!(still["revision"], 1);
        // Video: motion appears with the file's identity, a new revision, on every output.
        write(&poster, true, false, &clip)?;
        producer.refresh(&hub, &paths)?;
        let video = hub.borrow().snapshot(TOPIC).unwrap();
        let motion = &video["current"]["motion"];
        assert_eq!(motion["kind"], "video");
        assert_eq!(motion["path"], json!(clip));
        let identity: WallpaperFileIdentity =
            serde_json::from_value(motion["file_identity"].clone())?;
        assert!(identity.matches(&fs::metadata(&clip)?));
        assert_eq!(video["outputs"]["DP-2"]["motion"], *motion);
        assert_eq!(video["revision"], 2);
        // Unchanged inputs do not bump the revision.
        producer.refresh(&hub, &paths)?;
        assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["revision"], 2);
        // Animated GIF.
        let gif = home.path().join("anim.gif");
        fs::write(&gif, b"GIF89a")?;
        write(&poster, true, true, &gif)?;
        producer.refresh(&hub, &paths)?;
        let animation = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(animation["current"]["motion"]["kind"], "animation");
        assert_eq!(animation["revision"], 3);
        // A record for another poster, a symlink, a special file or a missing file: still only.
        let other = home.path().join("other.png");
        image::DynamicImage::new_rgb8(2, 2).save(&other)?;
        write(&other, true, false, &clip)?;
        let (pair, _) = read_pair(&paths, &mut None)?;
        assert!(pair.motion.is_none());
        let alias = home.path().join("alias.mp4");
        symlink(&clip, &alias)?;
        write(&poster, true, false, &alias)?;
        assert!(read_pair(&paths, &mut None)?.0.motion.is_none());
        write(&poster, true, false, &home.path().join("missing.mp4"))?;
        assert!(read_pair(&paths, &mut None)?.0.motion.is_none());
        fs::set_permissions(&clip, fs::Permissions::from_mode(0o666))?;
        write(&poster, true, false, &clip)?;
        assert!(read_pair(&paths, &mut None)?.0.motion.is_none());
        fs::set_permissions(&clip, fs::Permissions::from_mode(0o600))?;
        // Back to a still wallpaper removes the member again.
        write(&poster, false, false, &poster)?;
        producer.refresh(&hub, &paths)?;
        let back = hub.borrow().snapshot(TOPIC).unwrap();
        assert!(back["current"].get("motion").is_none());
        assert_eq!(back["revision"], 4);
        Ok(())
    }

    #[test]
    fn pending_video_selection_keeps_only_a_still_matching_previous_motion() -> Result<()> {
        let (home, paths, hub) = fixture()?;
        let poster_a = home.path().join("wall.png");
        let source_a = home.path().join("clip-a.mp4");
        let poster_b = home.path().join("poster-b.png");
        let source_b = home.path().join("clip-b.mp4");
        fs::write(&source_a, b"video A")?;
        fs::write(&source_b, b"video B")?;
        image::DynamicImage::new_rgb8(3, 2).save(&poster_b)?;
        let media = paths.state.join("wallpaper/media.json");
        fs::create_dir_all(media.parent().ok_or("missing media directory")?)?;
        let write_media = |poster: &Path, source: &Path, dynamic: bool| -> Result<()> {
            fs::write(
                &media,
                serde_json::to_vec(&json!({
                    "path":source,
                    "poster":poster,
                    "dynamic":dynamic,
                    "animated":false
                }))?,
            )?;
            Ok(())
        };
        let mut producer = Producer::default();
        write_media(&poster_a, &source_a, true)?;
        producer.refresh(&hub, &paths)?;
        let active_a = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(active_a["current"]["motion"]["path"], json!(source_a));
        let revision_a = active_a["revision"].as_u64().unwrap();

        // A new secure selection record leads its presentation, so keep showing A.
        write_media(&poster_b, &source_b, true)?;
        producer.refresh(&hub, &paths)?;
        let pending_b = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(pending_b["current"]["path"], json!(poster_a));
        assert_eq!(pending_b["current"]["motion"]["path"], json!(source_a));
        assert_eq!(pending_b["revision"], revision_a);

        // If A's source changes while B is pending, do not retain its stale motion.
        fs::write(&source_a, b"video A replaced")?;
        producer.refresh(&hub, &paths)?;
        let invalidated_a = hub.borrow().snapshot(TOPIC).unwrap();
        assert!(invalidated_a["current"].get("motion").is_none());

        // Publishing B's poster joins the already-secure B media record.
        let presentation_path = paths.state.join("presentation.json");
        let mut presentation: Value = serde_json::from_slice(&fs::read(&presentation_path)?)?;
        presentation["poster"] = json!(poster_b);
        fs::write(&presentation_path, serde_json::to_vec(&presentation)?)?;
        producer.refresh(&hub, &paths)?;
        let active_b = hub.borrow().snapshot(TOPIC).unwrap();
        assert_eq!(active_b["current"]["motion"]["path"], json!(source_b));
        let motion_identity: WallpaperFileIdentity =
            serde_json::from_value(active_b["current"]["motion"]["file_identity"].clone())?;
        assert!(motion_identity.matches(&fs::metadata(&source_b)?));

        // An explicit static record for the now-published poster clears motion.
        write_media(&poster_b, &poster_b, false)?;
        producer.refresh(&hub, &paths)?;
        assert!(hub.borrow().snapshot(TOPIC).unwrap()["current"]
            .get("motion")
            .is_none());
        Ok(())
    }
}
