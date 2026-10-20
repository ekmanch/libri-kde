//! A column that reports where each of its children starts. The grouped
//! library list uses it to know where each type's group is, for the
//! type-colored scrollbar's jumps and the pinned group header.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse};

/// What `Groups` reports: where each child starts, and how tall the
/// visible part of the column is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Measure {
    pub tops: Vec<f32>,
    pub viewport: f32,
}

pub struct Groups<'a, Message, Theme, Renderer> {
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    on_measure: Box<dyn Fn(Measure) -> Message + 'a>,
}

impl<'a, Message, Theme, Renderer> Groups<'a, Message, Theme, Renderer> {
    /// `on_measure` gets the top of every child, from the column's top,
    /// and the viewport's height, each time they change.
    pub fn new(
        children: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
        on_measure: impl Fn(Measure) -> Message + 'a,
    ) -> Self {
        Groups {
            children: children.into_iter().collect(),
            on_measure: Box::new(on_measure),
        }
    }
}

#[derive(Default)]
struct State {
    measure: Measure,
    /// What was last sent, so a layout that moves nothing stays quiet.
    sent: Option<Measure>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Groups<'_, Message, Theme, Renderer>
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
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let child_limits = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let mut y = 0.0;
        let mut tops = Vec::with_capacity(self.children.len());
        let nodes = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, child_tree)| {
                let node = child
                    .as_widget_mut()
                    .layout(child_tree, renderer, &child_limits)
                    .move_to(Point::new(0.0, y));
                tops.push(y);
                y += node.size().height;
                node
            })
            .collect();
        tree.state.downcast_mut::<State>().measure.tops = tops;
        layout::Node::with_children(Size::new(width, y), nodes)
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
        for ((child, child_tree), child_layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                child_tree,
                event,
                child_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        let state = tree.state.downcast_mut::<State>();
        state.measure.viewport = viewport.height;
        if state.sent.as_ref() != Some(&state.measure) {
            state.sent = Some(state.measure.clone());
            shell.publish((self.on_measure)(state.measure.clone()));
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
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, child_tree), child_layout)| {
                child.as_widget().mouse_interaction(
                    child_tree,
                    child_layout,
                    cursor,
                    viewport,
                    renderer,
                )
            })
            .max()
            .unwrap_or_default()
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
        for ((child, child_tree), child_layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            // Children far outside the view are skipped.
            if child_layout.bounds().intersects(viewport) {
                child.as_widget().draw(
                    child_tree,
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            }
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, child_tree), child_layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(child_tree, child_layout, renderer, operation);
            }
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
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<Groups<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(groups: Groups<'a, Message, Theme, Renderer>) -> Self {
        Element::new(groups)
    }
}
