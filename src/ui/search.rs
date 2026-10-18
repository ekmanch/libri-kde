use iced::widget::{container, row, text_input};
use iced::{Background, Border, Center, Color, Element, Fill};

use super::{Icon, icon, mono};
use crate::theme::{Tokens, size};

/// The search box: a magnifier, the field, and an optional shortcut hint
/// ("Ctrl K") on the right. One outline around all of it.
pub fn search_field<'a, Message: Clone + 'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    shortcut: Option<&'a str>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let input = text_input(placeholder, value)
        .on_input(on_input)
        .size(size::BODY)
        .padding(0)
        .style(move |_, _| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: tokens.sub,
            placeholder: tokens.sub,
            value: tokens.text,
            selection: tokens.selection,
        });
    let mut content = row![icon(Icon::Search, 16.0, tokens.sub), input]
        .spacing(8)
        .align_y(Center);
    if let Some(shortcut) = shortcut {
        let line = tokens.line;
        content = content.push(
            container(mono(shortcut, 11.0, tokens.sub))
                .padding([1, 6])
                .style(move |_| {
                    container::Style::default()
                        .border(Border::default().rounded(5).width(1).color(line))
                }),
        );
    }
    container(content)
        .height(36)
        .width(Fill)
        .padding([0, 10])
        .align_y(Center)
        .style(move |_| {
            container::background(tokens.field)
                .border(Border::default().rounded(8).width(1).color(tokens.line))
        })
        .into()
}
