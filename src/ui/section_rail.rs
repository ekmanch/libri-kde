//! The section rail down a list's right edge: one short label per section
//! (letters, decades, ratings, months), the section scrolled to on the
//! accent. A press on a label jumps there. With the button held, moving
//! along the rail jumps from label to label, and a bubble beside the rail
//! names the section in large type until the button is let go.
//!
//! The labels are ordinary focusable buttons, so Tab reaches them and
//! Enter jumps; `Rail` lays them out, follows the held button (anywhere
//! in the window, once the press was on the rail) and places the bubble.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, renderer};
use iced::widget::{button, container, text};
use iced::{Background, Border, Element, Event, Fill, Length, Point, Rectangle, Size, mouse};

use super::{FocusExt, panel_style};
use crate::theme::{Tokens, font};

/// A label's height, unless the rail is too short for all of them.
const LABEL: f32 = 16.0;
/// The room between two labels that are not spread over the height.
const GAP: f32 = 2.0;
/// From the list's right edge to the rail, and from the rail to the rows.
const EDGE: f32 = 8.0;
const INSET: f32 = 12.0;
/// The bubble: a rounded square this tall (wider for a long label), this
/// far left of the rail.
const BUBBLE: f32 = 76.0;
const BUBBLE_GAP: f32 = 14.0;
const BUBBLE_MAX: f32 = 400.0;

/// One label: its text, and whether the list has anything under it. A
/// letter with nothing is drawn faint and does nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct RailLabel {
    pub text: String,
    pub present: bool,
}

/// What the rail shows and sends.
pub struct SectionRail<'a, Message> {
    pub labels: Vec<RailLabel>,
    /// The label of the section scrolled to.
    pub current: Option<usize>,
    /// The label the bubble names, while the rail is being dragged.
    pub dragged: Option<usize>,
    /// The labels share the rail's height (the alphabet); otherwise they
    /// sit together from the top.
    pub spread: bool,
    /// A jump to a label, and whether the mouse button is held on the
    /// rail (`false` from the keyboard, where no release follows).
    pub on_jump: Box<dyn Fn(usize, bool) -> Message + 'a>,
    /// The button let go after a press on the rail.
    pub on_release: Message,
}

/// Whether any label is longer than two characters ("1990s", "Older"):
/// the rail is wider and its type smaller.
fn wide(labels: &[RailLabel]) -> bool {
    labels.iter().any(|label| label.text.chars().count() > 2)
}

/// Whether any label is longer than five ("Mar ’27", the Coming soon
/// page's months of another year): the rail is wider still.
fn widest(labels: &[RailLabel]) -> bool {
    labels.iter().any(|label| label.text.chars().count() > 5)
}

/// The rail's width for these labels.
pub fn width(labels: &[RailLabel]) -> f32 {
    if widest(labels) {
        46.0
    } else if wide(labels) {
        36.0
    } else {
        24.0
    }
}

/// The room a list leaves at its right for the rail of these labels.
pub fn room(labels: &[RailLabel]) -> f32 {
    EDGE + width(labels) + INSET
}

impl<'a, Message: Clone + 'a> SectionRail<'a, Message> {
    /// The rail as a layer over the list: at its right edge, top to bottom.
    pub fn view(self, tokens: &Tokens) -> Element<'a, Message> {
        let size = if wide(&self.labels) { 10.0 } else { 11.5 };
        let width = width(&self.labels);
        let label = |label: &RailLabel, color| {
            container(
                text(label.text.clone())
                    .size(size)
                    .font(font::BOLD)
                    .line_height(1.0)
                    .color(color),
            )
            .center(Fill)
        };
        let mut children: Vec<Element<'a, Message>> = Vec::with_capacity(self.labels.len() + 1);
        let mut jumps = Vec::with_capacity(self.labels.len());
        for (index, item) in self.labels.iter().enumerate() {
            if !item.present {
                children.push(label(item, tokens.line).into());
                jumps.push(None);
                continue;
            }
            let on = self.current == Some(index);
            let color = if on { tokens.on_accent } else { tokens.sub };
            let (accent, hover) = (tokens.accent, tokens.hover);
            children.push(
                button(label(item, color))
                    .padding(0)
                    .width(Fill)
                    .height(Fill)
                    .on_press((self.on_jump)(index, false))
                    .style(move |_, status| {
                        let fill = match status {
                            _ if on => Some(accent),
                            button::Status::Hovered | button::Status::Pressed => Some(hover),
                            _ => None,
                        };
                        button::Style {
                            background: fill.map(Background::Color),
                            text_color: color,
                            border: Border::default().rounded(6),
                            ..button::Style::default()
                        }
                    })
                    .focusable(6.0, tokens),
            );
            jumps.push(Some((self.on_jump)(index, true)));
        }
        let bubble = self.dragged.filter(|&index| index < self.labels.len());
        if let Some(index) = bubble {
            let tokens = *tokens;
            children.push(
                container(
                    text(self.labels[index].text.clone())
                        .size(40)
                        .font(font::EXTRABOLD)
                        .line_height(1.0)
                        .color(tokens.ink),
                )
                .padding([0, 14])
                .center_y(BUBBLE)
                .align_x(iced::Center)
                .style(move |_| panel_style(&tokens, 18.0))
                .into(),
            );
        }
        let rail = Rail {
            labels: self.labels.len(),
            children,
            jumps,
            bubble,
            spread: self.spread,
            width,
            on_release: self.on_release,
        };
        container(Element::new(rail))
            .width(Fill)
            .height(Fill)
            .align_right(Fill)
            .padding(iced::Padding::new(INSET).right(EDGE).left(0))
            .into()
    }
}

/// The labels in a column `width` wide, and the bubble as the last child
/// when there is one.
struct Rail<'a, Message, Theme, Renderer> {
    labels: usize,
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    /// What a press on each label, or the held button moving onto it,
    /// sends.
    jumps: Vec<Option<Message>>,
    /// The label the bubble sits beside.
    bubble: Option<usize>,
    spread: bool,
    width: f32,
    on_release: Message,
}

#[derive(Default)]
struct State {
    /// The button went down on the rail and is still held.
    dragging: bool,
    /// The label last jumped to in this drag, so moving within it stays
    /// quiet.
    last: Option<usize>,
}

impl<Message: Clone, Theme, Renderer> Rail<'_, Message, Theme, Renderer> {
    /// Jumps to the label nearest `y`, once per label.
    fn jump_at(
        &self,
        y: f32,
        layout: Layout<'_>,
        state: &mut State,
        shell: &mut Shell<'_, Message>,
    ) {
        let nearest = layout
            .children()
            .take(self.labels)
            .map(|label| (label.bounds().center_y() - y).abs())
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index);
        if let Some(index) = nearest
            && state.last != Some(index)
            && let Some(jump) = &self.jumps[index]
        {
            state.last = Some(index);
            shell.publish(jump.clone());
        }
    }
}

impl<Message: Clone, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Rail<'_, Message, Theme, Renderer>
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
        Size::new(Length::Fixed(self.width), Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(Length::Fixed(self.width), Length::Fill, Size::ZERO);
        let count = self.labels.max(1) as f32;
        // 16 px each, less when the rail is too short for all of them;
        // spread, the first sits at the top and the last at the bottom.
        let label = ((size.height - GAP * (count - 1.0)) / count).clamp(1.0, LABEL);
        let pitch = if self.spread && self.labels > 1 {
            (size.height - label) / (count - 1.0)
        } else {
            label + GAP
        };
        let label_limits = layout::Limits::new(Size::ZERO, Size::new(size.width, label));
        let bubble_limits =
            layout::Limits::new(Size::new(BUBBLE, BUBBLE), Size::new(BUBBLE_MAX, BUBBLE));
        let labels = self.labels;
        let bubble = self.bubble;
        let nodes = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .enumerate()
            .map(|(index, (child, child_tree))| {
                let widget = child.as_widget_mut();
                if index < labels {
                    let y = index as f32 * pitch;
                    return widget
                        .layout(child_tree, renderer, &label_limits)
                        .move_to(Point::new(0.0, y));
                }
                // The bubble: left of the rail, level with its label, and
                // never past the rail's ends.
                let node = widget.layout(child_tree, renderer, &bubble_limits);
                let level = bubble.unwrap_or_default() as f32 * pitch + label / 2.0;
                let y = (level - BUBBLE / 2.0).clamp(0.0, (size.height - BUBBLE).max(0.0));
                let x = -BUBBLE_GAP - node.size().width;
                node.move_to(Point::new(x, y))
            })
            .collect();
        layout::Node::with_children(size, nodes)
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
        let state = tree.state.downcast_mut::<State>();
        let left = mouse::Button::Left;
        // The rail takes the left button itself: the labels below see the
        // press and the release without the pointer, so none of them is
        // pressed as well, and a label focused by keyboard lets go.
        let mut pointer = cursor;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) if *button == left => {
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    state.dragging = true;
                    state.last = None;
                    self.jump_at(position.y, layout, state, shell);
                    shell.capture_event();
                    pointer = mouse::Cursor::Unavailable;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                self.jump_at(position.y, layout, state, shell);
            }
            Event::Mouse(mouse::Event::ButtonReleased(button))
                if *button == left && state.dragging =>
            {
                state.dragging = false;
                state.last = None;
                shell.publish(self.on_release.clone());
                pointer = mouse::Cursor::Unavailable;
            }
            _ => {}
        }
        for ((child, child_tree), child_layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .take(self.labels)
        {
            child.as_widget_mut().update(
                child_tree,
                event,
                child_layout,
                pointer,
                renderer,
                clipboard,
                shell,
                viewport,
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
        // Held, the pointer is the rail's wherever it is, so nothing under
        // it lights up.
        if tree.state.downcast_ref::<State>().dragging {
            return mouse::Interaction::Pointer;
        }
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .take(self.labels)
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
}
