use iced::widget::{button, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use crate::theme::Tokens;

/// A labeled on/off switch: the label on the left, a 30 × 18 track on the
/// right, the accent when on.
pub fn switch<'a, Message: Clone + 'a>(
    label: &'a str,
    on: bool,
    tokens: &Tokens,
    on_toggle: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    let (track, knob) = if on {
        (tokens.accent, tokens.on_accent)
    } else {
        (tokens.line, tokens.sub)
    };
    let knob = container(space())
        .width(14)
        .height(14)
        .style(move |_| container::background(knob).border(Border::default().rounded(7)));
    let track = container(knob)
        .width(30)
        .height(18)
        .padding(Padding::new(2.0))
        .align_x(if on {
            iced::alignment::Horizontal::Right
        } else {
            iced::alignment::Horizontal::Left
        })
        .style(move |_| container::background(track).border(Border::default().rounded(9)));
    button(
        row![text(label).size(13.5).width(Fill), track]
            .spacing(10)
            .align_y(Center)
            .height(Fill),
    )
    .width(Fill)
    .height(30)
    .padding(0)
    .on_press(on_toggle)
    .style(move |_, _| button::Style {
        background: None::<Background>,
        text_color: tokens.text,
        ..button::Style::default()
    })
    .into()
}
