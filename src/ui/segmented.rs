use iced::widget::{Row, button, container, text};
use iced::{Background, Border, Element, Fill};

use crate::theme::{Tokens, font};

/// Joined buttons in one outline, sharing the width; the picked one in
/// the selection gray: Status in the details and the entry form.
pub fn segmented<'a, Message: Clone + 'a>(
    items: impl IntoIterator<Item = (&'a str, bool, Message)>,
    height: f32,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let items: Vec<_> = items.into_iter().collect();
    let last = items.len().saturating_sub(1);
    let buttons = items
        .into_iter()
        .enumerate()
        .map(|(index, (label, on, message))| {
            let segment = button(container(text(label).size(13).font(font::SEMIBOLD)).center(Fill))
                .width(Fill)
                .height(height)
                .padding(0)
                .on_press(message)
                .style(move |_, status| button::Style {
                    background: if on {
                        Some(Background::Color(tokens.selection))
                    } else {
                        matches!(status, button::Status::Hovered)
                            .then_some(Background::Color(tokens.hover))
                    },
                    text_color: if on { tokens.text } else { tokens.sub },
                    ..button::Style::default()
                });
            // A line between neighbors.
            let line = tokens.line;
            let divider = container(iced::widget::space())
                .width(if index < last { 1 } else { 0 })
                .height(height)
                .style(move |_| container::background(line));
            Row::new().push(segment).push(divider).width(Fill).into()
        });
    container(Row::with_children(buttons))
        .padding(1)
        .clip(true)
        .style(move |_| {
            container::Style::default()
                .border(Border::default().rounded(8).width(1).color(tokens.line))
        })
        .into()
}
