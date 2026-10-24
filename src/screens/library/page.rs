//! A page in the content pane, in place of the list, with the inspector
//! kept: a series, all of a show's seasons, or a season's episodes. A
//! back button returns to the list.

use iced::widget::{Column, Row, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Message, Page};
use crate::app::{App, Message as AppMessage};
use crate::lookup;
use crate::model::{Entry, MediaType, Season, Series};
use crate::screens::entry::seasons;
use crate::series;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

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

    let (back_label, extra, kicker, kicker_color, title, line, body): Head<'a> = match page {
        Page::Series {
            media,
            name,
            highlight,
        } => {
            let rows = series::rows(library, *media, name, highlight.as_deref());
            let owned = rows.iter().filter(|r| r.owned()).count();
            let line = format!(
                "{} {} · {owned} in your library",
                rows.len(),
                Series::nouns(*media).1
            );
            let list = Column::with_children(
                rows.iter()
                    .map(|part| seasons::series_row(app, part, *media, &tokens)),
            )
            .spacing(2);
            // Series cover: with two or more parts in the library, and not
            // from an entry's "All N volumes".
            let cover = (owned >= 2 && highlight.is_none()).then(|| {
                ui::labeled_button(
                    "Series cover…",
                    ButtonKind::Outline,
                    tokens,
                    Some(msg(Message::OpenSeriesCover(*media, name.clone()))),
                )
            });
            (
                type_label(),
                cover,
                "Series".into(),
                tokens.type_color(*media),
                Series::display_name(name),
                text(line).size(14).color(tokens.sub).into(),
                list.into(),
            )
        }
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
                None,
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
            let episodes = lookup::episodes(entry, season);
            let (line, body) = episodes_view(app, entry, season, &episodes, &tokens);
            (
                entry.title.clone(),
                None,
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
    let mut top = Row::new().push(back).spacing(10).align_y(Center);
    if let Some(extra) = extra {
        top = top.push(space::horizontal()).push(extra);
    }
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

/// A page's parts: the back label, an extra button, the kicker and its
/// color, the title, the line under it and the body.
type Head<'a> = (
    String,
    Option<Element<'a, AppMessage>>,
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
        format!("{} – {last} · {count} {plural}", first.month_day())
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
    let credit = text("Episode details from TMDB.")
        .size(12)
        .color(tokens.sub);
    let body = column![status, table, credit].spacing(16).into();
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

/// Series cover: the covers of the parts in the library, the picked one
/// ringed; a click picks at once, Done closes.
pub fn cover_dialog<'a>(app: &'a App, media: MediaType, name: &str) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let parts = series::entries_in(library, media, name);
    let picked = parts
        .iter()
        .copied()
        .find(|p| p.series.as_ref().is_some_and(|s| s.cover == Some(true)))
        .or(parts.first().copied())
        .map(|p| p.id.clone());
    let tiles = parts.iter().enumerate().map(|(index, part)| {
        let on = picked.as_deref() == Some(part.id.as_str());
        let tint = theme::cover_color(&part.id);
        let (accent, line) = (tokens.accent, tokens.line);
        let cover = container(
            text(part.title.clone())
                .size(13)
                .font(font::EXTRABOLD)
                .color(iced::Color::WHITE),
        )
        .width(104)
        .height(156)
        .padding(10)
        .align_y(iced::alignment::Vertical::Bottom)
        .style(move |_| {
            container::background(tint).border(
                Border::default()
                    .rounded(8)
                    .width(if on { 3.0 } else { 1.0 })
                    .color(if on { accent } else { line }),
            )
        });
        let note = if !on {
            String::new()
        } else if index == 0 {
            "First in the series".into()
        } else {
            "Your pick".into()
        };
        let year = part.year().map(|y| y.to_string()).unwrap_or_default();
        let title = Series::part_title(&part.title, &Series::display_name(name));
        button(
            column![
                cover,
                container(
                    text(title)
                        .size(13)
                        .font(font::SEMIBOLD)
                        .wrapping(text::Wrapping::None)
                )
                .width(104)
                .clip(true),
                ui::mono(year, 12.0, tokens.sub),
                text(note).size(12).color(tokens.sub),
            ]
            .spacing(3)
            .width(104),
        )
        .padding(0)
        .on_press(msg(Message::PickSeriesCover(part.id.clone())))
        .style({
            let text_color = tokens.text;
            move |_, _| button::Style {
                text_color,
                ..button::Style::default()
            }
        })
        .focusable(8.0, &tokens)
    });
    let body = column![
        text(format!(
            "Shown for {} in your library.",
            Series::display_name(name)
        ))
        .size(14)
        .color(tokens.sub),
        Row::with_children(tiles).spacing(16).wrap().vertical_spacing(16),
        text("Only parts in your library can be the cover. The first in the series is used until you pick another.")
            .size(12.5)
            .color(tokens.sub),
    ]
    .spacing(16);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            tokens,
            Some(msg(Message::CloseSeriesCover))
        ),
    ];
    ui::dialog(
        "Series cover",
        body,
        footer,
        560,
        &tokens,
        msg(Message::CloseSeriesCover),
    )
}
