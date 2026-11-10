// SPDX-License-Identifier: GPL-3.0-only
//! A visible dashboard owns one bounded video child; the UI never loads libmpv.
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

#[derive(Clone, PartialEq)]
pub(super) struct Request {
    pub motion: nacre_protocol::wallpaper::WallpaperMotion,
    pub width: u32,
    pub height: u32,
}

type Frame = (u64, Arc<[u8]>);

pub(super) struct Preview {
    pub request: Request,
    frame: Arc<Mutex<Option<Frame>>>,
    child: Arc<Mutex<Option<Child>>>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Preview {
    pub fn start(request: Request, notify: Option<super::Notify>) -> Self {
        let mut preview = Self {
            request,
            frame: Default::default(),
            child: Default::default(),
            reader: None,
        };
        // Keep a failed attempt until its source/size/policy changes, rather than
        // respawning on every dashboard projection when libmpv is unavailable.
        if preview.spawn(notify).is_err() {
            stop_child(&preview.child);
        }
        preview
    }
    fn spawn(&mut self, notify: Option<super::Notify>) -> std::io::Result<()> {
        let mut child =
            Command::new(std::env::current_exe()?.with_file_name("bead-wallpaper-video"))
                .arg("--preview")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
        let request = serde_json::json!({
            "path": self.request.motion.path,
            "file_identity": self.request.motion.file_identity,
            "width": self.request.width, "height": self.request.height,
        });
        let input = child.stdin.take();
        let output = child.stdout.take();
        *self.child.lock().unwrap() = Some(child);
        input
            .ok_or_else(|| std::io::Error::other("preview stdin"))?
            .write_all(request.to_string().as_bytes())?;
        let mut output = output.ok_or_else(|| std::io::Error::other("preview stdout"))?;
        let frame = self.frame.clone();
        let child = self.child.clone();
        self.reader = Some(
            std::thread::Builder::new()
                .name("dashboard-video".into())
                .spawn(move || {
                    let mut sequence = 0;
                    while let Ok(bytes) = read_frame(&mut output) {
                        sequence += 1;
                        *frame.lock().unwrap() = Some((sequence, bytes));
                        if let Some(notify) = &notify {
                            notify(bead::wayland::UiEvent::Model(Default::default()));
                        }
                    }
                    stop_child(&child);
                    frame.lock().unwrap().take();
                    if let Some(notify) = &notify {
                        notify(bead::wayland::UiEvent::Model(Default::default()));
                    }
                })?,
        );
        Ok(())
    }
    pub fn frame(&self) -> Option<Frame> {
        self.frame.lock().unwrap().clone()
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        stop_child(&self.child);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn stop_child(slot: &Mutex<Option<Child>>) {
    if let Some(mut child) = slot.lock().unwrap().take() {
        // Exact owned child, including a stalled writer or failed decoder.
        let _ = child.kill();
        let _ = child.wait();
    }
}
fn read_frame(reader: &mut impl Read) -> std::io::Result<Arc<[u8]>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header) as usize;
    if size == 0 || size > 8 * 1024 * 1024 {
        return Err(std::io::Error::other("invalid preview frame length"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    Ok(bytes.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_dashboard_reaps_its_preview_even_with_a_blocked_reader() {
        let mut child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut output = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            let _ = read_frame(&mut output);
        });
        let path = std::env::current_exe().unwrap();
        let motion = nacre_protocol::wallpaper::WallpaperMotion {
            kind: nacre_protocol::wallpaper::MotionKind::Video,
            file_identity: nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                &std::fs::metadata(&path).unwrap(),
            ),
            path: path.to_string_lossy().into_owned(),
        };
        let dashboard = super::super::Dashboard::new(
            super::super::Page::Today,
            Arc::new(
                bead_test::source::FakeSource::new(super::super::tests::fixture("calendar"))
                    .unwrap(),
            ),
            None,
            true,
        );
        *dashboard.video.borrow_mut() = Some(Preview {
            request: Request {
                motion,
                width: 1,
                height: 1,
            },
            frame: Default::default(),
            child: Arc::new(Mutex::new(Some(child))),
            reader: Some(reader),
        });
        let stores = bead::core::Stores::default()
            .reconcile(&std::collections::BTreeMap::from([(
                "panel.open".into(),
                bead::core::Value::Bool(false),
            )]))
            .unwrap();
        dashboard.project_with_stores(&stores);
        assert!(dashboard.video.borrow().is_none());
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    #[test]
    fn frame_reader_bounds_and_truncation() {
        assert!(read_frame(&mut &0u32.to_le_bytes()[..]).is_err());
        assert!(read_frame(&mut &u32::MAX.to_le_bytes()[..]).is_err());
        assert!(read_frame(&mut &[2, 0, 0, 0, 1][..]).is_err());
        assert_eq!(&*read_frame(&mut &[2, 0, 0, 0, 1, 2][..]).unwrap(), &[1, 2]);
    }
}
