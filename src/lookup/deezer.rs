//! Albums, songs, track lists, covers and artist photos from Deezer's
//! public API (`deezer_source.dart`). No key. Ids are "deezer:album:302127"
//! and "deezer:track:3135556"; a song match also carries its album's id.
//! The search has no release date or genres, so `details` fetches the
//! album when a match is picked. Deezer answers errors as 200 with an
//! `error` object, which `data` turns into failures.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, int_at, objects_at, str_at};
use super::text::{pick_by_title_and_year, same_title};
use super::{
    ArtistHit, CreatorMatch, Failure, FailureKind, FollowKind, Hits, MATCHING_ARTISTS, Match,
    MusicList, MusicPage, MusicRequest, MusicScope, Reply, Source, TrackInfo, is_recent_or_ahead,
};
use crate::model::{Date, EntryKind, MediaType, Released};

const API: &str = "https://api.deezer.com";

/// Names that are not an artist.
pub const NO_PHOTO: [&str; 2] = ["various artists", "unknown artist"];

pub struct Deezer {
    http: Http,
    /// The album records asked this session, by Deezer's id: a search
    /// row's year and a pick's details read the same record.
    albums: Mutex<HashMap<String, Value>>,
    /// An artist's albums, newest first, by Deezer's id: paged through
    /// without asking again.
    discographies: Mutex<HashMap<String, Vec<Value>>>,
}

/// How many album records are kept before the cache starts over.
const ALBUM_CACHE: usize = 500;
/// How many artists a search asks for, to pick the known ones from.
const ARTIST_HITS: &str = "25";
/// An artist this far below the best known match is left out (a
/// hundredth of its fans).
const ARTIST_FAN_SHARE: i64 = 100;

impl Deezer {
    pub fn new() -> Deezer {
        Deezer {
            http: Http::new(Duration::from_millis(120)),
            albums: Mutex::new(HashMap::new()),
            discographies: Mutex::new(HashMap::new()),
        }
    }

    /// A GET that reads Deezer's answer: a failure for its `error` object
    /// (quota is rate limited, "no data" is `None`), else the JSON.
    fn data(&self, path: &str, query: &[(&str, &str)]) -> Reply<Option<Value>> {
        let url = http::url(&format!("{API}{path}"), query);
        // Deezer names genres in the language of the caller's country
        // ("Alternativmusik" from Sweden) unless told which one to use.
        let response = self.http.get(
            &url,
            &[("Accept", "application/json"), ("Accept-Language", "en")],
        )?;
        self.http.check(&response, &[200])?;
        let json = response.object()?;
        if let Some(error) = json.get("error").filter(|e| e.is_object()) {
            let code = int_at(error, "code");
            if code == Some(800) {
                return Ok(None);
            }
            return Err(Failure {
                kind: if code == Some(4) {
                    FailureKind::RateLimited
                } else {
                    FailureKind::Other
                },
                status: Some(response.status),
                message: str_at(error, "message")
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Deezer error {}", code.unwrap_or_default())),
            });
        }
        Ok(Some(json))
    }

    fn list(&self, path: &str, query: &[(&str, &str)]) -> Reply<Vec<Value>> {
        Ok(self
            .data(path, query)?
            .map(|json| objects_at(&json, "data").cloned().collect())
            .unwrap_or_default())
    }

    fn artist_name(item: &Value) -> Option<String> {
        str_at(&item["artist"], "name").map(str::to_owned)
    }

    fn album(item: &Value) -> Option<Match> {
        let id = int_at(item, "id")?;
        let title = str_at(item, "title")?;
        Some(Match {
            creator: Deezer::artist_name(item),
            kind: Some(EntryKind::Album),
            cover_url: str_at(item, "cover_medium").map(str::to_owned),
            large_cover_url: str_at(item, "cover_big").map(str::to_owned),
            ..Match::new(MediaType::Music, title, format!("deezer:album:{id}"))
        })
    }

    fn track(item: &Value) -> Option<Match> {
        let id = int_at(item, "id")?;
        let title = str_at(item, "title")?;
        let album = &item["album"];
        Some(Match {
            creator: Deezer::artist_name(item),
            kind: Some(EntryKind::Song),
            album: str_at(album, "title").map(str::to_owned),
            album_source: int_at(album, "id").map(|id| format!("deezer:album:{id}")),
            cover_url: str_at(album, "cover_medium").map(str::to_owned),
            large_cover_url: str_at(album, "cover_big").map(str::to_owned),
            ..Match::new(MediaType::Music, title, format!("deezer:track:{id}"))
        })
    }

    /// ("album", "302127") from "deezer:album:302127".
    fn parse(source: &str) -> Reply<(&str, &str)> {
        let parts: Vec<&str> = source.split(':').collect();
        match parts.as_slice() {
            ["deezer", kind @ ("album" | "track"), id] if id.parse::<u64>().is_ok() => {
                Ok((kind, id))
            }
            _ => Err(Failure::other(format!("not a Deezer id: {source}"))),
        }
    }

    /// The album record for an album or a track id.
    fn album_of(&self, source: &str) -> Reply<Option<Value>> {
        let (kind, id) = Deezer::parse(source)?;
        if kind == "album" {
            return self.album_record(id);
        }
        let Some(track) = self.data(&format!("/track/{id}"), &[])? else {
            return Ok(None);
        };
        match int_at(&track["album"], "id") {
            Some(album_id) => self.album_record(&album_id.to_string()),
            None => Ok(None),
        }
    }

    /// The album with Deezer's id `id`, asked once a session.
    fn album_record(&self, id: &str) -> Reply<Option<Value>> {
        if let Some(album) = self
            .albums
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            return Ok(Some(album.clone()));
        }
        let album = self.data(&format!("/album/{id}"), &[])?;
        if let Some(album) = &album {
            let mut albums = self.albums.lock().unwrap_or_else(|e| e.into_inner());
            if albums.len() >= ALBUM_CACHE {
                albums.clear();
            }
            albums.insert(id.to_owned(), album.clone());
        }
        Ok(album)
    }

    /// A page of one of Deezer's lists: its items, its `total` and
    /// whether a `next` page follows. A page may hold fewer items than
    /// asked (Deezer drops what is not available) and the next still
    /// starts a whole page on.
    fn page(&self, path: &str, query: &[(&str, &str)]) -> Reply<(Vec<Value>, Option<usize>, bool)> {
        Ok(self
            .data(path, query)?
            .map(|json| {
                let items = objects_at(&json, "data").cloned().collect();
                let total = int_at(&json, "total").and_then(|t| usize::try_from(t).ok());
                (items, total, str_at(&json, "next").is_some())
            })
            .unwrap_or_default())
    }

    /// Up to three artists for a search, the best known first: Deezer
    /// ranks a search's artists poorly ("Ghost & Gale" before Ghost), so
    /// the hits go by their fans, and one with a hundredth of the best's
    /// fans or fewer is left out.
    fn matching_artists(&self, query: &str) -> Reply<Vec<ArtistHit>> {
        let hits = self.list("/search/artist", &[("q", query), ("limit", ARTIST_HITS)])?;
        Ok(Deezer::best_artists(&hits))
    }

    fn best_artists(hits: &[Value]) -> Vec<ArtistHit> {
        let mut hits: Vec<(i64, &Value)> = hits
            .iter()
            .map(|hit| (int_at(hit, "nb_fan").unwrap_or_default(), hit))
            .collect();
        // Stable, so equal fans keep Deezer's order.
        hits.sort_by_key(|(fans, _)| std::cmp::Reverse(*fans));
        let best = hits.first().map_or(0, |(fans, _)| *fans);
        hits.into_iter()
            .filter(|(fans, _)| *fans * ARTIST_FAN_SHARE > best)
            .filter_map(|(_, hit)| {
                let name = str_at(hit, "name")?;
                if NO_PHOTO.contains(&name.trim().to_lowercase().as_str()) {
                    return None;
                }
                Some(ArtistHit {
                    source: format!("deezer:artist:{}", int_at(hit, "id")?),
                    name: name.to_owned(),
                    picture_url: str_at(hit, "picture_medium").map(str::to_owned),
                    albums: int_at(hit, "nb_album").and_then(|n| u32::try_from(n).ok()),
                })
            })
            .take(MATCHING_ARTISTS)
            .collect()
    }

    /// Every album, EP and single of the artist with Deezer's id `id`,
    /// newest first (Deezer lists the albums, then the EPs, then the
    /// singles, each newest first); asked once a session.
    fn discography(&self, id: &str) -> Reply<Vec<Value>> {
        if let Some(albums) = self
            .discographies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            return Ok(albums.clone());
        }
        let mut albums = self.list(&format!("/artist/{id}/albums"), &[("limit", "1000")])?;
        Deezer::newest_first(&mut albums);
        self.discographies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_owned(), albums.clone());
        Ok(albums)
    }

    /// Newest first by the release date; undated last, in Deezer's order.
    fn newest_first(albums: &mut [Value]) {
        albums.sort_by_key(|album| {
            std::cmp::Reverse(Deezer::released(album).map(Released::available_from))
        });
    }

    /// An album of an artist's list, which names no artist: `artist`'s.
    fn artist_album(item: &Value, artist: &str) -> Option<Match> {
        let mut album = Deezer::album(item)?;
        album.creator = Some(artist.to_owned());
        album.released = Deezer::released(item);
        Some(album)
    }

    /// The artist's songs from their albums, newest album first, from the
    /// album at `cursor` until `limit` songs are found; songs by others
    /// on the artist's records (a soundtrack's) are left out.
    fn artist_songs(
        &self,
        artist_id: &str,
        artist: &str,
        cursor: usize,
        limit: usize,
    ) -> Reply<MusicPage> {
        let albums = self.discography(artist_id)?;
        let mut page = MusicPage::default();
        let mut at = cursor;
        while at < albums.len() && page.matches.len() < limit {
            let album = &albums[at];
            at += 1;
            let Some(album_id) = int_at(album, "id") else {
                continue;
            };
            let tracks = self.list(&format!("/album/{album_id}/tracks"), &[("limit", "200")])?;
            for item in &tracks {
                if int_at(&item["artist"], "id")
                    .map(|id| id.to_string())
                    .as_deref()
                    != Some(artist_id)
                {
                    continue;
                }
                let (Some(id), Some(title)) = (int_at(item, "id"), str_at(item, "title")) else {
                    continue;
                };
                page.matches.push(Match {
                    creator: Some(artist.to_owned()),
                    kind: Some(EntryKind::Song),
                    album: str_at(album, "title").map(str::to_owned),
                    album_source: Some(format!("deezer:album:{album_id}")),
                    released: Deezer::released(album),
                    cover_url: str_at(album, "cover_medium").map(str::to_owned),
                    large_cover_url: str_at(album, "cover_big").map(str::to_owned),
                    ..Match::new(MediaType::Music, title, format!("deezer:track:{id}"))
                });
            }
        }
        page.next = (at < albums.len()).then_some(at);
        Ok(page)
    }

    fn released(album: &Value) -> Option<Released> {
        str_at(album, "release_date").and_then(Released::parse)
    }

    /// Deezer's own query syntax, for an album by a known artist.
    fn album_query(title: &str, artist: Option<&str>) -> String {
        match artist.filter(|a| !a.is_empty()) {
            Some(artist) => format!("artist:\"{artist}\" album:\"{title}\""),
            None => format!("album:\"{title}\""),
        }
    }
}

impl Default for Deezer {
    fn default() -> Self {
        Deezer::new()
    }
}

impl Source for Deezer {
    fn id(&self) -> &'static str {
        "deezer"
    }

    fn name(&self) -> &'static str {
        "Deezer"
    }

    fn types(&self) -> &'static [MediaType] {
        &[MediaType::Music]
    }

    fn is_music(&self) -> bool {
        true
    }

    fn search(&self, query: &str, _media: MediaType, limit: usize) -> Reply<Hits> {
        let each = limit.div_ceil(2).to_string();
        let albums = self.list("/search/album", &[("q", query), ("limit", &each)])?;
        let tracks = self.list("/search/track", &[("q", query), ("limit", &each)])?;
        Ok(Hits::of(
            albums
                .iter()
                .filter_map(Deezer::album)
                .chain(tracks.iter().filter_map(Deezer::track))
                .collect(),
        ))
    }

    /// The plain album before a remaster or a deluxe edition of it; the
    /// artist must agree when one is known.
    fn match_title(
        &self,
        title: &str,
        _media: MediaType,
        _year: Option<u16>,
        creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        let query = Deezer::album_query(title, creator);
        let hits: Vec<Match> = self
            .list("/search/album", &[("q", &query), ("limit", "10")])?
            .iter()
            .filter_map(Deezer::album)
            .filter(|hit| match creator.filter(|c| !c.is_empty()) {
                Some(creator) => hit
                    .creator
                    .as_deref()
                    .is_some_and(|c| same_title(c, creator)),
                None => true,
            })
            .collect();
        Ok(
            pick_by_title_and_year(&hits, title, None, |m| &m.title, |_| None)
                .map(|i| hits[i].clone()),
        )
    }

    fn details(&self, found: &Match) -> Reply<Match> {
        let Some(album) = self.album_of(&found.source)? else {
            return Ok(found.clone());
        };
        let mut full = found.clone();
        full.released = Deezer::released(&album).or(found.released);
        full.tags = objects_at(&album["genres"], "data")
            .filter_map(|g| str_at(g, "name"))
            .map(str::to_owned)
            .collect();
        if let Some(cover) = str_at(&album, "cover_medium") {
            full.cover_url = Some(cover.to_owned());
        }
        if let Some(cover) = str_at(&album, "cover_big") {
            full.large_cover_url = Some(cover.to_owned());
        }
        Ok(full)
    }

    fn cover_url(&self, source: &str, _media: MediaType) -> Reply<Option<String>> {
        Ok(self
            .album_of(source)?
            .and_then(|album| str_at(&album, "cover_big").map(str::to_owned)))
    }

    fn album_tracks(&self, album_source: &str) -> Reply<Vec<TrackInfo>> {
        let (kind, id) = Deezer::parse(album_source)?;
        if kind != "album" {
            return Err(Failure::other(format!("not an album id: {album_source}")));
        }
        Ok(self
            .list(&format!("/album/{id}/tracks"), &[("limit", "200")])?
            .iter()
            .filter_map(|item| {
                Some(TrackInfo {
                    id: format!("deezer:track:{}", int_at(item, "id")?),
                    title: str_at(item, "title")?.to_owned(),
                })
            })
            .collect())
    }

    fn music_page(&self, request: &MusicRequest, limit: usize) -> Reply<MusicPage> {
        let cursor = request.cursor;
        match &request.list {
            MusicList::Search(query) => {
                let (path, read): (&str, fn(&Value) -> Option<Match>) = match request.scope {
                    MusicScope::Albums => ("/search/album", Deezer::album),
                    MusicScope::Songs => ("/search/track", Deezer::track),
                };
                let (index, count) = (cursor.to_string(), limit.to_string());
                let (items, total, more) =
                    self.page(path, &[("q", query), ("index", &index), ("limit", &count)])?;
                let artists = if cursor == 0 {
                    self.matching_artists(query)?
                } else {
                    Vec::new()
                };
                Ok(MusicPage {
                    matches: items.iter().filter_map(read).collect(),
                    total,
                    next: more.then_some(cursor + limit),
                    artists,
                })
            }
            MusicList::Artist { source, name } => {
                let Some(id) = source.strip_prefix("deezer:artist:") else {
                    return Err(Failure::other(format!("not a Deezer artist id: {source}")));
                };
                match request.scope {
                    MusicScope::Albums => {
                        let albums = self.discography(id)?;
                        let end = (cursor + limit).min(albums.len());
                        Ok(MusicPage {
                            matches: albums
                                .get(cursor..end)
                                .unwrap_or_default()
                                .iter()
                                .filter_map(|item| Deezer::artist_album(item, name))
                                .collect(),
                            total: Some(albums.len()),
                            next: (end < albums.len()).then_some(end),
                            artists: Vec::new(),
                        })
                    }
                    MusicScope::Songs => self.artist_songs(id, name, cursor, limit),
                }
            }
        }
    }

    fn album_released(&self, album_source: &str) -> Reply<Option<Released>> {
        Ok(self
            .album_of(album_source)?
            .as_ref()
            .and_then(Deezer::released))
    }

    fn artist_photo_url(&self, name: &str) -> Reply<Option<String>> {
        Ok(self
            .artist(name)?
            .and_then(|hit| str_at(&hit, "picture_big").map(str::to_owned)))
    }

    fn creator_kinds(&self) -> &'static [FollowKind] {
        &[FollowKind::Artists]
    }

    /// The artist named exactly `name`, else the first hit.
    fn find_creator(&self, _kind: FollowKind, name: &str) -> Reply<Option<CreatorMatch>> {
        let hits = self.list("/search/artist", &[("q", name), ("limit", "5")])?;
        let found = hits
            .iter()
            .find(|hit| str_at(hit, "name").is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .or(hits.first());
        Ok(found
            .and_then(|hit| int_at(hit, "id"))
            .map(|id| CreatorMatch {
                provider_id: format!("deezer:artist:{id}"),
                inactive: None,
            }))
    }

    /// The artist's albums and EPs (`/artist/{id}/albums`), singles and
    /// compilations left out, each dated as Deezer lists it, soonest
    /// first. One request: the name comes from the check.
    fn creator_releases(
        &self,
        provider_id: &str,
        _kind: FollowKind,
        today: Date,
    ) -> Reply<Vec<Match>> {
        let Some(id) = provider_id.strip_prefix("deezer:artist:") else {
            return Err(Failure::other(format!(
                "not a Deezer artist id: {provider_id}"
            )));
        };
        let albums = self.list(&format!("/artist/{id}/albums"), &[("limit", "50")])?;
        let mut found: Vec<Match> = albums
            .iter()
            .filter(|item| matches!(str_at(item, "record_type"), Some("album" | "ep")))
            .filter_map(|item| {
                let released = Deezer::released(item)?;
                if !is_recent_or_ahead(Some(released), today, 30) {
                    return None;
                }
                let mut album = Deezer::album(item)?;
                album.released = Some(released);
                Some(album)
            })
            .collect();
        found.sort_by_key(|m| m.released.map(Released::available_from));
        Ok(found)
    }
}

impl Deezer {
    /// Deezer's record of the artist called exactly `name` (case aside);
    /// none for "Various Artists" and "Unknown artist".
    fn artist(&self, name: &str) -> Reply<Option<Value>> {
        if NO_PHOTO.contains(&name.trim().to_lowercase().as_str()) {
            return Ok(None);
        }
        let hits = self.list("/search/artist", &[("q", name), ("limit", "5")])?;
        Ok(hits
            .into_iter()
            .find(|hit| str_at(hit, "name").is_some_and(|n| same_title(n, name))))
    }
}

/// The album cover size the cover viewer asks for (Deezer's `cover_xl`).
const VIEWER_COVER: u32 = 1000;

impl Deezer {
    /// The same cover at `cover_xl`'s size, when `url` is a smaller Deezer
    /// image ("…/500x500-000000-80-0-0.jpg").
    pub fn viewer_urls(url: &str) -> Option<Vec<String>> {
        let (head, file) = url.rsplit_once('/')?;
        let host = url.split('/').nth(2)?;
        if !host.ends_with(".dzcdn.net") {
            return None;
        }
        let (size, tail) = file.split_once('-')?;
        let (w, h) = size.split_once('x')?;
        let (w, _): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
        Some(if w < VIEWER_COVER {
            vec![format!("{head}/{VIEWER_COVER}x{VIEWER_COVER}-{tail}")]
        } else {
            Vec::new()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn albums_and_tracks_become_matches() {
        let album: Value = serde_json::from_str(
            r#"{"id": 302127, "title": "Mezzanine", "artist": {"name": "Massive Attack"},
                "cover_medium": "m.jpg", "cover_big": "b.jpg"}"#,
        )
        .unwrap();
        let found = Deezer::album(&album).unwrap();
        assert_eq!(found.source, "deezer:album:302127");
        assert_eq!(found.kind, Some(EntryKind::Album));
        assert_eq!(found.cache_cover_url(), Some("b.jpg"));
        let track: Value = serde_json::from_str(
            r#"{"id": 1, "title": "Teardrop", "artist": {"name": "Massive Attack"},
                "album": {"id": 302127, "title": "Mezzanine", "cover_medium": "m.jpg"}}"#,
        )
        .unwrap();
        let song = Deezer::track(&track).unwrap();
        assert_eq!(song.album_source.as_deref(), Some("deezer:album:302127"));
        assert_eq!(song.kind, Some(EntryKind::Song));
        assert_eq!(Deezer::parse("deezer:track:3").unwrap(), ("track", "3"));
        assert!(Deezer::parse("deezer:artist:3").is_err());
        assert_eq!(
            Deezer::album_query("Dirt", Some("Alice in Chains")),
            "artist:\"Alice in Chains\" album:\"Dirt\""
        );
    }

    #[test]
    fn a_searchs_artists_go_by_their_fans_and_albums_newest_first() {
        let hits: Vec<Value> = serde_json::from_str(
            r#"[{"id": 1, "name": "Ghost & Gale", "nb_fan": 45, "nb_album": 1},
                {"id": 2, "name": "Ghost", "nb_fan": 615851, "nb_album": 30,
                 "picture_medium": "p.jpg"},
                {"id": 3, "name": "Ghostface Killah", "nb_fan": 402118, "nb_album": 17},
                {"id": 4, "name": "Various Artists", "nb_fan": 900000},
                {"id": 5, "name": "Ghost Town", "nb_fan": 52340, "nb_album": 3},
                {"id": 6, "name": "Ghost B.C.", "nb_fan": 6000}]"#,
        )
        .unwrap();
        let best = Deezer::best_artists(&hits);
        let names: Vec<&str> = best.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Ghost", "Ghostface Killah", "Ghost Town"]);
        assert_eq!(best[0].source, "deezer:artist:2");
        assert_eq!(best[0].albums, Some(30));
        assert_eq!(best[0].picture_url.as_deref(), Some("p.jpg"));

        let mut albums: Vec<Value> = serde_json::from_str(
            r#"[{"title": "Impera", "release_date": "2022-03-11", "record_type": "album"},
                {"title": "Popestar", "release_date": "2016-09-16", "record_type": "ep"},
                {"title": "Undated"},
                {"title": "It's A Sin", "release_date": "2026-01-16", "record_type": "single"}]"#,
        )
        .unwrap();
        Deezer::newest_first(&mut albums);
        let titles: Vec<&str> = albums.iter().filter_map(|a| str_at(a, "title")).collect();
        assert_eq!(titles, ["It's A Sin", "Impera", "Popestar", "Undated"]);
    }
}
