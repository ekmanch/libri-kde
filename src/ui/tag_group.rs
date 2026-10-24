use iced::widget::{Column, Row, button, container, row, space, text};
use iced::{Center, Element, Fill, Padding};

use super::{FocusExt, Icon, icon, mono, row_style};
use crate::theme::{Tokens, font, size};

/// A collapsible group of tag chips, as in the tag picker: a chevron, the
/// category in gray caps, its count, and "n selected" on the right. `rows`
/// follow the chips (the picker's "+ Add to Directors" rows).
#[allow(clippy::too_many_arguments)]
pub fn tag_group<'a, Message: Clone + 'a>(
    name: &str,
    count: usize,
    selected: usize,
    open: bool,
    on_toggle: Message,
    chips: Vec<Element<'a, Message>>,
    rows: Vec<Element<'a, Message>>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    let picked = if selected > 0 {
        format!("{selected} selected")
    } else {
        String::new()
    };
    let header = button(
        row![
            icon(chevron, 14.0, tokens.sub),
            text(name.to_uppercase())
                .size(size::SECTION)
                .font(font::BOLD)
                .color(tokens.sub),
            mono(count.to_string(), 11.0, tokens.sub),
            space::horizontal(),
            text(picked).size(12).color(tokens.text),
        ]
        .spacing(8)
        .align_y(Center),
    )
    .width(Fill)
    .height(36)
    .padding([0, 4])
    .on_press(on_toggle)
    .style(row_style(tokens, false, 8.0))
    .focusable(8.0, tokens);

    let mut group = Column::new().push(header).spacing(4);
    if open {
        let mut body = Column::new().spacing(8);
        if !chips.is_empty() {
            body = body.push(
                Row::with_children(chips)
                    .spacing(6)
                    .wrap()
                    .vertical_spacing(6),
            );
        }
        body = body.extend(rows);
        group = group.push(container(body).padding(Padding::new(0.0).left(26).right(4).bottom(12)));
    }
    group.into()
}
