//! Covers, artist photos and the other things the lookups fetch for the
//! screens (a season's episodes, a series' parts, an album's track list,
//! IGDB's platforms), as the app keeps them between frames. The screens
//! say what they show (`Wants`), the app queues what is missing, runs a
//! few lookups at a time on tokio's blocking pool and hands the answers
//! back as messages; the views read the results here and draw the
//! placeholder until the image is on disk.
//!
//! The phone's rules (its DESIGN.md 10c): an entry with a `source` asks
//! that provider for its cover, one without is matched by title, year and
//! creator (a song by its album); the result goes into `artwork.json`
//! under the cache folder keyed to the title, year, creator, album, source
//! and `cover_from`, so an edit to any of those means a new lookup. Only a
//! real answer of no cover is a miss, tried again after fourteen days; a
//! lookup that did not work out (offline, rate limited, not set up, a
//! server error) is kept in memory only and asked again a little later.
//! Image files live under `images/<sha1 of the url>.img`; a failed
//! download is not retried for six hours (thirty seconds when offline).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::Task;
use iced::widget::image;
use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::lookup::store::{
    IMAGE_RETRY, IMAGE_RETRY_OFFLINE, ImageFailure, ImageStore, JsonFile, Stores,
};
use crate::lookup::{
    self, Episode, FailureKind, GamePlatform, Reply, SeriesParts, Service, TrackInfo,
};
use crate::model::{Entry, EntryKind, Library, MediaType, Season};

/// How many cover and photo lookups run at once.
const CONCURRENCY: usize = 2;
/// How many image downloads run at once.
const DOWNLOADS: usize = 4;
/// How long a miss stands before the cover is looked for again.
const MISS_RETRY: i64 = 14 * 86_400;
/// How long a lookup that did not work out waits before it is asked again
/// in this session.
const FAILURE_RETRY: Duration = Duration::from_secs(10 * 60);
/// How long `artwork.json` waits for more changes before it is written.
const WRITE_DELAY: Duration = Duration::from_secs(2);
/// The `artwork.json` format: 2 since misses are only real answers.
const VERSION: i64 = 2;
/// How long a stored series list is trusted before the lookup is asked
/// again, since a series grows.
const SERIES_REFRESH: i64 = 7 * 86_400;
/// How long after a season's last episode its stored list is trusted for
/// good.
const EPISODES_SETTLE: i64 = 30;
/// The session's prune runs this long after the start.
pub const PRUNE_DELAY: Duration = Duration::from_secs(15);

/// What the screens show right now, so the missing pieces are fetched:
/// entries whose covers are on screen, artists whose photos are, image
/// URLs the form shows (match thumbnails), and the entries whose series,
/// season or track list is open.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wants {
    pub covers: Vec<String>,
    pub artists: Vec<String>,
    pub images: Vec<String>,
    pub series: Vec<String>,
    pub episodes: Vec<(String, u32)>,
    pub tracks: Vec<String>,
    pub platforms: bool,
}

/// One remembered answer: the URL, or a miss (no URL), with when it was
/// looked for and the id the entry was matched to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    key: String,
    url: Option<String>,
    source: Option<String>,
    resolved: Timestamp,
}

impl Record {
    fn from_json(json: &Value) -> Option<Record> {
        use crate::lookup::http::str_at;
        Some(Record {
            key: str_at(json, "key")?.to_owned(),
            url: str_at(json, "url").map(str::to_owned),
            source: str_at(json, "source").map(str::to_owned),
            resolved: Timestamp::parse(str_at(json, "resolved")?)?,
        })
    }

    fn to_json(&self) -> Value {
        let mut json = json!({"key": self.key, "resolved": self.resolved.to_rfc3339()});
        if let Some(url) = &self.url {
            json["url"] = Value::String(url.clone());
        }
        if let Some(source) = &self.source {
            json["source"] = Value::String(source.clone());
        }
        json
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ImageState {
    Ready(PathBuf),
    Failed(Instant),
}

/// Something fetched for an entry: loading, failed, none, or the value.
#[derive(Debug, Clone, PartialEq)]
pub enum Loaded<T> {
    /// Nothing known yet, and a lookup on its way (or waiting for the
    /// cover match that gives the id).
    Loading,
    /// The lookup gave no answer, and nothing is stored.
    Failed(FailureKind),
    /// The lookup knows nothing for this entry.
    None,
    Ready(T),
}

/// A season's episodes as the page shows them.
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodesState {
    /// The id the episodes were asked for, so a changed source asks again.
    source: String,
    pub loaded: Loaded<Vec<Episode>>,
    loading: bool,
}

/// A series' parts as the inspector and the series page show them.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesState {
    source: String,
    asked: Option<String>,
    pub loaded: Loaded<SeriesParts>,
    /// The stored list is all there is: asking for a newer one failed.
    pub update_failed: bool,
    loading: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracksState {
    source: String,
    pub loaded: Loaded<Vec<TrackInfo>>,
    loading: bool,
}

/// What a lookup task hands back.
#[derive(Debug, Clone)]
pub enum Message {
    CoverResolved {
        id: String,
        key: String,
        outcome: Result<(Option<String>, Option<String>), FailureKind>,
    },
    ArtistResolved {
        key: String,
        outcome: Result<Option<String>, FailureKind>,
    },
    ImageFetched {
        url: String,
        outcome: Result<PathBuf, ImageFailure>,
    },
    Episodes {
        id: String,
        season: u32,
        source: String,
        step: Step<Vec<Episode>>,
    },
    Series {
        id: String,
        source: String,
        step: Step<SeriesParts>,
    },
    Tracks {
        id: String,
        source: String,
        outcome: Reply<Vec<TrackInfo>>,
    },
    Platforms(Reply<Vec<GamePlatform>>),
    /// The session's prune, due.
    Prune,
    Pruned,
}

/// One step of a load that shows what is stored before what is fetched.
#[derive(Debug, Clone, PartialEq)]
pub enum Step<T> {
    /// The stored value, with more to come when `more` is set.
    Stored {
        value: T,
        more: bool,
    },
    Fetched(Option<T>),
    /// The fetch failed; with a stored value shown, that stays.
    Failed(FailureKind),
}

pub struct Artwork {
    stores: Stores,
    images: Arc<ImageStore>,
    file: Option<JsonFile>,
    records: HashMap<String, Record>,
    artists: HashMap<String, Record>,
    failures: HashMap<String, (FailureKind, Instant)>,
    resolving: HashSet<String>,
    queue: Vec<Job>,
    image_states: HashMap<String, ImageState>,
    downloading: HashSet<String>,
    image_queue: Vec<String>,
    series: HashMap<String, SeriesState>,
    episodes: HashMap<(String, u32), EpisodesState>,
    tracks: HashMap<String, TracksState>,
    platforms: Option<Reply<Vec<GamePlatform>>>,
    platforms_loading: bool,
    dirty: bool,
    changed_at: Option<Instant>,
    pruned: bool,
}

#[derive(Debug, Clone)]
enum Job {
    Entry(Box<Entry>),
    Artist(String),
}

impl std::fmt::Debug for Artwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Artwork")
            .field("records", &self.records.len())
            .field("images", &self.image_states.len())
            .finish_non_exhaustive()
    }
}

impl Default for Artwork {
    fn default() -> Self {
        Artwork::at(None)
    }
}

impl Artwork {
    /// The state over the cache folder `dir`; none keeps everything in
    /// memory (the tests, the mock runs).
    pub fn at(dir: Option<PathBuf>) -> Artwork {
        let file = dir.as_ref().map(|d| JsonFile(d.join("artwork.json")));
        let mut artwork = Artwork {
            stores: Stores::at(dir.clone()),
            images: Arc::new(ImageStore::at(dir)),
            file,
            records: HashMap::new(),
            artists: HashMap::new(),
            failures: HashMap::new(),
            resolving: HashSet::new(),
            queue: Vec::new(),
            image_states: HashMap::new(),
            downloading: HashSet::new(),
            image_queue: Vec::new(),
            series: HashMap::new(),
            episodes: HashMap::new(),
            tracks: HashMap::new(),
            platforms: None,
            platforms_loading: false,
            dirty: false,
            changed_at: None,
            pruned: false,
        };
        artwork.load();
        artwork
    }

    /// Reads `artwork.json` once. The misses of a version 1 file, which
    /// could be failed lookups, are dropped and looked for again.
    fn load(&mut self) {
        let Some(json) = self.file.as_ref().and_then(JsonFile::read) else {
            return;
        };
        let read = |into: &mut HashMap<String, Record>, value: Option<&Value>| {
            if let Some(Value::Object(map)) = value {
                for (key, value) in map {
                    if let Some(record) = Record::from_json(value) {
                        into.insert(key.clone(), record);
                    }
                }
            }
        };
        read(&mut self.records, json.get("entries"));
        read(&mut self.artists, json.get("artists"));
        if lookup::http::int_at(&json, "version") != Some(VERSION) {
            self.records.retain(|_, r| r.url.is_some());
            self.artists.retain(|_, r| r.url.is_some());
            self.mark_dirty();
        }
    }

    fn mark_dirty(&mut self) {
        if self.file.is_some() {
            self.dirty = true;
            self.changed_at.get_or_insert_with(Instant::now);
        }
    }

    /// Writes the records when something changed a moment ago, or at once
    /// with `now`.
    fn flush(&mut self, now: bool) {
        if !self.dirty {
            return;
        }
        if !now && self.changed_at.is_some_and(|at| at.elapsed() < WRITE_DELAY) {
            return;
        }
        let Some(file) = &self.file else {
            self.dirty = false;
            return;
        };
        let map = |records: &HashMap<String, Record>| {
            Value::Object(
                records
                    .iter()
                    .map(|(k, r)| (k.clone(), r.to_json()))
                    .collect(),
            )
        };
        let _ = file.write(&json!({
            "version": VERSION,
            "entries": map(&self.records),
            "artists": map(&self.artists),
        }));
        self.dirty = false;
        self.changed_at = None;
    }

    // What the views read.

    /// What identifies the entry for its record: a change to any of these
    /// means a new lookup.
    pub fn key_for(entry: &Entry) -> String {
        [
            entry.title.as_str(),
            &entry.year().map(|y| y.to_string()).unwrap_or_default(),
            entry.creator.as_deref().unwrap_or_default(),
            entry.album.as_deref().unwrap_or_default(),
            entry.source.as_deref().unwrap_or_default(),
            entry.cover_from.as_deref().unwrap_or_default(),
        ]
        .join("|")
    }

    fn record_of(&self, entry: &Entry) -> Option<&Record> {
        self.records
            .get(&entry.id)
            .filter(|r| r.key == Artwork::key_for(entry))
    }

    /// The cover URL when it is already known.
    pub fn cover_url(&self, entry: &Entry) -> Option<&str> {
        self.record_of(entry).and_then(|r| r.url.as_deref())
    }

    /// The lookup id the entry was matched to on this computer, when
    /// known, for its series, episodes and track list: its own `source`,
    /// else the id its cover was matched to.
    pub fn source_of(&self, entry: &Entry) -> Option<String> {
        entry
            .source
            .clone()
            .or_else(|| self.record_of(entry).and_then(|r| r.source.clone()))
    }

    /// The entry's cover, once it is on this computer.
    pub fn cover(&self, entry: &Entry) -> Option<image::Handle> {
        self.image(self.cover_url(entry)?)
    }

    /// An image by URL, once it is on this computer.
    pub fn image(&self, url: &str) -> Option<image::Handle> {
        match self.image_states.get(url) {
            Some(ImageState::Ready(path)) => Some(image::Handle::from_path(path)),
            _ => None,
        }
    }

    fn artist_key(name: &str) -> String {
        name.trim().to_lowercase()
    }

    /// The artist's photo, once it is on this computer.
    pub fn artist_photo(&self, name: &str) -> Option<image::Handle> {
        let url = self
            .artists
            .get(&Artwork::artist_key(name))?
            .url
            .as_deref()?;
        self.image(url)
    }

    pub fn series_for(&self, entry: &Entry) -> Option<&SeriesState> {
        self.series.get(&entry.id)
    }

    pub fn episodes_for(&self, entry: &Entry, season: u32) -> Option<&EpisodesState> {
        self.episodes.get(&(entry.id.clone(), season))
    }

    pub fn tracks_for(&self, entry: &Entry) -> Option<&TracksState> {
        self.tracks.get(&entry.id)
    }

    /// IGDB's platforms, once loaded; `None` while loading or before.
    pub fn platforms(&self) -> Option<&Reply<Vec<GamePlatform>>> {
        self.platforms.as_ref()
    }

    pub fn platform_named(&self, name: &str) -> Option<GamePlatform> {
        self.platforms
            .as_ref()
            .and_then(|p| p.as_ref().ok())
            .and_then(|list| list.iter().find(|p| p.name == name))
            .cloned()
    }

    // What the app says.

    /// Forgets what was found for `id`, so the next look asks again (after
    /// a pick in the form set a source).
    pub fn forget(&mut self, id: &str) {
        if self.records.remove(id).is_some() {
            self.mark_dirty();
        }
        self.series.remove(id);
        self.tracks.remove(id);
        self.episodes.retain(|(entry, _), _| entry != id);
    }

    /// Takes `wants` on board and starts what is missing, a few at a time.
    /// With an instant service (the fake) everything is answered on the
    /// spot and no task is returned.
    pub fn refresh(
        &mut self,
        wants: &Wants,
        library: &Library,
        service: &Arc<Service>,
    ) -> Task<Message> {
        self.queue.clear();
        self.image_queue.clear();
        let now = Instant::now();
        self.failures
            .retain(|_, (_, at)| now.duration_since(*at) < FAILURE_RETRY);
        for id in &wants.covers {
            if let Some(entry) = library.entry(id) {
                self.want_cover(entry);
            }
        }
        for name in &wants.artists {
            self.want_artist(name);
        }
        for url in &wants.images {
            self.want_image(url);
        }
        let mut tasks = Vec::new();
        for id in &wants.series {
            if let Some(entry) = library.entry(id) {
                tasks.extend(self.want_series(entry, service));
            }
        }
        for (id, season) in &wants.episodes {
            if let Some(entry) = library.entry(id)
                && let Some(season) = entry.season_list().iter().find(|s| s.season == *season)
            {
                tasks.extend(self.want_episodes(entry, season, service));
            }
        }
        for id in &wants.tracks {
            if let Some(entry) = library.entry(id) {
                tasks.extend(self.want_tracks(entry, service));
            }
        }
        if wants.platforms {
            tasks.extend(self.want_platforms(service));
        }
        tasks.extend(self.start(service));
        self.flush(false);
        Task::batch(tasks)
    }

    fn want_cover(&mut self, entry: &Entry) {
        if let Some(record) = self.record_of(entry) {
            match &record.url {
                Some(url) => {
                    let url = url.clone();
                    self.want_image(&url);
                    return;
                }
                None if Timestamp::now().0 - record.resolved.0 < MISS_RETRY => return,
                None => {}
            }
        }
        let job = format!("entry:{}", entry.id);
        if self.failures.contains_key(&job) || self.resolving.contains(&job) {
            return;
        }
        if !self
            .queue
            .iter()
            .any(|j| matches!(j, Job::Entry(e) if e.id == entry.id))
        {
            self.queue.push(Job::Entry(Box::new(entry.clone())));
        }
    }

    fn want_artist(&mut self, name: &str) {
        let key = Artwork::artist_key(name);
        if lookup::NO_PHOTO.contains(&key.as_str()) {
            return;
        }
        if let Some(record) = self.artists.get(&key) {
            match &record.url {
                Some(url) => {
                    let url = url.clone();
                    self.want_image(&url);
                    return;
                }
                None if Timestamp::now().0 - record.resolved.0 < MISS_RETRY => return,
                None => {}
            }
        }
        let job = format!("artist:{key}");
        if self.failures.contains_key(&job) || self.resolving.contains(&job) {
            return;
        }
        if !self
            .queue
            .iter()
            .any(|j| matches!(j, Job::Artist(n) if *n == name))
        {
            self.queue.push(Job::Artist(name.to_owned()));
        }
    }

    fn want_image(&mut self, url: &str) {
        match self.image_states.get(url) {
            Some(ImageState::Ready(_)) => return,
            Some(ImageState::Failed(until)) if Instant::now() < *until => return,
            _ => {}
        }
        if let Some(path) = self.images.cached(url) {
            self.image_states
                .insert(url.to_owned(), ImageState::Ready(path));
            return;
        }
        if !self.downloading.contains(url) && !self.image_queue.iter().any(|u| u == url) {
            self.image_queue.push(url.to_owned());
        }
    }

    /// Starts the queued lookups and downloads, within the limits.
    fn start(&mut self, service: &Arc<Service>) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();
        while self.resolving.len() < CONCURRENCY && !self.queue.is_empty() {
            let job = self.queue.remove(0);
            match job {
                Job::Entry(entry) => {
                    let id = entry.id.clone();
                    let key = Artwork::key_for(&entry);
                    if service.is_instant() {
                        let outcome = resolve_cover(service, &entry).map_err(|f| f.kind);
                        self.cover_resolved(id, key, outcome);
                        continue;
                    }
                    self.resolving.insert(format!("entry:{id}"));
                    let service = service.clone();
                    tasks.push(Task::perform(
                        crate::app::blocking(move || Ok(resolve_cover(&service, &entry))),
                        move |result| Message::CoverResolved {
                            id: id.clone(),
                            key: key.clone(),
                            outcome: result
                                .unwrap_or_else(|_| Err(lookup::Failure::other("no answer")))
                                .map_err(|f| f.kind),
                        },
                    ));
                }
                Job::Artist(name) => {
                    let key = Artwork::artist_key(&name);
                    if service.is_instant() {
                        let outcome = resolve_artist(service, &name).map_err(|f| f.kind);
                        self.artist_resolved(key, outcome);
                        continue;
                    }
                    self.resolving.insert(format!("artist:{key}"));
                    let service = service.clone();
                    tasks.push(Task::perform(
                        crate::app::blocking(move || Ok(resolve_artist(&service, &name))),
                        move |result| Message::ArtistResolved {
                            key: key.clone(),
                            outcome: result
                                .unwrap_or_else(|_| Err(lookup::Failure::other("no answer")))
                                .map_err(|f| f.kind),
                        },
                    ));
                }
            }
        }
        // The fake has no images to fetch; nothing leaves a mock run.
        if service.is_instant() {
            self.image_queue.clear();
        }
        while self.downloading.len() < DOWNLOADS && !self.image_queue.is_empty() {
            let url = self.image_queue.remove(0);
            self.downloading.insert(url.clone());
            let images = self.images.clone();
            let fetched = url.clone();
            tasks.push(Task::perform(
                crate::app::blocking(move || Ok(images.fetch(&fetched))),
                move |result| Message::ImageFetched {
                    url: url.clone(),
                    outcome: result.unwrap_or_else(|e| Err(ImageFailure::Other(e))),
                },
            ));
        }
        tasks
    }

    fn cover_resolved(
        &mut self,
        id: String,
        key: String,
        outcome: Result<(Option<String>, Option<String>), FailureKind>,
    ) {
        self.resolving.remove(&format!("entry:{id}"));
        match outcome {
            Ok((url, source)) => {
                self.failures.remove(&format!("entry:{id}"));
                if let Some(url) = &url {
                    self.want_image(url);
                }
                self.records.insert(
                    id,
                    Record {
                        key,
                        url,
                        source,
                        resolved: Timestamp::now(),
                    },
                );
                self.mark_dirty();
            }
            Err(kind) => {
                self.failures
                    .insert(format!("entry:{id}"), (kind, Instant::now()));
            }
        }
    }

    fn artist_resolved(&mut self, key: String, outcome: Result<Option<String>, FailureKind>) {
        self.resolving.remove(&format!("artist:{key}"));
        match outcome {
            Ok(url) => {
                if let Some(url) = &url {
                    self.want_image(url);
                }
                self.artists.insert(
                    key.clone(),
                    Record {
                        key,
                        url,
                        source: None,
                        resolved: Timestamp::now(),
                    },
                );
                self.mark_dirty();
            }
            Err(kind) => {
                self.failures
                    .insert(format!("artist:{key}"), (kind, Instant::now()));
            }
        }
    }

    /// A season's episodes: the stored list at once, and a fetch unless the
    /// list is settled.
    fn want_episodes(
        &mut self,
        entry: &Entry,
        season: &Season,
        service: &Arc<Service>,
    ) -> Option<Task<Message>> {
        let key = (entry.id.clone(), season.season);
        if service.is_instant() {
            let key_clone = key.clone();
            self.episodes
                .entry(key_clone)
                .or_insert_with(|| EpisodesState {
                    source: "fake".into(),
                    loaded: Loaded::Ready(lookup::fake::episodes(entry, season)),
                    loading: false,
                });
            return None;
        }
        let Some(source) = self.source_of(entry) else {
            // The cover match gives the id; until then, loading.
            self.episodes.entry(key).or_insert(EpisodesState {
                source: String::new(),
                loaded: Loaded::Loading,
                loading: false,
            });
            return None;
        };
        if let Some(state) = self.episodes.get(&key)
            && state.source == source
            && (state.loading || state.loaded != Loaded::Loading)
        {
            return None;
        }
        self.episodes.insert(
            key.clone(),
            EpisodesState {
                source: source.clone(),
                loaded: Loaded::Loading,
                loading: true,
            },
        );
        let service = service.clone();
        let stores = self.stores.clone();
        let (id, number) = key;
        let own = source.clone();
        Some(
            crate::app::steps(move |step: &mut dyn FnMut(Step<Vec<Episode>>)| {
                load_episodes(&service, &stores, &own, number, step);
            })
            .map(move |step| Message::Episodes {
                id: id.clone(),
                season: number,
                source: source.clone(),
                step,
            }),
        )
    }

    /// A series' parts: the stored list at once, and a fetch once it is a
    /// week old or was asked for another name.
    fn want_series(&mut self, entry: &Entry, service: &Arc<Service>) -> Option<Task<Message>> {
        let asked = entry.series.as_ref().map(|s| s.name.clone());
        if service.is_instant() {
            if self.series.get(&entry.id).is_some_and(|s| s.asked == asked) {
                return None;
            }
            let parts = asked
                .as_deref()
                .and_then(|name| lookup::fake::series_parts(entry.media, name));
            self.series.insert(
                entry.id.clone(),
                SeriesState {
                    source: "fake".into(),
                    asked,
                    loaded: parts.map_or(Loaded::None, Loaded::Ready),
                    update_failed: false,
                    loading: false,
                },
            );
            return None;
        }
        let Some(source) = self.source_of(entry) else {
            self.series.entry(entry.id.clone()).or_insert(SeriesState {
                source: String::new(),
                asked: asked.clone(),
                loaded: Loaded::Loading,
                update_failed: false,
                loading: false,
            });
            return None;
        };
        if let Some(state) = self.series.get(&entry.id)
            && state.source == source
            && state.asked == asked
            && (state.loading || state.loaded != Loaded::Loading)
        {
            return None;
        }
        self.series.insert(
            entry.id.clone(),
            SeriesState {
                source: source.clone(),
                asked: asked.clone(),
                loaded: Loaded::Loading,
                update_failed: false,
                loading: true,
            },
        );
        let service = service.clone();
        let stores = self.stores.clone();
        let id = entry.id.clone();
        let own = source.clone();
        Some(
            crate::app::steps(move |step: &mut dyn FnMut(Step<SeriesParts>)| {
                load_series(&service, &stores, &own, asked.as_deref(), step);
            })
            .map(move |step| Message::Series {
                id: id.clone(),
                source: source.clone(),
                step,
            }),
        )
    }

    fn want_tracks(&mut self, entry: &Entry, service: &Arc<Service>) -> Option<Task<Message>> {
        if service.is_instant() {
            self.tracks.entry(entry.id.clone()).or_insert_with(|| {
                let tracks = lookup::fake::track_list(crate::music::artist_of(entry), &entry.title);
                TracksState {
                    source: "fake".into(),
                    loaded: tracks.map_or(Loaded::None, Loaded::Ready),
                    loading: false,
                }
            });
            return None;
        }
        let Some(source) = self.source_of(entry) else {
            self.tracks.entry(entry.id.clone()).or_insert(TracksState {
                source: String::new(),
                loaded: Loaded::Loading,
                loading: false,
            });
            return None;
        };
        if let Some(state) = self.tracks.get(&entry.id)
            && state.source == source
            && (state.loading || state.loaded != Loaded::Loading)
        {
            return None;
        }
        if let Some(stored) = self.stores.read_tracks(&source) {
            self.tracks.insert(
                entry.id.clone(),
                TracksState {
                    source,
                    loaded: Loaded::Ready(stored),
                    loading: false,
                },
            );
            return None;
        }
        self.tracks.insert(
            entry.id.clone(),
            TracksState {
                source: source.clone(),
                loaded: Loaded::Loading,
                loading: true,
            },
        );
        let service = service.clone();
        let stores = self.stores.clone();
        let id = entry.id.clone();
        let album = source.clone();
        Some(Task::perform(
            crate::app::blocking(move || {
                let Some(music) = service.music() else {
                    return Ok(Err(lookup::Failure::new(FailureKind::NotConfigured, "")));
                };
                let tracks = music.album_tracks(&album);
                if let Ok(tracks) = &tracks
                    && !tracks.is_empty()
                {
                    stores.write_tracks(&album, tracks);
                }
                Ok(tracks)
            }),
            move |result| Message::Tracks {
                id: id.clone(),
                source: source.clone(),
                outcome: result.unwrap_or_else(|e| Err(lookup::Failure::other(e))),
            },
        ))
    }

    fn want_platforms(&mut self, service: &Arc<Service>) -> Option<Task<Message>> {
        if self.platforms_loading || matches!(self.platforms, Some(Ok(_))) {
            return None;
        }
        if service.is_instant() {
            self.platforms = Some(service.platforms());
            return None;
        }
        self.platforms_loading = true;
        let service = service.clone();
        Some(Task::perform(
            crate::app::blocking(move || Ok(service.platforms())),
            |result| Message::Platforms(result.unwrap_or_else(|e| Err(lookup::Failure::other(e)))),
        ))
    }

    /// Asks for the platforms again after a failure ("Try again").
    pub fn retry_platforms(&mut self) {
        if matches!(self.platforms, Some(Err(_))) {
            self.platforms = None;
        }
    }

    /// A task's answer.
    pub fn update(&mut self, message: Message, library: &Library) -> Task<Message> {
        match message {
            Message::CoverResolved { id, key, outcome } => self.cover_resolved(id, key, outcome),
            Message::ArtistResolved { key, outcome } => self.artist_resolved(key, outcome),
            Message::ImageFetched { url, outcome } => {
                self.downloading.remove(&url);
                let state = match outcome {
                    Ok(path) => ImageState::Ready(path),
                    Err(ImageFailure::Offline) => {
                        ImageState::Failed(Instant::now() + IMAGE_RETRY_OFFLINE)
                    }
                    Err(ImageFailure::Other(_)) => ImageState::Failed(Instant::now() + IMAGE_RETRY),
                };
                self.image_states.insert(url, state);
            }
            Message::Episodes {
                id,
                season,
                source,
                step,
            } => {
                if let Some(state) = self.episodes.get_mut(&(id, season))
                    && state.source == source
                {
                    match step {
                        Step::Stored { value, more } => {
                            state.loaded = Loaded::Ready(value);
                            state.loading = more;
                        }
                        Step::Fetched(Some(value)) => {
                            state.loaded = Loaded::Ready(value);
                            state.loading = false;
                        }
                        Step::Fetched(None) => {
                            if !matches!(state.loaded, Loaded::Ready(_)) {
                                state.loaded = Loaded::None;
                            }
                            state.loading = false;
                        }
                        Step::Failed(kind) => {
                            if !matches!(state.loaded, Loaded::Ready(_)) {
                                state.loaded = Loaded::Failed(kind);
                            }
                            state.loading = false;
                        }
                    }
                }
            }
            Message::Series { id, source, step } => {
                if let Some(state) = self.series.get_mut(&id)
                    && state.source == source
                {
                    match step {
                        Step::Stored { value, more } => {
                            state.loaded = Loaded::Ready(value);
                            state.loading = more;
                        }
                        Step::Fetched(Some(value)) => {
                            state.loaded = Loaded::Ready(value);
                            state.loading = false;
                            state.update_failed = false;
                        }
                        Step::Fetched(None) => {
                            if !matches!(state.loaded, Loaded::Ready(_)) {
                                state.loaded = Loaded::None;
                            }
                            state.loading = false;
                        }
                        Step::Failed(kind) => {
                            if matches!(state.loaded, Loaded::Ready(_)) {
                                state.update_failed = true;
                            } else {
                                state.loaded = Loaded::Failed(kind);
                            }
                            state.loading = false;
                        }
                    }
                }
            }
            Message::Tracks {
                id,
                source,
                outcome,
            } => {
                if let Some(state) = self.tracks.get_mut(&id)
                    && state.source == source
                {
                    state.loading = false;
                    state.loaded = match outcome {
                        Ok(tracks) if tracks.is_empty() => Loaded::None,
                        Ok(tracks) => Loaded::Ready(tracks),
                        Err(failure) => Loaded::Failed(failure.kind),
                    };
                }
            }
            Message::Platforms(result) => {
                self.platforms_loading = false;
                self.platforms = Some(result);
            }
            Message::Prune => return self.prune(library),
            Message::Pruned => {}
        }
        self.flush(false);
        Task::none()
    }

    /// Once per session: drops the records and files of entries and
    /// artists that are gone, and every image whose URL no record wants.
    fn prune(&mut self, library: &Library) -> Task<Message> {
        if self.pruned {
            return Task::none();
        }
        self.pruned = true;
        let ids: HashSet<&str> = library.entries.iter().map(|e| e.id.as_str()).collect();
        let names: HashSet<String> = library
            .entries
            .iter()
            .filter(|e| e.media == MediaType::Music)
            .map(|e| Artwork::artist_key(crate::music::artist_of(e)))
            .collect();
        let before = self.records.len() + self.artists.len();
        self.records.retain(|id, _| ids.contains(id.as_str()));
        self.artists.retain(|key, _| names.contains(key));
        if self.records.len() + self.artists.len() != before {
            self.mark_dirty();
        }
        self.flush(true);
        let keep: Vec<String> = self
            .records
            .values()
            .chain(self.artists.values())
            .filter_map(|r| r.url.clone())
            .collect();
        let sources = |test: &dyn Fn(&Entry) -> bool| -> Vec<String> {
            library
                .entries
                .iter()
                .filter(|e| test(e))
                .filter_map(|e| self.source_of(e))
                .collect()
        };
        let albums = sources(&|e| e.kind == Some(EntryKind::Album));
        let shows = sources(&Entry::uses_seasons);
        let series = sources(&|e| e.series.is_some());
        let images = self.images.clone();
        let stores = self.stores.clone();
        if stores.dir().is_none() {
            return Task::none();
        }
        Task::perform(
            crate::app::blocking(move || {
                images.prune(&keep);
                stores.prune("tracks", &albums);
                stores.prune("episodes", &shows);
                stores.prune("series", &series);
                Ok(())
            }),
            |_| Message::Pruned,
        )
    }
}

/// The cover URL and, for an entry matched on this computer, the id it was
/// matched to (the album's, for a song). A cover picked from another match
/// (`cover_from`) is asked of that match's provider; an entry without a
/// source is still matched for its id, which its track list and series go
/// by.
pub fn resolve_cover(service: &Service, entry: &Entry) -> Reply<(Option<String>, Option<String>)> {
    if let Some(cover_from) = &entry.cover_from {
        let url = match service.source_of(cover_from) {
            Some(provider) => provider.picked_cover_url(cover_from, entry.media)?,
            None => None,
        };
        if entry.source.is_some() {
            return Ok((url, None));
        }
        let plain = Entry {
            cover_from: None,
            ..entry.clone()
        };
        let (_, matched) = resolve_cover(service, &plain)?;
        return Ok((url, matched));
    }
    if let Some(source) = &entry.source {
        return match service.source_of(source) {
            Some(provider) => Ok((provider.cover_url(source, entry.media)?, None)),
            None => Ok((None, None)),
        };
    }
    let is_song = entry.media == MediaType::Music && entry.kind == Some(EntryKind::Song);
    let title = if is_song {
        entry.album.as_deref().unwrap_or(&entry.title)
    } else {
        &entry.title
    };
    for provider in service.sources_for(entry.media) {
        let year = if is_song { None } else { entry.year() };
        if let Some(found) =
            provider.match_title(title, entry.media, year, entry.creator.as_deref())?
        {
            return Ok((
                found.cache_cover_url().map(str::to_owned),
                Some(found.source),
            ));
        }
    }
    Ok((None, None))
}

/// An artist's photo from the music source; `None` when it has no artist
/// of that name, or no music source is set up.
pub fn resolve_artist(service: &Service, name: &str) -> Reply<Option<String>> {
    match service.music() {
        Some(music) => music.artist_photo_url(name),
        None => Ok(None),
    }
}

/// The id the episodes of `source` are read under. A lookup that cannot
/// say right now leaves it as it is.
fn season_source(service: &Service, source: &str) -> String {
    match service.source_of(source) {
        Some(lookup) if lookup.is_configured() => lookup
            .season_source(source)
            .unwrap_or_else(|_| source.to_owned()),
        _ => source.to_owned(),
    }
}

/// Whether a list fetched at `fetched` is kept without asking again: it
/// was fetched today, or every episode has a title and an air date and it
/// was fetched thirty days past the last one.
pub fn episodes_settled(fetched: Timestamp, episodes: &[Episode], now: Timestamp) -> bool {
    if fetched.local_date() >= now.local_date() {
        return true;
    }
    if episodes.is_empty() {
        return false;
    }
    let mut last = None;
    for episode in episodes {
        let (Some(_), Some(aired)) = (&episode.title, episode.aired) else {
            return false;
        };
        if last.is_none_or(|l| aired > l) {
            last = Some(aired);
        }
    }
    last.is_some_and(|last| fetched.local_date() >= last.add_days(EPISODES_SETTLE))
}

/// The phone's `EpisodeService.load`: the stored episodes first, then the
/// fetched ones unless the stored list is settled.
fn load_episodes(
    service: &Service,
    stores: &Stores,
    own: &str,
    season: u32,
    step: &mut dyn FnMut(Step<Vec<Episode>>),
) {
    let now = Timestamp::now();
    let source = season_source(service, own);
    let stored = stores.read_episodes(&source, season);
    let settled = stored
        .as_ref()
        .is_some_and(|s| episodes_settled(s.fetched, &s.episodes, now));
    if let Some(stored) = &stored {
        step(Step::Stored {
            value: stored.episodes.clone(),
            more: !settled,
        });
        if settled {
            return;
        }
    }
    let Some(lookup) = service.source_of(&source) else {
        if stored.is_none() {
            step(Step::Fetched(None));
        }
        return;
    };
    if !lookup.is_configured() {
        if stored.is_none() {
            step(Step::Failed(FailureKind::NotConfigured));
        }
        return;
    }
    match lookup.season_episodes(&source, season) {
        Err(failure) => {
            if stored.is_none() {
                step(Step::Failed(failure.kind));
            }
        }
        Ok(None) => {
            if stored.is_none() {
                step(Step::Fetched(None));
            }
        }
        Ok(Some(episodes)) if episodes.is_empty() => {
            if stored.is_none() {
                step(Step::Fetched(None));
            }
        }
        Ok(Some(episodes)) => {
            stores.write_episodes(&source, season, &episodes, now, lookup.name());
            step(Step::Fetched(Some(episodes)));
        }
    }
}

/// The phone's `SeriesService.load`: the stored parts first, then the
/// fetched ones once the list is a week old or was asked for another name.
fn load_series(
    service: &Service,
    stores: &Stores,
    source: &str,
    name: Option<&str>,
    step: &mut dyn FnMut(Step<SeriesParts>),
) {
    let now = Timestamp::now();
    let stored = stores.read_series(source);
    let fresh = stored
        .as_ref()
        .is_some_and(|s| now.0 - s.fetched.0 < SERIES_REFRESH && s.asked.as_deref() == name);
    if let Some(stored) = &stored {
        step(Step::Stored {
            value: stored.series.clone(),
            more: !fresh,
        });
        if fresh {
            return;
        }
    }
    let Some(lookup) = service.source_of(source) else {
        if stored.is_none() {
            step(Step::Fetched(None));
        }
        return;
    };
    if !lookup.is_configured() {
        if stored.is_none() {
            step(Step::Failed(FailureKind::NotConfigured));
        }
        return;
    }
    match lookup.series_parts(source, name) {
        Err(failure) => step(Step::Failed(failure.kind)),
        Ok(Some(parts)) if !parts.parts.is_empty() => {
            stores.write_series(source, &parts, now, name);
            step(Step::Fetched(Some(parts)));
        }
        Ok(_) => {
            if stored.is_none() {
                step(Step::Fetched(None));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Date, EntryStatus};

    fn entry(id: &str, title: &str, source: Option<&str>) -> Entry {
        Entry {
            id: id.into(),
            media: MediaType::Movies,
            title: title.into(),
            released: Some(crate::model::Released::parse("1999").unwrap()),
            creator: None,
            status: EntryStatus::Want,
            rating: None,
            tags: Vec::new(),
            tag_categories: None,
            note: String::new(),
            description: String::new(),
            seasonal_only: false,
            added: Date::parse("2026-10-01").unwrap(),
            source: source.map(str::to_owned),
            cover_from: None,
            platform: None,
            formats: None,
            pages: None,
            series: None,
            kind: None,
            album: None,
            tracks: None,
            seasons: None,
        }
    }

    #[test]
    fn the_record_key_follows_the_fields_that_change_a_cover() {
        let mut e = entry("movies/x-1999", "X", Some("tmdb:movie:1"));
        let key = Artwork::key_for(&e);
        assert_eq!(key, "X|1999|||tmdb:movie:1|");
        e.cover_from = Some("itunes:1".into());
        assert_ne!(Artwork::key_for(&e), key);
    }

    #[test]
    fn a_resolved_cover_is_recorded_and_a_changed_entry_asks_again() {
        let mut artwork = Artwork::at(None);
        let e = entry("movies/x-1999", "X", None);
        artwork.cover_resolved(
            e.id.clone(),
            Artwork::key_for(&e),
            Ok((Some("https://x/a.jpg".into()), Some("tmdb:movie:1".into()))),
        );
        assert_eq!(artwork.cover_url(&e), Some("https://x/a.jpg"));
        assert_eq!(artwork.source_of(&e).as_deref(), Some("tmdb:movie:1"));
        assert!(artwork.cover(&e).is_none(), "not on disk yet");
        let renamed = Entry {
            title: "Y".into(),
            ..e.clone()
        };
        assert_eq!(artwork.cover_url(&renamed), None);
        artwork.forget(&e.id);
        assert_eq!(artwork.cover_url(&e), None);
    }

    #[test]
    fn a_failed_lookup_is_not_recorded_and_waits_before_asking_again() {
        let mut artwork = Artwork::at(None);
        let e = entry("movies/x-1999", "X", None);
        artwork.cover_resolved(
            e.id.clone(),
            Artwork::key_for(&e),
            Err(FailureKind::Offline),
        );
        assert!(artwork.records.is_empty());
        artwork.want_cover(&e);
        assert!(artwork.queue.is_empty(), "not again right away");
        artwork.failures.clear();
        artwork.want_cover(&e);
        assert_eq!(artwork.queue.len(), 1);
    }

    #[test]
    fn the_fake_answers_wants_on_the_spot() {
        let library = crate::model::codec::decode(crate::fixtures::LIBRARY_JSON).unwrap();
        let service = Arc::new(Service::fake());
        let mut artwork = Artwork::at(None);
        let wants = Wants {
            covers: vec!["movies/the-mummy-1999".into()],
            series: vec!["books/berserk-vol-43-2025".into()],
            tracks: vec!["music/dirt-alice-in-chains".into()],
            platforms: true,
            ..Wants::default()
        };
        let _ = artwork.refresh(&wants, &library, &service);
        let berserk = library.entry("books/berserk-vol-43-2025").unwrap();
        assert!(matches!(
            &artwork.series_for(berserk).unwrap().loaded,
            Loaded::Ready(parts) if parts.parts.len() == 43
        ));
        let dirt = library.entry("music/dirt-alice-in-chains").unwrap();
        assert!(matches!(
            &artwork.tracks_for(dirt).unwrap().loaded,
            Loaded::Ready(tracks) if tracks.len() == 13
        ));
        assert!(artwork.platform_named("Super Nintendo").is_some());
        let mummy = library.entry("movies/the-mummy-1999").unwrap();
        assert!(
            artwork.records.contains_key(&mummy.id),
            "the fake has no Mummy, so a miss is recorded"
        );
        assert_eq!(artwork.cover_url(mummy), None);
    }

    #[test]
    fn episodes_settle_after_a_month_or_when_fetched_today() {
        let now = Timestamp::parse("2026-10-19T12:00:00Z").unwrap();
        let episode = |aired: &str, title: Option<&str>| Episode {
            number: 1,
            title: title.map(str::to_owned),
            aired: Date::parse(aired),
            runtime: None,
        };
        let today = Timestamp::parse("2026-10-19T08:00:00Z").unwrap();
        assert!(episodes_settled(today, &[], now));
        let old = Timestamp::parse("2026-09-01T08:00:00Z").unwrap();
        assert!(!episodes_settled(old, &[], now));
        assert!(episodes_settled(
            old,
            &[episode("2026-07-01", Some("Pilot"))],
            now
        ));
        assert!(!episodes_settled(
            old,
            &[episode("2026-08-20", Some("Pilot"))],
            now
        ));
        assert!(!episodes_settled(old, &[episode("2026-07-01", None)], now));
    }

    #[test]
    fn steps_update_the_states_and_a_failure_keeps_a_stored_list() {
        let library = crate::model::codec::decode(crate::fixtures::LIBRARY_JSON).unwrap();
        let mut artwork = Artwork::at(None);
        let id = "movies/x".to_owned();
        artwork.series.insert(
            id.clone(),
            SeriesState {
                source: "tmdb:movie:1".into(),
                asked: None,
                loaded: Loaded::Loading,
                update_failed: false,
                loading: true,
            },
        );
        let parts = SeriesParts {
            name: "X".into(),
            parts: Vec::new(),
        };
        let _ = artwork.update(
            Message::Series {
                id: id.clone(),
                source: "tmdb:movie:1".into(),
                step: Step::Stored {
                    value: parts.clone(),
                    more: true,
                },
            },
            &library,
        );
        let _ = artwork.update(
            Message::Series {
                id: id.clone(),
                source: "tmdb:movie:1".into(),
                step: Step::Failed(FailureKind::Offline),
            },
            &library,
        );
        let state = &artwork.series[&id];
        assert_eq!(state.loaded, Loaded::Ready(parts));
        assert!(state.update_failed && !state.loading);
        let _ = artwork.update(
            Message::Series {
                id: id.clone(),
                source: "other".into(),
                step: Step::Fetched(None),
            },
            &library,
        );
        assert!(
            matches!(artwork.series[&id].loaded, Loaded::Ready(_)),
            "another source's answer is dropped"
        );
    }
}
