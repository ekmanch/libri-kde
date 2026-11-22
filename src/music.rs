//! Music by artist: the artist list, the Songs list (song entries, and
//! under a search the album tracks it finds, rated or not), and the matching
//! behind the phone's replace dialogs (DESIGN.md section 5 of the phone's
//! design).

use std::cmp::Reverse;

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
/// (`artist_key`), by how many of their albums or songs are saved in the
/// two orders by count (the most first, ties by name), and under Newest
/// added each where their first entry is (the one added last). Each
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
    match sort {
        Sort::TitleAz | Sort::ArtistAz => artists.sort_by_key(|a| artist_key(a.name)),
        Sort::MostAlbums => {
            artists.sort_by_key(|a| (Reverse(a.albums.len()), artist_key(a.name)));
        }
        Sort::MostSongs => {
            artists.sort_by_key(|a| (Reverse(a.songs.len()), artist_key(a.name)));
        }
        _ => {}
    }
    artists
}

/// The entries that lend an artist their cover while the artist has no
/// photo of their own, best first (`covers::artists::cover_order`): of
/// everything saved by `name` in the library, whatever the list shows,
/// the albums most recently added first and then the songs. The artist
/// shows the first of them that has a cover.
pub fn library_covers<'a>(entries: &'a [Entry], name: &str) -> Vec<&'a Entry> {
    let by: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.media == MediaType::Music && artist_of(e).eq_ignore_ascii_case(name))
        .collect();
    let saved: Vec<crate::covers::artists::Saved> = by
        .iter()
        .map(|entry| crate::covers::artists::Saved {
            song: is_song(entry),
            added: entry.added,
        })
        .collect();
    crate::covers::artists::cover_order(&saved)
        .into_iter()
        .map(|i| by[i])
        .collect()
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

/// The words of a search (`browse::search_words`) left for a track's
/// title once the entry's artist is named, or none when the search does
/// not name the artist. Words the artist's name holds anywhere name it
/// when they are the whole search ("ylan"); beside other words only
/// those that start a word of the name do, so "in" is no part of Pink
/// Floyd in "alice in".
fn beside_artist<'w>(entry: &Entry, words: &'w [String]) -> Option<Vec<&'w str>> {
    let artist = entry.creator.as_deref()?.to_lowercase();
    if words.is_empty() {
        return None;
    }
    if words.iter().all(|word| artist.contains(word.as_str())) {
        return Some(Vec::new());
    }
    let starts = |word: &str| {
        artist
            .split(|c: char| !c.is_alphanumeric())
            .any(|part| part.starts_with(word))
    };
    let (named, rest): (Vec<&str>, Vec<&str>) =
        words.iter().map(String::as_str).partition(|w| starts(w));
    (!named.is_empty()).then_some(rest)
}

/// The Songs tab may list tracks of `entry` that the entry itself does
/// not hold: under a search any album, since the words may be in the
/// title of a track only its fetched list holds ("america" for a live
/// album's "America", "bob dylan girl").
pub fn may_list_tracks(entry: &Entry, query: &str) -> bool {
    entry.media == MediaType::Music
        && !is_song(entry)
        && !crate::browse::search_words(query).is_empty()
}

/// The tracks of `album` whose title holds `words`, rated or not, as its
/// details list them: the fetched list in its order with the stored
/// ratings matched by title, then the rated tracks that list lacks.
/// Without a fetched list (not loaded yet, or the album has no online
/// match) the stored tracks.
fn tracks_of<'a>(
    album: &'a Entry,
    fetched: Option<&'a [TrackInfo]>,
    words: &[&str],
) -> Vec<Song<'a>> {
    let stored = album.tracks.as_deref().unwrap_or_default();
    let holds = |title: &str| crate::browse::holds_all(title, words);
    let row = |title: &'a str, rating| Song::Track {
        album,
        title,
        rating,
    };
    let Some(fetched) = fetched else {
        let stored = stored.iter().filter(|t| holds(&t.title));
        return stored.map(|t| row(&t.title, t.rating)).collect();
    };
    let key = crate::lookup::text::track_title_key;
    let listed = fetched.iter().filter(|t| holds(&t.title)).map(|track| {
        let rating = stored
            .iter()
            .find(|t| key(&t.title) == key(&track.title))
            .and_then(|t| t.rating);
        row(&track.title, rating)
    });
    let extra = stored
        .iter()
        .filter(|t| t.rating.is_some() && holds(&t.title))
        .filter(|t| !fetched.iter().any(|l| key(&l.title) == key(&t.title)))
        .map(|t| row(&t.title, t.rating));
    listed.chain(extra).collect()
}

/// The Songs tab, in the order of `entries` (the current sort). Without
/// a search it lists song entries only: the songs saved on their own. A
/// search also brings in the tracks of albums, right where their album
/// would have been, rated or not (`fetched` gives an album's track list
/// once it is on this computer; until then its stored tracks stand in).
/// The search is its words: those an album's artist holds name the
/// artist, and the rest are for the track's own title. So an artist
/// alone brings every track of their albums, an artist with more words
/// those of their tracks whose title holds the rest ("bob dylan girl"),
/// and words that name no artist any track whose title holds them all
/// (that song, not the rest of its album).
pub fn songs_matching<'a>(
    entries: &[&'a Entry],
    query: &str,
    fetched: impl Fn(&'a Entry) -> Option<&'a [TrackInfo]>,
) -> Vec<Song<'a>> {
    let words = crate::browse::search_words(query);
    let mut songs = Vec::new();
    for &entry in entries.iter().filter(|e| e.media == MediaType::Music) {
        if is_song(entry) {
            songs.push(Song::Entry(entry));
            continue;
        }
        if words.is_empty() {
            continue;
        }
        let rest = beside_artist(entry, &words)
            .unwrap_or_else(|| words.iter().map(String::as_str).collect());
        songs.extend(tracks_of(entry, fetched(entry), &rest));
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
    fn an_artists_library_covers_are_their_albums_newest_added_first_then_their_songs() {
        let mut library = crate::model::codec::decode(crate::fixtures::LIBRARY_JSON).unwrap();
        let day = |text: &str| crate::model::Date::parse(text).unwrap();
        let mut by_artist: Vec<Entry> = library
            .entries
            .iter()
            .filter(|e| e.media == MediaType::Music && !is_song(e))
            .take(1)
            .cloned()
            .collect();
        let base = by_artist.remove(0);
        let made = |id: &str, song: bool, added: &str| {
            let mut entry = base.clone();
            entry.id = id.into();
            entry.creator = Some("Test Artist".into());
            entry.kind = song.then_some(EntryKind::Song);
            entry.added = day(added);
            entry
        };
        library.entries.extend([
            made("music/song", true, "2026-11-01"),
            made("music/old", false, "2025-01-01"),
            made("music/new-a", false, "2026-03-02"),
            made("music/new-b", false, "2026-03-02"),
        ]);
        let ids: Vec<&str> = library_covers(&library.entries, "test artist")
            .into_iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["music/new-b", "music/new-a", "music/old", "music/song"]
        );
        assert!(library_covers(&library.entries, "Nobody").is_empty());
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
        for sort in [Sort::NewestAdded, Sort::TitleAz] {
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
    fn artists_by_how_much_is_saved() {
        let mut library = library();
        // Two more albums by one artist and two songs by another.
        let mut extra = |creator: &str, kind: Option<EntryKind>, n: usize| {
            let mut entry = library
                .entries
                .iter()
                .find(|e| e.media == MediaType::Music)
                .unwrap()
                .clone();
            entry.id = format!("music/extra-{n}");
            entry.title = format!("Extra {n}");
            entry.creator = Some(creator.to_owned());
            entry.kind = kind;
            library.entries.push(entry);
        };
        extra("The Cramps", Some(EntryKind::Song), 1);
        extra("The Cramps", Some(EntryKind::Song), 2);
        extra("Alice in Chains", Some(EntryKind::Album), 3);
        extra("Alice in Chains", Some(EntryKind::Album), 4);
        for (sort, first) in [
            (Sort::MostAlbums, "Alice in Chains"),
            (Sort::MostSongs, "The Cramps"),
        ] {
            let mut entries: Vec<&Entry> = library.entries.iter().collect();
            sort.apply(&mut entries);
            let artists = artists(&entries, sort);
            assert_eq!(artists[0].name, first, "{sort:?}");
            let count = |a: &Artist| match sort {
                Sort::MostAlbums => a.albums.len(),
                _ => a.songs.len(),
            };
            for pair in artists.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                assert!(count(a) >= count(b), "{sort:?}: the most first");
                if count(a) == count(b) {
                    assert!(
                        artist_key(a.name) <= artist_key(b.name),
                        "{sort:?}: ties by name, {} before {}",
                        a.name,
                        b.name
                    );
                }
            }
        }
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

        // More words than the artist: their tracks whose title holds the
        // rest, the unrated ones too once the album's list is here.
        let titles: Vec<&str> = songs_matching(&found("alice in hole"), "alice in hole", |_| None)
            .iter()
            .map(|s| s.title())
            .collect();
        assert_eq!(titles, ["Down in a Hole"]);
        // A track found by its own title needs no rating either: Dam
        // That River is not rated, and no word names the artist.
        let albums: Vec<&Entry> = library
            .entries
            .iter()
            .filter(|e| may_list_tracks(e, "river"))
            .collect();
        assert!(albums.len() > 1, "every album may hold the track");
        let titles: Vec<&str> = songs_matching(&albums, "river", |album| {
            (album.title == "Dirt").then_some(list.as_slice())
        })
        .iter()
        .map(|s| s.title())
        .collect();
        assert_eq!(titles, ["Dam That River"]);
        // The entry itself does not hold it.
        let query = "chains river";
        assert!(found(query).is_empty());
        let dirt: Vec<&Entry> = library
            .entries
            .iter()
            .filter(|e| may_list_tracks(e, query))
            .collect();
        let titles: Vec<&str> = songs_matching(&dirt, query, |album| {
            (album.title == "Dirt").then_some(list.as_slice())
        })
        .iter()
        .map(|s| s.title())
        .collect();
        assert_eq!(titles, ["Dam That River"], "not the whole album");
        // "in" is no part of Pink Floyd or Vince Guaraldi here: only
        // Dirt's artist is named, so only Dirt lists every track.
        let albums: Vec<&Entry> = library
            .entries
            .iter()
            .filter(|e| may_list_tracks(e, "alice in"))
            .collect();
        let named = songs_matching(&albums, "alice in", |_| None);
        assert!(named.iter().all(|s| s.entry().title == "Dirt"));
        assert_eq!(named.len(), 3);

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
