//! A series in the details pane, opened from its row in a long list or an
//! entry's "All N volumes" (decided 2026-10-31, in place of a page over
//! the list): the series cover, which picks the part that stands for it,
//! the progress, and every part, the missing ones with + Add. A part opens
//! its details with ‹ the series' name to come back.

use iced::widget::{Column, Row, button, column, container, progress_bar, row, space, stack, text};
use iced::{Background, Border, Element, Fill, Padding};

use super::{details, seasons};
use crate::app::{App, Message as AppMessage};
use crate::model::{MediaType, Series};
use crate::screens::library::{self, Message, OpenSeries};
use crate::series::{self, SeriesRow};
use crate::theme::{self, font};
use crate::ui::{self, ButtonKind, FocusExt};

fn msg(message: library::Message) -> AppMessage {
    AppMessage::Library(message)
}

/// The fixed heights above the parts, so the pane can scroll to one: the
/// top padding, the cover with its stack, the progress, the heading and
/// the gaps between them.
const PAD: f32 = 22.0;
const COVER_H: f32 = 162.0;
const GAP: f32 = 20.0;
const PROGRESS_H: f32 = 31.0;
const HEADING_H: f32 = 18.0;
const HEADING_GAP: f32 = 6.0;
/// A part's row and the space under it.
const ROW_H: f32 = 50.0;
const ROW_GAP: f32 = 2.0;

/// The series' parts as the pane lists them: the lookup's list when it has
/// one, else the library's own.
fn rows<'a>(app: &'a App, open: &OpenSeries) -> Vec<SeriesRow<'a>> {
    let library = app.repo.library();
    let parts = series::entries_in(library, open.media, &open.name);
    let fetched = parts
        .first()
        .and_then(|first| app.artwork.series_for(first))
        .and_then(|state| match &state.loaded {
            crate::artwork::Loaded::Ready(parts) => Some(parts),
            _ => None,
        });
    series::rows(
        library,
        open.media,
        &open.name,
        open.highlight.as_deref(),
        fetched,
    )
}

/// Where the pane scrolls to for `open`: the marked part about a third of
/// the way down, else the top.
pub fn offset(app: &App, open: &OpenSeries) -> f32 {
    let Some(index) = rows(app, open).iter().position(|r| r.is_this) else {
        return 0.0;
    };
    let top = PAD + COVER_H + GAP + PROGRESS_H + GAP + HEADING_H + HEADING_GAP;
    (top + index as f32 * (ROW_H + ROW_GAP) - 3.0 * (ROW_H + ROW_GAP)).max(0.0)
}

/// The pane, or nothing when no part of the series is in the library any
/// more.
pub fn view<'a>(app: &'a App, open: &OpenSeries) -> Option<Element<'a, AppMessage>> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let parts = series::entries_in(library, open.media, &open.name);
    let lead = series::cover_entry(&parts)?;
    let rows = rows(app, open);
    let name = Series::display_name(&open.name);
    let noun = Series::nouns(open.media).1;

    // The cover with two parts' worth of stack behind it; a click picks
    // which part's cover stands for the series.
    let tint = theme::cover_color(&lead.id);
    let layer = |alpha: f32, padding: Padding| {
        container(container(space()).width(Fill).height(Fill).style(move |_| {
            container::background(tint.scale_alpha(alpha)).border(Border::default().rounded(8))
        }))
        .padding(padding)
        .width(Fill)
        .height(Fill)
    };
    let cover = stack![
        layer(0.35, Padding::new(0.0).left(8).bottom(14)),
        layer(0.6, Padding::new(0.0).left(4).right(3).top(3).bottom(4)),
        container(ui::titled_cover(
            app.artwork.cover(lead),
            tint,
            &name,
            104.0,
            156.0,
            14.0
        ))
        .padding(Padding::new(0.0).top(6).right(6)),
    ]
    .width(110)
    .height(COVER_H);
    let cover: Element<'a, AppMessage> = if parts.len() >= 2 {
        button(cover)
            .padding(0)
            .on_press(msg(Message::OpenSeriesCover(open.media, open.name.clone())))
            .style(|_, _| button::Style::default())
            .focusable(8.0, &tokens)
    } else {
        cover.into()
    };

    let years: Vec<u16> = rows.iter().filter_map(|r| r.year).collect();
    let span = match (years.iter().min(), years.iter().max()) {
        (Some(a), Some(b)) if a == b => format!(" · {a}"),
        (Some(a), Some(b)) => format!(" · {a}–{b}"),
        _ => String::new(),
    };
    let owned = rows.iter().filter(|r| r.owned()).count();
    let in_library = if owned == rows.len() {
        "All in your library".to_owned()
    } else {
        format!("{owned} of {} in your library", rows.len())
    };
    let facts = column![
        ui::mono(
            format!("Series · {}", open.media.label()).to_uppercase(),
            11.0,
            tokens.type_color(open.media)
        ),
        text(name.clone()).size(24).font(font::EXTRABOLD),
        text(format!("{} {noun}{span}", rows.len()))
            .size(14)
            .color(tokens.sub),
        text(in_library).size(13).color(tokens.sub),
    ]
    .spacing(6)
    .padding(Padding::new(0.0).top(8))
    .width(Fill);
    let header = row![cover, facts].spacing(16).height(COVER_H);

    let done = parts
        .iter()
        .filter(|p| p.status == crate::model::EntryStatus::Done)
        .count();
    let progress_label = if done == parts.len() {
        "All done".to_owned()
    } else {
        format!("{done} of {} done", parts.len())
    };
    let (accent, line) = (tokens.accent, tokens.line);
    let progress = column![
        row![
            ui::section_header("Progress", &tokens),
            space::horizontal(),
            text(progress_label).size(13.5).font(font::SEMIBOLD),
        ]
        .align_y(iced::Center)
        .height(PROGRESS_H - 13.0),
        progress_bar(0.0..=parts.len() as f32, done as f32)
            .girth(5)
            .style(move |_| progress_bar::Style {
                background: Background::Color(line),
                bar: Background::Color(accent),
                border: Border::default().rounded(3),
            }),
    ]
    .spacing(8);

    let mut heading = noun.to_owned();
    heading[..1].make_ascii_uppercase();
    let list = Column::with_children(
        rows.iter()
            .map(|part| seasons::series_row(app, part, open.media, true, &tokens)),
    )
    .spacing(ROW_GAP);
    let parts_list = column![
        container(ui::section_header(&heading, &tokens)).height(HEADING_H),
        list
    ]
    .spacing(HEADING_GAP);

    let body = column![header, progress, parts_list].spacing(GAP);
    Some(details::pane(body.into(), None, &tokens))
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
        let cover = container(ui::titled_cover(
            app.artwork.cover(part),
            tint,
            &part.title,
            104.0,
            156.0,
            13.0,
        ))
        .style(move |_| {
            container::Style::default().border(
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
