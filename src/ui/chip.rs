//! Chips: filter options, status filters with counts, active filters that
//! can be removed, and an entry's tags.

use iced::widget::{Row, button, row, text};
use iced::{Background, Border, Center, Color, Element, Padding};

use super::{FocusExt, Icon, figure, icon};
use crate::theme::{Tokens, font};

/// An option in the Filter popover and the tag picker: 30 px, the
/// selection gray when on. `count` shows after the label.
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
        content = content.push(figure(count.to_string(), 11.0, tokens.sub));
    }
    chip(
        content,
        30.0,
        Padding::from([0, 11]),
        tokens.text,
        on.then_some(tokens.selection),
        tokens,
        on_press,
    )
}

/// A status filter under the title: "Want 12". Gray text until picked,
/// then accent-colored: the solid accent with dark text on the dark
/// themes, a pale tint of it on Light (`Tokens::status_fill`).
pub fn status_chip<'a, Message: Clone + 'a>(
    label: &'a str,
    count: usize,
    on: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let fg = if on { tokens.on_status() } else { tokens.sub };
    let content = row![
        text(label).size(13).font(font::MEDIUM),
        figure(count.to_string(), 11.5, fg.scale_alpha(0.75)),
    ]
    .spacing(6)
    .align_y(Center);
    chip(
        content,
        28.0,
        Padding::from([0, 12]),
        fg,
        on.then_some(tokens.status_fill()),
        tokens,
        on_press,
    )
}

/// A tag the search suggests as a filter: "#classic 79", outlined like
/// a status chip that is not picked, the count in the secondary color.
/// The mockup's border is dashed; iced 0.14 draws only solid borders.
pub fn suggested_chip<'a, Message: Clone + 'a>(
    label: String,
    count: usize,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let content = row![
        text(label).size(13).font(font::MEDIUM),
        figure(count.to_string(), 11.5, tokens.sub),
    ]
    .spacing(6)
    .align_y(Center);
    chip(
        content,
        28.0,
        Padding::from([0, 11]),
        tokens.text,
        None,
        tokens,
        on_press,
    )
}

/// An active filter in the chips row, removed by a click: "#horror ×",
/// with a type's dot in front for a picked type, and a tag's role after
/// it in the secondary color ("#clint-eastwood · Director"). Filled as
/// the sidebar's selected row is: gray on the dark themes, the accent's
/// tint on Light.
pub fn removable_chip<'a, Message: Clone + 'a>(
    label: String,
    detail: Option<String>,
    dot: Option<Color>,
    tokens: &Tokens,
    on_remove: Message,
) -> Element<'a, Message> {
    let mut content = Row::new().spacing(6).align_y(Center);
    if let Some(color) = dot {
        content = content.push(super::dot(color, 8.0, 3.0));
    }
    content = content.push(text(label).size(13).font(font::SEMIBOLD));
    if let Some(detail) = detail {
        content = content.push(text(format!("· {detail}")).size(12.5).color(tokens.sub));
    }
    let content = content.push(icon(Icon::Close, 12.0, tokens.text));
    chip(
        content,
        28.0,
        Padding::new(0.0).left(12).right(8),
        tokens.text,
        Some(tokens.selection_tint),
        tokens,
        on_remove,
    )
}

/// One of an entry's tags, not clickable, with its roles after it in the
/// secondary color for a tag in several categories ("· Director, Actor").
pub fn tag_chip<'a, Message: 'a>(
    label: String,
    detail: Option<String>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let raised = tokens.raised;
    let mut content = Row::new()
        .spacing(5)
        .align_y(Center)
        .push(text(label).size(13));
    if let Some(detail) = detail {
        content = content.push(text(format!("· {detail}")).size(12.5).color(tokens.sub));
    }
    iced::widget::container(content)
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
        .focusable(13.0, &tokens)
}

fn chip<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    height: f32,
    padding: Padding,
    fg: Color,
    // The fill of a chip that is on.
    on: Option<Color>,
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
        let background = if on.is_some() {
            on
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
                .color(on.unwrap_or(tokens.line)),
            ..button::Style::default()
        }
    })
    .focusable(height / 2.0, &tokens)
}
