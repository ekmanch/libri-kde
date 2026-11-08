//! The poster frame's backdrop: an album's square cover in a row of 2:3
//! posters sits on a blurred copy of itself filling the 2:3 tile
//! (`ui::cover::poster_frame`). iced draws no blur, so the copy is made
//! once per image here, on tokio's blocking pool: decoded, shrunk to a
//! small square and blurred, then drawn stretched over the tile, where
//! the GPU's smooth scaling keeps it soft. The darkening is the frame's
//! own (the copy drawn faded over black).

use std::path::Path;

use iced::widget::image::Handle;

/// The shorter side of the blurred copy, in pixels.
const SIDE: u32 = 64;
/// The box blur's radius at `SIDE`, run three times (close to a Gaussian
/// of about 5.5 px, near 18 px over a tile of Home's shelf or Coming
/// soon's grid, where the square fills the 2:3 tile's height).
const RADIUS: usize = 5;
const PASSES: usize = 3;

/// The image file at `path` blurred, or `None` when it does not decode.
pub fn backdrop(path: &Path) -> Option<Handle> {
    let buffer = iced::advanced::graphics::image::load(&Handle::from_path(path)).ok()?;
    let (width, height) = (buffer.width(), buffer.height());
    let (pixels, w, h) = blurred(buffer.as_raw(), width, height)?;
    Some(Handle::from_rgba(w, h, pixels))
}

/// RGBA `pixels` of `width` × `height` shrunk so the shorter side is
/// `SIDE` (never grown) and blurred, with the new width and height.
pub fn blurred(pixels: &[u8], width: u32, height: u32) -> Option<(Vec<u8>, u32, u32)> {
    if width == 0 || height == 0 || pixels.len() < (width * height * 4) as usize {
        return None;
    }
    let scale = (SIDE as f32 / width.min(height) as f32).min(1.0);
    let w = ((width as f32 * scale).round() as u32).max(1);
    let h = ((height as f32 * scale).round() as u32).max(1);
    let mut small = shrink(pixels, width, height, w, h);
    for _ in 0..PASSES {
        box_blur(&mut small, w as usize, h as usize, RADIUS);
    }
    Some((small, w, h))
}

/// The average of the source pixels under each pixel of the smaller image.
fn shrink(pixels: &[u8], width: u32, height: u32, w: u32, h: u32) -> Vec<u8> {
    let (width, height, w, h) = (width as usize, height as usize, w as usize, h as usize);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let (y0, y1) = (
            y * height / h,
            ((y + 1) * height / h).max(y * height / h + 1),
        );
        for x in 0..w {
            let (x0, x1) = (x * width / w, ((x + 1) * width / w).max(x * width / w + 1));
            let mut sum = [0u32; 4];
            for sy in y0..y1.min(height) {
                let row = &pixels[(sy * width + x0) * 4..(sy * width + x1.min(width)) * 4];
                for px in row.as_chunks::<4>().0 {
                    for (s, &v) in sum.iter_mut().zip(px) {
                        *s += u32::from(v);
                    }
                }
            }
            let count = ((y1.min(height) - y0) * (x1.min(width) - x0)).max(1) as u32;
            let at = (y * w + x) * 4;
            for (c, s) in sum.iter().enumerate() {
                out[at + c] = (s / count) as u8;
            }
        }
    }
    out
}

/// One box blur of `radius` across and then down, the edges repeated.
fn box_blur(pixels: &mut [u8], w: usize, h: usize, radius: usize) {
    let mut line = Vec::new();
    for y in 0..h {
        blur_line(pixels, y * w * 4, 4, w, radius, &mut line);
    }
    for x in 0..w {
        blur_line(pixels, x * 4, w * 4, h, radius, &mut line);
    }
}

/// Blurs the `len` pixels starting at `start`, `stride` bytes apart.
fn blur_line(
    pixels: &mut [u8],
    start: usize,
    stride: usize,
    len: usize,
    radius: usize,
    line: &mut Vec<[u8; 4]>,
) {
    line.clear();
    line.extend((0..len).map(|i| {
        let at = start + i * stride;
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    }));
    let span = (2 * radius + 1) as u32;
    let at = |i: isize| line[i.clamp(0, len as isize - 1) as usize];
    let mut sum = [0u32; 4];
    for i in -(radius as isize)..=radius as isize {
        for (s, v) in sum.iter_mut().zip(at(i)) {
            *s += u32::from(v);
        }
    }
    for i in 0..len {
        let out = start + i * stride;
        for c in 0..4 {
            pixels[out + c] = ((sum[c] + span / 2) / span) as u8;
        }
        let (add, drop) = (
            at(i as isize + radius as isize + 1),
            at(i as isize - radius as isize),
        );
        for c in 0..4 {
            sum[c] = sum[c] + u32::from(add[c]) - u32::from(drop[c]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_is_shrunk_to_a_small_copy_and_softened() {
        // A 256 px square, black with a white left half.
        let side = 256u32;
        let mut pixels = Vec::new();
        for _y in 0..side {
            for x in 0..side {
                let v = if x < side / 2 { 255 } else { 0 };
                pixels.extend([v, v, v, 255]);
            }
        }
        let (out, w, h) = blurred(&pixels, side, side).unwrap();
        assert_eq!((w, h), (SIDE, SIDE));
        let red = |x: u32| out[((h / 2 * w + x) * 4) as usize];
        // The far edges keep their colors, the seam turns gray, the alpha
        // stays opaque.
        assert_eq!(red(0), 255);
        assert_eq!(red(w - 1), 0);
        let seam = red(w / 2);
        assert!((64..192).contains(&seam), "seam {seam}");
        assert!(out.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
    }

    #[test]
    fn a_small_image_is_not_grown_and_a_bad_one_is_none() {
        let (_, w, h) = blurred(&[9; 10 * 20 * 4], 10, 20).unwrap();
        assert_eq!((w, h), (10, 20));
        assert!(blurred(&[], 0, 0).is_none());
        assert!(blurred(&[0; 8], 4, 4).is_none());
    }
}
