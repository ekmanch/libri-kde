//! An indeterminate ring, for the lines that wait on GitHub: three
//! quarters of a circle in the given color, turning. iced 0.14 has no
//! animation for a border and no rotation for a widget, so the ring is a
//! small SVG of which one frame per 15 degrees is kept, and the caller
//! picks the frame from a phase that a tick advances (the splash's).

use std::sync::OnceLock;

use iced::widget::svg::{self, Svg};
use iced::{Color, Element};

/// Frames per turn.
const FRAMES: usize = 24;

/// The ring's SVG at every angle.
fn frames() -> &'static [svg::Handle] {
    static FRAMES_SVG: OnceLock<Vec<svg::Handle>> = OnceLock::new();
    FRAMES_SVG.get_or_init(|| {
        (0..FRAMES)
            .map(|i| {
                let angle = i as f32 * 360.0 / FRAMES as f32;
                // A 24-unit box, a radius of 10 and a stroke of 3 scale to
                // a 2 px line at 14 px; the dash covers three quarters of
                // the circumference (2π · 10 ≈ 62.8).
                let text = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="none" stroke="#000" stroke-width="3" stroke-linecap="round" stroke-dasharray="47.1 62.8" transform="rotate({angle} 12 12)"/></svg>"##
                );
                svg::Handle::from_memory(text.into_bytes())
            })
            .collect()
    })
}

/// The ring at `size` px in `color`, turned by `phase` (0 to 1 is one
/// turn).
pub fn spinner<'a, Message>(size: f32, color: Color, phase: f32) -> Element<'a, Message> {
    let frames = frames();
    let index = ((phase.rem_euclid(1.0) * FRAMES as f32) as usize).min(FRAMES - 1);
    Svg::new(frames[index].clone())
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

/// Frames per turn of the busy ring: one per tick at 0.8 s a turn.
const RING_FRAMES: usize = 48;
/// The busy ring's line, in px at any size.
const RING_LINE: f32 = 2.0;

/// The busy ring's two layers at `size` px: the whole circle, and the
/// quarter arc at every angle. Drawn in the ring's own pixels, so the
/// line is 2 px at any size; kept per size (two are used).
fn ring_frames(size: f32) -> (svg::Handle, Vec<svg::Handle>) {
    type Rings = std::collections::HashMap<u32, (svg::Handle, Vec<svg::Handle>)>;
    static RINGS: OnceLock<std::sync::Mutex<Rings>> = OnceLock::new();
    let mut rings = RINGS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    rings
        .entry(size.to_bits())
        .or_insert_with(|| {
            let (center, radius) = (size / 2.0, (size - RING_LINE) / 2.0);
            let around = std::f32::consts::TAU * radius;
            let circle = |extra: String| {
                svg::Handle::from_memory(
                    format!(
                        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}"><circle cx="{center}" cy="{center}" r="{radius}" fill="none" stroke="#000" stroke-width="{RING_LINE}" {extra}/></svg>"##
                    )
                    .into_bytes(),
                )
            };
            let arcs = (0..RING_FRAMES)
                .map(|i| {
                    // The quarter at the top (from 225 to 315 degrees of
                    // a circle that starts at three o'clock), then on.
                    let angle = 225.0 + i as f32 * 360.0 / RING_FRAMES as f32;
                    circle(format!(
                        r#"stroke-dasharray="{} {around}" transform="rotate({angle} {center} {center})""#,
                        around / 4.0
                    ))
                })
                .collect();
            (circle(String::new()), arcs)
        })
        .clone()
}

/// The busy ring of a search in progress (canvas 2026-11-22): a 2 px
/// `track` circle with a quarter `arc` going round, turned by `phase` (0
/// to 1 is one turn; `App::ring_phase` makes a turn in 0.8 s).
pub fn ring<'a, Message: 'a>(
    size: f32,
    track: Color,
    arc: Color,
    phase: f32,
) -> Element<'a, Message> {
    let (circle, arcs) = ring_frames(size);
    let index = ((phase.rem_euclid(1.0) * RING_FRAMES as f32) as usize).min(RING_FRAMES - 1);
    // The renderer tints an SVG with the color's red, green and blue
    // alone, so a see-through color (Glass's line) gives its alpha as the
    // layer's opacity.
    let layer = |handle: svg::Handle, color: Color| {
        Svg::new(handle)
            .width(size)
            .height(size)
            .opacity(color.a)
            .style(move |_, _| svg::Style { color: Some(color) })
    };
    iced::widget::stack![layer(circle, track), layer(arcs[index].clone(), arc)].into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_has_a_frame_and_a_turn_comes_back_to_the_first() {
        assert_eq!(frames().len(), FRAMES);
        let first = frames()[0].id();
        for phase in [0.0, 0.5, 0.999, 1.0, 1.25, -0.25] {
            let _ = spinner::<()>(14.0, Color::WHITE, phase);
        }
        // Phase 1.0 is phase 0 again.
        let index = ((1.0f32.rem_euclid(1.0) * FRAMES as f32) as usize).min(FRAMES - 1);
        assert_eq!(frames()[index].id(), first);
    }

    #[test]
    fn the_busy_ring_has_a_frame_for_every_tick_of_a_turn() {
        let (track, arcs) = ring_frames(12.0);
        assert_eq!(arcs.len(), RING_FRAMES);
        // Every frame is another picture, and none is the track's.
        let mut ids: Vec<_> = arcs.iter().map(|a| a.id()).collect();
        ids.push(track.id());
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), RING_FRAMES + 1);
        // The same size is built once; another size is its own.
        assert_eq!(ring_frames(12.0).0.id(), track.id());
        assert_ne!(ring_frames(14.0).0.id(), track.id());
        for phase in [0.0, 0.5, 0.999, 1.0, -0.25] {
            let _ = ring::<()>(12.0, Color::BLACK, Color::WHITE, phase);
        }
    }
}
