//! Covers and the other things the lookups fetch for the
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
//! Image files live under `images/<sha1 of the url>.<format>`; a failed
//! download is not retried for six hours (thirty seconds when offline).
//!
//! One of the owner's own covers (`cover_from` naming a file in the
//! library repo, `covers`) is an image like the others, keyed by that
//! `repo:` value: it needs no lookup, and its file comes from GitHub or
//! is on this computer already, which the app sees to (`repo_wanted`,
//! `set_image`). A song shows the cover of its album when the album is
//! in the library and has one, whatever the song's own would be.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::Task;
use iced::widget::image;
use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::covers;
use crate::lookup::store::{
    self, IMAGE_RETRY, IMAGE_RETRY_OFFLINE, ImageFailure, ImageStore, JsonFile, Stores,
};
use crate::lookup::{
    self, Episode, FailureKind, GamePlatform, Reply, SeriesParts, Service, TrackInfo,
};
use crate::model::{Entry, EntryKind, Library, MediaType, Season};

/// How many cover and photo lookups run at once.
const CONCURRENCY: usize = 2;
/// How many image downloads run at once; the cover viewer's sharper copy
/// starts over that.
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
/// The source of a track list state for an album the fake has no match
/// for.
const FAKE_UNMATCHED: &str = "fake:unmatched";
/// And for one it answers for.
const FAKE_MATCHED: &str = "fake";
/// The session's prune runs this long after the start.
pub const PRUNE_DELAY: Duration = Duration::from_secs(15);

/// What the screens show right now, so the missing pieces are fetched:
/// entries whose covers are on screen, image
/// URLs the form shows (match thumbnails), and the entries whose series,
/// season or track list is open. `large` are the cover viewer's sharper
/// copies, fetched ahead of everything else and without waiting for a
/// free download; `sizes` the images whose width and height are read
/// from the file once it is here; `backdrops` the images a poster frame
/// shows (an album among posters), blurred once each (`blur`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wants {
    pub large: Vec<String>,
    pub sizes: Vec<String>,
    pub backdrops: Vec<String>,
    pub covers: Vec<String>,
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
    /// The entry's `cover_from` found no image and `url` is the match's
    /// own cover (`Found::nothing`).
    nothing: bool,
    resolved: Timestamp,
}

impl Record {
    fn from_json(json: &Value) -> Option<Record> {
        use crate::lookup::http::str_at;
        Some(Record {
            key: str_at(json, "key")?.to_owned(),
            url: str_at(json, "url").map(str::to_owned),
            source: str_at(json, "source").map(str::to_owned),
            nothing: json.get("cover_from_missing").and_then(Value::as_bool) == Some(true),
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
        if self.nothing {
            json["cover_from_missing"] = Value::Bool(true);
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

/// An album's track list as the details show it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackList<'a> {
    /// The lookup, or the match that gives the album its id, is on its
    /// way.
    Loading,
    /// The lookup or the match gave no answer: "Couldn't load the track
    /// list." with Retry.
    Failed,
    /// The album has no online match, so nothing is looked up: its
    /// tracks are the owner's own, in `Entry::tracks`.
    Unmatched,
    /// The album is matched and its source has no track list.
    Missing,
    Ready(&'a [TrackInfo]),
}

/// A track list state forced for a look (`LIBRI_TRACKS`, the captures),
/// on every album with an online match until Retry is pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcedTracks {
    Loading,
    Failed,
}

impl ForcedTracks {
    pub fn from_env(value: &str) -> Option<ForcedTracks> {
        match value {
            "loading" => Some(ForcedTracks::Loading),
            "failed" => Some(ForcedTracks::Failed),
            _ => None,
        }
    }
}

/// What a lookup task hands back.
#[derive(Debug, Clone)]
pub enum Message {
    CoverResolved {
        id: String,
        key: String,
        outcome: Result<Found, FailureKind>,
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
    /// An image's blurred copy for a poster frame, none when it did not
    /// decode.
    Blurred {
        url: String,
        backdrop: Option<image::Handle>,
    },
    /// The session's prune, due; the app answers it with `prune` and
    /// Discover's covers.
    Prune,
    /// The image files the prune removed, by `key_for` of their URLs.
    Pruned(HashSet<String>),
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
    failures: HashMap<String, (FailureKind, Instant)>,
    resolving: HashSet<String>,
    queue: Vec<Job>,
    image_states: HashMap<String, ImageState>,
    downloading: HashSet<String>,
    image_queue: Vec<String>,
    /// The owner's own covers the screens show that are not on this
    /// computer yet, by their `repo:` key; the app fetches them.
    repo_wanted: Vec<String>,
    /// The album entry each song shown belongs to (none when its album
    /// is not in the library), found once and kept until the library
    /// changes.
    albums: HashMap<String, Option<Entry>>,
    /// Images' widths and heights by URL, read once each (none when the
    /// header was not understood).
    sizes: HashMap<String, Option<(u32, u32)>>,
    /// The poster frames' blurred copies by URL, in memory only (none
    /// when the image did not decode), and the ones being made.
    backdrops: HashMap<String, Option<image::Handle>>,
    blurring: HashSet<String>,
    series: HashMap<String, SeriesState>,
    episodes: HashMap<(String, u32), EpisodesState>,
    tracks: HashMap<String, TracksState>,
    pub forced_tracks: Option<ForcedTracks>,
    /// A match was found, or the library changed, since `take_matches`.
    matches_changed: bool,
    platforms: Option<Reply<Vec<GamePlatform>>>,
    platforms_loading: bool,
    dirty: bool,
    changed_at: Option<Instant>,
    pruned: bool,
}

#[derive(Debug, Clone)]
enum Job {
    Entry(Box<Entry>),
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
            failures: HashMap::new(),
            resolving: HashSet::new(),
            queue: Vec::new(),
            image_states: HashMap::new(),
            downloading: HashSet::new(),
            image_queue: Vec::new(),
            repo_wanted: Vec::new(),
            albums: HashMap::new(),
            sizes: HashMap::new(),
            backdrops: HashMap::new(),
            blurring: HashSet::new(),
            series: HashMap::new(),
            episodes: HashMap::new(),
            tracks: HashMap::new(),
            forced_tracks: None,
            matches_changed: true,
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
        if lookup::http::int_at(&json, "version") != Some(VERSION) {
            self.records.retain(|_, r| r.url.is_some());
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

    /// The cover's image key when it is already known: the URL the
    /// lookup gave, or the `repo:` value of one of the owner's files. A
    /// song has its album's while the album is in the library with a
    /// cover.
    pub fn cover_url<'a>(&'a self, entry: &'a Entry) -> Option<&'a str> {
        if let Some(Some(album)) = self.albums.get(&entry.id)
            && let Some(url) = self.own_cover_url(album)
        {
            return Some(url);
        }
        self.own_cover_url(entry)
    }

    /// The entry's own cover, a song's album left aside.
    pub fn own_cover_url<'a>(&'a self, entry: &'a Entry) -> Option<&'a str> {
        if let Some(cover_from) = entry.cover_from.as_deref()
            && covers::repo_path(cover_from).is_some()
        {
            return Some(cover_from);
        }
        self.record_of(entry).and_then(|r| r.url.as_deref())
    }

    /// Whether the entry's `cover_from` is known to find no image: its
    /// source answered that it has none, and the match's own cover is
    /// shown. The form's next Save then removes the `cover_from`.
    pub fn cover_from_finds_nothing(&self, entry: &Entry) -> bool {
        self.record_of(entry).is_some_and(|r| r.nothing)
    }

    /// The album entry in the library the song `id` shows the cover of,
    /// once a screen has shown the song.
    pub fn album_of(&self, id: &str) -> Option<&Entry> {
        self.albums.get(id).and_then(Option::as_ref)
    }

    /// The library changed: which album a song belongs to is found
    /// again.
    pub fn library_changed(&mut self) {
        self.albums.clear();
        self.matches_changed = true;
    }

    /// The albums with no `source` whose match on this computer found
    /// one, with the id found, when a match or the library changed since
    /// the last call: the app writes them into the library, so every
    /// device reads the same match and a wrong one is corrected once, in
    /// Edit. The fake's matches are never kept.
    pub fn take_matches(&mut self, library: &Library) -> Vec<(String, String)> {
        if !std::mem::take(&mut self.matches_changed) {
            return Vec::new();
        }
        library
            .entries
            .iter()
            .filter(|e| e.media == MediaType::Music && !crate::music::is_song(e))
            .filter(|e| e.source.is_none())
            .filter_map(|e| Some((e.id.clone(), self.record_of(e)?.source.clone()?)))
            .filter(|(_, source)| !source.starts_with("fake:"))
            .collect()
    }

    /// The entry's match is in its `source` now: its record follows, so
    /// the cover it found is not looked for again.
    pub fn match_kept(&mut self, entry: &Entry) {
        if let Some(record) = self.records.get_mut(&entry.id) {
            record.key = Artwork::key_for(entry);
            record.source = None;
            self.mark_dirty();
        }
    }

    /// The owner's covers the screens asked for since the last call that
    /// are not on this computer.
    pub fn take_repo_wanted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.repo_wanted)
    }

    /// One of the owner's covers is on this computer, at `path`.
    pub fn set_image(&mut self, url: &str, path: PathBuf) {
        self.sizes.remove(url);
        self.backdrops.remove(url);
        self.image_states
            .insert(url.to_owned(), ImageState::Ready(path));
    }

    /// One of the owner's covers could not be fetched; it is asked for
    /// again after a while.
    pub fn set_image_failed(&mut self, url: &str) {
        self.image_states.insert(
            url.to_owned(),
            ImageState::Failed(Instant::now() + IMAGE_RETRY_OFFLINE),
        );
    }

    /// Forgets an image's file, size and blurred copy: the file was
    /// deleted, moved or replaced.
    pub fn forget_image(&mut self, url: &str) {
        self.image_states.remove(url);
        self.sizes.remove(url);
        self.backdrops.remove(url);
    }

    /// Forgets every one of the owner's covers, so each is looked for
    /// again (a new list came from GitHub).
    pub fn forget_repo_images(&mut self) {
        let is_repo = |url: &String| covers::repo_path(url).is_some();
        self.image_states.retain(|url, _| !is_repo(url));
        self.sizes.retain(|url, _| !is_repo(url));
        self.backdrops.retain(|url, _| !is_repo(url));
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

    /// Where an image is on this computer, once it is.
    pub fn image_path(&self, url: &str) -> Option<PathBuf> {
        match self.image_states.get(url) {
            Some(ImageState::Ready(path)) => Some(path.clone()),
            _ => None,
        }
    }

    /// An image's width and height, once it is on this computer and was
    /// wanted in `Wants::sizes`.
    pub fn image_size(&self, url: &str) -> Option<(u32, u32)> {
        self.sizes.get(url).copied().flatten()
    }

    /// An image's blurred copy for a poster frame, once it is made (it
    /// was wanted in `Wants::backdrops`).
    pub fn backdrop(&self, url: &str) -> Option<image::Handle> {
        self.backdrops.get(url).cloned().flatten()
    }

    /// Whether the image's download failed and is not to be tried again
    /// yet.
    pub fn image_failed(&self, url: &str) -> bool {
        matches!(self.image_states.get(url), Some(ImageState::Failed(until)) if Instant::now() < *until)
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

    /// The album's track list as the details show it. An album is
    /// without an online match once it has no `source` and the match on
    /// this computer was made and found none; a match that failed is a
    /// list that could not be loaded, never an album without one.
    pub fn track_list(&self, entry: &Entry) -> TrackList<'_> {
        let state = self.tracks.get(&entry.id);
        if self.source_of(entry).is_none() {
            if self.unmatched(entry) {
                return TrackList::Unmatched;
            }
            if self.failures.contains_key(&format!("entry:{}", entry.id)) {
                return TrackList::Failed;
            }
        }
        match (self.forced_tracks, state.map(|s| &s.loaded)) {
            (Some(ForcedTracks::Loading), _) => TrackList::Loading,
            (Some(ForcedTracks::Failed), _) => TrackList::Failed,
            (None, None | Some(Loaded::Loading)) => TrackList::Loading,
            (None, Some(Loaded::Failed(_))) => TrackList::Failed,
            (None, Some(Loaded::None)) => TrackList::Missing,
            (None, Some(Loaded::Ready(tracks))) => TrackList::Ready(tracks),
        }
    }

    /// The album has no `source` and the match on this computer found
    /// none.
    fn unmatched(&self, entry: &Entry) -> bool {
        entry.source.is_none()
            && match self.tracks.get(&entry.id) {
                // The fake answers on the spot and knows no such album.
                Some(state) if state.source == FAKE_UNMATCHED => true,
                Some(state) if state.source == FAKE_MATCHED => false,
                _ => self.record_of(entry).is_some_and(|r| r.source.is_none()),
            }
    }

    /// Retry on "Couldn't load the track list.": the failed lookup, or
    /// the failed match, is forgotten, so the next refresh asks again.
    pub fn retry_tracks(&mut self, id: &str) {
        self.forced_tracks = None;
        self.tracks.remove(id);
        self.failures.remove(&format!("entry:{id}"));
    }

    /// An album's track list as if a lookup had just answered.
    #[cfg(test)]
    pub fn set_tracks(&mut self, id: &str, tracks: Vec<TrackInfo>) {
        self.tracks.insert(
            id.to_owned(),
            TracksState {
                source: "test".into(),
                loaded: Loaded::Ready(tracks),
                loading: false,
            },
        );
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
        for url in &wants.large {
            self.want_image(url);
        }
        // The viewer is open on them: they start whatever else is
        // downloading.
        let urgent = self.image_queue.len();
        for id in &wants.covers {
            if let Some(entry) = library.entry(id) {
                self.want_cover(entry);
                // A song shows its album's cover.
                if crate::music::is_song(entry) && entry.media == MediaType::Music {
                    let album = self.albums.entry(id.clone()).or_insert_with(|| {
                        crate::music::album_of_song(&library.entries, entry).cloned()
                    });
                    if let Some(album) = album.clone() {
                        self.want_cover(&album);
                    }
                }
            }
        }
        for url in &wants.images {
            self.want_image(url);
        }
        for url in &wants.sizes {
            if !self.sizes.contains_key(url)
                && let Some(ImageState::Ready(path)) = self.image_states.get(url)
            {
                let size = lookup::store::image_size_of(path);
                self.sizes.insert(url.clone(), size);
            }
        }
        let mut tasks = Vec::new();
        for url in &wants.backdrops {
            tasks.extend(self.want_backdrop(url));
        }
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
        tasks.extend(self.start(service, urgent));
        self.flush(false);
        Task::batch(tasks)
    }

    fn want_cover(&mut self, entry: &Entry) {
        // One of the owner's files needs no lookup; an entry with no
        // source is still matched below, for its series and track list.
        if let Some(cover_from) = entry.cover_from.as_deref()
            && covers::repo_path(cover_from).is_some()
        {
            self.want_image(cover_from);
            if entry.source.is_some() {
                return;
            }
        }
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

    /// Starts the blurred copy of an image that is on this computer.
    fn want_backdrop(&mut self, url: &str) -> Option<Task<Message>> {
        if self.backdrops.contains_key(url) || self.blurring.contains(url) {
            return None;
        }
        let Some(ImageState::Ready(path)) = self.image_states.get(url) else {
            return None;
        };
        let path = path.clone();
        let url = url.to_owned();
        self.blurring.insert(url.clone());
        Some(Task::perform(
            crate::app::blocking(move || Ok(crate::blur::backdrop(&path))),
            move |result| Message::Blurred {
                url: url.clone(),
                backdrop: result.ok().flatten(),
            },
        ))
    }

    fn want_image(&mut self, url: &str) {
        match self.image_states.get(url) {
            Some(ImageState::Ready(_)) => return,
            Some(ImageState::Failed(until)) if Instant::now() < *until => return,
            _ => {}
        }
        // The owner's own covers are the app's to fetch, from GitHub.
        if covers::repo_path(url).is_some() {
            if !self.repo_wanted.iter().any(|wanted| wanted == url) {
                self.repo_wanted.push(url.to_owned());
            }
            return;
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

    /// Starts the queued lookups and downloads, within the limits; the
    /// first `urgent` downloads of the queue start over the limit.
    fn start(&mut self, service: &Arc<Service>, urgent: usize) -> Vec<Task<Message>> {
        let mut tasks = Vec::new();
        while self.resolving.len() < CONCURRENCY && !self.queue.is_empty() {
            let job = self.queue.remove(0);
            match job {
                Job::Entry(entry) => {
                    let id = entry.id.clone();
                    let key = Artwork::key_for(&entry);
                    if service.is_instant() {
                        let outcome = resolve(service, &entry).map_err(|f| f.kind);
                        self.cover_resolved(id, key, outcome);
                        continue;
                    }
                    self.resolving.insert(format!("entry:{id}"));
                    let service = service.clone();
                    tasks.push(Task::perform(
                        crate::app::blocking(move || Ok(resolve(&service, &entry))),
                        move |result| Message::CoverResolved {
                            id: id.clone(),
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
        let mut urgent = urgent;
        while !self.image_queue.is_empty() && (urgent > 0 || self.downloading.len() < DOWNLOADS) {
            urgent = urgent.saturating_sub(1);
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

    fn cover_resolved(&mut self, id: String, key: String, outcome: Result<Found, FailureKind>) {
        self.resolving.remove(&format!("entry:{id}"));
        match outcome {
            Ok(Found {
                url,
                source,
                nothing,
            }) => {
                self.failures.remove(&format!("entry:{id}"));
                self.matches_changed |= source.is_some();
                if let Some(url) = &url {
                    self.want_image(url);
                }
                self.records.insert(
                    id,
                    Record {
                        key,
                        url,
                        source,
                        nothing,
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
                // An album the fake has no list for and that was added
                // by hand has no online match.
                let source = if tracks.is_none() && entry.source.is_none() {
                    FAKE_UNMATCHED
                } else {
                    FAKE_MATCHED
                };
                TracksState {
                    source: source.into(),
                    loaded: tracks.map_or(Loaded::None, Loaded::Ready),
                    loading: false,
                }
            });
            return None;
        }
        let Some(source) = self.source_of(entry) else {
            // The match gives the id; it is asked for here too, so the
            // list never waits on a cover nothing shows.
            self.want_cover(entry);
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
                // The album's own catalog lists it (an album saved from
                // the extended search is Apple Music's), else the music
                // source.
                let Some(music) = service
                    .source_of(&album)
                    .filter(|s| s.is_configured())
                    .or_else(|| service.music())
                else {
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
            Message::Blurred { url, backdrop } => {
                self.blurring.remove(&url);
                self.backdrops.insert(url, backdrop);
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
            Message::Prune => return self.prune(library, &[]),
            Message::Pruned(removed) => self.pruned_images(&removed),
        }
        self.flush(false);
        Task::none()
    }

    /// The images to keep: each cover and photo the records want, the
    /// `extra` URLs (Discover's covers, which no entry has), the cover
    /// viewer's sharper copies of all of them, and every image this
    /// session has shown or is fetching, so nothing on screen loses its
    /// file.
    fn kept_images(&self, extra: &[String]) -> Vec<String> {
        let wanted = self
            .records
            .values()
            .filter_map(|r| r.url.clone())
            .chain(extra.iter().cloned())
            .flat_map(|url| {
                let large = lookup::viewer_urls(&url);
                std::iter::once(url).chain(large)
            });
        let session = self
            .image_states
            .keys()
            .chain(&self.downloading)
            .chain(&self.image_queue)
            .cloned();
        wanted.chain(session).collect()
    }

    /// The prune removed these files: an image that was fetched while it
    /// ran is fetched again rather than drawn from a file that is gone.
    fn pruned_images(&mut self, removed: &HashSet<String>) {
        if removed.is_empty() {
            return;
        }
        self.image_states.retain(|url, state| {
            !matches!(state, ImageState::Ready(_)) || !removed.contains(&store::key_for(url))
        });
    }

    /// Once per session: drops the records and files of entries and
    /// artists that are gone, and every image whose URL nothing wants
    /// (see `kept_images`).
    pub fn prune(&mut self, library: &Library, extra: &[String]) -> Task<Message> {
        if self.pruned {
            return Task::none();
        }
        self.pruned = true;
        let ids: HashSet<&str> = library.entries.iter().map(|e| e.id.as_str()).collect();
        let before = self.records.len();
        self.records.retain(|id, _| ids.contains(id.as_str()));
        if self.records.len() != before {
            self.mark_dirty();
        }
        self.flush(true);
        let keep = self.kept_images(extra);
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
                let removed = images.prune(&keep);
                stores.prune("tracks", &albums);
                stores.prune("episodes", &shows);
                stores.prune("series", &series);
                Ok(removed)
            }),
            |removed| Message::Pruned(removed.unwrap_or_default()),
        )
    }
}

/// What a lookup found for an entry's cover.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Found {
    /// The cover's URL.
    pub url: Option<String>,
    /// For an entry matched on this computer, the id it was matched to
    /// (the album's, for a song).
    pub source: Option<String>,
    /// The entry's `cover_from` finds no image: its source is set up and
    /// answered that it has none for that id, so `url` is the match's own
    /// cover. Never for one of the owner's files, a source this build
    /// does not know or has no key for, or a lookup that failed (that is
    /// an `Err`): only a real "no image" counts.
    pub nothing: bool,
}

impl Found {
    pub fn of(url: Option<String>, source: Option<String>) -> Found {
        Found {
            url,
            source,
            nothing: false,
        }
    }
}

/// The cover URL and, for an entry matched on this computer, the id it was
/// matched to (`resolve`, without the word on a `cover_from` that finds
/// nothing).
pub fn resolve_cover(service: &Service, entry: &Entry) -> Reply<(Option<String>, Option<String>)> {
    resolve(service, entry).map(|found| (found.url, found.source))
}

/// The entry's cover. A cover picked from another match (`cover_from`) is
/// asked of that match's provider, and one that finds no image there
/// falls back to the match's own cover (an anime's TMDB show without a
/// poster shows AniList's) and is said to find nothing; an entry without
/// a source is still matched for its id, which its track list and series
/// go by.
pub fn resolve(service: &Service, entry: &Entry) -> Reply<Found> {
    if let Some(cover_from) = &entry.cover_from {
        let provider = service.source_of(cover_from);
        let url = match provider {
            Some(provider) => provider.picked_cover_url(cover_from, entry.media)?,
            None => None,
        };
        // One of the owner's files is never looked up, and never
        // replaced by a lookup's cover.
        let falls_back = url.is_none() && covers::repo_path(cover_from).is_none();
        if entry.source.is_some() && !falls_back {
            return Ok(Found::of(url, None));
        }
        let plain = Entry {
            cover_from: None,
            ..entry.clone()
        };
        let own = resolve(service, &plain)?;
        return Ok(Found {
            url: url.or(own.url.filter(|_| falls_back)),
            source: own.source,
            nothing: falls_back && provider.is_some_and(|provider| provider.is_configured()),
        });
    }
    if let Some(source) = &entry.source {
        return match service.source_of(source) {
            Some(provider) => Ok(Found::of(provider.cover_url(source, entry.media)?, None)),
            None => Ok(Found::default()),
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
            return Ok(Found::of(
                found.cache_cover_url().map(str::to_owned),
                Some(found.source),
            ));
        }
    }
    Ok(Found::default())
}

/// An artist's photo from the music source, for the artist's Pick a
/// cover alone ("Another photo"): it is never shown on its own any more
/// (DESIGN.md "Artist photos"). `None` when the source has no artist of
/// that name, or no music source is set up.
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
    use crate::repo::Operation;

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
            tmdb: None,
            cover_from: None,
            cover_keep_mine: None,
            platform: None,
            formats: None,
            pages: None,
            series: None,
            kind: None,
            album: None,
            tracks: None,
            seasons_from: None,
            seasons: None,
        }
    }

    #[test]
    fn the_viewers_copies_are_kept_and_sizes_read_once_here() {
        let dir = crate::storage::TempDir::new("artwork-sizes");
        let mut artwork = Artwork::at(None);
        let e = entry("movies/x-1999", "X", Some("tmdb:movie:1"));
        let cached = "https://image.tmdb.org/t/p/w342/x.jpg";
        artwork.cover_resolved(
            e.id.clone(),
            Artwork::key_for(&e),
            Ok(Found::of(Some(cached.into()), None)),
        );
        let mut kept = artwork.kept_images(&[]);
        kept.sort();
        kept.dedup();
        assert_eq!(kept, [cached, "https://image.tmdb.org/t/p/w780/x.jpg"]);

        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend(342u32.to_be_bytes());
        png.extend(513u32.to_be_bytes());
        let path = dir.path("x.png");
        std::fs::write(&path, &png).unwrap();
        let library = Library::default();
        let service = Arc::new(Service::fake());
        let wants = Wants {
            sizes: vec![cached.into()],
            ..Wants::default()
        };
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.image_size(cached), None, "not here yet");
        let _ = artwork.update(
            Message::ImageFetched {
                url: cached.into(),
                outcome: Ok(path),
            },
            &library,
        );
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.image_size(cached), Some((342, 513)));
    }

    #[test]
    fn the_prune_keeps_discovers_covers_and_what_this_session_shows() {
        let dir = crate::storage::TempDir::new("artwork-prune");
        let mut artwork = Artwork::at(None);
        let library = Library::default();
        let discover = "https://image.tmdb.org/t/p/w342/discover.jpg";
        let thumbnail = "https://example.org/match.jpg";
        let path = dir.path("match.jpg");
        std::fs::write(&path, b"\xFF\xD8\xFF").unwrap();
        let _ = artwork.update(
            Message::ImageFetched {
                url: thumbnail.into(),
                outcome: Ok(path),
            },
            &library,
        );
        let kept = artwork.kept_images(&[discover.into()]);
        assert!(kept.iter().any(|u| u == discover), "a Discover cover");
        assert!(
            kept.iter()
                .any(|u| u == "https://image.tmdb.org/t/p/w780/discover.jpg"),
            "and its sharper copy"
        );
        assert!(
            kept.iter().any(|u| u == thumbnail),
            "an image already shown"
        );

        // A file the prune took anyway is fetched again, not drawn from
        // a path that is gone.
        assert!(artwork.image(thumbnail).is_some());
        let _ = artwork.update(
            Message::Pruned(HashSet::from([store::key_for(thumbnail)])),
            &library,
        );
        assert!(artwork.image(thumbnail).is_none());
    }

    #[test]
    fn a_track_list_ends_as_failed_or_unmatched_instead_of_loading_for_good() {
        let mut library = Library::default();
        let mut album = entry("music/live-adele", "Live", None);
        album.media = MediaType::Music;
        library.entries.push(album.clone());
        // Not the fake; the tasks are dropped unrun.
        let service = Arc::new(Service::with(Vec::new(), false));
        let mut artwork = Artwork::at(None);
        let wants = Wants {
            tracks: vec![album.id.clone()],
            ..Wants::default()
        };
        // The match that gives the album its id is asked for.
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.track_list(&album), TrackList::Loading);
        assert!(artwork.resolving.contains("entry:music/live-adele"));

        // It failed: the list could not be loaded, and Retry asks again.
        let key = Artwork::key_for(&album);
        artwork.cover_resolved(album.id.clone(), key.clone(), Err(FailureKind::Offline));
        assert_eq!(artwork.track_list(&album), TrackList::Failed);
        artwork.retry_tracks(&album.id);
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.track_list(&album), TrackList::Loading);
        assert!(artwork.resolving.contains("entry:music/live-adele"));

        // It found nothing: the album has no online match.
        artwork.cover_resolved(album.id.clone(), key.clone(), Ok(Found::default()));
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.track_list(&album), TrackList::Unmatched);

        // It found the album: its list is asked for, and a failure there
        // is a list that could not be loaded too.
        let source = "deezer:album:1".to_owned();
        artwork.cover_resolved(
            album.id.clone(),
            key,
            Ok(Found::of(None, Some(source.clone()))),
        );
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.track_list(&album), TrackList::Loading);
        let _ = artwork.update(
            Message::Tracks {
                id: album.id.clone(),
                source,
                outcome: Err(lookup::Failure::new(FailureKind::Offline, "")),
            },
            &library,
        );
        assert_eq!(artwork.track_list(&album), TrackList::Failed);
        artwork.retry_tracks(&album.id);
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.track_list(&album), TrackList::Loading);
    }

    #[test]
    fn an_albums_match_is_handed_over_once_and_its_record_follows() {
        let mut library = Library::default();
        let mut album = entry("music/live-adele", "Live", None);
        album.media = MediaType::Music;
        let mut movie = entry("movies/heat-1995", "Heat", None);
        movie.media = MediaType::Movies;
        library.entries = vec![album.clone(), movie.clone()];
        let mut artwork = Artwork::at(None);
        assert!(artwork.take_matches(&library).is_empty());

        // Both are matched; only the album's match is kept.
        let found = |n: u32| {
            Ok(Found::of(
                Some(format!("https://e.test/{n}.jpg")),
                Some(format!("deezer:album:{n}")),
            ))
        };
        artwork.cover_resolved(album.id.clone(), Artwork::key_for(&album), found(1));
        artwork.cover_resolved(movie.id.clone(), Artwork::key_for(&movie), found(2));
        assert_eq!(
            artwork.take_matches(&library),
            [(album.id.clone(), "deezer:album:1".to_owned())]
        );
        assert!(artwork.take_matches(&library).is_empty(), "asked once");

        // With the id in the entry the cover found stays the entry's.
        Operation::SetSource {
            id: album.id.clone(),
            source: "deezer:album:1".into(),
        }
        .apply_to(&mut library)
        .unwrap();
        let kept = library.entry(&album.id).unwrap();
        artwork.match_kept(kept);
        artwork.library_changed();
        assert_eq!(artwork.cover_url(kept), Some("https://e.test/1.jpg"));
        assert!(artwork.take_matches(&library).is_empty());

        // A match the owner picked is never replaced.
        Operation::SetSource {
            id: album.id.clone(),
            source: "deezer:album:9".into(),
        }
        .apply_to(&mut library)
        .unwrap();
        assert_eq!(
            library.entry(&album.id).unwrap().source.as_deref(),
            Some("deezer:album:1")
        );
    }

    #[test]
    fn the_viewers_copy_starts_over_the_download_limit() {
        let library = Library::default();
        // Not the fake, whose images are never fetched; the tasks are
        // dropped unrun.
        let service = Arc::new(Service::with(Vec::new(), false));
        let mut artwork = Artwork::at(None);
        let cover = |n: usize| format!("https://image.tmdb.org/t/p/w342/{n}.jpg");
        let mut wants = Wants {
            images: (0..6).map(cover).collect(),
            ..Wants::default()
        };
        let _ = artwork.refresh(&wants, &library, &service);
        assert_eq!(artwork.downloading.len(), DOWNLOADS);
        assert_eq!(artwork.image_queue, [cover(4), cover(5)]);

        let large = "https://image.tmdb.org/t/p/w780/0.jpg";
        wants.large.push(large.into());
        let _ = artwork.refresh(&wants, &library, &service);
        assert!(artwork.downloading.contains(large));
        assert_eq!(artwork.downloading.len(), DOWNLOADS + 1);
        assert_eq!(artwork.image_queue, [cover(4), cover(5)], "the rest wait");
    }

    /// A lookup that knows one cover, or none, or does not answer.
    struct OneCover(&'static str, Reply<Option<&'static str>>);

    impl crate::lookup::Source for OneCover {
        fn id(&self) -> &'static str {
            self.0
        }

        fn name(&self) -> &'static str {
            self.0
        }

        fn types(&self) -> &'static [MediaType] {
            &MediaType::ALL
        }

        fn search(&self, _: &str, _: MediaType, _: usize) -> Reply<crate::lookup::Hits> {
            Ok(crate::lookup::Hits::of(Vec::new()))
        }

        fn cover_url(&self, _: &str, _: MediaType) -> Reply<Option<String>> {
            self.1.clone().map(|url| url.map(str::to_owned))
        }

        fn match_title(
            &self,
            _: &str,
            _: MediaType,
            _: Option<u16>,
            _: Option<&str>,
        ) -> Reply<Option<crate::lookup::Match>> {
            Ok(None)
        }
    }

    #[test]
    fn an_animes_cover_from_shows_its_tmdb_poster_and_falls_back_to_the_matchs() {
        let (poster, own) = (
            "https://image.tmdb.org/t/p/w342/frieren.jpg",
            "https://s4.anilist.co/file/anilistcdn/media/anime/cover/large/bx154587.jpg",
        );
        let service = |poster: Option<&'static str>| {
            Service::with(
                vec![
                    std::sync::Arc::new(OneCover("tmdb", Ok(poster))),
                    std::sync::Arc::new(OneCover("anilist", Ok(Some(own)))),
                ],
                true,
            )
        };
        // Frieren as the phone's standing check leaves it.
        let mut frieren = entry(
            "anime/frieren-beyond-journeys-end-2023",
            "Frieren: Beyond Journey's End",
            Some("anilist:154587"),
        );
        frieren.media = MediaType::Anime;
        frieren.tmdb = Some("tmdb:tv:209867".into());
        frieren.cover_from = frieren.tmdb.clone();
        let url = |service: &Service, entry: &Entry| resolve_cover(service, entry).unwrap().0;
        assert_eq!(
            url(&service(Some(poster)), &frieren).as_deref(),
            Some(poster)
        );
        assert!(!resolve(&service(Some(poster)), &frieren).unwrap().nothing);
        // A show without a poster: the match's own cover, and the word
        // that the `cover_from` finds nothing, which the next Save acts on.
        assert_eq!(url(&service(None), &frieren).as_deref(), Some(own));
        assert!(resolve(&service(None), &frieren).unwrap().nothing);
        // No answer is not "no image": nothing is shown and nothing said.
        let offline = Service::with(
            vec![
                std::sync::Arc::new(OneCover(
                    "tmdb",
                    Err(lookup::Failure::new(FailureKind::Offline, "no network")),
                )),
                std::sync::Arc::new(OneCover("anilist", Ok(Some(own)))),
            ],
            true,
        );
        assert!(resolve(&offline, &frieren).is_err());
        // A source this build does not know: the match's cover, and the
        // `cover_from` stays.
        let unknown = Entry {
            cover_from: Some("elsewhere:1".into()),
            ..frieren.clone()
        };
        let found = resolve(&service(Some(poster)), &unknown).unwrap();
        assert_eq!((found.url.as_deref(), found.nothing), (Some(own), false));
        // AniList's cover picked, stored under the entry's own id.
        frieren.cover_from = frieren.source.clone();
        assert_eq!(url(&service(Some(poster)), &frieren).as_deref(), Some(own));
        // One of the owner's files is never looked up.
        frieren.cover_from = Some("repo:anime/frieren-beyond-journeys-end-2023.jpg".into());
        assert_eq!(url(&service(Some(poster)), &frieren), None);
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
            Ok(Found::of(
                Some("https://x/a.jpg".into()),
                Some("tmdb:movie:1".into()),
            )),
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
            Loaded::Ready(parts) if parts.parts.len() == 44
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
