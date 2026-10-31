//! Anime from AniList's GraphQL API (`anilist_source.dart`). No key. Ids
//! are "anilist:21". The search carries everything the form needs for an
//! anime movie; an anime series (AniList's TV, TV short, ONA and OVA
//! formats) also gets its seasons with `details`, the same way a show
//! does: the series' entries are walked back along AniList's prequel
//! relations to the first one, and that one is looked for on TMDB by
//! title and year. Found, TMDB's seasons and episodes are used, and the
//! link is kept in `anime_links.json` so it is asked for once. With no
//! TMDB match, the seasons are built from AniList's sequel chain, one per
//! entry, and the episodes are "Episode N" with the air dates of AniList's
//! airing schedule. AniList allows about 30 requests a minute.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use super::http::{Http, int_at, objects_at, str_at};
use super::store::JsonFile;
use super::text::{language_of_country, pick_by_title_and_year, strip_html};
use super::tmdb::Tmdb;
use super::{Episode, Failure, Hits, Match, Reply, ShowSeasons, Source};
use crate::clock::Timestamp;
use crate::model::{Date, EntryKind, MediaType, Released, Season};

const API: &str = "https://graphql.anilist.co";
/// How long a "no TMDB match" is believed before TMDB is asked again.
const RETRY_AFTER: i64 = 14 * 86_400;
/// How far a prequel or sequel chain is followed, each way.
const MAX_HOPS: usize = 60;
/// Episodes per airing-schedule page, and how many pages are read at most.
const SCHEDULE_PAGE: usize = 50;
const SCHEDULE_PAGES: usize = 10;

const FIELDS: &str = "id format title { romaji english } startDate { year month day } \
    description(asHtml: false) coverImage { large extraLarge } \
    studios(isMain: true) { nodes { name } } countryOfOrigin genres";
const NODE: &str = "id format status title { romaji english } startDate { year month day } \
    endDate { year month day } episodes duration";

fn search_query() -> String {
    format!(
        "query ($search: String, $perPage: Int) {{ Page(perPage: $perPage) {{ \
         media(search: $search, type: ANIME, sort: SEARCH_MATCH) {{ {FIELDS} }} }} }}"
    )
}

fn by_id_query() -> String {
    format!("query ($id: Int) {{ Media(id: $id, type: ANIME) {{ {FIELDS} }} }}")
}

/// An entry with its relations two deep, so a short chain costs one
/// request and a long one a request every two hops.
fn chain_query() -> String {
    format!(
        "query ($id: Int) {{ Media(id: $id, type: ANIME) {{ {NODE} relations {{ edges {{ \
         relationType node {{ {NODE} relations {{ edges {{ relationType node {{ {NODE} }} }} }} \
         }} }} }} }} }}"
    )
}

const SCHEDULE_QUERY: &str = "query ($id: Int, $page: Int, $perPage: Int) { \
    Media(id: $id, type: ANIME) { airingSchedule(page: $page, perPage: $perPage) { \
    pageInfo { hasNextPage } nodes { episode airingAt } } } }";

/// One AniList entry in a prequel and sequel chain. `prequels` and
/// `sequels` are `None` until the entry was fetched with its relations.
#[derive(Debug, Clone, PartialEq)]
pub struct AniNode {
    pub id: i64,
    pub format: Option<String>,
    pub status: Option<String>,
    pub english: Option<String>,
    pub romaji: Option<String>,
    pub start: Option<Released>,
    pub episodes: Option<u32>,
    pub duration: Option<u32>,
    pub prequels: Option<Vec<i64>>,
    pub sequels: Option<Vec<i64>>,
}

impl AniNode {
    fn loaded(&self) -> bool {
        self.prequels.is_some()
    }

    pub fn is_series(&self) -> bool {
        AniList::kind_of(self.format.as_deref()) == Some(EntryKind::Series)
    }

    /// A recap entry, by its title; left out of the seasons.
    pub fn is_recap(&self) -> bool {
        let text = format!(
            "{} {}",
            self.english.as_deref().unwrap_or_default(),
            self.romaji.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        text.split(|c: char| !c.is_alphanumeric())
            .any(|w| w == "recap")
    }
}

/// What Libri learned about an anime: its kind, the TMDB show its seasons
/// come from when one was found, and when it was last checked.
#[derive(Debug, Clone, PartialEq)]
struct Link {
    tmdb: Option<String>,
    kind: Option<EntryKind>,
    checked: Timestamp,
}

pub struct AniList {
    http: Http,
    tmdb: Option<Arc<Tmdb>>,
    links_file: Option<JsonFile>,
    links: Mutex<HashMap<i64, Link>>,
    /// The chains walked this session, by any entry in them.
    chains: Mutex<HashMap<i64, Arc<Vec<AniNode>>>>,
}

impl AniList {
    pub fn new(tmdb: Option<Arc<Tmdb>>, cache: Option<&std::path::Path>) -> AniList {
        AniList {
            http: Http::new(Duration::from_secs(2)),
            tmdb,
            links_file: cache.map(|dir| JsonFile(dir.join("anime_links.json"))),
            links: Mutex::new(HashMap::new()),
            chains: Mutex::new(HashMap::new()),
        }
    }

    /// An AniList format as a kind: TV, TV short, ONA and OVA are a series
    /// (seasons, like a show); a movie, a special or a music video is a
    /// movie. `None` for no format, or one that is neither.
    pub fn kind_of(format: Option<&str>) -> Option<EntryKind> {
        match format? {
            "TV" | "TV_SHORT" | "ONA" | "OVA" => Some(EntryKind::Series),
            "MOVIE" | "SPECIAL" | "MUSIC" => Some(EntryKind::Movie),
            _ => None,
        }
    }

    fn query(&self, query: &str, variables: Value) -> Reply<Value> {
        let body = json!({"query": query, "variables": variables}).to_string();
        let response = self.http.post(
            API,
            &[
                ("Content-Type", "application/json"),
                ("Accept", "application/json"),
            ],
            &body,
        )?;
        // AniList answers 404 for an unknown id with an errors array.
        self.http.check(&response, &[200, 404])?;
        let json = response.object()?;
        match json.get("data").filter(|d| d.is_object()) {
            Some(data) => Ok(data.clone()),
            None => Err(Failure {
                kind: super::FailureKind::Other,
                status: Some(response.status),
                message: objects_at(&json, "errors")
                    .next()
                    .and_then(|e| str_at(e, "message"))
                    .unwrap_or("no data")
                    .to_owned(),
            }),
        }
    }

    fn date(value: &Value) -> Option<Released> {
        let year = u16::try_from(int_at(value, "year")?).ok()?;
        Some(Released {
            year,
            month: int_at(value, "month").and_then(|m| u8::try_from(m).ok()),
            day: int_at(value, "day").and_then(|d| u8::try_from(d).ok()),
        })
    }

    fn hit(media: &Value) -> Option<Match> {
        let id = int_at(media, "id")?;
        let titles = &media["title"];
        let title = str_at(titles, "english").or_else(|| str_at(titles, "romaji"))?;
        let studios: Vec<String> = objects_at(&media["studios"], "nodes")
            .filter_map(|s| str_at(s, "name"))
            .map(str::to_owned)
            .collect();
        let cover = &media["coverImage"];
        Some(Match {
            released: AniList::date(&media["startDate"]),
            creator: (!studios.is_empty()).then(|| studios.join(", ")),
            companies: studios,
            description: strip_html(str_at(media, "description").unwrap_or_default()),
            tags: super::http::strings_at(media, "genres")
                .map(str::to_owned)
                .collect(),
            kind: AniList::kind_of(str_at(media, "format")),
            language: language_of_country(str_at(media, "countryOfOrigin")).map(str::to_owned),
            cover_url: str_at(cover, "large").map(str::to_owned),
            large_cover_url: str_at(cover, "extraLarge").map(str::to_owned),
            ..Match::new(MediaType::Anime, title, format!("anilist:{id}"))
        })
    }

    fn search_hits(&self, query: &str, limit: usize) -> Reply<Vec<Match>> {
        let data = self.query(&search_query(), json!({"search": query, "perPage": limit}))?;
        Ok(objects_at(&data["Page"], "media")
            .filter_map(AniList::hit)
            .collect())
    }

    fn parse(source: &str) -> Reply<i64> {
        source
            .strip_prefix("anilist:")
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| Failure::other(format!("not an AniList id: {source}")))
    }

    // Seasons.

    /// Parses an entry and, when its relations came with it, the entries
    /// they point at, into `nodes`. An entry seen with its relations is
    /// kept over the same one seen without.
    pub fn parse_node(media: &Value, nodes: &mut HashMap<i64, AniNode>) -> Option<AniNode> {
        let id = int_at(media, "id")?;
        let titles = &media["title"];
        let (mut prequels, mut sequels) = (None, None);
        if let Some(relations) = media.get("relations").filter(|r| r.is_object()) {
            let (mut p, mut s) = (Vec::new(), Vec::new());
            for edge in objects_at(relations, "edges") {
                let Some(related) = edge.get("node").filter(|n| n.is_object()) else {
                    continue;
                };
                let Some(related) = AniList::parse_node(related, nodes) else {
                    continue;
                };
                match str_at(edge, "relationType") {
                    Some("PREQUEL") => p.push(related.id),
                    Some("SEQUEL") => s.push(related.id),
                    _ => {}
                }
            }
            prequels = Some(p);
            sequels = Some(s);
        }
        let parsed = AniNode {
            id,
            format: str_at(media, "format").map(str::to_owned),
            status: str_at(media, "status").map(str::to_owned),
            english: str_at(titles, "english").map(str::to_owned),
            romaji: str_at(titles, "romaji").map(str::to_owned),
            start: AniList::date(&media["startDate"]),
            episodes: int_at(media, "episodes").and_then(|e| u32::try_from(e).ok()),
            duration: int_at(media, "duration").and_then(|d| u32::try_from(d).ok()),
            prequels,
            sequels,
        };
        let keep = nodes
            .get(&id)
            .is_none_or(|known| !known.loaded() || parsed.loaded());
        if keep {
            nodes.insert(id, parsed);
        }
        nodes.get(&id).cloned()
    }

    /// Among related entries, the first that is a series (a TV sequel over
    /// a recap movie), else the first of them.
    fn pick(ids: &[i64], nodes: &HashMap<i64, AniNode>) -> Option<i64> {
        ids.iter()
            .copied()
            .find(|id| nodes.get(id).is_some_and(AniNode::is_series))
            .or_else(|| ids.first().copied())
    }

    /// The chain of entries the anime `id` is part of, first to last,
    /// walked back along the prequels and forward along the sequels. A
    /// movie, a special or anything else that is not a series is its own
    /// chain.
    fn chain(&self, id: i64) -> Reply<Arc<Vec<AniNode>>> {
        if let Some(chain) = self
            .chains
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
        {
            return Ok(chain.clone());
        }
        let mut nodes: HashMap<i64, AniNode> = HashMap::new();
        let load = |id: i64, nodes: &mut HashMap<i64, AniNode>| -> Reply<AniNode> {
            if let Some(known) = nodes.get(&id).filter(|n| n.loaded()) {
                return Ok(known.clone());
            }
            let data = self.query(&chain_query(), json!({"id": id}))?;
            let media = &data["Media"];
            if !media.is_object() || int_at(media, "id").is_none() {
                return Err(Failure::other(format!("no anime {id}")));
            }
            AniList::parse_node(media, nodes).ok_or_else(|| Failure::other("no anime"))
        };
        let mut current = load(id, &mut nodes)?;
        let chain = if current.is_series() {
            let mut behind = vec![id];
            for _ in 0..MAX_HOPS {
                let Some(previous) =
                    AniList::pick(current.prequels.as_deref().unwrap_or_default(), &nodes)
                else {
                    break;
                };
                if behind.contains(&previous) {
                    break;
                }
                behind.push(previous);
                current = load(previous, &mut nodes)?;
            }
            let mut chain = vec![current.clone()];
            let mut ahead = vec![current.id];
            for _ in 0..MAX_HOPS {
                let Some(next) =
                    AniList::pick(current.sequels.as_deref().unwrap_or_default(), &nodes)
                else {
                    break;
                };
                if ahead.contains(&next) {
                    break;
                }
                ahead.push(next);
                current = load(next, &mut nodes)?;
                chain.push(current.clone());
            }
            chain
        } else {
            vec![current]
        };
        let chain = Arc::new(chain);
        let mut chains = self.chains.lock().unwrap_or_else(|e| e.into_inner());
        for node in chain.iter() {
            chains.insert(node.id, chain.clone());
        }
        chains.insert(id, chain.clone());
        Ok(chain)
    }

    /// The entries of a chain that are seasons: series, recaps left out.
    pub fn season_entries(chain: &[AniNode]) -> Vec<&AniNode> {
        chain
            .iter()
            .filter(|n| n.is_series() && !n.is_recap())
            .collect()
    }

    /// One season per entry, dated by the entry's start.
    pub fn chain_seasons(chain: &[AniNode]) -> Vec<Season> {
        AniList::season_entries(chain)
            .into_iter()
            .enumerate()
            .map(|(i, node)| Season {
                season: i as u32 + 1,
                released: node.start,
                status: None,
                fresh: None,
                manual: None,
            })
            .collect()
    }

    fn tmdb_ready(&self) -> bool {
        self.tmdb.as_ref().is_some_and(|t| t.is_configured())
    }

    fn read_link(&self, id: i64) -> Option<Link> {
        if let Some(link) = self
            .links
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
        {
            return Some(link.clone());
        }
        let json = self.links_file.as_ref()?.read()?;
        let link = json.get(id.to_string())?;
        let checked = Timestamp::parse(str_at(link, "checked")?)?;
        Some(Link {
            tmdb: str_at(link, "tmdb").map(str::to_owned),
            kind: match str_at(link, "kind") {
                Some("series") => Some(EntryKind::Series),
                Some("movie") => Some(EntryKind::Movie),
                _ => None,
            },
            checked,
        })
    }

    fn write_link(&self, id: i64, link: Link) {
        if let Some(file) = &self.links_file {
            let mut json = file
                .read()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({}));
            json[id.to_string()] = json!({
                "tmdb": link.tmdb,
                "kind": link.kind.map(|k| match k {
                    EntryKind::Series => "series",
                    EntryKind::Movie => "movie",
                    EntryKind::Album => "album",
                    EntryKind::Song => "song",
                }),
                "checked": link.checked.to_rfc3339(),
            });
            let _ = file.write(&json);
        }
        self.links
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, link);
    }

    /// The anime's kind and the TMDB show its chain was matched to, from
    /// the store when it was answered before (a match for good, a miss for
    /// two weeks), else from AniList and TMDB now. Nothing is stored when
    /// TMDB gave no answer, so it is asked again.
    fn link(&self, id: i64) -> Reply<Link> {
        let now = Timestamp::now();
        if let Some(stored) = self.read_link(id)
            && (stored.tmdb.is_some()
                || stored.kind == Some(EntryKind::Movie)
                || now.0 - stored.checked.0 < RETRY_AFTER)
        {
            return Ok(stored);
        }
        let chain = self.chain(id)?;
        let entry = chain.iter().find(|n| n.id == id).or(chain.first());
        let kind = AniList::kind_of(entry.and_then(|n| n.format.as_deref()));
        let mut linked = None;
        if kind == Some(EntryKind::Series)
            && let (Some(tmdb), Some(root)) = (&self.tmdb, chain.first())
            && tmdb.is_configured()
        {
            for title in [root.english.as_deref(), root.romaji.as_deref()]
                .into_iter()
                .flatten()
            {
                if let Some(hit) =
                    tmdb.match_title(title, MediaType::Shows, root.start.map(|r| r.year), None)?
                {
                    linked = Some(hit.source);
                    break;
                }
            }
        }
        let link = Link {
            tmdb: linked,
            kind,
            checked: now,
        };
        self.write_link(id, link.clone());
        Ok(link)
    }

    /// The episodes of the `season`th entry of the chain: "Episode N" (no
    /// titles) with the air dates of AniList's airing schedule, and the
    /// entry's episode length. A finished entry's episodes without a
    /// scheduled date are all dated the entry's start.
    pub fn chain_episodes(node: &AniNode, schedule: &HashMap<u32, Date>) -> Vec<Episode> {
        let count = node
            .episodes
            .unwrap_or_else(|| schedule.keys().copied().max().unwrap_or(0));
        let finished = (node.status.as_deref() == Some("FINISHED"))
            .then(|| node.start.map(|r| r.available_from()))
            .flatten();
        (1..=count)
            .map(|n| Episode {
                number: n,
                title: None,
                aired: schedule.get(&n).copied().or(finished),
                runtime: node.duration,
            })
            .collect()
    }
}

impl Source for AniList {
    fn id(&self) -> &'static str {
        "anilist"
    }

    fn name(&self) -> &'static str {
        "AniList"
    }

    fn types(&self) -> &'static [MediaType] {
        &[MediaType::Anime]
    }

    fn search(&self, query: &str, _media: MediaType, limit: usize) -> Reply<Hits> {
        Ok(Hits::of(self.search_hits(query, limit)?))
    }

    fn match_title(
        &self,
        title: &str,
        _media: MediaType,
        year: Option<u16>,
        _creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        let hits = self.search_hits(title, 5)?;
        Ok(
            pick_by_title_and_year(&hits, title, year, |m| &m.title, Match::year)
                .map(|i| hits[i].clone()),
        )
    }

    /// A series gets its seasons, as a show does from TMDB; a movie is
    /// complete from the search.
    fn details(&self, found: &Match) -> Reply<Match> {
        if found.kind != Some(EntryKind::Series) {
            return Ok(found.clone());
        }
        let mut full = found.clone();
        full.seasons = self
            .seasons(&found.source)?
            .map(|s| s.seasons)
            .unwrap_or_default();
        Ok(full)
    }

    /// A series' seasons: TMDB's when the series has a match there, else
    /// one per entry of its AniList chain. A movie has none.
    fn seasons(&self, source: &str) -> Reply<Option<ShowSeasons>> {
        let id = AniList::parse(source)?;
        let link = self.link(id)?;
        if link.kind == Some(EntryKind::Movie) {
            return Ok(Some(ShowSeasons {
                seasons: Vec::new(),
                canceled: false,
                kind: link.kind,
            }));
        }
        if let (Some(linked), Some(tmdb)) = (&link.tmdb, &self.tmdb)
            && tmdb.is_configured()
            && let Some(found) = tmdb.seasons(linked)?
        {
            return Ok(Some(ShowSeasons {
                seasons: found.seasons,
                canceled: found.canceled,
                kind: link.kind,
            }));
        }
        let chain = self.chain(id)?;
        Ok(Some(ShowSeasons {
            seasons: AniList::chain_seasons(&chain),
            canceled: chain
                .last()
                .is_some_and(|n| n.status.as_deref() == Some("CANCELLED")),
            kind: link.kind,
        }))
    }

    /// The TMDB show the anime's episodes come from, when it has one; else
    /// the anime itself, whose episodes come from its chain.
    fn season_source(&self, source: &str) -> Reply<String> {
        let link = self.link(AniList::parse(source)?)?;
        Ok(match link.tmdb {
            Some(linked) if self.tmdb_ready() => linked,
            _ => source.to_owned(),
        })
    }

    fn season_episodes(&self, source: &str, season: u32) -> Reply<Option<Vec<Episode>>> {
        let chain = self.chain(AniList::parse(source)?)?;
        let entries = AniList::season_entries(&chain);
        let Some(node) = usize::try_from(season)
            .ok()
            .filter(|s| *s >= 1)
            .and_then(|s| entries.get(s - 1))
        else {
            return Ok(Some(Vec::new()));
        };
        let mut schedule: HashMap<u32, Date> = HashMap::new();
        for page in 1..=SCHEDULE_PAGES {
            let data = self.query(
                SCHEDULE_QUERY,
                json!({"id": node.id, "page": page, "perPage": SCHEDULE_PAGE}),
            )?;
            let airing = &data["Media"]["airingSchedule"];
            for item in objects_at(airing, "nodes") {
                if let (Some(episode), Some(at)) =
                    (int_at(item, "episode"), int_at(item, "airingAt"))
                    && let Ok(number) = u32::try_from(episode)
                {
                    schedule.insert(number, Date::from_days(at.div_euclid(86_400)));
                }
            }
            if airing["pageInfo"]["hasNextPage"] != Value::Bool(true) {
                break;
            }
        }
        Ok(Some(AniList::chain_episodes(node, &schedule)))
    }

    fn cover_url(&self, source: &str, _media: MediaType) -> Reply<Option<String>> {
        let data = self.query(&by_id_query(), json!({"id": AniList::parse(source)?}))?;
        let cover = &data["Media"]["coverImage"];
        Ok(str_at(cover, "extraLarge")
            .or_else(|| str_at(cover, "large"))
            .map(str::to_owned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_media_record_becomes_a_match_with_its_kind_and_language() {
        let media: Value = serde_json::from_str(
            r#"{"id": 457, "format": "TV", "title": {"romaji": "Mushishi", "english": null},
                "startDate": {"year": 2005, "month": 10, "day": 23}, "description": "Ginko<br>wanders.",
                "coverImage": {"large": "l.jpg", "extraLarge": "xl.jpg"},
                "studios": {"nodes": [{"name": "Artland"}]}, "countryOfOrigin": "JP", "genres": ["Fantasy"]}"#,
        )
        .unwrap();
        let found = AniList::hit(&media).unwrap();
        assert_eq!(found.source, "anilist:457");
        assert_eq!(
            found.title, "Mushishi",
            "romaji when there is no English title"
        );
        assert_eq!(found.kind, Some(EntryKind::Series));
        assert_eq!(found.description, "Ginko\nwanders.");
        assert_eq!(found.language.as_deref(), Some("ja"));
        assert_eq!(found.released.unwrap().to_string(), "2005-10-23");
        assert_eq!(AniList::kind_of(Some("MOVIE")), Some(EntryKind::Movie));
        assert_eq!(AniList::kind_of(Some("MANGA")), None);
    }

    #[test]
    fn a_chain_is_parsed_two_deep_and_gives_seasons_and_episodes() {
        let media: Value = serde_json::from_str(
            r#"{"id": 2, "format": "TV", "status": "FINISHED", "title": {"romaji": "Two"},
                "startDate": {"year": 2024, "month": 1, "day": 5}, "episodes": 2, "duration": 24,
                "relations": {"edges": [
                    {"relationType": "PREQUEL", "node": {"id": 1, "format": "TV", "title": {"romaji": "One"}, "startDate": {"year": 2023}, "relations": {"edges": []}}},
                    {"relationType": "SEQUEL", "node": {"id": 3, "format": "MOVIE", "title": {"romaji": "Recap Movie"}, "startDate": {"year": 2025}}}]}}"#,
        )
        .unwrap();
        let mut nodes = HashMap::new();
        let node = AniList::parse_node(&media, &mut nodes).unwrap();
        assert_eq!(node.prequels, Some(vec![1]));
        assert_eq!(node.sequels, Some(vec![3]));
        assert!(
            nodes[&1].loaded(),
            "a relation with its own relations counts as loaded"
        );
        assert!(!nodes[&3].loaded());
        assert!(nodes[&3].is_recap());

        let chain = vec![nodes[&1].clone(), node.clone(), nodes[&3].clone()];
        let seasons = AniList::chain_seasons(&chain);
        assert_eq!(seasons.len(), 2, "the movie is no season");
        assert_eq!(seasons[1].released.unwrap().to_string(), "2024-01-05");

        let mut schedule = HashMap::new();
        schedule.insert(2, Date::parse("2024-01-12").unwrap());
        let episodes = AniList::chain_episodes(&node, &schedule);
        assert_eq!(episodes.len(), 2);
        assert_eq!(
            episodes[0].aired,
            Date::parse("2024-01-05"),
            "a finished entry dates the rest at its start"
        );
        assert_eq!(episodes[1].aired, Date::parse("2024-01-12"));
        assert_eq!(episodes[1].runtime, Some(24));
        assert_eq!(
            AniList::pick(&[3, 1], &nodes),
            Some(1),
            "a series over a movie"
        );
    }
}
