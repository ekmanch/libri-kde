//! Modal dialogs: a panel centered over a scrim that covers the window.

use iced::widget::{center, column, container, mouse_area, opaque, row, stack, text};
use iced::{Center, Element, Length};

use super::{Icon, icon_button, panel_style};
use crate::theme::{Tokens, font};

/// `base` with `dialog` over it while it is `Some`. A click on the scrim
/// sends `on_dismiss`.
pub fn modal<'a, Message: Clone + 'a>(
    base: impl Into<Element<'a, Message>>,
    dialog: Option<Element<'a, Message>>,
    on_dismiss: Message,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let Some(dialog) = dialog else {
        return base.into();
    };
    let scrim = tokens.scrim;
    let layer = mouse_area(center(opaque(dialog)).style(move |_| container::background(scrim)))
        .on_press(on_dismiss);
    stack![base.into(), opaque(layer)].into()
}

/// The dialog panel: a title with a close button, the body, and a footer
/// of buttons. `width` is the panel's width in px.
pub fn dialog<'a, Message: Clone + 'a>(
    title: impl text::IntoFragment<'a>,
    body: impl Into<Element<'a, Message>>,
    footer: impl Into<Element<'a, Message>>,
    width: impl Into<Length>,
    tokens: &Tokens,
    on_close: Message,
) -> Element<'a, Message> {
    let header = row![
        text(title).size(19).font(font::EXTRABOLD),
        iced::widget::space::horizontal(),
        icon_button(Icon::Close, 16.0, tokens, on_close),
    ]
    .align_y(Center);
    let tokens = *tokens;
    container(column![header, body.into(), footer.into()].spacing(14))
        .width(width)
        .padding(iced::Padding::new(18.0).bottom(14))
        .style(move |_| panel_style(&tokens, 14.0))
        .into()
}
