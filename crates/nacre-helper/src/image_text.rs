//! Pillow-compatible FreeType text masks; font lookup uses Fontconfig directly.
//! Pixel placement and coverage composition follow Pillow 12.3.0 _imagingft.c.
//! Pillow's MIT-CMU notice is retained in ../LICENSE-Pillow.
use crate::Result;
use freetype::face::LoadFlag;
use image::GrayImage;
use std::path::Path;
#[derive(Clone, Copy, Debug)]
struct Glyph {
    index: u32,
    x_advance: i32,
    y_advance: i32,
    x_offset: i32,
    y_offset: i32,
}

// This small FFI boundary uses libraqm's stable public glyph ABI. No old bindgen
// dependency or alternate text engine is introduced. Face/text handles stay owned
// for every call; only copied scalar positions leave the boundary.
#[allow(unsafe_code)]
mod shaping {
    use super::{Glyph, Result};
    use std::{
        ffi::{c_char, c_void},
        ptr::NonNull,
    };
    #[repr(C)]
    struct RawGlyph {
        index: u32,
        x_advance: i32,
        y_advance: i32,
        x_offset: i32,
        y_offset: i32,
        cluster: u32,
        face: freetype::ffi::FT_Face,
    }
    #[link(name = "raqm")]
    unsafe extern "C" {
        fn raqm_create() -> *mut c_void;
        fn raqm_destroy(handle: *mut c_void);
        fn raqm_set_text_utf8(handle: *mut c_void, text: *const c_char, length: usize) -> bool;
        fn raqm_set_freetype_face(handle: *mut c_void, face: freetype::ffi::FT_Face) -> bool;
        fn raqm_layout(handle: *mut c_void) -> bool;
        fn raqm_get_glyphs(handle: *mut c_void, length: *mut usize) -> *mut RawGlyph;
    }
    struct Guard(NonNull<c_void>);
    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: this guard exclusively owns a successful raqm_create handle.
            unsafe {
                raqm_destroy(self.0.as_ptr());
            }
        }
    }
    pub(super) fn shape(face: &mut freetype::Face, text: &str) -> Result<Vec<Glyph>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        if text.len() > 16384 {
            return Err("Text layout input is too large".into());
        }
        // SAFETY: create returns an owned handle checked for null. Both the text
        // bytes and live mutable Face outlive the guard, including error paths.
        // libraqm owns its glyph array; we bound count and copy it before Drop.
        unsafe {
            let handle = Guard(NonNull::new(raqm_create()).ok_or("Cannot create text layout")?);
            if !raqm_set_text_utf8(handle.0.as_ptr(), text.as_ptr().cast(), text.len())
                || !raqm_set_freetype_face(handle.0.as_ptr(), face.raw_mut() as *mut _)
                || !raqm_layout(handle.0.as_ptr())
            {
                return Err("Native text layout failed".into());
            }
            let mut count = 0usize;
            let glyphs = raqm_get_glyphs(handle.0.as_ptr(), &mut count);
            if count == 0 {
                return Ok(Vec::new());
            }
            if glyphs.is_null() || count > 65536 {
                return Err("Invalid native text layout result".into());
            }
            Ok(std::slice::from_raw_parts(glyphs, count)
                .iter()
                .map(|g| Glyph {
                    index: g.index,
                    x_advance: g.x_advance,
                    y_advance: g.y_advance,
                    x_offset: g.x_offset,
                    y_offset: g.y_offset,
                })
                .collect())
        }
    }
}
pub struct Font {
    face: freetype::Face,
    _library: freetype::Library,
}
pub struct Mask {
    pub pixels: GrayImage,
    pub offset: (i32, i32),
    pub advance: f64,
}
fn pixel(value: i64) -> i64 {
    (value + 32) >> 6
}
pub(crate) fn multiply(a: u8, b: u8) -> u8 {
    let value = u32::from(a) * u32::from(b) + 128;
    (((value >> 8) + value) >> 8) as u8
}
impl Font {
    pub fn open(path: &Path, pixels: u32) -> Result<Self> {
        if pixels == 0 || pixels > 4096 {
            return Err("Invalid font pixel size".into());
        }
        let library = freetype::Library::init()?;
        let face = library.new_face(path, 0)?;
        face.set_char_size((pixels * 64) as isize, (pixels * 64) as isize, 0, 0)?;
        Ok(Self {
            face,
            _library: library,
        })
    }
    pub fn find(family: &str, style: &str, pixels: u32) -> Result<Self> {
        let config = fontconfig::Fontconfig::new().ok_or("Fontconfig initialization failed")?;
        let font = config
            .find(family, Some(style))
            .map_err(|e| format!("Font lookup failed: {e}"))?;
        // The Python reference passes only fc-match's path to ImageFont.truetype;
        // preserve its default face index and variation selection.
        Self::open(&font.path, pixels)
    }
    pub fn advance(&mut self, text: &str) -> Result<f64> {
        Ok(shaping::shape(&mut self.face, text)?
            .iter()
            .map(|glyph| i64::from(glyph.x_advance))
            .sum::<i64>() as f64
            / 64.0)
    }
    pub fn mask(&mut self, text: &str) -> Result<Mask> {
        let glyphs = shaping::shape(&mut self.face, text)?;
        let (mut x_min, mut x_max, mut y_min, mut y_max) = (0i64, 0i64, 0i64, 0i64);
        let mut position = 0i64;
        for glyph in &glyphs {
            let px = pixel(position + i64::from(glyph.x_offset));
            let py = pixel(i64::from(glyph.y_offset));
            position += i64::from(glyph.x_advance);
            x_max = x_max.max(pixel(position));
            self.face.load_glyph(glyph.index, LoadFlag::DEFAULT)?;
            let bbox = self
                .face
                .glyph()
                .get_glyph()?
                .get_cbox(freetype::ffi::FT_GLYPH_BBOX_PIXELS);
            x_min = x_min.min(bbox.xMin + px);
            x_max = x_max.max(bbox.xMax + px);
            y_min = y_min.min(bbox.yMin + py);
            y_max = y_max.max(bbox.yMax + py);
        }
        let width = u32::try_from(x_max - x_min)?;
        let height = u32::try_from(y_max - y_min)?;
        if u64::from(width) * u64::from(height) > orient_decoder::MAX_PIXELS {
            return Err("Text mask is too large".into());
        }
        let ascender = pixel(
            self.face
                .size_metrics()
                .ok_or("Font size metrics missing")?
                .ascender,
        );
        let mut mask = GrayImage::new(width, height);
        let (mut x, mut y) = (0i64, 0i64);
        for glyph in &glyphs {
            self.face
                .load_glyph(glyph.index, LoadFlag::DEFAULT | LoadFlag::RENDER)?;
            let slot = self.face.glyph();
            let bitmap = slot.bitmap();
            let left = pixel(x + i64::from(glyph.x_offset)) + i64::from(slot.bitmap_left()) - x_min;
            let top = y_max - pixel(y + i64::from(glyph.y_offset)) - i64::from(slot.bitmap_top());
            if bitmap.width() > 0 && bitmap.rows() > 0 {
                if bitmap.pixel_mode() != Ok(freetype::bitmap::PixelMode::Gray) {
                    return Err("Unsupported font bitmap pixel mode".into());
                }
                let pitch = bitmap.pitch().unsigned_abs() as usize;
                for row in 0..bitmap.rows() as u32 {
                    for column in 0..bitmap.width() as u32 {
                        let tx = left + i64::from(column);
                        let ty = top + i64::from(row);
                        if tx < 0 || ty < 0 || tx >= i64::from(width) || ty >= i64::from(height) {
                            continue;
                        }
                        let offset = if bitmap.pitch() < 0 {
                            (bitmap.rows() as usize - 1 - row as usize) * pitch
                        } else {
                            row as usize * pitch
                        } + column as usize;
                        let source = bitmap.buffer()[offset];
                        let target = &mut mask[(tx as u32, ty as u32)][0];
                        *target = (u16::from(source) + u16::from(multiply(*target, 255 - source)))
                            .min(255) as u8;
                    }
                }
            }
            x += i64::from(glyph.x_advance);
            y += i64::from(glyph.y_advance);
        }
        Ok(Mask {
            pixels: mask,
            offset: (x_min as i32, (ascender - y_max) as i32),
            advance: position as f64 / 64.0,
        })
    }
}

/// A bounded counterpart of the existing 16-entry font cache; no idle work.
#[derive(Default)]
pub struct FontBook {
    overrides: std::collections::BTreeMap<String, std::path::PathBuf>,
    entries: Vec<((String, String, u32), u64, Font)>,
    age: u64,
}
impl FontBook {
    pub fn with_overrides(
        overrides: std::collections::BTreeMap<String, std::path::PathBuf>,
    ) -> Self {
        Self {
            overrides,
            ..Default::default()
        }
    }
    pub fn get(&mut self, family: &str, style: &str, size: u32) -> Result<&mut Font> {
        self.age = self.age.wrapping_add(1);
        let key = (family.to_owned(), style.to_owned(), size);
        let index = if let Some(index) = self
            .entries
            .iter()
            .position(|(current, _, _)| *current == key)
        {
            index
        } else {
            if self.entries.len() == 16 {
                let index = self
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (_, age, _))| age)
                    .map(|(index, _)| index)
                    .ok_or("Font cache is empty")?;
                self.entries.remove(index);
            }
            let font = if let Some(path) = self.overrides.get(family) {
                Font::open(path, size)?
            } else {
                Font::find(family, style, size)?
            };
            self.entries.push((key, self.age, font));
            self.entries.len() - 1
        };
        self.entries[index].1 = self.age;
        Ok(&mut self.entries[index].2)
    }
}
