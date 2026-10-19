use iced::widget::{button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use crate::theme::{Tokens, font};

/// A labeled on/off switch: the label on the left, a 30 × 18 track on the
/// right, the accent when on.
pub fn switch<'a, Message: Clone + 'a>(
    label: &'a str,
    on: bool,
    tokens: &Tokens,
    on_toggle: Message,
) -> Element<'a, Message> {
    let label = text(label).size(13.5).width(Fill);
    let content = row![label, track(on, tokens)]
        .spacing(10)
        .align_y(Center)
        .height(Fill);
    toggle(content, 30.0, tokens, on_toggle)
}

/// A setting with a name and help on the left and the switch at the top
/// right; the whole row toggles it.
pub fn setting_switch<'a, Message: Clone + 'a>(
    name: &'a str,
    help: &'a str,
    on: bool,
    tokens: &Tokens,
    on_toggle: Message,
) -> Element<'a, Message> {
    let label = column![
        text(name).size(13).font(font::SEMIBOLD),
        text(help).size(12.5).color(tokens.sub),
    ]
    .spacing(3)
    .width(Fill);
    let content = row![label, track(on, tokens)].spacing(14);
    toggle(content, iced::Shrink, tokens, on_toggle)
}

fn track<'a, Message: 'a>(on: bool, tokens: &Tokens) -> Element<'a, Message> {
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
    track.into()
}

fn toggle<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    height: impl Into<iced::Length>,
    tokens: &Tokens,
    on_toggle: Message,
) -> Element<'a, Message> {
    let tokens = *tokens;
    button(content)
        .width(Fill)
        .height(height)
        .padding(0)
        .on_press(on_toggle)
        .style(move |_, _| button::Style {
            background: None::<Background>,
            text_color: tokens.text,
            ..button::Style::default()
        })
        .into()
}
