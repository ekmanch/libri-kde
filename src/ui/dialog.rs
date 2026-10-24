//! Modal dialogs: a panel centered over a scrim that covers the window.

use iced::widget::{Space, center, column, container, mouse_area, opaque, row, stack, text};
use iced::{Center, Element, Length};

use super::{Barrier, Icon, icon_button, panel_style};
use crate::theme::{Tokens, font};

/// `base` with `dialog` over it while it is `Some`. A click on the scrim
/// sends `on_dismiss`.
///
/// The widget tree keeps the same shape with and without a dialog: a stack
/// of the barrier and the layer, the layer empty when there is no dialog.
/// Were the stack only there with a dialog, iced would rebuild the whole
/// tree when one opens and the screen under it would lose its state: its
/// scroll positions, text input cursors and so on.
pub fn modal<'a, Message: Clone + 'a>(
    base: impl Into<Element<'a, Message>>,
    dialog: Option<Element<'a, Message>>,
    on_dismiss: Message,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let scrim = tokens.scrim;
    let active = dialog.is_some();
    let layer: Element<'a, Message> = match dialog {
        Some(dialog) => opaque(
            mouse_area(center(opaque(dialog)).style(move |_| container::background(scrim)))
                .on_press(on_dismiss),
        ),
        None => Space::new().width(0).height(0).into(),
    };
    // Focus and keys stay in the dialog.
    stack![Barrier::new(base).active(active), layer].into()
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
    panel(header, body, footer, width, tokens)
}

/// A dialog panel with a title row of its own: a drill-in view with a
/// back arrow.
pub fn panel<'a, Message: 'a>(
    header: impl Into<Element<'a, Message>>,
    body: impl Into<Element<'a, Message>>,
    footer: impl Into<Element<'a, Message>>,
    width: impl Into<Length>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    container(column![header.into(), body.into(), footer.into()].spacing(14))
        .width(width)
        .padding(iced::Padding::new(18.0).bottom(14))
        .style(move |_| panel_style(&tokens, 14.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{PEACH, Theme, Tokens};

    /// Opening a dialog must not change the root widget, or iced rebuilds
    /// the tree and the screen under the dialog loses its scroll position.
    #[test]
    fn the_tree_keeps_its_shape_with_a_dialog() {
        let tokens = Tokens::new(Theme::Graphite, PEACH);
        let without: Element<'_, ()> = modal(text("base"), None, (), &tokens);
        let with: Element<'_, ()> = modal(text("base"), Some(text("dialog").into()), (), &tokens);
        assert_eq!(without.as_widget().tag(), with.as_widget().tag());
        assert_eq!(
            without.as_widget().children()[0].tag,
            with.as_widget().children()[0].tag
        );
    }
}
