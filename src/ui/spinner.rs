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
}
