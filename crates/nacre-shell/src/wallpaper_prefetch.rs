// SPDX-License-Identifier: GPL-3.0-only
//! B0171: while the picker is open, decode every catalogue image to each output's display size
//! once, so the first selection of any wallpaper is a cache hit instead of a 400 ms+ decode.
use nacre_protocol::wallpaper::ActiveWallpaper;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Default)]
pub(crate) struct Prefetch {
    epoch: Arc<AtomicU64>,
    /// One decode at a time, whichever prefetch generation asks for it.
    gate: Arc<Mutex<()>>,
}

impl Prefetch {
    /// Replace any earlier plan; `jobs` (pair, width, height, hold) are decoded in order. A `hold`
    /// job (the strand's +-2 window) stays resident in memory; the rest only warm the disk cache.
    pub(crate) fn start(&self, jobs: Vec<(ActiveWallpaper, u32, u32, bool)>) {
        let mine = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let window: Vec<_> = jobs
            .iter()
            .filter(|job| job.3)
            .map(|job| (job.0.clone(), job.1, job.2))
            .collect();
        crate::wallpaper_cache::hot_retain(&window);
        if jobs.is_empty() {
            return;
        }
        let (epoch, gate) = (self.epoch.clone(), self.gate.clone());
        let _ = std::thread::Builder::new()
            .name("wallpaper-prefetch".into())
            .spawn(move || {
                unsafe {
                    // Wallpaper activation runs at nice 10; let that foreground
                    // selection finish before this cancellable raster worker.
                    libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as u32, 19);
                }
                // Nothing heavy while Strand's 675 ms open storyboard runs (five 4K decodes and holds took the
                // memory bus and the main thread's frames on a busy laptop): wait for it to settle.
                if crate::wallpaper_cache::hot_resident().0 == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(750));
                }
                let mut held = false;
                for (pair, width, height, hold) in jobs {
                    if epoch.load(Ordering::Acquire) != mine {
                        return;
                    }
                    if hold {
                        if !crate::wallpaper_cache::hot_has(&pair, width, height) {
                            let _one = gate.lock().unwrap_or_else(|p| p.into_inner());
                            if let Ok(decoded) =
                                crate::wallpaper::decode_pair(pair.clone(), width, height)
                            {
                                crate::wallpaper_cache::hot_hold(
                                    &pair,
                                    width,
                                    height,
                                    decoded.rgba,
                                );
                            }
                        }
                        continue;
                    }
                    if !std::mem::replace(&mut held, true) {
                        strand_predecode_log();
                    }
                    if crate::wallpaper_cache::is_cached(&pair, width, height) {
                        continue;
                    }
                    let _one = gate.lock().unwrap_or_else(|p| p.into_inner());
                    if epoch.load(Ordering::Acquire) != mine {
                        return;
                    }
                    let _ = crate::wallpaper::decode_pair(pair, width, height);
                }
                if !held {
                    strand_predecode_log();
                }
            });
    }

    /// The picker closed: stop decoding and release the held window.
    pub(crate) fn cancel(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        crate::wallpaper_cache::hot_retain(&[]);
        eprintln!("STRAND_PREDECODE entries=0 resident_bytes=0");
    }
}

fn strand_predecode_log() {
    let (entries, resident_bytes) = crate::wallpaper_cache::hot_resident();
    eprintln!("STRAND_PREDECODE entries={entries} resident_bytes={resident_bytes}");
}

/// The chosen wallpaper and its two neighbours either side, wrapping around like the strand.
pub(crate) fn window(ids: &[String], active: Option<usize>) -> Vec<String> {
    let (count, centre) = (ids.len() as isize, active.unwrap_or(0) as isize);
    let mut window: Vec<String> = Vec::new();
    for offset in 0..=2 {
        for step in [offset, -offset] {
            if count > 0 {
                let id = &ids[(centre + step).rem_euclid(count) as usize];
                if !window.contains(id) {
                    window.push(id.clone());
                }
            }
        }
    }
    window
}

/// Catalogue ids nearest the active wallpaper first, so the likeliest next picks are ready first.
pub(crate) fn nearest_first(ids: &[String], active: Option<usize>) -> Vec<String> {
    let centre = active.unwrap_or(0) as isize;
    let mut order: Vec<(isize, usize)> = (0..ids.len())
        .map(|index| ((index as isize - centre).abs(), index))
        .collect();
    order.sort();
    order
        .into_iter()
        .map(|(_, index)| ids[index].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_the_chosen_wallpaper_and_two_neighbours_wrapping() {
        let ids: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"]
            .map(String::from)
            .to_vec();
        assert_eq!(window(&ids, Some(3)), ["d", "e", "c", "f", "b"]);
        assert_eq!(window(&ids, Some(0)), ["a", "b", "g", "c", "f"]);
        assert_eq!(window(&ids[..3], Some(0)), ["a", "b", "c"]);
        assert!(window(&[], None).is_empty());
    }

    #[test]
    fn prefetch_order_starts_beside_the_active_wallpaper() {
        let ids: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
        assert_eq!(nearest_first(&ids, Some(2)), ["c", "b", "d", "a", "e"]);
        assert_eq!(nearest_first(&ids, None), ["a", "b", "c", "d", "e"]);
    }
}
