//! Music by artist: the artist list, the Songs list (song entries, and
//! under a search the album tracks it finds), and the matching
//! behind the phone's replace dialogs (DESIGN.md section 5 of the phone's
//! design).

use crate::browse::Sort;
use crate::lookup::TrackInfo;
use crate::model::{Entry, EntryKind, MediaType, Track};

/// Pinned after Z in the artist list.
pub const VARIOUS_ARTISTS: &str = "Various Artists";
/// For music saved without an artist.
pub const UNKNOWN_ARTIST: &str = "Unknown artist";

/// An entry's artist, or "Unknown artist".
pub fn artist_of(entry: &Entry) -> &str {
    entry
        .creator
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or(UNKNOWN_ARTIST)
}

pub fn is_song(entry: &Entry) -> bool {
    entry.kind == Some(EntryKind::Song)
}

/// How an artist sorts by name: a leading "The" ignored, case ignored,
/// Various Artists after everything.
pub fn artist_key(name: &str) -> (bool, String) {
    let lower = name.trim().to_lowercase();
    let key = lower.strip_prefix("the ").unwrap_or(&lower).to_owned();
    (name == VARIOUS_ARTISTS, key)
}

/// One artist with what is saved of theirs.
#[derive(Debug, Clone, PartialEq)]
pub struct Artist<'a> {
    pub name: &'a str,
    /// Their entry that comes first in the list's sort: the one that
    /// places the artist, and whose section the artist is in.
    pub lead: &'a Entry,
    pub albums: Vec<&'a Entry>,
    pub songs: Vec<&'a Entry>,
}

impl Artist<'_> {
    /// "7 albums", "2 albums, 3 songs", "1 song".
    pub fn summary(&self) -> String {
        let count =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        match (self.albums.len(), self.songs.len()) {
            (a, 0) => count(a, "album", "albums"),
            (0, s) => count(s, "song", "songs"),
            (a, s) => format!(
                "{}, {}",
                count(a, "album", "albums"),
                count(s, "song", "songs")
            ),
        }
    }
}

/// Every artist with an album or a song among `entries`, which are in
/// `sort` order. The artists follow that sort: by name in the title order
/// (`artist_key`), and in the others each where their first entry is (the
/// one added last, the newest or oldest release, the highest rated). Each
/// artist's entries keep their order.
pub fn artists<'a>(entries: &[&'a Entry], sort: Sort) -> Vec<Artist<'a>> {
    let mut artists: Vec<Artist<'a>> = Vec::new();
    for &entry in entries.iter().filter(|e| e.media == MediaType::Music) {
        let name = artist_of(entry);
        let index = match artists
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
        {
            Some(index) => index,
            None => {
                artists.push(Artist {
                    name,
                    lead: entry,
                    albums: Vec::new(),
                    songs: Vec::new(),
                });
                artists.len() - 1
            }
        };
        let artist = &mut artists[index];
        if is_song(entry) {
            artist.songs.push(entry);
        } else {
            artist.albums.push(entry);
        }
    }
    if matches!(sort, Sort::TitleAz | Sort::ArtistAz) {
        artists.sort_by_key(|a| artist_key(a.name));
    }
    artists
}

/// A row on the Songs tab: a song entry, or a track of an album.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Song<'a> {
    Entry(&'a Entry),
    Track {
        album: &'a Entry,
        title: &'a str,
        rating: Option<u8>,
    },
}

impl<'a> Song<'a> {
    pub fn title(&self) -> &'a str {
        match self {
            Song::Entry(entry) => &entry.title,
            Song::Track { title, .. } => title,
        }
    }

    /// The song entry, or the album a track is on: what a click opens.
    pub fn entry(&self) -> &'a Entry {
        match self {
            Song::Entry(entry) | Song::Track { album: entry, .. } => entry,
        }
    }

    pub fn rating(&self) -> Option<u8> {
        match self {
            Song::Entry(entry) => entry.rating,
            Song::Track { rating, .. } => *rating,
        }
    }

    /// "Artist · Year": the year the song, or the album it is on, came
    /// out.
    pub fn subtitle(&self) -> String {
        let entry = self.entry();
        match entry.year() {
            Some(year) => format!("{} · {year}", artist_of(entry)),
            None => artist_of(entry).to_owned(),
        }
    }
}

/// The search (a needle from `browse::search_needle`) names the entry's
/// artist.
pub fn names_artist(entry: &Entry, needle: &str) -> bool {
    !needle.is_empty()
        && entry
            .creator
            .as_deref()
            .is_some_and(|artist| artist.to_lowercase().contains(needle))
}

/// Every track of `album`, rated or not, as its details list them: the
/// fetched list in its order with the stored ratings matched by title,
/// then the rated tracks that list lacks. Without a fetched list (not
/// loaded yet, or the album has no online match) the stored tracks.
fn tracks_of<'a>(album: &'a Entry, fetched: Option<&'a [TrackInfo]>) -> Vec<Song<'a>> {
    let stored = album.tracks.as_deref().unwrap_or_default();
    let row = |title: &'a str, rating| Song::Track {
        album,
        title,
        rating,
    };
    let Some(fetched) = fetched else {
        return stored.iter().map(|t| row(&t.title, t.rating)).collect();
    };
    let key = crate::lookup::text::track_title_key;
    let listed = fetched.iter().map(|track| {
        let rating = stored
            .iter()
            .find(|t| key(&t.title) == key(&track.title))
            .and_then(|t| t.rating);
        row(&track.title, rating)
    });
    let extra = stored
        .iter()
        .filter(|t| t.rating.is_some())
        .filter(|t| !fetched.iter().any(|l| key(&l.title) == key(&t.title)))
        .map(|t| row(&t.title, t.rating));
    listed.chain(extra).collect()
}

/// The Songs tab, in the order of `entries` (the current sort). Without
/// a search it lists song entries only: the songs saved on their own. A
/// search also brings in the tracks of albums, right where their album
/// would have been: every track of an artist the search names, rated or
/// not (`fetched` gives an album's track list once it is on this
/// computer; until then its stored tracks stand in), and any rated track
/// whose own title holds the search (that song, not the rest of its
/// album).
pub fn songs_matching<'a>(
    entries: &[&'a Entry],
    query: &str,
    fetched: impl Fn(&'a Entry) -> Option<&'a [TrackInfo]>,
) -> Vec<Song<'a>> {
    let needle = crate::browse::search_needle(query);
    let mut songs = Vec::new();
    for &entry in entries.iter().filter(|e| e.media == MediaType::Music) {
        if is_song(entry) {
            songs.push(Song::Entry(entry));
        } else if names_artist(entry, &needle) {
            songs.extend(tracks_of(entry, fetched(entry)));
        } else if !needle.is_empty() {
            songs.extend(
                entry
                    .tracks
                    .iter()
                    .flatten()
                    .filter(|track| track.rating.is_some())
                    .filter(|track| track.title.to_lowercase().contains(&needle))
                    .map(|track| Song::Track {
                        album: entry,
                        title: &track.title,
                        rating: track.rating,
                    }),
            );
        }
    }
    songs
}

fn norm(text: Option<&str>) -> String {
    text.unwrap_or_default().trim().to_lowercase()
}

fn same_artist(a: &Entry, b: &Entry) -> bool {
    let artist = norm(a.creator.as_deref());
    !artist.is_empty() && artist == norm(b.creator.as_deref())
}

/// Song entries from `album`: the same artist, and the song's album field
/// names it. Saving the album then asks D12.
pub fn songs_from_album<'a>(entries: &'a [Entry], album: &Entry) -> Vec<&'a Entry> {
    entries
        .iter()
        .filter(|e| {
            e.media == MediaType::Music
                && is_song(e)
                && e.id != album.id
                && same_artist(e, album)
                && norm(e.album.as_deref()) == norm(Some(&album.title))
        })
        .collect()
}

/// The album entry `song` is from, by artist and album title; none when
/// the song names no album. Saving the song then asks D13.
pub fn album_of_song<'a>(entries: &'a [Entry], song: &Entry) -> Option<&'a Entry> {
    if norm(song.album.as_deref()).is_empty() {
        return None;
    }
    entries.iter().find(|e| {
        e.media == MediaType::Music
            && !is_song(e)
            && e.id != song.id
            && same_artist(e, song)
            && norm(Some(&e.title)) == norm(song.album.as_deref())
    })
}

/// The album with the songs' ratings moved into its tracks. Unrated songs
/// leave nothing; a track already rated keeps its rating.
pub fn absorb_songs(mut album: Entry, songs: &[&Entry]) -> Entry {
    let mut tracks = album.tracks.take().unwrap_or_default();
    for song in songs {
        let Some(rating) = song.rating else { continue };
        match tracks
            .iter_mut()
            .find(|t| norm(Some(&t.title)) == norm(Some(&song.title)))
        {
            // A track of the album's own list, not rated yet.
            Some(track) => {
                track.rating.get_or_insert(rating);
            }
            None => tracks.push(Track::rated(song.title.clone(), None, rating)),
        }
    }
    album.tracks = (!tracks.is_empty()).then_some(tracks);
    album
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{Library, codec};

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    #[test]
    fn artists_sort_without_the_and_various_artists_last() {
        let library = library();
        let entries: Vec<&Entry> = library.entries.iter().collect();
        let artists = artists(&entries, Sort::TitleAz);
        let names: Vec<&str> = artists.iter().map(|a| a.name).collect();
        assert_eq!(names.last(), Some(&VARIOUS_ARTISTS));
        let beatles = names.iter().position(|n| *n == "The Beatles").unwrap();
        let creedence = names
            .iter()
            .position(|n| *n == "Creedence Clearwater Revival")
            .unwrap();
        assert!(beatles < creedence, "Beatles sorts under B");
        assert_eq!(names.first(), Some(&"2Pac"), "digits first");
    }

    #[test]
    fn artists_follow_the_sort() {
        let library = library();
        let music = |sort: Sort| {
            let mut entries: Vec<&Entry> = library
                .entries
                .iter()
                .filter(|e| e.media == MediaType::Music)
                .collect();
            sort.apply(&mut entries);
            entries
        };
        for sort in Sort::ALL {
            let entries = music(sort);
            let artists = artists(&entries, sort);
            if sort == Sort::TitleAz {
                assert_eq!(artists[0].name, "2Pac", "by name, not by title");
                continue;
            }
            // Each artist sits where their first entry in the sort is.
            let mut seen: Vec<&str> = Vec::new();
            for entry in &entries {
                let name = artist_of(entry);
                if !seen.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                    seen.push(name);
                }
            }
            let names: Vec<&str> = artists.iter().map(|a| a.name).collect();
            assert_eq!(names, seen, "{sort:?}");
            assert_eq!(artists[0].lead.id, entries[0].id, "{sort:?}");
        }
        let newest: Vec<&str> = artists(&music(Sort::NewestAdded), Sort::NewestAdded)
            .iter()
            .map(|a| a.name)
            .collect();
        let by_name: Vec<&str> = artists(&music(Sort::TitleAz), Sort::TitleAz)
            .iter()
            .map(|a| a.name)
            .collect();
        assert_ne!(newest, by_name);
    }

    #[test]
    fn an_artist_says_what_is_saved() {
        let library = library();
        let entries: Vec<&Entry> = library.entries.iter().collect();
        let artists = artists(&entries, Sort::TitleAz);
        let find = |name| artists.iter().find(|a| a.name == name).unwrap().summary();
        assert_eq!(find("Alice in Chains"), "1 album");
        assert_eq!(find("The Cramps"), "1 song");
        let two = Artist {
            name: "x",
            lead: &library.entries[0],
            albums: vec![&library.entries[0], &library.entries[1]],
            songs: vec![
                &library.entries[2],
                &library.entries[3],
                &library.entries[4],
            ],
        };
        assert_eq!(two.summary(), "2 albums, 3 songs");
    }

    #[test]
    fn without_a_search_songs_are_song_entries_only() {
        let library = library();
        let entries: Vec<&Entry> = library.entries.iter().collect();
        let songs = songs_matching(&entries, "", |_| None);
        assert!(songs.iter().all(|s| matches!(s, Song::Entry(_))));
        let human_fly = songs.iter().find(|s| s.title() == "Human Fly").unwrap();
        assert_eq!(human_fly.subtitle(), "The Cramps · 1981");
        let dirt = library.entries.iter().find(|e| e.title == "Dirt").unwrap();
        assert!(
            dirt.tracks.iter().flatten().any(|t| t.title == "Rooster"),
            "the fixture rates Rooster on Dirt"
        );
        assert!(songs.iter().all(|s| s.title() != "Rooster"));
    }

    #[test]
    fn a_search_lists_an_artists_rated_tracks_or_the_matching_track() {
        let library = library();
        let found = |query: &str| -> Vec<&Entry> {
            library
                .entries
                .iter()
                .filter(|e| crate::browse::matches_search(e, query))
                .collect()
        };
        let songs = songs_matching(&found("alice in"), "alice in", |_| None);
        let titles: Vec<&str> = songs.iter().map(|s| s.title()).collect();
        assert_eq!(
            titles,
            ["Them Bones", "Rooster", "Down in a Hole"],
            "the stored tracks until the album's list is here"
        );

        // With the album's list: every track by the artist, rated or
        // not, in the album's order, each rating on its track.
        let list = crate::lookup::fake::track_list("Alice in Chains", "Dirt").unwrap();
        let all = songs_matching(&found("alice in"), "alice in", |album| {
            (album.title == "Dirt").then_some(list.as_slice())
        });
        assert_eq!(all.len(), 13);
        assert!(all.iter().any(|s| s.rating().is_none()), "unrated tracks");
        assert_eq!(
            all.iter().filter(|s| s.rating().is_some()).count(),
            3,
            "the three rated ones keep their ratings"
        );
        assert_eq!(all[0].title(), list[0].title);

        let rooster = songs[1];
        assert_eq!(rooster.entry().title, "Dirt", "a track opens its album");
        assert_eq!(rooster.subtitle(), "Alice in Chains · 1992");

        let titles: Vec<&str> = songs_matching(&found("rooster"), "rooster", |_| None)
            .iter()
            .map(|s| s.title())
            .collect();
        assert_eq!(titles, ["Rooster"], "not the rest of the album");

        // An album found by its own title or a tag brings no tracks along.
        let titles: Vec<&str> = songs_matching(&found("dirt"), "dirt", |_| None)
            .iter()
            .filter(|s| matches!(s, Song::Track { .. }))
            .map(|s| s.title())
            .collect();
        assert!(titles.is_empty(), "{titles:?}");
    }

    #[test]
    fn replace_matching_needs_the_same_artist_and_album() {
        let library = library();
        let human_fly = library.entry("music/human-fly-the-cramps").unwrap();
        let album = Entry {
            id: "music/bad-music-for-bad-people-the-cramps".into(),
            title: "Bad Music for Bad People".into(),
            kind: Some(EntryKind::Album),
            tracks: None,
            ..human_fly.clone()
        };
        let found = songs_from_album(&library.entries, &album);
        assert_eq!(found.len(), 1);
        let absorbed = absorb_songs(album.clone(), &found);
        assert_eq!(absorbed.tracks.unwrap()[0].rating, Some(9));

        let mut entries = library.entries.clone();
        entries.push(album);
        assert!(album_of_song(&entries, human_fly).is_some());
        let last_christmas = library.entry("music/last-christmas-wham").unwrap();
        assert!(
            album_of_song(&entries, last_christmas).is_none(),
            "no album named"
        );
    }
}
