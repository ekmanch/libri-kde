//! A season's status pill, which opens the season status menu: Not set,
//! Want, In progress, Done, DNF as radio rows, then "Mark seasons 1–28
//! Done" for catching up. Used by the inspector's Seasons section, the
//! all-seasons and episodes pages, and the form's season editor.

use iced::widget::{Column, button, column, container, row, text};
use iced::{Background, Border, Center, Element, Fill};

use super::{Align, FocusExt, Icon, Popover, icon, panel_style, row_style};
use crate::browse;
use crate::model::{EntryStatus, MediaType};
use crate::theme::{Tokens, font};

/// The status a menu row stands for, in menu order; `None` is Not set.
pub const CHOICES: [Option<EntryStatus>; 5] = [
    None,
    Some(EntryStatus::Want),
    Some(EntryStatus::InProgress),
    Some(EntryStatus::Done),
    Some(EntryStatus::Dnf),
];

/// "Done", "Watching", or "Not set".
pub fn status_text(media: MediaType, status: Option<EntryStatus>) -> &'static str {
    match status {
        Some(status) => browse::status_label(Some(media), status),
        None => "Not set",
    }
}

/// The pill with its menu. `catch_up` is the catch-up button's label and
/// message, when there are earlier seasons to mark Done.
pub fn season_pill<'a, Message: Clone + 'a>(
    media: MediaType,
    status: Option<EntryStatus>,
    open: bool,
    on_toggle: Message,
    on_pick: impl Fn(Option<EntryStatus>) -> Message + 'a,
    catch_up: Option<(String, Message)>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens_copy = *tokens;
    let label = status_text(media, status);
    let mut content = row![].spacing(5).align_y(Center).height(Fill);
    if status == Some(EntryStatus::Done) {
        content = content.push(icon(Icon::Check, 12.0, tokens.text));
    }
    content = content
        .push(text(label).size(12.5).font(font::MEDIUM))
        .push(icon(Icon::ChevronDown, 12.0, tokens.sub));
    let anchor = button(content)
        .height(26)
        .padding([0, 9])
        .on_press(on_toggle.clone())
        .style(move |_, hovered| {
            let (background, border) = match status {
                // "Not set" is quieter: no fill, a faint border.
                None => (None, tokens_copy.line.scale_alpha(0.7)),
                Some(EntryStatus::InProgress) => (Some(tokens_copy.raised), tokens_copy.raised),
                Some(_) => (None, tokens_copy.line),
            };
            let background = if matches!(hovered, button::Status::Hovered) {
                Some(tokens_copy.hover)
            } else {
                background
            };
            button::Style {
                background: background.map(Background::Color),
                text_color: if status.is_some() {
                    tokens_copy.text
                } else {
                    tokens_copy.sub
                },
                border: Border::default().rounded(13).width(1).color(border),
                ..button::Style::default()
            }
        })
        .focusable(13.0, tokens);
    let menu = open.then(|| menu(media, status, on_pick, catch_up, tokens));
    Popover::new(anchor, menu)
        .align(Align::End)
        .on_dismiss(on_toggle)
        .into()
}

/// The radio rows and the catch-up button.
fn menu<'a, Message: Clone + 'a>(
    media: MediaType,
    status: Option<EntryStatus>,
    on_pick: impl Fn(Option<EntryStatus>) -> Message + 'a,
    catch_up: Option<(String, Message)>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let rows = CHOICES.into_iter().map(|choice| {
        let on = choice == status;
        button(
            row![
                super::menu::radio(on, tokens),
                text(status_text(media, choice)).size(14)
            ]
            .spacing(10)
            .align_y(Center)
            .height(Fill),
        )
        .width(Fill)
        .height(36)
        .padding([0, 10])
        .on_press(on_pick(choice))
        .style(row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
    });
    let mut panel = Column::with_children(rows);
    if let Some((label, message)) = catch_up {
        let tokens_copy = *tokens;
        let button = button(
            column![
                text(label).size(13.5).font(font::SEMIBOLD),
                text("For catching up when you add a show you're partway through.")
                    .size(12)
                    .color(tokens.sub),
            ]
            .spacing(2),
        )
        .width(Fill)
        .padding([8, 10])
        .on_press(message)
        .style(move |_, hovered| button::Style {
            background: matches!(hovered, button::Status::Hovered)
                .then_some(Background::Color(tokens_copy.hover)),
            text_color: tokens_copy.text,
            border: Border::default()
                .rounded(8)
                .width(1)
                .color(tokens_copy.line),
            ..button::Style::default()
        })
        .focusable(8.0, tokens);
        panel = panel.push(container(button).padding(iced::Padding::new(0.0).top(6)));
    }
    let tokens = *tokens;
    container(panel.width(272))
        .padding(6)
        .style(move |_| panel_style(&tokens, 12.0))
        .into()
}
