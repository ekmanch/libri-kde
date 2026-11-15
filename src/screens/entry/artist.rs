//! An artist's photo, large, and Pick a cover for it.
//!
//! Ctrl+click on an artist in the Artists tab opens what the artist
//! shows (the photo, a cover from the library, or the initials) in the
//! cover viewer's place, with a line that says how it came to be but
//! never where from: "Picked by you", "Uploaded by you", nothing for the
//! suggested photo, and without a file that a cover from the library is
//! shown. Change cover… opens Pick a cover beside it, the titles' dialog
//! with one list and no headers: Upload, the owner's own picture, the
//! matched Apple artist's photo marked "Suggested", the photos of the
//! other artists of the name once they are asked for, Deezer's as
//! "Another photo", and the artist's saved album covers.
//!
//! A pick saves that picture as the artist's file, `covers/artists/
//! <slug>.jpg` (`App::set_artist_photo`): the file is the choice, and
//! nothing is stored on entries. Apple's photo is stored as it was
//! served; any other picture is scaled and saved as JPEG like an upload.
//! Upload, Ctrl+V and Adjust are the titles' (`pick`), on the artist's
//! file. Use automatic deletes the file, so the automatic order applies
//! again and the fetch looks for the suggested photo anew.
//!
//! Apple is asked only when Change cover… opens (the match, kept in the
//! cache), and the other artists' pages only on "Not the right artist?
//! Show others named …", at most ten, at the fetch's pace
//! (`lookup::apple_artists`). A mock run asks nobody and shows stand-ins.

use std::path::PathBuf;

use iced::widget::{Column, Row, column, container, mouse_area, opaque, row, stack, text};
use iced::{Center, Element, Fill, Right, Task};

use super::{Dialog, Message as EntryMessage, cover, pick};
use crate::app::{App, Message as AppMessage};
use crate::artwork::{self, Wants};
use crate::covers::{self, PhotoKind, artists, backfill, picture};
use crate::lookup::apple_artists::{Choices, Other};
use crate::music;
use crate::screens::library::music::{ArtistShows, artist_shows, want_artist};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(EntryMessage::Artist(message))
}

/// The photo's side in the viewer, at most.
const MAX_SIDE: f32 = 520.0;
const MARGIN: f32 = 240.0;
const RADIUS: f32 = 12.0;
/// The other artists whose pages are read, at most.
const OTHERS: usize = 10;

/// The viewer on an artist, and Pick a cover's state.
#[derive(Debug, Default)]
pub struct ArtistView {
    pub name: String,
    /// Pick a cover is open beside the photo.
    pub changing: bool,
    /// Upload, Fit your cover: the titles' picker state.
    pub pick: pick::PickState,
    /// What Apple has for the artist, once asked.
    pub choices: Option<Choices>,
    pub loading: bool,
    /// The lookup did not answer.
    pub failed: bool,
    /// Deezer's photo of the artist.
    pub deezer: Option<String>,
    /// The other artists of the name whose page has a photo, as they
    /// come: each with its photo.
    pub others: Vec<(Other, String)>,
    pub others_asked: bool,
    pub others_loading: bool,
    /// A picked picture is being read and made a JPEG.
    pub saving: bool,
    /// A push dropped the picture picked here before, for another
    /// device's.
    pub dropped: bool,
}

/// A picture offered in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The matched Apple artist's photo.
    Suggested(String),
    /// Another Apple artist's of the name.
    Other(String),
    Deezer(String),
    /// The cover of this entry of the library.
    Cover(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Ctrl+click on an artist: the photo, large.
    View(String),
    Change,
    Done,
    Choices {
        name: String,
        result: Result<Choices, String>,
    },
    Deezer {
        name: String,
        url: Option<String>,
    },
    /// "Show others named …".
    ShowOthers,
    /// One other artist's page was read and has a photo.
    OtherFound {
        name: String,
        other: Other,
        url: String,
    },
    /// The pages are read; `refused` says Apple stopped answering.
    OthersDone {
        name: String,
        refused: bool,
    },
    Pick(Source),
    /// The picked picture as the JPEG to store.
    Made {
        name: String,
        kind: PhotoKind,
        result: Result<Vec<u8>, String>,
    },
    /// Use automatic.
    Auto,
}

fn view_mut<'a>(app: &'a mut App, name: &str) -> Option<&'a mut ArtistView> {
    match app.dialog.as_mut()? {
        Dialog::Artist(view) if view.name == name => Some(view),
        _ => None,
    }
}

fn open(app: &App) -> Option<&ArtistView> {
    match app.dialog.as_ref()? {
        Dialog::Artist(view) => Some(view),
        _ => None,
    }
}

/// The stand-ins of a mock run: a suggested photo and two other
/// artists of the name for everyone, so every state can be looked at.
fn fake_choices(name: &str) -> Choices {
    let slug = artists::slug(name).unwrap_or_default();
    let others = ["Lieder", "Live Sessions"]
        .into_iter()
        .enumerate()
        .map(|(i, album)| Other {
            id: format!("{slug}-{i}"),
            page: String::new(),
            album: Some(album.to_owned()),
        })
        .collect();
    Choices {
        suggested: Some(format!("fake:artist:{slug}")),
        others,
    }
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::View(name) => {
            let dropped = artists::file_path(&name).is_some_and(|path| {
                let was = app.photos_dropped.len();
                app.photos_dropped.retain(|dropped| *dropped != path);
                app.photos_dropped.len() != was
            });
            app.dialog = Some(Dialog::Artist(Box::new(ArtistView {
                name,
                dropped,
                ..ArtistView::default()
            })));
        }
        Message::Change => return change(app),
        Message::Done => {
            if let Some(Dialog::Artist(view)) = &mut app.dialog {
                view.changing = false;
                view.pick = pick::PickState::default();
            }
        }
        Message::Choices { name, result } => {
            if let Some(view) = view_mut(app, &name) {
                view.loading = false;
                match result {
                    Ok(choices) => view.choices = Some(choices),
                    Err(error) => {
                        eprintln!("libri: no photos of {name} from Apple: {error}");
                        view.failed = true;
                    }
                }
            }
        }
        Message::Deezer { name, url } => {
            if let Some(view) = view_mut(app, &name) {
                view.deezer = url;
            }
        }
        Message::ShowOthers => return show_others(app),
        Message::OtherFound { name, other, url } => {
            if let Some(view) = view_mut(app, &name)
                && !view.others.iter().any(|(known, _)| known.id == other.id)
            {
                view.others.push((other, url));
            }
        }
        Message::OthersDone { name, refused } => {
            if let Some(view) = view_mut(app, &name) {
                view.others_loading = false;
            }
            if refused {
                // Apple refusing stops the photo work for the session.
                app.photos.stopped = true;
            }
        }
        Message::Pick(source) => return pick_source(app, source),
        Message::Made { name, kind, result } => {
            if let Some(view) = view_mut(app, &name) {
                view.saving = false;
                view.dropped = false;
                match result {
                    Ok(bytes) => {
                        let summary = Some(format!("Set the photo of {name}"));
                        app.set_artist_photo(&name, &bytes, kind, summary);
                    }
                    Err(error) => view.pick.error = Some(error),
                }
            }
        }
        Message::Auto => {
            let Some(name) = open(app).map(|view| view.name.clone()) else {
                return Task::none();
            };
            app.remove_artist_photo(&name, Some(format!("Remove the photo of {name}")));
            if let Some(slug) = artists::slug(&name) {
                app.photos.ask_again(&slug);
            }
        }
    }
    Task::none()
}

/// Change cover…: Pick a cover opens, and Apple and Deezer are asked
/// what they have, once per viewer.
fn change(app: &mut App) -> Task<AppMessage> {
    let listing = app.list_covers_if_stale();
    let service = app.lookups.clone();
    let apple = app.apple.clone();
    let library = app.repo.library();
    let Some(Dialog::Artist(view)) = &app.dialog else {
        return Task::none();
    };
    let name = view.name.clone();
    let owned = backfill::work_for(library, &name).map_or_else(Vec::new, |work| work.owned);
    let skipped = artists::is_skipped(&name);
    let Some(Dialog::Artist(view)) = &mut app.dialog else {
        return Task::none();
    };
    view.changing = true;
    if view.choices.is_some() || view.loading || view.failed || skipped {
        return listing;
    }
    if service.is_instant() {
        view.choices = Some(fake_choices(&name));
        return listing;
    }
    view.loading = true;
    let (asked, deezer_for) = (name.clone(), name.clone());
    let choices = Task::perform(
        crate::app::blocking(move || Ok(apple.choices(&asked, &owned))),
        move |result| {
            msg(Message::Choices {
                name: name.clone(),
                result: result.and_then(|reply| reply.map_err(|failure| failure.to_string())),
            })
        },
    );
    let asked = deezer_for.clone();
    let deezer = Task::perform(
        crate::app::blocking(move || Ok(artwork::resolve_artist(&service, &asked))),
        move |result| {
            msg(Message::Deezer {
                name: deezer_for.clone(),
                url: result.ok().and_then(Result::ok).flatten(),
            })
        },
    );
    Task::batch([listing, choices, deezer])
}

/// "Show others named …": the other artists' pages, one after another
/// at the fetch's pace, each tile added as its photo is found.
fn show_others(app: &mut App) -> Task<AppMessage> {
    let apple = app.apple.clone();
    let instant = app.lookups.is_instant();
    let Some(Dialog::Artist(view)) = &mut app.dialog else {
        return Task::none();
    };
    if view.others_asked {
        return Task::none();
    }
    view.others_asked = true;
    let others: Vec<Other> = view
        .choices
        .iter()
        .flat_map(|choices| choices.others.iter().take(OTHERS).cloned())
        .collect();
    let name = view.name.clone();
    if instant {
        view.others = others
            .into_iter()
            .map(|other| {
                let url = format!("fake:artist:{}", other.id);
                (other, url)
            })
            .collect();
        return Task::none();
    }
    view.others_loading = true;
    crate::app::steps(move |send| {
        let mut refused = false;
        for other in others {
            match apple.photo_of(&other.id, &other.page) {
                Ok(Some(url)) => send(msg(Message::OtherFound {
                    name: name.clone(),
                    other,
                    url,
                })),
                Ok(None) => {}
                Err(failure) => {
                    refused = crate::lookup::apple_artists::refused(&failure);
                    break;
                }
            }
        }
        send(msg(Message::OthersDone {
            name: name.clone(),
            refused,
        }));
    })
}

/// Where a picture offered in the list is on this computer.
fn file_of(app: &App, source: &Source) -> Option<PathBuf> {
    match source {
        Source::Suggested(url) | Source::Other(url) | Source::Deezer(url) => {
            app.artwork.image_path(url)
        }
        Source::Cover(id) => {
            let entry = app.repo.library().entry(id)?;
            let url = app.artwork.cover_url(entry)?;
            match covers::repo_path(url) {
                Some(path) => app.covers.ready(path),
                None => app.artwork.image_path(url),
            }
        }
    }
}

/// A pick: the picture is read, made a JPEG unless it is Apple's own,
/// and saved as the artist's file. One that is not on this computer yet
/// is not picked.
fn pick_source(app: &mut App, source: Source) -> Task<AppMessage> {
    let Some(view) = open(app) else {
        return Task::none();
    };
    if view.saving {
        return Task::none();
    }
    let name = view.name.clone();
    // The suggested photo is what the fetch would have stored.
    let kind = match source {
        Source::Suggested(_) => PhotoKind::Fetched,
        _ => PhotoKind::Picked,
    };
    if app.lookups.is_instant() {
        // A mock run has no pictures: the file is listed alone.
        let summary = Some(format!("Set the photo of {name}"));
        app.set_artist_photo(&name, &[], kind, summary);
        return Task::none();
    }
    let Some(path) = file_of(app, &source) else {
        return Task::none();
    };
    if let Some(Dialog::Artist(view)) = &mut app.dialog {
        view.saving = true;
        view.pick.error = None;
    }
    let as_served = matches!(source, Source::Suggested(_) | Source::Other(_));
    Task::perform(
        crate::app::blocking(move || {
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("The picture could not be read ({e})."))?;
            // Apple's 640 × 640 JPEG is stored exactly as it was served.
            if as_served && bytes.starts_with(&[0xFF, 0xD8]) {
                return Ok(bytes);
            }
            picture::prepare(&bytes).map(|prepared| prepared.bytes)
        }),
        move |result| {
            msg(Message::Made {
                name: name.clone(),
                kind,
                result,
            })
        },
    )
}

/// The line under the artist's name in the viewer: how the picture came
/// to be, never where from.
pub fn caption(app: &App, name: &str) -> &'static str {
    match artist_shows(app, name) {
        ArtistShows::Photo(_) => {
            let kind = artists::file_path(name).and_then(|path| app.covers.kind(&path));
            match kind {
                Some(PhotoKind::Picked) => "Picked by you",
                Some(PhotoKind::Uploaded) => "Uploaded by you",
                Some(PhotoKind::Fetched) | None => "",
            }
        }
        ArtistShows::Cover(_) => "No photo yet: showing a cover from your library",
        ArtistShows::Initials => "No photo and no cover: initials",
    }
}

/// The image the viewer shows, with its key for the sharper look-ups.
fn face<'a>(app: &'a App, name: &str, side: f32) -> Element<'a, AppMessage> {
    let handle = match artist_shows(app, name) {
        ArtistShows::Photo(url) => app.artwork.image(&url),
        ArtistShows::Cover(entry) => app.artwork.cover(entry),
        ArtistShows::Initials => None,
    };
    ui::cover(
        handle,
        theme::cover_color(name),
        side,
        side,
        RADIUS,
        Some(ui::cover::Label {
            words: ui::music::initials(name),
            size: side * 0.3,
            padding: 0.0,
            centered: true,
        }),
    )
}

pub fn view<'a>(app: &'a App, view: &'a ArtistView) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let side = (app.window.height - MARGIN).clamp(240.0, MAX_SIDE);
    let mut words = column![
        text(view.name.clone())
            .size(16)
            .font(font::SEMIBOLD)
            .color(theme::VIEWER_TEXT),
    ]
    .spacing(3)
    .align_x(Center);
    let caption = caption(app, &view.name);
    if !caption.is_empty() {
        words = words.push(text(caption).size(13).color(theme::VIEWER_SUB));
    }
    if view.dropped {
        words = words.push(
            text("Another device chose a photo for this artist first, so your pick was dropped.")
                .size(13)
                .color(theme::VIEWER_SUB),
        );
    }
    let mut photo = column![face(app, &view.name, side), words]
        .spacing(14)
        .align_x(Center);
    // A name with no letters or digits never gets a file.
    if !view.changing && artists::slug(&view.name).is_some() {
        photo = photo.push(cover::change_button(
            EntryMessage::Artist(Message::Change),
            &tokens,
        ));
    }
    let mut content = Row::new().spacing(48).align_y(Center);
    let room = side + 48.0 + pick::PANEL_WIDTH + 80.0;
    if !view.changing || app.window.width >= room {
        content = content.push(opaque(photo));
    }
    if view.changing {
        content = content.push(opaque(pick::panel(
            app,
            EntryMessage::Artist(Message::Done),
            &tokens,
        )));
    }
    let close = cover::round_button(Icon::Close, 40.0, 18.0, EntryMessage::Close, &tokens);
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
    .on_press(AppMessage::Entry(EntryMessage::Close))
    .into()
}

/// The albums whose covers the artist can take: everything saved by
/// them that is not a song, best first.
fn albums<'a>(app: &'a App, name: &str) -> Vec<&'a crate::model::Entry> {
    music::library_covers(&app.repo.library().entries, name)
        .into_iter()
        .filter(|entry| !music::is_song(entry))
        .collect()
}

/// Pick a cover for an artist: one list, no headers and no source names.
pub fn picker<'a>(
    app: &'a App,
    state: &'a pick::PickState,
    tile: f32,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let Some(view) = open(app) else {
        return iced::widget::space().into();
    };
    let name = view.name.as_str();
    let file = app.covers.artist_file(name);
    let kind = file.and_then(|file| app.covers.kind(&file.path));
    let header = row![
        column![
            text("Pick a cover").size(20).font(font::EXTRABOLD),
            text(format!(
                "{name}. One photo per artist, saved in your library so every device shows \
                 the same."
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
            Some(msg(Message::Done))
        ),
    ]
    .spacing(12);

    let picture = |handle, key: &str, title: &str, sub: String, badge, on, source: Source| {
        let choice = ui::cover::Choice {
            handle,
            tint: theme::cover_color(key),
            title,
            sub,
            note: "",
            on,
            square: true,
            badge,
        };
        ui::cover::choice_tile(choice, tile, msg(Message::Pick(source)), tokens)
    };

    let mut tiles: Vec<Element<'a, AppMessage>> = vec![ui::cover::upload_tile(
        tile,
        true,
        state.busy || view.saving,
        AppMessage::Entry(EntryMessage::Covers(pick::Message::Upload)),
        tokens,
    )];
    // The owner's own picture, when the file is not the suggested photo.
    let own = file.filter(|_| kind != Some(PhotoKind::Fetched));
    if let Some(file) = own {
        let url = file.url();
        let choice = ui::cover::Choice {
            handle: app.artwork.image(&url),
            tint: theme::cover_color(name),
            title: "Your photo",
            sub: match kind {
                Some(PhotoKind::Uploaded) => "Uploaded by you".to_owned(),
                Some(PhotoKind::Picked) => "Picked by you".to_owned(),
                _ => String::new(),
            },
            note: "",
            on: true,
            square: true,
            badge: None,
        };
        // A click on the picture in use changes nothing.
        let tile_element = ui::cover::choice_tile(choice, tile, msg(Message::Done), tokens);
        let off = pick::shape_of(app, state, &file.path)
            .is_some_and(|(w, h)| picture::off_shape(w, h, true));
        tiles.push(if off {
            ui::cover::with_chip(
                tile_element,
                tile,
                true,
                "Not 1:1 · ".to_owned(),
                "Adjust",
                AppMessage::Entry(EntryMessage::Covers(pick::Message::Adjust(
                    file.path.clone(),
                ))),
                tokens,
            )
        } else {
            tile_element
        });
    }
    // The photos found, best first: the suggested one (the file itself
    // while it is in use), the other artists of the name, Deezer's.
    let others_shown = !view.others.is_empty();
    let suggested_sub = if others_shown {
        "Has albums you own".to_owned()
    } else {
        String::new()
    };
    match (
        file,
        kind,
        view.choices.as_ref().and_then(|c| c.suggested.as_ref()),
    ) {
        (Some(file), Some(PhotoKind::Fetched), _) => {
            let url = file.url();
            tiles.push(picture(
                app.artwork.image(&url),
                name,
                name,
                suggested_sub,
                Some("Suggested"),
                true,
                Source::Suggested(url),
            ));
        }
        (_, _, Some(url)) => tiles.push(picture(
            app.artwork.image(url),
            name,
            name,
            suggested_sub,
            Some("Suggested"),
            false,
            Source::Suggested(url.clone()),
        )),
        _ => {}
    }
    for (other, url) in &view.others {
        let sub = other
            .album
            .as_ref()
            .map(|album| format!("With “{album}”"))
            .unwrap_or_default();
        tiles.push(picture(
            app.artwork.image(url),
            &other.id,
            name,
            sub,
            None,
            false,
            Source::Other(url.clone()),
        ));
    }
    if let Some(url) = &view.deezer {
        tiles.push(picture(
            app.artwork.image(url),
            url,
            name,
            "Another photo".to_owned(),
            None,
            false,
            Source::Deezer(url.clone()),
        ));
    }
    // Then the artist's saved album covers.
    for album in albums(app, name) {
        let sub = match album.year() {
            Some(year) => format!("Album cover · {year}"),
            None => "Album cover".to_owned(),
        };
        tiles.push(picture(
            app.artwork.cover(album),
            &album.id,
            &album.title,
            sub,
            None,
            false,
            Source::Cover(album.id.clone()),
        ));
    }

    let mut body = Column::new().spacing(14).push(pick::grid(tiles));
    if let Some(error) = &state.error {
        body = body.push(text(error.clone()).size(12.5).color(tokens.danger));
    }
    // Under the list: the other artists of the name, on demand.
    let namesakes = view.choices.as_ref().map_or(0, |c| c.others.len());
    let line = |words: String| text(words).size(12.5).color(tokens.sub);
    let mut foot = Row::new().spacing(8).align_y(Center);
    if view.loading {
        foot = foot.push(line("Looking for photos…".to_owned()));
    } else if view.failed {
        foot = foot.push(line(
            "The photo lookup did not answer. Try again in a moment.".to_owned(),
        ));
    } else if view.others_loading {
        foot = foot.push(line(format!("Looking for others named {name}…")));
    } else if namesakes > 0 && !view.others_asked {
        foot = foot.push(line("Not the right artist?".to_owned()));
        foot = foot.push(ui::labeled_button(
            format!("Show others named {name}"),
            ButtonKind::Text,
            *tokens,
            Some(msg(Message::ShowOthers)),
        ));
    } else if view.others_asked && view.others.is_empty() {
        foot = foot.push(line(format!("No other artist named {name} has a photo.")));
    }
    foot = foot.push(iced::widget::space().width(Fill));
    // Use automatic: only over a picture of the owner's own.
    if own.is_some() {
        foot = foot.push(ui::labeled_button(
            "Use automatic",
            ButtonKind::Text,
            *tokens,
            Some(msg(Message::Auto)),
        ));
    }
    body = body.push(foot);

    let max_height = (app.window.height - 190.0).max(260.0);
    let tokens = *tokens;
    container(
        column![
            header,
            container(ui::scroll(
                container(body).padding(iced::Padding::new(0.0).right(12))
            ))
            .max_height(max_height),
        ]
        .spacing(14),
    )
    .width(pick::PANEL_WIDTH)
    .padding(pick::PANEL_PADDING)
    .style(move |_| ui::panel_style(&tokens, 14.0))
    .into()
}

/// What the open viewer draws: what the artist shows, and with Pick a
/// cover open every picture offered.
pub fn wanted(app: &App, view: &ArtistView, wants: &mut Wants) {
    want_artist(app, &view.name, wants);
    if !view.changing {
        return;
    }
    if let Some(file) = app.covers.artist_file(&view.name) {
        wants.sizes.push(file.url());
    }
    let suggested = view.choices.iter().filter_map(|c| c.suggested.clone());
    let others = view.others.iter().map(|(_, url)| url.clone());
    wants
        .images
        .extend(suggested.chain(others).chain(view.deezer.clone()));
    wants
        .covers
        .extend(albums(app, &view.name).into_iter().map(|e| e.id.clone()));
}

#[cfg(test)]
mod tests {
    use iced::keyboard;

    use super::*;
    use crate::app::KeyPress;
    use crate::screens::Screen;
    use crate::screens::library::Message as LibraryMessage;

    const NAME: &str = "Pink Floyd";
    const FILE: &str = "artists/pink-floyd.jpg";

    fn send(app: &mut App, message: Message) {
        let _ = app.update(msg(message));
    }

    fn viewer(app: &App) -> &ArtistView {
        open(app).expect("the artist's viewer is open")
    }

    /// Ctrl+click on the artist, then Change cover….
    fn open_picker(app: &mut App) {
        let _ = app.update(AppMessage::Modifiers(keyboard::Modifiers::CTRL));
        let _ = app.update(AppMessage::Library(LibraryMessage::OpenArtist(NAME.into())));
        let _ = app.update(AppMessage::Modifiers(keyboard::Modifiers::empty()));
        send(app, Message::Change);
    }

    #[test]
    fn ctrl_click_on_an_artist_opens_the_photo_and_a_plain_click_the_page() {
        let mut app = App::new(Screen::Library);
        let click = || AppMessage::Library(LibraryMessage::OpenArtist(NAME.into()));
        let _ = app.update(AppMessage::Modifiers(keyboard::Modifiers::CTRL));
        let _ = app.update(click());
        assert_eq!(viewer(&app).name, NAME);
        assert!(app.library.artist.is_none(), "the page stays shut");
        // Nothing is on this computer in a test: initials.
        assert_eq!(caption(&app, NAME), "No photo and no cover: initials");
        // Esc closes Pick a cover first, then the viewer.
        send(&mut app, Message::Change);
        assert!(viewer(&app).changing);
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert!(!viewer(&app).changing);
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert!(app.dialog.is_none());
        let _ = app.update(AppMessage::Modifiers(keyboard::Modifiers::empty()));
        let _ = app.update(click());
        assert!(app.dialog.is_none());
        assert_eq!(app.library.artist.as_deref(), Some(NAME));
    }

    #[test]
    fn a_pick_becomes_the_artists_file_and_use_automatic_removes_it() {
        let mut app = App::new(Screen::Library);
        open_picker(&mut app);
        let suggested = viewer(&app)
            .choices
            .as_ref()
            .and_then(|choices| choices.suggested.clone())
            .expect("the fake suggests a photo");
        // The suggested photo: the file, with nothing said about it.
        send(&mut app, Message::Pick(Source::Suggested(suggested)));
        assert!(app.covers.artist_file(NAME).is_some());
        assert_eq!(app.covers.kind(FILE), Some(PhotoKind::Fetched));
        assert_eq!(caption(&app, NAME), "");
        // An album cover instead: picked by the owner.
        let album = albums(&app, NAME)[0].id.clone();
        send(&mut app, Message::Pick(Source::Cover(album)));
        assert_eq!(app.covers.kind(FILE), Some(PhotoKind::Picked));
        assert_eq!(caption(&app, NAME), "Picked by you");
        // Use automatic: the file goes, and the fetch may ask again.
        send(&mut app, Message::Auto);
        assert!(app.covers.artist_file(NAME).is_none());
        assert_eq!(caption(&app, NAME), "No photo and no cover: initials");
    }

    #[test]
    fn the_other_artists_of_the_name_are_shown_only_when_asked_for() {
        let mut app = App::new(Screen::Library);
        open_picker(&mut app);
        assert_eq!(viewer(&app).choices.as_ref().unwrap().others.len(), 2);
        assert!(viewer(&app).others.is_empty());
        send(&mut app, Message::ShowOthers);
        assert_eq!(viewer(&app).others.len(), 2);
        let (_, url) = viewer(&app).others[0].clone();
        send(&mut app, Message::Pick(Source::Other(url)));
        assert_eq!(app.covers.kind(FILE), Some(PhotoKind::Picked));
        // An answer for another artist's viewer is dropped.
        send(
            &mut app,
            Message::Deezer {
                name: "Radiohead".into(),
                url: Some("https://example.com/photo.jpg".into()),
            },
        );
        assert_eq!(viewer(&app).deezer, None);
    }

    #[test]
    fn an_upload_becomes_the_artists_file_and_a_dropped_pick_is_told_once() {
        let mut app = App::new(Screen::Library);
        open_picker(&mut app);
        let subject = pick::subject(&app).expect("the picker works on the artist");
        assert_eq!(subject.artist.as_deref(), Some(NAME));
        assert_eq!(subject.slug(), "pink-floyd");
        let picture = image::RgbImage::from_pixel(300, 300, image::Rgb([9, 9, 9]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(picture)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let prepared = picture::prepare(&bytes).unwrap();
        let upload = pick::Upload {
            prepared: std::sync::Arc::new(prepared),
            original: std::sync::Arc::new(bytes),
        };
        let _ = app.update(AppMessage::Entry(EntryMessage::Covers(
            pick::Message::Prepared(Ok(Some(upload))),
        )));
        assert_eq!(app.covers.kind(FILE), Some(PhotoKind::Uploaded));
        assert_eq!(caption(&app, NAME), "Uploaded by you");
        // No entry was touched: the file is the choice.
        assert!(
            app.repo
                .library()
                .entries
                .iter()
                .all(|entry| entry.cover_from.is_none())
        );
        // A push dropped the pick: said when the viewer next opens, once.
        app.photos_dropped.push(FILE.into());
        send(&mut app, Message::View(NAME.into()));
        assert!(viewer(&app).dropped);
        assert!(app.photos_dropped.is_empty());
        send(&mut app, Message::View(NAME.into()));
        assert!(!viewer(&app).dropped);
    }
}
