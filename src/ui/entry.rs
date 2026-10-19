//! How an entry looks in each of the library's three views: a card in the
//! grid, a row with a cover in the list, and a row in the text table.

use iced::widget::svg::{self, Svg};
use iced::widget::{Space, button, column, container, row, stack, text};
use iced::{Background, Border, Center, Color, Element, Fill, FillPortion, Length, Padding};

use super::{Icon, icon, mono, row_style};
use crate::model::EntryStatus;
use crate::theme::{self, Tokens, font};

/// What a view shows of an entry, worked out by the screen.
#[derive(Debug, Clone)]
pub struct EntryLook<'a> {
    pub title: &'a str,
    pub creator: &'a str,
    /// "1999", or empty.
    pub year: String,
    /// The full release text: "2026-12-25".
    pub released: String,
    /// "Movie", on the cover.
    pub kind: &'static str,
    pub type_color: Color,
    pub cover: Color,
    pub status: EntryStatus,
    /// "Watching", "Done".
    pub status_label: &'static str,
    /// "8/10" or "—".
    pub rating: String,
    /// "#horror  #dark".
    pub tags: String,
    pub selected: bool,
    /// Another entry is selected, so the grid dims this one.
    pub dimmed: bool,
}

/// A grid card: the 2:3 cover with the type, a status badge, the title and
/// creator on it, then the title and year under it. Fills its grid cell.
pub fn entry_card<'a, Message: Clone + 'a>(
    look: EntryLook<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let white = Color::WHITE;
    let mut top = row![
        text(look.kind.to_uppercase())
            .size(10.5)
            .font(font::MONO)
            .color(white)
    ]
    .height(24)
    .align_y(Center);
    if let Some(badge) = status_badge(look.status) {
        top = top.push(Space::new().width(Fill)).push(badge);
    }
    let on_cover = column![
        top,
        Space::new().height(Fill),
        text(look.title).size(19).font(font::EXTRABOLD).color(white),
        text(look.creator)
            .size(12)
            .color(Color { a: 0.78, ..white }),
    ]
    .spacing(4);
    let cover = container(on_cover)
        .width(Fill)
        .height(Fill)
        .padding(Padding::new(14.0).top(10).right(10))
        .style({
            let tint = look.cover;
            move |_| container::background(tint).border(Border::default().rounded(8))
        });
    // A long title is cut off before the year.
    let caption = row![
        container(
            text(look.title)
                .size(13.5)
                .font(font::SEMIBOLD)
                .wrapping(text::Wrapping::None)
        )
        .width(Fill)
        .clip(true),
        mono(look.year, 12.0, tokens.sub),
    ]
    .spacing(8)
    .padding([0, 2])
    .height(20);
    let card = column![cover, caption].spacing(8);

    let content: Element<'a, Message> = if look.dimmed {
        let veil = tokens.content.scale_alpha(0.4);
        stack![
            card,
            container(Space::new())
                .width(Fill)
                .height(Fill)
                .style(move |_| container::background(veil))
        ]
        .into()
    } else {
        card.into()
    };
    button(content)
        .padding(0)
        .on_press(on_press)
        .style({
            let text_color = tokens.text;
            move |_, _| button::Style {
                text_color,
                ..button::Style::default()
            }
        })
        .into()
}

/// The disc in a cover's corner: a check for done, a half-filled ring for
/// in progress, a slashed circle for DNF, nothing for want.
fn status_badge<'a, Message: 'a>(status: EntryStatus) -> Option<Element<'a, Message>> {
    const HALF_RING: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><circle cx="6" cy="6" r="5" fill="none" stroke="#5EE08A" stroke-width="2"/><path d="M6 1a5 5 0 0 0 0 10z" fill="#5EE08A"/></svg>"##;
    let mark: Element<'a, Message> = match status {
        EntryStatus::Done => icon(Icon::Check, 14.0, theme::BADGE_MARK),
        EntryStatus::Dnf => icon(Icon::Dnf, 14.0, theme::BADGE_DNF),
        EntryStatus::InProgress => Svg::new(svg::Handle::from_memory(HALF_RING.as_bytes()))
            .width(10)
            .height(10)
            .into(),
        _ => return None,
    };
    Some(
        container(mark)
            .center(24)
            .style(|_| container::background(theme::BADGE).border(Border::default().rounded(12)))
            .into(),
    )
}

/// A list row: a small cover, title and creator, then tags, status, rating
/// and year in fixed columns.
pub fn entry_list_row<'a, Message: Clone + 'a>(
    look: EntryLook<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tint = look.cover;
    let cover = container(Space::new())
        .width(44)
        .height(66)
        .style(move |_| container::background(tint).border(Border::default().rounded(6)));
    let content = row![
        cover,
        column![
            text(look.title)
                .size(15)
                .font(font::SEMIBOLD)
                .wrapping(text::Wrapping::None),
            text(look.creator)
                .size(13)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(3)
        .width(Fill),
        cell(
            text(look.tags)
                .size(13)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None),
            200.0
        ),
        cell(text(look.status_label).size(13).color(tokens.sub), 90.0),
        cell(mono(look.rating, 13.0, tokens.text), 56.0),
        container(mono(look.year, 12.5, tokens.sub))
            .width(44)
            .align_right(44),
    ]
    .spacing(16)
    .align_y(Center);
    button(content)
        .width(Fill)
        .padding([8, 12])
        .on_press(on_press)
        .style(row_style(tokens, look.selected, 10.0))
        .into()
}

fn cell<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    width: f32,
) -> Element<'a, Message> {
    container(content).width(width).clip(true).into()
}

/// The text view's column widths: title, creator, released, status,
/// rating, tags.
const COLUMNS: [u16; 6] = [22, 15, 8, 9, 6, 16];

/// The text view's header row, in small gray caps, with a line under it.
pub fn entry_table_header<'a, Message: 'a>(
    labels: [&str; 6],
    tokens: &Tokens,
) -> Element<'a, Message> {
    let cells = labels.into_iter().zip(COLUMNS).map(|(label, portion)| {
        text(label.to_uppercase())
            .size(11.5)
            .font(font::SEMIBOLD)
            .color(tokens.sub)
            .width(FillPortion(portion))
            .into()
    });
    let header = container(
        iced::widget::Row::with_children(cells)
            .spacing(16)
            .align_y(Center),
    )
    .height(34)
    .padding([0, 12])
    .align_y(Center);
    column![header, rule(tokens.line)].into()
}

/// A text view row, 42 px: a type dot and the title, then the columns.
pub fn entry_table_row<'a, Message: Clone + 'a>(
    look: EntryLook<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let width = |portion| Length::FillPortion(portion);
    let content = row![
        row![
            super::dot(look.type_color, 8.0, 3.0),
            text(look.title)
                .size(14)
                .font(font::SEMIBOLD)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(10)
        .align_y(Center)
        .width(width(COLUMNS[0])),
        text(look.creator)
            .size(14)
            .color(tokens.sub)
            .wrapping(text::Wrapping::None)
            .width(width(COLUMNS[1])),
        mono(look.released, 12.5, tokens.sub).width(width(COLUMNS[2])),
        text(look.status_label).size(14).width(width(COLUMNS[3])),
        mono(look.rating, 13.0, tokens.text).width(width(COLUMNS[4])),
        text(look.tags)
            .size(13)
            .color(tokens.sub)
            .wrapping(text::Wrapping::None)
            .width(width(COLUMNS[5])),
    ]
    .spacing(16)
    .align_y(Center)
    .height(Fill)
    .clip(true);
    button(content)
        .width(Fill)
        .height(42)
        .padding([0, 12])
        .on_press(on_press)
        .style(row_style(tokens, look.selected, 0.0))
        .into()
}

/// A one-pixel horizontal line.
pub fn rule<'a, Message: 'a>(color: Color) -> Element<'a, Message> {
    container(Space::new())
        .width(Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        })
        .into()
}
