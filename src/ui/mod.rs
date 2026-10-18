//! Widgets shared by every screen. Anything that repeats across screens
//! (rows, chips, buttons, search field) lives here, never copied.

pub mod chip;
pub mod dialog;
pub mod empty;
pub mod entry;
pub mod icon;
pub mod menu;
pub mod popover;
pub mod rating;
pub mod search;
pub mod switch;
pub mod tag_group;
pub mod toolbar;

use iced::alignment::Vertical;
use iced::widget::{self, Text, button, container, scrollable, text};
use iced::{Background, Border, Color, Element, Fill, Padding, Shadow, Vector};

use crate::theme::{Tokens, font, size};

pub use chip::{add_chip, filter_chip, removable_chip, status_chip, tag_chip};
pub use dialog::{dialog, modal};
pub use empty::empty_state;
pub use entry::{EntryLook, entry_card, entry_list_row, entry_table_header, entry_table_row, rule};
pub use icon::{Icon, icon};
pub use menu::radio_menu;
pub use popover::{Align, Placement, Popover};
pub use rating::rating;
pub use search::search_field;
pub use switch::switch;
pub use tag_group::tag_group;
pub use toolbar::{add_button, icon_switch, menu_button};

/// A section header in small gray caps.
pub fn section_header<'a, Message>(label: &str, tokens: &Tokens) -> Element<'a, Message> {
    text(label.to_uppercase())
        .size(size::SECTION)
        .font(font::BOLD)
        .color(tokens.sub)
        .into()
}

/// A screen or dialog title.
pub fn heading<'a, Message>(
    label: impl text::IntoFragment<'a>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    text(label)
        .size(size::HEADING)
        .font(font::EXTRABOLD)
        .color(tokens.text)
        .into()
}

/// Years, counts and dates.
pub fn mono<'a>(value: impl text::IntoFragment<'a>, text_size: f32, color: Color) -> Text<'a> {
    text(value).size(text_size).font(font::MONO).color(color)
}

/// A small colored square or circle: type dots, the sync dot, swatches.
pub fn dot<'a, Message: 'a>(color: Color, diameter: f32, radius: f32) -> Element<'a, Message> {
    container(widget::space())
        .width(diameter)
        .height(diameter)
        .style(move |_| container::background(color).border(Border::default().rounded(radius)))
        .into()
}

/// The surface of popovers and dialogs.
pub fn panel_style(tokens: &Tokens, radius: f32) -> container::Style {
    container::Style {
        text_color: Some(tokens.text),
        background: Some(Background::Color(tokens.popover)),
        border: Border::default()
            .rounded(radius)
            .width(1)
            .color(tokens.line),
        shadow: Shadow {
            color: tokens.shadow,
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

/// A vertical scroll area with the mockup's thin gray scroller.
pub fn scroll<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> scrollable::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(4)
                .scroller_width(4)
                .margin(3),
        ))
        .style(|theme, status| {
            let mut style = scrollable::default(theme, status);
            let thumb = Color::from_rgba(0.5, 0.5, 0.5, 0.3);
            style.vertical_rail.background = None;
            style.vertical_rail.border = Border::default();
            style.vertical_rail.scroller.background = Background::Color(thumb);
            style.vertical_rail.scroller.border = Border::default().rounded(2);
            style
        })
}

/// A borderless button for rows and list items: the selection gray when
/// `selected`, a light hover otherwise.
pub fn row_style(
    tokens: &Tokens,
    selected: bool,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let tokens = *tokens;
    move |_, status| {
        let background = match status {
            _ if selected => Some(tokens.selection),
            button::Status::Hovered | button::Status::Pressed => Some(tokens.hover),
            _ => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: tokens.text,
            border: Border::default().rounded(radius),
            ..button::Style::default()
        }
    }
}

/// A 30 px square icon button in the secondary color: Close, Settings.
pub fn icon_button<'a, Message: Clone + 'a>(
    which: Icon,
    icon_size: f32,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    button(container(icon(which, icon_size, tokens.sub)).center(Fill))
        .width(30)
        .height(30)
        .padding(0)
        .on_press(on_press)
        .style(row_style(tokens, false, 8.0))
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent fill.
    Primary,
    /// Line border, no fill.
    Outline,
    /// Raised fill.
    Neutral,
    /// Line border, no fill, red text: Delete, Disconnect. Never a red
    /// fill.
    Destructive,
}

impl ButtonKind {
    pub const ALL: [ButtonKind; 4] = [
        ButtonKind::Primary,
        ButtonKind::Outline,
        ButtonKind::Neutral,
        ButtonKind::Destructive,
    ];
}

/// A 36 px button with a SemiBold label.
pub fn labeled_button<'a, Message: Clone + 'a>(
    label: &'a str,
    kind: ButtonKind,
    tokens: Tokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(size::BODY)
            .font(font::SEMIBOLD)
            .height(Fill)
            .align_y(Vertical::Center),
    )
    .height(36)
    .padding(Padding::from([0, 16]))
    .on_press_maybe(on_press)
    .style(move |_, status| button_style(kind, &tokens, status))
    .into()
}

fn button_style(kind: ButtonKind, tokens: &Tokens, status: button::Status) -> button::Style {
    let (fill, text_color, border) = match kind {
        ButtonKind::Primary => (Some(tokens.accent), tokens.on_accent, None),
        ButtonKind::Outline => (None, tokens.text, Some(tokens.line)),
        ButtonKind::Neutral => (Some(tokens.raised), tokens.text, None),
        ButtonKind::Destructive => (None, tokens.danger, Some(tokens.line)),
    };
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => {
            Some(fill.map_or(tokens.hover, |c| c.scale_alpha(0.85)))
        }
        _ => fill,
    };
    let disabled = status == button::Status::Disabled;
    let fill = if disabled {
        fill.map(|c| c.scale_alpha(0.5))
    } else {
        fill
    };
    button::Style {
        background: fill.map(Background::Color),
        text_color: if disabled {
            text_color.scale_alpha(0.5)
        } else {
            text_color
        },
        border: Border {
            color: border.unwrap_or(Color::TRANSPARENT),
            width: if border.is_some() { 1.0 } else { 0.0 },
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}
