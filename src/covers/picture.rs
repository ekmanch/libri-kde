//! The image work behind an upload: the picked picture is decoded (with
//! its EXIF turn applied), scaled so its longer side is at most 1000 px
//! and saved as JPEG. Fit your cover makes a cover of the type's shape
//! (2:3, or square for music) from a picture of another shape: Crop
//! keeps the largest frame of that shape, placed along the picture's
//! long side, and Whole image puts all of it on a blurred copy of itself.
//! Files committed by hand never come through here.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage, imageops};

use crate::github::blob_sha;

/// The longer side of an upload, at most.
pub const LONG_SIDE: u32 = 1000;
/// The JPEG quality of an upload.
const QUALITY: u8 = 88;
/// How far a picture's shape may be from the type's before its tile
/// offers Adjust: a tenth, either way.
const TOLERANCE: f32 = 0.10;
/// How much of its brightness the blurred copy behind a whole image
/// keeps, as the poster frames darken theirs.
const BACKDROP_LIGHT: f32 = 0.6;

/// A picture ready to be stored: the JPEG, its size and git's blob sha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub sha: String,
}

/// How Fit your cover makes the shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    #[default]
    Crop,
    Whole,
}

/// The shape a cover of the type has, width over height.
pub fn shape(square: bool) -> f32 {
    if square { 1.0 } else { 2.0 / 3.0 }
}

/// Whether a picture of `width` × `height` is clearly not the type's
/// shape, so a center crop would lose a good part of it.
pub fn off_shape(width: u32, height: u32, square: bool) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    let ratio = width as f32 / height as f32;
    let target = shape(square);
    (ratio / target - 1.0).abs() > TOLERANCE
}

/// The largest frame of the type's shape inside a picture of `width` ×
/// `height`, placed `at` (0 to 1) along the side with room to spare: x,
/// y, width, height.
pub fn crop_frame(width: u32, height: u32, square: bool, at: f32) -> (u32, u32, u32, u32) {
    let target = shape(square);
    let at = at.clamp(0.0, 1.0);
    if width as f32 / height.max(1) as f32 > target {
        // Wider than the shape: the frame keeps the height and slides
        // sideways.
        let w = ((height as f32 * target).round() as u32).clamp(1, width);
        let x = ((width - w) as f32 * at).round() as u32;
        (x, 0, w, height)
    } else {
        let h = ((width as f32 / target).round() as u32).clamp(1, height.max(1));
        let y = ((height.saturating_sub(h)) as f32 * at).round() as u32;
        (0, y, width, h)
    }
}

/// The size Fit your cover saves a picture of `width` × `height` at.
pub fn fitted_size(width: u32, height: u32, square: bool, fit: Fit) -> (u32, u32) {
    let (max_w, max_h) = if square {
        (LONG_SIDE, LONG_SIDE)
    } else {
        (LONG_SIDE, LONG_SIDE * 3 / 2)
    };
    match fit {
        Fit::Crop => {
            let (_, _, w, h) = crop_frame(width, height, square, 0.5);
            within(w, h, max_w, max_h)
        }
        Fit::Whole => {
            let (w, h) = within(width, height, max_w, max_h);
            // The smallest frame of the shape that holds all of it.
            let target = shape(square);
            if w as f32 / h.max(1) as f32 > target {
                (w, ((w as f32 / target).round() as u32).max(1))
            } else {
                (((h as f32 * target).round() as u32).max(1), h)
            }
        }
    }
}

/// `width` × `height` scaled down to fit in `max_w` × `max_h`, never up.
fn within(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let scale = (max_w as f32 / width.max(1) as f32)
        .min(max_h as f32 / height.max(1) as f32)
        .min(1.0);
    (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    )
}

/// The picture in `bytes`, upright.
fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "This file is not an image Libri can read.".to_owned())?;
    let orientation = decoder.orientation().ok();
    let mut picture = DynamicImage::from_decoder(decoder)
        .map_err(|_| "This image could not be read.".to_owned())?;
    if let Some(orientation) = orientation {
        picture.apply_orientation(orientation);
    }
    if picture.width() == 0 || picture.height() == 0 {
        return Err("This image is empty.".into());
    }
    Ok(picture)
}

fn encode(picture: &RgbImage) -> Result<Prepared, String> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, QUALITY)
        .encode_image(picture)
        .map_err(|error| error.to_string())?;
    let sha = blob_sha(&bytes);
    Ok(Prepared {
        bytes,
        width: picture.width(),
        height: picture.height(),
        sha,
    })
}

/// The width and height of the picture in `bytes`, upright.
pub fn size_of(bytes: &[u8]) -> Option<(u32, u32)> {
    decode(bytes).ok().map(|p| (p.width(), p.height()))
}

/// An upload: the picture scaled so its longer side is at most
/// `LONG_SIDE`, as JPEG. A see-through picture is put on white.
pub fn prepare(bytes: &[u8]) -> Result<Prepared, String> {
    let picture = decode(bytes)?;
    let picture = if picture.width().max(picture.height()) > LONG_SIDE {
        picture.resize(LONG_SIDE, LONG_SIDE, FilterType::Lanczos3)
    } else {
        picture
    };
    encode(&flatten(&picture))
}

/// Fit your cover: the picture in `bytes` in the type's shape, cropped
/// with the frame `at` along its long side, or whole on a blurred copy
/// of itself.
pub fn fit(bytes: &[u8], mode: Fit, at: f32, square: bool) -> Result<Prepared, String> {
    let picture = decode(bytes)?;
    let (width, height) = (picture.width(), picture.height());
    let (out_w, out_h) = fitted_size(width, height, square, mode);
    match mode {
        Fit::Crop => {
            let (x, y, w, h) = crop_frame(width, height, square, at);
            let cropped = picture.crop_imm(x, y, w, h);
            let scaled = if (w, h) == (out_w, out_h) {
                cropped
            } else {
                cropped.resize_exact(out_w, out_h, FilterType::Lanczos3)
            };
            encode(&flatten(&scaled))
        }
        Fit::Whole => {
            let front = flatten(&picture);
            // The backdrop: a small copy filling the frame, blurred, then
            // stretched back, which keeps it soft and quick.
            let small_w = (out_w / 12).max(8);
            let small_h = (out_h / 12).max(8);
            let small = DynamicImage::ImageRgb8(front.clone())
                .resize_to_fill(small_w, small_h, FilterType::Triangle)
                .fast_blur(3.0);
            let mut canvas = small
                .resize_exact(out_w, out_h, FilterType::Triangle)
                .to_rgb8();
            for pixel in canvas.pixels_mut() {
                for channel in &mut pixel.0 {
                    *channel = (f32::from(*channel) * BACKDROP_LIGHT) as u8;
                }
            }
            let (w, h) = within(width, height, out_w, out_h);
            let scaled = if (w, h) == (width, height) {
                front
            } else {
                imageops::resize(&front, w, h, FilterType::Lanczos3)
            };
            let x = i64::from((out_w - w.min(out_w)) / 2);
            let y = i64::from((out_h - h.min(out_h)) / 2);
            imageops::overlay(&mut canvas, &scaled, x, y);
            encode(&canvas)
        }
    }
}

/// The picture without its alpha, on white where it was see-through.
fn flatten(picture: &DynamicImage) -> RgbImage {
    if !picture.color().has_alpha() {
        return picture.to_rgb8();
    }
    let rgba = picture.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (to, from) in out.pixels_mut().zip(rgba.pixels()) {
        let alpha = u32::from(from.0[3]);
        for channel in 0..3 {
            let value = u32::from(from.0[channel]) * alpha + 255 * (255 - alpha);
            to.0[channel] = (value / 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG of `width` × `height`: red on the left half, blue on the
    /// right.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = RgbImage::from_fn(width, height, |x, _| {
            if x < width / 2 {
                image::Rgb([220, 30, 30])
            } else {
                image::Rgb([30, 30, 220])
            }
        });
        let mut bytes = Vec::new();
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn an_upload_is_scaled_to_1000_px_and_saved_as_jpeg() {
        let prepared = prepare(&png(3000, 2000)).unwrap();
        assert_eq!((prepared.width, prepared.height), (1000, 667));
        assert!(prepared.bytes.starts_with(&[0xFF, 0xD8, 0xFF]), "a JPEG");
        assert_eq!(prepared.sha, blob_sha(&prepared.bytes));
        // A small picture is never grown.
        let small = prepare(&png(300, 450)).unwrap();
        assert_eq!((small.width, small.height), (300, 450));
        assert!(prepare(b"not an image").is_err());
    }

    #[test]
    fn only_a_clearly_different_shape_is_off() {
        assert!(!off_shape(1000, 1500, false));
        assert!(!off_shape(1000, 1400, false), "within a tenth of 2:3");
        assert!(off_shape(1920, 1080, false));
        assert!(off_shape(1000, 1000, false));
        assert!(!off_shape(1000, 1000, true));
        assert!(!off_shape(1000, 950, true));
        assert!(off_shape(1000, 1500, true));
    }

    #[test]
    fn the_crop_frame_slides_along_the_long_side() {
        // A wide picture: the frame keeps the height.
        assert_eq!(crop_frame(1920, 1080, false, 0.0), (0, 0, 720, 1080));
        assert_eq!(crop_frame(1920, 1080, false, 1.0), (1200, 0, 720, 1080));
        assert_eq!(crop_frame(1920, 1080, false, 0.5), (600, 0, 720, 1080));
        // A tall one: it keeps the width and slides up and down.
        assert_eq!(crop_frame(1000, 3000, false, 0.5), (0, 750, 1000, 1500));
        assert_eq!(crop_frame(1000, 1500, true, 1.0), (0, 500, 1000, 1000));
    }

    #[test]
    fn a_fitted_cover_has_the_types_shape() {
        let wide = png(3840, 2160);
        let cropped = fit(&wide, Fit::Crop, 0.0, false).unwrap();
        assert_eq!((cropped.width, cropped.height), (1000, 1500));
        // The left end of the picture is red.
        let decoded = decode(&cropped.bytes).unwrap().to_rgb8();
        let pixel = decoded.get_pixel(500, 750).0;
        assert!(pixel[0] > 150 && pixel[2] < 100, "{pixel:?}");

        let whole = fit(&wide, Fit::Whole, 0.5, false).unwrap();
        assert_eq!((whole.width, whole.height), (1000, 1500));
        // All of the picture is there, across the middle: red on the
        // left, blue on the right, and the darkened backdrop above.
        let decoded = decode(&whole.bytes).unwrap().to_rgb8();
        assert!(decoded.get_pixel(100, 750).0[0] > 150);
        assert!(decoded.get_pixel(900, 750).0[2] > 150);
        assert!(decoded.get_pixel(100, 100).0[0] < 150);

        // Square for music, and a small picture is not grown.
        let square = fit(&png(600, 400), Fit::Crop, 0.5, true).unwrap();
        assert_eq!((square.width, square.height), (400, 400));
        assert_eq!(fitted_size(600, 400, true, Fit::Whole), (600, 600));
        assert_eq!(fitted_size(1920, 1080, false, Fit::Crop), (720, 1080));
    }
}
