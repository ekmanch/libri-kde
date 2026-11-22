//! Apple Music through the iTunes Search API, the second music catalog
//! (the phone's `itunes_music.dart`, its DESIGN.md 10c "Apple Music on
//! demand"). Nothing asks it on its own: only the Add dialog's extended
//! search, and the entries saved from it, whose ids are
//! "itunes:album:1440650711" and "itunes:track:1440650834" (a plain
//! "itunes:6756269989" is an Apple Books id). It serves no type, so no
//! search, automatic match, artist list or daily check reaches it; an
//! entry's cover, date and track list find it by the entry's id. It asks
//! on the books lookup's client, since the two share Apple's twenty
//! requests a minute. An empty answer from one store is no reliable miss,
//! so a search asks every store and merges by id, and a lookup by id asks
//! them in turn until one answers.

use serde_json::Value;

use super::books::{ITunes, LARGE_ARTWORK, ROW_ARTWORK};
use super::http::{int_at, str_at};
use super::{Failure, FailureKind, Hits, Match, MusicScope, Reply, Source, TrackInfo};
use crate::model::{EntryKind, MediaType, Released};

/// The storefronts asked, in order: the owner's, then the largest.
const STORES: [&str; 2] = ["SE", "US"];
/// How many rows one store is asked for.
const SEARCH_LIMIT: &str = "50";

pub struct AppleMusic {
    api: ITunes,
}

/// ("album", "1440650711") from "itunes:album:1440650711"; `None` for
/// anything else, an Apple Books id ("itunes:6756269989") included.
pub fn parse(source: &str) -> Option<(&str, &str)> {
    let mut parts = source.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("itunes"), Some(kind @ ("album" | "track")), Some(id), None)
            if id.parse::<u64>().is_ok() =>
        {
            Some((kind, id))
        }
        _ => None,
    }
}

fn parsed(source: &str) -> Reply<(&str, &str)> {
    parse(source).ok_or_else(|| Failure::other(format!("not an Apple Music id: {source}")))
}

/// Apple titles a single "Boss Bitch - Single" and an EP "Burgh Island -
/// EP": the note is no part of the title.
fn without_record_note(name: &str) -> &str {
    name.strip_suffix(" - Single")
        .or_else(|| name.strip_suffix(" - EP"))
        .unwrap_or(name)
        .trim_end()
}

fn released(item: &Value) -> Option<Released> {
    str_at(item, "releaseDate")
        .and_then(|d| d.split('T').next())
        .and_then(Released::parse)
}

fn is_album(item: &Value) -> bool {
    str_at(item, "wrapperType") == Some("collection")
}

fn is_song(item: &Value) -> bool {
    str_at(item, "wrapperType") == Some("track") && str_at(item, "kind") == Some("song")
}

/// What an album's and a song's row share: the artist, the date, the
/// genre as a tag and the artwork at the row's and the cache's size.
fn row(item: &Value, title: &str, source: String, kind: EntryKind) -> Match {
    let artwork = str_at(item, "artworkUrl100");
    Match {
        creator: str_at(item, "artistName").map(str::to_owned),
        released: released(item),
        kind: Some(kind),
        tags: str_at(item, "primaryGenreName")
            .map(str::to_owned)
            .into_iter()
            .collect(),
        cover_url: artwork.map(|a| ITunes::artwork_at(a, ROW_ARTWORK)),
        large_cover_url: artwork.map(|a| ITunes::artwork_at(a, LARGE_ARTWORK)),
        ..Match::new(MediaType::Music, title, source)
    }
}

pub fn album(item: &Value) -> Option<Match> {
    if !is_album(item) {
        return None;
    }
    let id = int_at(item, "collectionId")?;
    let name = str_at(item, "collectionName")?;
    Some(row(
        item,
        without_record_note(name),
        format!("itunes:album:{id}"),
        EntryKind::Album,
    ))
}

pub fn track(item: &Value) -> Option<Match> {
    if !is_song(item) {
        return None;
    }
    let id = int_at(item, "trackId")?;
    let title = str_at(item, "trackName")?;
    Some(Match {
        album: str_at(item, "collectionName").map(|name| without_record_note(name).to_owned()),
        album_source: int_at(item, "collectionId").map(|id| format!("itunes:album:{id}")),
        ..row(item, title, format!("itunes:track:{id}"), EntryKind::Song)
    })
}

/// The stores' answers as one list, each id once, in the stores' order.
/// A store that failed after another answered is passed over; when none
/// answered, the failure is the answer.
pub fn merge_stores(answers: Vec<Reply<Vec<Match>>>) -> Reply<Vec<Match>> {
    let mut out: Vec<Match> = Vec::new();
    let mut failure = None;
    let mut answered = false;
    for answer in answers {
        match answer {
            Ok(matches) => {
                answered = true;
                for found in matches {
                    if !out.iter().any(|m| m.source == found.source) {
                        out.push(found);
                    }
                }
            }
            Err(e) => failure = Some(e),
        }
    }
    match failure {
        Some(failure) if !answered => Err(failure),
        _ => Ok(out),
    }
}

/// An album's songs in disc and track order, as its track list.
pub fn track_list(items: &[Value]) -> Vec<TrackInfo> {
    let at = |item: &Value, key: &str| int_at(item, key).unwrap_or_default();
    let mut songs: Vec<&Value> = items.iter().filter(|item| is_song(item)).collect();
    // Stable, so songs Apple gives no number keep its order.
    songs.sort_by_key(|item| (at(item, "discNumber"), at(item, "trackNumber")));
    songs
        .into_iter()
        .filter_map(|item| {
            Some(TrackInfo {
                id: format!("itunes:track:{}", int_at(item, "trackId")?),
                title: str_at(item, "trackName")?.to_owned(),
            })
        })
        .collect()
}

impl AppleMusic {
    pub fn new() -> AppleMusic {
        AppleMusic { api: ITunes::new() }
    }

    /// The records Apple lists for `id` (`entity` adds an album's songs),
    /// from the first store that has any.
    fn lookup(&self, id: &str, entity: Option<&str>) -> Reply<Vec<Value>> {
        for store in STORES {
            let mut query = vec![("id", id), ("country", store), ("limit", "200")];
            if let Some(entity) = entity {
                query.push(("entity", entity));
            }
            let items = self.api.objects("/lookup", &query)?;
            if !items.is_empty() {
                return Ok(items);
            }
        }
        Ok(Vec::new())
    }

    /// The album's or the song's own record, or `None` when no store has
    /// it.
    fn record(&self, source: &str) -> Reply<Option<Match>> {
        let (kind, id) = parsed(source)?;
        let read: fn(&Value) -> Option<Match> = if kind == "album" { album } else { track };
        Ok(self.lookup(id, None)?.iter().find_map(read))
    }

    /// `term` in every store, merged by id.
    fn search_stores(&self, term: &str, scope: MusicScope) -> Reply<Vec<Match>> {
        let (entity, read): (&str, fn(&Value) -> Option<Match>) = match scope {
            MusicScope::Albums => ("album", album),
            MusicScope::Songs => ("song", track),
        };
        let mut answers = Vec::new();
        for store in STORES {
            let answer = self.api.objects(
                "/search",
                &[
                    ("term", term),
                    ("media", "music"),
                    ("entity", entity),
                    ("country", store),
                    ("limit", SEARCH_LIMIT),
                ],
            );
            // Apple refusing one store refuses the next too.
            let stop = matches!(
                &answer,
                Err(e) if matches!(e.kind, FailureKind::RateLimited | FailureKind::Offline)
            );
            answers.push(answer.map(|items| items.iter().filter_map(read).collect()));
            if stop {
                break;
            }
        }
        merge_stores(answers)
    }
}

impl Default for AppleMusic {
    fn default() -> Self {
        AppleMusic::new()
    }
}

impl Source for AppleMusic {
    fn id(&self) -> &'static str {
        "itunes"
    }

    fn name(&self) -> &'static str {
        "Apple Music"
    }

    /// No type: the music search is Deezer's.
    fn types(&self) -> &'static [MediaType] {
        &[]
    }

    fn issued(&self, source: &str) -> bool {
        parse(source).is_some()
    }

    fn search(&self, _query: &str, _media: MediaType, _limit: usize) -> Reply<Hits> {
        Ok(Hits::default())
    }

    /// Never matched automatically.
    fn match_title(
        &self,
        _title: &str,
        _media: MediaType,
        _year: Option<u16>,
        _creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        Ok(None)
    }

    fn cover_url(&self, source: &str, _media: MediaType) -> Reply<Option<String>> {
        Ok(self
            .record(source)?
            .and_then(|m| m.cache_cover_url().map(str::to_owned)))
    }

    fn album_tracks(&self, album_source: &str) -> Reply<Vec<TrackInfo>> {
        let (kind, id) = parsed(album_source)?;
        if kind != "album" {
            return Err(Failure::other(format!("not an album id: {album_source}")));
        }
        Ok(track_list(&self.lookup(id, Some("song"))?))
    }

    fn album_released(&self, album_source: &str) -> Reply<Option<Released>> {
        Ok(self.record(album_source)?.and_then(|m| m.released))
    }

    fn extended_music(&self, query: &str, scope: MusicScope) -> Option<Reply<Vec<Match>>> {
        Some(self.search_stores(query.trim(), scope))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn song(id: i64, name: &str, disc: i64, number: i64) -> Value {
        json!({
            "wrapperType": "track", "kind": "song", "trackId": id, "trackName": name,
            "discNumber": disc, "trackNumber": number,
        })
    }

    #[test]
    fn an_apple_music_id_is_told_from_an_apple_books_id() {
        assert_eq!(
            parse("itunes:album:1440650711"),
            Some(("album", "1440650711"))
        );
        assert_eq!(parse("itunes:track:7"), Some(("track", "7")));
        for other in [
            "itunes:6756269989",
            "itunes:album:x",
            "itunes:artist:1",
            "itunes:album:1:2",
            "deezer:album:1",
        ] {
            assert_eq!(parse(other), None, "{other}");
        }
        let music = AppleMusic::new();
        assert!(music.issued("itunes:track:7") && !music.issued("itunes:7"));
        assert!(music.types().is_empty());
    }

    #[test]
    fn a_song_becomes_a_match_with_its_album_and_no_record_note() {
        let item = json!({
            "wrapperType": "track", "kind": "song",
            "trackId": 572697308, "trackName": "Oats In the Water",
            "collectionId": 572697239, "collectionName": "Burgh Island - EP",
            "artistName": "Ben Howard", "primaryGenreName": "Singer/Songwriter",
            "releaseDate": "2012-11-25T08:00:00Z",
            "artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/a/b/100x100bb.jpg",
        });
        let found = track(&item).unwrap();
        assert_eq!(found.source, "itunes:track:572697308");
        assert_eq!(found.title, "Oats In the Water");
        assert_eq!(found.kind, Some(EntryKind::Song));
        assert_eq!(found.creator.as_deref(), Some("Ben Howard"));
        assert_eq!(found.album.as_deref(), Some("Burgh Island"));
        assert_eq!(
            found.album_source.as_deref(),
            Some("itunes:album:572697239")
        );
        assert_eq!(found.year(), Some(2012));
        assert_eq!(found.tags, ["Singer/Songwriter"]);
        assert_eq!(
            found.cache_cover_url(),
            Some("https://is1-ssl.mzstatic.com/image/thumb/a/b/600x600bb.jpg")
        );
        // A song row is no album, and a music video no song.
        assert!(album(&item).is_none());
        assert!(track(&json!({"wrapperType": "track", "kind": "music-video", "trackId": 1, "trackName": "x"})).is_none());
    }

    #[test]
    fn an_album_becomes_a_match_and_a_single_loses_its_note() {
        let item = json!({
            "wrapperType": "collection", "collectionId": 9, "collectionName": "Boss Bitch - Single",
            "artistName": "Doja Cat", "releaseDate": "2020-01-23T08:00:00Z",
        });
        let found = album(&item).unwrap();
        assert_eq!(found.source, "itunes:album:9");
        assert_eq!(found.title, "Boss Bitch");
        assert_eq!(found.kind, Some(EntryKind::Album));
        assert!(found.album.is_none() && found.cover_url.is_none());
    }

    #[test]
    fn the_stores_merge_by_id_and_fail_only_when_none_answered() {
        let m = |id: &str| Match::new(MediaType::Music, "t", id);
        let offline = || Failure::new(FailureKind::Offline, "");
        let merged = merge_stores(vec![
            Ok(vec![m("itunes:track:1"), m("itunes:track:2")]),
            Ok(vec![m("itunes:track:2"), m("itunes:track:3")]),
        ])
        .unwrap();
        let ids: Vec<&str> = merged.iter().map(|m| m.source.as_str()).collect();
        assert_eq!(ids, ["itunes:track:1", "itunes:track:2", "itunes:track:3"]);
        assert_eq!(
            merge_stores(vec![Ok(vec![m("itunes:track:1")]), Err(offline())])
                .unwrap()
                .len(),
            1
        );
        assert!(merge_stores(vec![Err(offline()), Err(offline())]).is_err());
        assert!(
            merge_stores(vec![Ok(Vec::new()), Ok(Vec::new())])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_track_list_is_in_disc_and_track_order_without_the_albums_own_row() {
        let items = vec![
            json!({"wrapperType": "collection", "collectionId": 1, "collectionName": "A"}),
            song(13, "Three", 2, 1),
            song(12, "Two", 1, 2),
            song(11, "One", 1, 1),
        ];
        let tracks = track_list(&items);
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["One", "Two", "Three"]);
        assert_eq!(tracks[0].id, "itunes:track:11");
    }
}
