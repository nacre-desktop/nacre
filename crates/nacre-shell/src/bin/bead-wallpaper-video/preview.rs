// SPDX-License-Identifier: GPL-3.0-only
//! Bounded framed-PNG stream for a single dashboard wallpaper preview.
use crate::mpv::{Notice, Player};
use bead_wallpaper::protocol::Fit;
use nacre_protocol::wallpaper::WallpaperFileIdentity;
use serde::Deserialize;
use std::{
    io::{self, Read, Write},
    os::fd::AsRawFd,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_REQUEST: usize = 16 * 1024;
const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAX_WIDTH: u32 = 1600;
const MAX_HEIGHT: u32 = 1000;
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);

fn valid_dimensions(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width <= MAX_WIDTH && height <= MAX_HEIGHT
}

fn valid_path(path: &str) -> bool {
    path.len() <= 4096
        && !path.contains(['\0', '\n'])
        && !path.contains("://")
        && Path::new(path).is_absolute()
        && !Path::new(path)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    path: String,
    file_identity: WallpaperFileIdentity,
    width: u32,
    height: u32,
}

fn request() -> Result<(Request, std::fs::File), String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_REQUEST + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_REQUEST {
        return Err("request too large".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !valid_dimensions(request.width, request.height) {
        return Err("preview dimensions out of bounds".into());
    }
    request.file_identity.validate().map_err(|e| e.message)?;
    if !valid_path(&request.path) {
        return Err("preview path is not a valid local absolute path".into());
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&request.path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || !request.file_identity.matches(&metadata) {
        return Err("preview source identity changed".into());
    }
    Ok((request, file))
}

pub fn run() -> Result<(), String> {
    let (request, file) = request()?;
    let (ping, _source) = calloop::ping::make_ping().map_err(|e| e.to_string())?;
    // Reuse the live renderer's VA-API copy-back path, with software fallback.
    let player = Player::new("vaapi", None, None, Arc::new(ping))?;
    // Use bilinear software scaling for the small preview, as the GL path does.
    player.property("sws-allow-zimg", "no");
    player.property("sws-scaler", "bilinear");
    player.property("sws-fast", "yes");
    player.load(
        &format!("/proc/self/fd/{}", file.as_raw_fd()),
        Fit::Crop,
        true,
        0.0,
    )?;

    let stride = request.width as usize * 4;
    let mut bgr = vec![0_u8; stride * request.height as usize];
    let first_deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
    let mut loaded = false;
    let mut delivered_frame = false;
    let mut next = Instant::now();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    loop {
        for notice in player.notices() {
            match notice {
                Notice::Loaded => loaded = true,
                Notice::Failed(message) => return Err(message),
            }
        }
        if !delivered_frame && Instant::now() > first_deadline {
            return Err("timed out waiting for first preview frame".into());
        }
        if !loaded {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        if delivered_frame {
            let now = Instant::now();
            if now < next {
                std::thread::sleep(next - now);
            }
        }
        if player.frame_ready() {
            let frame_started = Instant::now();
            if !player.render(request.width, request.height, stride, &mut bgr) {
                return Err("mpv preview render failed".into());
            }
            // Pack opaque RGB into the prefix; mpv overwrites the buffer next render.
            for index in 0..bgr.len() / 4 {
                let rgb = [bgr[index * 4 + 2], bgr[index * 4 + 1], bgr[index * 4]];
                bgr[index * 3..index * 3 + 3].copy_from_slice(&rgb);
            }
            use image::ImageEncoder;
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new_with_quality(
                &mut png,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Sub,
            )
            .write_image(
                &bgr[..bgr.len() / 4 * 3],
                request.width,
                request.height,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| e.to_string())?;
            if png.len() > MAX_FRAME {
                return Err("encoded preview frame exceeds 8 MiB".into());
            }
            stdout
                .write_all(&(png.len() as u32).to_le_bytes())
                .and_then(|_| stdout.write_all(&png))
                .and_then(|_| stdout.flush())
                .map_err(|e| e.to_string())?;
            delivered_frame = true;
            next = frame_started + FRAME_INTERVAL;
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_rejects_unbounded_dimensions() {
        assert!(!valid_dimensions(MAX_WIDTH + 1, 1));
        assert!(!valid_dimensions(1, MAX_HEIGHT + 1));
        assert!(!valid_dimensions(0, 1));
        assert!(valid_dimensions(640, 360));
    }

    #[test]
    fn source_path_matches_wallpaper_path_rules() {
        assert!(valid_path("/home/test/wallpaper.mp4"));
        assert!(!valid_path("/home/test/../outside.mp4"));
        assert!(!valid_path("/home/test/movie://remote"));
        assert!(!valid_path("relative.mp4"));
    }
}
