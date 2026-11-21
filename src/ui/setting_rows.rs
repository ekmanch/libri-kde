//! The rows of one setting in Settings: one outlined group with a line
//! between the rows and no fill (canvas, Settings notes, "Settings rows",
//! 2026-11-21). A setting with a single row is that row in its own frame.

use iced::widget::{Column, Row, column, container, text};
use iced::{Border, Center, Element, Fill};

use crate::theme::{Tokens, font};

/// A row of the group: its name with a line under it, and a button at the
/// right. What else belongs to the row (a progress bar, another line) is
/// pushed under it, 10 px apart, and stays inside the row.
pub fn setting_row<'a, Message: 'a>(
    name: &'a str,
    detail: String,
    action: Option<Element<'a, Message>>,
    tokens: &Tokens,
) -> Column<'a, Message> {
    let mut line = Row::new().spacing(12).align_y(Center).push(
        column![
            text(name).size(14).font(font::SEMIBOLD),
            text(detail).size(12.5).line_height(1.45).color(tokens.sub),
        ]
        .spacing(1)
        .width(Fill),
    );
    if let Some(action) = action {
        line = line.push(action);
    }
    Column::new().spacing(10).push(line)
}

/// The group: a 1 px frame with 12 px corners around the rows and a 1 px
/// line between them, on the page's own background.
pub fn setting_rows<'a, Message: 'a>(
    rows: impl IntoIterator<Item = Element<'a, Message>>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let line = tokens.group_line;
    let mut group = Column::new();
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            group = group.push(
                container("")
                    .width(Fill)
                    .height(1)
                    .style(move |_| container::background(line)),
            );
        }
        group = group.push(container(row).width(Fill).padding([12, 14]));
    }
    // The frame's own pixel is kept clear of the lines, which would show
    // twice as strong where a see-through line crossed it.
    container(group)
        .width(Fill)
        .padding(1)
        .style(move |_| container::Style {
            border: Border::default().rounded(12).width(1).color(line),
            ..container::Style::default()
        })
        .into()
}
