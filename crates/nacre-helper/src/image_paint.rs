//! Integer raster geometry and RGBA composition matching Pillow's existing output.
//! Algorithms follow Pillow 12.3.0 Draw.c/AlphaComposite.c; see ../LICENSE-Pillow.
use crate::Result;
use image::{GrayImage, Luma, Rgba, RgbaImage};
fn bounds(size: (u32, u32)) -> Result<()> {
    if size.0 == 0
        || size.1 == 0
        || u64::from(size.0) * u64::from(size.1) > orient_decoder::MAX_PIXELS
    {
        return Err("Invalid raster bounds".into());
    }
    Ok(())
}
pub fn ellipse_mask(size: (u32, u32)) -> Result<GrayImage> {
    bounds(size)?;
    let mut image = GrayImage::new(size.0, size.1);
    let a = i64::from(size.0) - 1;
    let b = i64::from(size.1) - 1;
    if a + b == 0 {
        return Ok(image);
    }
    let (mut x, mut y) = (a, b % 2);
    let (a2, b2) = (a * a, b * b);
    let delta = |x: i64, y: i64| (a2 * y * y + b2 * x * x - a2 * b2).abs();
    let mut rows = std::collections::BTreeMap::new();
    loop {
        rows.entry(y).or_insert(x);
        if x == a % 2 && y == b {
            break;
        }
        let (mut nx, mut ny) = (x, y + 2);
        let mut best = delta(nx, ny);
        if nx > 1 {
            let next = delta(x - 2, y + 2);
            if best > next {
                nx = x - 2;
                ny = y + 2;
                best = next;
            }
            if best > delta(x - 2, y) {
                nx = x - 2;
                ny = y;
            }
        }
        x = nx;
        y = ny;
    }
    for (y, radius) in rows {
        for row in [(b - y) / 2, (b + y) / 2] {
            for column in (a - radius) / 2..=(a + radius) / 2 {
                image[(column as u32, row as u32)] = Luma([255]);
            }
        }
    }
    Ok(image)
}
pub fn rounded_mask(size: (u32, u32), radius: u32) -> Result<GrayImage> {
    bounds(size)?;
    let mut diameter = i64::from(radius) * 2;
    let right = i64::from(size.0) - 1;
    let bottom = i64::from(size.1) - 1;
    diameter = diameter.min(right).min(bottom);
    let full_x = diameter >= right - 1;
    if full_x {
        diameter = right;
    }
    let full_y = diameter >= bottom - 1;
    if full_y {
        diameter = bottom;
    }
    if full_x && full_y {
        return ellipse_mask(size);
    }
    if diameter == 0 {
        return Ok(GrayImage::from_pixel(size.0, size.1, Luma([255])));
    }
    let r = diameter / 2;
    let corner = ellipse_mask((diameter as u32 + 1, diameter as u32 + 1))?;
    let mut result = GrayImage::new(size.0, size.1);
    for y in 0..=bottom {
        for x in 0..=right {
            let sample = if full_x {
                if y <= r {
                    Some((x, y))
                } else if y >= bottom - r {
                    Some((x, y - bottom + diameter))
                } else {
                    None
                }
            } else if full_y {
                if x <= r {
                    Some((x, y))
                } else if x >= right - r {
                    Some((x - right + diameter, y))
                } else {
                    None
                }
            } else if x <= r && y <= r {
                Some((x, y))
            } else if x >= right - r && y <= r {
                Some((x - right + diameter, y))
            } else if x <= r && y >= bottom - r {
                Some((x, y - bottom + diameter))
            } else if x >= right - r && y >= bottom - r {
                Some((x - right + diameter, y - bottom + diameter))
            } else {
                None
            };
            result[(x as u32, y as u32)] =
                sample.map_or(Luma([255]), |(sx, sy)| corner[(sx as u32, sy as u32)]);
        }
    }
    Ok(result)
}
#[derive(Clone, Copy)]
struct Edge {
    x0: i32,
    y0: i32,
    xmin: i32,
    xmax: i32,
    ymin: i32,
    ymax: i32,
    dx: f32,
}
impl Edge {
    fn new((x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> Self {
        Self {
            x0,
            y0,
            xmin: x0.min(x1),
            xmax: x0.max(x1),
            ymin: y0.min(y1),
            ymax: y0.max(y1),
            dx: if y0 == y1 {
                0.0
            } else {
                (x1 - x0) as f32 / (y1 - y0) as f32
            },
        }
    }
    fn at(&self, y: i32) -> f32 {
        (y - self.y0) as f32 * self.dx + self.x0 as f32
    }
}
fn polygon_spans(size: (u32, u32), points: &[(i32, i32)]) -> Vec<(i32, i32, i32)> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut edges: Vec<Edge> = Vec::new();
    for i in 0..points.len() - 1 {
        let a = points[i];
        let b = points[i + 1];
        if a.1 == b.1 && i != 0 && a.1 == points[i - 1].1 {
            if b.0 > a.0 && a.0 > points[i - 1].0 {
                if let Some(edge) = edges.last_mut() {
                    edge.xmax = b.0;
                    continue;
                }
            } else if b.0 < a.0 && a.0 < points[i - 1].0 {
                if let Some(edge) = edges.last_mut() {
                    edge.xmin = b.0;
                    continue;
                }
            }
        }
        edges.push(Edge::new(a, b));
    }
    if points.last() != points.first() {
        edges.push(Edge::new(*points.last().unwrap(), points[0]));
    }
    let mut spans = Vec::new();
    let mut ymin = size.1 as i32 - 1;
    let mut ymax = 0;
    for edge in &edges {
        ymin = ymin.min(edge.ymin);
        ymax = ymax.max(edge.ymax);
        if edge.ymin == edge.ymax {
            spans.push((edge.ymin, edge.xmin, edge.xmax));
        }
    }
    edges.retain(|edge| edge.ymin != edge.ymax);
    ymin = ymin.max(0);
    ymax = ymax.min(size.1 as i32);
    for y in ymin..=ymax {
        let mut xs = Vec::new();
        for (i, edge) in edges.iter().enumerate() {
            if y < edge.ymin || y > edge.ymax {
                continue;
            }
            let mut value = edge.at(y);
            if y == edge.ymax && y < ymax {
                xs.push(value);
            } else if (y == edge.ymin || y == edge.ymax) && edge.dx != 0.0 {
                for other in &edges[..i] {
                    if (y != other.ymin && y != other.ymax) || other.dx == 0.0 {
                        continue;
                    }
                    if value.round() != other.at(y).round() {
                        continue;
                    }
                    let offset = if y == edge.ymax { -1 } else { 1 };
                    let adjacent = edge.at(y + offset);
                    if y + offset >= other.ymin && y + offset <= other.ymax {
                        let adjacent_other = other.at(y + offset);
                        if value > adjacent + 1.0 && value > adjacent_other + 1.0 {
                            value = adjacent.max(adjacent_other).round() + 1.0;
                        } else if value < adjacent - 1.0 && value < adjacent_other - 1.0 {
                            value = adjacent.min(adjacent_other).round() - 1.0;
                        }
                        break;
                    }
                }
            }
            xs.push(value);
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.chunks_exact(2) {
            let left = pair[0].round() as i32;
            let v = pair[1];
            let right = if v >= 0.0 {
                (v - 0.5).ceil() as i32
            } else {
                -((v.abs() - 0.5).ceil() as i32)
            };
            spans.push((y, left, right));
        }
    }
    spans
}
pub fn polygon_mask(size: (u32, u32), points: &[(f64, f64)]) -> Result<GrayImage> {
    bounds(size)?;
    let mut result = GrayImage::new(size.0, size.1);
    let points: Vec<_> = points.iter().map(|(x, y)| (*x as i32, *y as i32)).collect();
    for (y, left, right) in polygon_spans(size, &points) {
        if y < 0 || y >= size.1 as i32 {
            continue;
        }
        for x in left.max(0)..=right.min(size.0 as i32 - 1) {
            result[(x as u32, y as u32)] = Luma([255]);
        }
    }
    Ok(result)
}
pub fn avatar_mask(size: u32) -> Result<GrayImage> {
    let s = f64::from(size);
    let vertices: Vec<_> = (0..5)
        .map(|i| {
            let angle = f64::from(i) * 2.0 * std::f64::consts::PI / 5.0;
            (
                s / 2.0 + s * 0.49 * angle.sin(),
                s / 2.0 - s * 0.49 * angle.cos(),
            )
        })
        .collect();
    let mut points = Vec::new();
    for i in 0..5 {
        let v = vertices[i];
        let previous = vertices[(i + 4) % 5];
        let next = vertices[(i + 1) % 5];
        let start = (
            v.0 + 0.18 * (previous.0 - v.0),
            v.1 + 0.18 * (previous.1 - v.1),
        );
        let end = (v.0 + 0.18 * (next.0 - v.0), v.1 + 0.18 * (next.1 - v.1));
        for step in 0..17 {
            let t = f64::from(step) / 16.0;
            points.push((
                (1.0 - t).powi(2) * start.0 + 2.0 * (1.0 - t) * t * v.0 + t * t * end.0,
                (1.0 - t).powi(2) * start.1 + 2.0 * (1.0 - t) * t * v.1 + t * t * end.1,
            ));
        }
    }
    polygon_mask((size, size), &points)
}
pub fn composite(
    destination: &mut RgbaImage,
    source: &RgbaImage,
    position: (i32, i32),
    mask: Option<&GrayImage>,
) {
    for (x, y, pixel) in source.enumerate_pixels() {
        let dx = position.0 + i32::try_from(x).unwrap_or(i32::MAX);
        let dy = position.1 + i32::try_from(y).unwrap_or(i32::MAX);
        if dx < 0 || dy < 0 || dx >= destination.width() as i32 || dy >= destination.height() as i32
        {
            continue;
        }
        let mut source = pixel.0;
        if let Some(mask) = mask {
            source[3] = (u16::from(source[3]) * u16::from(mask[(x, y)][0]) / 255) as u8;
        }
        if source[3] == 0 {
            continue;
        }
        let target = &mut destination[(dx as u32, dy as u32)].0;
        let blend = u32::from(target[3]) * (255 - u32::from(source[3]));
        let alpha = u32::from(source[3]) * 255 + blend;
        let first = u32::from(source[3]) * 255 * 255 * 128 / alpha;
        let second = 255 * 128 - first;
        for i in 0..3 {
            let value = u32::from(source[i]) * first + u32::from(target[i]) * second + (128 << 7);
            target[i] = (((value >> 8) + value) >> 8 >> 7) as u8;
        }
        let value = alpha + 128;
        target[3] = (((value >> 8) + value) >> 8) as u8;
    }
}
pub fn paste(
    destination: &mut RgbaImage,
    source: &RgbaImage,
    position: (i32, i32),
    mask: &GrayImage,
) {
    for (x, y, pixel) in source.enumerate_pixels() {
        let dx = position.0 + x as i32;
        let dy = position.1 + y as i32;
        if dx < 0 || dy < 0 || dx >= destination.width() as i32 || dy >= destination.height() as i32
        {
            continue;
        }
        let alpha = u32::from(mask[(x, y)][0]);
        let target = &mut destination[(dx as u32, dy as u32)].0;
        for i in 0..4 {
            let value = u32::from(target[i]) * (255 - alpha) + u32::from(pixel[i]) * alpha + 128;
            target[i] = (((value >> 8) + value) >> 8) as u8;
        }
    }
}
pub fn text(
    destination: &mut RgbaImage,
    mask: &crate::image_text::Mask,
    position: (i32, i32),
    color: [u8; 3],
) {
    for (x, y, value) in mask.pixels.enumerate_pixels() {
        let dx = position.0 + mask.offset.0 + x as i32;
        let dy = position.1 + mask.offset.1 + y as i32;
        if dx < 0 || dy < 0 || dx >= destination.width() as i32 || dy >= destination.height() as i32
        {
            continue;
        }
        let coverage = value[0];
        if coverage == 0 {
            continue;
        }
        let target = &mut destination[(dx as u32, dy as u32)].0;
        for i in 0..4 {
            let source = if i == 3 { 255 } else { color[i] };
            let value = u32::from(target[i]) * (255 - u32::from(coverage))
                + u32::from(source) * u32::from(coverage)
                + 128;
            target[i] = (((value >> 8) + value) >> 8) as u8;
        }
    }
}
pub fn polygon(destination: &mut RgbaImage, points: &[(f64, f64)], color: Rgba<u8>) -> Result<()> {
    let mask = polygon_mask(destination.dimensions(), points)?;
    for (x, y, value) in mask.enumerate_pixels() {
        if value[0] != 0 {
            destination[(x, y)] = color;
        }
    }
    Ok(())
}
pub fn ellipse(
    destination: &mut RgbaImage,
    box_: (i32, i32, i32, i32),
    color: Rgba<u8>,
) -> Result<()> {
    let mask = ellipse_mask(((box_.2 - box_.0 + 1) as u32, (box_.3 - box_.1 + 1) as u32))?;
    for (x, y, value) in mask.enumerate_pixels() {
        let x = box_.0 + x as i32;
        let y = box_.1 + y as i32;
        if value[0] != 0
            && x >= 0
            && y >= 0
            && x < destination.width() as i32
            && y < destination.height() as i32
        {
            destination[(x as u32, y as u32)] = color;
        }
    }
    Ok(())
}
pub fn vertical_line(
    destination: &mut RgbaImage,
    x: i32,
    y0: i32,
    y1: i32,
    width: u32,
    color: Rgba<u8>,
) {
    let left = x - (width as i32 - 1) / 2;
    for y in y0.min(y1)..=y0.max(y1) {
        for x in left..left + width as i32 {
            if x >= 0 && y >= 0 && x < destination.width() as i32 && y < destination.height() as i32
            {
                destination[(x as u32, y as u32)] = color;
            }
        }
    }
}

/// Keep existing preview JPEG bytes while using native image buffers/codec APIs.
pub fn encode_jpeg(image: &image::RgbImage, quality: i32) -> Result<Vec<u8>> {
    bounds(image.dimensions())?;
    let mut compressor = turbojpeg::Compressor::new()?;
    compressor.set_quality(quality)?;
    compressor.set_subsamp(turbojpeg::Subsamp::Sub2x2)?;
    compressor.set_optimize(false)?;
    compressor.set_progressive(false)?;
    compressor.set_arithmetic(false)?;
    Ok(compressor.compress_to_vec(turbojpeg::Image {
        pixels: image.as_raw().as_slice(),
        width: image.width() as usize,
        height: image.height() as usize,
        pitch: image.width() as usize * 3,
        format: turbojpeg::PixelFormat::RGB,
    })?)
}

pub fn fit(
    picture: &RgbaImage,
    size: (u32, u32),
    filter: orient_decoder::Filter,
) -> Result<RgbaImage> {
    let (width, height) = (f64::from(picture.width()), f64::from(picture.height()));
    let live_ratio = width / height;
    let target_ratio = f64::from(size.0) / f64::from(size.1);
    let crop = if live_ratio == target_ratio {
        (width, height)
    } else if live_ratio >= target_ratio {
        (target_ratio * height, height)
    } else {
        (width, width / target_ratio)
    };
    let left = (width - crop.0) * 0.5;
    let top = (height - crop.1) * 0.5;
    orient_decoder::resize_rgba_region(
        picture,
        size,
        (left, top, left + crop.0, top + crop.1),
        filter,
    )
}

/// Preserve the palette CLI's explicit square JPEG draft before EXIF/thumbnailing.
/// Uses the same native TurboJPEG API as Orient; no decoder subprocess is started.
pub fn thumbnail_source(path: &std::path::Path, maximum: usize) -> Result<image::RgbImage> {
    use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
    if maximum == 0 {
        return Err("Invalid JPEG draft bound".into());
    }
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    if reader.format() != Some(ImageFormat::Jpeg) {
        return Ok(DynamicImage::ImageRgba8(orient_decoder::load_rgba_oriented(path)?).to_rgb8());
    }
    let mut metadata = reader.into_decoder()?;
    let orientation = metadata.orientation()?;
    let bytes = std::fs::read(path)?;
    let mut decoder = turbojpeg::Decompressor::new()?;
    let header = decoder.read_header(&bytes)?;
    if header.width as u64 * header.height as u64 > orient_decoder::MAX_PIXELS {
        return Err("Image exceeds the native pixel limit".into());
    }
    let ratio = (header.width / maximum).min(header.height / maximum);
    let scale = if ratio >= 8 {
        turbojpeg::ScalingFactor::ONE_EIGHTH
    } else if ratio >= 4 {
        turbojpeg::ScalingFactor::ONE_QUARTER
    } else if ratio >= 2 {
        turbojpeg::ScalingFactor::ONE_HALF
    } else {
        turbojpeg::ScalingFactor::ONE
    };
    decoder.set_scaling_factor(scale)?;
    let cmyk = matches!(
        header.colorspace,
        turbojpeg::Colorspace::CMYK | turbojpeg::Colorspace::YCCK
    );
    let header = header.scaled(scale);
    let channels = if cmyk { 4 } else { 3 };
    let mut image = turbojpeg::Image {
        pixels: vec![0; header.width * header.height * channels],
        width: header.width,
        height: header.height,
        pitch: header.width * channels,
        format: if cmyk {
            turbojpeg::PixelFormat::CMYK
        } else {
            turbojpeg::PixelFormat::RGB
        },
    };
    decoder.decompress(&bytes, image.as_deref_mut())?;
    let pixels = if cmyk {
        image
            .pixels
            .chunks_exact(4)
            .flat_map(|pixel| {
                let black = u32::from(pixel[3]);
                [0, 1, 2].map(|i| {
                    let product = u32::from(255 - pixel[i]) * black + 128;
                    (black - (((product >> 8) + product) >> 8)) as u8
                })
            })
            .collect()
    } else {
        image.pixels
    };
    let rgb =
        image::RgbImage::from_raw(header.width.try_into()?, header.height.try_into()?, pixels)
            .ok_or("Invalid JPEG draft dimensions")?;
    let mut image = DynamicImage::ImageRgb8(rgb);
    image.apply_orientation(orientation);
    Ok(image.to_rgb8())
}
