//! Music: Artists, Albums and Songs tabs, and one artist's page with
//! their albums and songs.

use iced::widget::{Column, column, container, grid, row, text};
use iced::{Element, Fill, Padding};

use super::{Listing, Message, MusicTab, View};
use crate::app::{App, Message as AppMessage};
use crate::model::{Entry, MediaType};
use crate::music::{self, Song};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Icon, music::SongLook};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// Grid cells, as for covers: at most this wide.
const CELL_MAX_WIDTH: f32 = 190.0;
const CELL_WIDTH: f32 = 173.0;

pub fn body<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    let content: Element<'a, AppMessage> = match &state.artist {
        Some(name) => artist_page(app, listing, name, view, tokens),
        None => match state.music_tab {
            MusicTab::Artists => {
                let artists = music::artists(&listing.shown);
                if artists.is_empty() {
                    return empty("No artists here", tokens);
                }
                // A long list gets letter sections and the jump rail.
                if state.is_long(artists.len(), view) {
                    let sections = artist_sections(artists);
                    if sections.len() >= 2 {
                        let labels: Vec<String> = sections.iter().map(|s| s.1.clone()).collect();
                        let children = sections.into_iter().map(|(label, _, artists)| {
                            column![
                                super::entries::section_header(&label, artists.len(), tokens),
                                artist_block(app, artists, view, tokens)
                            ]
                            .into()
                        });
                        return super::entries::sectioned_list(app, children, &labels, tokens);
                    }
                    return artist_block(app, artist_sections_flat(sections), view, tokens);
                }
                artist_block(app, artists, view, tokens)
            }
            MusicTab::Albums => {
                let albums: Vec<&Entry> = listing
                    .shown
                    .iter()
                    .copied()
                    .filter(|e| !music::is_song(e))
                    .collect();
                if albums.is_empty() {
                    return empty("No albums here", tokens);
                }
                super::entries::block_entries(app, &albums, view, tokens)
            }
            MusicTab::Songs => {
                let songs = music::songs(&listing.shown);
                if songs.is_empty() {
                    return empty("No songs here", tokens);
                }
                song_block(app, &songs, view, false, tokens)
            }
        },
    };
    ui::scroll(container(content).padding(Padding::new(24.0).top(4)))
        .height(Fill)
        .into()
}

/// A tab with nothing in it while Music has entries elsewhere.
fn empty<'a>(title: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    ui::empty_state(
        Icon::Document,
        title.to_owned(),
        "Nothing saved matches this tab and the filters.".into(),
        None,
        tokens,
    )
}

/// The letter sections of an artist list already in order: the first
/// letter with a leading "The" ignored, and Various Artists on its own at
/// the end. Each is (header, rail label, artists).
#[allow(clippy::type_complexity)]
fn artist_sections(
    artists: Vec<music::Artist<'_>>,
) -> Vec<(String, String, Vec<music::Artist<'_>>)> {
    let mut sections: Vec<(String, String, Vec<music::Artist<'_>>)> = Vec::new();
    for artist in artists {
        let (various, key) = music::artist_key(artist.name);
        let (label, short) = if various {
            (music::VARIOUS_ARTISTS.to_owned(), "VA".to_owned())
        } else {
            let initial = crate::browse::title_initial(&key).to_string();
            (initial.clone(), initial)
        };
        match sections.last_mut() {
            Some(last) if last.0 == label => last.2.push(artist),
            _ => sections.push((label, short, vec![artist])),
        }
    }
    sections
}

fn artist_sections_flat(
    sections: Vec<(String, String, Vec<music::Artist<'_>>)>,
) -> Vec<music::Artist<'_>> {
    sections
        .into_iter()
        .flat_map(|(_, _, artists)| artists)
        .collect()
}

fn artist_block<'a>(
    app: &'a App,
    artists: Vec<music::Artist<'a>>,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let rows = artists.into_iter().map(|artist| {
        let open = msg(Message::OpenArtist(artist.name.to_owned()));
        let summary = artist.summary();
        let photo = app.artwork.artist_photo(artist.name);
        match view {
            View::Text => column![
                ui::music::artist_row(artist.name, summary, tokens, open),
                ui::rule(tokens.line.scale_alpha(0.5))
            ]
            .into(),
            View::List => ui::music::artist_list_row(photo, artist.name, summary, tokens, open),
            View::Grid => ui::music::artist_card(photo, artist.name, summary, tokens, open),
        }
    });
    match view {
        View::Grid => grid(rows)
            .fluid(CELL_MAX_WIDTH)
            .spacing(20)
            .height(grid::aspect_ratio(CELL_WIDTH, CELL_WIDTH + 70.0))
            .into(),
        View::List => Column::with_children(rows).spacing(2).into(),
        View::Text => Column::with_children(rows).into(),
    }
}

/// Songs in the chosen view. On an artist's page a song shows its album
/// ("Album · Year"); elsewhere its artist ("Artist · Year").
fn song_block<'a>(
    app: &'a App,
    songs: &[Song<'a>],
    view: View,
    on_artist_page: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let selected = app.library.selected.as_deref();
    let rows = songs.iter().map(|song| {
        let entry = song.entry();
        let subtitle = if on_artist_page {
            let album = match song {
                Song::Entry(e) => e.album.as_deref(),
                Song::Track { album, .. } => Some(album.title.as_str()),
            };
            match (album, entry.year()) {
                (Some(album), Some(year)) => format!("{album} · {year}"),
                (Some(album), None) => album.to_owned(),
                (None, Some(year)) => year.to_string(),
                (None, None) => String::new(),
            }
        } else {
            song.subtitle()
        };
        let look = SongLook {
            title: song.title(),
            subtitle,
            rating: song.rating().map_or("—".into(), |r| format!("{r}/10")),
            cover: theme::cover_color(&entry.id),
            image: app.artwork.cover(entry),
            type_color: tokens.type_color(MediaType::Music),
            selected: selected == Some(entry.id.as_str()) && matches!(song, Song::Entry(_)),
        };
        // A track opens its album.
        let open = msg(Message::Select(entry.id.clone()));
        match view {
            View::Text => column![
                ui::music::song_row(look, tokens, open),
                ui::rule(tokens.line.scale_alpha(0.5))
            ]
            .into(),
            View::List => ui::music::song_list_row(look, tokens, open),
            View::Grid => ui::music::song_card(look, tokens, open),
        }
    });
    match view {
        View::Grid => grid(rows)
            .fluid(CELL_MAX_WIDTH)
            .spacing(20)
            .height(grid::aspect_ratio(CELL_WIDTH, CELL_WIDTH + 28.0))
            .into(),
        View::List => Column::with_children(rows).spacing(2).into(),
        View::Text => Column::with_children(rows).into(),
    }
}

/// One artist: their albums, then their songs.
fn artist_page<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    name: &str,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let artists = music::artists(&listing.shown);
    let Some(artist) = artists
        .into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(name))
    else {
        return empty("Nothing by this artist here", tokens);
    };
    let mut page = Column::new();
    if !artist.albums.is_empty() {
        page = page
            .push(section("Albums", artist.albums.len(), tokens))
            .push(super::entries::block_entries(
                app,
                &artist.albums,
                view,
                tokens,
            ));
    }
    if !artist.songs.is_empty() {
        let songs: Vec<Song<'a>> = artist.songs.iter().map(|&e| Song::Entry(e)).collect();
        page = page
            .push(section("Songs", songs.len(), tokens))
            .push(song_block(app, &songs, view, true, tokens));
    }
    page.into()
}

fn section<'a>(title: &str, count: usize, tokens: &Tokens) -> Element<'a, AppMessage> {
    container(
        row![
            text(title.to_uppercase())
                .size(theme::size::SECTION)
                .font(font::BOLD)
                .color(tokens.sub),
            ui::mono(count.to_string(), 12.0, tokens.sub),
        ]
        .spacing(8),
    )
    .padding(Padding::new(0.0).top(14).bottom(10))
    .into()
}
