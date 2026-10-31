//! Games from IGDB (`igdb_source.dart`), which sits behind Twitch: a client
//! id and secret turn into an app access token (kept in `igdb_token.json`
//! under the cache folder), sent with every query. Ids are "igdb:1009".
//! One search carries everything the form needs: name, release date,
//! summary, developers, platforms, genres and cover; the game's
//! collection (its series) comes with `details`.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};

use super::http::{Http, int_at, objects_at, str_at};
use super::store::JsonFile;
use super::text::pick_by_title_and_year;
use super::{
    Failure, FailureKind, GamePlatform, Hits, Match, Reply, SeriesPart, SeriesParts, Source,
};
use crate::clock::Timestamp;
use crate::model::{Date, MediaType, Released, Series, text};

const API: &str = "https://api.igdb.com/v4/";
const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const IMAGES: &str = "https://images.igdb.com/igdb/image/upload/";
/// 264 × 374, enough for a row and the details header.
const COVER_SIZE: &str = "t_cover_big";
/// How long before its expiry a stored token stops being used.
const TOKEN_MARGIN: i64 = 86_400;
/// How long a stored platform list is trusted: IGDB adds a platform a few
/// times a year.
const PLATFORMS_MAX_AGE: i64 = 30 * 86_400;

const FIELDS: &str = "name,first_release_date,summary,cover.image_id,platforms.name,\
    involved_companies.company.name,involved_companies.developer,genres.name,\
    language_supports.language.locale,language_supports.language_support_type";
/// `FIELDS` and what ranks a search hit: its platforms' types, how many
/// ratings it has and the game it is a port of.
const SEARCH_FIELDS: &str = "name,first_release_date,summary,cover.image_id,platforms.name,\
    involved_companies.company.name,involved_companies.developer,genres.name,\
    language_supports.language.locale,language_supports.language_support_type,\
    platforms.platform_type,platforms.id,total_rating_count,parent_game";
/// The collections a game is in, with their games: the series and its
/// parts. `game_type` tells main games and remakes from bundles, DLC,
/// remasters and ports.
const COLLECTION_FIELDS: &str = "name,parent_game,collections.name,collections.games.name,\
    collections.games.first_release_date,collections.games.game_type,\
    collections.games.version_parent,collections.games.cover.image_id";
/// Packs and updates are never offered.
const WHERE: &str = "version_parent = null & game_type != (13, 14)";

/// IGDB's platform types: 2 is an arcade cabinet, 6 a home computer.
const ARCADE: i64 = 2;
const COMPUTER: i64 = 6;
/// Platforms nobody adds a game for: handheld LCD games and pre-smartphone
/// mobiles.
const MINOR_PLATFORMS: [i64; 2] = [411, 55];
/// IGDB's `language_support_type` id for spoken audio.
const AUDIO_SUPPORT: i64 = 1;
/// IGDB's game types that count as a part of a series: a main game, a
/// remake.
const PART_TYPES: [i64; 2] = [0, 8];

/// Collections whose game types mislead, by IGDB's collection id: the
/// extra types that count as parts, and the games left out.
const COLLECTION_RULES: [(i64, &[i64], &[i64]); 2] = [
    // Street Fighter II: the revisions count; the movie game, a pachislot,
    // Uno and pinball go.
    (2879, &[9, 10], &[45192, 55060, 55063, 55062]),
    // Street Fighter: Final Fight, 2010, the puzzle and gem fighters and
    // the other spin-offs go.
    (
        219,
        &[9, 10],
        &[
            1655, 45196, 20803, 23072, 39314, 55061, 60889, 55065, 69377, 45184, 1239, 343673,
        ],
    ),
];

pub struct Igdb {
    client_id: String,
    client_secret: String,
    http: Http,
    token_file: Option<JsonFile>,
    platforms_file: Option<JsonFile>,
    token: Mutex<Option<String>>,
    platforms: Mutex<Option<Vec<GamePlatform>>>,
}

impl Igdb {
    pub fn new(client_id: &str, client_secret: &str, cache: Option<&std::path::Path>) -> Igdb {
        Igdb {
            client_id: client_id.trim().to_owned(),
            client_secret: client_secret.trim().to_owned(),
            http: Http::new(Duration::from_millis(300)),
            token_file: cache.map(|dir| JsonFile(dir.join("igdb_token.json"))),
            platforms_file: cache.map(|dir| JsonFile(dir.join("igdb_platforms.json"))),
            token: Mutex::new(None),
            platforms: Mutex::new(None),
        }
    }

    fn require_configured(&self) -> Reply<()> {
        if self.is_configured() {
            Ok(())
        } else {
            Err(Failure::new(FailureKind::NotConfigured, ""))
        }
    }

    /// The stored token while it is good for at least a day more.
    fn stored_token(&self) -> Option<String> {
        let json = self.token_file.as_ref()?.read()?;
        let token = str_at(&json, "access_token")?;
        let expires = Timestamp::parse(str_at(&json, "expires_at")?)?;
        (Timestamp::now().0 + TOKEN_MARGIN < expires.0).then(|| token.to_owned())
    }

    fn access_token(&self) -> Reply<String> {
        if let Some(token) = self.token.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Ok(token);
        }
        if let Some(token) = self.stored_token() {
            *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
            return Ok(token);
        }
        let url = super::http::url(
            TOKEN_URL,
            &[
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("grant_type", "client_credentials"),
            ],
        );
        let response = self
            .http
            .post(&url, &[("Accept", "application/json")], "")?;
        self.http.check(&response, &[200])?;
        let json = response.object()?;
        let token = str_at(&json, "access_token")
            .ok_or_else(|| Failure::other("no access token from Twitch"))?
            .to_owned();
        let expires_in = int_at(&json, "expires_in").unwrap_or(3600);
        if let Some(file) = &self.token_file {
            let _ = file.write(&json!({
                "access_token": token,
                "expires_at": Timestamp(Timestamp::now().0 + expires_in).to_rfc3339(),
            }));
        }
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        Ok(token)
    }

    /// Runs an Apicalypse `body` against `endpoint`. A refused token is
    /// dropped and fetched again once.
    fn query(&self, endpoint: &str, body: &str) -> Reply<Vec<Value>> {
        self.require_configured()?;
        for attempt in 0..2 {
            let token = self.access_token()?;
            let auth = format!("Bearer {token}");
            let response = self.http.post(
                &format!("{API}{endpoint}"),
                &[
                    ("Client-ID", &self.client_id),
                    ("Authorization", &auth),
                    ("Accept", "application/json"),
                ],
                body,
            )?;
            if response.status == 401 && attempt == 0 {
                *self.token.lock().unwrap_or_else(|e| e.into_inner()) = None;
                if let Some(file) = &self.token_file {
                    file.delete();
                }
                continue;
            }
            self.http.check(&response, &[200])?;
            let json = response.json()?;
            return match json {
                Value::Array(items) => Ok(items.into_iter().filter(Value::is_object).collect()),
                _ => Err(Failure {
                    kind: FailureKind::Other,
                    status: Some(response.status),
                    message: "not a JSON list".into(),
                }),
            };
        }
        unreachable!("two attempts return")
    }

    fn games(&self, body: &str) -> Reply<Vec<Value>> {
        self.query("games", body)
    }

    fn quote(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }

    /// IGDB says "PC (Microsoft Windows)"; the form's normalizer takes it
    /// from there.
    fn platform_name(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        let mut depth = 0;
        for c in name.chars() {
            match c {
                '(' => depth += 1,
                ')' => depth = (depth - 1).max(0),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The language a game was made in, as far as IGDB tells: its one
    /// audio language. A game dubbed into several says nothing.
    fn language(game: &Value) -> Option<String> {
        let mut audio: Vec<&str> = objects_at(game, "language_supports")
            .filter(|l| int_at(l, "language_support_type") == Some(AUDIO_SUPPORT))
            .filter_map(|l| str_at(&l["language"], "locale"))
            .collect();
        audio.sort_unstable();
        audio.dedup();
        match audio.as_slice() {
            [only] => Some((*only).to_owned()),
            _ => None,
        }
    }

    /// A unix time as the day it falls on, in UTC.
    fn day_of(epoch: i64) -> Released {
        let date = Date::from_days(epoch.div_euclid(86_400));
        Released {
            year: date.year,
            month: Some(date.month),
            day: Some(date.day),
        }
    }

    pub fn cover_url_for(image_id: &str) -> String {
        format!("{IMAGES}{COVER_SIZE}/{image_id}.jpg")
    }

    fn hit(game: &Value) -> Option<Match> {
        let id = int_at(game, "id")?;
        let name = str_at(game, "name")?;
        let developers: Vec<String> = objects_at(game, "involved_companies")
            .filter(|c| c.get("developer") == Some(&Value::Bool(true)))
            .filter_map(|c| str_at(&c["company"], "name"))
            .map(str::to_owned)
            .collect();
        let image_id = str_at(&game["cover"], "image_id");
        Some(Match {
            released: int_at(game, "first_release_date").map(Igdb::day_of),
            creator: (!developers.is_empty()).then(|| developers.join(", ")),
            companies: developers,
            description: str_at(game, "summary").unwrap_or_default().to_owned(),
            platforms: objects_at(game, "platforms")
                .filter_map(|p| str_at(p, "name"))
                .map(Igdb::platform_name)
                .collect(),
            tags: objects_at(game, "genres")
                .filter_map(|g| str_at(g, "name"))
                .map(str::to_owned)
                .collect(),
            language: Igdb::language(game),
            cover_url: image_id.map(Igdb::cover_url_for),
            ..Match::new(MediaType::Games, name, format!("igdb:{id}"))
        })
    }

    /// IGDB's search ranks ports poorly and often leaves them out, so the
    /// games named exactly `query` are asked too, the most rated first,
    /// and fill up to half of the list ahead of the search's own hits.
    fn search_games(&self, query: &str, limit: usize) -> Reply<Vec<Match>> {
        let named = Igdb::rank_named(self.games(&format!(
            "fields {SEARCH_FIELDS}; where {WHERE} & name ~ {}; sort total_rating_count desc; limit {limit};",
            Igdb::quote(query)
        ))?);
        let searched = self.games(&format!(
            "search {}; fields {SEARCH_FIELDS}; where {WHERE}; limit {limit};",
            Igdb::quote(query)
        ))?;
        Ok(Igdb::merge_hits(named, searched, limit))
    }

    /// The exact-name hits ahead of the search's, half the list at most,
    /// the rest after; one of each id.
    pub fn merge_hits(named: Vec<Value>, searched: Vec<Value>, limit: usize) -> Vec<Match> {
        let half = limit.div_ceil(2);
        let (first, later) = if named.len() > half {
            named.split_at(half)
        } else {
            (named.as_slice(), &[][..])
        };
        let mut seen: Vec<i64> = Vec::new();
        first
            .iter()
            .chain(&searched)
            .chain(later)
            .filter(|game| {
                let id = int_at(game, "id").unwrap_or_default();
                if seen.contains(&id) {
                    false
                } else {
                    seen.push(id);
                    true
                }
            })
            .filter_map(Igdb::hit)
            .take(limit)
            .collect()
    }

    /// How likely a game is the one wanted, from its platforms, lower
    /// first: a console, handheld or PC 0, a home computer 1, only arcades
    /// or minor platforms 2.
    fn platform_rank(game: &Value) -> i64 {
        let mut rank = 2;
        for p in objects_at(game, "platforms") {
            let kind = int_at(p, "platform_type");
            if MINOR_PLATFORMS.contains(&int_at(p, "id").unwrap_or_default())
                || kind == Some(ARCADE)
            {
                continue;
            }
            rank = rank.min(if kind == Some(COMPUTER) { 1 } else { 0 });
        }
        rank
    }

    /// The exact-name hits, the most rated first as IGDB sorted them, the
    /// unrated ones by `platform_rank`; a game that came out in arcades
    /// goes right after its first port, since that is what people played.
    pub fn rank_named(games: Vec<Value>) -> Vec<Value> {
        let rating = |g: &Value| int_at(g, "total_rating_count");
        let mut ranked: Vec<Value> = games
            .iter()
            .filter(|g| rating(g).is_some())
            .cloned()
            .collect();
        let mut unrated: Vec<(usize, i64, Value)> = games
            .iter()
            .filter(|g| rating(g).is_none())
            .enumerate()
            .map(|(i, g)| (i, Igdb::platform_rank(g), g.clone()))
            .collect();
        unrated.sort_by_key(|(i, rank, _)| (*rank, *i));
        ranked.extend(unrated.into_iter().map(|(_, _, g)| g));
        let order: Vec<Value> = ranked.clone();
        for game in order {
            let in_arcades =
                objects_at(&game, "platforms").any(|p| int_at(p, "platform_type") == Some(ARCADE));
            if !in_arcades {
                continue;
            }
            let id = int_at(&game, "id");
            let Some(at) = ranked.iter().position(|g| int_at(g, "id") == id) else {
                continue;
            };
            let port = ranked
                .iter()
                .position(|g| int_at(g, "parent_game") == id && id.is_some());
            if let Some(port) = port
                && port > at
            {
                let moved = ranked.remove(at);
                ranked.insert(port, moved);
            }
        }
        ranked
    }

    /// The game's first release on `platform_id` (any platform with
    /// `None`), as precise as IGDB has it: a year-only date sits on
    /// December 31, so its format says what is known.
    fn released_on(game: &Value, platform_id: Option<i64>) -> Option<Released> {
        let first = objects_at(game, "release_dates")
            .filter(|r| platform_id.is_none_or(|id| int_at(r, "platform") == Some(id)))
            .filter_map(|r| int_at(r, "date").map(|date| (date, r)))
            .min_by_key(|(date, _)| *date)?;
        let day = Igdb::day_of(first.0);
        Some(match int_at(first.1, "date_format") {
            Some(0) => day,
            Some(1) => Released { day: None, ..day },
            _ => Released {
                month: None,
                day: None,
                ..day
            },
        })
    }

    /// IGDB's platform records as the picker lists them, A to Z.
    pub fn platforms_from(rows: &[Value]) -> Vec<GamePlatform> {
        let mut platforms: Vec<GamePlatform> = rows
            .iter()
            .filter_map(|row| {
                let id = int_at(row, "id")?;
                let full = str_at(row, "name")?.trim();
                if full.is_empty() {
                    return None;
                }
                let name = text::normalize_platform(&Igdb::platform_name(full));
                let mut seen = vec![name.to_lowercase()];
                let mut aka = Vec::new();
                let others = str_at(row, "abbreviation")
                    .into_iter()
                    .chain(
                        str_at(row, "alternative_name")
                            .unwrap_or_default()
                            .split(','),
                    )
                    .chain(std::iter::once(full));
                for other in others {
                    let other = other.trim();
                    if other.is_empty() || seen.contains(&other.to_lowercase()) {
                        continue;
                    }
                    seen.push(other.to_lowercase());
                    aka.push(other.to_owned());
                }
                Some(GamePlatform { id, name, aka })
            })
            .collect();
        platforms.sort_by_key(|p| p.name.to_lowercase());
        platforms
    }

    /// The collection in a game record, as a series: the games that are
    /// main games or remakes and no edition of another, in date order,
    /// numbered from 1. A game in several collections takes the one named
    /// `name` (the entry's series), else the one its title starts with,
    /// else the one its title names, the longest first, else the first.
    pub fn collection_of(game: &Value, name: Option<&str>) -> Option<SeriesParts> {
        let title = str_at(game, "name").unwrap_or_default();
        let collection = Igdb::series_collection(game, title, name)?;
        let series_name = str_at(collection, "name")?.trim().to_owned();
        let rule = int_at(collection, "id")
            .and_then(|id| COLLECTION_RULES.iter().find(|(c, _, _)| *c == id));
        let (extra_types, skip): (&[i64], &[i64]) = rule.map_or((&[], &[]), |(_, t, s)| (t, s));
        let mut parts: Vec<(i64, String, Option<i64>, Option<String>)> =
            objects_at(collection, "games")
                .filter_map(|g| {
                    let id = int_at(g, "id")?;
                    let title = str_at(g, "name")?;
                    let no_edition = g.get("version_parent").is_none_or(Value::is_null);
                    if !no_edition
                        || !Igdb::is_part(g.get("game_type"), extra_types)
                        || skip.contains(&id)
                    {
                        return None;
                    }
                    Some((
                        id,
                        title.to_owned(),
                        int_at(g, "first_release_date"),
                        str_at(&g["cover"], "image_id").map(str::to_owned),
                    ))
                })
                .collect();
        parts.sort_by(|a, b| match (a.2, b.2) {
            (Some(x), Some(y)) => x.cmp(&y),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        });
        Some(SeriesParts {
            name: series_name,
            parts: parts
                .into_iter()
                .enumerate()
                .map(|(i, (id, title, date, image))| {
                    let released = date.map(Igdb::day_of);
                    SeriesPart {
                        number: Some(i as u32 + 1),
                        title,
                        year: released.map(|r| r.year),
                        released,
                        source: Some(format!("igdb:{id}")),
                        creator: None,
                        cover: image.as_deref().map(Igdb::cover_url_for),
                        extra: false,
                    }
                })
                .collect(),
        })
    }

    fn series_collection<'a>(
        game: &'a Value,
        title: &str,
        name: Option<&str>,
    ) -> Option<&'a Value> {
        let named: Vec<(&Value, String)> = objects_at(game, "collections")
            .filter_map(|c| {
                let n = str_at(c, "name")?.trim();
                (!n.is_empty()).then(|| (c, n.to_lowercase()))
            })
            .collect();
        if named.is_empty() {
            return None;
        }
        if let Some(wanted) = name.map(|n| n.trim().to_lowercase())
            && let Some((c, _)) = named.iter().find(|(_, n)| *n == wanted)
        {
            return Some(c);
        }
        let lower = title.trim().to_lowercase();
        let longest_first = |test: &dyn Fn(&str) -> bool| {
            named
                .iter()
                .filter(|(_, n)| test(n))
                .max_by_key(|(_, n)| n.len())
                .map(|(c, _)| *c)
        };
        longest_first(&|n| lower.starts_with(n))
            .or_else(|| longest_first(&|n| lower.contains(n)))
            .or(Some(named[0].0))
    }

    /// A game type IGDB gives as a number or an expanded object, counted
    /// as a part when it is a main game, a remake or one of `extra`; a
    /// record without one counts as a main game.
    fn is_part(kind: Option<&Value>, extra: &[i64]) -> bool {
        let id = match kind {
            None | Some(Value::Null) => return true,
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::Object(_)) => kind.and_then(|k| int_at(k, "id")),
            _ => return true,
        };
        id.is_none_or(|id| PART_TYPES.contains(&id) || extra.contains(&id))
    }

    fn collection_game(&self, source: &str) -> Reply<Option<Value>> {
        let id = Igdb::parse(source)?;
        Ok(self
            .games(&format!(
                "fields {COLLECTION_FIELDS}; where id = {id}; limit 1;"
            ))?
            .into_iter()
            .next())
    }

    fn parse(source: &str) -> Reply<i64> {
        source
            .strip_prefix("igdb:")
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| Failure::other(format!("not an IGDB id: {source}")))
    }
}

impl Source for Igdb {
    fn id(&self) -> &'static str {
        "igdb"
    }

    fn name(&self) -> &'static str {
        "IGDB"
    }

    fn types(&self) -> &'static [MediaType] {
        &[MediaType::Games]
    }

    fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }

    fn search(&self, query: &str, _media: MediaType, limit: usize) -> Reply<Hits> {
        Ok(Hits::of(self.search_games(query, limit)?))
    }

    /// One search narrowed to `platform`, with no exact-name query beside
    /// it: on one platform the search finds the ports it otherwise misses.
    fn search_on_platform(
        &self,
        query: &str,
        platform: &GamePlatform,
        limit: usize,
    ) -> Reply<Hits> {
        let games = self.games(&format!(
            "search {}; fields {FIELDS},release_dates.platform,release_dates.date,\
             release_dates.date_format; where {WHERE} & platforms = ({}); limit {limit};",
            Igdb::quote(query),
            platform.id
        ))?;
        Ok(Hits::of(
            games
                .iter()
                .filter_map(|game| {
                    let mut found = Igdb::hit(game)?;
                    if let Some(released) = Igdb::released_on(game, Some(platform.id)) {
                        found.released = Some(released);
                    }
                    found.searched_platform = Some(platform.name.clone());
                    Some(found)
                })
                .collect(),
        ))
    }

    /// IGDB's platforms, kept in the cache folder and asked again once the
    /// list is thirty days old; a stored list of any age stands in when
    /// IGDB does not answer.
    fn platforms(&self) -> Reply<Vec<GamePlatform>> {
        if let Some(platforms) = self
            .platforms
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Ok(platforms);
        }
        let stored = self.platforms_file.as_ref().and_then(JsonFile::read);
        let rows: Option<Vec<Value>> = stored
            .as_ref()
            .and_then(|json| json.get("platforms"))
            .and_then(Value::as_array)
            .map(|rows| rows.iter().filter(|r| r.is_object()).cloned().collect());
        let fetched = stored
            .as_ref()
            .and_then(|json| str_at(json, "fetched"))
            .and_then(Timestamp::parse);
        let fresh_enough = fetched.is_some_and(|at| Timestamp::now().0 - at.0 < PLATFORMS_MAX_AGE);
        let platforms = match (&rows, fresh_enough) {
            (Some(rows), true) => Igdb::platforms_from(rows),
            _ => match self.query(
                "platforms",
                "fields name,abbreviation,alternative_name; limit 500;",
            ) {
                Ok(fresh) => {
                    if let Some(file) = &self.platforms_file {
                        let _ = file.write(&json!({
                            "fetched": Timestamp::now().to_rfc3339(),
                            "platforms": fresh,
                        }));
                    }
                    Igdb::platforms_from(&fresh)
                }
                Err(failure) => match rows {
                    Some(rows) => Igdb::platforms_from(&rows),
                    None => return Err(failure),
                },
            },
        };
        *self.platforms.lock().unwrap_or_else(|e| e.into_inner()) = Some(platforms.clone());
        Ok(platforms)
    }

    /// The same name again is usually a remake or a port; the year tells
    /// them apart, and without one the first (IGDB's best) is taken.
    fn match_title(
        &self,
        title: &str,
        _media: MediaType,
        year: Option<u16>,
        _creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        let hits = self.search_games(title, 10)?;
        Ok(
            pick_by_title_and_year(&hits, title, year, |m| &m.title, Match::year)
                .map(|i| hits[i].clone()),
        )
    }

    /// Reads the game's collection, so the form gets its series and
    /// number. A port is no part of its own, so it takes the number of the
    /// game it is a port of.
    fn details(&self, found: &Match) -> Reply<Match> {
        let Some(game) = self.collection_game(&found.source)? else {
            return Ok(found.clone());
        };
        let Some(parts) = Igdb::collection_of(&game, None) else {
            return Ok(found.clone());
        };
        let parent = int_at(&game, "parent_game").map(|id| format!("igdb:{id}"));
        let number = parts
            .parts
            .iter()
            .find(|p| p.source.as_deref() == Some(found.source.as_str()))
            .or_else(|| {
                parts
                    .parts
                    .iter()
                    .find(|p| p.source.is_some() && p.source == parent)
            })
            .and_then(|p| p.number);
        let mut full = found.clone();
        full.series = Some(Series {
            name: parts.name,
            number,
            cover: None,
        });
        Ok(full)
    }

    fn series_parts(&self, source: &str, name: Option<&str>) -> Reply<Option<SeriesParts>> {
        Ok(self
            .collection_game(source)?
            .and_then(|game| Igdb::collection_of(&game, name)))
    }

    fn cover_url(&self, source: &str, _media: MediaType) -> Reply<Option<String>> {
        let id = Igdb::parse(source)?;
        let games = self.games(&format!("fields cover.image_id; where id = {id}; limit 1;"))?;
        Ok(games
            .first()
            .and_then(|game| str_at(&game["cover"], "image_id"))
            .map(Igdb::cover_url_for))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: i64, name: &str, extra: &str) -> Value {
        serde_json::from_str(&format!(r#"{{"id": {id}, "name": "{name}"{extra}}}"#)).unwrap()
    }

    #[test]
    fn a_game_record_becomes_a_match() {
        let found = Igdb::hit(&game(
            1098,
            "Street Fighter II",
            r#", "first_release_date": 708134400, "summary": "Eight warriors.",
            "cover": {"image_id": "co1"},
            "platforms": [{"name": "Super Nintendo Entertainment System"}, {"name": "PC (Microsoft Windows)"}],
            "involved_companies": [{"developer": true, "company": {"name": "Capcom"}}],
            "genres": [{"name": "Fighting"}],
            "language_supports": [{"language_support_type": 1, "language": {"locale": "ja-JP"}}]"#,
        ))
        .unwrap();
        assert_eq!(found.source, "igdb:1098");
        assert_eq!(found.released.map(|r| r.year), Some(1992));
        assert_eq!(found.creator.as_deref(), Some("Capcom"));
        assert_eq!(
            found.platforms,
            ["Super Nintendo Entertainment System", "PC"]
        );
        assert_eq!(found.language.as_deref(), Some("ja-JP"));
        assert_eq!(
            found.cover_url.as_deref(),
            Some("https://images.igdb.com/igdb/image/upload/t_cover_big/co1.jpg")
        );
        assert_eq!(Igdb::quote(r#"Say "hi""#), r#""Say \"hi\"""#);
    }

    #[test]
    fn exact_name_hits_come_first_with_arcades_after_their_port() {
        let arcade = game(
            1,
            "SF",
            r#", "platforms": [{"id": 52, "platform_type": 2}], "first_release_date": 1"#,
        );
        let snes = game(
            2,
            "SF",
            r#", "platforms": [{"id": 19, "platform_type": 1}], "parent_game": 1"#,
        );
        let pc = game(3, "SF", r#", "platforms": [{"id": 6, "platform_type": 6}]"#);
        let rated = game(4, "SF", r#", "total_rating_count": 50"#);
        let ranked = Igdb::rank_named(vec![arcade, pc, snes, rated]);
        let ids: Vec<i64> = ranked.iter().map(|g| int_at(g, "id").unwrap()).collect();
        assert_eq!(
            ids,
            [4, 2, 3, 1],
            "rated first, then by platform; the cabinet already follows its port"
        );

        let searched = vec![game(9, "SF Alpha", ""), game(4, "SF", "")];
        let merged = Igdb::merge_hits(ranked, searched, 4);
        let sources: Vec<&str> = merged.iter().map(|m| m.source.as_str()).collect();
        assert_eq!(sources, ["igdb:4", "igdb:2", "igdb:9", "igdb:3"]);
    }

    #[test]
    fn collections_pick_the_right_one_and_keep_main_games() {
        let record: Value = serde_json::from_str(
            r#"{"id": 2, "name": "Donkey Kong Country", "collections": [
                {"id": 7, "name": "Donkey Kong", "games": [{"id": 1, "name": "Donkey Kong", "first_release_date": 1}]},
                {"id": 8, "name": "Donkey Kong Country", "games": [
                    {"id": 2, "name": "Donkey Kong Country", "first_release_date": 100, "game_type": 0, "cover": {"image_id": "dk"}},
                    {"id": 3, "name": "DKC 2", "first_release_date": 200, "game_type": {"id": 0}},
                    {"id": 4, "name": "DKC Remaster", "first_release_date": 300, "game_type": 9},
                    {"id": 5, "name": "DKC Port", "first_release_date": 400, "version_parent": 2}]}]}"#,
        )
        .unwrap();
        let parts = Igdb::collection_of(&record, None).unwrap();
        assert_eq!(
            parts.name, "Donkey Kong Country",
            "the title starts with it"
        );
        assert_eq!(parts.parts.len(), 2);
        assert_eq!(parts.parts[1].number, Some(2));
        assert!(parts.parts[0].cover.as_deref().unwrap().contains("dk"));
        let other = Igdb::collection_of(&record, Some("Donkey Kong")).unwrap();
        assert_eq!(other.name, "Donkey Kong", "the entry's series wins");
        assert!(Igdb::is_part(None, &[]));
        assert!(!Igdb::is_part(Some(&Value::from(9)), &[]));
        assert!(Igdb::is_part(Some(&Value::from(9)), &[9]));
    }

    #[test]
    fn platforms_are_named_as_the_app_spells_them_and_dates_keep_their_precision() {
        let rows: Vec<Value> = serde_json::from_str(
            r#"[{"id": 19, "name": "Super Nintendo Entertainment System", "abbreviation": "SNES", "alternative_name": "Super Famicom, SNES"},
                {"id": 6, "name": "PC (Microsoft Windows)", "abbreviation": "PC"}, {"id": 1, "name": " "}]"#,
        )
        .unwrap();
        let platforms = Igdb::platforms_from(&rows);
        assert_eq!(platforms.len(), 2);
        assert_eq!(platforms[0].name, "PC");
        assert_eq!(
            platforms[0].aka,
            Vec::<String>::from(["PC (Microsoft Windows)".to_owned()])
        );
        assert_eq!(platforms[1].name, "Super Nintendo");
        assert_eq!(
            platforms[1].aka,
            [
                "SNES",
                "Super Famicom",
                "Super Nintendo Entertainment System"
            ]
        );
        assert!(platforms[1].matches("famicom"));

        let game: Value = serde_json::from_str(
            r#"{"release_dates": [{"platform": 19, "date": 740000000, "date_format": 2},
                {"platform": 6, "date": 700000000, "date_format": 0}]}"#,
        )
        .unwrap();
        assert_eq!(
            Igdb::released_on(&game, Some(19)).unwrap().to_string(),
            "1993"
        );
        assert_eq!(
            Igdb::released_on(&game, None).unwrap().to_string(),
            "1992-03-07"
        );
        assert_eq!(Igdb::released_on(&game, Some(99)), None);
    }

    #[test]
    fn without_keys_nothing_is_asked() {
        let igdb = Igdb::new("", "", None);
        assert!(!igdb.is_configured());
        assert_eq!(
            igdb.search("x", MediaType::Games, 5).unwrap_err().kind,
            FailureKind::NotConfigured
        );
        assert_eq!(Igdb::parse("igdb:12").unwrap(), 12);
        assert!(Igdb::parse("tmdb:movie:1").is_err());
    }
}
