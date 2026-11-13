// SPDX-License-Identifier: GPL-3.0-only
//! Native desktop font selection stays in the application, not Bead's dependency graph.
use bead::render::{Primitive, Result, Scene};
use skrifa::MetadataProvider;
use std::{
    collections::BTreeSet,
    ffi::CString,
    path::PathBuf,
    sync::{LazyLock, Mutex, MutexGuard},
    time::Instant,
};
const OUTFIT: &[u8] = include_bytes!("../assets/fonts/Outfit-Variable.ttf");
static OUTFIT_PATH: std::sync::OnceLock<std::result::Result<PathBuf, String>> =
    std::sync::OnceLock::new();
static OUTFIT_REGISTERED: std::sync::OnceLock<std::result::Result<(), String>> =
    std::sync::OnceLock::new();
#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigGetCurrent() -> *mut std::ffi::c_void;
    fn FcConfigAppFontAddFile(config: *mut std::ffi::c_void, file: *const u8) -> i32;
    fn FcConfigBuildFonts(config: *mut std::ffi::c_void) -> i32;
}

fn bundled_outfit_path() -> Result<PathBuf> {
    OUTFIT_PATH
        .get_or_init(|| -> std::result::Result<PathBuf, String> {
            let cache = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
                .unwrap_or_else(std::env::temp_dir)
                .join("bead/fonts");
            std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
            let path = cache.join("Outfit-Variable.ttf");
            if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() == OUTFIT.len() as u64)
            {
                return Ok(path);
            }
            let temporary = cache.join(format!(".Outfit-{}.ttf", std::process::id()));
            std::fs::write(&temporary, OUTFIT).map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
            Ok(path)
        })
        .clone()
        .map_err(|error| error.into())
        .and_then(|path| {
            OUTFIT_REGISTERED
                .get_or_init(|| {
                    let filename = std::ffi::CString::new(path.to_string_lossy().as_bytes())
                        .map_err(|error| error.to_string())?;
                    // Register this bundled family only in the shell process; never install a
                    // system font or alter the host Fontconfig configuration.
                    let config = unsafe { FcConfigGetCurrent() };
                    if config.is_null()
                        || unsafe { FcConfigAppFontAddFile(config, filename.as_ptr().cast()) } == 0
                        || unsafe { FcConfigBuildFonts(config) } == 0
                    {
                        return Err("Fontconfig rejected bundled Outfit".to_owned());
                    }
                    Ok(())
                })
                .clone()
                .map(|()| path)
                .map_err(Into::into)
        })
}
type Selection = (BTreeSet<(String, u16)>, BTreeSet<char>);
#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcWeightFromOpenType(weight: std::ffi::c_int) -> std::ffi::c_int;
}
type Selections = bead::core::cache::Lru<Selection, Vec<PathBuf>>;
/// One cache for the process: panels rebuild their session on every open, and each fresh
/// runtime and text meter would otherwise rerun Fontconfig for the same font set.
// shortcut: fonts installed while the shell runs show up only after a reload clears this.
static SELECTIONS: LazyLock<Mutex<Selections>> =
    LazyLock::new(|| Mutex::new(Selections::new("font-selector", 512 * 1024)));
fn selections() -> MutexGuard<'static, Selections> {
    SELECTIONS.lock().unwrap_or_else(|p| p.into_inner())
}
#[derive(Default)]
pub struct Selector {
    ascii: bool,
}
impl Selector {
    /// Select for printable ASCII as well, so the first typed query reuses the opening
    /// selection instead of running Fontconfig again on the input path.
    pub fn with_ascii(mut self) -> Self {
        self.ascii = true;
        self
    }
    pub fn clear(&mut self) {
        selections().clear();
    }
    pub fn select(&mut self, scene: &Scene) -> Result<Vec<PathBuf>> {
        let _profile = bead::core::profile::Span::new("font_select");
        let mut families = BTreeSet::new();
        let mut chars = BTreeSet::new();
        for p in &scene.primitives {
            if let Primitive::Text {
                family,
                text,
                weight,
                ..
            } = p
            {
                families.insert((family.clone(), *weight));
                chars.extend(text.chars().filter(|c| !c.is_control()));
            }
        }
        if self.ascii {
            chars.extend(' '..='~');
        }
        let entries = selections();
        if let Some(paths) = entries
            .keys()
            .find(|(f, c)| *f == families && c.is_superset(&chars))
            .and_then(|key| entries.get(key))
        {
            return Ok(paths.clone());
        }
        drop(entries);
        let paths = select_chars(scene, self.ascii)?;
        if families.len() <= 32 && chars.len() <= 4096 {
            let bytes = families
                .iter()
                .map(|(f, _)| f.capacity() + 64)
                .sum::<usize>()
                + chars.len() * 40
                + paths
                    .iter()
                    .map(|p| p.as_os_str().len() + 32)
                    .sum::<usize>()
                + 128;
            let mut entries = selections();
            // Admission failure merely disables this optional acceleration.
            let _ = entries.insert((families, chars), paths.clone(), bytes);
            if entries.len() > 32 {
                entries.pop_lru();
            }
        }
        Ok(paths)
    }
}
pub fn select(scene: &Scene) -> Result<Vec<PathBuf>> {
    Selector::default().select(scene)
}
fn select_chars(scene: &Scene, ascii: bool) -> Result<Vec<PathBuf>> {
    let start = Instant::now();
    let mut families = BTreeSet::new();
    let mut needed = BTreeSet::new();
    for p in &scene.primitives {
        if let Primitive::Text {
            family,
            text,
            weight,
            ..
        } = p
        {
            families.insert((family.as_str(), *weight));
            needed.extend(text.chars().filter(|c| !c.is_control()));
        }
    }
    if families.is_empty() {
        return Ok(Vec::new());
    }
    if ascii {
        needed.extend(' '..='~');
    }
    let fc = fontconfig::Fontconfig::new().ok_or("native fontconfig unavailable")?;
    let mut paths = BTreeSet::new();
    let mut db = fontdb::Database::new();
    for (family, weight) in &families {
        if family.eq_ignore_ascii_case("Outfit") {
            load(&mut db, &mut paths, bundled_outfit_path()?, &mut needed)?;
            continue;
        }
        let mut pattern = fontconfig::Pattern::new(&fc)?;
        pattern.add_string(fontconfig::FC_FAMILY, &CString::new(*family)?)?;
        pattern.add_integer(c"weight", unsafe { FcWeightFromOpenType((*weight).into()) })?;
        let matched = pattern.font_match()?;
        load(
            &mut db,
            &mut paths,
            PathBuf::from(matched.filename()?),
            &mut needed,
        )?;
    }
    while let Some(ch) = needed.pop_first() {
        let mut pattern = fontconfig::Pattern::new(&fc)?;
        let fallback_family = families
            .iter()
            .map(|family| family.0)
            .find(|family| !family.eq_ignore_ascii_case("Outfit"))
            .unwrap_or("sans-serif");
        pattern.add_string(fontconfig::FC_FAMILY, &CString::new(fallback_family)?)?;
        let mut charset = fontconfig::CharSet::new(&fc)?;
        charset.add_char(ch)?;
        pattern.add_charset(charset)?;
        let matched = pattern.font_match()?;
        load(
            &mut db,
            &mut paths,
            PathBuf::from(matched.filename()?),
            &mut needed,
        )?;
    }
    if bead::core::profile::enabled() {
        println!(
            "PROFILE stage=font_select ms={:.3} files={} faces={}",
            start.elapsed().as_secs_f64() * 1000.,
            paths.len(),
            db.len()
        );
    }
    Ok(paths.into_iter().collect())
}
fn load(
    db: &mut fontdb::Database,
    paths: &mut BTreeSet<PathBuf>,
    path: PathBuf,
    needed: &mut BTreeSet<char>,
) -> Result<()> {
    if paths.insert(path.clone()) {
        db.load_font_file(path)?;
        for face in db.faces() {
            db.with_face_data(face.id, |bytes, index| {
                if let Ok(font) = skrifa::FontRef::from_index(bytes, index) {
                    let map = font.charmap();
                    needed.retain(|ch| map.map(*ch).is_none());
                }
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::render::{
        CpuRenderer, Damage, PixelFormat, Primitive, Rect, Renderer, Scene, ShmTarget, TextAlign,
    };

    #[test]
    fn bundled_outfit_thin_digit_has_solid_ink_inside_the_stem() {
        let mut scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 240.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 240., 180.),
            primitives: vec![Primitive::Text {
                rect: Rect::new(0., 0., 240., 180.),
                text: "1".into(),
                family: "Outfit".into(),
                size: 120.,
                color: [238, 246, 243, 255],
                weight: 200,
                align: Some(TextAlign::Center),
                shadow: None,
            }],
        };
        scene.font_files = Selector::default().select(&scene).unwrap();
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, 1.).unwrap();
        let mut pixels = vec![0; 240 * 180 * 4];
        renderer
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut pixels,
                    width: 240,
                    height: 180,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                Damage::full(240, 180),
            )
            .unwrap();
        let mut best = None;
        for (y, row) in pixels.as_chunks::<960>().0.iter().enumerate() {
            let mut start = None;
            for x in 0..=240 {
                let ink = row.get(x * 4 + 3).is_some_and(|alpha| *alpha > 220);
                if ink && start.is_none() {
                    start = Some(x);
                } else if !ink && let Some(begin) = start.take() {
                    let length = x - begin;
                    if best.is_none_or(|(_, _, largest)| length > largest) {
                        best = Some((begin, y, length));
                    }
                }
            }
        }
        let (begin, y, run) = best.expect("the Outfit digit must contain fully covered pixels");
        assert!(
            run >= 2,
            "digit has no solid interior scanline, longest run {run}"
        );
        let x = begin + run / 2;
        let center = &pixels[(y * 240 + x) * 4..][..4];
        assert!(
            center[3] > 220,
            "digit center pixel ({x},{y}) alpha {}",
            center[3]
        );
        assert!(
            center[0] > 210 && center[1] > 220 && center[2] > 215,
            "digit center pixel ({x},{y}) is not Outfit text color: {center:?}"
        );
    }

    #[test]
    fn fresh_selectors_and_text_meters_reuse_one_font_selection() {
        let family = "Nacre Font Cache Probe".to_owned();
        let probe = vec![PathBuf::from("/nacre-font-cache-probe.ttf")];
        let key = (
            BTreeSet::from([(family.clone(), 400)]),
            ('a'..='z').collect(),
        );
        selections().insert(key, probe.clone(), 64).unwrap();
        let scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 100.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 100., 20.),
            primitives: vec![Primitive::Text {
                rect: Rect::new(0., 0., 100., 20.),
                text: "cache".into(),
                family,
                size: 16.,
                color: [0; 4],
                weight: 400,
                align: None,
                shadow: None,
            }],
        };
        // A panel reopened with a new runtime, and the session's text-meter picker, hit the
        // shared entry instead of asking Fontconfig (which knows no such family).
        assert_eq!(Selector::default().select(&scene).unwrap(), probe);
        assert_eq!(select(&scene).unwrap(), probe);
    }
}
