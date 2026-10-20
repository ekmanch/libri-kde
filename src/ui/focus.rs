//! Keyboard focus for buttons, chips and rows, which iced 0.14 gives to
//! text fields alone. `Focusable` wraps a widget: Tab and Shift+Tab reach
//! it through iced's focus operations, Enter and Space press it, and a
//! ring in the accent ink shows where focus is once it moved by keyboard.
//! A click moves focus silently, so Tab goes on from the clicked widget
//! without a ring or a second press on Enter.
//!
//! `Barrier` sits under a modal dialog: focus operations and keys do not
//! reach the screen behind the scrim.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, Widget, operation, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::keyboard::{self, key::Named};
use iced::{Border, Color, Element, Event, Length, Rectangle, Size, Vector, mouse};

use crate::theme::Tokens;

pub struct Focusable<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    child: Element<'a, Message, Theme, Renderer>,
    ring: Color,
    radius: f32,
    /// The ring drawn just inside the bounds, for a widget clipped by its
    /// parent (a segment of a joined group).
    inside: bool,
}

impl<'a, Message, Theme, Renderer> Focusable<'a, Message, Theme, Renderer> {
    /// `child` with a focus ring in `ring`, rounded by `radius` (the
    /// child's own radius; the ring sits 2 px outside it).
    pub fn new(
        child: impl Into<Element<'a, Message, Theme, Renderer>>,
        ring: Color,
        radius: f32,
    ) -> Self {
        Focusable {
            child: child.into(),
            ring,
            radius,
            inside: false,
        }
    }

    /// Draws the ring inside the bounds instead of around them.
    pub fn inside(mut self) -> Self {
        self.inside = true;
        self
    }
}

/// `.focusable(radius, tokens)` on any widget: wraps it in a `Focusable`
/// with the ring in the accent ink.
pub trait FocusExt<'a, Message> {
    fn focusable(self, radius: f32, tokens: &Tokens) -> Element<'a, Message>;
    /// The ring just inside the bounds, for a widget its parent clips.
    fn focusable_inside(self, radius: f32, tokens: &Tokens) -> Element<'a, Message>;
}

impl<'a, Message: 'a, T: Into<Element<'a, Message>>> FocusExt<'a, Message> for T {
    fn focusable(self, radius: f32, tokens: &Tokens) -> Element<'a, Message> {
        Focusable::new(self, tokens.ink, radius).into()
    }

    fn focusable_inside(self, radius: f32, tokens: &Tokens) -> Element<'a, Message> {
        Focusable::new(self, tokens.ink, radius).inside().into()
    }
}

#[derive(Debug, Default)]
struct State {
    focused: bool,
    /// Focus came by keyboard, so the ring shows and Enter presses.
    by_keyboard: bool,
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
        self.by_keyboard = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
        self.by_keyboard = false;
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Focusable<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.child.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(node.size(), vec![node])
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
        let bounds = layout.bounds();
        let child_layout = layout.children().next().expect("the child's layout");
        let state = tree.state.downcast_mut::<State>();
        match event {
            // A click moves focus here, or away, without a ring.
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                state.focused = cursor.is_over(bounds);
                state.by_keyboard = false;
            }
            // Enter or Space presses the child, as a click at its center.
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if state.focused
                    && state.by_keyboard
                    && !shell.is_event_captured()
                    && matches!(key, keyboard::Key::Named(Named::Enter | Named::Space)) =>
            {
                let center = mouse::Cursor::Available(bounds.center());
                for synthetic in [
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                ] {
                    self.child.as_widget_mut().update(
                        &mut tree.children[0],
                        &synthetic,
                        child_layout,
                        center,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    );
                }
                shell.capture_event();
                return;
            }
            _ => {}
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout,
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().expect("the child's layout"),
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().expect("the child's layout"),
            cursor,
            viewport,
        );
        let state = tree.state.downcast_ref::<State>();
        if state.focused && state.by_keyboard {
            let (bounds, radius) = if self.inside {
                (layout.bounds(), self.radius)
            } else {
                (layout.bounds().expand(2.0), self.radius + 2.0)
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border::default().rounded(radius).width(2).color(self.ring),
                    ..renderer::Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(None, layout.bounds(), state);
        operation.traverse(&mut |operation| {
            self.child.as_widget_mut().operate(
                &mut tree.children[0],
                layout.children().next().expect("the child's layout"),
                renderer,
                operation,
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().expect("the child's layout"),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Focusable<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(focusable: Focusable<'a, Message, Theme, Renderer>) -> Self {
        Element::new(focusable)
    }
}

/// The screen under a modal dialog: drawn and laid out as usual, but
/// focus operations and keys stop here, so Tab and Enter stay in the
/// dialog.
pub struct Barrier<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    child: Element<'a, Message, Theme, Renderer>,
}

impl<'a, Message, Theme, Renderer> Barrier<'a, Message, Theme, Renderer> {
    pub fn new(child: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Barrier {
            child: child.into(),
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Barrier<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.child.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(node.size(), vec![node])
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
        if matches!(event, Event::Keyboard(_)) {
            return;
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().expect("the child's layout"),
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().expect("the child's layout"),
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().expect("the child's layout"),
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        _layout: Layout<'_>,
        _renderer: &Renderer,
        _operation: &mut dyn widget::Operation,
    ) {
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().expect("the child's layout"),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Barrier<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(barrier: Barrier<'a, Message, Theme, Renderer>) -> Self {
        Element::new(barrier)
    }
}
