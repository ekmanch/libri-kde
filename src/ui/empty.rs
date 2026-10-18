use iced::widget::{Column, column, container, text};
use iced::{Center, Element, Fill};

use super::{ButtonKind, Icon, icon, labeled_button};
use crate::theme::{Tokens, font};

/// A centered message for a list with nothing to show: an icon, a title,
/// a line of explanation and an optional button.
pub fn empty_state<'a, Message: Clone + 'a>(
    which: Icon,
    title: String,
    body: String,
    action: Option<(&'a str, Message)>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let mut content = Column::new()
        .push(icon(which, 36.0, tokens.sub))
        .push(
            column![
                text(title).size(15).font(font::SEMIBOLD).color(tokens.text),
                text(body).size(13).color(tokens.sub).center(),
            ]
            .spacing(6)
            .align_x(Center),
        )
        .spacing(10)
        .align_x(Center)
        .max_width(360);
    if let Some((label, message)) = action {
        content = content.push(labeled_button(
            label,
            ButtonKind::Neutral,
            *tokens,
            Some(message),
        ));
    }
    container(content).center(Fill).into()
}
