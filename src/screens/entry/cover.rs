//! The cover viewer: a click on the cover in details opens it large in
//! the middle of the window over a dark backdrop, with the title and where
//! the cover comes from. Change cover… opens Pick a cover beside it, the
//! same choices as the entry form's: a choice changes the cover at once,
//! everywhere, and "Use the match's cover" puts the default back. Esc
//! closes Pick a cover, then the viewer; × or a click on the backdrop
//! closes it.

use iced::widget::{Column, Row, button, column, container, mouse_area, opaque, row, stack, text};
use iced::{Border, Center, Element, Fill, Right};

use super::{CoverView, Message};
use crate::app::{App, Message as AppMessage};
use crate::artwork::Wants;
use crate::browse;
use crate::lookup::Match;
use crate::model::{Entry, MediaType};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// The cover at its largest, and the room kept around it.
const MAX_HEIGHT: f32 = 600.0;
const MARGIN: f32 = 240.0;
/// Pick a cover's panel, its three columns of tiles and the gaps.
const PANEL_WIDTH: f32 = 560.0;
const PANEL_PADDING: f32 = 22.0;
const TILE_GAP: f32 = 14.0;
/// A tile's three caption lines under its cover.
const TILE_CAPTION: f32 = 60.0;
/// The panel around the grid: padding, the header and the lines under it.
const PANEL_CHROME: f32 = 180.0;

/// One choice: the lookup id to store as `cover_from` (`None` for the
/// match's own cover), the match when there is one, and its image.
struct Choice<'a> {
    cover_from: Option<String>,
    found: Option<&'a Match>,
    url: Option<String>,
}

/// The choices Pick a cover lists: the match's own cover first (from the
/// matches, or the cover the library shows while none is picked), then
/// the other matches for the title, one per image.
fn choices<'a>(app: &App, view: &'a CoverView, entry: &Entry) -> Vec<Choice<'a>> {
    let own_source = app.artwork.source_of(entry);
    let matches: Vec<&Match> = view.choices.iter().flat_map(|r| r.matches.iter()).collect();
    let own_match = matches
        .iter()
        .copied()
        .find(|m| Some(m.source.as_str()) == own_source.as_deref());
    let own_url = match own_match {
        Some(found) => found.cache_cover_url().map(str::to_owned),
        None if entry.cover_from.is_none() => app.artwork.cover_url(entry).map(str::to_owned),
        None => None,
    };
    let mut list = vec![Choice {
        cover_from: None,
        found: own_match,
        url: own_url.clone(),
    }];
    for found in matches {
        if Some(found.source.as_str()) == own_source.as_deref() {
            continue;
        }
        let url = found.cache_cover_url().map(str::to_owned);
        if url.is_some() && url == own_url {
            continue;
        }
        list.push(Choice {
            cover_from: Some(found.cover_from().to_owned()),
            found: Some(found),
            url,
        });
    }
    list
}

/// The picked choice's image while the new cover record is on its way,
/// else the cover the library shows.
fn current_handle(
    app: &App,
    view: &CoverView,
    entry: &Entry,
) -> Option<iced::widget::image::Handle> {
    if let Some(cover_from) = &entry.cover_from
        && let Some(url) = view
            .choices
            .iter()
            .flat_map(|r| r.matches.iter())
            .find(|m| m.cover_from() == cover_from)
            .and_then(Match::cache_cover_url)
        && let Some(handle) = app.artwork.image(url)
    {
        return Some(handle);
    }
    app.artwork.cover(entry)
}

pub fn view<'a>(app: &'a App, view: &'a CoverView) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let Some(entry) = app.repo.library().entry(&view.id) else {
        return iced::widget::space().into();
    };

    // 2:3, or square for music, as tall as the window allows.
    let height = (app.window.height - MARGIN).clamp(240.0, MAX_HEIGHT);
    let width = if entry.media == MediaType::Music {
        height
    } else {
        height / 1.5
    };
    let face = ui::cover(
        current_handle(app, view, entry),
        theme::cover_color(&entry.id),
        width,
        height,
        12.0,
        Some(ui::cover::Label {
            words: entry.title.clone(),
            size: 36.0,
            padding: 28.0,
            centered: false,
        }),
    );

    let picked = entry.cover_from.as_deref().and_then(|cover_from| {
        view.choices
            .iter()
            .flat_map(|r| r.matches.iter())
            .find(|m| m.cover_from() == cover_from)
    });
    let from = match (&entry.cover_from, picked) {
        (None, _) => "From the match".to_owned(),
        (Some(_), Some(found)) if !found.year_and_edition().is_empty() => {
            format!("From another match: {}", found.year_and_edition())
        }
        (Some(_), _) => "From another match".to_owned(),
    };
    let mut kind = browse::kind_label(entry).to_owned();
    if let Some(years) = entry.years(app.today) {
        kind = format!("{kind} · {years}");
    }
    let mut cover = column![
        face,
        column![
            text(entry.title.clone())
                .size(16)
                .font(font::SEMIBOLD)
                .color(theme::VIEWER_TEXT),
            text(format!("{kind} · {from}"))
                .size(13)
                .color(theme::VIEWER_SUB),
        ]
        .spacing(3)
        .align_x(Center),
    ]
    .spacing(14)
    .align_x(Center);
    if !view.changing {
        cover = cover.push(change_button(&tokens));
    }

    let mut content = Row::new().spacing(48).align_y(Center).push(opaque(cover));
    if view.changing {
        content = content.push(opaque(picker(app, view, entry, &tokens)));
    }

    let close = button(container(ui::icon(Icon::Close, 18.0, theme::VIEWER_TEXT)).center(Fill))
        .width(40)
        .height(40)
        .padding(0)
        .on_press(msg(Message::Close))
        .style(|_, _| button::Style {
            background: Some(theme::VIEWER_CLOSE.into()),
            border: Border::default().rounded(20),
            ..button::Style::default()
        })
        .focusable(20.0, &tokens);
    let scrim = theme::viewer_scrim(tokens.scrim);
    mouse_area(
        container(stack![
            container(content).center(Fill),
            container(close)
                .width(Fill)
                .align_x(Right)
                .padding([18, 20]),
        ])
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(scrim)),
    )
    .on_press(msg(Message::Close))
    .into()
}

/// Change cover…, outlined in white on the dark backdrop.
fn change_button<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    button(
        container(
            text("Change cover…")
                .size(13.5)
                .font(font::SEMIBOLD)
                .color(theme::VIEWER_TEXT),
        )
        .center_y(Fill),
    )
    .height(36)
    .padding([0, 16])
    .on_press(msg(Message::ChangeCover))
    .style(|_, status| button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered) {
                theme::VIEWER_CLOSE
            } else {
                theme::VIEWER_FILL
            }
            .into(),
        ),
        border: Border::default()
            .rounded(8)
            .width(1)
            .color(theme::VIEWER_LINE),
        ..button::Style::default()
    })
    .focusable(8.0, tokens)
}

/// Pick a cover beside the cover: the title and what changes, Done, the
/// choices three to a row, and "Use the match's cover" while another
/// match's is in use.
fn picker<'a>(
    app: &'a App,
    view: &'a CoverView,
    entry: &'a Entry,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let header = row![
        column![
            text("Pick a cover").size(20).font(font::EXTRABOLD),
            text(format!(
                "Other matches for \u{201c}{}\u{201d}. Only the cover changes.",
                entry.title.trim()
            ))
            .size(13)
            .color(tokens.sub),
        ]
        .spacing(4)
        .width(Fill),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::DoneChangingCover))
        ),
    ]
    .spacing(12);

    let tile = (PANEL_WIDTH - 2.0 * PANEL_PADDING - 2.0 * TILE_GAP) / 3.0;
    let list = choices(app, view, entry);
    // The grid scrolls once its rows outgrow the window.
    let rows = list.len().div_ceil(3) as f32;
    let grid_height = rows * (tile * 1.5 + TILE_CAPTION) + (rows - 1.0).max(0.0) * TILE_GAP;
    let max_height = (app.window.height - 80.0).max(300.0);
    let grid_height = grid_height.min(max_height - PANEL_CHROME);
    let tiles = list.into_iter().map(|choice| {
        let on = choice.cover_from == entry.cover_from
            || (choice.cover_from.is_none() && entry.cover_from.is_none());
        let is_own = choice.cover_from.is_none();
        let title = choice
            .found
            .map_or(entry.title.as_str(), |found| found.title.as_str());
        let sub = match choice.found {
            Some(found) => found.year_and_edition(),
            None => entry.year().map(|y| y.to_string()).unwrap_or_default(),
        };
        let tint = match (choice.found, is_own) {
            (_, true) => theme::cover_color(&entry.id),
            (Some(found), false) => theme::cover_color(&found.source),
            (None, false) => theme::cover_color(&entry.id),
        };
        let tile_choice = ui::cover::Choice {
            handle: choice.url.as_deref().and_then(|url| app.artwork.image(url)),
            tint,
            title,
            sub,
            note: if is_own { "The match's cover" } else { "" },
            on,
        };
        ui::cover::choice_tile(
            tile_choice,
            tile,
            msg(Message::SetCover(choice.cover_from)),
            tokens,
        )
    });
    let grid = Row::with_children(tiles)
        .spacing(TILE_GAP)
        .wrap()
        .vertical_spacing(TILE_GAP);

    let mut body = Column::new()
        .spacing(14)
        .push(header)
        .push(ui::scroll(grid).height(grid_height).width(Fill));
    if view.loading {
        body = body.push(text("Looking for other covers…").size(13).color(tokens.sub));
    } else if view
        .choices
        .as_ref()
        .is_some_and(|c| c.partly_failed() || c.all_failed().is_some())
    {
        body = body.push(
            text("Some lookups could not be reached, so a cover may be missing.")
                .size(13)
                .color(tokens.sub),
        );
    }
    if entry.cover_from.is_some() {
        body = body.push(ui::labeled_button(
            "Use the match's cover",
            ButtonKind::Text,
            *tokens,
            Some(msg(Message::SetCover(None))),
        ));
    }
    let tokens = *tokens;
    container(body)
        .width(PANEL_WIDTH)
        .padding(PANEL_PADDING)
        .style(move |_| ui::panel_style(&tokens, 14.0))
        .into()
}

/// What the viewer shows: the entry's cover, and with Pick a cover open
/// each choice's image.
pub fn wanted(app: &App, view: &CoverView, wants: &mut Wants) {
    wants.covers.push(view.id.clone());
    if !view.changing {
        return;
    }
    let Some(entry) = app.repo.library().entry(&view.id) else {
        return;
    };
    for choice in choices(app, view, entry) {
        if let Some(url) = choice.url {
            wants.images.push(url);
        }
    }
}
