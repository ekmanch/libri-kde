//! Widgets shared by every screen. Anything that repeats across screens
//! (rows, chips, buttons, search field) lives here, never copied.

pub mod icon;

use iced::alignment::Vertical;
use iced::widget::{self, button, text};
use iced::{Background, Border, Element, Fill, Padding};

use crate::theme::{Tokens, font, size};

pub use icon::{Icon, icon};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent fill.
    Primary,
    /// Line border, no fill.
    Outline,
    /// Raised fill.
    Neutral,
    /// Raised fill with red text.
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
    widget::button(
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
        ButtonKind::Destructive => (Some(tokens.raised), tokens.danger, None),
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
            color: border.unwrap_or(iced::Color::TRANSPARENT),
            width: if border.is_some() { 1.0 } else { 0.0 },
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}
