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
//!
//! Pick a cover itself is `pick`: the owner's own covers with Upload, the
//! online matches and the folder's other files. A song shows its album's
//! cover, so on a song the picker works on the album.

use iced::widget::image::{Handle, Image};
use iced::widget::{Row, button, column, container, mouse_area, opaque, row, stack, text};
use iced::{Border, Center, ContentFit, Element, Fill, Right};

use super::{CoverView, Message, pick};
use crate::app::{App, Message as AppMessage};
use crate::artwork::Wants;
use crate::browse;
use crate::covers;
use crate::lookup::{self, Match};
use crate::model::{Entry, Series};
use crate::series;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, FocusExt, Icon};

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
/// The image the viewer shows: the picked choice's while the new cover
/// record is on its way, else the cover the library shows (a song's is
/// its album's).
fn shown_url<'a>(app: &'a App, view: &'a CoverView, entry: &'a Entry) -> Option<&'a str> {
    let owner = pick::cover_owner(app, entry);
    if let Some(cover_from) = &owner.cover_from
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
    app.artwork
        .own_cover_url(owner)
        .or_else(|| app.artwork.cover_url(entry))
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
    let face = ui::flat_cover(
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

    // A song shows its album's cover.
    let owner = pick::cover_owner(app, entry);
    let picked = owner.cover_from.as_deref().and_then(|cover_from| {
        view.choices
            .iter()
            .flat_map(|r| r.matches.iter())
            .find(|m| m.cover_from() == cover_from)
    });
    // Where the cover comes from is only said when it is not the
    // match's own: one of the owner's files, or another match's.
    let from = match (owner.cover_from.as_deref(), picked) {
        (None, _) => None,
        // An anime's two own covers: its TMDB show's poster, and its
        // match's cover stored under the entry's own id.
        (Some(cover_from), _) if pick::is_own_cover(owner, cover_from) => None,
        (Some(cover_from), _) if covers::repo_path(cover_from).is_some() => Some(format!(
            "Your cover: {}",
            covers::name_of(covers::repo_path(cover_from).unwrap_or_default())
        )),
        (Some(_), Some(found)) if !found.year_and_edition().is_empty() => {
            Some(format!("From another match: {}", found.year_and_edition()))
        }
        (Some(_), _) => Some("From another match".to_owned()),
    };
    let mut kind = browse::kind_label(entry).to_owned();
    if owner.id != entry.id {
        // "Song · Massive Attack · on Mezzanine".
        if let Some(artist) = entry.creator.as_deref().filter(|c| !c.is_empty()) {
            kind = format!("{kind} · {artist}");
        }
        kind = format!("{kind} · on {}", owner.title);
    } else if let Some(years) = entry.years(app.today) {
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
    // The button is named after what it does: a series' chooses which
    // part stands for it, and a title without a cover gets one added.
    if !view.changing {
        cover = cover.push(if view.series.is_some() {
            change_button("Choose series cover…", Message::ChooseSeriesCover, &tokens)
        } else if shown_url(app, view, entry).is_none() {
            change_button("Add a cover…", Message::ChangeCover, &tokens)
        } else {
            change_button("Change cover…", Message::ChangeCover, &tokens)
        });
    }

    // Pick a cover beside the cover; alone where the window has no room
    // for both.
    let mut content = Row::new().spacing(48).align_y(Center);
    let room = width + 48.0 + pick::PANEL_WIDTH + 80.0;
    if !view.changing || app.window.width >= room {
        content = content.push(opaque(cover));
    }
    if view.changing {
        content = content.push(opaque(pick::panel(
            app,
            Message::DoneChangingCover,
            &tokens,
        )));
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
pub(super) fn round_button<'a>(
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

/// Change cover… (or what the viewer's button does there: Add a cover…,
/// Choose series cover…, Change photo…), an opaque dark button with a
/// lighter outline, so it reads over whatever the backdrop lets through.
pub(super) fn change_button<'a>(
    label: &'static str,
    message: Message,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    button(
        container(
            text(label)
                .size(13.5)
                .font(font::SEMIBOLD)
                .color(theme::VIEWER_TEXT),
        )
        .center_y(Fill),
    )
    .height(36)
    .padding([0, 16])
    .on_press(msg(message))
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
    pick::wanted(app, wants);
}
