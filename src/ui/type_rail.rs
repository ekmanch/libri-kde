//! The type-colored scrollbar of a list grouped by type: one segment per
//! group, as tall on the rail as the group is in the list, and an
//! outlined thumb for the part in view. It is a scrollbar like any other:
//! a press on the thumb drags it, a press elsewhere on the rail brings the
//! thumb's middle there and drags on from it, and the held button follows
//! anywhere in the window until it is let go.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, renderer};
use iced::{Background, Border, Color, Element, Event, Length, Rectangle, Size, mouse};

/// The rail's width, wider than what it draws so it is easy to hit.
pub const WIDTH: f32 = 21.0;
/// The segments' width and the thumb's.
const SEGMENT: f32 = 5.0;
const THUMB: f32 = 11.0;
/// The room between two segments.
const GAP: f32 = 2.0;
/// The thumb is never shorter than this, however long the list.
const MIN_THUMB: f32 = 14.0;

/// The list as the rail sees it, in the list's px.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    /// Each group's color, and where it starts and ends in the list.
    pub segments: Vec<(Color, f32, f32)>,
    /// The list's whole height, and its part in view.
    pub content: f32,
    pub offset: f32,
    pub viewport: f32,
}

impl Track {
    /// How far the list scrolls at most.
    fn max_offset(&self) -> f32 {
        (self.content - self.viewport).max(0.0)
    }

    /// The thumb's top and height on a rail `height` px tall.
    fn thumb(&self, height: f32) -> (f32, f32) {
        if self.content <= 0.0 || self.content <= self.viewport {
            return (0.0, height);
        }
        let size = (self.viewport / self.content * height)
            .max(MIN_THUMB)
            .min(height);
        let top = (self.offset / self.content * height).clamp(0.0, height - size);
        (top, size)
    }

    /// The offset that puts the thumb's top at `top` px down the rail.
    fn offset_at(&self, top: f32, height: f32) -> f32 {
        if height <= 0.0 {
            return 0.0;
        }
        (top / height * self.content).clamp(0.0, self.max_offset())
    }
}

/// The rail, filling the height it is given.
pub struct TypeRail<'a, Message> {
    track: Track,
    edge: Color,
    on_scroll: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<'a, Message> TypeRail<'a, Message> {
    /// The rail over `track` with the thumb outlined in `edge`;
    /// `on_scroll` gets the offset to scroll the list to.
    pub fn new(track: Track, edge: Color, on_scroll: impl Fn(f32) -> Message + 'a) -> Self {
        TypeRail {
            track,
            edge,
            on_scroll: Box::new(on_scroll),
        }
    }
}

#[derive(Default)]
struct State {
    /// While the button is held: how far below the thumb's top it went
    /// down, in rail px.
    grab: Option<f32>,
    /// The offset last asked for in this drag, so a still pointer stays
    /// quiet.
    last: Option<f32>,
}

impl<Message> TypeRail<'_, Message> {
    fn scroll_to(
        &self,
        y: f32,
        bounds: Rectangle,
        state: &mut State,
        shell: &mut Shell<'_, Message>,
    ) {
        let grab = state.grab.unwrap_or_default();
        let offset = self.track.offset_at(y - bounds.y - grab, bounds.height);
        if state.last.is_none_or(|last| (last - offset).abs() >= 0.5) {
            state.last = Some(offset);
            shell.publish((self.on_scroll)(offset));
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for TypeRail<'_, Message>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(WIDTH), Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(WIDTH), Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let left = mouse::Button::Left;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) if *button == left => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };
                let (top, size) = self.track.thumb(bounds.height);
                let y = position.y - bounds.y;
                if (top..=top + size).contains(&y) {
                    state.grab = Some(y - top);
                    state.last = Some(self.track.offset);
                } else {
                    state.grab = Some(size / 2.0);
                    state.last = None;
                    self.scroll_to(position.y, bounds, state, shell);
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.grab.is_some() => {
                self.scroll_to(position.y, bounds, state, shell);
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(button))
                if *button == left && state.grab.is_some() =>
            {
                state.grab = None;
                state.last = None;
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let held = tree.state.downcast_ref::<State>().grab.is_some();
        if held {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let track = &self.track;
        if track.content <= 0.0 || bounds.height <= 0.0 {
            return;
        }
        let scale = bounds.height / track.content;
        let last = track.segments.len().saturating_sub(1);
        for (index, &(color, start, end)) in track.segments.iter().enumerate() {
            // Half the gap off each end that meets another segment.
            let top = start * scale + if index > 0 { GAP / 2.0 } else { 0.0 };
            let bottom = end * scale - if index < last { GAP / 2.0 } else { 0.0 };
            if bottom <= top {
                continue;
            }
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.center_x() - SEGMENT / 2.0,
                        y: bounds.y + top,
                        width: SEGMENT,
                        height: bottom - top,
                    },
                    border: Border::default().rounded(2),
                    ..renderer::Quad::default()
                },
                Background::Color(color),
            );
        }
        let (top, size) = track.thumb(bounds.height);
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.center_x() - THUMB / 2.0,
                    y: bounds.y + top,
                    width: THUMB,
                    height: size,
                },
                border: Border::default().rounded(5).width(1.5).color(self.edge),
                ..renderer::Quad::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
    }
}

impl<'a, Message: 'a, Theme, Renderer> From<TypeRail<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn from(rail: TypeRail<'a, Message>) -> Self {
        Element::new(rail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3000 px of list, 600 in view, on a 300 px rail.
    fn track(offset: f32) -> Track {
        Track {
            segments: Vec::new(),
            content: 3000.0,
            offset,
            viewport: 600.0,
        }
    }

    #[test]
    fn the_thumb_shows_the_part_in_view() {
        assert_eq!(track(0.0).thumb(300.0), (0.0, 60.0));
        assert_eq!(track(1500.0).thumb(300.0), (150.0, 60.0));
        assert_eq!(track(2400.0).thumb(300.0), (240.0, 60.0));
        // A list that fits: the thumb is the whole rail.
        let short = Track {
            content: 500.0,
            ..track(0.0)
        };
        assert_eq!(short.thumb(300.0), (0.0, 300.0));
        // Never too short to grab.
        let long = Track {
            content: 300_000.0,
            ..track(0.0)
        };
        assert_eq!(long.thumb(300.0).1, MIN_THUMB);
    }

    #[test]
    fn a_place_on_the_rail_is_the_same_place_in_the_list() {
        let track = track(0.0);
        assert_eq!(track.offset_at(150.0, 300.0), 1500.0);
        assert_eq!(track.offset_at(-20.0, 300.0), 0.0);
        assert_eq!(
            track.offset_at(290.0, 300.0),
            2400.0,
            "no further than the end"
        );
    }

    /// Presses and moves of the left button at these heights on a rail
    /// 300 px tall, over `track(0.0)`; the offsets the rail asked for.
    fn drag(steps: &[(&str, f32)]) -> Vec<f32> {
        use iced::advanced::clipboard::Null;
        use iced::advanced::renderer::Headless;
        use iced::{Point, Size};
        use iced_test::runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced_test::futures::futures::executor::block_on(
            <iced::Renderer as Headless>::new(iced::Font::default(), 14.into(), None),
        )
        .expect("a headless renderer");
        let rail: Element<'_, f32> = TypeRail::new(track(0.0), Color::WHITE, |o| o).into();
        let mut ui = UserInterface::build(
            rail,
            Size::new(WIDTH, 300.0),
            Cache::default(),
            &mut renderer,
        );
        let mut asked = Vec::new();
        for &(what, y) in steps {
            let position = Point::new(WIDTH / 2.0, y);
            let event = match what {
                "press" => mouse::Event::ButtonPressed(mouse::Button::Left),
                "release" => mouse::Event::ButtonReleased(mouse::Button::Left),
                _ => mouse::Event::CursorMoved { position },
            };
            let cursor = mouse::Cursor::Available(position);
            let _ = ui.update(
                &[Event::Mouse(event)],
                cursor,
                &mut renderer,
                &mut Null,
                &mut asked,
            );
        }
        asked
    }

    #[test]
    fn a_press_off_the_thumb_centers_it_there() {
        // The thumb is 60 px: its middle at 180 puts its top at 150.
        assert_eq!(drag(&[("press", 180.0)]), vec![1500.0]);
    }

    #[test]
    fn the_thumb_follows_the_held_button() {
        let asked = drag(&[
            ("press", 30.0),
            ("move", 130.0),
            ("move", 330.0),
            ("release", 330.0),
            ("move", 100.0),
        ]);
        // Grabbed at its middle, the thumb moves 100 px, then to the
        // end; once let go, moving does nothing.
        assert_eq!(asked, vec![1000.0, 2400.0]);
    }
}
