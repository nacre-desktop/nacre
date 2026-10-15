use base64::{engine::general_purpose::STANDARD, Engine};
use nacre_helper::{image_text::Font, Result};
use serde_json::Value;
use std::{io::Read, path::Path};
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
