//! Movies and shows from TMDB (`tmdb_source.dart`). Ids are
//! "tmdb:movie:603" and "tmdb:tv:1399". The search gives title, date,
//! overview and poster; the director or the show's creators, the genres,
//! the cast, a show's seasons and a movie's collection (its series) come
//! with `details`, fetched when a match is picked.

use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, int_at, objects_at, str_at};
use super::text::{collection_name, pick_by_title_and_year};
use super::{
    Episode, Failure, FailureKind, Hits, Match, Reply, SeriesPart, SeriesParts, ShowSeasons, Source,
};
use crate::model::{MediaType, Released, Season, Series};

const API: &str = "https://api.themoviedb.org/3/";
const IMAGES: &str = "https://image.tmdb.org/t/p/";
/// Poster widths: rows show a small thumbnail, the cache keeps the list
/// and details size.
const ROW_POSTER: &str = "w185";
const COVER_POSTER: &str = "w342";
/// How many of the billed cast a match carries.
const TOP_BILLED: usize = 3;

pub struct Tmdb {
    token: String,
    http: Http,
}

impl Tmdb {
    pub fn new(token: &str) -> Tmdb {
        Tmdb {
            token: token.trim().to_owned(),
            http: Http::new(Duration::from_millis(50)),
        }
    }

    fn require_configured(&self) -> Reply<()> {
        if self.is_configured() {
            Ok(())
        } else {
            Err(Failure::new(FailureKind::NotConfigured, ""))
        }
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> Reply<Value> {
        self.require_configured()?;
        let auth = format!("Bearer {}", self.token);
        let url = http::url(&format!("{API}{path}"), query);
        let response = self.http.get(
            &url,
            &[("Authorization", &auth), ("Accept", "application/json")],
        )?;
        self.http.check(&response, &[200])?;
        response.object()
    }

    fn kind(media: MediaType) -> &'static str {
        if media == MediaType::Shows {
            "tv"
        } else {
            "movie"
        }
    }

    fn poster(path: Option<&str>, width: &str) -> Option<String> {
        path.map(|p| format!("{IMAGES}{width}{p}"))
    }

    /// A search hit as a match.
    fn hit(item: &Value, media: MediaType) -> Option<Match> {
        let tv = media == MediaType::Shows;
        let title = str_at(item, if tv { "name" } else { "title" })?;
        let id = int_at(item, "id")?;
        let date = str_at(item, if tv { "first_air_date" } else { "release_date" });
        let poster = str_at(item, "poster_path");
        Some(Match {
            released: date.and_then(Released::parse),
            description: str_at(item, "overview").unwrap_or_default().to_owned(),
            language: str_at(item, "original_language").map(str::to_owned),
            cover_url: Tmdb::poster(poster, ROW_POSTER),
            large_cover_url: Tmdb::poster(poster, COVER_POSTER),
            ..Match::new(media, title, format!("tmdb:{}:{id}", Tmdb::kind(media)))
        })
    }

    fn search_hits(&self, query: &str, media: MediaType) -> Reply<Vec<Match>> {
        let json = self.get(
            &format!("search/{}", Tmdb::kind(media)),
            &[("query", query), ("include_adult", "false")],
        )?;
        Ok(objects_at(&json, "results")
            .filter_map(|item| Tmdb::hit(item, media))
            .collect())
    }

    /// ("movie", "603") from "tmdb:movie:603".
    fn parse(source: &str) -> Reply<(&str, &str)> {
        let parts: Vec<&str> = source.split(':').collect();
        match parts.as_slice() {
            ["tmdb", kind @ ("movie" | "tv"), id] if !id.is_empty() => Ok((kind, id)),
            _ => Err(Failure::other(format!("not a TMDB id: {source}"))),
        }
    }

    /// A show's seasons with their air dates; season 0 (specials) is left
    /// out, and a season not dated yet has none.
    pub fn seasons_of(json: &Value) -> Vec<Season> {
        objects_at(json, "seasons")
            .filter_map(|s| {
                let number = int_at(s, "season_number").filter(|n| *n > 0)?;
                Some(Season {
                    season: u32::try_from(number).ok()?,
                    released: str_at(s, "air_date").and_then(Released::parse),
                    status: None,
                    fresh: None,
                    manual: None,
                })
            })
            .collect()
    }

    /// `/collection/{id}` for the collection a movie record names: the
    /// parts in date order, numbered from 1, a part without a date last.
    /// TMDB names collections "X Collection"; the series is "X".
    fn collection_of(&self, movie: &Value) -> Reply<Option<SeriesParts>> {
        let collection = &movie["belongs_to_collection"];
        if !collection.is_object() {
            return Ok(None);
        }
        let Some(id) = int_at(collection, "id") else {
            return Ok(None);
        };
        let json = self.get(&format!("collection/{id}"), &[])?;
        Ok(Tmdb::parse_collection(&json, str_at(collection, "name")))
    }

    /// The parts of a collection record, as `collection_of` lists them.
    pub fn parse_collection(json: &Value, fallback_name: Option<&str>) -> Option<SeriesParts> {
        let name = collection_name(str_at(json, "name").or(fallback_name).unwrap_or_default());
        if name.is_empty() {
            return None;
        }
        let mut parts: Vec<(i64, String, Option<Released>, Option<String>)> =
            objects_at(json, "parts")
                .filter_map(|p| {
                    Some((
                        int_at(p, "id")?,
                        str_at(p, "title")?.to_owned(),
                        str_at(p, "release_date").and_then(Released::parse),
                        str_at(p, "poster_path").map(str::to_owned),
                    ))
                })
                .collect();
        parts.sort_by(|a, b| match (a.2, b.2) {
            (Some(x), Some(y)) => x.available_from().cmp(&y.available_from()),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        });
        Some(SeriesParts {
            name,
            parts: parts
                .into_iter()
                .enumerate()
                .map(|(i, (id, title, released, poster))| SeriesPart {
                    number: Some(i as u32 + 1),
                    title,
                    year: released.map(|r| r.year),
                    released,
                    source: Some(format!("tmdb:movie:{id}")),
                    creator: None,
                    cover: Tmdb::poster(poster.as_deref(), COVER_POSTER),
                    extra: false,
                })
                .collect(),
        })
    }

    /// The episodes of a season record, in number order. TMDB names an
    /// episode without a title "Episode 5"; that counts as no title.
    pub fn parse_episodes(json: &Value) -> Vec<Episode> {
        let mut episodes: Vec<Episode> = objects_at(json, "episodes")
            .filter_map(|e| {
                let number = u32::try_from(int_at(e, "episode_number")?).ok()?;
                let title = str_at(e, "name")
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && *name != format!("Episode {number}"))
                    .map(str::to_owned);
                Some(Episode {
                    number,
                    title,
                    aired: str_at(e, "air_date").and_then(crate::model::Date::parse),
                    runtime: int_at(e, "runtime")
                        .filter(|m| *m > 0)
                        .and_then(|m| u32::try_from(m).ok()),
                })
            })
            .collect();
        episodes.sort_by_key(|e| e.number);
        episodes
    }
}

impl Source for Tmdb {
    fn id(&self) -> &'static str {
        "tmdb"
    }

    fn name(&self) -> &'static str {
        "TMDB"
    }

    fn types(&self) -> &'static [MediaType] {
        &[MediaType::Movies, MediaType::Shows]
    }

    fn is_configured(&self) -> bool {
        !self.token.is_empty()
    }

    fn search(&self, query: &str, media: MediaType, limit: usize) -> Reply<Hits> {
        let mut hits = self.search_hits(query, media)?;
        hits.truncate(limit);
        Ok(Hits::of(hits))
    }

    fn match_title(
        &self,
        title: &str,
        media: MediaType,
        year: Option<u16>,
        _creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        let hits = self.search_hits(title, media)?;
        Ok(
            pick_by_title_and_year(&hits, title, year, |m| &m.title, Match::year)
                .map(|i| hits[i].clone()),
        )
    }

    /// The credits carry the director (movies) and the billed cast (both).
    fn details(&self, found: &Match) -> Reply<Match> {
        let (kind, id) = Tmdb::parse(&found.source)?;
        let json = self.get(
            &format!("{kind}/{id}"),
            &[("append_to_response", "credits")],
        )?;
        let credits = &json["credits"];
        let tv = kind == "tv";
        let creators: Vec<&str> = if tv {
            objects_at(&json, "created_by")
                .filter_map(|c| str_at(c, "name"))
                .collect()
        } else {
            objects_at(credits, "crew")
                .filter(|c| str_at(c, "job") == Some("Director"))
                .filter_map(|c| str_at(c, "name"))
                .collect()
        };
        let cast: Vec<String> = objects_at(credits, "cast")
            .take(TOP_BILLED)
            .filter_map(|c| str_at(c, "name"))
            .map(str::to_owned)
            .collect();
        let date = str_at(&json, if tv { "first_air_date" } else { "release_date" });
        let poster = str_at(&json, "poster_path");
        let series = if tv {
            None
        } else {
            self.collection_of(&json)?.map(|parts| Series {
                number: parts
                    .parts
                    .iter()
                    .find(|p| p.source.as_deref() == Some(found.source.as_str()))
                    .and_then(|p| p.number),
                name: parts.name,
                cover: None,
            })
        };
        let mut full = found.clone();
        if !creators.is_empty() {
            full.creator = Some(creators.join(", "));
        }
        full.released = date.and_then(Released::parse).or(found.released);
        if let Some(overview) = str_at(&json, "overview") {
            full.description = overview.to_owned();
        }
        full.companies = objects_at(&json, "production_companies")
            .filter_map(|c| str_at(c, "name"))
            .map(str::to_owned)
            .collect();
        full.cast = cast;
        full.tags = objects_at(&json, "genres")
            .filter_map(|g| str_at(g, "name"))
            .map(str::to_owned)
            .collect();
        full.seasons = if tv {
            Tmdb::seasons_of(&json)
        } else {
            Vec::new()
        };
        full.series = series;
        if let Some(language) = str_at(&json, "original_language") {
            full.language = Some(language.to_owned());
        }
        full.network = tv
            .then(|| objects_at(&json, "networks").find_map(|n| str_at(n, "name")))
            .flatten()
            .map(str::to_owned);
        if poster.is_some() {
            full.cover_url = Tmdb::poster(poster, ROW_POSTER);
            full.large_cover_url = Tmdb::poster(poster, COVER_POSTER);
        }
        Ok(full)
    }

    fn seasons(&self, source: &str) -> Reply<Option<ShowSeasons>> {
        let (kind, id) = Tmdb::parse(source)?;
        if kind != "tv" {
            return Ok(None);
        }
        let json = self.get(&format!("tv/{id}"), &[])?;
        Ok(Some(ShowSeasons {
            seasons: Tmdb::seasons_of(&json),
            canceled: str_at(&json, "status") == Some("Canceled"),
            kind: None,
        }))
    }

    /// `/tv/{id}/season/{n}`. A season TMDB does not know (404) has no
    /// episodes.
    fn season_episodes(&self, source: &str, season: u32) -> Reply<Option<Vec<Episode>>> {
        let (kind, id) = Tmdb::parse(source)?;
        if kind != "tv" {
            return Ok(None);
        }
        match self.get(&format!("tv/{id}/season/{season}"), &[]) {
            Ok(json) => Ok(Some(Tmdb::parse_episodes(&json))),
            Err(failure) if failure.status == Some(404) => Ok(Some(Vec::new())),
            Err(failure) => Err(failure),
        }
    }

    /// A movie's collection; `None` for a show or a movie in none.
    fn series_parts(&self, source: &str, _name: Option<&str>) -> Reply<Option<SeriesParts>> {
        let (kind, id) = Tmdb::parse(source)?;
        if kind != "movie" {
            return Ok(None);
        }
        let json = self.get(&format!("movie/{id}"), &[])?;
        self.collection_of(&json)
    }

    fn cover_url(&self, source: &str, _media: MediaType) -> Reply<Option<String>> {
        let (kind, id) = Tmdb::parse(source)?;
        let json = self.get(&format!("{kind}/{id}"), &[])?;
        Ok(Tmdb::poster(str_at(&json, "poster_path"), COVER_POSTER))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_hit_becomes_a_match_with_its_posters() {
        let item: Value = serde_json::from_str(
            r#"{"id": 694, "title": "The Shining", "release_date": "1980-05-23",
                "overview": "A writer…", "poster_path": "/x.jpg", "original_language": "en"}"#,
        )
        .unwrap();
        let found = Tmdb::hit(&item, MediaType::Movies).unwrap();
        assert_eq!(found.source, "tmdb:movie:694");
        assert_eq!(found.year(), Some(1980));
        assert_eq!(
            found.cover_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w185/x.jpg")
        );
        assert_eq!(
            found.cache_cover_url(),
            Some("https://image.tmdb.org/t/p/w342/x.jpg")
        );
        assert!(
            Tmdb::hit(&item, MediaType::Shows).is_none(),
            "a show needs a name"
        );
        assert_eq!(Tmdb::parse("tmdb:tv:1399").unwrap(), ("tv", "1399"));
        assert!(Tmdb::parse("igdb:1").is_err());
    }

    #[test]
    fn collections_are_numbered_by_date_and_seasons_leave_out_specials() {
        let json: Value = serde_json::from_str(
            r#"{"name": "The Lord of the Rings Collection", "parts": [
                {"id": 122, "title": "Return", "release_date": "2003-12-01", "poster_path": "/r.jpg"},
                {"id": 120, "title": "Fellowship", "release_date": "2001-12-19"},
                {"id": 9, "title": "Untitled"}]}"#,
        )
        .unwrap();
        let parts = Tmdb::parse_collection(&json, None).unwrap();
        assert_eq!(parts.name, "The Lord of the Rings");
        assert_eq!(parts.parts[0].title, "Fellowship");
        assert_eq!(parts.parts[1].number, Some(2));
        assert_eq!(
            parts.parts[1].cover.as_deref(),
            Some("https://image.tmdb.org/t/p/w342/r.jpg")
        );
        assert_eq!(
            parts.parts[2].year, None,
            "a part without a date comes last"
        );

        let show: Value = serde_json::from_str(
            r#"{"seasons": [{"season_number": 0, "air_date": "2016-01-01"},
                {"season_number": 1, "air_date": "2016-07-15"}, {"season_number": 2}]}"#,
        )
        .unwrap();
        let seasons = Tmdb::seasons_of(&show);
        assert_eq!(seasons.len(), 2);
        assert_eq!(seasons[1].released, None);

        let season: Value = serde_json::from_str(
            r#"{"episodes": [{"episode_number": 2, "name": "Episode 2", "runtime": 0},
                {"episode_number": 1, "name": "Pilot", "air_date": "2016-07-15", "runtime": 49}]}"#,
        )
        .unwrap();
        let episodes = Tmdb::parse_episodes(&season);
        assert_eq!(episodes[0].title.as_deref(), Some("Pilot"));
        assert_eq!(episodes[1].title, None, "TMDB's placeholder counts as none");
        assert_eq!(episodes[1].runtime, None);
    }

    #[test]
    fn without_a_token_nothing_is_asked() {
        let tmdb = Tmdb::new("");
        assert!(!tmdb.is_configured());
        assert_eq!(
            tmdb.search("x", MediaType::Movies, 5).unwrap_err().kind,
            FailureKind::NotConfigured
        );
    }
}
