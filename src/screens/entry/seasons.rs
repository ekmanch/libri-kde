//! The rows shared by the inspector and the content-pane pages: a season
//! with its status pill, a part of a series, and the "All N seasons" row.

use iced::widget::{button, column, container, row, text};
use iced::{Center, Element, Fill, Padding};

use super::Message;
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Date, Entry, EntryStatus, MediaType, Season};
use crate::screens::library::{self, Page, SeasonMenu};
use crate::series::SeriesRow;
use crate::theme::{Tokens, font};
use crate::ui::{self, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// The catch-up button for `season`: "Mark seasons 1–28 Done" when an
/// earlier season is out and not Done.
pub fn catch_up(entry: &Entry, season: &Season, today: Date) -> Option<(String, AppMessage)> {
    let earlier = entry.season_list().iter().any(|s| {
        s.season < season.season && s.is_released_on(today) && s.status != Some(EntryStatus::Done)
    });
    if !earlier {
        return None;
    }
    let done = browse::status_label(Some(entry.media), EntryStatus::Done);
    let label = if season.season == 2 {
        format!("Mark season 1 {done}")
    } else {
        format!("Mark seasons 1–{} {done}", season.season - 1)
    };
    Some((
        label,
        msg(Message::CatchUp {
            id: entry.id.clone(),
            season: season.season,
        }),
    ))
}

/// A season's status pill with its menu, for the inspector (`in_page`
/// false) or a page.
pub fn pill<'a>(
    app: &'a App,
    entry: &'a Entry,
    season: &'a Season,
    in_page: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let key = SeasonMenu {
        id: entry.id.clone(),
        season: season.season,
        in_page,
    };
    let open = app.library.season_menu.as_ref() == Some(&key);
    let toggle = AppMessage::Library(library::Message::SeasonMenu((!open).then_some(key)));
    let id = entry.id.clone();
    let number = season.season;
    ui::season_pill(
        entry.media,
        season.status,
        open,
        toggle,
        move |status| {
            msg(Message::SetSeasonStatus {
                id: id.clone(),
                season: number,
                status,
            })
        },
        catch_up(entry, season, app.today),
        tokens,
    )
}

/// One season: "Season 2" with "New" while it is new, the date, the pill
/// (shows with more than one season) and a chevron; a click opens the
/// season's episodes.
pub fn season_row<'a>(
    app: &'a App,
    entry: &'a Entry,
    season: &'a Season,
    in_page: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let today = app.today;
    let mut name = row![text(season.name()).size(14).font(font::SEMIBOLD)]
        .spacing(8)
        .align_y(Center);
    if season.is_new() && season.is_released_on(today) {
        name = name.push(small_badge("New", tokens));
    }
    let words = column![
        name,
        text(season.date_label(today)).size(12.5).color(tokens.sub),
    ]
    .spacing(2)
    .width(Fill);
    let mut content = row![words].spacing(10).align_y(Center).height(Fill);
    if entry.season_list().len() > 1 {
        content = content.push(pill(app, entry, season, in_page, tokens));
    }
    content = content.push(ui::icon(Icon::ChevronRight, 14.0, tokens.sub));
    let open = AppMessage::Library(library::Message::OpenPage(Page::Season {
        id: entry.id.clone(),
        number: season.season,
    }));
    let on_page = matches!(
        &app.library.page,
        Some(Page::Season { id, number }) if *id == entry.id && *number == season.season
    );
    button(content)
        .width(Fill)
        .height(50)
        .padding([0, 10])
        .on_press(open)
        .style(ui::row_style(tokens, on_page, 8.0))
        .focusable(8.0, tokens)
}

/// One part of a series: its number, title and year, then "This one", the
/// part's status with a chevron, or "+ Add" for a part not in the library.
pub fn series_row<'a>(
    app: &'a App,
    part: &SeriesRow<'a>,
    media: MediaType,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let number = part
        .number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "–".into());
    let title_color = if part.owned() {
        tokens.text
    } else {
        tokens.sub
    };
    let words = column![
        text(part.title.clone())
            .size(14)
            .font(font::SEMIBOLD)
            .color(title_color)
            .wrapping(text::Wrapping::None),
        text(part.subtitle()).size(12.5).color(tokens.sub),
    ]
    .spacing(2)
    .width(Fill);
    let mut content = row![
        container(ui::mono(number, 12.5, tokens.sub)).width(26),
        words
    ]
    .spacing(10)
    .align_y(Center)
    .height(Fill);
    let (on_press, selected) = match part.entry {
        Some(_) if part.is_this => {
            content = content.push(
                text("This one")
                    .size(12.5)
                    .font(font::SEMIBOLD)
                    .color(tokens.ink),
            );
            (None, true)
        }
        Some(entry) => {
            let label = browse::status_label(Some(entry.media), entry.status);
            content = content.push(status_pill(label, tokens)).push(ui::icon(
                Icon::ChevronRight,
                14.0,
                tokens.sub,
            ));
            let selected = app.library.selected.as_deref() == Some(entry.id.as_str());
            (
                Some(AppMessage::Library(library::Message::Select(
                    entry.id.clone(),
                ))),
                selected,
            )
        }
        None => {
            content = content.push(ui::add_chip(
                "+ Add",
                tokens,
                msg(Message::AddWithTitle(media, part.title.clone())),
            ));
            (None, false)
        }
    };
    button(content)
        .width(Fill)
        .height(50)
        .padding([0, 10])
        .on_press_maybe(on_press)
        .style(ui::row_style(tokens, selected, 8.0))
        .focusable(8.0, tokens)
}

/// "All 29 seasons ›", "All 43 volumes ›".
pub fn all_row<'a>(
    label: String,
    on_press: AppMessage,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    button(
        row![
            text(label).size(14).font(font::SEMIBOLD).width(Fill),
            ui::icon(Icon::ChevronRight, 14.0, tokens.sub),
        ]
        .align_y(Center)
        .height(Fill),
    )
    .width(Fill)
    .height(44)
    .padding([0, 10])
    .on_press(on_press)
    .style(ui::row_style(tokens, false, 8.0))
    .focusable(8.0, tokens)
}

/// A read-only status pill, as a part's status shows in a series list.
fn status_pill<'a>(label: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    let line = tokens.line;
    container(text(label).size(12.5).font(font::MEDIUM))
        .height(26)
        .padding(Padding::new(0.0).left(9).right(9))
        .align_y(Center)
        .style(move |_| {
            container::Style::default()
                .border(iced::Border::default().rounded(13).width(1).color(line))
        })
        .into()
}

/// "New" beside a season's name.
fn small_badge<'a>(label: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    container(
        text(label)
            .size(10.5)
            .font(font::SEMIBOLD)
            .color(tokens.ink),
    )
    .height(18)
    .padding([0, 7])
    .align_y(Center)
    .style(move |_| {
        container::background(tokens.selection).border(iced::Border::default().rounded(9))
    })
    .into()
}
