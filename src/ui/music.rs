//! Music's own rows: artists (with a disc of their initials until photos
//! come in phase 4) and songs, in the text, list and grid views.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Border, Center, Color, Element, Fill};

use super::{FocusExt, mono, row_style};
use crate::theme::{self, Tokens, font};

/// "AC" for Alice in Chains, "B" for The Beatles: the first letters of
/// the first and the last word, a leading "The" left out.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split_whitespace()
        .filter(|w| !w.eq_ignore_ascii_case("the"))
        .collect();
    let first = |word: &str| {
        word.chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    };
    match words.as_slice() {
        [] => String::new(),
        [only] => first(only),
        [head, .., last] => first(head) + &first(last),
    }
}

/// A round placeholder for an artist's photo: their initials on a color
/// picked from the name.
pub fn artist_disc<'a, Message: 'a>(name: &str, size: f32) -> Element<'a, Message> {
    let tint = theme::cover_color(name);
    container(
        text(initials(name))
            .size(size * 0.34)
            .font(font::EXTRABOLD)
            .color(Color::WHITE),
    )
    .center(size)
    .style(move |_| container::background(tint).border(Border::default().rounded(size / 2.0)))
    .into()
}

/// An artist in the text view: the name, then what is saved.
pub fn artist_row<'a, Message: Clone + 'a>(
    name: &'a str,
    summary: String,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let content = row![
        text(name).size(14).font(font::SEMIBOLD).width(Fill),
        text(summary).size(13).color(tokens.sub),
    ]
    .spacing(16)
    .align_y(Center)
    .height(Fill);
    button(content)
        .width(Fill)
        .height(42)
        .padding([0, 12])
        .on_press(on_press)
        .style(row_style(tokens, false, 0.0))
        .focusable(0.0, tokens)
}

/// An artist in the list view: the disc, the name and what is saved.
pub fn artist_list_row<'a, Message: Clone + 'a>(
    name: &'a str,
    summary: String,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let content = row![
        artist_disc(name, 44.0),
        column![
            text(name).size(15).font(font::SEMIBOLD),
            text(summary).size(13).color(tokens.sub)
        ]
        .spacing(3),
    ]
    .spacing(16)
    .align_y(Center);
    button(content)
        .width(Fill)
        .padding([8, 12])
        .on_press(on_press)
        .style(row_style(tokens, false, 10.0))
        .focusable(10.0, tokens)
}

/// An artist in the grid: a large disc, the name and what is saved,
/// centered. Fills its grid cell.
pub fn artist_card<'a, Message: Clone + 'a>(
    name: &'a str,
    summary: String,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let content = column![
        artist_disc(name, 150.0),
        text(name).size(13.5).font(font::SEMIBOLD).center(),
        text(summary).size(12).color(tokens.sub).center(),
    ]
    .spacing(6)
    .align_x(Center)
    .width(Fill);
    button(content)
        .padding([8, 0])
        .on_press(on_press)
        .style(row_style(tokens, false, 12.0))
        .focusable(12.0, tokens)
}

/// What a song row shows: a song entry or a rated album track.
pub struct SongLook<'a> {
    pub title: &'a str,
    /// "Artist · Year".
    pub subtitle: String,
    /// "9/10" or "—".
    pub rating: String,
    /// The color of the song's (or its album's) placeholder cover.
    pub cover: Color,
    pub type_color: Color,
    pub selected: bool,
}

/// A song in the text view: a type dot, the title, "Artist · Year" and
/// the rating.
pub fn song_row<'a, Message: Clone + 'a>(
    look: SongLook<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let content = row![
        super::dot(look.type_color, 8.0, 3.0),
        text(look.title).size(14).font(font::SEMIBOLD).width(Fill),
        text(look.subtitle).size(13).color(tokens.sub),
        container(mono(look.rating, 13.0, tokens.text)).width(56),
    ]
    .spacing(12)
    .align_y(Center)
    .height(Fill);
    button(content)
        .width(Fill)
        .height(42)
        .padding([0, 12])
        .on_press(on_press)
        .style(row_style(tokens, look.selected, 0.0))
        .focusable(0.0, tokens)
}

/// A song in the list view: a square cover, the title over "Artist ·
/// Year", and the rating.
pub fn song_list_row<'a, Message: Clone + 'a>(
    look: SongLook<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tint = look.cover;
    let content = row![
        container(Space::new())
            .width(44)
            .height(44)
            .style(move |_| container::background(tint).border(Border::default().rounded(6))),
        column![
            text(look.title).size(15).font(font::SEMIBOLD),
            text(look.subtitle).size(13).color(tokens.sub)
        ]
        .spacing(3)
        .width(Fill),
        container(mono(look.rating, 13.0, tokens.text)).width(56),
    ]
    .spacing(16)
    .align_y(Center);
    button(content)
        .width(Fill)
        .padding([8, 12])
        .on_press(on_press)
        .style(row_style(tokens, look.selected, 10.0))
        .focusable(10.0, tokens)
}

/// A song in the grid: a square cover with the title on it, then
/// "Artist · Year". Fills its grid cell.
pub fn song_card<'a, Message: Clone + 'a>(
    look: SongLook<'a>,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let tint = look.cover;
    let cover = container(
        text(look.title)
            .size(17)
            .font(font::EXTRABOLD)
            .color(Color::WHITE),
    )
    .width(Fill)
    .height(Fill)
    .padding(14)
    .align_y(iced::alignment::Vertical::Bottom)
    .style(move |_| container::background(tint).border(Border::default().rounded(8)));
    let caption = row![
        container(
            text(look.subtitle)
                .size(12.5)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None)
        )
        .width(Fill)
        .clip(true),
        mono(look.rating, 12.0, tokens.sub),
    ]
    .spacing(8)
    .height(20);
    let text_color = tokens.text;
    button(column![cover, caption].spacing(8))
        .padding(0)
        .on_press(on_press)
        .style(move |_, _| button::Style {
            text_color,
            ..button::Style::default()
        })
        .focusable(8.0, tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_leave_out_a_leading_the() {
        assert_eq!(initials("Alice in Chains"), "AC");
        assert_eq!(initials("The Beatles"), "B");
        assert_eq!(initials("2Pac"), "2");
        assert_eq!(initials("Creedence Clearwater Revival"), "CR");
    }
}
