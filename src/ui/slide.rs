//! Moves its content by an offset, without changing its size or anything
//! around it: the details drawer slides in from the window's edge through
//! it, and the Coming soon page's list slides sideways between its
//! slices. The content keeps its layout where it is and is drawn `offset`
//! px away, with the mouse shifted to match, cut to the slide's own place:
//! what moves past its edge is not drawn.
//!
//! Only the drawing moves, never the layout: a scroll area reports its
//! viewport whenever its bounds change, so a moved layout made the Coming
//! soon list send `Scrolled` three times a frame, each one a rebuild of the
//! whole window, and the slide ran at a fraction of the display's rate.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse};

pub struct Slide<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    offset: Vector,
}

/// `content` placed `offset` px to the right of where it would be.
pub fn slide<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    offset: f32,
) -> Slide<'a, Message, Theme, Renderer> {
    slide_by(content, Vector::new(offset.max(0.0), 0.0))
}

/// `content` moved by `offset`, in either direction on both axes.
pub fn slide_by<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    offset: Vector,
) -> Slide<'a, Message, Theme, Renderer> {
    Slide {
        content: content.into(),
        offset,
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Slide<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self.content.as_widget_mut().layout(tree, renderer, limits);
        let size = node.size();
        layout::Node::with_children(size, vec![node.move_to(Point::ORIGIN)])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Some(inner) = layout.children().next() {
            self.content.as_widget_mut().update(
                tree,
                event,
                inner,
                cursor - self.offset,
                renderer,
                clipboard,
                shell,
                &(*viewport - self.offset),
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        layout
            .children()
            .next()
            .map_or_else(Default::default, |inner| {
                self.content.as_widget().mouse_interaction(
                    tree,
                    inner,
                    cursor - self.offset,
                    &(*viewport - self.offset),
                    renderer,
                )
            })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let Some(inner) = layout.children().next() else {
            return;
        };
        if self.offset == Vector::ZERO {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, inner, cursor, viewport);
            return;
        }
        // Moved, the content is cut to where the slide itself is: iced's
        // `clip` on a container only narrows the viewport it hands down,
        // and a scroll area whose content fits draws it without a clip of
        // its own, so a short Coming soon list slid over the panel.
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        let (cursor, viewport) = (cursor - self.offset, clip - self.offset);
        renderer.with_layer(clip, |renderer| {
            renderer.with_translation(self.offset, |renderer| {
                self.content
                    .as_widget()
                    .draw(tree, renderer, theme, style, inner, cursor, &viewport);
            });
        });
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if let Some(inner) = layout.children().next() {
            self.content
                .as_widget_mut()
                .operate(tree, inner, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let inner = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            tree,
            inner,
            renderer,
            viewport,
            translation + self.offset,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Slide<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(slide: Slide<'a, Message, Theme, Renderer>) -> Self {
        Element::new(slide)
    }
}

#[cfg(test)]
mod tests {
    use iced::widget::{button, container};
    use iced::{Element, Point};
    use iced_test::simulator;

    use super::slide;

    #[derive(Debug, Clone, PartialEq)]
    struct Pressed;

    fn moved(offset: f32) -> Element<'static, Pressed> {
        container(slide(
            button("Go").width(100).height(40).on_press(Pressed),
            offset,
        ))
        .width(400)
        .height(100)
        .into()
    }

    #[test]
    fn the_drawing_and_the_mouse_move_but_never_the_layout() {
        // The layout is where it would be without the slide.
        let mut ui = simulator(moved(200.0));
        assert_eq!(ui.find("Go").unwrap().bounds(), {
            let mut still = simulator(moved(0.0));
            still.find("Go").unwrap().bounds()
        });

        // A click where the button was laid out misses it; one where it is
        // drawn, 200 px on, presses it.
        ui.point_at(Point::new(50.0, 20.0));
        let _ = ui.simulate(iced_test::simulator::click());
        assert!(ui.into_messages().next().is_none());
        let mut ui = simulator(moved(200.0));
        ui.point_at(Point::new(250.0, 20.0));
        let _ = ui.simulate(iced_test::simulator::click());
        assert_eq!(ui.into_messages().collect::<Vec<_>>(), vec![Pressed]);
    }
}
