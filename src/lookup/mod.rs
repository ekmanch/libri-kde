//! Online lookups: the providers the phone uses (its DESIGN.md sections
//! 10 and 10c), asked from the Add and Edit dialog, for covers and artist
//! photos, track lists, a season's episodes and a series' parts. Every
//! provider is a [`Source`]; the [`Service`] asks the right ones for a
//! type and merges their answers. The sources block (`ureq`), so the app
//! runs them on tokio's blocking pool from a Task, as it does GitHub.
//! No provider is named anywhere in the UI but Settings › About.

mod anilist;
pub mod books;
mod deezer;
pub mod fake;
pub mod http;
mod igdb;
mod keys;
pub mod store;
pub mod text;
mod tmdb;
pub mod wikidata;

use std::sync::{Arc, Mutex};

pub use deezer::NO_PHOTO;
pub use keys::Keys;

use crate::model::{EntryKind, MediaType, Released, Season, Series};

/// One candidate from a title lookup, or the match a cover was resolved
/// from.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub media: MediaType,
    pub title: String,
    /// The lookup id, as stored on the entry: "tmdb:movie:603",
    /// "deezer:album:302127", "openlibrary:OL45804W".
    pub source: String,
    pub released: Option<Released>,
    /// A book dated by its Apple Books ebook alone, so the year may be a
    /// reprint's. The add form says so under the date; never stored.
    pub ebook_dated: bool,
    pub creator: Option<String>,
    pub description: String,
    /// A song's album and that album's lookup id.
    pub album: Option<String>,
    pub album_source: Option<String>,
    /// A game's platforms, as the lookup lists them.
    pub platforms: Vec<String>,
    /// The companies behind it: production companies, studios,
    /// developers. The form reads the studio's tag from them (#pixar).
    pub companies: Vec<String>,
    /// The top billed cast of a movie or show, in billing order.
    pub cast: Vec<String>,
    /// Tag names the lookup suggests (genres, themes), as it spells them.
    pub tags: Vec<String>,
    /// A show's seasons with their dates, specials left out.
    pub seasons: Vec<Season>,
    pub pages: Option<u32>,
    /// An anime's kind (series or movie), or a music match's (album or
    /// song).
    pub kind: Option<EntryKind>,
    pub series: Option<Series>,
    /// The work's language as a code ("ja", "jpn", "ja-JP"), when the
    /// lookup says.
    pub language: Option<String>,
    /// A show's first network as TMDB names it ("Netflix").
    pub network: Option<String>,
    /// A cover sized for a row, and the same image at the size the cache
    /// keeps.
    pub cover_url: Option<String>,
    pub large_cover_url: Option<String>,
    /// The id the cover is found by when it is not `source`: an Open
    /// Library edition's, for the edition's cover a work row shows.
    pub cover_source: Option<String>,
    /// A book's edition, for the cover picker: "Hardcover", "Ebook".
    pub edition: Option<String>,
    pub edition_year: Option<u16>,
    /// A game found in a search on one platform: that platform's name,
    /// which the form picks. Never stored.
    pub searched_platform: Option<String>,
}

impl Match {
    /// A match with nothing but its type, title and id.
    pub fn new(media: MediaType, title: impl Into<String>, source: impl Into<String>) -> Match {
        Match {
            media,
            title: title.into(),
            source: source.into(),
            released: None,
            ebook_dated: false,
            creator: None,
            description: String::new(),
            album: None,
            album_source: None,
            platforms: Vec::new(),
            companies: Vec::new(),
            cast: Vec::new(),
            tags: Vec::new(),
            seasons: Vec::new(),
            pages: None,
            kind: None,
            series: None,
            language: None,
            network: None,
            cover_url: None,
            large_cover_url: None,
            cover_source: None,
            edition: None,
            edition_year: None,
            searched_platform: None,
        }
    }

    /// The provider part of a lookup id: "tmdb" for "tmdb:movie:603".
    pub fn provider_of(source: &str) -> &str {
        source.split(':').next().unwrap_or_default()
    }

    /// The image the cache keeps: the large cover, else the row's.
    pub fn cache_cover_url(&self) -> Option<&str> {
        self.large_cover_url
            .as_deref()
            .or(self.cover_url.as_deref())
    }

    /// The id the cover is found by: `cover_source`, else `source`.
    pub fn cover_from(&self) -> &str {
        self.cover_source.as_deref().unwrap_or(&self.source)
    }

    pub fn year(&self) -> Option<u16> {
        self.released.map(|r| r.year)
    }

    /// "2019", or "".
    pub fn year_text(&self) -> String {
        self.year().map(|y| y.to_string()).unwrap_or_default()
    }

    /// "2004 · Special edition", as Pick a cover captions a choice.
    pub fn year_and_edition(&self) -> String {
        let year = self.year_text();
        match &self.edition {
            Some(edition) if !year.is_empty() => format!("{year} · {edition}"),
            Some(edition) => edition.clone(),
            None => year,
        }
    }
}

/// Why a lookup gave no answer. The screen words each one; no provider
/// name reaches the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// No network, or the provider never answered (a timeout too).
    Offline,
    /// The provider needs a key this build was made without.
    NotConfigured,
    /// The provider refused for now (429, or its own quota answer).
    RateLimited,
    /// Anything else: an error status, a malformed answer.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub status: Option<u16>,
    pub message: String,
}

impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Failure {
        Failure {
            kind,
            status: None,
            message: message.into(),
        }
    }

    pub fn other(message: impl Into<String>) -> Failure {
        Failure::new(FailureKind::Other, message)
    }

    /// The provider gave no verdict for now: offline, rate limited, not
    /// set up, or its own server error (5xx). Asking again later may well
    /// answer, so nothing is concluded from it.
    pub fn is_transient(&self) -> bool {
        self.kind != FailureKind::Other || self.status.is_some_and(|s| s >= 500)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)?;
        if let Some(status) = self.status {
            write!(f, " {status}")?;
        }
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

pub type Reply<T> = Result<T, Failure>;

/// A search's answer: the matches, and the failure of a second provider
/// behind the same source that did not answer (books ask two), so the
/// screen can say some lookups were not reached rather than "No matches".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hits {
    pub matches: Vec<Match>,
    pub failure: Option<Failure>,
}

impl Hits {
    pub fn of(matches: Vec<Match>) -> Hits {
        Hits {
            matches,
            failure: None,
        }
    }
}

/// A show's seasons as the lookup lists them, whether it is canceled, and
/// for an anime its kind.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowSeasons {
    pub seasons: Vec<Season>,
    pub canceled: bool,
    pub kind: Option<EntryKind>,
}

/// One part of a series as the lookup lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesPart {
    /// Its number in the series; `None` when the source gives none.
    pub number: Option<u32>,
    pub title: String,
    pub year: Option<u16>,
    /// Its release as precise as the source has it.
    pub released: Option<Released>,
    /// Its lookup id, when the source has one.
    pub source: Option<String>,
    /// Its authors, when the source names them (Wikidata does).
    pub creator: Option<String>,
    /// Its cover at the size the cache keeps, when the list carried one.
    pub cover: Option<String>,
    /// Counted as an extra by the automatic rules: hidden until shown.
    pub extra: bool,
}

impl SeriesPart {
    /// Its release as a date: `released`, else the year alone.
    pub fn release(&self) -> Option<Released> {
        self.released.or_else(|| {
            self.year.map(|year| Released {
                year,
                month: None,
                day: None,
            })
        })
    }
}

/// A series: its name and its parts in the source's order.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesParts {
    pub name: String,
    pub parts: Vec<SeriesPart>,
}

/// One episode of a season, as the lookup lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub number: u32,
    /// `None` while the lookup has none ("Episode 5" counts as none).
    pub title: Option<String>,
    pub aired: Option<crate::model::Date>,
    /// In minutes.
    pub runtime: Option<u32>,
}

/// A game platform as the lookup lists it: its own id, the name the app
/// shows and saves, and the other names it goes by, which a search in the
/// picker matches too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePlatform {
    pub id: i64,
    pub name: String,
    pub aka: Vec<String>,
}

impl GamePlatform {
    /// "SNES · Super Nintendo Entertainment System".
    pub fn others(&self) -> String {
        self.aka.join(" · ")
    }

    /// Whether `query` is in the name or the other names, case aside.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.name.to_lowercase().contains(&query)
            || self.aka.iter().any(|a| a.to_lowercase().contains(&query))
    }
}

/// One track of an album's track list, with its lookup id
/// ("deezer:track:3135556").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
}

/// One provider of matches. Every method fails with a [`Failure`] and
/// never returns a match of a type outside `types`.
pub trait Source: Send + Sync {
    /// The provider part of the ids it issues: "tmdb" for "tmdb:movie:603".
    fn id(&self) -> &'static str;

    /// The provider's name, for the episode files: "TMDB".
    fn name(&self) -> &'static str;

    fn types(&self) -> &'static [MediaType];

    /// False when a needed key is missing; then nothing is asked and the
    /// screen says the lookup is not set up.
    fn is_configured(&self) -> bool {
        true
    }

    /// Whether `source` is one of this source's ids.
    fn issued(&self, source: &str) -> bool {
        Match::provider_of(source) == self.id()
    }

    /// Candidates for what the user typed, in the provider's order.
    fn search(&self, query: &str, media: MediaType, limit: usize) -> Reply<Hits>;

    /// `search` in steps, for a source that asks more than one provider:
    /// `step` gets the matches so far, and the return is the complete
    /// answer. One step by default.
    fn search_steps(
        &self,
        query: &str,
        media: MediaType,
        limit: usize,
        _step: &mut dyn FnMut(Vec<Match>),
    ) -> Reply<Hits> {
        self.search(query, media, limit)
    }

    /// `early`, a match from an early step, as the complete answer lists
    /// it, so a pick made before the search was done takes what the later
    /// sources added.
    fn same_match(&self, early: &Match, complete: &[Match]) -> Option<Match> {
        complete.iter().find(|m| m.source == early.source).cloned()
    }

    /// The game platforms a search can be narrowed to, A to Z.
    fn platforms(&self) -> Reply<Vec<GamePlatform>> {
        Ok(Vec::new())
    }

    /// Candidates among the games released on `platform`, each dated by
    /// its release there.
    fn search_on_platform(
        &self,
        query: &str,
        _platform: &GamePlatform,
        limit: usize,
    ) -> Reply<Hits> {
        self.search(query, MediaType::Games, limit)
    }

    /// The one best hit for an entry that was never looked up, so its
    /// cover can be found: the same title, and the same year when known.
    fn match_title(
        &self,
        title: &str,
        media: MediaType,
        year: Option<u16>,
        creator: Option<&str>,
    ) -> Reply<Option<Match>>;

    /// The match with what the search left out (a director, a release
    /// date, genres), fetched once when it is picked.
    fn details(&self, found: &Match) -> Reply<Match> {
        Ok(found.clone())
    }

    /// The seasons of the show with `source`. `None` from a source without
    /// shows.
    fn seasons(&self, _source: &str) -> Reply<Option<ShowSeasons>> {
        Ok(None)
    }

    /// The id the seasons' episodes are read under: `source` itself, or
    /// for an anime series the TMDB show it was matched to.
    fn season_source(&self, source: &str) -> Reply<String> {
        Ok(source.to_owned())
    }

    /// The episodes of season `season` of the show with `source`, in
    /// number order; empty when the show has no such season; `None` from
    /// a source without shows.
    fn season_episodes(&self, _source: &str, _season: u32) -> Reply<Option<Vec<Episode>>> {
        Ok(None)
    }

    /// The series the title with `source` belongs to, with every part in
    /// the series' own order, or `None` when the provider knows none.
    /// `name` is the series name the entry carries, for a provider that
    /// lists parts by name.
    fn series_parts(&self, _source: &str, _name: Option<&str>) -> Reply<Option<SeriesParts>> {
        Ok(None)
    }

    /// A cover image for an entry with `source`, sized for the cache.
    fn cover_url(&self, source: &str, media: MediaType) -> Reply<Option<String>>;

    /// Matches to take a cover from for an entry titled `title`, each with
    /// a cover. Defaults to a search for the title.
    fn cover_choices(&self, title: &str, media: MediaType, _creator: Option<&str>) -> Reply<Hits> {
        self.search(title, media, 20)
    }

    /// The image of a cover picked in `cover_choices`, by its
    /// [`Match::cover_from`] id, sized for the cache.
    fn picked_cover_url(&self, id: &str, media: MediaType) -> Reply<Option<String>> {
        self.cover_url(id, media)
    }

    /// Whether this is the music source, with track lists and artist
    /// photos.
    fn is_music(&self) -> bool {
        false
    }

    /// The tracks of the album with lookup id `album_source`, in order.
    fn album_tracks(&self, _album_source: &str) -> Reply<Vec<TrackInfo>> {
        Ok(Vec::new())
    }

    /// A photo of the artist called `name`, or `None` when the source has
    /// no artist of exactly that name.
    fn artist_photo_url(&self, _name: &str) -> Reply<Option<String>> {
        Ok(None)
    }
}

/// What a search came back with: the matches, and which asks failed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchResult {
    pub matches: Vec<Match>,
    /// One per ask (a source and a type) that failed.
    pub failures: Vec<FailureKind>,
    /// How many providers were asked: one per configured source, and one
    /// more for each that answered only in part. Zero means none is set
    /// up for the type.
    pub asked: usize,
    /// False while some sources have not answered yet.
    pub complete: bool,
}

impl SearchResult {
    pub fn not_configured(&self) -> bool {
        self.asked == 0
    }

    /// Every source asked failed: offline when all were, rate limited when
    /// any was, else other.
    pub fn all_failed(&self) -> Option<FailureKind> {
        if self.asked == 0 || self.failures.len() < self.asked {
            return None;
        }
        if self.failures.iter().all(|f| *f == FailureKind::Offline) {
            return Some(FailureKind::Offline);
        }
        if self.failures.contains(&FailureKind::RateLimited) {
            return Some(FailureKind::RateLimited);
        }
        Some(FailureKind::Other)
    }

    pub fn partly_failed(&self) -> bool {
        !self.failures.is_empty() && self.all_failed().is_none()
    }

    /// The state the lookup pane shows for this answer.
    pub fn state(&self) -> LookupState {
        if !self.complete && self.matches.is_empty() {
            return LookupState::Loading;
        }
        if self.not_configured() {
            return LookupState::NotSetUp;
        }
        match self.all_failed() {
            Some(FailureKind::Offline) => LookupState::Offline,
            Some(_) => LookupState::Failed,
            None if self.partly_failed() && self.matches.is_empty() => LookupState::PartialNone,
            None if self.partly_failed() => LookupState::Partial,
            None => LookupState::Results,
        }
    }
}

/// What the lookup pane shows. `LIBRI_LOOKUP` forces one for a look:
/// "loading", "offline", "not-set-up", "failed", "partial",
/// "partial-none".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LookupState {
    #[default]
    Results,
    Loading,
    Offline,
    NotSetUp,
    /// Every source failed for another reason than being offline.
    Failed,
    /// Some sources failed; the rows are what the others found.
    Partial,
    /// Some sources failed and the others found nothing.
    PartialNone,
}

impl LookupState {
    pub fn from_env(value: &str) -> Option<LookupState> {
        Some(match value {
            "results" => LookupState::Results,
            "loading" => LookupState::Loading,
            "offline" => LookupState::Offline,
            "not-set-up" => LookupState::NotSetUp,
            "failed" => LookupState::Failed,
            "partial" => LookupState::Partial,
            "partial-none" => LookupState::PartialNone,
            _ => return None,
        })
    }

    /// Whether the pane shows rows in this state.
    pub fn shows_rows(self) -> bool {
        matches!(self, LookupState::Results | LookupState::Partial)
    }
}

/// How many matches one type gets, and how many each type gets on a
/// search across every type.
pub const CAP_ONE_TYPE: usize = 10;
/// How many complete answers are remembered for the session.
const CACHE_SIZE: usize = 50;

/// Asks the right sources for a query and merges their answers. A
/// complete answer with no failures is remembered for the session, so
/// switching the type chips back and forth costs nothing.
pub struct Service {
    sources: Vec<Arc<dyn Source>>,
    cache: Mutex<Vec<(String, SearchResult)>>,
    /// The fake answers at once and never touches the network, so the
    /// app asks it on the spot instead of from a task.
    instant: bool,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field(
                "sources",
                &self.sources.iter().map(|s| s.id()).collect::<Vec<_>>(),
            )
            .field("instant", &self.instant)
            .finish()
    }
}

impl Service {
    /// The real providers, with the keys this run has and the files under
    /// `cache` (the IGDB token and platform list, the anime links).
    pub fn real(keys: &Keys, cache: Option<&std::path::Path>) -> Service {
        let tmdb = Arc::new(tmdb::Tmdb::new(&keys.tmdb_read_token));
        let sources: Vec<Arc<dyn Source>> = vec![
            Arc::new(igdb::Igdb::new(
                &keys.igdb_client_id,
                &keys.igdb_client_secret,
                cache,
            )),
            tmdb.clone(),
            Arc::new(anilist::AniList::new(Some(tmdb), cache)),
            Arc::new(books::Books::new(&keys.google_books_key)),
            Arc::new(deezer::Deezer::new()),
        ];
        Service::with(sources, false)
    }

    /// The fixture catalog, for the tests and the mock runs.
    pub fn fake() -> Service {
        Service::with(vec![Arc::new(fake::Fake)], true)
    }

    pub fn with(sources: Vec<Arc<dyn Source>>, instant: bool) -> Service {
        Service {
            sources,
            cache: Mutex::new(Vec::new()),
            instant,
        }
    }

    /// Whether the sources answer at once, without the network.
    pub fn is_instant(&self) -> bool {
        self.instant
    }

    pub fn sources(&self) -> &[Arc<dyn Source>] {
        &self.sources
    }

    /// The configured sources for `media`.
    pub fn sources_for(&self, media: MediaType) -> Vec<&dyn Source> {
        self.sources
            .iter()
            .map(|s| s.as_ref())
            .filter(|s| s.is_configured() && s.types().contains(&media))
            .collect()
    }

    /// The source that issued `id` ("tmdb:movie:603").
    pub fn source_of(&self, id: &str) -> Option<&dyn Source> {
        self.sources
            .iter()
            .map(|s| s.as_ref())
            .find(|s| s.issued(id))
    }

    /// The music source, when one is configured.
    pub fn music(&self) -> Option<&dyn Source> {
        self.sources
            .iter()
            .map(|s| s.as_ref())
            .find(|s| s.is_configured() && s.is_music())
    }

    /// The key a search is remembered under.
    pub fn key(query: &str, media: MediaType, platform: Option<&GamePlatform>) -> String {
        format!(
            "{}|{}|{}",
            media.key(),
            platform
                .map(|p| format!("{}:{}", p.id, p.name))
                .unwrap_or_default(),
            query.trim().to_lowercase()
        )
    }

    /// A remembered complete answer.
    pub fn cached(&self, key: &str) -> Option<SearchResult> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.iter().find(|(k, _)| k == key).map(|(_, r)| r.clone())
    }

    fn remember(&self, key: &str, result: &SearchResult) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|(k, _)| k != key);
        cache.push((key.to_owned(), result.clone()));
        while cache.len() > CACHE_SIZE {
            cache.remove(0);
        }
    }

    /// The last of `search_steps`.
    pub fn search(
        &self,
        query: &str,
        media: MediaType,
        platform: Option<&GamePlatform>,
    ) -> SearchResult {
        let mut last = SearchResult::default();
        self.search_steps(query, media, platform, &mut |result| last = result);
        last
    }

    /// The matches as the sources answer: `step` gets a result each time
    /// one answers (or answers in part), marked complete on the last. Only
    /// a complete result with no failures is remembered. `platform` counts
    /// for games.
    pub fn search_steps(
        &self,
        query: &str,
        media: MediaType,
        platform: Option<&GamePlatform>,
        step: &mut dyn FnMut(SearchResult),
    ) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            step(SearchResult {
                complete: true,
                ..SearchResult::default()
            });
            return;
        }
        let platform = if media == MediaType::Games {
            platform
        } else {
            None
        };
        let key = Service::key(trimmed, media, platform);
        if let Some(cached) = self.cached(&key) {
            step(cached);
            return;
        }
        let asks = self.sources_for(media);
        if asks.is_empty() {
            step(SearchResult {
                complete: true,
                ..SearchResult::default()
            });
            return;
        }

        enum Answer {
            Partial(Vec<Match>),
            Done(Reply<Hits>),
        }
        let (tx, rx) = std::sync::mpsc::channel::<(usize, Answer)>();
        std::thread::scope(|scope| {
            for (i, source) in asks.iter().enumerate() {
                let tx = tx.clone();
                scope.spawn(move || {
                    let mut partial = |matches: Vec<Match>| {
                        let _ = tx.send((i, Answer::Partial(matches)));
                    };
                    let done = match platform {
                        Some(platform) => {
                            source.search_on_platform(trimmed, platform, CAP_ONE_TYPE)
                        }
                        None => source.search_steps(trimmed, media, CAP_ONE_TYPE, &mut partial),
                    };
                    let _ = tx.send((i, Answer::Done(done)));
                });
            }
            drop(tx);
            // The latest answer of each ask, and how many gave their last.
            let mut answers: Vec<Option<Answer>> = (0..asks.len()).map(|_| None).collect();
            let mut pending = asks.len();
            for (i, answer) in rx {
                if matches!(answer, Answer::Done(_)) {
                    pending -= 1;
                }
                answers[i] = Some(answer);
                let mut result = SearchResult {
                    complete: pending == 0,
                    asked: asks.len(),
                    ..SearchResult::default()
                };
                for answer in answers.iter().flatten() {
                    match answer {
                        Answer::Partial(matches) => result
                            .matches
                            .extend(matches.iter().take(CAP_ONE_TYPE).cloned()),
                        Answer::Done(Ok(hits)) => {
                            result
                                .matches
                                .extend(hits.matches.iter().take(CAP_ONE_TYPE).cloned());
                            if let Some(failure) = &hits.failure {
                                result.failures.push(failure.kind);
                                result.asked += 1;
                            }
                        }
                        Answer::Done(Err(failure)) => result.failures.push(failure.kind),
                    }
                }
                if result.complete && result.failures.is_empty() {
                    self.remember(&key, &result);
                }
                step(result);
            }
        });
    }

    /// Covers to pick from for an entry titled `title`: each configured
    /// source for `media` in turn, only matches with a cover and one of
    /// each image. Nothing is remembered.
    pub fn cover_choices(
        &self,
        title: &str,
        media: MediaType,
        creator: Option<&str>,
    ) -> SearchResult {
        let trimmed = title.trim();
        let mut result = SearchResult {
            complete: true,
            ..SearchResult::default()
        };
        if trimmed.is_empty() {
            return result;
        }
        let asked = self.sources_for(media);
        result.asked = asked.len();
        let mut seen: Vec<String> = Vec::new();
        let mut add = |matches: Vec<Match>, result: &mut SearchResult| {
            for found in matches {
                let Some(url) = found.cache_cover_url() else {
                    continue;
                };
                if seen.iter().any(|s| s == url) {
                    continue;
                }
                seen.push(url.to_owned());
                result.matches.push(found);
            }
        };
        for source in asked {
            match source.cover_choices(trimmed, media, creator) {
                Ok(hits) => {
                    if let Some(failure) = &hits.failure {
                        result.failures.push(failure.kind);
                        result.asked += 1;
                    }
                    add(hits.matches, &mut result);
                }
                Err(failure) => result.failures.push(failure.kind),
            }
        }
        result
    }

    /// The platforms games can be narrowed to: the first configured game
    /// source's. Empty when none is set up.
    pub fn platforms(&self) -> Reply<Vec<GamePlatform>> {
        for source in self.sources_for(MediaType::Games) {
            let platforms = source.platforms()?;
            if !platforms.is_empty() {
                return Ok(platforms);
            }
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that answers as told, for the service's rules.
    struct Scripted {
        id: &'static str,
        types: &'static [MediaType],
        configured: bool,
        answer: Reply<Hits>,
    }

    impl Source for Scripted {
        fn id(&self) -> &'static str {
            self.id
        }
        fn name(&self) -> &'static str {
            self.id
        }
        fn types(&self) -> &'static [MediaType] {
            self.types
        }
        fn is_configured(&self) -> bool {
            self.configured
        }
        fn search(&self, _: &str, _: MediaType, _: usize) -> Reply<Hits> {
            self.answer.clone()
        }
        fn match_title(
            &self,
            _: &str,
            _: MediaType,
            _: Option<u16>,
            _: Option<&str>,
        ) -> Reply<Option<Match>> {
            Ok(None)
        }
        fn cover_url(&self, _: &str, _: MediaType) -> Reply<Option<String>> {
            Ok(None)
        }
    }

    fn hit(title: &str) -> Match {
        Match::new(MediaType::Books, title, format!("x:{title}"))
    }

    #[test]
    fn a_search_merges_the_sources_and_counts_their_failures() {
        let service = Service::with(
            vec![
                Arc::new(Scripted {
                    id: "a",
                    types: &[MediaType::Books],
                    configured: true,
                    answer: Ok(Hits {
                        matches: vec![hit("One")],
                        failure: Some(Failure::new(FailureKind::Offline, "")),
                    }),
                }),
                Arc::new(Scripted {
                    id: "b",
                    types: &[MediaType::Books],
                    configured: true,
                    answer: Err(Failure::new(FailureKind::RateLimited, "")),
                }),
                Arc::new(Scripted {
                    id: "c",
                    types: &[MediaType::Games],
                    configured: false,
                    answer: Ok(Hits::default()),
                }),
            ],
            true,
        );
        let result = service.search("one", MediaType::Books, None);
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.asked, 3, "two sources, one answered in part");
        assert_eq!(result.failures.len(), 2);
        assert!(result.complete);
        assert_eq!(result.state(), LookupState::Partial);
        assert!(
            service
                .cached(&Service::key("one", MediaType::Books, None))
                .is_none()
        );

        let games = service.search("one", MediaType::Games, None);
        assert_eq!(games.state(), LookupState::NotSetUp);
        let empty = service.search("  ", MediaType::Books, None);
        assert!(empty.complete && empty.matches.is_empty());
    }

    #[test]
    fn a_complete_answer_without_failures_is_remembered() {
        let service = Service::with(
            vec![Arc::new(Scripted {
                id: "a",
                types: &[MediaType::Books],
                configured: true,
                answer: Ok(Hits::of(vec![hit("One"), hit("Two")])),
            })],
            true,
        );
        let result = service.search("One ", MediaType::Books, None);
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.state(), LookupState::Results);
        assert!(
            service
                .cached(&Service::key("one", MediaType::Books, None))
                .is_some()
        );
    }

    #[test]
    fn the_states_follow_the_phones_rules() {
        let failed = |failures: Vec<FailureKind>, matches: usize| SearchResult {
            matches: (0..matches).map(|i| hit(&i.to_string())).collect(),
            failures,
            asked: 2,
            complete: true,
        };
        assert_eq!(
            failed(vec![FailureKind::Offline, FailureKind::Offline], 0).state(),
            LookupState::Offline
        );
        assert_eq!(
            failed(vec![FailureKind::Offline, FailureKind::Other], 0).state(),
            LookupState::Failed
        );
        assert_eq!(
            failed(vec![FailureKind::Offline], 0).state(),
            LookupState::PartialNone
        );
        assert_eq!(failed(vec![], 0).state(), LookupState::Results);
        let loading = SearchResult {
            asked: 1,
            complete: false,
            ..SearchResult::default()
        };
        assert_eq!(loading.state(), LookupState::Loading);
        assert_eq!(LookupState::from_env("failed"), Some(LookupState::Failed));
        assert_eq!(LookupState::from_env("nope"), None);
        assert_eq!(Match::provider_of("tmdb:movie:603"), "tmdb");
    }
}
