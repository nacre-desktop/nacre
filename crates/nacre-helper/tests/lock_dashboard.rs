use base64::{engine::general_purpose::STANDARD, Engine};
use nacre_helper::{image_text::Font, Result};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

fn freetype_version() -> Result<(i32, i32, i32)> {
    unsafe extern "C" {
        fn FT_Library_Version(
            library: freetype::ffi::FT_Library,
            major: *mut i32,
            minor: *mut i32,
            patch: *mut i32,
        );
    }
    let library = freetype::Library::init()?;
    let (mut major, mut minor, mut patch) = (0, 0, 0);
    // SAFETY: the library stays live and the three output pointers are valid
    // distinct FT_Int values for this stable public FreeType query.
    unsafe { FT_Library_Version(library.raw(), &mut major, &mut minor, &mut patch) };
    Ok((major, minor, patch))
}

// FreeType 2.15 intentionally changed v40 hinting. Keep the immutable masks on
// their compatible renderer, while the separate live parity test checks the
// current system renderer. No installed code uses this test-only bootstrap.
fn frozen_renderer(name: &str) -> Result<bool> {
    let spec: Value = serde_json::from_str(include_str!("support/font-renderer.json"))?;
    let expected: Vec<i32> = serde_json::from_value(spec["version"].clone())?;
    let actual = freetype_version()?;
    if expected == [actual.0, actual.1, actual.2] {
        return Ok(false);
    }
    if std::env::var_os("NACRE_FROZEN_FONT_CHILD").is_some() {
        return Err("Frozen font child did not load the reference renderer".into());
    }
    let setup = Command::new("python3")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/prepare_freetype.py"))
        .output()?;
    if !setup.status.success() {
        return Err(format!(
            "Frozen font setup failed: {}",
            String::from_utf8_lossy(&setup.stderr)
        )
        .into());
    }
    let libdir = String::from_utf8(setup.stdout)?.trim().to_owned();
    let mut paths = vec![std::path::PathBuf::from(libdir)];
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let result = Command::new(std::env::current_exe()?)
        .args(["--exact", name, "--nocapture", "--color=never"])
        .env("NACRE_FROZEN_FONT_CHILD", "1")
        .env("LD_LIBRARY_PATH", std::env::join_paths(paths)?)
        .output()?;
    let output = String::from_utf8(result.stdout)?;
    if !result.status.success() || !output.contains("1 passed; 0 failed") {
        return Err(format!(
            "Frozen font child failed: {output}\n{}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(true)
}
fn reference() -> Result<Value> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/lock-dashboard-reference.json.gz").as_slice(),
    )
    .read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
#[test]
fn text_masks_offsets_and_advances_match_frozen_pillow() -> Result<()> {
    if frozen_renderer("text_masks_offsets_and_advances_match_frozen_pillow")? {
        return Ok(());
    }
    let reference = reference()?;
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lock-fonts");
    for case in reference["text_cases"]
        .as_array()
        .ok_or("Missing text cases")?
    {
        let text = case["text"].as_str().ok_or("Missing text")?;
        let name = case["font"].as_str().ok_or("Missing font")?;
        let size = case["size"].as_u64().ok_or("Missing size")? as u32;
        let mut font = Font::open(&fonts.join(name), size)?;
        let mask = font.mask(text)?;
        assert_eq!(
            mask.advance,
            case["advance"].as_f64().ok_or("Missing advance")?,
            "{name} {size} {text}"
        );
        assert_eq!(
            mask.offset,
            (
                case["offset"][0].as_i64().unwrap() as i32,
                case["offset"][1].as_i64().unwrap() as i32
            ),
            "{name} {size} {text}"
        );
        assert_eq!(
            mask.pixels.dimensions(),
            (
                case["mask_size"][0].as_u64().unwrap() as u32,
                case["mask_size"][1].as_u64().unwrap() as u32
            ),
            "{name} {size} {text}"
        );
        let expected = STANDARD.decode(case["mask"].as_str().ok_or("Missing mask")?)?;
        let actual = mask.pixels.into_raw();
        assert!(
            actual == expected,
            "Text mask differs {name} {size} {text}: {} samples",
            actual.iter().zip(&expected).filter(|(a, b)| a != b).count()
        );
    }
    Ok(())
}

#[test]
fn current_system_text_masks_match_pillow_with_the_same_freetype() -> Result<()> {
    let reference = reference()?;
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lock-fonts");
    let cases = reference["text_cases"]
        .as_array()
        .ok_or("Missing text cases")?;
    let input: Vec<Value> = cases
        .iter()
        .map(|case| {
            serde_json::json!({
                "font": fonts.join(case["font"].as_str().unwrap()),
                "size": case["size"], "text": case["text"]
            })
        })
        .collect();
    // PyPI wheels can carry their own FreeType. Compare the two algorithms on
    // the *same* loaded engine rather than silently comparing different versions.
    let library = std::fs::read_to_string("/proc/self/maps")?
        .lines()
        .filter_map(|line| line.split_whitespace().nth(5))
        .find(|path| {
            Path::new(path)
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("libfreetype.so"))
        })
        .ok_or("Cannot identify the system FreeType for live Pillow parity")?
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\134", "\\");
    let script = r#"
import base64, json, sys
from PIL import ImageFont
result = []
for case in json.load(sys.stdin):
    font = ImageFont.truetype(case['font'], case['size'])
    mask, offset = font.getmask2(case['text'], mode='L')
    result.append({'advance':font.getlength(case['text']), 'offset':offset,
        'size':mask.size, 'pixels':base64.b64encode(bytes(mask)).decode()})
print(json.dumps({'version':ImageFont.core.freetype2_version, 'cases':result}))
"#;
    let mut child = Command::new("python3")
        .args(["-c", script])
        .env("LD_PRELOAD", library)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("Missing Pillow input pipe")?
        .write_all(&serde_json::to_vec(&input)?)?;
    let result = child.wait_with_output()?;
    if !result.status.success() {
        return Err(format!(
            "Live Pillow parity failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    let expected: Value = serde_json::from_slice(&result.stdout)?;
    let version = freetype_version()?;
    assert_eq!(
        expected["version"],
        format!("{}.{}.{}", version.0, version.1, version.2)
    );
    let rows = expected["cases"]
        .as_array()
        .ok_or("Missing live Pillow masks")?;
    assert_eq!(rows.len(), cases.len());
    for (case, want) in cases.iter().zip(rows) {
        let mut font = Font::open(
            &fonts.join(case["font"].as_str().unwrap()),
            case["size"].as_u64().unwrap() as u32,
        )?;
        let mask = font.mask(case["text"].as_str().unwrap())?;
        assert_eq!(mask.advance, want["advance"].as_f64().unwrap());
        assert_eq!(
            serde_json::json!([mask.offset.0, mask.offset.1]),
            want["offset"]
        );
        assert_eq!(
            serde_json::json!([mask.pixels.width(), mask.pixels.height()]),
            want["size"]
        );
        assert!(
            mask.pixels.into_raw() == STANDARD.decode(want["pixels"].as_str().unwrap())?,
            "Current Pillow pixels differ for {} {} {}",
            case["font"],
            case["size"],
            case["text"]
        );
    }
    Ok(())
}

#[test]
fn integer_ellipse_rounding_and_avatar_geometry_match_frozen_pillow() -> Result<()> {
    use nacre_helper::image_paint;
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/lock-geometry-reference.json"))?;
    for case in reference["cases"]
        .as_array()
        .ok_or("Missing geometry cases")?
    {
        let image = match case["operation"].as_str().ok_or("Missing operation")? {
            "avatar_mask" => {
                image_paint::avatar_mask(case["size"].as_u64().ok_or("Missing avatar size")? as u32)?
            }
            operation => {
                let size = (
                    case["size"][0].as_u64().ok_or("Missing width")? as u32,
                    case["size"][1].as_u64().ok_or("Missing height")? as u32,
                );
                if operation == "ellipse" {
                    image_paint::ellipse_mask(size)?
                } else {
                    image_paint::rounded_mask(
                        size,
                        case["radius"].as_u64().ok_or("Missing radius")? as u32,
                    )?
                }
            }
        };
        let expected = STANDARD.decode(case["pixels"].as_str().ok_or("Missing pixels")?)?;
        let actual = image.into_raw();
        assert!(
            actual == expected,
            "Geometry differs {} size{} radius{}: {} samples",
            case["operation"],
            case["size"],
            case["radius"],
            actual.iter().zip(&expected).filter(|(a, b)| a != b).count()
        );
    }
    Ok(())
}

#[test]
fn complete_cold_panels_match_frozen_python_pixels_and_png_bytes() -> Result<()> {
    if frozen_renderer("complete_cold_panels_match_frozen_python_pixels_and_png_bytes")? {
        return Ok(());
    }
    use nacre_helper::{image_text::FontBook, lock_dashboard as dashboard, paths::Paths};
    let reference = reference()?;
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lock-fonts");
    for case in reference["cases"].as_array().ok_or("Missing panel cases")? {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let wallpaper = if let Some(key) = case["wallpaper"].as_str() {
            let file = home.path().join("wallpaper.png");
            std::fs::write(
                &file,
                STANDARD.decode(
                    reference["inputs"][key]
                        .as_str()
                        .ok_or("Missing wallpaper input")?,
                )?,
            )?;
            file
        } else {
            std::path::PathBuf::new()
        };
        let mut context = dashboard::Context {
            fonts: FontBook::with_overrides(
                [
                    ("IBM Plex Sans".into(), fonts.join("FixtureSans.ttf")),
                    (
                        "Material Symbols Rounded".into(),
                        fonts.join("FixtureSymbols.ttf"),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
            now: chrono::DateTime::parse_from_rfc3339("2026-07-14T14:30:00+00:00")?,
            system: Some(reference["system_info"].clone()),
        };
        let result = dashboard::render(
            &case["data"],
            case["colors"].as_object().ok_or("Missing colors")?,
            &wallpaper,
            Some(&home.path().join("missing-artwork.png")),
            &paths,
            if case["monitor"].is_null() {
                None
            } else {
                Some(&case["monitor"])
            },
            &mut context,
        )?;
        let expected_png = STANDARD.decode(case["png"].as_str().ok_or("Missing expected PNG")?)?;
        let expected = image::load_from_memory(&expected_png)?.to_rgba8();
        assert_eq!(result.dimensions(), expected.dimensions());
        if result.as_raw() != expected.as_raw() {
            let different = result
                .enumerate_pixels()
                .filter(|(x, y, pixel)| **pixel != expected[(*x, *y)])
                .count();
            let first: Vec<_> = result
                .enumerate_pixels()
                .filter(|(x, y, pixel)| **pixel != expected[(*x, *y)])
                .take(8)
                .map(|(x, y, pixel)| (x, y, pixel.0, expected[(x, y)].0))
                .collect();
            return Err(format!(
                "Cold panel {}: {different} differing pixels; first {first:?}",
                case["name"]
            )
            .into());
        }
        assert!(
            orient_decoder::encode_rgba_png(&result, 2)? == expected_png,
            "Cold panel {} PNG bytes differ",
            case["name"]
        );
    }
    Ok(())
}

#[test]
fn existing_jpeg_preview_bytes_match_pillow_quality_settings() -> Result<()> {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/jpeg-encode-reference.json"))?;
    for case in reference["cases"].as_array().ok_or("Missing JPEG cases")? {
        let size = (
            case["size"][0].as_u64().unwrap() as u32,
            case["size"][1].as_u64().unwrap() as u32,
        );
        let image = image::RgbImage::from_raw(
            size.0,
            size.1,
            STANDARD.decode(case["rgb"].as_str().unwrap())?,
        )
        .ok_or("Invalid RGB pixels")?;
        let actual = nacre_helper::image_paint::encode_jpeg(
            &image,
            case["quality"].as_i64().unwrap() as i32,
        )?;
        let expected = STANDARD.decode(case["jpeg"].as_str().unwrap())?;
        assert!(
            actual == expected,
            "JPEG differs {} quality{}: lengths {}/{}",
            case["size"],
            case["quality"],
            actual.len(),
            expected.len()
        );
    }
    Ok(())
}
