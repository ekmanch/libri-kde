//! A popover anchored to a widget: the Filter and Sort menus, the rating
//! picker, the year dropdown. iced 0.14 has no anchored overlay besides
//! tooltips, so this is a small custom widget.
//!
//! The popover opens below its anchor (or above, when there is no room),
//! aligned to the anchor's left or right edge. A click outside it and its
//! anchor sends `on_dismiss`; a click on the anchor is left to the anchor,
//! so a toggle button closes it as usual.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::widget::space;
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse};

/// Which edges of the anchor and the popover line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    End,
}

pub struct Popover<'a, Message, Theme, Renderer> {
    anchor: Element<'a, Message, Theme, Renderer>,
    popup: Element<'a, Message, Theme, Renderer>,
    open: bool,
    align: Align,
    gap: f32,
    on_dismiss: Option<Message>,
}

impl<'a, Message, Theme, Renderer> Popover<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    /// `popup` shows while it is `Some`.
    pub fn new(
        anchor: impl Into<Element<'a, Message, Theme, Renderer>>,
        popup: Option<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        let open = popup.is_some();
        Popover {
            anchor: anchor.into(),
            popup: popup.unwrap_or_else(|| space().into()),
            open,
            align: Align::Start,
            gap: 8.0,
            on_dismiss: None,
        }
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn on_dismiss(mut self, message: Message) -> Self {
        self.on_dismiss = Some(message);
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Popover<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor), Tree::new(&self.popup)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.anchor.as_widget(), self.popup.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.anchor.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
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
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.anchor
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let [anchor_tree, popup_tree] = &mut tree.children[..] else {
            return None;
        };
        let anchor = self.anchor.as_widget_mut().overlay(
            anchor_tree,
            layout,
            renderer,
            viewport,
            translation,
        );
        let popup = self.open.then(|| {
            overlay::Element::new(Box::new(Overlay {
                anchor: layout.bounds() + translation,
                popup: &mut self.popup,
                tree: popup_tree,
                align: self.align,
                gap: self.gap,
                on_dismiss: self.on_dismiss.clone(),
            }))
        });
        match (anchor, popup) {
            (None, None) => None,
            (anchor, popup) => Some(
                overlay::Group::with_children(anchor.into_iter().chain(popup).collect()).overlay(),
            ),
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Popover<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(popover: Popover<'a, Message, Theme, Renderer>) -> Self {
        Element::new(popover)
    }
}

struct Overlay<'a, 'b, Message, Theme, Renderer> {
    anchor: Rectangle,
    popup: &'b mut Element<'a, Message, Theme, Renderer>,
    tree: &'b mut Tree,
    align: Align,
    gap: f32,
    on_dismiss: Option<Message>,
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for Overlay<'_, '_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let node = self.popup.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let size = node.size();
        let position = place(self.anchor, size, bounds, self.align, self.gap);
        node.move_to(position)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.popup.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.popup
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let bounds = layout.bounds();
        self.popup.as_widget_mut().update(
            self.tree, event, layout, cursor, renderer, clipboard, shell, &bounds,
        );

        if let Event::Mouse(mouse::Event::ButtonPressed(_)) = event {
            if cursor.is_over(bounds) {
                // Nothing under the popover reacts to a click on it.
                shell.capture_event();
            } else if !cursor.is_over(self.anchor)
                && let Some(message) = &self.on_dismiss
            {
                shell.publish(message.clone());
            }
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.popup.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, Message, Theme, Renderer>> {
        self.popup.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &layout.bounds(),
            Vector::ZERO,
        )
    }
}

/// Below the anchor when it fits, otherwise above; kept inside the window.
fn place(anchor: Rectangle, size: Size, window: Size, align: Align, gap: f32) -> Point {
    let x = match align {
        Align::Start => anchor.x,
        Align::End => anchor.x + anchor.width - size.width,
    };
    let below = anchor.y + anchor.height + gap;
    let above = anchor.y - gap - size.height;
    let y = if below + size.height <= window.height || above < 0.0 {
        below
    } else {
        above
    };
    Point::new(
        x.clamp(0.0, (window.width - size.width).max(0.0)),
        y.clamp(0.0, (window.height - size.height).max(0.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1600.0, 900.0);

    #[test]
    fn opens_below_aligned_to_the_chosen_edge() {
        let anchor = Rectangle::new(Point::new(1000.0, 20.0), Size::new(100.0, 36.0));
        let size = Size::new(240.0, 200.0);
        assert_eq!(
            place(anchor, size, WINDOW, Align::Start, 8.0),
            Point::new(1000.0, 64.0)
        );
        assert_eq!(
            place(anchor, size, WINDOW, Align::End, 8.0),
            Point::new(860.0, 64.0)
        );
    }

    #[test]
    fn opens_above_when_there_is_no_room_below_and_stays_in_the_window() {
        let anchor = Rectangle::new(Point::new(1500.0, 840.0), Size::new(100.0, 30.0));
        let size = Size::new(320.0, 560.0);
        assert_eq!(
            place(anchor, size, WINDOW, Align::Start, 8.0),
            Point::new(1280.0, 272.0)
        );
    }
}
