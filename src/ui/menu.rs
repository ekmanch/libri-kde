use iced::widget::{Column, button, container, row, text};
use iced::{Background, Border, Center, Element};

use super::{FocusExt, panel_style, row_style, section_header};
use crate::theme::Tokens;

/// A menu of mutually exclusive choices with radio rings, as the Sort menu:
/// a gray caps title, then one 36 px row per item.
pub fn radio_menu<'a, Message: Clone + 'a>(
    title: &str,
    items: impl IntoIterator<Item = (&'a str, bool, Message)>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let rows = items.into_iter().map(|(label, on, message)| {
        button(
            row![radio(on, tokens), text(label).size(14)]
                .spacing(10)
                .align_y(Center)
                .height(iced::Fill),
        )
        .width(iced::Fill)
        .height(36)
        .padding([0, 10])
        .on_press(message)
        .style({
            let tokens = *tokens;
            move |theme, status| {
                let mut style = row_style(&tokens, false, 8.0)(theme, status);
                if on {
                    style.background = Some(Background::Color(tokens.hover));
                }
                style
            }
        })
        .focusable(8.0, tokens)
    });
    let header = container(section_header(title, tokens)).padding([8, 10]);
    container(Column::new().push(header).extend(rows))
        .width(240)
        .padding(6)
        .style({
            let tokens = *tokens;
            move |_| panel_style(&tokens, 12.0)
        })
        .into()
}

/// A 16 px radio ring, with the accent dot when on.
pub fn radio<'a, Message: 'a>(on: bool, tokens: &Tokens) -> Element<'a, Message> {
    let (ring, dot) = if on {
        (tokens.accent, Some(tokens.accent))
    } else {
        (tokens.sub, None)
    };
    let inner = container(iced::widget::space())
        .width(8)
        .height(8)
        .style(move |_| container::Style {
            background: dot.map(Background::Color),
            border: Border::default().rounded(4),
            ..container::Style::default()
        });
    container(inner)
        .width(16)
        .height(16)
        .center(16)
        .style(move |_| {
            container::Style::default().border(Border::default().rounded(8).width(2).color(ring))
        })
        .into()
}
