//! The cover viewer: a click on the cover in details opens it large in
//! the middle of the window over a dark backdrop, with the title and where
//! the cover comes from. Change cover… opens Pick a cover beside it, the
//! same choices as the entry form's: a choice changes the cover at once,
//! everywhere, and "Use the match's cover" puts the default back. Esc
//! closes Pick a cover, then the viewer; × or a click on the backdrop
//! closes it.
//!
//! The cover opens at once as the cache keeps it, in a frame of the
//! image's own shape, and a sharper copy of the same image
//! (`lookup::viewer_urls`) is fetched and drawn over it once it is here;
//! a copy that does not come leaves the cached cover, without a word.
//! Hovering the cover in details starts that fetch already. Ctrl+click on
//! a title in the library opens the viewer too, without selecting it.
//!
//! A title in a series with other parts in the library gets ‹ and ›
//! beside its cover (and the Left and Right keys), which step through the
//! parts' covers in series order, from the one it opened on: Ctrl+click
//! on a series' row opens on the cover the row shows. The covers and the
//! sharper copies of the parts on either side are fetched ahead.

use iced::widget::image::{Handle, Image};
use iced::widget::{Column, Row, button, column, container, mouse_area, opaque, row, stack, text};
use iced::{Border, Center, ContentFit, Element, Fill, Right};

use super::{CoverView, Message};
use crate::app::{App, Message as AppMessage};
use crate::artwork::Wants;
use crate::browse;
use crate::lookup::{self, Match};
use crate::model::{Entry, Series};
use crate::series;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// The cover's longer side at its largest (an album's square side at
/// most `MAX_ALBUM_SIDE`), the room kept around it and its corners.
const MAX_SIDE: f32 = 600.0;
const MAX_ALBUM_SIDE: f32 = 520.0;
const MARGIN: f32 = 240.0;
const RADIUS: f32 = 12.0;
/// The round ‹ and › beside the cover, and their gap to it.
const ARROW: f32 = 44.0;
const ARROW_GAP: f32 = 28.0;
/// The shapes a cover's frame keeps within, width over height.
const NARROWEST: f32 = 0.5;
const WIDEST: f32 = 1.5;
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

/// The image the viewer shows: the picked choice's while the new cover
/// record is on its way, else the cover the library shows.
fn shown_url<'a>(app: &'a App, view: &'a CoverView, entry: &Entry) -> Option<&'a str> {
    if let Some(cover_from) = &entry.cover_from
        && let Some(url) = view
            .choices
            .iter()
            .flat_map(|r| r.matches.iter())
            .find(|m| m.cover_from() == cover_from)
            .and_then(Match::cache_cover_url)
        && app.artwork.image(url).is_some()
    {
        return Some(url);
    }
    app.artwork.cover_url(entry)
}

/// The parts of the entry's series in the library, in series order, and
/// where the entry is among them: the covers ‹ and › step through. None
/// for a title on its own.
fn in_series<'a>(app: &'a App, entry: &Entry) -> Option<(Vec<&'a Entry>, usize)> {
    let part_of = entry.series.as_ref().filter(|_| entry.can_have_series())?;
    let parts = series::entries_in(app.repo.library(), entry.media, &part_of.name);
    let at = parts.iter().position(|part| part.id == entry.id)?;
    (parts.len() > 1).then_some((parts, at))
}

/// The part `step` covers on from the entry `id` in its series; none past
/// either end.
pub fn neighbor(app: &App, id: &str, step: isize) -> Option<String> {
    let entry = app.repo.library().entry(id)?;
    let (parts, at) = in_series(app, entry)?;
    let to = at.checked_add_signed(step)?;
    parts.get(to).map(|part| part.id.clone())
}

/// The sharper copy of the cached cover `url` that is here, best first.
fn large_handle(app: &App, url: &str) -> Option<Handle> {
    lookup::viewer_urls(url)
        .iter()
        .find_map(|large| app.artwork.image(large))
}

/// Asks for the sharper copy of the cached cover `url`: the best one that
/// has not failed, so IGDB's 2× cover is tried when its 1080p is not
/// there. Once all have failed, nothing.
pub fn want_large(app: &App, url: &str, wants: &mut Wants) {
    let copies = lookup::viewer_urls(url);
    if copies
        .iter()
        .any(|large| app.artwork.image(large).is_some())
    {
        return;
    }
    if let Some(large) = copies
        .into_iter()
        .find(|large| !app.artwork.image_failed(large))
    {
        wants.large.push(large);
    }
}

pub fn view<'a>(app: &'a App, view: &'a CoverView) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let Some(entry) = app.repo.library().entry(&view.id) else {
        return iced::widget::space().into();
    };

    // The image's own shape, 2:3 or square for music until it is
    // measured, its longer side as long as the window allows.
    let url = shown_url(app, view, entry);
    let square = ui::cover::is_square(entry.media);
    let max_side = if square { MAX_ALBUM_SIDE } else { MAX_SIDE };
    let side = (app.window.height - MARGIN).clamp(240.0, max_side);
    let ratio = url
        .and_then(|url| app.artwork.image_size(url))
        .map(|(w, h)| w as f32 / h as f32)
        .unwrap_or(if square { 1.0 } else { 2.0 / 3.0 })
        .clamp(NARROWEST, WIDEST);
    let (width, height) = if ratio <= 1.0 {
        (side * ratio, side)
    } else {
        (side, side / ratio)
    };
    let face = ui::cover(
        url.and_then(|url| app.artwork.image(url)),
        theme::cover_color(&entry.id),
        width,
        height,
        RADIUS,
        Some(ui::cover::Label {
            words: entry.title.clone(),
            size: 36.0,
            padding: 28.0,
            centered: false,
        }),
    );
    // The sharper copy over it, which draws nothing until it is decoded.
    let face: Element<'_, AppMessage> = match url.and_then(|url| large_handle(app, url)) {
        Some(large) => stack![
            face,
            Image::new(large)
                .width(width)
                .height(height)
                .content_fit(ContentFit::Cover)
                .border_radius(RADIUS),
        ]
        .into(),
        None => face,
    };

    let picked = entry.cover_from.as_deref().and_then(|cover_from| {
        view.choices
            .iter()
            .flat_map(|r| r.matches.iter())
            .find(|m| m.cover_from() == cover_from)
    });
    // Where the cover comes from is only said when it is another match's.
    let from = match (&entry.cover_from, picked) {
        (None, _) => None,
        (Some(_), Some(found)) if !found.year_and_edition().is_empty() => {
            Some(format!("From another match: {}", found.year_and_edition()))
        }
        (Some(_), _) => Some("From another match".to_owned()),
    };
    let mut kind = browse::kind_label(entry).to_owned();
    if let Some(years) = entry.years(app.today) {
        kind = format!("{kind} · {years}");
    }
    if let Some(from) = from {
        kind = format!("{kind} · {from}");
    }
    let mut words = column![
        text(entry.title.clone())
            .size(16)
            .font(font::SEMIBOLD)
            .color(theme::VIEWER_TEXT),
        text(kind).size(13).color(theme::VIEWER_SUB),
    ]
    .spacing(3)
    .align_x(Center);

    // ‹ and › through the series' covers, each gone at its end with its
    // place kept, and both while Pick a cover is open on this one.
    let parts = in_series(app, entry);
    let face: Element<'_, AppMessage> = match &parts {
        Some((parts, at)) if !view.changing => {
            let arrow = |icon: Icon, step: isize| -> Element<'_, AppMessage> {
                let there = at
                    .checked_add_signed(step)
                    .is_some_and(|to| to < parts.len());
                if there {
                    round_button(icon, ARROW, 22.0, Message::StepCover(step), &tokens)
                } else {
                    iced::widget::space().width(ARROW).height(ARROW).into()
                }
            };
            row![
                arrow(Icon::ChevronLeft, -1),
                face,
                arrow(Icon::ChevronRight, 1)
            ]
            .spacing(ARROW_GAP)
            .align_y(Center)
            .into()
        }
        _ => face,
    };
    if let Some((parts, at)) = &parts
        && let Some(part_of) = &entry.series
    {
        let place = format!(
            "{} · {} of {}",
            Series::display_name(&part_of.name),
            at + 1,
            parts.len()
        );
        words = words.push(text(place).size(13).color(theme::VIEWER_SUB));
    }
    let mut cover = column![face, words].spacing(14).align_x(Center);
    if !view.changing {
        cover = cover.push(change_button(&tokens));
    }

    let mut content = Row::new().spacing(48).align_y(Center).push(opaque(cover));
    if view.changing {
        content = content.push(opaque(picker(app, view, entry, &tokens)));
    }

    let close = round_button(Icon::Close, 40.0, 18.0, Message::Close, &tokens);
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

/// A round button on the dark backdrop: ×, and ‹ › beside the cover.
fn round_button<'a>(
    icon: Icon,
    size: f32,
    icon_size: f32,
    message: Message,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    button(container(ui::icon(icon, icon_size, theme::VIEWER_TEXT)).center(Fill))
        .width(size)
        .height(size)
        .padding(0)
        .on_press(msg(message))
        .style(move |_, status| button::Style {
            background: Some(
                if matches!(status, button::Status::Hovered) {
                    theme::VIEWER_ROUND_HOVER
                } else {
                    theme::VIEWER_ROUND
                }
                .into(),
            ),
            border: Border::default().rounded(size / 2.0),
            ..button::Style::default()
        })
        .focusable(size / 2.0, tokens)
}

/// Change cover…, an opaque dark button with a lighter outline, so it
/// reads over whatever the backdrop lets through.
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
                theme::VIEWER_BUTTON_HOVER
            } else {
                theme::VIEWER_BUTTON
            }
            .into(),
        ),
        border: Border::default()
            .rounded(8)
            .width(1)
            .color(theme::VIEWER_BUTTON_LINE),
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
    let square = ui::cover::is_square(entry.media);
    let grid_height = rows * (ui::cover::height_for(tile, square) + TILE_CAPTION)
        + (rows - 1.0).max(0.0) * TILE_GAP;
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
            square,
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
    let Some(entry) = app.repo.library().entry(&view.id) else {
        return;
    };
    if let Some(url) = shown_url(app, view, entry) {
        wants.sizes.push(url.to_owned());
        want_large(app, url, wants);
    }
    if !view.changing {
        // The parts ‹ and › lead to, so a step shows its cover at once
        // and sharp.
        for step in [1, -1] {
            let next = neighbor(app, &view.id, step);
            let Some(next) = next.and_then(|id| app.repo.library().entry(&id)) else {
                continue;
            };
            wants.covers.push(next.id.clone());
            if let Some(url) = app.artwork.cover_url(next) {
                wants.sizes.push(url.to_owned());
                want_large(app, url, wants);
            }
        }
        return;
    }
    for choice in choices(app, view, entry) {
        if let Some(url) = choice.url {
            wants.images.push(url);
        }
    }
}
