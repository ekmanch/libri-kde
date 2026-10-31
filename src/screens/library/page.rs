//! A page in the content pane, in place of the list, with the inspector
//! kept: all of a show's seasons, or a season's episodes. A back button
//! returns to the list, scrolled where it was.

use iced::widget::{Column, Row, button, column, container, row, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Message, Page};
use crate::app::{App, Message as AppMessage};
use crate::lookup;
use crate::model::{Entry, MediaType, Season};
use crate::screens::entry::seasons;
use crate::theme::{Tokens, font};
use crate::ui::{self, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

pub fn view<'a>(app: &'a App, page: &'a Page) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let state = &app.library;
    let type_label = || {
        state
            .single()
            .map_or("Library".to_owned(), |m| m.label().to_owned())
    };
    let entry_of = |id: &str| library.entry(id);

    let (back_label, kicker, kicker_color, title, line, body): Head<'a> = match page {
        Page::Seasons { id } => {
            let Some(entry) = entry_of(id) else {
                return missing(&tokens);
            };
            let list = Column::with_children(
                entry
                    .season_list()
                    .iter()
                    .map(|season| seasons::season_row(app, entry, season, true, &tokens)),
            )
            .spacing(2);
            let years = entry.years(app.today).unwrap_or_default();
            (
                type_label(),
                kind_kicker(entry),
                tokens.type_color(entry.media),
                entry.title.clone(),
                text(format!("{} · {years}", entry.season_count_label()))
                    .size(14)
                    .color(tokens.sub)
                    .into(),
                list.into(),
            )
        }
        Page::Season { id, number } => {
            let Some(entry) = entry_of(id) else {
                return missing(&tokens);
            };
            let Some(season) = entry.season_list().iter().find(|s| s.season == *number) else {
                return missing(&tokens);
            };
            use crate::artwork::Loaded;
            use crate::lookup::FailureKind;
            let state = app.artwork.episodes_for(entry, *number);
            let (line, body) = match state.map(|s| &s.loaded) {
                Some(Loaded::Ready(episodes)) => {
                    episodes_view(app, entry, season, episodes, &tokens)
                }
                other => {
                    let note = match other {
                        None | Some(Loaded::Loading) => "Loading episodes…",
                        Some(Loaded::Failed(FailureKind::Offline)) => {
                            "The episodes need a connection."
                        }
                        Some(Loaded::Failed(FailureKind::NotConfigured)) => {
                            "The episode lookup is not set up on this build."
                        }
                        Some(Loaded::Failed(FailureKind::RateLimited)) => {
                            "The episode lookup is busy. Try again in a minute."
                        }
                        Some(Loaded::Failed(FailureKind::Other)) => {
                            "The episode lookup did not answer. Try again in a moment."
                        }
                        _ => "No episodes were found for this season.",
                    };
                    (
                        text(date_of(season, app.today))
                            .size(14)
                            .color(tokens.sub)
                            .into(),
                        text(note).size(14).color(tokens.sub).into(),
                    )
                }
            };
            (
                entry.title.clone(),
                entry.title.clone(),
                tokens.type_color(entry.media),
                season.name(),
                line,
                body,
            )
        }
    };

    let back = {
        let tokens_copy = tokens;
        button(
            row![
                ui::icon(Icon::ChevronLeft, 18.0, tokens.text),
                text(back_label).size(14).font(font::SEMIBOLD),
            ]
            .spacing(4)
            .align_y(Center)
            .height(Fill),
        )
        .height(36)
        .padding(Padding::new(0.0).left(8).right(14))
        .on_press(msg(Message::ClosePage))
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered)
                .then_some(Background::Color(tokens_copy.hover)),
            text_color: tokens_copy.text,
            border: Border::default().rounded(8),
            ..button::Style::default()
        })
        .focusable(8.0, &tokens)
    };
    let top = Row::new().push(back).spacing(10).align_y(Center);
    let header = column![
        ui::mono(kicker.to_uppercase(), 11.5, kicker_color),
        text(title).size(28).font(font::EXTRABOLD),
        line,
    ]
    .spacing(6);
    let content = column![header, body].spacing(18).max_width(760);
    column![
        container(top).padding(Padding::new(0.0).top(14).left(24).right(24)),
        ui::scroll(container(content).padding(Padding::new(24.0).top(16))).height(Fill),
    ]
    .height(Fill)
    .into()
}

/// A page's parts: the back label, the kicker and its color, the title,
/// the line under it and the body.
type Head<'a> = (
    String,
    String,
    iced::Color,
    String,
    Element<'a, AppMessage>,
    Element<'a, AppMessage>,
);

/// "Show", "Anime".
fn kind_kicker(entry: &Entry) -> String {
    match entry.media {
        MediaType::Anime => "Anime".into(),
        _ => "Show".into(),
    }
}

fn missing<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    container(text("Nothing to show.").size(14).color(tokens.sub))
        .padding(24)
        .into()
}

/// The season page's line under the title and its body: the Status row
/// with the pill, the episode table, and the credit.
fn episodes_view<'a>(
    app: &'a App,
    entry: &'a Entry,
    season: &'a Season,
    episodes: &[lookup::Episode],
    tokens: &Tokens,
) -> (Element<'a, AppMessage>, Element<'a, AppMessage>) {
    let today = app.today;
    let out: Vec<&lookup::Episode> = episodes
        .iter()
        .filter(|e| e.aired.is_none_or(|d| d <= today))
        .collect();
    let count = episodes.len();
    let plural = if count == 1 { "episode" } else { "episodes" };
    let first = episodes.first().and_then(|e| e.aired);
    let last = episodes.last().and_then(|e| e.aired);
    let one_day = first.is_some() && first == last;
    let full_date = season
        .released
        .is_some_and(|r| r.month.is_some() && r.day.is_some());
    let line = if !season.is_released_on(today) {
        format!("Coming {} · {count} {plural}", date_of(season, today))
    } else if !full_date || one_day || first.is_none() {
        format!("{} · {count} {plural}", date_of(season, today))
    } else if out.len() < count {
        format!(
            "Started {} · {} of {count} {plural} out",
            date_of(season, today),
            out.len()
        )
    } else {
        let (first, last) = (first.unwrap_or(today), last.unwrap_or(today));
        let last = format!("{} {}, {}", &last.month_name()[..3], last.day, last.year);
        format!(
            "{} – {last} · {count} {plural}",
            short_day(first, first.year)
        )
    };

    // The status row: the pill for a show with several seasons; a note for
    // a one-season show, whose status is the show's.
    let status: Element<'a, AppMessage> = if entry.season_list().len() > 1 {
        row![
            ui::section_header("Status", tokens),
            seasons::pill(app, entry, season, true, tokens),
        ]
        .spacing(12)
        .align_y(Center)
        .into()
    } else {
        row![
            ui::section_header("Status", tokens),
            text("Status is the show's, set at the top of its details.")
                .size(12.5)
                .color(tokens.sub),
        ]
        .spacing(12)
        .align_y(Center)
        .into()
    };

    let header = container(
        row![
            container(small_caps("#", tokens)).width(32),
            container(small_caps("Episode", tokens)).width(Fill),
            container(small_caps("Aired", tokens)).width(110),
            container(small_caps("Runtime", tokens)).width(70),
        ]
        .spacing(12),
    )
    .padding(Padding::new(0.0).left(10).right(10).bottom(6));
    let year = first.map_or(today.year, |d| d.year);
    let rows = episodes.iter().map(|episode| {
        let out = episode.aired.is_none_or(|d| d <= today);
        let color = if out { tokens.text } else { tokens.sub };
        let title = match (&episode.title, out) {
            (_, false) => "Title TBA".to_owned(),
            (Some(title), true) => title.clone(),
            (None, true) => format!("Episode {}", episode.number),
        };
        let aired = match episode.aired {
            Some(_) if out && one_day => String::new(),
            Some(day) if out => short_day(day, year),
            Some(day) => format!("Airs {}", short_day(day, year)),
            None if out => String::new(),
            None => "Air date not set".into(),
        };
        let runtime = if out {
            episode
                .runtime
                .map(|m| format!("{m} min"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        container(
            row![
                container(ui::mono(episode.number.to_string(), 12.5, tokens.sub)).width(32),
                container(
                    text(title)
                        .size(14)
                        .color(color)
                        .wrapping(text::Wrapping::None)
                )
                .width(Fill)
                .clip(true),
                container(ui::mono(aired, 12.5, tokens.sub)).width(110),
                container(ui::mono(runtime, 12.5, tokens.sub)).width(70),
            ]
            .spacing(12)
            .align_y(Center),
        )
        .height(40)
        .padding([0, 10])
        .align_y(Center)
        .into()
    });
    let table = column![
        header,
        ui::rule(tokens.line),
        Column::with_children(rows),
        ui::rule(tokens.line.scale_alpha(0.5)),
    ];
    // No source is named here: the sources are listed in Settings › About.
    let body = column![status, table].spacing(16).into();
    (text(line).size(14).color(tokens.sub).into(), body)
}

/// A season's date for the page's line: "Oct 9, 2026" or "2026".
fn date_of(season: &Season, _today: crate::model::Date) -> String {
    season.released.map(|r| r.long()).unwrap_or_default()
}

/// "Oct 1", or "Oct 1, 2025" outside `year`.
fn short_day(day: crate::model::Date, year: u16) -> String {
    let month = &day.month_name()[..3];
    if day.year == year {
        format!("{month} {}", day.day)
    } else {
        format!("{month} {}, {}", day.day, day.year)
    }
}

fn small_caps<'a>(label: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(label.to_uppercase())
        .size(11.5)
        .font(font::SEMIBOLD)
        .color(tokens.sub)
        .into()
}
