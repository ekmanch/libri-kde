use iced::widget::{column, text, text_input};
use iced::{Background, Border, Element, Font};

use crate::theme::{Tokens, font};

/// A 38 px text field: the field color, a line border that turns to the
/// accent while typing.
pub fn text_field<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    field_font: Font,
    tokens: &Tokens,
) -> text_input::TextInput<'a, Message> {
    let tokens = *tokens;
    text_input(placeholder, value)
        .on_input(on_input)
        .font(field_font)
        .size(if field_font == font::MONO { 14 } else { 15 })
        .padding([9, 12])
        .style(move |_, status| text_input::Style {
            background: Background::Color(tokens.field),
            border: Border::default().rounded(8).width(1).color(match status {
                text_input::Status::Focused { .. } => tokens.ink,
                _ => tokens.line,
            }),
            icon: tokens.sub,
            placeholder: tokens.sub,
            value: tokens.text,
            selection: tokens.selection,
        })
}

/// A form field: its label in small gray SemiBold, then the control.
pub fn labeled<'a, Message: 'a>(
    label: &str,
    control: impl Into<Element<'a, Message>>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    column![
        text(label.to_owned())
            .size(12.5)
            .font(font::SEMIBOLD)
            .color(tokens.sub),
        control.into()
    ]
    .spacing(6)
    .into()
}
