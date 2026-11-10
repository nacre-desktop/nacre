// SPDX-License-Identifier: GPL-3.0-only
use image::ImageReader;
use nacre_protocol::wallpaper::WallpaperFileIdentity;
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(
    path: &std::path::Path,
    width: u32,
    height: u32,
    identity: WallpaperFileIdentity,
) -> ChildGuard {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bead-wallpaper-video"))
        .arg("--preview")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    serde_json::to_writer(
        child.stdin.take().unwrap(),
        &serde_json::json!({"path":path,"file_identity":identity,"width":width,"height":height}),
    )
    .unwrap();
    ChildGuard(child)
}

fn rejected(path: &std::path::Path, width: u32, height: u32, identity: WallpaperFileIdentity) {
    let mut child = start(path, width, height, identity);
    let mut stdout = Vec::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    assert!(!child.0.wait().unwrap().success());
    assert!(stdout.is_empty());
}

#[test]
fn preview_stream_has_changing_frames_and_rejects_invalid_requests() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("nacre-preview-{}-{unique}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let video = root.join("changing.mp4");
    let generated = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x64:rate=10:duration=3",
            "-an",
            "-c:v",
            "mpeg4",
            "-q:v",
            "2",
        ])
        .arg(&video)
        .status()
        .expect("ffmpeg is required by the Nacre desktop CI image");
    assert!(generated.success());
    let identity = WallpaperFileIdentity::from_metadata(&std::fs::metadata(&video).unwrap());

    let mut child = start(&video, 64, 64, identity.clone());
    let mut stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let frames = (|| -> Result<_, String> {
            let mut frames = Vec::new();
            for _ in 0..2 {
                let mut size = [0; 4];
                stdout.read_exact(&mut size).map_err(|e| e.to_string())?;
                let size = u32::from_le_bytes(size) as usize;
                if size == 0 || size > 8 * 1024 * 1024 {
                    return Err("invalid PNG frame length".into());
                }
                let mut png = vec![0; size];
                stdout.read_exact(&mut png).map_err(|e| e.to_string())?;
                let image = ImageReader::new(std::io::Cursor::new(png))
                    .with_guessed_format()
                    .map_err(|e| e.to_string())?
                    .decode()
                    .map_err(|e| e.to_string())?
                    .to_rgba8();
                frames.push(image);
            }
            Ok(frames)
        })();
        let _ = tx.send(frames);
    });
    let frames = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("two preview frames within 10s")
        .unwrap();
    assert_eq!(frames[0].dimensions(), (64, 64));
    assert_eq!(frames[1].dimensions(), (64, 64));
    assert_ne!(
        frames[0].as_raw(),
        frames[1].as_raw(),
        "video preview must animate"
    );
    drop(child);
    reader.join().unwrap();

    let mut bad_identity = identity.clone();
    bad_identity.inode = "0".into();
    rejected(&video, 64, 64, bad_identity);
    rejected(&video, 1601, 64, identity);
    std::fs::remove_dir_all(root).unwrap();
}
