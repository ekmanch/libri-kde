//! Music: Artists, Albums and Songs tabs, and one artist's page with
//! their albums and songs.

use iced::widget::{Column, column, container, grid, row, text};
use iced::{Element, Fill, Padding};

use super::entries::{Cell, Sectioned};
use super::{Listing, Message, MusicTab, View, menu, rail};
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, Sort};
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
const CELL_SPACING: f32 = 20.0;
/// Under a song's cover in the grid: the gap, the title line and the
/// artist line.
const SONG_CAPTION: f32 = 46.0;
/// An artist's card: the photo and two lines under it.
const ARTIST_HEIGHT: f32 = CELL_WIDTH + 70.0;

pub fn body<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    if let Some(name) = &state.artist {
        let page = artist_page(app, listing, name, view, tokens);
        // The same scroll area as the other types' lists, so it reports
        // its size and where it is scrolled: the long-list rule goes by
        // them.
        return super::entries::scrolling([page], Vec::new(), rail::LEFT);
    }
    // The tabs side by side, the list sliding between them as the Coming
    // soon page's does between its slices: the picked tab is the live
    // list, the others pass as still copies where they were left.
    let current = tab_list(app, listing, state.music_tab, view, None, tokens);
    super::soon::sliding(
        app,
        state.tab_slide.as_ref(),
        state.music_tab.index(),
        MusicTab::ALL.len(),
        current,
        |index| {
            let tab = MusicTab::ALL[index];
            let left = state.tab_scroll.get(&tab).copied().unwrap_or(0.0);
            tab_list(app, listing, tab, view, Some(left), tokens)
        },
    )
}

/// One tab's list: the scroll area with its rail, or with `still` the
/// copy that slides past, scrolled to that offset and without the rail.
fn tab_list<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    tab: MusicTab,
    view: View,
    still: Option<f32>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    // Each tab in its own sort, the still copies beside the picked one too.
    let sort = state.music_sorts.get(tab);
    let shown = state.shown_in_tab(listing, tab);
    let fits = state.rail_fits();
    let cell = |height: f32| Cell {
        max_width: CELL_MAX_WIDTH,
        spacing: CELL_SPACING,
        ratio: CELL_WIDTH / height,
    };
    // While the tabs slide, a grid is only its rows in view: the live
    // list where it is scrolled, a still copy where it was left.
    let window = (state.tab_slide.is_some() && view == View::Grid).then(|| {
        let viewport = if state.scroll.viewport > 0.0 {
            state.scroll.viewport
        } else {
            app.window.height
        };
        (still.unwrap_or(state.scroll.offset), viewport)
    });
    // Every tab is in the sections of its sort, with the section rail
    // once the list is long.
    let (children, on_rail, room) = match tab {
        MusicTab::Artists => {
            let artists = music::artists(&shown, sort);
            if artists.is_empty() {
                return empty("No artists here", tokens);
            }
            Sectioned {
                entries: artists.len(),
                sections: artist_sections(&artists, sort, fits),
                alphabet: sort == Sort::TitleAz,
                cell: cell(ARTIST_HEIGHT),
                window,
            }
            .view(app, view, tokens, <[_]>::len, |artists| {
                artist_block(app, artists.to_vec(), view, tokens)
            })
        }
        MusicTab::Albums => {
            let albums: Vec<&Entry> = shown
                .iter()
                .copied()
                .filter(|e| !music::is_song(e))
                .collect();
            if albums.is_empty() {
                return empty("No albums here", tokens);
            }
            Sectioned {
                entries: albums.len(),
                sections: browse::entry_sections(sort, &albums, fits),
                alphabet: by_letter(sort),
                cell: super::entries::card_cell(MediaType::Music),
                window,
            }
            .view(app, view, tokens, <[_]>::len, |albums| {
                super::entries::block_entries(app, albums, view, tokens)
            })
        }
        MusicTab::Songs => {
            let songs = state.songs(&shown, &app.artwork);
            if songs.is_empty() {
                return empty("No songs here", tokens);
            }
            // A track of an album sits where its album sorts, so in its
            // album's section.
            let sections = browse::sections(&songs, sort.steps(), fits, |song, step| {
                browse::section_label(sort, song.entry(), step)
            });
            Sectioned {
                entries: songs.len(),
                sections,
                alphabet: by_letter(sort),
                cell: cell(CELL_WIDTH + SONG_CAPTION),
                window,
            }
            .view(app, view, tokens, <[_]>::len, |songs| {
                song_block(app, songs, view, false, tokens)
            })
        }
    };
    match still {
        // Laid out as the scroll area lays out the live list, so it
        // starts to slide from where it was.
        Some(offset) => {
            let padding = Padding::new(rail::LEFT)
                .top(super::entries::TOP)
                .right(room);
            let column = container(Column::with_children(children))
                .padding(padding)
                .width(Fill);
            ui::scrolled(column, offset).into()
        }
        None => super::entries::scrolling(children, on_rail.into_iter().collect(), room),
    }
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

/// Whether the rail lists the alphabet under `sort`: by title, by artist.
fn by_letter(sort: Sort) -> bool {
    matches!(sort, Sort::TitleAz | Sort::ArtistAz)
}

/// What stands for an artist (DESIGN.md "Artist photos").
#[derive(Debug, Clone, PartialEq)]
pub enum ArtistShows<'a> {
    /// The artist's file in the library repo, by its image key.
    Photo(String),
    /// A cover from the library: this entry's.
    Cover(&'a Entry),
    Initials,
}

/// What the artist called `name` shows: their file in the repo, whoever
/// put it there (the initials until it is on this computer, rather than
/// a cover that the photo then replaces); else the first of their
/// entries, in `music::library_covers` order, whose cover is here; else
/// their initials.
pub fn artist_shows<'a>(app: &'a App, name: &str) -> ArtistShows<'a> {
    if let Some(file) = app.covers.artist_file(name) {
        return ArtistShows::Photo(file.url());
    }
    music::library_covers(&app.repo.library().entries, name)
        .into_iter()
        .find(|entry| app.artwork.cover(entry).is_some())
        .map_or(ArtistShows::Initials, ArtistShows::Cover)
}

/// The image drawn for the artist, when there is one on this computer.
pub fn artist_image(app: &App, name: &str) -> Option<iced::widget::image::Handle> {
    match artist_shows(app, name) {
        ArtistShows::Photo(url) => app.artwork.image(&url),
        ArtistShows::Cover(entry) => app.artwork.cover(entry),
        ArtistShows::Initials => None,
    }
}

/// Asks for what the artist shows: their file, else the covers of their
/// entries, best first, up to the first one that is here.
pub fn want_artist(app: &App, name: &str, wants: &mut crate::artwork::Wants) {
    if let Some(file) = app.covers.artist_file(name) {
        wants.images.push(file.url());
        return;
    }
    for entry in music::library_covers(&app.repo.library().entries, name) {
        wants.covers.push(entry.id.clone());
        if app.artwork.cover(entry).is_some() {
            break;
        }
    }
}

/// The sections of an artist list already in `sort` order, for a rail
/// with room for `fits` labels. By name: the first letter with a leading
/// "The" ignored, and Various Artists on its own at the end. By count:
/// how many albums or songs are saved, exact and then in ranges
/// (`browse::count_label`). Newest added: the section of the entry that
/// places the artist (`Artist::lead`), the month and then the year.
fn artist_sections<'a>(
    artists: &[music::Artist<'a>],
    sort: Sort,
    fits: usize,
) -> Vec<browse::ListSection<music::Artist<'a>>> {
    let counted = |artist: &music::Artist<'_>| match sort {
        Sort::MostAlbums => artist.albums.len(),
        _ => artist.songs.len(),
    };
    let most = artists.iter().map(counted).max().unwrap_or(0);
    browse::sections(artists, sort.steps(), fits, |artist, step| match sort {
        Sort::TitleAz => browse::artist_initial(artist.name),
        Sort::MostAlbums | Sort::MostSongs => browse::count_label(counted(artist), step, most),
        _ => browse::section_label(sort, artist.lead, step),
    })
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
        let photo = artist_image(app, artist.name);
        let marked = app.library.menu_on_artist(artist.name);
        let name = artist.name;
        let row = match view {
            View::Text => column![
                ui::music::artist_row(name, summary, marked, tokens, open),
                ui::rule(tokens.line.scale_alpha(0.5))
            ]
            .into(),
            View::List => ui::music::artist_list_row(photo, name, summary, marked, tokens, open),
            View::Grid => ui::music::artist_card(photo, name, summary, marked, tokens, open),
        };
        menu::area(menu::Target::Artist(name.to_owned()), row)
    });
    match view {
        View::Grid => grid(rows)
            .fluid(CELL_MAX_WIDTH)
            .spacing(CELL_SPACING)
            .height(grid::aspect_ratio(CELL_WIDTH, ARTIST_HEIGHT))
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
            menu: app.library.menu_on_entry(&entry.id),
        };
        // A track opens its album, and has its album's menu.
        let open = msg(Message::Select(entry.id.clone()));
        let row = match view {
            View::Text => column![
                ui::music::song_row(look, tokens, open),
                ui::rule(tokens.line.scale_alpha(0.5))
            ]
            .into(),
            View::List => ui::music::song_list_row(look, tokens, open),
            View::Grid => ui::music::song_card(look, tokens, open),
        };
        menu::area(menu::Target::Entry(entry.id.clone()), row)
    });
    match view {
        View::Grid => grid(rows)
            .fluid(CELL_MAX_WIDTH)
            .spacing(20)
            .height(grid::aspect_ratio(CELL_WIDTH, CELL_WIDTH + SONG_CAPTION))
            .into(),
        View::List => Column::with_children(rows).spacing(2).into(),
        View::Text => Column::with_children(rows).into(),
    }
}

/// The songs of an artist's page, for the menu's "Show songs".
pub const SONGS_ID: &str = "library-artist-songs";

/// One artist: their albums, then their songs.
fn artist_page<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    name: &str,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let artists = music::artists(&listing.shown, app.library.current_sort());
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
        // Found by its id, for the menu's "Show songs".
        page = page.push(
            container(column![
                section("Songs", songs.len(), tokens),
                song_block(app, &songs, view, true, tokens)
            ])
            .id(SONGS_ID),
        );
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
            ui::figure(count.to_string(), 12.0, tokens.sub),
        ]
        .spacing(8),
    )
    .padding(Padding::new(0.0).top(14).bottom(10))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::codec;
    use crate::screens::Screen;

    fn labels(sections: &[browse::ListSection<music::Artist<'_>>]) -> Vec<String> {
        sections.iter().map(|s| s.label.clone()).collect()
    }

    #[test]
    fn an_artist_shows_their_file_else_a_library_cover_else_initials() {
        let mut app = App::new(crate::screens::Screen::Library);
        let (name, best) = {
            let entries = &app.repo.library().entries;
            let album = entries
                .iter()
                .find(|e| e.media == MediaType::Music && !music::is_song(e))
                .expect("the fixture has an album");
            let name = music::artist_of(album).to_owned();
            let best = music::library_covers(entries, &name)[0].id.clone();
            (name, best)
        };
        // No cover is on this computer in a test: initials, with the
        // artist's best entry asked for.
        assert_eq!(artist_shows(&app, &name), ArtistShows::Initials);
        let mut wants = crate::artwork::Wants::default();
        want_artist(&app, &name, &mut wants);
        assert_eq!(wants.covers.first(), Some(&best));
        assert!(wants.images.is_empty());
        // With a file in the repo, the file alone.
        let file = app
            .set_artist_photo(&name, b"jpeg", crate::covers::PhotoKind::Fetched, None)
            .unwrap();
        assert_eq!(artist_shows(&app, &name), ArtistShows::Photo(file.url()));
        let mut wants = crate::artwork::Wants::default();
        want_artist(&app, &name, &mut wants);
        assert_eq!(wants.images, [file.url()]);
        assert!(wants.covers.is_empty());
    }

    #[test]
    fn artists_by_count_are_in_sections_of_exact_counts_and_then_ranges() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        let entry = &library.entries[0];
        let artist = |name: &'static str, albums: usize, songs: usize| music::Artist {
            name,
            lead: entry,
            albums: vec![entry; albums],
            songs: vec![entry; songs],
        };
        // In the order by songs, as `music::artists` gives them.
        let artists = [
            artist("Daft Punk", 9, 63),
            artist("Hans Zimmer", 14, 57),
            artist("Massive Attack", 4, 27),
            artist("Pink Floyd", 15, 24),
            artist("Adele", 5, 12),
            artist("Nina Simone", 1, 3),
            artist(music::VARIOUS_ARTISTS, 2, 3),
        ];
        // Room for every count: exact, and two artists with three songs
        // share a section.
        assert_eq!(
            labels(&artist_sections(&artists, Sort::MostSongs, 30)),
            ["63", "57", "27", "24", "12", "3"]
        );
        // Room for five: the ranges, the highest one open-ended.
        let ranges = artist_sections(&artists, Sort::MostSongs, 5);
        assert_eq!(labels(&ranges), ["50+", "20–49", "10–19", "3"]);
        assert_eq!(ranges[0].items.len(), 2);
        // By name, whatever the room: letters, Various Artists alone.
        let mut by_name = artists.to_vec();
        by_name.sort_by_key(|a| music::artist_key(a.name));
        assert_eq!(
            labels(&artist_sections(&by_name, Sort::TitleAz, 2)),
            ["A", "D", "H", "M", "N", "P", "VA"]
        );
    }

    /// Music on `tab` in `sort`, long, as the window draws it.
    fn headers(tab: MusicTab, sort: Sort) -> Vec<String> {
        let mut app = App::new(Screen::Library);
        app.library.force_long = true;
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Music))));
        let _ = app.update(msg(Message::MusicTab(tab)));
        let _ = app.update(msg(Message::SetView(View::List)));
        let _ = app.update(msg(Message::Sort(sort)));
        let state = &app.library;
        let listing = state.listing(app.repo.library(), app.today);
        let shown = state.shown_in_tab(&listing, tab);
        let fits = state.rail_fits();
        let found: Vec<String> = match tab {
            MusicTab::Artists => {
                labels(&artist_sections(&music::artists(&shown, sort), sort, fits))
            }
            _ => {
                let listed: Vec<&Entry> = shown
                    .iter()
                    .copied()
                    .filter(|e| music::is_song(e) == (tab == MusicTab::Songs))
                    .collect();
                browse::entry_sections(sort, &listed, fits)
                    .into_iter()
                    .map(|s| s.label)
                    .collect()
            }
        };
        // Each is a header in the list, in the rail's words; one section
        // alone has none.
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        for label in &found {
            assert_eq!(
                ui.find(label.as_str()).is_ok(),
                found.len() >= 2,
                "{tab:?} {sort:?}: {label}"
            );
        }
        found
    }

    #[test]
    fn every_music_tab_is_in_sections_under_every_sort() {
        for sort in Sort::FOR_ARTISTS {
            let found = headers(MusicTab::Artists, sort);
            assert!(found.len() >= 2, "artists, {sort:?}: {found:?}");
        }
        for sort in Sort::WITH_ARTIST {
            let found = headers(MusicTab::Albums, sort);
            assert!(found.len() >= 2, "albums, {sort:?}: {found:?}");
        }
        // The fixture's few songs were all added in one month.
        assert_eq!(headers(MusicTab::Songs, Sort::NewestAdded), ["Oct 25"]);
        assert_eq!(headers(MusicTab::Songs, Sort::YearNewest), ["1981", "1969"]);
    }
}
