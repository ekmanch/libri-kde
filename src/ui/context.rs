//! The right-click menu: a layer over a screen that draws a menu where
//! the pointer was, with a submenu beside the row that opens it, and the
//! pieces a menu is made of. iced 0.14 has no context menu, and
//! `Popover` hangs from a widget, so this is a small widget of its own.
//!
//! The layer is the top child of a `stack` and is always there, so the
//! screen under it keeps its place in the tree. Closed, it only reports
//! where the right button went down (`on_point`), in its own
//! coordinates, and lets the press through: the widget under the pointer
//! then says what was pressed. Open, it keeps the menus inside its
//! bounds; a press outside them sends `on_dismiss` and goes no further,
//! so closing the menu never selects what is under it.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell, renderer};
use iced::widget::{Column, Space, button, container, row, space, text};
use iced::{
    Background, Border, Center, Color, Element, Event, Fill, Length, Point, Rectangle, Size, mouse,
    touch,
};

use super::{FocusExt, Icon, icon, panel_style};
use crate::theme::{Tokens, font};

/// The menu keeps this far from the layer's edges.
const EDGE: f32 = 8.0;
/// A submenu lies this far over its menu's side.
const OVERLAP: f32 = 4.0;
/// A menu's padding, and the heights of its header, rows and separators.
pub const PADDING: f32 = 6.0;
pub const HEADER: f32 = 28.0;
pub const ROW: f32 = 32.0;
pub const SEPARATOR: f32 = 9.0;
/// How wide a menu and a submenu are.
pub const WIDTH: f32 = 256.0;
pub const SUB_WIDTH: f32 = 268.0;

pub struct Layer<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    menu: Element<'a, Message, Theme, Renderer>,
    sub: Element<'a, Message, Theme, Renderer>,
    open: bool,
    /// The submenu is open, its first row level with the row this far
    /// down the menu.
    sub_row: Option<f32>,
    at: Point,
    on_point: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    on_dismiss: Option<Message>,
}

impl<'a, Message, Theme, Renderer> Layer<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer + 'a,
    Message: 'a,
    Theme: 'a,
{
    /// The layer with `menu` open at `at`, or closed with `None`.
    pub fn new(menu: Option<Element<'a, Message, Theme, Renderer>>, at: Point) -> Self {
        let open = menu.is_some();
        Layer {
            menu: menu.unwrap_or_else(|| space().into()),
            sub: space().into(),
            open,
            sub_row: None,
            at,
            on_point: None,
            on_dismiss: None,
        }
    }

    /// A submenu beside the row that starts `row` px down the menu.
    pub fn sub(mut self, sub: Element<'a, Message, Theme, Renderer>, row: f32) -> Self {
        self.sub = sub;
        self.sub_row = Some(row);
        self
    }

    /// Where the right button went down while the menu is closed.
    pub fn on_point(mut self, on_point: impl Fn(Point) -> Message + 'a) -> Self {
        self.on_point = Some(Box::new(on_point));
        self
    }

    pub fn on_dismiss(mut self, message: Message) -> Self {
        self.on_dismiss = Some(message);
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Layer<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.menu), Tree::new(&self.sub)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.menu.as_widget(), self.sub.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.max();
        let loose = layout::Limits::new(Size::ZERO, size);
        let menu = self
            .menu
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &loose);
        let sub = self
            .sub
            .as_widget_mut()
            .layout(&mut tree.children[1], renderer, &loose);
        let (menu_at, sub_at) = place(
            self.at,
            menu.size(),
            self.sub_row.map(|row| (sub.size(), row)),
            size,
        );
        layout::Node::with_children(
            size,
            vec![
                menu.move_to(menu_at),
                sub.move_to(sub_at.unwrap_or(Point::ORIGIN)),
            ],
        )
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
        if !self.open {
            if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
                && let Some(point) = cursor.position_in(layout.bounds())
                && let Some(on_point) = &self.on_point
            {
                shell.publish(on_point(point));
            }
            return;
        }
        let mut layouts = layout.children();
        let (Some(menu), Some(sub)) = (layouts.next(), layouts.next()) else {
            return;
        };
        let [menu_tree, sub_tree] = &mut tree.children[..] else {
            return;
        };
        if self.sub_row.is_some() {
            self.sub.as_widget_mut().update(
                sub_tree, event, sub, cursor, renderer, clipboard, shell, viewport,
            );
        }
        if !shell.is_event_captured() {
            self.menu.as_widget_mut().update(
                menu_tree, event, menu, cursor, renderer, clipboard, shell, viewport,
            );
        }
        if shell.is_event_captured() {
            return;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                let inside = cursor.is_over(menu.bounds())
                    || (self.sub_row.is_some() && cursor.is_over(sub.bounds()));
                if !inside && let Some(message) = &self.on_dismiss {
                    shell.publish(message.clone());
                }
                shell.capture_event();
            }
            // The list under the menu stays where it is.
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => shell.capture_event(),
            _ => {}
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
        if !self.open {
            return mouse::Interaction::None;
        }
        let mut layouts = layout.children();
        let (Some(menu), Some(sub)) = (layouts.next(), layouts.next()) else {
            return mouse::Interaction::None;
        };
        let over = if self.sub_row.is_some() && cursor.is_over(sub.bounds()) {
            self.sub.as_widget().mouse_interaction(
                &tree.children[1],
                sub,
                cursor,
                viewport,
                renderer,
            )
        } else {
            self.menu.as_widget().mouse_interaction(
                &tree.children[0],
                menu,
                cursor,
                viewport,
                renderer,
            )
        };
        // Anything but `None` keeps the pointer from what is under the
        // layer, so nothing there lights up while the menu is open.
        match over {
            mouse::Interaction::None => mouse::Interaction::Idle,
            over => over,
        }
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
        if !self.open {
            return;
        }
        let mut layouts = layout.children();
        let (Some(menu), Some(sub)) = (layouts.next(), layouts.next()) else {
            return;
        };
        self.menu.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            menu,
            cursor,
            viewport,
        );
        if self.sub_row.is_some() {
            // In a layer of its own, over the menu's text.
            renderer.with_layer(layout.bounds(), |renderer| {
                self.sub.as_widget().draw(
                    &tree.children[1],
                    renderer,
                    theme,
                    style,
                    sub,
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if !self.open {
            return;
        }
        let mut layouts = layout.children();
        let (Some(menu), Some(sub)) = (layouts.next(), layouts.next()) else {
            return;
        };
        let [menu_tree, sub_tree] = &mut tree.children[..] else {
            return;
        };
        self.menu
            .as_widget_mut()
            .operate(menu_tree, menu, renderer, operation);
        if self.sub_row.is_some() {
            self.sub
                .as_widget_mut()
                .operate(sub_tree, sub, renderer, operation);
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Layer<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(layer: Layer<'a, Message, Theme, Renderer>) -> Self {
        Element::new(layer)
    }
}

/// Where a menu of `menu` size opens for a pointer at `at` in a layer of
/// `window` size: its corner at the pointer, moved in from the right and
/// bottom edges. A submenu (its size, and how far down the menu its row
/// starts) opens at the menu's right with its first row level with that
/// row, or at the menu's left when the right has no room.
fn place(at: Point, menu: Size, sub: Option<(Size, f32)>, window: Size) -> (Point, Option<Point>) {
    let within =
        |value: f32, size: f32, room: f32| value.clamp(EDGE, (room - size - EDGE).max(EDGE));
    let menu_at = Point::new(
        within(at.x, menu.width, window.width),
        within(at.y, menu.height, window.height),
    );
    let sub_at = sub.map(|(size, row)| {
        let right = menu_at.x + menu.width - OVERLAP;
        let x = if right + size.width + EDGE > window.width {
            (menu_at.x - size.width + OVERLAP).max(EDGE)
        } else {
            right
        };
        let y = within(menu_at.y + row - PADDING, size.height, window.height);
        Point::new(x, y)
    });
    (menu_at, sub_at)
}

/// A menu's panel, `width` px wide: an optional header naming what the
/// menu is for, then its rows.
pub fn menu<'a, Message: 'a>(
    head: Option<String>,
    rows: impl IntoIterator<Item = Element<'a, Message>>,
    width: f32,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let mut panel = Column::new();
    if let Some(head) = head {
        panel = panel.push(
            container(
                text(head)
                    .size(12)
                    .font(font::SEMIBOLD)
                    .color(tokens.sub)
                    .wrapping(text::Wrapping::None),
            )
            .height(HEADER)
            .width(Fill)
            .padding([0, 10])
            .align_y(Center)
            .clip(true),
        );
    }
    container(panel.extend(rows))
        .width(width)
        .padding(PADDING)
        .style(move |_| panel_style(&tokens, 10.0))
        .into()
}

/// What a menu row shows at its right, and how it reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowLook {
    pub label: String,
    /// A shortcut ("Ctrl+click") or the current value ("8/10").
    pub hint: String,
    /// The row opens a submenu: a chevron at its right.
    pub arrow: bool,
    /// Its submenu is open.
    pub active: bool,
    /// In the danger color: Delete….
    pub danger: bool,
}

/// One row of a menu.
pub fn menu_row<'a, Message: Clone + 'a>(
    look: RowLook,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let color = if look.danger {
        tokens.danger
    } else {
        tokens.text
    };
    let mut content = row![
        text(look.label)
            .size(13.5)
            .wrapping(text::Wrapping::None)
            .width(Length::Shrink),
        iced::widget::space::horizontal(),
    ]
    .spacing(10)
    .align_y(Center)
    .height(Fill);
    if !look.hint.is_empty() {
        content = content.push(
            container(
                text(look.hint)
                    .size(12)
                    .color(tokens.sub)
                    .wrapping(text::Wrapping::None),
            )
            .max_width(124)
            .clip(true),
        );
    }
    if look.arrow {
        content = content.push(icon(Icon::ChevronRight, 13.0, color));
    }
    let active = look.active;
    button(content)
        .width(Fill)
        .height(ROW)
        .padding([0, 10])
        .on_press(on_press)
        .style(move |_, status| item_style(&tokens, color, active, status))
        .focusable(7.0, &tokens)
}

/// A row of a submenu: a check while `on`, then the label.
pub fn check_row<'a, Message: Clone + 'a>(
    label: String,
    on: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let mark: Element<'a, Message> = if on {
        icon(Icon::Check, 13.0, tokens.ink)
    } else {
        Space::new().into()
    };
    let content = row![
        container(mark).width(14),
        container(text(label).size(13.5).wrapping(text::Wrapping::None))
            .width(Fill)
            .clip(true),
    ]
    .spacing(8)
    .align_y(Center)
    .height(Fill);
    button(content)
        .width(Fill)
        .height(ROW)
        .padding([0, 10])
        .on_press(on_press)
        .style(move |_, status| item_style(&tokens, tokens.text, false, status))
        .focusable(7.0, &tokens)
}

fn item_style(
    tokens: &Tokens,
    color: Color,
    active: bool,
    status: button::Status,
) -> button::Style {
    let background = match status {
        _ if active => Some(tokens.selection),
        button::Status::Hovered | button::Status::Pressed => Some(tokens.hover),
        _ => None,
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: color,
        border: Border::default().rounded(7),
        ..button::Style::default()
    }
}

/// A line between two groups of rows.
pub fn separator<'a, Message: 'a>(tokens: &Tokens) -> Element<'a, Message> {
    let line = tokens.line;
    container(
        container(Space::new())
            .width(Fill)
            .height(1)
            .style(move |_| container::background(line)),
    )
    .height(SEPARATOR)
    .padding([0, 6])
    .align_y(Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1600.0, 900.0);
    const MENU: Size = Size::new(WIDTH, 400.0);

    #[test]
    fn the_menu_opens_at_the_pointer_and_stays_in_the_window() {
        let (at, sub) = place(Point::new(548.0, 236.0), MENU, None, WINDOW);
        assert_eq!((at, sub), (Point::new(548.0, 236.0), None));
        let (at, _) = place(Point::new(1590.0, 880.0), MENU, None, WINDOW);
        assert_eq!(at, Point::new(1600.0 - WIDTH - EDGE, 900.0 - 400.0 - EDGE));
        // A window smaller than the menu shows its top left.
        let (at, _) = place(
            Point::new(100.0, 100.0),
            MENU,
            None,
            Size::new(200.0, 300.0),
        );
        assert_eq!(at, Point::new(EDGE, EDGE));
    }

    #[test]
    fn a_submenu_opens_beside_its_row_or_on_the_left_without_room() {
        let sub = Size::new(SUB_WIDTH, 140.0);
        let row = PADDING + HEADER + 4.0 * ROW;
        let (menu, at) = place(Point::new(548.0, 236.0), MENU, Some((sub, row)), WINDOW);
        assert_eq!(
            at,
            Some(Point::new(
                menu.x + WIDTH - OVERLAP,
                menu.y + HEADER + 4.0 * ROW
            ))
        );
        let (menu, at) = place(Point::new(1300.0, 236.0), MENU, Some((sub, row)), WINDOW);
        assert_eq!(at.unwrap().x, menu.x - SUB_WIDTH + OVERLAP);
        // Near the bottom the submenu moves up into the window.
        let (_, at) = place(Point::new(548.0, 600.0), MENU, Some((sub, 380.0)), WINDOW);
        assert_eq!(at.unwrap().y, 900.0 - 140.0 - EDGE);
    }
}
