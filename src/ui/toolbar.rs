//! Pieces of the library toolbar: outlined menu buttons and the view
//! switcher.

use iced::widget::{Row, button, container, row, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Icon, icon};
use crate::theme::{Tokens, font};

/// A 36 px outlined button that opens a popover: an optional icon, the
/// content, an optional accent badge with a number, and a chevron. The
/// selection gray while `active` (open, or filtering).
pub fn menu_button<'a, Message: Clone + 'a>(
    leading: Option<Icon>,
    content: impl Into<Element<'a, Message>>,
    badge: Option<usize>,
    active: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let mut inner = Row::new().spacing(6).align_y(Center).height(Fill);
    if let Some(leading) = leading {
        inner = inner.push(icon(leading, 15.0, tokens.text));
    }
    inner = inner.push(content);
    if let Some(count) = badge {
        inner = inner.push(
            container(
                text(count.to_string())
                    .size(11.5)
                    .font(font::BOLD)
                    .color(tokens.on_accent),
            )
            .height(18)
            .padding([0, 5])
            .center_y(18)
            .style(move |_| {
                container::background(tokens.accent).border(Border::default().rounded(9))
            }),
        );
    }
    inner = inner.push(icon(Icon::ChevronDown, 14.0, tokens.text));
    button(inner)
        .height(36)
        .padding(Padding::new(0.0).left(12).right(10))
        .on_press(on_press)
        .style(move |_, status| button::Style {
            background: (active || matches!(status, button::Status::Hovered)).then_some(
                Background::Color(if active {
                    tokens.selection
                } else {
                    tokens.hover
                }),
            ),
            text_color: tokens.text,
            border: Border::default().rounded(8).width(1).color(tokens.line),
            ..button::Style::default()
        })
        .into()
}

/// Icon buttons joined in one outlined group; the picked one in the
/// selection gray.
pub fn icon_switch<'a, Message: Clone + 'a>(
    items: impl IntoIterator<Item = (Icon, bool, Message)>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let buttons = items.into_iter().map(|(which, on, message)| {
        let color = if on { tokens.text } else { tokens.sub };
        button(container(icon(which, 16.0, color)).center(Fill))
            .width(38)
            .height(34)
            .padding(0)
            .on_press(message)
            .style(move |_, status| button::Style {
                background: (on || matches!(status, button::Status::Hovered)).then_some(
                    Background::Color(if on { tokens.selection } else { tokens.hover }),
                ),
                ..button::Style::default()
            })
            .into()
    });
    container(Row::with_children(buttons))
        .padding(1)
        .style(move |_| {
            container::Style::default()
                .border(Border::default().rounded(8).width(1).color(tokens.line))
        })
        .clip(true)
        .into()
}

/// The accent "Add" button with a plus.
pub fn add_button<'a, Message: Clone + 'a>(
    label: &'a str,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    button(
        row![
            icon(Icon::Plus, 16.0, tokens.on_accent),
            text(label).font(font::SEMIBOLD)
        ]
        .spacing(6)
        .align_y(Center)
        .height(Fill),
    )
    .height(36)
    .padding([0, 14])
    .on_press(on_press)
    .style(move |_, status| button::Style {
        background: Some(Background::Color(
            if matches!(status, button::Status::Hovered) {
                tokens.accent.scale_alpha(0.85)
            } else {
                tokens.accent
            },
        )),
        text_color: tokens.on_accent,
        border: Border::default().rounded(8),
        ..button::Style::default()
    })
    .into()
}
