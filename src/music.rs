//! Music by artist: the artist list, the Songs list (song entries and
//! rated album tracks together), and the matching behind the phone's
//! replace dialogs (DESIGN.md section 5 of the phone's design).

use crate::model::{Entry, MediaType, MusicKind, Track};

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
    entry.kind == Some(MusicKind::Song)
}

/// How an artist sorts: a leading "The" ignored, case ignored, Various
/// Artists after everything.
pub fn artist_key(name: &str) -> (bool, String) {
    let lower = name.trim().to_lowercase();
    let key = lower.strip_prefix("the ").unwrap_or(&lower).to_owned();
    (name == VARIOUS_ARTISTS, key)
}

/// One artist with what is saved of theirs.
#[derive(Debug, Clone, PartialEq)]
pub struct Artist<'a> {
    pub name: &'a str,
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

/// Every artist with an album or a song among `entries`, A to Z by
/// `artist_key`; each artist's entries keep their order.
pub fn artists<'a>(entries: &[&'a Entry]) -> Vec<Artist<'a>> {
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
    artists.sort_by_key(|a| artist_key(a.name));
    artists
}

/// A row on the Songs tab: a song entry, or a rated track of an album.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Song<'a> {
    Entry(&'a Entry),
    Track { album: &'a Entry, track: &'a Track },
}

impl<'a> Song<'a> {
    pub fn title(&self) -> &'a str {
        match self {
            Song::Entry(entry) => &entry.title,
            Song::Track { track, .. } => &track.title,
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
            Song::Track { track, .. } => Some(track.rating),
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

/// The Songs tab: song entries and the rated tracks of albums, in the
/// order of `entries` (the current sort), a track right after its album
/// would have been.
pub fn songs<'a>(entries: &[&'a Entry]) -> Vec<Song<'a>> {
    let mut songs = Vec::new();
    for &entry in entries.iter().filter(|e| e.media == MediaType::Music) {
        if is_song(entry) {
            songs.push(Song::Entry(entry));
        } else {
            songs.extend(entry.tracks.iter().flatten().map(|track| Song::Track {
                album: entry,
                track,
            }));
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
        if tracks
            .iter()
            .any(|t| norm(Some(&t.title)) == norm(Some(&song.title)))
        {
            continue;
        }
        tracks.push(Track {
            title: song.title.clone(),
            id: None,
            rating,
        });
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
        let artists = artists(&entries);
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
    fn an_artist_says_what_is_saved() {
        let library = library();
        let entries: Vec<&Entry> = library.entries.iter().collect();
        let artists = artists(&entries);
        let find = |name| artists.iter().find(|a| a.name == name).unwrap().summary();
        assert_eq!(find("Alice in Chains"), "1 album");
        assert_eq!(find("The Cramps"), "1 song");
        let two = Artist {
            name: "x",
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
    fn songs_mix_song_entries_and_rated_tracks() {
        let library = library();
        let entries: Vec<&Entry> = library.entries.iter().collect();
        let songs = songs(&entries);
        let rooster = songs.iter().find(|s| s.title() == "Rooster").unwrap();
        assert_eq!(rooster.entry().title, "Dirt", "a track opens its album");
        assert_eq!(rooster.subtitle(), "Alice in Chains · 1992");
        assert!(songs.iter().any(|s| s.title() == "Human Fly"));
    }

    #[test]
    fn replace_matching_needs_the_same_artist_and_album() {
        let library = library();
        let human_fly = library.entry("music/human-fly-the-cramps").unwrap();
        let album = Entry {
            id: "music/bad-music-for-bad-people-the-cramps".into(),
            title: "Bad Music for Bad People".into(),
            kind: Some(MusicKind::Album),
            tracks: None,
            ..human_fly.clone()
        };
        let found = songs_from_album(&library.entries, &album);
        assert_eq!(found.len(), 1);
        let absorbed = absorb_songs(album.clone(), &found);
        assert_eq!(absorbed.tracks.unwrap()[0].rating, 9);

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
