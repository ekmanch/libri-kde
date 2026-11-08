//! Discover: titles not in the library that the daily check finds for
//! things that are (decided 2026-11-07; the phone's DESIGN.md 10c has the
//! checks it follows): the next season of a show, the next part of a
//! followed series (a part Done or In progress), a new book by an author
//! in the library and a new album by an album artist. Each item carries
//! its type, title, an optional sub ("Season 3"), the creator, its release
//! as precise as the provider has it, a "why" line, a description, the
//! provider and the lookup id.
//!
//! The check runs as the phone's scheduler does (its DESIGN.md 10c,
//! `upcoming_scheduler.dart`), over what `settings.json` follows: a little
//! after the start, then on the hour's timer until it has finished that
//! day. What the owner is waiting on is asked daily; everything else takes
//! its turn in one of seven weekly shares, by a stable hash of its id, and
//! a first run asks all seven. A creator's provider id is kept for ninety
//! days and a series' parts for a week, so most of a run asks nothing
//! twice. What was found stays until its owner is asked again, and each
//! item asked is written as it is done, so a close or a provider that
//! refuses loses nothing and the next run takes up where this one stopped.
//! A provider offline or refusing is left alone for the rest of the run;
//! the others go on. All of it is kept in `discover.json` under the cache
//! folder, with the time of the check and the providers asked, so the page
//! shows at once and the footer can say "Checked today 09:12 · TMDB, IGDB, Open Library,
//! Deezer".
//! Items already in the library (the same lookup id, a season the show
//! now has, or the same title) are left out when the page is drawn, so
//! an entry added on the phone drops its item on the next sync.
//!
//! "Not interested" hides an item by its key (the phone's
//! `upcoming.not_interested` of `settings.json`: a lookup id, or
//! `<show id>#<season>` for a season). The keys are kept in
//! `settings.json` beside `library.json`, in the phone's shape, so the
//! `settings.json` sync of TODO.md phase 4 syncs them with the phone;
//! until then the file is this computer's.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::connection::Target;
use crate::github;
use crate::lookup::store::Stores;
use crate::lookup::{self, FailureKind, FollowKind, Match, Reply, SeriesParts, Service, Source};
use crate::model::{
    Date, Entry, EntryStatus, Library, MediaType, Released, Series, tag_display_name,
};
use crate::storage;

/// Where an item came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Kind {
    /// The next season of the show with this id.
    Season { show: String, number: u32 },
    /// The next part of a series in the library.
    Part { series: String },
    /// New work by a creator in the library.
    Creator,
}

/// One title on its way that is not in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub media: MediaType,
    pub title: String,
    /// "Season 3".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub released: Option<Released>,
    /// "Part of The Lord of the Rings · 2 in your library".
    pub why: String,
    #[serde(default)]
    pub description: String,
    /// What the hidden list goes by: the lookup id, or "<show id>#<n>"
    /// for a season.
    pub key: String,
    /// The lookup id of the title; a season's is its show's.
    pub source: String,
    /// The provider's name: "TMDB".
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// The day the check first listed it.
    pub found: Date,
    /// The follow it came from ("shows/severance-2022", "authors/stephen-king",
    /// "movies/halo" under series): it stays until that one is asked again.
    #[serde(default)]
    pub owner: String,
    #[serde(flatten)]
    pub kind: Kind,
}

impl Found {
    /// "Severance · Season 3", or the title alone.
    pub fn full_title(&self) -> String {
        match &self.sub {
            Some(sub) => format!("{} · {sub}", self.title),
            None => self.title.clone(),
        }
    }

    /// Whether the item is spoken for in `library`: its lookup id is on
    /// an entry, the show has the season by now, or an entry of the type
    /// has its title (added by hand).
    pub fn in_library(&self, library: &Library) -> bool {
        self.in_owned(&Owned::of(library))
    }

    fn in_owned(&self, owned: &Owned<'_>) -> bool {
        match &self.kind {
            Kind::Season { show, number } => owned
                .library
                .entry(show)
                .is_none_or(|e| e.season_list().iter().any(|s| s.season == *number)),
            Kind::Part { .. } | Kind::Creator => {
                owned.sources.contains(self.source.as_str())
                    || owned
                        .titles
                        .contains(&(self.media, lookup::text::title_key(&self.title)))
            }
        }
    }
}

/// The library's lookup ids and titles (as `lookup::text::same_title`
/// compares them), gathered once so each item asks a set: the page is
/// drawn on every frame of a slide, and asking every entry for every item
/// took 4 ms a frame on a library of a thousand entries (2026-11-07).
struct Owned<'a> {
    library: &'a Library,
    sources: HashSet<&'a str>,
    titles: HashSet<(MediaType, String)>,
}

impl<'a> Owned<'a> {
    fn of(library: &'a Library) -> Owned<'a> {
        Owned {
            library,
            sources: library
                .entries
                .iter()
                .filter_map(|e| e.source.as_deref())
                .collect(),
            titles: library
                .entries
                .iter()
                .map(|e| (e.media, lookup::text::title_key(&e.title)))
                .collect(),
        }
    }
}

/// What a check came back with.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub items: Vec<Found>,
    /// The providers asked, in order, by name.
    pub sources: Vec<String>,
    /// False when a provider was offline, busy or down, or the window
    /// closed, so the day is not recorded and a later run goes on.
    pub complete: bool,
    /// Why an incomplete run stopped, printed to stderr: "Open Library:
    /// Offline: …", or "the window closed".
    pub stopped: Option<String>,
    /// The record as the run left it; `None` when it asked nothing.
    pub record: Option<Record>,
}

impl Outcome {
    /// A run that gave nothing, for a task that did not finish.
    pub fn failed(why: impl Into<String>) -> Outcome {
        Outcome {
            items: Vec::new(),
            sources: Vec::new(),
            complete: false,
            stopped: Some(why.into()),
            record: None,
        }
    }
}

/// What `settings.json` says about what to follow (the phone's
/// `AppSettings`; only the keys Discover reads).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub languages: Vec<String>,
    unfollowed: BTreeMap<String, BTreeSet<String>>,
    followed: BTreeMap<String, BTreeSet<String>>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            enabled: true,
            languages: vec!["en".to_owned()],
            unfollowed: BTreeMap::new(),
            followed: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// The file's text; another version or a broken file is the defaults.
    pub fn parse(text: &str) -> Settings {
        let mut settings = Settings::default();
        let Ok(json) = serde_json::from_str::<Value>(text) else {
            return settings;
        };
        if json.get("version").and_then(Value::as_u64) != Some(1) {
            return settings;
        }
        let ids = |value: Option<&Value>| -> BTreeMap<String, BTreeSet<String>> {
            value
                .and_then(Value::as_object)
                .map(|kinds| {
                    kinds
                        .iter()
                        .map(|(kind, list)| {
                            let list = list.as_array().into_iter().flatten();
                            let list = list.filter_map(Value::as_str).map(str::to_owned);
                            (kind.clone(), list.collect())
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let upcoming = json.get("upcoming");
        if let Some(enabled) = upcoming
            .and_then(|u| u.get("enabled"))
            .and_then(Value::as_bool)
        {
            settings.enabled = enabled;
        }
        settings.unfollowed = ids(upcoming.and_then(|u| u.get("unfollowed")));
        settings.followed = ids(upcoming.and_then(|u| u.get("followed")));
        if let Some(languages) = json["books"]["languages"].as_array() {
            settings.languages = languages
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_lowercase)
                .collect();
        }
        settings
    }

    fn listed(map: &BTreeMap<String, BTreeSet<String>>, kind: &str, id: &str) -> bool {
        map.get(kind).is_some_and(|ids| ids.contains(id))
    }

    /// On by default unless unfollowed; off by default unless followed.
    pub fn is_followed(&self, kind: &str, id: &str, default_on: bool) -> bool {
        if default_on {
            !Settings::listed(&self.unfollowed, kind, id)
        } else {
            Settings::listed(&self.followed, kind, id)
        }
    }

    /// Ticked although it is off by default.
    pub fn is_ticked(&self, kind: &str, id: &str) -> bool {
        Settings::listed(&self.followed, kind, id)
    }
}

/// `settings.json` in the folder of the library file: "settings.json" for
/// "library.json", "data/settings.json" for "data/library.json".
fn settings_path(library_path: &str) -> String {
    match library_path.rsplit_once('/') {
        Some((folder, _)) => format!("{folder}/settings.json"),
        None => "settings.json".to_owned(),
    }
}

/// `text`, with the Not interested keys of the copy on this computer added:
/// Hide on the desktop has nowhere to go but that copy until `settings.json`
/// syncs both ways.
fn with_local_hidden(text: &str, local: &str) -> Option<String> {
    let mut json: Value = serde_json::from_str(text).ok()?;
    let keys = |json: &Value| -> Vec<String> {
        json["upcoming"]["not_interested"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let local: Value = serde_json::from_str(local).unwrap_or(Value::Null);
    let mut merged: BTreeSet<String> = keys(&json).into_iter().collect();
    merged.extend(keys(&local));
    if !merged.is_empty() {
        let upcoming = json
            .as_object_mut()?
            .entry("upcoming")
            .or_insert_with(|| json!({}));
        upcoming
            .as_object_mut()?
            .insert("not_interested".into(), json!(merged));
    }
    let mut text = serde_json::to_string_pretty(&json).ok()?;
    text.push('\n');
    Some(text)
}

/// What to follow: GitHub's `settings.json` while connected, kept in `file`
/// for offline and for Not interested; else that copy. A file missing on
/// GitHub is the defaults. An error with no copy at all stops the run,
/// since the defaults would follow everything the owner unticked.
pub fn read_settings(
    remote: Option<(&github::Client, &Target)>,
    file: Option<&Path>,
) -> Result<Settings, String> {
    let local = file.and_then(|path| std::fs::read_to_string(path).ok());
    let Some((client, target)) = remote else {
        return Ok(local.as_deref().map(Settings::parse).unwrap_or_default());
    };
    match client.file_raw(&settings_path(&target.file), &target.branch, None) {
        Ok(Some(raw)) => {
            let text = raw.text().map_err(|error| error.to_string())?.to_owned();
            if let Some(path) = file
                && let Some(merged) = with_local_hidden(&text, local.as_deref().unwrap_or("null"))
                && let Err(error) = storage::write_atomically(path, &merged)
            {
                eprintln!("libri: {} could not be written: {error}", path.display());
            }
            Ok(Settings::parse(&text))
        }
        Ok(None) => Ok(local.as_deref().map(Settings::parse).unwrap_or_default()),
        Err(failure) if failure.kind == github::FailureKind::NotFound => {
            Ok(local.as_deref().map(Settings::parse).unwrap_or_default())
        }
        Err(failure) => match local {
            Some(text) => Ok(Settings::parse(&text)),
            None => Err(format!("settings.json: {}", failure.describe())),
        },
    }
}

/// Which of the seven weekly shares `id` is in: the phone's hash of its
/// UTF-16 code units, so an item is asked on the same weekday on both.
pub fn share_of(id: &str) -> u32 {
    let mut hash: u32 = 0;
    for unit in id.encode_utf16() {
        hash = (hash.wrapping_mul(31).wrapping_add(u32::from(unit))) & 0x7fff_ffff;
    }
    hash % 7
}

/// The share whose turn it is on `day`.
fn share_on(day: Date) -> u32 {
    let epoch = Date::parse("2026-01-01").expect("a date");
    epoch.days_until(day).rem_euclid(7) as u32
}

/// The shares due on `today`: today's and those of the days missed since
/// the last completed run. A first run, or a week away, asks all seven.
pub fn due_shares(last_run: Option<Date>, today: Date) -> BTreeSet<u32> {
    let Some(last_run) = last_run else {
        return (0..7).collect();
    };
    let missed = last_run.days_until(today);
    if missed >= 7 {
        return (0..7).collect();
    }
    (0..missed).map(|i| share_on(today.add_days(-i))).collect()
}

/// How long a creator's provider id and activity are trusted.
const STATUS_DAYS: i64 = 90;
/// How many names one Wikidata query asks about.
const INACTIVE_BATCH: usize = 50;
/// How long after its listed release a book part is still asked for its
/// edition in the languages read.
const EDITION_WINDOW: i64 = 365;
/// How old a stored series list may be before it is asked for again, and
/// how old one with a part still to come.
const SERIES_REFRESH: i64 = 7 * 86_400;
const SERIES_UPCOMING_REFRESH: i64 = 86_400;

/// Set when the window closes: a run under way stops before its next
/// request, so the exit never waits for the rest of the check (tokio
/// waits for blocking tasks when it shuts down).
static CANCELLED: AtomicBool = AtomicBool::new(false);

pub fn cancel() {
    CANCELLED.store(true, Ordering::Relaxed);
}

fn cancelled() -> bool {
    CANCELLED.load(Ordering::Relaxed)
}

/// The messages of the check.
#[derive(Debug, Clone)]
pub enum Message {
    /// Runs the check, unless it ran today or is running.
    Check,
    /// The check came back.
    Done(Box<Outcome>),
}

/// The record of the last check: `discover.json`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Record {
    pub checked: Option<Timestamp>,
    pub sources: Vec<String>,
    pub items: Vec<Found>,
    /// The last day every check finished, so the shares of the days since
    /// are caught up.
    pub last_run: Option<Date>,
    /// The day each of the three checks last finished.
    pub done: BTreeMap<String, Date>,
    /// What was asked today, so a run after an unfinished one goes on from
    /// there.
    pub asked: BTreeSet<String>,
    pub asked_on: Option<Date>,
    /// What each creator is to their provider, by follow key.
    pub creators: BTreeMap<String, CreatorStatus>,
}

/// A creator's provider id and whether they are still active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatorStatus {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub inactive: Option<String>,
    pub checked_on: Date,
}

const RECORD_VERSION: u64 = 1;

impl Record {
    fn from_json(json: &Value) -> Option<Record> {
        if json.get("version").and_then(Value::as_u64) != Some(RECORD_VERSION) {
            return None;
        }
        let items = json
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Some(Record {
            checked: json
                .get("checked")
                .and_then(Value::as_str)
                .and_then(Timestamp::parse),
            sources: json
                .get("sources")
                .and_then(Value::as_array)
                .map(|s| {
                    s.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            items,
            last_run: json
                .get("last_run")
                .and_then(Value::as_str)
                .and_then(Date::parse),
            done: serde_json::from_value(json["done"].clone()).unwrap_or_default(),
            asked: serde_json::from_value(json["asked"].clone()).unwrap_or_default(),
            asked_on: json
                .get("asked_on")
                .and_then(Value::as_str)
                .and_then(Date::parse),
            creators: serde_json::from_value(json["creators"].clone()).unwrap_or_default(),
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "version": RECORD_VERSION,
            "checked": self.checked.map(Timestamp::to_rfc3339),
            "sources": self.sources,
            "items": self.items,
            "last_run": self.last_run.map(|d| d.to_string()),
            "done": self.done,
            "asked": self.asked,
            "asked_on": self.asked_on.map(|d| d.to_string()),
            "creators": self.creators,
        })
    }
}

/// The keys of the items marked Not interested, and the file they live in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hidden {
    keys: BTreeSet<String>,
    path: Option<PathBuf>,
}

impl Hidden {
    /// Reads `upcoming.not_interested` from the phone's `settings.json`
    /// at `path`.
    fn at(path: PathBuf) -> Hidden {
        let keys = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|json| {
                json.get("upcoming")?
                    .get("not_interested")?
                    .as_array()
                    .map(|keys| {
                        keys.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
            })
            .unwrap_or_default();
        Hidden {
            keys,
            path: Some(path),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Writes the keys into `settings.json`, keeping whatever else the file
    /// holds: the key is written only while the list has something, as
    /// the phone writes it, and a file with nothing else is `{"version": 1}`.
    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let mut json = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({"version": 1}));
        let upcoming = json
            .as_object_mut()
            .expect("an object")
            .entry("upcoming")
            .or_insert_with(|| json!({}));
        if !upcoming.is_object() {
            *upcoming = json!({});
        }
        let map = upcoming.as_object_mut().expect("an object");
        if self.keys.is_empty() {
            map.remove("not_interested");
        } else {
            map.insert("not_interested".into(), json!(self.keys));
        }
        if map.is_empty() {
            json.as_object_mut().expect("an object").remove("upcoming");
        }
        let mut text = serde_json::to_string_pretty(&json).expect("settings encode");
        text.push('\n');
        if let Err(error) = storage::write_atomically(path, &text) {
            eprintln!("libri: {} could not be written: {error}", path.display());
        }
    }
}

/// The check's state on the app: the record, the hidden keys, whether a
/// run is out, and when the last run started.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub record: Record,
    pub hidden: Hidden,
    file: Option<PathBuf>,
    pub running: bool,
}

impl State {
    /// The record under `cache` and the hidden keys beside the library
    /// under `data`; nothing kept when either is `None` (the tests).
    pub fn at(cache: Option<&Path>, data: Option<&Path>) -> State {
        let file = cache.map(|dir| dir.join("discover.json"));
        let record = file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|json| Record::from_json(&json))
            .unwrap_or_default();
        State {
            record,
            hidden: data.map_or_else(Hidden::default, |dir| Hidden::at(dir.join("settings.json"))),
            file,
            running: false,
        }
    }

    /// Whether the check ran on today's date (this computer's clock).
    pub fn checked_today(&self, now: Timestamp) -> bool {
        self.record
            .checked
            .is_some_and(|checked| checked.local_date() == now.local_date())
    }

    /// Whether a run should start now: none out and none completed today.
    pub fn is_due(&self, now: Timestamp) -> bool {
        !self.running && !self.checked_today(now)
    }

    /// A run starts now.
    pub fn start(&mut self) {
        self.running = true;
    }

    /// The items Discover lists: not in the library, not hidden.
    pub fn visible(&self, library: &Library) -> Vec<&Found> {
        self.lists(library).0
    }

    /// The hidden items still worth restoring: not in the library.
    pub fn hidden_items(&self, library: &Library) -> Vec<&Found> {
        self.lists(library).1
    }

    /// Both at once, the library gathered once: what `visible` and
    /// `hidden_items` return.
    pub fn lists(&self, library: &Library) -> (Vec<&Found>, Vec<&Found>) {
        let owned = Owned::of(library);
        self.record
            .items
            .iter()
            .filter(|item| !item.in_owned(&owned))
            .partition(|item| !self.hidden.contains(&item.key))
    }

    pub fn item(&self, key: &str) -> Option<&Found> {
        self.record.items.iter().find(|item| item.key == key)
    }

    /// Not interested: the key goes on the hidden list, which is saved.
    pub fn hide(&mut self, key: &str) {
        if self.hidden.keys.insert(key.to_owned()) {
            self.hidden.save();
        }
    }

    /// Restore: the key leaves the hidden list.
    pub fn restore(&mut self, key: &str) {
        if self.hidden.keys.remove(key) {
            self.hidden.save();
        }
    }

    /// The check came back: the record as it left it, stamped when the run
    /// was complete. `settings.json` may have come with it, with its Not
    /// interested keys.
    pub fn finish(&mut self, outcome: Outcome, now: Timestamp) {
        self.running = false;
        if let Some(path) = self.hidden.path.clone() {
            self.hidden = Hidden::at(path);
        }
        let Some(mut record) = outcome.record else {
            return;
        };
        if outcome.complete {
            record.checked = Some(now);
        }
        self.record = record;
        write_record(&self.record, self.file.as_deref());
    }

    /// Where the record is kept, for the run to write as it goes.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// The copy of `settings.json` on this computer.
    pub fn settings_file(&self) -> Option<&Path> {
        self.hidden.path.as_deref()
    }

    /// "Checked today 09:12 · TMDB, IGDB, Open Library, Deezer", or
    /// `None` before the first check.
    pub fn check_line(&self, now: Timestamp) -> Option<String> {
        let checked = self.record.checked?;
        let when = checked.day_and_time(now);
        if self.record.sources.is_empty() {
            return Some(format!("Checked {when}"));
        }
        Some(format!(
            "Checked {when} · {}",
            self.record.sources.join(", ")
        ))
    }
}

fn write_record(record: &Record, file: Option<&Path>) {
    let Some(path) = file else {
        return;
    };
    let mut text = serde_json::to_string_pretty(&record.to_json()).expect("record encode");
    text.push('\n');
    if let Err(error) = storage::write_atomically(path, &text) {
        eprintln!("libri: {} could not be written: {error}", path.display());
    }
}

/// Runs what is due against `service` for `library`, following `settings`,
/// from the record `previous`: the next seasons, the next parts of followed
/// series, the creators' new work. `file` is where the record is written
/// as the run goes; the stored series lists are beside it. Blocks on the
/// providers, so the app runs it on the blocking pool (the fake answers at
/// once).
pub fn run(
    service: &Service,
    library: &Library,
    today: Date,
    settings: &Settings,
    previous: &Record,
    file: Option<&Path>,
) -> Outcome {
    if !settings.enabled {
        return Outcome {
            items: previous.items.clone(),
            sources: previous.sources.clone(),
            complete: true,
            stopped: None,
            record: None,
        };
    }
    let mut record = previous.clone();
    if record.asked_on != Some(today) {
        record.asked.clear();
        record.asked_on = Some(today);
    }
    let mut check = Check {
        service,
        library,
        today,
        settings,
        previous,
        shares: due_shares(previous.last_run, today),
        record,
        file,
        stores: Stores::at(file.and_then(Path::parent).map(Path::to_path_buf)),
        owners: BTreeSet::new(),
        down: BTreeMap::new(),
        complete: true,
        all_complete: true,
    };
    let stopped = check.stage("seasons", Check::seasons);
    let stopped = stopped.and_then(|()| check.stage("series", Check::series));
    let stopped = stopped.and_then(|()| check.stage("creators", Check::creators));
    let cancelled = stopped.is_err();
    // What was found for what is no longer followed goes, and what is out.
    let owners = std::mem::take(&mut check.owners);
    if !cancelled {
        check
            .record
            .items
            .retain(|item| item.owner.is_empty() || owners.contains(&item.owner));
    }
    check
        .record
        .items
        .retain(|item| !item.released.is_some_and(|r| r.is_released_on(today)));
    let mut seen = BTreeSet::new();
    check
        .record
        .items
        .retain(|item| seen.insert(item.key.clone()));
    let complete = !cancelled && check.all_complete;
    if complete {
        check.record.last_run = Some(today);
    }
    check.save();
    let why = if cancelled {
        Some("the window closed".to_owned())
    } else if complete {
        None
    } else if check.down.is_empty() {
        Some("a provider had a server error".to_owned())
    } else {
        let down: Vec<String> = check
            .down
            .iter()
            .map(|(name, why)| format!("{name}: {why}"))
            .collect();
        Some(down.join(", "))
    };
    Outcome {
        items: check.record.items.clone(),
        sources: check.record.sources.clone(),
        complete,
        stopped: why,
        record: Some(check.record),
    }
}

/// A run in progress.
struct Check<'a> {
    service: &'a Service,
    library: &'a Library,
    today: Date,
    settings: &'a Settings,
    previous: &'a Record,
    shares: BTreeSet<u32>,
    record: Record,
    file: Option<&'a Path>,
    stores: Stores,
    /// Every follow still on, whether or not it was due.
    owners: BTreeSet<String>,
    /// The providers offline or refusing, left alone for the rest of the
    /// run, with why.
    down: BTreeMap<String, String>,
    /// Whether the check under way has had every answer.
    complete: bool,
    all_complete: bool,
}

/// The window closed: the run stops.
struct Stopped;

/// What a provider said.
enum Asked<T> {
    Got(T),
    /// A 404 or a bad answer: nothing learned about this one.
    Nothing,
    /// No verdict for now: what is known of this one stands, and a later
    /// run asks again.
    NoAnswer,
}

/// A creator followed for new work.
struct Creator {
    kind: FollowKind,
    id: String,
    name: String,
    count: usize,
}

impl Creator {
    fn key(&self) -> String {
        format!("{}/{}", self.kind.key(), self.id)
    }
}

impl Check<'_> {
    fn save(&self) {
        write_record(&self.record, self.file);
    }

    /// Runs one of the three checks. It registers its follows even on a
    /// day it is already done, so what it found is kept.
    fn stage(
        &mut self,
        name: &str,
        check: fn(&mut Self, bool) -> Result<(), Stopped>,
    ) -> Result<(), Stopped> {
        let ask = self.record.done.get(name) != Some(&self.today);
        self.complete = true;
        check(self, ask)?;
        if !ask {
            return Ok(());
        }
        if self.complete {
            self.record.done.insert(name.to_owned(), self.today);
            self.save();
        } else {
            self.all_complete = false;
        }
        Ok(())
    }

    fn in_share(&self, id: &str) -> bool {
        self.shares.contains(&share_of(id))
    }

    /// Whether `owner` still needs asking today.
    fn fresh(&self, owner: &str) -> bool {
        !self.record.asked.contains(owner)
    }

    /// What `owner` has now; the record is written, so a close loses
    /// nothing asked so far.
    fn answered(&mut self, owner: &str, items: Vec<Found>) {
        self.record.items.retain(|item| item.owner != owner);
        self.record.items.extend(items);
        self.record.asked.insert(owner.to_owned());
        self.save();
    }

    /// Asks `reply` of `source`, lazily: a closed window or a provider
    /// already down sends nothing.
    fn ask<T>(
        &mut self,
        source: &dyn Source,
        reply: impl FnOnce() -> Reply<T>,
    ) -> Result<Asked<T>, Stopped> {
        if cancelled() {
            return Err(Stopped);
        }
        let name = source.name().to_owned();
        if self.down.contains_key(&name) {
            self.complete = false;
            return Ok(Asked::NoAnswer);
        }
        if !self.record.sources.contains(&name) {
            self.record.sources.push(name.clone());
        }
        Ok(match reply() {
            Ok(value) => Asked::Got(value),
            Err(failure) if failure.kind == FailureKind::NotConfigured => Asked::Nothing,
            Err(failure)
                if matches!(
                    failure.kind,
                    FailureKind::Offline | FailureKind::RateLimited
                ) =>
            {
                self.down.insert(name, failure.to_string());
                self.complete = false;
                Asked::NoAnswer
            }
            Err(failure) if failure.is_transient() => {
                self.complete = false;
                Asked::NoAnswer
            }
            Err(_) => Asked::Nothing,
        })
    }

    /// The source an entry is linked to, else the first configured one for
    /// its type that knows the title; `None` when nobody does.
    fn source_for(&mut self, entry: &Entry) -> Result<Option<(&'static str, String)>, Stopped> {
        if let Some(id) = &entry.source
            && let Some(source) = self.service.source_of(id)
        {
            return Ok(Some((source.name(), id.clone())));
        }
        let sources: Vec<&dyn Source> = self.service.sources_for(entry.media);
        for source in sources {
            let reply = || {
                source.match_title(
                    &entry.title,
                    entry.media,
                    entry.year(),
                    entry.creator.as_deref(),
                )
            };
            if let Asked::Got(Some(found)) = self.ask(source, reply)? {
                return Ok(Some((source.name(), found.source)));
            }
        }
        Ok(None)
    }

    fn first_found(&self, key: &str) -> Date {
        self.previous
            .items
            .iter()
            .find(|item| item.key == key)
            .map_or(self.today, |item| item.found)
    }

    fn previous_description(&self, key: &str) -> Option<String> {
        self.previous
            .items
            .iter()
            .find(|item| item.key == key && !item.description.is_empty())
            .map(|item| item.description.clone())
    }

    /// The next season of every show and anime series followed, and linked
    /// to a lookup: daily with a season still to come, else in its share.
    fn seasons(&mut self, ask: bool) -> Result<(), Stopped> {
        let shows: Vec<&Entry> = self
            .library
            .entries
            .iter()
            .filter(|e| {
                // The fixture's shows carry no ids, and the fake knows them.
                let linked = self.service.is_instant()
                    || e.source
                        .as_deref()
                        .is_some_and(|s| s.starts_with("tmdb:tv:") || s.starts_with("anilist:"));
                e.uses_seasons() && linked && e.status != EntryStatus::Dnf && !e.seasonal_only
            })
            .filter(|e| self.settings.is_followed("shows", &e.id, true))
            .collect();
        for show in shows {
            let owner = format!("shows/{}", show.id);
            self.owners.insert(owner.clone());
            let coming = show
                .season_list()
                .iter()
                .any(|s| s.released.is_some_and(|r| !r.is_released_on(self.today)));
            if !ask || !(coming || self.in_share(&show.id)) || !self.fresh(&owner) {
                continue;
            }
            let Some((_, id)) = self.source_for(show)? else {
                continue;
            };
            let Some(source) = self.service.source_of(&id) else {
                continue;
            };
            let listed = match self.ask(source, || source.seasons(&id))? {
                Asked::Got(Some(listed)) => listed,
                Asked::Got(None) | Asked::Nothing => {
                    self.answered(&owner, Vec::new());
                    continue;
                }
                Asked::NoAnswer => continue,
            };
            let highest = show
                .season_list()
                .iter()
                .map(|s| s.season)
                .max()
                .unwrap_or(0);
            let mut items = Vec::new();
            for season in listed.seasons {
                let Some(released) = season.released else {
                    continue;
                };
                if season.season <= highest || released.is_released_on(self.today) {
                    continue;
                }
                let key = crate::upcoming::key_of(&show.id, Some(season.season));
                items.push(Found {
                    media: show.media,
                    title: show.title.clone(),
                    sub: Some(format!("Season {}", season.season)),
                    creator: show.creator.clone(),
                    released: Some(released),
                    why: format!("Next season of {}", show.title),
                    description: show.description.clone(),
                    found: self.first_found(&key),
                    owner: owner.clone(),
                    key,
                    source: id.clone(),
                    provider: source.name().to_owned(),
                    cover_url: None,
                    kind: Kind::Season {
                        show: show.id.clone(),
                        number: season.season,
                    },
                });
            }
            self.answered(&owner, items);
        }
        Ok(())
    }

    /// The parts of the series `lead` is in: the stored list unless it is
    /// due (a week old, or a day with a part still to come), then the
    /// provider's, with the stored list standing in when it does not answer.
    fn series_list(
        &mut self,
        lead: &Entry,
        name: &str,
    ) -> Result<Option<(String, SeriesParts)>, Stopped> {
        let id = match self.source_for(lead)? {
            Some((_, id)) => id,
            // The fixture's parts carry no ids, and the fake goes by name.
            None if self.service.is_instant() => String::new(),
            None => return Ok(None),
        };
        let now = Timestamp::now();
        let stored = self.stores.read_series(&id);
        let usable = stored.as_ref().filter(|s| s.asked.as_deref() == Some(name));
        if let Some(stored) = usable {
            let age = now.0 - stored.fetched.0;
            let coming = stored
                .series
                .parts
                .iter()
                .any(|p| p.release().is_some_and(|r| !r.is_released_on(self.today)));
            if age < SERIES_UPCOMING_REFRESH || (age < SERIES_REFRESH && !coming) {
                return Ok(Some((id, stored.series.clone())));
            }
        }
        let stored = stored.map(|s| s.series);
        let Some(source) = self.service.source_of(&id) else {
            return Ok(stored.map(|series| (id, series)));
        };
        let parts = match self.ask(source, || source.series_parts(&id, Some(name)))? {
            Asked::Got(Some(parts)) if !parts.parts.is_empty() => {
                self.stores.write_series(&id, &parts, now, Some(name));
                Some(parts)
            }
            _ => stored,
        };
        Ok(parts.map(|series| (id, series)))
    }

    /// A part's edition in the languages read; `None` keeps it as listed.
    fn edition(&mut self, source_id: &str) -> Result<Option<lookup::EditionRelease>, Stopped> {
        let Some(source) = self.service.source_of(source_id) else {
            return Ok(None);
        };
        let languages = self.settings.languages.clone();
        match self.ask(source, || source.edition_release(source_id, &languages))? {
            Asked::Got(edition) => Ok(edition),
            _ => Ok(None),
        }
    }

    /// The next part of every followed series: one with a part Done or In
    /// progress (Want alone is not following, as on the phone) and not
    /// unticked; daily with a dated part recorded, else in its share.
    fn series(&mut self, ask: bool) -> Result<(), Stopped> {
        let mut groups: Vec<(MediaType, String, Vec<&Entry>)> = Vec::new();
        for entry in &self.library.entries {
            let Some(series) = &entry.series else {
                continue;
            };
            if !entry.can_have_series() || entry.seasonal_only {
                continue;
            }
            match groups
                .iter_mut()
                .find(|(media, name, _)| *media == entry.media && Series::same(name, &series.name))
            {
                Some((_, _, parts)) => parts.push(entry),
                None => groups.push((entry.media, series.name.clone(), vec![entry])),
            }
        }
        for (media, name, parts) in groups {
            let key = format!(
                "{}/{}",
                media.key(),
                lookup::text::title_key(&name).replace(' ', "")
            );
            let followed = parts
                .iter()
                .any(|p| matches!(p.status, EntryStatus::Done | EntryStatus::InProgress));
            if !followed || !self.settings.is_followed("series", &key, true) {
                continue;
            }
            let owner = format!("series/{key}");
            self.owners.insert(owner.clone());
            let dated = self.record.items.iter().any(|item| {
                item.owner == owner
                    && item
                        .released
                        .is_some_and(|r| r.day.is_some() || r.month.is_some())
            });
            if !ask || !(dated || self.in_share(&key)) || !self.fresh(&owner) {
                continue;
            }
            let lead = parts
                .iter()
                .copied()
                .find(|p| p.source.is_some())
                .unwrap_or(parts[0]);
            let before = self.complete;
            let Some((id, listed)) = self.series_list(lead, &name)? else {
                if self.complete == before {
                    self.answered(&owner, Vec::new());
                }
                continue;
            };
            let source = self.service.source_of(&id);
            let provider = source.map(|s| s.name()).unwrap_or_default();
            let display = Series::display_name(&listed.name);
            let in_library = parts.len();
            let mut items = Vec::new();
            for part in listed.parts {
                if part.extra {
                    continue;
                }
                let Some(mut released) = part.release() else {
                    continue;
                };
                let out = released.is_released_on(self.today);
                // A book out within the last year may still be to come in
                // the languages read.
                let asks_edition = media == MediaType::Books
                    && !self.settings.languages.is_empty()
                    && part.source.is_some()
                    && (!out
                        || lookup::is_recent_or_ahead(Some(released), self.today, EDITION_WINDOW));
                if out && !asks_edition {
                    continue;
                }
                let owned = parts.iter().any(|p| {
                    (part.source.is_some() && p.source == part.source)
                        || lookup::text::same_title(&p.title, &part.title)
                });
                if owned {
                    continue;
                }
                if asks_edition
                    && let Some(part_source) = &part.source
                    && let Some(edition) = self.edition(part_source)?
                {
                    if !edition.in_languages {
                        continue;
                    }
                    released = edition.released.unwrap_or(released);
                }
                if released.is_released_on(self.today) {
                    continue;
                }
                let key = part.source.clone().unwrap_or_else(|| {
                    format!(
                        "part:{}:{}",
                        media.key(),
                        lookup::text::title_key(&part.title)
                    )
                });
                let description = match (&part.source, self.previous_description(&key), source) {
                    (_, Some(description), _) => description,
                    (Some(part_source), None, Some(source)) => {
                        let probe = Match::new(media, part.title.clone(), part_source.clone());
                        match self.ask(source, || source.details(&probe))? {
                            Asked::Got(full) => full.description,
                            _ => String::new(),
                        }
                    }
                    _ => String::new(),
                };
                items.push(Found {
                    media,
                    title: part.title.clone(),
                    sub: None,
                    creator: part.creator.clone().or_else(|| lead.creator.clone()),
                    released: Some(released),
                    why: format!("Part of {display} · {in_library} in your library"),
                    description,
                    found: self.first_found(&key),
                    owner: owner.clone(),
                    source: key.clone(),
                    key,
                    provider: provider.to_owned(),
                    cover_url: part.cover.clone(),
                    kind: Kind::Part {
                        series: display.clone(),
                    },
                });
            }
            self.answered(&owner, items);
        }
        Ok(())
    }

    /// The creators of `kind` in the library, as the phone derives them:
    /// the tags of its category with an entry not dropped; artists from
    /// the creator field of albums and songs.
    fn creators_of(&self, kind: FollowKind) -> Vec<Creator> {
        let library = self.library;
        // The fixture has no author tags; a mock run still shows their rows.
        let by_field = kind == FollowKind::Authors && self.service.is_instant();
        if let Some(category) = kind.category().filter(|_| !by_field) {
            return library
                .tags
                .get(&category)
                .into_iter()
                .flatten()
                .filter_map(|tag| {
                    let entries: Vec<&Entry> = library
                        .entries
                        .iter()
                        .filter(|e| e.has_tag_as(tag, category))
                        .collect();
                    if entries.iter().all(|e| e.status == EntryStatus::Dnf) {
                        return None;
                    }
                    Some(Creator {
                        kind,
                        id: tag.clone(),
                        name: creator_name(tag, &entries),
                        count: entries.len(),
                    })
                })
                .collect();
        }
        let media = if by_field {
            MediaType::Books
        } else {
            MediaType::Music
        };
        let mut named: BTreeMap<String, Creator> = BTreeMap::new();
        for entry in library.entries_of(media) {
            let field = entry.creator.as_deref().unwrap_or_default().trim();
            let names = if by_field {
                lookup::text::split_creators(field)
            } else {
                vec![field.to_owned()]
            };
            for name in names {
                let id = if by_field {
                    crate::model::text::slugify(&name)
                } else {
                    name.to_lowercase()
                };
                if id.is_empty() || lookup::NO_PHOTO.contains(&id.as_str()) {
                    continue;
                }
                named
                    .entry(id.clone())
                    .or_insert_with(|| Creator {
                        kind,
                        id,
                        name,
                        count: 0,
                    })
                    .count += 1;
            }
        }
        named.into_values().collect()
    }

    /// Whether the creator is on: the settings, and a creator no longer
    /// active only when ticked anyway.
    fn is_on(&self, creator: &Creator) -> bool {
        let kind = creator.kind.key();
        let inactive = self
            .record
            .creators
            .get(&creator.key())
            .is_some_and(|status| status.inactive.is_some());
        let default_on = creator.kind.default_on() && !inactive;
        self.settings.is_followed(kind, &creator.id, default_on)
            && (!inactive || self.settings.is_ticked(kind, &creator.id))
    }

    fn status_wanted(&self, key: &str) -> bool {
        self.record
            .creators
            .get(key)
            .is_none_or(|status| status.checked_on.days_until(self.today) >= STATUS_DAYS)
    }

    /// Who is no longer active, for many names at once, before the creators
    /// are gone through one by one. Not knowing is no reason to skip them.
    fn inactive(&mut self, due: &[&Creator]) -> Result<BTreeMap<String, String>, Stopped> {
        let mut result = BTreeMap::new();
        for kind in FollowKind::ALL {
            // TMDB says it itself for people it credits.
            if matches!(kind, FollowKind::Directors | FollowKind::Actors) {
                continue;
            }
            let of_kind: Vec<&&Creator> = due
                .iter()
                .filter(|c| c.kind == kind && self.status_wanted(&c.key()))
                .collect();
            let sources: Vec<&dyn Source> = self
                .service
                .sources()
                .iter()
                .map(|s| s.as_ref())
                .filter(|s| s.is_configured())
                .collect();
            for batch in of_kind.chunks(INACTIVE_BATCH) {
                let names: Vec<String> = batch.iter().map(|c| c.name.clone()).collect();
                if cancelled() {
                    return Err(Stopped);
                }
                // Wikidata's answer, or none: a refusal here says nothing
                // about the provider it is asked through.
                let answer = sources
                    .iter()
                    .find_map(|source| source.inactive_of(kind, &names).ok().flatten());
                for creator in batch {
                    if let Some(why) = answer.as_ref().and_then(|a| a.get(&creator.name)) {
                        result.insert(creator.key(), why.clone());
                    }
                }
            }
        }
        Ok(result)
    }

    /// New work by the followed creators, each in its share: an author's
    /// books, an artist's albums, a director's or an actor's movies and
    /// shows, a studio's movies or games.
    fn creators(&mut self, ask: bool) -> Result<(), Stopped> {
        let mut followed = Vec::new();
        for kind in FollowKind::ALL {
            for creator in self.creators_of(kind) {
                if self.is_on(&creator) {
                    self.owners.insert(creator.key());
                    followed.push(creator);
                }
            }
        }
        if !ask {
            return Ok(());
        }
        let due: Vec<&Creator> = followed
            .iter()
            .filter(|c| self.in_share(&c.key()) && self.fresh(&c.key()))
            .collect();
        let inactive = self.inactive(&due)?;
        for creator in due {
            self.creator(creator, inactive.get(&creator.key()))?;
        }
        Ok(())
    }

    fn creator(&mut self, creator: &Creator, inactive: Option<&String>) -> Result<(), Stopped> {
        let key = creator.key();
        let sources: Vec<&dyn Source> = self
            .service
            .sources()
            .iter()
            .map(|s| s.as_ref())
            .filter(|s| s.is_configured() && s.creator_kinds().contains(&creator.kind))
            .collect();
        if sources.is_empty() {
            return Ok(());
        }
        if self.status_wanted(&key) {
            let known = self.record.creators.get(&key).cloned();
            let mut found = None;
            for &source in &sources {
                let reply = || source.find_creator(creator.kind, &creator.name);
                match self.ask(source, reply)? {
                    Asked::Got(Some(hit)) => {
                        found = Some(hit);
                        break;
                    }
                    Asked::NoAnswer => return Ok(()),
                    _ => {}
                }
            }
            let status = CreatorStatus {
                provider_id: found
                    .as_ref()
                    .map(|hit| hit.provider_id.clone())
                    .or_else(|| known.as_ref().and_then(|k| k.provider_id.clone())),
                inactive: inactive
                    .cloned()
                    .or_else(|| found.and_then(|hit| hit.inactive))
                    .or_else(|| known.and_then(|k| k.inactive)),
                checked_on: self.today,
            };
            self.record.creators.insert(key.clone(), status);
        }
        // Just learned to be no longer active: off too, unless ticked.
        if !self.is_on(creator) {
            self.owners.remove(&key);
            self.answered(&key, Vec::new());
            return Ok(());
        }
        let Some(provider_id) = self
            .record
            .creators
            .get(&key)
            .and_then(|status| status.provider_id.clone())
        else {
            self.answered(&key, Vec::new());
            return Ok(());
        };
        let mut matches: Vec<(&dyn Source, Match)> = Vec::new();
        for &source in &sources {
            // A studio's TMDB id is not asked of IGDB.
            if !source.issued(&provider_id) {
                continue;
            }
            let today = self.today;
            let reply = || source.creator_releases(&provider_id, creator.kind, today);
            match self.ask(source, reply)? {
                Asked::Got(found) => matches.extend(found.into_iter().map(|m| (source, m))),
                Asked::Nothing => {}
                Asked::NoAnswer => return Ok(()),
            }
        }
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        for (source, mut hit) in matches {
            if !seen.insert(hit.source.clone()) {
                continue;
            }
            if hit.media == MediaType::Books && !self.settings.languages.is_empty() {
                if let Some(edition) = self.edition(&hit.source)? {
                    if !edition.in_languages {
                        continue;
                    }
                    if let Some(title) = edition
                        .title
                        .filter(|title| other_script(&hit.title, title))
                    {
                        if hit
                            .creator
                            .as_deref()
                            .is_none_or(|by| other_script(by, &title))
                        {
                            hit.creator = Some(creator.name.clone());
                        }
                        hit.title = title;
                    }
                    if edition.released.is_some() {
                        hit.released = edition.released;
                    }
                }
            } else if hit.media == MediaType::Movies {
                let reply = || source.movie_date(&hit.source);
                if let Asked::Got(Some(date)) = self.ask(source, reply)? {
                    hit.released = Some(date);
                }
            }
            if hit.released.is_some_and(|r| r.is_released_on(self.today)) {
                continue;
            }
            let found_key = hit.source.clone();
            let count = creator.count;
            let noun = match (hit.media, count) {
                (MediaType::Books, 1) => "1 book".to_owned(),
                (MediaType::Books, n) => format!("{n} books"),
                (_, 1) => "1 title".to_owned(),
                (_, n) => format!("{n} titles"),
            };
            items.push(Found {
                media: hit.media,
                title: hit.title.clone(),
                sub: None,
                creator: hit.creator.clone().or_else(|| Some(creator.name.clone())),
                released: hit.released,
                why: format!("{} is in your library · {noun}", creator.name),
                description: hit.description.clone(),
                found: self.first_found(&found_key),
                owner: key.clone(),
                source: found_key.clone(),
                key: found_key,
                provider: source.name().to_owned(),
                cover_url: hit.cover_url.clone(),
                kind: Kind::Creator,
            });
        }
        self.answered(&key, items);
        Ok(())
    }
}

/// A creator tag as the name it stands for: the name in a creator field of
/// one of its entries ("J. K. Rowling" for #j-k-rowling), else the tag in
/// words.
fn creator_name(tag: &str, entries: &[&Entry]) -> String {
    entries
        .iter()
        .filter_map(|entry| entry.creator.as_deref())
        .flat_map(lookup::text::split_creators)
        .find(|name| crate::model::text::slugify(name) == tag)
        .unwrap_or_else(|| tag_display_name(tag))
}

/// Whether one text is in Latin letters and the other is not: a work's
/// "ベルセルク 43" takes its English edition's "Berserk 43".
fn other_script(a: &str, b: &str) -> bool {
    let latin = |text: &str| {
        text.chars()
            .find(|c| c.is_alphabetic())
            .map(|c| (c as u32) < 0x250)
    };
    matches!((latin(a), latin(b)), (Some(a), Some(b)) if a != b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::lookup::fake::Fake;
    use crate::model::codec;
    use std::sync::{Arc, Mutex};

    fn fake_run(library: &Library) -> Outcome {
        run(
            &Service::fake(),
            library,
            today(),
            &Settings::default(),
            &Record::default(),
            None,
        )
    }

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    fn today() -> Date {
        Date::parse("2026-10-19").unwrap()
    }

    #[test]
    fn the_fake_check_finds_seasons_parts_and_creators_work() {
        let library = library();
        let outcome = fake_run(&library);
        assert!(outcome.complete);
        assert_eq!(outcome.sources, ["Fake"]);
        let keys: Vec<&str> = outcome.items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"shows/severance-2022#3"), "{keys:?}");
        assert!(keys.contains(&"fake:movie:hunt-for-gollum"));
        assert!(keys.contains(&"part:books:berserkvol44"), "{keys:?}");
        assert!(keys.contains(&"fake:book:king-2027"));
        assert!(keys.contains(&"fake:album:alice-in-chains"));
        let gollum = outcome
            .items
            .iter()
            .find(|i| i.key == "fake:movie:hunt-for-gollum")
            .unwrap();
        assert_eq!(
            gollum.why,
            "Part of The Lord of the Rings · 2 in your library"
        );
        assert!(
            gollum.description.starts_with("Andy Serkis"),
            "details fetched"
        );
        assert_eq!(gollum.found, today());
        let season = outcome
            .items
            .iter()
            .find(|i| i.key == "shows/severance-2022#3")
            .unwrap();
        assert_eq!(season.full_title(), "Severance · Season 3");
        assert_eq!(season.why, "Next season of Severance");
        let king = outcome
            .items
            .iter()
            .find(|i| i.key == "fake:book:king-2027")
            .unwrap();
        assert_eq!(king.why, "Stephen King is in your library · 2 books");
        // The Bear and The Expanse are Want alone with one season each; a
        // DNF show (Stranger Things) is not asked.
        assert!(!keys.iter().any(|k| k.starts_with("shows/stranger-things")));
        // Volume 44 is not in the library; volume 43 is.
        assert!(!keys.contains(&"part:books:berserkvol43"));

        // A later run keeps the day an item was first found.
        let mut earlier = Record {
            items: outcome.items.clone(),
            ..Record::default()
        };
        earlier.items[0].found = Date::parse("2026-10-01").unwrap();
        let first_key = earlier.items[0].key.clone();
        let again = run(
            &Service::fake(),
            &library,
            today(),
            &Settings::default(),
            &earlier,
            None,
        );
        let kept = again.items.iter().find(|i| i.key == first_key).unwrap();
        assert_eq!(kept.found, Date::parse("2026-10-01").unwrap());
    }

    #[test]
    fn items_in_the_library_and_hidden_ones_are_left_out() {
        let mut library = library();
        let outcome = fake_run(&library);
        let mut state = State::default();
        state.finish(outcome, Timestamp::parse("2026-10-19T09:12:00Z").unwrap());
        let visible = state.visible(&library).len();
        assert!(visible >= 5);
        // Not interested hides one; Restore brings it back.
        state.hide("fake:movie:hunt-for-gollum");
        assert_eq!(state.visible(&library).len(), visible - 1);
        assert_eq!(state.hidden_items(&library).len(), 1);
        state.restore("fake:movie:hunt-for-gollum");
        assert_eq!(state.visible(&library).len(), visible);
        // The season added to the show drops its item.
        let show = library
            .entries
            .iter_mut()
            .find(|e| e.id == "shows/severance-2022")
            .unwrap();
        show.seasons.as_mut().unwrap().push(crate::model::Season {
            season: 3,
            released: Released::parse("2027"),
            status: Some(EntryStatus::Want),
            fresh: None,
            manual: None,
        });
        assert_eq!(state.visible(&library).len(), visible - 1);
        // A title added by hand drops its item too.
        let mut gollum = library.entries[0].clone();
        gollum.id = "movies/the-lord-of-the-rings-the-hunt-for-gollum-2027".into();
        gollum.title = "The Lord of the Rings: The Hunt for Gollum".into();
        gollum.source = None;
        library.entries.push(gollum);
        assert_eq!(state.visible(&library).len(), visible - 2);
        assert!(state.checked_today(Timestamp::parse("2026-10-19T20:00:00Z").unwrap()));
        assert!(!state.checked_today(Timestamp::parse("2026-10-20T20:00:00Z").unwrap()));
    }

    #[test]
    fn the_record_and_the_hidden_keys_read_back_from_their_files() {
        let dir = storage::TempDir::new("discover");
        let path = dir.0.as_path();
        let outcome = fake_run(&library());
        let now = Timestamp::parse("2026-10-19T09:12:00Z").unwrap();
        {
            let mut state = State::at(Some(path), Some(path));
            state.finish(outcome.clone(), now);
            state.hide("fake:book:king-2027");
        }
        let state = State::at(Some(path), Some(path));
        assert_eq!(state.record.checked, Some(now));
        assert_eq!(state.record.items, outcome.items);
        assert!(state.hidden.contains("fake:book:king-2027"));
        let settings = std::fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(settings.contains("\"not_interested\""));
        assert!(settings.contains("\"version\": 1"));
        let line = state.check_line(now).unwrap();
        assert!(
            line.starts_with("Checked today ") && line.ends_with(" · Fake"),
            "{line}"
        );
        // Restoring the last key takes the list out of the file.
        let mut state = state;
        state.restore("fake:book:king-2027");
        let settings = std::fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(!settings.contains("not_interested"));
        // An incomplete run changes nothing.
        let before = state.record.clone();
        state.finish(Outcome::failed("Fake: Offline"), now);
        assert_eq!(state.record, before);
    }

    #[test]
    fn an_unfinished_run_goes_again_and_a_finished_one_waits_for_tomorrow() {
        // Noon, so an hour on stays on the same date in any time zone
        // this runs in, and a day on does not.
        let now = Timestamp::parse("2026-10-19T12:00:00Z").unwrap();
        let hour_on = Timestamp(now.0 + 3_600);
        let mut state = State::at(None, None);
        assert!(state.is_due(now));
        state.start();
        assert!(!state.is_due(now), "running");
        state.finish(Outcome::failed("Fake: Offline"), now);
        assert!(state.is_due(hour_on), "not finished");
        state.start();
        let outcome = fake_run(&library());
        state.finish(outcome, hour_on);
        assert!(!state.is_due(hour_on), "finished today");
        assert!(state.is_due(Timestamp(now.0 + 86_400)));
    }

    /// The fake with its requests counted, and refusing while `limited`.
    #[derive(Default)]
    struct Counting {
        asked: Mutex<Vec<String>>,
        limited: AtomicBool,
    }

    impl Counting {
        fn note(&self, what: String) -> Reply<()> {
            if self.limited.load(Ordering::Relaxed) {
                return Err(lookup::Failure::new(FailureKind::RateLimited, "429"));
            }
            self.asked.lock().unwrap().push(what);
            Ok(())
        }

        fn count(&self, starts: &str) -> usize {
            let asked = self.asked.lock().unwrap();
            asked.iter().filter(|a| a.starts_with(starts)).count()
        }
    }

    impl Source for Arc<Counting> {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn name(&self) -> &'static str {
            "Fake"
        }
        fn types(&self) -> &'static [MediaType] {
            &MediaType::ALL
        }
        fn is_configured(&self) -> bool {
            true
        }
        fn issued(&self, _source: &str) -> bool {
            true
        }
        fn search(&self, query: &str, media: MediaType, limit: usize) -> Reply<lookup::Hits> {
            Fake.search(query, media, limit)
        }
        fn match_title(
            &self,
            title: &str,
            media: MediaType,
            year: Option<u16>,
            creator: Option<&str>,
        ) -> Reply<Option<Match>> {
            Fake.match_title(title, media, year, creator)
        }
        fn cover_url(&self, _source: &str, _media: MediaType) -> Reply<Option<String>> {
            Ok(None)
        }
        fn seasons(&self, source: &str) -> Reply<Option<lookup::ShowSeasons>> {
            self.note(format!("seasons {source}"))?;
            Fake.seasons(source)
        }
        fn series_parts(&self, source: &str, name: Option<&str>) -> Reply<Option<SeriesParts>> {
            self.note(format!("parts {}", name.unwrap_or_default()))?;
            Fake.series_parts(source, name)
        }
        fn creator_kinds(&self) -> &'static [FollowKind] {
            Fake.creator_kinds()
        }
        fn find_creator(
            &self,
            kind: FollowKind,
            name: &str,
        ) -> Reply<Option<lookup::CreatorMatch>> {
            self.note(format!("find {name}"))?;
            Fake.find_creator(kind, name)
        }
        fn creator_releases(
            &self,
            provider_id: &str,
            kind: FollowKind,
            today: Date,
        ) -> Reply<Vec<Match>> {
            self.note(format!("releases {provider_id}"))?;
            Fake.creator_releases(provider_id, kind, today)
        }
    }

    fn counting() -> (Arc<Counting>, Service) {
        let counting = Arc::new(Counting::default());
        let source: Arc<dyn Source> = Arc::new(counting.clone());
        (counting, Service::with(vec![source], true))
    }

    const SETTINGS: &str = r#"{"version": 1, "books": {"languages": ["en", "sv"]},
        "upcoming": {
            "unfollowed": {"shows": ["shows/severance-2022"], "artists": ["alice in chains"]},
            "followed": {"actors": ["tom-cruise"]},
            "not_interested": ["x"]
        }}"#;

    #[test]
    fn settings_say_what_is_followed() {
        let settings = Settings::parse(SETTINGS);
        assert_eq!(settings.languages, ["en", "sv"]);
        assert!(!settings.is_followed("shows", "shows/severance-2022", true));
        assert!(settings.is_followed("shows", "shows/other", true));
        assert!(settings.is_followed("actors", "tom-cruise", false));
        assert!(!settings.is_followed("actors", "brad-pitt", false));
        assert_eq!(settings_path("library.json"), "settings.json");
        assert_eq!(settings_path("data/library.json"), "data/settings.json");
        assert_eq!(Settings::parse(r#"{"version": 2}"#), Settings::default());
        assert!(!Settings::parse(r#"{"version": 1, "upcoming": {"enabled": false}}"#).enabled);
    }

    #[test]
    fn what_is_unfollowed_is_never_asked_and_its_rows_go() {
        let (counting, service) = counting();
        let library = library();
        let settings = Settings::parse(SETTINGS);
        let outcome = run(
            &service,
            &library,
            today(),
            &settings,
            &Record::default(),
            None,
        );
        assert!(outcome.complete, "{:?}", outcome.stopped);
        assert_eq!(counting.count("seasons tmdb:tv:95396"), 0);
        assert_eq!(counting.count("find Alice in Chains"), 0);
        let keys: Vec<&str> = outcome.items.iter().map(|i| i.key.as_str()).collect();
        assert!(!keys.contains(&"shows/severance-2022#3"), "{keys:?}");
        assert!(!keys.contains(&"fake:album:alice-in-chains"), "{keys:?}");
        // Unticked after a run: what it found goes at the next run.
        let everything = fake_run(&library).record.unwrap();
        assert!(
            everything
                .items
                .iter()
                .any(|i| i.key == "shows/severance-2022#3")
        );
        let tomorrow = today().add_days(1);
        let outcome = run(&service, &library, tomorrow, &settings, &everything, None);
        assert!(
            !outcome
                .items
                .iter()
                .any(|i| i.key == "shows/severance-2022#3")
        );
    }

    #[test]
    fn the_shares_are_the_phones() {
        assert_eq!(share_of("abc"), 96354 % 7);
        assert_eq!(due_shares(None, today()).len(), 7);
        let last = today().add_days(-1);
        assert_eq!(
            due_shares(Some(last), today()),
            BTreeSet::from([share_on(today())])
        );
        assert_eq!(due_shares(Some(today()), today()).len(), 0);
        assert_eq!(due_shares(Some(today().add_days(-3)), today()).len(), 3);
        assert_eq!(due_shares(Some(today().add_days(-9)), today()).len(), 7);
    }

    #[test]
    fn a_later_day_asks_only_what_is_due_and_keeps_the_rest() {
        let (counting, service) = counting();
        let library = library();
        let settings = Settings::default();
        let first = run(
            &service,
            &library,
            today(),
            &settings,
            &Record::default(),
            None,
        );
        assert!(first.complete);
        let asked_first = counting.asked.lock().unwrap().len();
        assert!(asked_first > 0);
        let record = first.record.unwrap();
        // Later the same day everything is done: nothing is asked.
        counting.asked.lock().unwrap().clear();
        let again = run(&service, &library, today(), &settings, &record, None);
        assert!(again.complete);
        assert!(counting.asked.lock().unwrap().is_empty());
        assert_eq!(again.items, first.items);
        // The next day a seventh or so of it, and every row stays.
        let next = run(
            &service,
            &library,
            today().add_days(1),
            &settings,
            &record,
            None,
        );
        assert!(next.complete);
        let asked_next = counting.asked.lock().unwrap().len();
        assert!(asked_next < asked_first, "{asked_next} of {asked_first}");
        assert_eq!(counting.count("find "), 0, "the provider ids are kept");
        for item in &first.items {
            assert!(next.items.iter().any(|i| i.key == item.key), "{}", item.key);
        }
    }

    #[test]
    fn a_refusing_provider_loses_nothing_and_the_next_run_goes_on() {
        let (counting, service) = counting();
        let library = library();
        let settings = Settings::default();
        counting.limited.store(true, Ordering::Relaxed);
        let stopped = run(
            &service,
            &library,
            today(),
            &settings,
            &Record::default(),
            None,
        );
        assert!(!stopped.complete);
        assert!(stopped.stopped.unwrap().contains("Fake"));
        let record = stopped.record.unwrap();
        assert_eq!(record.last_run, None);
        counting.limited.store(false, Ordering::Relaxed);
        let done = run(&service, &library, today(), &settings, &record, None);
        assert!(done.complete, "{:?}", done.stopped);
        assert!(!done.items.is_empty());
        // Half through: what was asked is not asked again the same day.
        let done = done.record.unwrap();
        let mut partway = done.clone();
        partway.done.clear();
        partway.last_run = None;
        counting.asked.lock().unwrap().clear();
        let resumed = run(&service, &library, today(), &settings, &partway, None);
        assert!(resumed.complete);
        assert!(counting.asked.lock().unwrap().is_empty(), "all asked today");
    }

    #[test]
    fn the_run_writes_as_it_goes_and_the_hidden_keys_join_the_download() {
        let dir = storage::TempDir::new("discover");
        let file = dir.0.join("discover.json");
        let library = library();
        let _ = run(
            &Service::fake(),
            &library,
            today(),
            &Settings::default(),
            &Record::default(),
            Some(&file),
        );
        let state = State::at(Some(dir.0.as_path()), None);
        assert!(!state.record.items.is_empty());
        assert!(!state.record.creators.is_empty());
        let merged = with_local_hidden(SETTINGS, r#"{"upcoming": {"not_interested": ["y"]}}"#);
        let merged: Value = serde_json::from_str(&merged.unwrap()).unwrap();
        assert_eq!(merged["upcoming"]["not_interested"], json!(["x", "y"]));
        assert_eq!(merged["books"]["languages"], json!(["en", "sv"]));
    }

    #[test]
    fn creators_come_from_their_tags_with_the_name_in_the_creator_field() {
        let mut library = library();
        let tag = "j-k-rowling";
        library
            .tags
            .insert(crate::model::TagCategory::Authors, vec![tag.to_owned()]);
        let book = library
            .entries
            .iter_mut()
            .find(|e| e.media == MediaType::Books);
        let book = book.unwrap();
        book.creator = Some("J. K. Rowling".to_owned());
        book.tags.push(tag.to_owned());
        let settings = Settings::default();
        let previous = Record::default();
        let (_, service) = counting();
        // A real service: the fake's tagless fallback is off.
        let service = Service::with(service.sources().to_vec(), false);
        let check = Check {
            service: &service,
            library: &library,
            today: today(),
            settings: &settings,
            previous: &previous,
            shares: BTreeSet::new(),
            record: Record::default(),
            file: None,
            stores: Stores::at(None),
            owners: BTreeSet::new(),
            down: BTreeMap::new(),
            complete: true,
            all_complete: true,
        };
        let authors = check.creators_of(FollowKind::Authors);
        assert_eq!(authors.len(), 1);
        assert_eq!(authors[0].name, "J. K. Rowling");
        assert_eq!(authors[0].key(), "authors/j-k-rowling");
    }
}
