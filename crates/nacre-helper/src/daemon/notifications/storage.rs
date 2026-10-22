//! Pinned notification history and private image cache for the daemon producer.
use crate::{daemon::writer_lease::WriterLease, notification_history};
use nacre_protocol::{core_services::HistoryStorage, ErrorCode, Failure};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

const HISTORY_ROWS: usize = 10_000;
const HISTORY_BYTES: usize = 16 * 1024 * 1024;
const IMAGE_BYTES: usize = 4 * 1024 * 1024;
const IMAGE_CACHE_BYTES: usize = 16 * 1024 * 1024;
const IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const IMAGE_SIDE: u32 = 8192;

pub struct Store {
    path: PathBuf,
    schema: &'static crate::state::Schema,
    lease: WriterLease,
    policy: Value,
    rows: Vec<Value>,
    fingerprint: Option<[u8; 32]>,
    health: HistoryStorage,
    needs_save: bool,
}

impl Store {
    pub fn open(path: PathBuf, policy: Value) -> crate::Result<Self> {
        let parent = path.parent().ok_or("Invalid notification history path")?;
        let schema = crate::state::by_id("notifications")?;
        if crate::state::schema(&path).is_none_or(|found| found.id != schema.id) {
            return Err("Notification history path does not match its schema".into());
        }
        let lease = WriterLease::acquire(parent)?;
        let parent_ready = private_dir(&lease, parent);
        let mut needs_save = false;
        let (rows, fingerprint, health) = if !parent_ready {
            (
                Vec::new(),
                None,
                unavailable("Notification history directory is unsafe or read-only"),
            )
        } else {
            match crate::state::read_document_pinned(&lease, &path, schema) {
                Ok((document, fingerprint)) => {
                    let filtered = notification_history::display_rows(
                        &document,
                        &policy,
                        chrono::Utc::now().timestamp(),
                    );
                    match filtered {
                        Ok(rows) if !super::model::history_supported(&rows) => (
                            Vec::new(),
                            fingerprint,
                            unavailable("Notification history contains unsupported rows"),
                        ),
                        Ok(rows) => {
                            needs_save = document != Value::Array(rows.clone());
                            let writable = history_writable(&lease, &path);
                            let health = HistoryStorage {
                                ready: true,
                                writable,
                                error: (!writable).then(|| {
                                    failure(
                                        ErrorCode::Io,
                                        "Notification history is read-only or unsafe",
                                        true,
                                    )
                                }),
                            };
                            (rows, fingerprint, health)
                        }
                        Err(_) => (
                            Vec::new(),
                            fingerprint,
                            unavailable("Notification history could not be filtered"),
                        ),
                    }
                }
                Err((_, message)) => (
                    Vec::new(),
                    None,
                    unavailable(if message.contains("Unsupported") {
                        "Notification history version is unsupported"
                    } else {
                        "Notification history could not be read"
                    }),
                ),
            }
        };
        Ok(Self {
            path,
            schema,
            lease,
            policy,
            rows,
            fingerprint,
            health,
            needs_save,
        })
    }

    pub fn needs_save(&self) -> bool {
        self.needs_save
    }

    pub fn rows(&self) -> Vec<Value> {
        self.rows.clone()
    }

    pub fn health(&self) -> HistoryStorage {
        self.health.clone()
    }

    /// Persist already-pruned model rows. This layer enforces hard file bounds,
    /// but never evicts active or closed rows on the model's behalf.
    pub fn save(&mut self, rows: &Value) -> Result<(), Failure> {
        if !self.health.ready || !self.health.writable {
            return Err(self.health.error.clone().unwrap_or_else(|| {
                failure(
                    ErrorCode::Unavailable,
                    "Notification history is unavailable",
                    true,
                )
            }));
        }
        let selected =
            notification_history::display_rows(rows, &self.policy, chrono::Utc::now().timestamp())
                .map_err(|_| {
                    failure(ErrorCode::Validation, "Invalid notification history", false)
                })?;
        if selected.len() > HISTORY_ROWS {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification history limit reached",
                false,
            ));
        }
        let envelope = json!({"version": self.schema.version, "items": &selected});
        let bytes = crate::json::encode(&envelope).map_err(|_| {
            failure(
                ErrorCode::Internal,
                "Cannot encode notification history",
                false,
            )
        })?;
        if bytes.len() > HISTORY_BYTES {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification history size limit reached",
                false,
            ));
        }
        if !history_writable(&self.lease, &self.path) {
            let error = failure(
                ErrorCode::Io,
                "Notification history is read-only or unsafe",
                true,
            );
            self.health.error = Some(error.clone());
            return Err(error);
        }
        if selected == self.rows && !self.needs_save && self.health.error.is_none() {
            return Ok(());
        }
        if crate::state::write_checked_pinned(
            &self.lease,
            &self.path,
            self.schema,
            &Value::Array(selected.clone()),
            self.fingerprint,
        )
        .is_err()
        {
            let error = failure(
                ErrorCode::Io,
                "Notification history could not be saved",
                true,
            );
            self.health.error = Some(error.clone());
            return Err(error);
        }
        let (saved, fingerprint) =
            crate::state::read_document_pinned(&self.lease, &self.path, self.schema).map_err(
                |_| {
                    let error = failure(
                        ErrorCode::Io,
                        "Notification history could not be verified",
                        true,
                    );
                    self.health.error = Some(error.clone());
                    error
                },
            )?;
        let saved = notification_history::display_rows(
            &saved,
            &self.policy,
            chrono::Utc::now().timestamp(),
        )
        .map_err(|_| {
            failure(
                ErrorCode::Internal,
                "Saved notification history is invalid",
                false,
            )
        })?;
        self.rows = saved;
        self.needs_save = false;
        self.fingerprint = fingerprint;
        self.health = HistoryStorage {
            ready: true,
            writable: true,
            error: None,
        };
        Ok(())
    }
}

fn failure(code: ErrorCode, message: &str, retry: bool) -> Failure {
    Failure::new(code, message, retry)
}

fn unavailable(message: &str) -> HistoryStorage {
    HistoryStorage {
        ready: false,
        writable: false,
        error: Some(failure(ErrorCode::Unavailable, message, true)),
    }
}

fn private_dir(lease: &WriterLease, path: &Path) -> bool {
    if lease.ensure_valid().is_err() {
        return false;
    }
    let Some(directory) = lease.directory_fd(&path.join(".writer.lock")) else {
        return false;
    };
    nix::sys::stat::fstat(directory).is_ok_and(|meta| {
        meta.st_mode & nix::sys::stat::SFlag::S_IFMT.bits() == nix::sys::stat::SFlag::S_IFDIR.bits()
            && meta.st_uid == nix::unistd::getuid().as_raw()
            && meta.st_mode & 0o777 == 0o700
    })
}

fn history_writable(lease: &WriterLease, path: &Path) -> bool {
    let Some(parent) = lease.directory_fd(path) else {
        return false;
    };
    let Ok(parent_meta) = nix::sys::stat::fstat(parent) else {
        return false;
    };
    if parent_meta.st_mode & 0o777 != 0o700 {
        return false;
    }
    let Some(name) = path.file_name() else {
        return false;
    };
    match nix::sys::stat::fstatat(parent, name, nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW) {
        Err(nix::errno::Errno::ENOENT) => true,
        Ok(meta) => {
            meta.st_mode & nix::sys::stat::SFlag::S_IFMT.bits()
                == nix::sys::stat::SFlag::S_IFREG.bits()
                && meta.st_uid == nix::unistd::getuid().as_raw()
                && meta.st_nlink == 1
                && meta.st_mode & 0o777 == 0o600
                && meta.st_mode & 0o200 != 0
        }
        Err(_) => false,
    }
}

/// Cache supported notification images under digest names only; no sender path
/// or provider URL is ever returned to a client.
pub struct ImageCache {
    root: PathBuf,
    lease: WriterLease,
}

struct TempCleanup<'a> {
    root: &'a fs::File,
    name: String,
}

impl Drop for TempCleanup<'_> {
    fn drop(&mut self) {
        let _ = nix::unistd::unlinkat(
            self.root,
            self.name.as_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        );
    }
}

impl ImageCache {
    pub fn open(root: PathBuf) -> Result<Self, Failure> {
        let lease = WriterLease::acquire(&root).map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache is unavailable",
                true,
            )
        })?;
        if !private_dir(&lease, &root) {
            return Err(failure(
                ErrorCode::Io,
                "Notification image cache is unsafe or read-only",
                false,
            ));
        }
        let cache = Self { root, lease };
        cache.cleanup_temporaries()?;
        Ok(cache)
    }

    pub fn resolve_path(&self, source: &str) -> Result<String, Failure> {
        if source.len() > 4096 {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image source is too long",
                false,
            ));
        }
        let path = local_image_path(source)?;
        let file = open_local_image(&path)?;
        let metadata = file
            .metadata()
            .map_err(|_| failure(ErrorCode::Io, "Notification image is unavailable", false))?;
        if !metadata.is_file() {
            return Err(failure(
                ErrorCode::Validation,
                "Notification image source is unsafe",
                false,
            ));
        }
        if metadata.uid() != nix::unistd::getuid().as_raw() && metadata.uid() != 0 {
            return Err(failure(
                ErrorCode::PermissionDenied,
                "Notification image source is not accessible",
                false,
            ));
        }
        if metadata.len() > IMAGE_BYTES as u64 {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image exceeds its size limit",
                false,
            ));
        }
        let mut input = Vec::new();
        file.take(IMAGE_BYTES as u64 + 1)
            .read_to_end(&mut input)
            .map_err(|_| failure(ErrorCode::Io, "Notification image could not be read", false))?;
        if input.len() > IMAGE_BYTES {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image exceeds its size limit",
                false,
            ));
        }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(input))
            .with_guessed_format()
            .map_err(|_| {
                failure(
                    ErrorCode::Validation,
                    "Unsupported notification image",
                    false,
                )
            })?;
        reader.limits(image_limits());
        let decoded = reader.decode().map_err(|_| {
            failure(
                ErrorCode::Validation,
                "Unsupported notification image",
                false,
            )
        })?;
        if !within_image_bounds(decoded.width(), decoded.height()) {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image dimensions exceed their limit",
                false,
            ));
        }
        self.publish_png(decoded.to_rgba8())
    }

    /// GdkPixbuf tuple: width, height, rowstride, alpha, bits/sample, channels, bytes.
    pub fn resolve_data(
        &self,
        pixbuf: (i32, i32, i32, bool, i32, i32, Vec<u8>),
    ) -> Result<String, Failure> {
        let (width, height, rowstride, alpha, bits, channels, bytes) = pixbuf;
        if width <= 0
            || height <= 0
            || rowstride <= 0
            || bits != 8
            || channels != if alpha { 4 } else { 3 }
            || !within_image_bounds(width as u32, height as u32)
            || bytes.len() > IMAGE_BYTES
        {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image data exceeds its bounds",
                false,
            ));
        }
        let row_bytes = (width as usize)
            .checked_mul(channels as usize)
            .ok_or_else(|| {
                failure(
                    ErrorCode::TooLarge,
                    "Invalid notification image data",
                    false,
                )
            })?;
        if (rowstride as usize) < row_bytes
            || (height as usize - 1)
                .checked_mul(rowstride as usize)
                .and_then(|offset| offset.checked_add(row_bytes))
                .is_none_or(|needed| needed > bytes.len())
        {
            return Err(failure(
                ErrorCode::Validation,
                "Invalid notification image data",
                false,
            ));
        }
        let mut image = image::RgbaImage::new(width as u32, height as u32);
        for y in 0..height as usize {
            let row = y * rowstride as usize;
            for x in 0..width as usize {
                let start = row + x * channels as usize;
                let pixel = if alpha {
                    [
                        bytes[start],
                        bytes[start + 1],
                        bytes[start + 2],
                        bytes[start + 3],
                    ]
                } else {
                    [bytes[start], bytes[start + 1], bytes[start + 2], 255]
                };
                image.put_pixel(x as u32, y as u32, image::Rgba(pixel));
            }
        }
        self.publish_png(image)
    }

    pub fn retain(&self, referenced: &[String]) -> Result<(), Failure> {
        let keep: std::collections::BTreeSet<_> = referenced
            .iter()
            .filter_map(|path| self.cache_name(path))
            .collect();
        self.lease
            .ensure_valid()
            .map_err(|_| failure(ErrorCode::Io, "Notification image cache is unsafe", false))?;
        let root = self
            .lease
            .directory_fd(&self.root.join(".writer.lock"))
            .ok_or_else(|| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache is unavailable",
                    true,
                )
            })?;
        for entry in fs::read_dir(&self.root).map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache is unavailable",
                true,
            )
        })? {
            let entry = entry.map_err(|_| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache is unavailable",
                    true,
                )
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_cache_name(&name) || keep.contains(&name) {
                continue;
            }
            let stat = nix::sys::stat::fstatat(
                root,
                name.as_str(),
                nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW,
            )
            .map_err(|_| failure(ErrorCode::Io, "Notification image cache is unsafe", false))?;
            if !private_cache_file(&stat) {
                return Err(failure(
                    ErrorCode::Io,
                    "Notification image cache is unsafe",
                    false,
                ));
            }
            nix::unistd::unlinkat(root, name.as_str(), nix::unistd::UnlinkatFlags::NoRemoveDir)
                .map_err(|_| {
                    failure(
                        ErrorCode::Io,
                        "Notification image cache cleanup failed",
                        true,
                    )
                })?;
        }
        root.sync_all().map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache cleanup failed",
                true,
            )
        })?;
        Ok(())
    }

    fn publish_png(&self, image: image::RgbaImage) -> Result<String, Failure> {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|_| {
                failure(
                    ErrorCode::Validation,
                    "Notification image could not be encoded",
                    false,
                )
            })?;
        let bytes = png.into_inner();
        if bytes.len() > IMAGE_BYTES {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image exceeds its size limit",
                false,
            ));
        }
        let digest = format!("{:x}.png", Sha256::digest(&bytes));
        let path = self.root.join(&digest);
        self.lease
            .ensure_valid()
            .map_err(|_| failure(ErrorCode::Io, "Notification image cache is unsafe", false))?;
        if path.exists() {
            self.verify_cached(&digest)?;
            if self.cache_bytes()? > IMAGE_CACHE_BYTES {
                return Err(failure(
                    ErrorCode::TooLarge,
                    "Notification image cache is full",
                    false,
                ));
            }
            return Ok(path.to_string_lossy().into_owned());
        }
        let total = self.cache_bytes()?;
        if total
            .checked_add(bytes.len())
            .is_none_or(|size| size > IMAGE_CACHE_BYTES)
        {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification image cache is full",
                false,
            ));
        }
        let root = self
            .lease
            .directory_fd(&self.root.join(".writer.lock"))
            .ok_or_else(|| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache is unavailable",
                    true,
                )
            })?;
        let temporary = format!(".image-{}.tmp", uuid::Uuid::new_v4().simple());
        let fd = nix::fcntl::openat(
            root,
            temporary.as_str(),
            nix::fcntl::OFlag::O_WRONLY
                | nix::fcntl::OFlag::O_CREAT
                | nix::fcntl::OFlag::O_EXCL
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::from_bits_truncate(0o600),
        )
        .map_err(|_| failure(ErrorCode::Io, "Notification image cache write failed", true))?;
        let _cleanup = TempCleanup {
            root,
            name: temporary.clone(),
        };
        let mut file = fs::File::from(fd);
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| failure(ErrorCode::Io, "Notification image cache write failed", true))?;
        self.lease
            .ensure_valid()
            .map_err(|_| failure(ErrorCode::Io, "Notification image cache is unsafe", false))?;
        nix::fcntl::renameat(root, temporary.as_str(), root, digest.as_str()).map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache publish failed",
                true,
            )
        })?;
        root.sync_all().map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache publish failed",
                true,
            )
        })?;
        Ok(path.to_string_lossy().into_owned())
    }

    fn verify_cached(&self, name: &str) -> Result<(), Failure> {
        if !is_cache_name(name) {
            return Err(failure(
                ErrorCode::Validation,
                "Invalid notification cache entry",
                false,
            ));
        }
        let stat = nix::sys::stat::fstatat(
            self.lease
                .directory_fd(&self.root.join(".writer.lock"))
                .ok_or_else(|| {
                    failure(
                        ErrorCode::Io,
                        "Notification image cache is unavailable",
                        true,
                    )
                })?,
            name,
            nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW,
        )
        .map_err(|_| failure(ErrorCode::Io, "Notification image cache is unsafe", false))?;
        if !private_cache_file(&stat) || stat.st_size < 0 || stat.st_size as usize > IMAGE_BYTES {
            return Err(failure(
                ErrorCode::Io,
                "Notification image cache is unsafe",
                false,
            ));
        }
        Ok(())
    }

    fn cleanup_temporaries(&self) -> Result<(), Failure> {
        let root = self
            .lease
            .directory_fd(&self.root.join(".writer.lock"))
            .ok_or_else(|| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache is unavailable",
                    true,
                )
            })?;
        let mut changed = false;
        for entry in fs::read_dir(&self.root).map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache is unavailable",
                true,
            )
        })? {
            let entry = entry.map_err(|_| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache is unavailable",
                    true,
                )
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_cache_temporary(&name) {
                continue;
            }
            let stat = nix::sys::stat::fstatat(
                root,
                name.as_str(),
                nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW,
            )
            .map_err(|_| failure(ErrorCode::Io, "Notification image cache is unsafe", false))?;
            if !private_cache_file(&stat) {
                return Err(failure(
                    ErrorCode::Io,
                    "Notification image cache is unsafe",
                    false,
                ));
            }
            nix::unistd::unlinkat(root, name.as_str(), nix::unistd::UnlinkatFlags::NoRemoveDir)
                .map_err(|_| {
                    failure(
                        ErrorCode::Io,
                        "Notification image cache cleanup failed",
                        true,
                    )
                })?;
            changed = true;
        }
        if changed {
            root.sync_all().map_err(|_| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache cleanup failed",
                    true,
                )
            })?;
        }
        Ok(())
    }

    fn cache_bytes(&self) -> Result<usize, Failure> {
        let mut total = 0usize;
        for entry in fs::read_dir(&self.root).map_err(|_| {
            failure(
                ErrorCode::Io,
                "Notification image cache is unavailable",
                true,
            )
        })? {
            let entry = entry.map_err(|_| {
                failure(
                    ErrorCode::Io,
                    "Notification image cache is unavailable",
                    true,
                )
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_cache_name(&name) {
                self.verify_cached(&name)?;
                total = total.saturating_add(
                    entry
                        .metadata()
                        .map_err(|_| {
                            failure(ErrorCode::Io, "Notification image cache is unsafe", false)
                        })?
                        .len() as usize,
                );
            }
        }
        Ok(total)
    }

    fn cache_name(&self, path: &str) -> Option<String> {
        let path = Path::new(path);
        if path.parent() != Some(self.root.as_path()) {
            return None;
        }
        let name = path.file_name()?.to_str()?;
        is_cache_name(name).then(|| name.to_owned())
    }
}

fn private_cache_file(stat: &nix::sys::stat::FileStat) -> bool {
    stat.st_mode & nix::sys::stat::SFlag::S_IFMT.bits() == nix::sys::stat::SFlag::S_IFREG.bits()
        && stat.st_uid == nix::unistd::getuid().as_raw()
        && stat.st_nlink == 1
        && stat.st_mode & 0o777 == 0o600
}

fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(IMAGE_SIDE);
    limits.max_image_height = Some(IMAGE_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits
}

fn within_image_bounds(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= IMAGE_SIDE
        && height <= IMAGE_SIDE
        && u64::from(width) * u64::from(height) <= IMAGE_PIXELS
}

fn is_cache_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 68 && &bytes[64..] == b".png" && bytes[..64].iter().all(u8::is_ascii_hexdigit)
}

fn is_cache_temporary(name: &str) -> bool {
    name.strip_prefix(".image-")
        .and_then(|value| value.strip_suffix(".tmp"))
        .is_some_and(|value| {
            value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn local_image_path(source: &str) -> Result<PathBuf, Failure> {
    let decoded = if let Some(rest) = source.strip_prefix("file://") {
        let path = if rest.starts_with('/') {
            rest
        } else if let Some(local) = rest.strip_prefix("localhost/") {
            return percent_decode(&format!("/{local}"));
        } else {
            return Err(failure(
                ErrorCode::Validation,
                "Unsupported notification image source",
                false,
            ));
        };
        percent_decode(path)?
    } else if source.starts_with("/") {
        PathBuf::from(source)
    } else {
        return Err(failure(
            ErrorCode::Validation,
            "Unsupported notification image source",
            false,
        ));
    };
    if !decoded.is_absolute()
        || decoded
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(failure(
            ErrorCode::Validation,
            "Unsupported notification image source",
            false,
        ));
    }
    Ok(decoded)
}

fn percent_decode(value: &str) -> Result<PathBuf, Failure> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(failure(
                    ErrorCode::Validation,
                    "Invalid notification image URI",
                    false,
                ));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or_else(|| {
                    failure(
                        ErrorCode::Validation,
                        "Invalid notification image URI",
                        false,
                    )
                })?;
            output.push(hex);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map(PathBuf::from).map_err(|_| {
        failure(
            ErrorCode::Validation,
            "Invalid notification image URI",
            false,
        )
    })
}

/// Walk with directory descriptors: checking names then opening the full path
/// would allow a parent symlink to be swapped between those operations.
fn open_local_image(path: &Path) -> Result<fs::File, Failure> {
    use nix::{
        fcntl::{openat, OFlag},
        sys::stat::Mode,
    };
    let denied = || {
        failure(
            ErrorCode::Validation,
            "Notification image source is unsafe",
            false,
        )
    };
    let mut directory = fs::File::open("/").map_err(|_| denied())?;
    let components = path
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (index, name) in components.iter().enumerate() {
        let flags = OFlag::O_RDONLY
            | OFlag::O_NOFOLLOW
            | OFlag::O_CLOEXEC
            | if index + 1 == components.len() {
                OFlag::O_NONBLOCK
            } else {
                OFlag::O_DIRECTORY
            };
        let fd = openat(&directory, *name, flags, Mode::empty()).map_err(|_| denied())?;
        directory = fs::File::from(fd);
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn policy() -> Value {
        serde_json::from_str(include_str!(
            "../../../../../nacre/shell-tools/notification-policy.json"
        ))
        .unwrap()
    }

    fn history(root: &Path) -> PathBuf {
        root.join("nacre/shell/notifications/history.json")
    }

    #[test]
    fn strict_store_reads_filters_and_uses_the_parent_lease() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = history(root.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            &path,
            br#"[{"key":"kept","time":"2099-01-01T00:00:00Z","summary":"hello","body":"world","secret":"drop"}]"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let original = fs::read(&path).unwrap();
        let store = Store::open(path.clone(), policy()).unwrap();
        assert!(store.health().ready);
        assert!(store.health().writable);
        assert_eq!(store.rows().len(), 1);
        assert!(store.rows()[0].get("secret").is_none());
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "open must not prune on disk"
        );
        assert!(WriterLease::acquire(path.parent().unwrap()).is_err());
    }

    #[test]
    fn unsupported_legacy_rows_are_protected_instead_of_dropped_on_the_next_save() {
        for row in [
            json!({"summary":"undated retained text"}),
            json!({"time":"2099-01-01T00:00:00Z","body":"x".repeat(8193)}),
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let path = history(root.path());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
            let original = serde_json::to_vec(&json!([row])).unwrap();
            fs::write(&path, &original).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let mut store = Store::open(path.clone(), policy()).unwrap();
            assert!(!store.health().ready);
            assert!(!store.health().writable);
            assert!(store.save(&json!([])).is_err());
            assert_eq!(fs::read(path).unwrap(), original);
        }
    }

    #[test]
    fn startup_retention_pruning_is_flushed_even_without_new_notifications() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = history(root.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            &path,
            br#"[{"key":"expired","time":"2000-01-01T00:00:00Z","summary":"expired"}]"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let mut store = Store::open(path.clone(), policy()).unwrap();
        assert!(store.rows().is_empty());
        assert!(store.needs_save());
        store.save(&json!([])).unwrap();
        assert!(!store.needs_save());
        let disk: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(disk["items"], json!([]));
    }

    #[test]
    fn corrupt_and_future_documents_are_read_only_and_preserved() {
        for bytes in [
            br#"{"version":1,"items":["broken"]}"#.as_slice(),
            br#"{"version":2,"items":[]}"#.as_slice(),
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let path = history(root.path());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let original = fs::read(&path).unwrap();
            let mut store = Store::open(path.clone(), policy()).unwrap();
            assert!(!store.health().ready);
            assert!(!store.health().writable);
            assert!(store.rows().is_empty());
            assert!(store.save(&json!([])).is_err());
            assert_eq!(fs::read(path).unwrap(), original);
        }
    }

    #[test]
    fn store_saves_whitelisted_rows_atomically_with_private_modes() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = history(root.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        let mut store = Store::open(path.clone(), policy()).unwrap();
        store
            .save(&json!([{
                "key":"saved", "time":"2099-01-01T00:00:00Z", "summary":"saved",
                "body":"text", "unknown":"secret"
            }]))
            .unwrap();
        assert_eq!(store.rows().len(), 1);
        assert!(store.rows()[0].get("unknown").is_none());
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn read_only_history_is_available_but_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = history(root.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            &path,
            br#"[{"key":"kept","time":"2099-01-01T00:00:00Z","summary":"hello"}]"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let original = fs::read(&path).unwrap();
        let mut store = Store::open(path.clone(), policy()).unwrap();
        assert!(store.health().ready);
        assert!(!store.health().writable);
        assert_eq!(store.rows().len(), 1);
        assert!(store.save(&json!([])).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn transient_save_failure_keeps_memory_and_can_retry() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = history(root.path());
        let mut store = Store::open(path.clone(), policy()).unwrap();
        let original = json!([{"key":"old","time":"2099-01-01T00:00:00Z","summary":"old"}]);
        store.save(&original).unwrap();
        let changed = json!([{"key":"new","time":"2099-01-01T00:00:00Z","summary":"new"}]);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert!(store.save(&changed).is_err());
        assert!(store.health().error.is_some());
        assert_eq!(store.rows()[0]["key"], "old");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        store.save(&changed).unwrap();
        assert!(store.health().error.is_none());
        assert_eq!(store.rows()[0]["key"], "new");
    }

    #[test]
    fn image_cache_copies_local_images_and_rejects_remote_and_bad_pixbufs() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let source = root.path().join("source.png");
        image::DynamicImage::new_rgba8(2, 2).save(&source).unwrap();
        let cache = ImageCache::open(root.path().join("nacre/cache/notification-images")).unwrap();
        let cached = cache.resolve_path(source.to_str().unwrap()).unwrap();
        let metadata = fs::metadata(&cached).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert!(cache.resolve_path("https://example.test/a.png").is_err());
        assert!(cache.resolve_data((1, 1, 1, true, 8, 4, vec![1])).is_err());
        cache.retain(std::slice::from_ref(&cached)).unwrap();
        assert!(Path::new(&cached).exists());
        cache.retain(&[]).unwrap();
        assert!(!Path::new(&cached).exists());
    }
}
