//! The rating control: a button showing the rating that opens a 1 to 10
//! grid. Never a slider.

use iced::widget::{Column, button, container, row, text};
use iced::{Background, Border, Center, Element, Fill};

use super::{Align, Icon, Popover, icon, panel_style, row_style};
use crate::theme::{Tokens, font};

pub fn rating<'a, Message: Clone + 'a>(
    value: Option<u8>,
    open: bool,
    on_toggle: Message,
    on_pick: impl Fn(Option<u8>) -> Message + 'a,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let label = match value {
        Some(n) => row![
            text(n.to_string()).size(15).font(font::SEMIBOLD),
            text("/ 10").size(15).color(tokens.sub),
        ],
        None => row![text("Not rated").size(15).font(font::SEMIBOLD)],
    };
    let anchor = {
        let tokens = *tokens;
        button(
            row![label.spacing(5), icon(Icon::ChevronDown, 14.0, tokens.sub)]
                .spacing(9)
                .align_y(Center)
                .height(Fill),
        )
        .height(36)
        .padding(iced::Padding::new(0.0).left(12).right(10))
        .on_press(on_toggle.clone())
        .style(move |_, _| button::Style {
            background: Some(Background::Color(tokens.field)),
            text_color: tokens.text,
            border: Border::default().rounded(8).width(1).color(tokens.line),
            ..button::Style::default()
        })
    };
    let picker = open.then(|| picker(value, on_pick, tokens));
    Popover::new(anchor, picker)
        .align(Align::Start)
        .on_dismiss(on_toggle)
        .into()
}

/// A compact rating for a list row, an album's track: "9" or a gray
/// "Rate", opening the same picker.
pub fn track_rating<'a, Message: Clone + 'a>(
    value: Option<u8>,
    open: bool,
    on_toggle: Message,
    on_pick: impl Fn(Option<u8>) -> Message + 'a,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens_copy = *tokens;
    let label = match value {
        Some(n) => text(format!("{n}/10"))
            .font(font::MONO_MEDIUM)
            .size(13)
            .color(tokens.text),
        None => text("Rate").size(13).color(tokens.sub),
    };
    let anchor = button(container(label).center_y(Fill))
        .height(28)
        .padding([0, 10])
        .on_press(on_toggle.clone())
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered)
                .then_some(Background::Color(tokens_copy.hover)),
            border: Border::default()
                .rounded(7)
                .width(1)
                .color(tokens_copy.line),
            ..button::Style::default()
        });
    let picker = open.then(|| picker(value, on_pick, tokens));
    Popover::new(anchor, picker)
        .align(Align::End)
        .on_dismiss(on_toggle)
        .into()
}

/// Five by two, the current rating in the accent, then "Clear rating".
fn picker<'a, Message: Clone + 'a>(
    value: Option<u8>,
    on_pick: impl Fn(Option<u8>) -> Message + 'a,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let cell = |n: u8| {
        let on = value == Some(n);
        button(container(text(n.to_string()).size(14).font(font::SEMIBOLD)).center(Fill))
            .width(40)
            .height(36)
            .padding(0)
            .on_press(on_pick(Some(n)))
            .style(move |_, status| {
                let background = if on {
                    Some(tokens.accent)
                } else if matches!(status, button::Status::Hovered) {
                    Some(tokens.hover)
                } else {
                    None
                };
                button::Style {
                    background: background.map(Background::Color),
                    text_color: if on { tokens.on_accent } else { tokens.text },
                    border: Border::default().rounded(7).width(1).color(if on {
                        tokens.accent
                    } else {
                        tokens.line
                    }),
                    ..button::Style::default()
                }
            })
            .into()
    };
    let grid = Column::with_children([1u8, 6].map(|first| {
        iced::widget::Row::with_children((first..first + 5).map(&cell))
            .spacing(6)
            .into()
    }))
    .spacing(6);
    let clear = button(container(text("Clear rating").size(13).color(tokens.sub)).center_x(Fill))
        .width(Fill)
        .height(30)
        .on_press(on_pick(None))
        .style(row_style(&tokens, false, 7.0));
    container(Column::new().push(grid).push(clear).spacing(8).width(224))
        .padding(10)
        .style(move |_| panel_style(&tokens, 10.0))
        .into()
}
