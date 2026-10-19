//! Chips: filter options, status filters with counts, active filters that
//! can be removed, and an entry's tags.

use iced::widget::{Row, button, row, text};
use iced::{Background, Border, Center, Color, Element, Padding};

use super::{Icon, icon, mono};
use crate::theme::{Tokens, font};

/// An option in the Filter popover and the tag picker: 30 px, the
/// selection gray when on. `count` shows in mono after the label.
pub fn filter_chip<'a, Message: Clone + 'a>(
    label: String,
    dot: Option<Color>,
    count: Option<usize>,
    on: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let mut content = Row::new().spacing(6).align_y(Center);
    if let Some(color) = dot {
        content = content.push(super::dot(color, 8.0, 3.0)).spacing(7);
    }
    content = content.push(text(label).size(13).font(font::MEDIUM));
    if let Some(count) = count {
        content = content.push(mono(count.to_string(), 11.0, tokens.sub));
    }
    chip(
        content,
        30.0,
        Padding::from([0, 11]),
        tokens.text,
        on,
        tokens,
        on_press,
    )
}

/// A status filter under the title: "Want 12". Gray text until picked.
pub fn status_chip<'a, Message: Clone + 'a>(
    label: &'a str,
    count: usize,
    on: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let fg = if on { tokens.text } else { tokens.sub };
    let content = row![
        text(label).size(13).font(font::MEDIUM),
        mono(count.to_string(), 11.5, fg.scale_alpha(0.75)),
    ]
    .spacing(6)
    .align_y(Center);
    chip(
        content,
        28.0,
        Padding::from([0, 12]),
        fg,
        on,
        tokens,
        on_press,
    )
}

/// An active filter in the chips row, removed by a click: "#horror ×",
/// with a type's dot in front for a picked type.
pub fn removable_chip<'a, Message: Clone + 'a>(
    label: String,
    dot: Option<Color>,
    tokens: &Tokens,
    on_remove: Message,
) -> Element<'a, Message> {
    let mut content = Row::new().spacing(6).align_y(Center);
    if let Some(color) = dot {
        content = content.push(super::dot(color, 8.0, 3.0));
    }
    let content = content
        .push(text(label).size(13).font(font::SEMIBOLD))
        .push(icon(Icon::Close, 12.0, tokens.text));
    chip(
        content,
        28.0,
        Padding::new(0.0).left(12).right(8),
        tokens.text,
        true,
        tokens,
        on_remove,
    )
}

/// One of an entry's tags, not clickable.
pub fn tag_chip<'a, Message: 'a>(label: String, tokens: &Tokens) -> Element<'a, Message> {
    let raised = tokens.raised;
    iced::widget::container(text(label).size(13))
        .height(26)
        .padding([0, 10])
        .align_y(Center)
        .style(move |_| {
            iced::widget::container::background(raised).border(Border::default().rounded(13))
        })
        .into()
}

/// "+ Tag": an outlined chip in the secondary color. The mockup's border
/// is dashed; iced 0.14 draws only solid borders.
pub fn add_chip<'a, Message: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    button(text(label).size(13).height(iced::Fill).align_y(Center))
        .height(26)
        .padding([0, 10])
        .on_press(on_press)
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered)
                .then_some(Background::Color(tokens.hover)),
            text_color: tokens.sub,
            border: Border::default().rounded(13).width(1).color(tokens.line),
            ..button::Style::default()
        })
        .into()
}

fn chip<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    height: f32,
    padding: Padding,
    fg: Color,
    on: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    button(
        iced::widget::container(content)
            .height(iced::Fill)
            .align_y(Center),
    )
    .height(height)
    .padding(padding)
    .on_press(on_press)
    .style(move |_, status| {
        let background = if on {
            Some(tokens.selection)
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(tokens.hover)
        } else {
            None
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: fg,
            border: Border::default()
                .rounded(height / 2.0)
                .width(1)
                .color(if on { tokens.selection } else { tokens.line }),
            ..button::Style::default()
        }
    })
    .into()
}
