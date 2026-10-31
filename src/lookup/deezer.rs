//! Albums, songs, track lists, covers and artist photos from Deezer's
//! public API (`deezer_source.dart`). No key. Ids are "deezer:album:302127"
//! and "deezer:track:3135556"; a song match also carries its album's id.
//! The search has no release date or genres, so `details` fetches the
//! album when a match is picked. Deezer answers errors as 200 with an
//! `error` object, which `data` turns into failures.

use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, int_at, objects_at, str_at};
use super::text::{pick_by_title_and_year, same_title};
use super::{Failure, FailureKind, Hits, Match, Reply, Source, TrackInfo};
use crate::model::{EntryKind, MediaType, Released};

const API: &str = "https://api.deezer.com";

/// Names that are not an artist.
pub const NO_PHOTO: [&str; 2] = ["various artists", "unknown artist"];

pub struct Deezer {
    http: Http,
}

impl Deezer {
    pub fn new() -> Deezer {
        Deezer {
            http: Http::new(Duration::from_millis(120)),
        }
    }

    /// A GET that reads Deezer's answer: a failure for its `error` object
    /// (quota is rate limited, "no data" is `None`), else the JSON.
    fn data(&self, path: &str, query: &[(&str, &str)]) -> Reply<Option<Value>> {
        let url = http::url(&format!("{API}{path}"), query);
        let response = self.http.get(&url, &[("Accept", "application/json")])?;
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
            return self.data(&format!("/album/{id}"), &[]);
        }
        let Some(track) = self.data(&format!("/track/{id}"), &[])? else {
            return Ok(None);
        };
        match int_at(&track["album"], "id") {
            Some(album_id) => self.data(&format!("/album/{album_id}"), &[]),
            None => Ok(None),
        }
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

    fn artist_photo_url(&self, name: &str) -> Reply<Option<String>> {
        if NO_PHOTO.contains(&name.trim().to_lowercase().as_str()) {
            return Ok(None);
        }
        let hits = self.list("/search/artist", &[("q", name), ("limit", "5")])?;
        Ok(hits
            .iter()
            .find(|hit| str_at(hit, "name").is_some_and(|n| same_title(n, name)))
            .and_then(|hit| str_at(hit, "picture_big").map(str::to_owned)))
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
}
