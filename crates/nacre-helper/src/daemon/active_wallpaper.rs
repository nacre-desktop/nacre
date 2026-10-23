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
use std::os::unix::fs::MetadataExt;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Component, Path},
};

pub const TOPIC: &str = "wallpaper.active";
const RECORD_BYTES: u64 = 128 * 1024;
const IMAGE_BYTES: u64 = nacre_protocol::wallpaper::POSTER_BYTES;
/// Videos are large; the identity carries the exact size, so only sanity-bound it.
const MOTION_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// Resolve through directory descriptors, refusing symlinks at every component.
/// Only user-owned regular files below safe ancestors are eligible for disclosure.
fn private_file(path: &Path, limit: u64) -> Result<File> {
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
fn read_motion(paths: &Paths, poster: &str) -> Option<WallpaperMotion> {
    let file = private_file(&paths.state.join("wallpaper/media.json"), RECORD_BYTES).ok()?;
    let mut bytes = Vec::new();
    file.take(RECORD_BYTES + 1).read_to_end(&mut bytes).ok()?;
    let media: Value = serde_json::from_slice(&bytes).ok()?;
    if media["poster"].as_str() != Some(poster) || media["dynamic"] != true {
        return None;
    }
    let path = media["path"].as_str()?;
    let file = private_file(Path::new(path), MOTION_BYTES).ok()?;
    let motion = WallpaperMotion {
        kind: if media["animated"] == true {
            MotionKind::Animation
        } else {
            MotionKind::Video
        },
        path: path.into(),
        file_identity: WallpaperFileIdentity::from_metadata(&file.metadata().ok()?),
    };
    motion.is_valid().then_some(motion)
}

fn read_pair(
    paths: &Paths,
    watched: &mut Option<std::path::PathBuf>,
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
    let pair = ActiveWallpaper {
        motion: read_motion(paths, path),
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
    pair.validate()?;
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&pair)?));
    Ok((pair, fingerprint))
}

pub fn initial(hub: &Shared) -> Result<()> {
    hub.borrow_mut().publish(
        TOPIC,
        serde_json::to_value(WallpaperActive {
            available: false,
            stale: false,
            revision: 0,
            current: None,
            outputs: BTreeMap::new(),
            error: None,
        })?,
    )?;
    Ok(())
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
        let pair = read_pair(paths, &mut self.watched);
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
                pair.revision = state.revision;
                pair.palette_revision = state.revision;
                state.current = Some(pair);
                state.available = true;
                state.stale = false;
                state.error = None;
            }
            Err(_) => {
                state.available = false;
                state.stale = state.current.is_some();
                state.error = Some("Wallpaper presentation is unavailable".into());
            }
        }
        state.outputs.clear();
        if let Some(pair) = &state.current {
            state
                .outputs
                .extend(outputs.keys().map(|key| (key.clone(), pair.clone())));
        }
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
    BTreeMap::from([(
        "wallpaper.activate".into(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 1000,
            params: serde_json::from_str(include_str!(
                "../../../../protocol/schema/wallpaper-activate.json"
            ))
            .expect("wallpaper request schema"),
            result: serde_json::from_str(include_str!(
                "../../../../protocol/schema/wallpaper-active-image.json"
            ))
            .expect("wallpaper result schema"),
        },
    )])
}

pub fn activate(hub: Shared, params: Value) -> std::result::Result<Value, Failure> {
    let request: WallpaperActivate = serde_json::from_value(params)
        .map_err(|_| failure(ErrorCode::Validation, "Invalid wallpaper activation", false))?;
    request.validate()?;
    let state: WallpaperActive = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Wallpaper is unavailable", true))?;
    if !state.available || state.stale {
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
    Ok(serde_json::to_value(state.current).expect("finite wallpaper pair"))
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
}
