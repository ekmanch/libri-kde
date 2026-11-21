//! Discover: titles not in the library that the daily check finds for
//! things that are (decided 2026-11-07; the phone's DESIGN.md 10c has the
//! checks it follows): the next season of a show, the next part of a
//! followed series (a part Done or In progress), and new work by the
//! creators in the library. Each item carries its type, title, an
//! optional sub ("Season 3"), the creator, its release as precise as the
//! provider has it, a "why" line, a description, the provider, the lookup
//! id and the follow it was found for.
//!
//! What is followed is `follows` over `settings.json` (`shared`): the
//! follows come from the library, and the file holds the master switch
//! and what is unticked or ticked. One title found for several follows is
//! kept once per follow and drawn as one row with every reason
//! (`Found::reasons`); it shows while one of them is still followed, so
//! an unfollow takes its rows away at once and the next run drops them.
//!
//! The check runs as the phone's scheduler does (its DESIGN.md 10c,
//! `upcoming_scheduler.dart`): a little after the start, then on the
//! hour's timer until it has finished that day. What the owner is waiting
//! on is asked daily; everything else takes its turn in one of seven
//! weekly shares, by a stable hash of its id, and a first run asks all
//! seven. Check now asks all seven whatever was asked today. A creator's
//! provider id is kept for ninety days and a series' parts for a week, so
//! most of a run asks nothing twice. What was found stays until its
//! follow is asked again, and each item asked is written as it is done,
//! so a close, a Stop or a provider that refuses loses nothing and the
//! next run takes up where this one stopped. A provider offline or
//! refusing is left alone for the rest of the run; the others go on. All
//! of it is kept in `discover.json` under the cache folder, with the time
//! of the check and the providers asked, so the page shows at once and
//! the footer can say "Checked today 09:12 · TMDB, IGDB, Open Library,
//! Deezer".
//! Items already in the library (the same lookup id, a season the show
//! now has, or the same title) are left out when the page is drawn, so
//! an entry added on the phone drops its item on the next sync.
//!
//! "Not interested" hides an item by its key (`upcoming.not_interested`
//! of `settings.json`: a lookup id, or `<show id>#<season>` for a
//! season), which syncs with the phone like the rest of that file.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::follows::{self, Creator, Kind as Follow};
use crate::lookup::store::Stores;
use crate::lookup::{self, FailureKind, FollowKind, Match, Reply, SeriesParts, Service, Source};
use crate::model::{Date, Entry, Library, MediaType, Released, Series, tag_display_name};
use crate::shared::Settings;
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

/// A follow that found a title: "Stephen King · Author".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reason {
    pub kind: Follow,
    pub id: String,
    pub name: String,
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
    /// The follow it came from ("shows/shows/severance-2022",
    /// "authors/stephen-king", "series/movies/halo"): it stays until that
    /// one is asked again.
    #[serde(default)]
    pub owner: String,
    /// That follow's name, for the menu's "Unfollow Stephen King".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub owner_name: String,
    #[serde(flatten)]
    pub kind: Kind,
    /// Every follow the title was found for, on the row the page draws:
    /// the record keeps one item per follow, and the rows merge them.
    #[serde(skip)]
    pub reasons: Vec<Reason>,
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

    /// The follow this item was found for, with the name the record kept
    /// or, for an item from before it kept one, the best the item says.
    fn reason(&self) -> Option<Reason> {
        let (kind, id) = follows::parse_key(&self.owner)?;
        let name = if !self.owner_name.is_empty() {
            self.owner_name.clone()
        } else {
            match (&self.kind, kind) {
                (Kind::Season { .. }, _) => self.title.clone(),
                (Kind::Part { series }, _) => series.clone(),
                (Kind::Creator, Follow::Artists) => {
                    self.creator.clone().unwrap_or_else(|| id.to_owned())
                }
                (Kind::Creator, _) => tag_display_name(id),
            }
        };
        Some(Reason {
            kind,
            id: id.to_owned(),
            name,
        })
    }
}

/// The record's items as rows: one per key, in the order first found,
/// with every follow that found it.
fn rows_of(items: &[Found]) -> Vec<Found> {
    let mut rows: Vec<Found> = Vec::new();
    let mut at: BTreeMap<&str, usize> = BTreeMap::new();
    for item in items {
        let index = *at.entry(item.key.as_str()).or_insert_with(|| {
            rows.push(item.clone());
            rows.len() - 1
        });
        if let Some(reason) = item.reason()
            && !rows[index].reasons.contains(&reason)
        {
            rows[index].reasons.push(reason);
        }
    }
    rows
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
    /// False when a provider was offline, busy or down, the check was
    /// stopped or the window closed, so the day is not recorded and a
    /// later run goes on.
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

/// How far a run is, shared between the app and the task that makes it:
/// "Checking 184 of 423…", and Stop.
#[derive(Debug, Default)]
pub struct Progress {
    done: AtomicUsize,
    total: AtomicUsize,
    stop: AtomicBool,
}

impl Progress {
    /// A run this far along, for a look at the row.
    pub fn at(done: usize, total: usize) -> Progress {
        Progress {
            done: AtomicUsize::new(done),
            total: AtomicUsize::new(total),
            stop: AtomicBool::new(false),
        }
    }

    /// How many follows have been asked, of how many this run asks.
    pub fn counts(&self) -> (usize, usize) {
        let total = self.total.load(Ordering::Relaxed);
        (self.done.load(Ordering::Relaxed).min(total), total)
    }

    /// Stop: the run ends before its next request.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn tick(&self) {
        self.done.fetch_add(1, Ordering::Relaxed);
    }
}

/// How a run is made.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Check now: every share, and what was asked today again.
    pub force: bool,
    pub progress: Arc<Progress>,
}

/// The messages of the check.
#[derive(Debug, Clone)]
pub enum Message {
    /// Runs the check, unless it ran today or is running.
    Check,
    /// Check now: the whole set at once, whatever the day.
    Now,
    /// Stop: the run under way ends where it is.
    Stop,
    /// While a run is out: the row counts on.
    Tick,
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
    /// How many titles the runs of `new_on` found that were not recorded
    /// before: the "3 new" of Last checked.
    pub new: usize,
    pub new_on: Option<Date>,
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

/// Version 2 (2026-11-09): version 1 recorded a creator as active when
/// Wikidata refused to say, so its authors, artists and studios without
/// a reason are dropped on reading and asked again.
const RECORD_VERSION: u64 = 2;

impl Record {
    fn from_json(json: &Value) -> Option<Record> {
        let version = json.get("version").and_then(Value::as_u64)?;
        if version != 1 && version != RECORD_VERSION {
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
        let day = |key: &str| json.get(key).and_then(Value::as_str).and_then(Date::parse);
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
            last_run: day("last_run"),
            done: serde_json::from_value(json["done"].clone()).unwrap_or_default(),
            asked: serde_json::from_value(json["asked"].clone()).unwrap_or_default(),
            asked_on: day("asked_on"),
            creators: {
                let mut creators: BTreeMap<String, CreatorStatus> =
                    serde_json::from_value(json["creators"].clone()).unwrap_or_default();
                if version == 1 {
                    // TMDB says who died itself; the rest asked Wikidata.
                    creators.retain(|key, status| {
                        status.inactive.is_some()
                            || key.starts_with("directors/")
                            || key.starts_with("actors/")
                    });
                }
                creators
            },
            new: json["new"].as_u64().unwrap_or(0) as usize,
            new_on: day("new_on"),
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
            "new": self.new,
            "new_on": self.new_on.map(|d| d.to_string()),
        })
    }
}

/// The check's state on the app: the record, its items as the rows the
/// page draws, and the run that is out.
#[derive(Debug, Clone, Default)]
pub struct State {
    record: Record,
    rows: Vec<Found>,
    file: Option<PathBuf>,
    pub running: bool,
    /// The run under way, or the last one.
    pub progress: Arc<Progress>,
}

impl State {
    /// The record under `cache`; nothing kept with `None` (the tests).
    pub fn at(cache: Option<&Path>) -> State {
        let file = cache.map(|dir| dir.join("discover.json"));
        let record: Record = file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|json| Record::from_json(&json))
            .unwrap_or_default();
        State {
            rows: rows_of(&record.items),
            record,
            file,
            ..State::default()
        }
    }

    pub fn record(&self) -> &Record {
        &self.record
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

    /// A run starts now; `force` is Check now.
    pub fn start(&mut self, force: bool) -> Options {
        self.running = true;
        self.progress = Arc::new(Progress::default());
        Options {
            force,
            progress: self.progress.clone(),
        }
    }

    /// A run this far along that nothing is making, for a look at the
    /// row (`LIBRI_DISCOVER=checking`, the captures).
    pub fn look_checking(&mut self, done: usize, total: usize) {
        self.running = true;
        self.progress = Arc::new(Progress::at(done, total));
    }

    /// Stop: the run under way ends before its next request.
    pub fn stop(&self) {
        self.progress.stop();
    }

    /// Whether the follow behind `reason` is on: the settings, and a
    /// creator the check found to be no longer active only when ticked.
    pub fn is_followed(&self, settings: &Settings, reason: &Reason) -> bool {
        let key = follows::key_of(reason.kind, &reason.id);
        let inactive = self
            .record
            .creators
            .get(&key)
            .is_some_and(|status| status.inactive.is_some());
        settings.is_followed(
            reason.kind,
            &reason.id,
            reason.kind.default_on() && !inactive,
        )
    }

    /// The reasons of `found` that are still followed: what its menu
    /// offers to unfollow.
    pub fn live_reasons<'a>(&self, settings: &Settings, found: &'a Found) -> Vec<&'a Reason> {
        found
            .reasons
            .iter()
            .filter(|reason| self.is_followed(settings, reason))
            .collect()
    }

    /// The items Discover lists: not in the library, not hidden.
    pub fn visible(&self, library: &Library, settings: &Settings) -> Vec<&Found> {
        self.lists(library, settings).0
    }

    /// The hidden items still worth restoring: not in the library.
    pub fn hidden_items(&self, library: &Library, settings: &Settings) -> Vec<&Found> {
        self.lists(library, settings).1
    }

    /// Both at once, the library gathered once: what `visible` and
    /// `hidden_items` return. Nothing with the check switched off, and no
    /// title whose every reason has been unfollowed.
    pub fn lists(&self, library: &Library, settings: &Settings) -> (Vec<&Found>, Vec<&Found>) {
        if !settings.enabled {
            return (Vec::new(), Vec::new());
        }
        let owned = Owned::of(library);
        self.rows
            .iter()
            .filter(|item| !item.in_owned(&owned))
            .filter(|item| {
                item.reasons.is_empty()
                    || item
                        .reasons
                        .iter()
                        .any(|reason| self.is_followed(settings, reason))
            })
            .partition(|item| !settings.not_interested.contains(&item.key))
    }

    pub fn item(&self, key: &str) -> Option<&Found> {
        self.rows.iter().find(|item| item.key == key)
    }

    /// The check came back: the record as it left it, stamped when the run
    /// was complete.
    pub fn finish(&mut self, outcome: Outcome, now: Timestamp) {
        self.running = false;
        let Some(mut record) = outcome.record else {
            return;
        };
        if outcome.complete {
            record.checked = Some(now);
        }
        self.rows = rows_of(&record.items);
        self.record = record;
        write_record(&self.record, self.file.as_deref());
    }

    /// Where the record is kept, for the run to write as it goes.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
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

    /// The Last checked row of Settings: "Today 20:07 · 3 new",
    /// "Yesterday 22:40", "Oct 12", "Never"; while a run is out,
    /// "Checking 184 of 423…".
    pub fn last_checked(&self, now: Timestamp) -> String {
        if self.running {
            let (done, total) = self.progress.counts();
            return match total {
                0 => "Checking…".to_owned(),
                _ => format!("Checking {done} of {total}…"),
            };
        }
        let Some(checked) = self.record.checked else {
            return "Never".to_owned();
        };
        let when = checked.day_and_time(now);
        let mut line = match checked.days_from(now) {
            0 | -1 => {
                let mut letters = when.chars();
                match letters.next() {
                    Some(first) => first.to_uppercase().chain(letters).collect(),
                    None => when,
                }
            }
            // "Nov 3, 09:12": the day alone.
            _ => when.split(',').next().unwrap_or_default().to_owned(),
        };
        if self.record.new > 0 && self.record.new_on == Some(checked.local_date()) {
            line.push_str(&format!(" · {} new", self.record.new));
        }
        line
    }

    /// How far the run under way is, 0 to 1.
    pub fn fraction(&self) -> f32 {
        match self.progress.counts() {
            (_, 0) => 0.0,
            (done, total) => done as f32 / total as f32,
        }
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
    let options = Options::default();
    run_with(service, library, today, settings, previous, file, &options)
}

/// `run`, as `options` say: Check now asks every share and what was asked
/// today again, and the run counts what it asks and can be stopped.
pub fn run_with(
    service: &Service,
    library: &Library,
    today: Date,
    settings: &Settings,
    previous: &Record,
    file: Option<&Path>,
    options: &Options,
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
    if record.asked_on != Some(today) || options.force {
        record.asked.clear();
        record.asked_on = Some(today);
    }
    let shares = if options.force {
        record.done.clear();
        (0..7).collect()
    } else {
        due_shares(previous.last_run, today)
    };
    let mut check = Check {
        service,
        library,
        today,
        settings,
        languages: settings.reading_languages(),
        previous,
        shares,
        record,
        file,
        stores: Stores::at(file.and_then(Path::parent).map(Path::to_path_buf)),
        owners: BTreeSet::new(),
        down: BTreeMap::new(),
        complete: true,
        all_complete: true,
        progress: &options.progress,
    };
    let total = check.count_due();
    options.progress.total.store(total, Ordering::Relaxed);
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
    // One item per follow and title: the rows merge the follows.
    let mut seen = BTreeSet::new();
    check
        .record
        .items
        .retain(|item| seen.insert((item.owner.clone(), item.key.clone())));
    // "3 new": the titles not recorded before, added up over a day's runs.
    let before: BTreeSet<&str> = previous.items.iter().map(|i| i.key.as_str()).collect();
    let now: BTreeSet<&str> = check.record.items.iter().map(|i| i.key.as_str()).collect();
    let fresh = now.difference(&before).count();
    if check.record.new_on == Some(today) && !options.force {
        check.record.new += fresh;
    } else {
        check.record.new = fresh;
        check.record.new_on = Some(today);
    }
    let complete = !cancelled && check.all_complete;
    if complete {
        check.record.last_run = Some(today);
    }
    check.save();
    let why = if cancelled && options.progress.stopped() {
        Some("stopped".to_owned())
    } else if cancelled {
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
    /// The languages a found book must have an edition in.
    languages: Vec<String>,
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
    progress: &'a Progress,
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

/// What Wikidata said of the creators due, by creator key: why each one
/// it knows is no longer active, and who it was not asked about because
/// the ask failed.
#[derive(Default)]
struct Classified {
    found: BTreeMap<String, String>,
    unknown: BTreeSet<String>,
}

/// What Wikidata said of one creator.
#[derive(Clone, Copy)]
enum Classification<'a> {
    /// Asked: why they are no longer active, or `None` while they are.
    Known(Option<&'a String>),
    /// The ask failed: nothing learned.
    Unknown,
}

impl<'a> Classification<'a> {
    fn found(self) -> Option<&'a String> {
        match self {
            Classification::Known(why) => why,
            Classification::Unknown => None,
        }
    }
}

impl<'a> Check<'a> {
    fn save(&self) {
        write_record(&self.record, self.file);
    }

    /// The window closed, or Stop was pressed.
    fn stopped(&self) -> bool {
        CANCELLED.load(Ordering::Relaxed) || self.progress.stopped()
    }

    /// Whether the check `name` still asks today.
    fn asks(&self, name: &str) -> bool {
        self.record.done.get(name) != Some(&self.today)
    }

    /// The shows followed for new seasons.
    fn shows(&self) -> Vec<&'a Entry> {
        follows::shows(self.library, self.service.is_instant())
            .into_iter()
            .filter(|show| self.settings.is_followed(Follow::Shows, &show.id, true))
            .collect()
    }

    /// Whether a show is asked in this run: daily with a season still to
    /// come, else in its share.
    fn show_due(&self, show: &Entry) -> bool {
        let coming = show
            .season_list()
            .iter()
            .any(|s| s.released.is_some_and(|r| !r.is_released_on(self.today)));
        (coming || self.in_share(&show.id)) && self.fresh(&follows::key_of(Follow::Shows, &show.id))
    }

    /// The series followed for next parts.
    fn series_followed(&self) -> Vec<follows::SeriesGroup<'a>> {
        follows::series(self.library)
            .into_iter()
            .filter(|group| self.settings.is_followed(Follow::Series, &group.key, true))
            .collect()
    }

    /// Whether a series is asked in this run: daily with a dated part
    /// recorded, else in its share.
    fn series_due(&self, key: &str) -> bool {
        let owner = follows::key_of(Follow::Series, key);
        let dated = self.record.items.iter().any(|item| {
            item.owner == owner
                && item
                    .released
                    .is_some_and(|r| r.day.is_some() || r.month.is_some())
        });
        (dated || self.in_share(key)) && self.fresh(&owner)
    }

    /// The creators of every kind: those followed, and the rest.
    fn creators_split(&self) -> (Vec<Creator<'a>>, Vec<Creator<'a>>) {
        let instant = self.service.is_instant();
        FollowKind::ALL
            .into_iter()
            .flat_map(|kind| follows::creators(self.library, kind, instant))
            .partition(|creator| self.is_on(creator))
    }

    /// Whether a followed creator is asked in this run: in its share.
    fn creator_due(&self, creator: &Creator) -> bool {
        let key = creator.key();
        self.in_share(&key) && self.fresh(&key)
    }

    /// Whether a creator who is not followed is asked who they are to
    /// their provider, so the Following window can say when they are no
    /// longer active: in its share, when that is not known or is old.
    fn status_due(&self, creator: &Creator) -> bool {
        let key = creator.key();
        self.in_share(&key) && self.status_wanted(&key) && self.fresh(&status_key(&key))
    }

    /// How many follows this run asks about: the "423" of "Checking 184
    /// of 423…".
    fn count_due(&self) -> usize {
        let mut total = 0;
        if self.asks("seasons") {
            total += self.shows().iter().filter(|s| self.show_due(s)).count();
        }
        if self.asks("series") {
            let groups = self.series_followed();
            total += groups.iter().filter(|g| self.series_due(&g.key)).count();
        }
        if self.asks("creators") {
            let (followed, others) = self.creators_split();
            total += followed.iter().filter(|c| self.creator_due(c)).count();
            total += others.iter().filter(|c| self.status_due(c)).count();
        }
        total
    }

    /// Runs one of the three checks. It registers its follows even on a
    /// day it is already done, so what it found is kept.
    fn stage(
        &mut self,
        name: &str,
        check: fn(&mut Self, bool) -> Result<(), Stopped>,
    ) -> Result<(), Stopped> {
        let ask = self.asks(name);
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
        if self.stopped() {
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
        for show in self.shows() {
            let owner = follows::key_of(Follow::Shows, &show.id);
            self.owners.insert(owner.clone());
            if !ask || !self.show_due(show) {
                continue;
            }
            self.progress.tick();
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
                    owner_name: show.title.clone(),
                    key,
                    source: id.clone(),
                    provider: source.name().to_owned(),
                    cover_url: None,
                    kind: Kind::Season {
                        show: show.id.clone(),
                        number: season.season,
                    },
                    reasons: Vec::new(),
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
        let languages = self.languages.clone();
        match self.ask(source, || source.edition_release(source_id, &languages))? {
            Asked::Got(edition) => Ok(edition),
            _ => Ok(None),
        }
    }

    /// The next part of every followed series: one with a part Done or In
    /// progress (Want alone is not following, as on the phone) and not
    /// unticked; daily with a dated part recorded, else in its share.
    fn series(&mut self, ask: bool) -> Result<(), Stopped> {
        for group in self.series_followed() {
            let follows::SeriesGroup {
                media,
                name,
                key,
                parts,
            } = group;
            let owner = follows::key_of(Follow::Series, &key);
            self.owners.insert(owner.clone());
            if !ask || !self.series_due(&key) {
                continue;
            }
            self.progress.tick();
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
                    && !self.languages.is_empty()
                    && part.source.is_some()
                    && (!out
                        || lookup::is_recent_or_ahead(Some(released), self.today, EDITION_WINDOW));
                // A game out elsewhere within the last year may still be
                // to come on the default platform.
                let asks_platform = media == MediaType::Games
                    && self.settings.platform.is_some()
                    && part.source.is_some()
                    && (!out
                        || lookup::is_recent_or_ahead(Some(released), self.today, EDITION_WINDOW));
                if out && !asks_edition && !asks_platform {
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
                // A game is dated by the default platform when it is
                // listed for it: no date while it has none there.
                let mut released = Some(released);
                if asks_platform
                    && let Some(part_source) = &part.source
                    && let Some(games) = self.service.source_of(part_source)
                    && let Asked::Got(Some(game)) = self.ask(games, || games.game(part_source))?
                {
                    released = game.released_for(self.settings.platform.as_deref());
                }
                if released.is_some_and(|r| r.is_released_on(self.today)) {
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
                    released,
                    why: format!("Part of {display} · {in_library} in your library"),
                    description,
                    found: self.first_found(&key),
                    owner: owner.clone(),
                    owner_name: display.clone(),
                    source: key.clone(),
                    key,
                    provider: provider.to_owned(),
                    cover_url: part.cover.clone(),
                    kind: Kind::Part {
                        series: display.clone(),
                    },
                    reasons: Vec::new(),
                });
            }
            self.answered(&owner, items);
        }
        Ok(())
    }

    /// Whether the creator is on: the settings, and a creator no longer
    /// active only when ticked anyway.
    fn is_on(&self, creator: &Creator) -> bool {
        let kind = Follow::from(creator.kind);
        let inactive = self
            .record
            .creators
            .get(&creator.key())
            .is_some_and(|status| status.inactive.is_some());
        self.settings
            .is_followed(kind, &creator.id, kind.default_on() && !inactive)
    }

    fn status_wanted(&self, key: &str) -> bool {
        self.record
            .creators
            .get(key)
            .is_none_or(|status| status.checked_on.days_until(self.today) >= STATUS_DAYS)
    }

    /// Who is no longer active, for many names at once, before the creators
    /// are gone through one by one. Not knowing is no reason to skip them,
    /// but a batch Wikidata did not answer leaves the run incomplete and
    /// its creators in `Classified::unknown`, to be asked again.
    fn inactive(&mut self, due: &[&Creator]) -> Result<Classified, Stopped> {
        let mut result = Classified::default();
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
                if self.stopped() {
                    return Err(Stopped);
                }
                // Wikidata's answer through the source that has it. A
                // refusal says nothing about that source itself, so it is
                // not marked down, but the run is not complete either.
                let mut answer = None;
                for source in &sources {
                    match source.inactive_of(kind, &names) {
                        Ok(Some(found)) => {
                            answer = Some(found);
                            break;
                        }
                        Ok(None) => {}
                        Err(failure) => {
                            eprintln!("Discover: who died: {failure}");
                            if failure.kind != FailureKind::NotConfigured {
                                self.complete = false;
                            }
                            break;
                        }
                    }
                }
                for creator in batch {
                    match &answer {
                        Some(found) => {
                            if let Some(why) = found.get(&creator.name) {
                                result.found.insert(creator.key(), why.clone());
                            }
                        }
                        None => {
                            result.unknown.insert(creator.key());
                        }
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
        let (followed, others) = self.creators_split();
        for creator in &followed {
            self.owners.insert(creator.key());
        }
        if !ask {
            return Ok(());
        }
        let due: Vec<&Creator> = followed.iter().filter(|c| self.creator_due(c)).collect();
        // Who is not followed is still asked who they are, so the
        // Following window can list them under No longer active.
        let unfollowed: Vec<&Creator> = others.iter().filter(|c| self.status_due(c)).collect();
        let asked: Vec<&Creator> = due.iter().chain(&unfollowed).copied().collect();
        let classified = self.inactive(&asked)?;
        let said = |key: &str| {
            if classified.unknown.contains(key) {
                Classification::Unknown
            } else {
                Classification::Known(classified.found.get(key))
            }
        };
        for creator in due {
            self.progress.tick();
            self.creator(creator, said(&creator.key()))?;
        }
        for creator in unfollowed {
            self.progress.tick();
            self.status_only(creator, said(&creator.key()))?;
        }
        Ok(())
    }

    /// The creator's kinds' sources that are set up.
    fn creator_sources(&self, kind: FollowKind) -> Vec<&'a dyn Source> {
        self.service
            .sources()
            .iter()
            .map(|s| s.as_ref())
            .filter(|s| s.is_configured() && s.creator_kinds().contains(&kind))
            .collect()
    }

    /// What is known of a creator now: the provider's id and why they are
    /// no longer active, from what was just asked and what was known.
    fn status_of(
        &self,
        known: Option<CreatorStatus>,
        found: Option<lookup::CreatorMatch>,
        inactive: Classification,
    ) -> CreatorStatus {
        CreatorStatus {
            provider_id: found
                .as_ref()
                .map(|hit| hit.provider_id.clone())
                .or_else(|| known.as_ref().and_then(|k| k.provider_id.clone())),
            inactive: inactive
                .found()
                .cloned()
                .or_else(|| found.and_then(|hit| hit.inactive))
                .or_else(|| known.and_then(|k| k.inactive)),
            // Not asked of Wikidata: already due again, so the next
            // run asks rather than taking them as alive for 90 days.
            checked_on: match inactive {
                Classification::Unknown => self.today.add_days(-STATUS_DAYS),
                Classification::Known(_) => self.today,
            },
        }
    }

    /// A creator who is not followed: only whether they are still active.
    /// Wikidata has said it for an author, an artist or a studio; TMDB
    /// says it with a director's or an actor's own record.
    fn status_only(&mut self, creator: &Creator, inactive: Classification) -> Result<(), Stopped> {
        let key = creator.key();
        let known = self.record.creators.get(&key).cloned();
        let mut found = None;
        if matches!(creator.kind, FollowKind::Directors | FollowKind::Actors) {
            for source in self.creator_sources(creator.kind) {
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
        }
        let status = self.status_of(known, found, inactive);
        self.record.creators.insert(key.clone(), status);
        self.record.asked.insert(status_key(&key));
        self.save();
        Ok(())
    }

    fn creator(&mut self, creator: &Creator, inactive: Classification) -> Result<(), Stopped> {
        let key = creator.key();
        let sources = self.creator_sources(creator.kind);
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
            let status = self.status_of(known, found, inactive);
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
            if hit.media == MediaType::Books && !self.languages.is_empty() {
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
            } else if hit.media == MediaType::Games {
                hit.released = hit.released_for(self.settings.platform.as_deref());
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
            let count = creator.entries.len();
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
                owner_name: creator.name.clone(),
                source: found_key.clone(),
                key: found_key,
                provider: source.name().to_owned(),
                cover_url: hit.cover_url.clone(),
                kind: Kind::Creator,
                reasons: Vec::new(),
            });
        }
        self.answered(&key, items);
        Ok(())
    }
}

/// What a status-only ask of the creator `key` is recorded under for the
/// day, apart from the follow's own key: a creator ticked later the same
/// day is still asked for new work.
fn status_key(key: &str) -> String {
    format!("status/{key}")
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
    use crate::model::{EntryStatus, codec};
    use crate::shared::{self, Edit};
    use std::sync::Mutex;

    fn unfollow(settings: &mut Settings, kind: Follow, id: &str) {
        settings.apply(&Edit::Follow {
            kind,
            ids: vec![id.to_owned()],
            on: false,
            default_on: kind.default_on(),
        });
    }

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
        let mut settings = Settings::default();
        let visible = state.visible(&library, &settings).len();
        assert!(visible >= 5);
        // Not interested hides one; Restore brings it back.
        settings.apply(&Edit::Hide("fake:movie:hunt-for-gollum".into()));
        assert_eq!(state.visible(&library, &settings).len(), visible - 1);
        assert_eq!(state.hidden_items(&library, &settings).len(), 1);
        settings.apply(&Edit::Restore("fake:movie:hunt-for-gollum".into()));
        assert_eq!(state.visible(&library, &settings).len(), visible);
        // With the check switched off nothing is listed.
        settings.apply(&Edit::Enabled(false));
        assert!(state.visible(&library, &settings).is_empty());
        assert!(state.hidden_items(&library, &settings).is_empty());
        settings.apply(&Edit::Enabled(true));
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
        assert_eq!(state.visible(&library, &settings).len(), visible - 1);
        // A title added by hand drops its item too.
        let mut gollum = library.entries[0].clone();
        gollum.id = "movies/the-lord-of-the-rings-the-hunt-for-gollum-2027".into();
        gollum.title = "The Lord of the Rings: The Hunt for Gollum".into();
        gollum.source = None;
        library.entries.push(gollum);
        assert_eq!(state.visible(&library, &settings).len(), visible - 2);
        assert!(state.checked_today(Timestamp::parse("2026-10-19T20:00:00Z").unwrap()));
        assert!(!state.checked_today(Timestamp::parse("2026-10-20T20:00:00Z").unwrap()));
    }

    #[test]
    fn the_record_reads_back_from_its_file() {
        let dir = storage::TempDir::new("discover");
        let path = dir.0.as_path();
        let outcome = fake_run(&library());
        let now = Timestamp::parse("2026-10-19T09:12:00Z").unwrap();
        {
            let mut state = State::at(Some(path));
            state.finish(outcome.clone(), now);
        }
        let mut state = State::at(Some(path));
        assert_eq!(state.record.checked, Some(now));
        assert_eq!(state.record.items, outcome.items);
        assert_eq!(state.record.new, outcome.items.len(), "all of them new");
        let line = state.check_line(now).unwrap();
        assert!(
            line.starts_with("Checked today ") && line.ends_with(" · Fake"),
            "{line}"
        );
        // An incomplete run changes nothing.
        let before = state.record.clone();
        state.finish(Outcome::failed("Fake: Offline"), now);
        assert_eq!(state.record, before);
    }

    #[test]
    fn last_checked_reads_as_the_settings_row_does() {
        let mut state = State::default();
        // Noon, so the day is the same in any time zone this runs in.
        let now = Timestamp::parse("2026-10-19T12:00:00Z").unwrap();
        assert_eq!(state.last_checked(now), "Never");
        state.record.checked = Some(Timestamp(now.0 - 600));
        let line = state.last_checked(now);
        assert!(
            line.starts_with("Today ") && !line.contains("new"),
            "{line}"
        );
        state.record.new = 3;
        state.record.new_on = Some(now.local_date());
        let line = state.last_checked(now);
        assert!(
            line.starts_with("Today ") && line.ends_with(" · 3 new"),
            "{line}"
        );
        let line = state.last_checked(Timestamp(now.0 + 86_400));
        assert!(line.starts_with("Yesterday "), "{line}");
        // Further back it is the day alone.
        let line = state.last_checked(Timestamp(now.0 + 7 * 86_400));
        assert_eq!(line, "Oct 19 · 3 new");
        // While a run is out the row counts.
        state.look_checking(184, 423);
        assert_eq!(state.last_checked(now), "Checking 184 of 423…");
        assert!((state.fraction() - 184.0 / 423.0).abs() < 1e-6);
    }

    #[test]
    fn a_title_found_for_several_follows_is_one_row_with_every_reason() {
        let library = library();
        let outcome = fake_run(&library);
        let mut record = outcome.record.unwrap();
        // The series' next part was found for its director too.
        let part = record
            .items
            .iter()
            .find(|i| i.key == "fake:movie:hunt-for-gollum")
            .cloned()
            .expect("the fake's next part");
        assert_eq!(part.owner_name, "The Lord of the Rings");
        let mut again = part.clone();
        again.owner = "directors/andy-serkis".into();
        again.owner_name = "Andy Serkis".into();
        record.items.push(again);
        let mut state = State::default();
        let complete = Outcome {
            items: record.items.clone(),
            sources: Vec::new(),
            complete: true,
            stopped: None,
            record: Some(record),
        };
        state.finish(complete, Timestamp::parse("2026-10-19T09:12:00Z").unwrap());
        let mut settings = Settings::default();
        let rows = state.visible(&library, &settings);
        let row = rows
            .iter()
            .find(|i| i.key == part.key)
            .expect("listed once");
        assert_eq!(rows.iter().filter(|i| i.key == part.key).count(), 1);
        let reasons: Vec<(Follow, &str)> = row
            .reasons
            .iter()
            .map(|r| (r.kind, r.name.as_str()))
            .collect();
        assert_eq!(
            reasons,
            [
                (Follow::Series, "The Lord of the Rings"),
                (Follow::Directors, "Andy Serkis")
            ]
        );
        // One reason unfollowed: the title stays for the other.
        let series = row.reasons[0].id.clone();
        unfollow(&mut settings, Follow::Series, &series);
        let row = state.item(&part.key).unwrap();
        assert_eq!(state.live_reasons(&settings, row).len(), 1);
        assert!(
            state
                .visible(&library, &settings)
                .iter()
                .any(|i| i.key == part.key)
        );
        // The last one too: it leaves at once, with no run in between,
        // and is not kept under Hidden either.
        unfollow(&mut settings, Follow::Directors, "andy-serkis");
        assert!(
            !state
                .visible(&library, &settings)
                .iter()
                .any(|i| i.key == part.key)
        );
        assert!(state.hidden_items(&library, &settings).is_empty());
        // Followed again it is back, as the record still has it.
        assert!(
            state
                .visible(&library, &Settings::default())
                .iter()
                .any(|i| i.key == part.key)
        );
    }

    #[test]
    fn a_run_keeps_one_item_for_each_follow_that_found_a_title() {
        let library = library();
        let mut record = fake_run(&library).record.unwrap();
        let part = record
            .items
            .iter()
            .find(|i| i.key == "fake:movie:hunt-for-gollum")
            .cloned()
            .unwrap();
        // A second follow's copy, as an earlier run of its share left it.
        let author = record
            .items
            .iter()
            .find(|i| i.owner.starts_with("authors/"))
            .map(|i| i.owner.clone())
            .expect("the fake finds an author's book");
        let mut copy = part.clone();
        copy.owner = author.clone();
        record.items.push(copy);
        let outcome = run(
            &Service::fake(),
            &library,
            today(),
            &Settings::default(),
            &record,
            None,
        );
        let owners: Vec<&str> = outcome
            .items
            .iter()
            .filter(|i| i.key == part.key)
            .map(|i| i.owner.as_str())
            .collect();
        assert_eq!(owners.len(), 2, "{owners:?}");
        assert!(owners.contains(&author.as_str()));
    }

    #[test]
    fn an_unfinished_run_goes_again_and_a_finished_one_waits_for_tomorrow() {
        // Noon, so an hour on stays on the same date in any time zone
        // this runs in, and a day on does not.
        let now = Timestamp::parse("2026-10-19T12:00:00Z").unwrap();
        let hour_on = Timestamp(now.0 + 3_600);
        let mut state = State::at(None);
        assert!(state.is_due(now));
        let _ = state.start(false);
        assert!(!state.is_due(now), "running");
        state.finish(Outcome::failed("Fake: Offline"), now);
        assert!(state.is_due(hour_on), "not finished");
        let _ = state.start(false);
        let outcome = fake_run(&library());
        state.finish(outcome, hour_on);
        assert!(!state.is_due(hour_on), "finished today");
        assert!(state.is_due(Timestamp(now.0 + 86_400)));
    }

    /// The fake with its requests counted, and refusing while `limited`;
    /// its Wikidata knows Miles Davis died, and refuses while
    /// `wikidata_limited`.
    #[derive(Default)]
    struct Counting {
        asked: Mutex<Vec<String>>,
        limited: AtomicBool,
        wikidata_limited: AtomicBool,
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
        fn inactive_of(
            &self,
            _kind: FollowKind,
            names: &[String],
        ) -> Reply<Option<std::collections::HashMap<String, String>>> {
            if self.wikidata_limited.load(Ordering::Relaxed) {
                return Err(lookup::Failure::new(FailureKind::RateLimited, "429"));
            }
            Ok(Some(
                names
                    .iter()
                    .filter(|name| *name == "Miles Davis")
                    .map(|name| (name.clone(), "Died 1991".to_owned()))
                    .collect(),
            ))
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
    fn what_is_unfollowed_is_never_asked_and_its_rows_go() {
        let (counting, service) = counting();
        let library = library();
        let settings = shared::decode(SETTINGS).unwrap();
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
    fn who_died_is_skipped_and_a_refused_ask_is_asked_again() {
        let (counting, service) = counting();
        let library = library();
        let settings = Settings::default();
        let davis = "artists/miles davis";
        let found_by =
            |outcome: &Outcome, owner: &str| outcome.items.iter().any(|item| item.owner == owner);
        // Wikidata refuses: Miles Davis is still checked (not knowing is
        // no reason to skip), but the run is not complete and his status
        // is already due again rather than alive for ninety days.
        counting.wikidata_limited.store(true, Ordering::Relaxed);
        let refused = run(
            &service,
            &library,
            today(),
            &settings,
            &Record::default(),
            None,
        );
        assert!(!refused.complete);
        let record = refused.record.clone().unwrap();
        let status = &record.creators[davis];
        assert_eq!(status.inactive, None);
        assert_eq!(status.checked_on.days_until(today()), STATUS_DAYS);
        // The next run asks again and learns he died: not followed, and
        // nothing of his is listed.
        counting.wikidata_limited.store(false, Ordering::Relaxed);
        let next_day = today().add_days(1);
        let answered = run(&service, &library, next_day, &settings, &record, None);
        let record = answered.record.clone().unwrap();
        assert_eq!(
            record.creators[davis].inactive.as_deref(),
            Some("Died 1991")
        );
        assert_eq!(record.creators[davis].checked_on, next_day);
        assert!(!found_by(&answered, davis));
        assert!(record.creators["artists/metallica"].inactive.is_none());
    }

    #[test]
    fn a_version_1_record_asks_again_who_died() {
        let json = json!({
            "version": 1,
            "creators": {
                "artists/eva cassidy": {"checked_on": "2026-11-07", "inactive": null},
                "artists/miles davis": {"checked_on": "2026-11-07", "inactive": "Died 1991"},
                "directors/clint-eastwood": {"checked_on": "2026-11-07", "inactive": null},
            },
        });
        let record = Record::from_json(&json).unwrap();
        let keys: Vec<&str> = record.creators.keys().map(String::as_str).collect();
        assert_eq!(keys, ["artists/miles davis", "directors/clint-eastwood"]);
        assert_eq!(record.to_json()["version"], RECORD_VERSION);
        assert!(Record::from_json(&json!({"version": 3})).is_none());
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
        let state = State::at(Some(dir.0.as_path()));
        assert!(!state.record.items.is_empty());
        assert!(!state.record.creators.is_empty());
    }

    #[test]
    fn check_now_asks_every_share_again_and_counts_what_it_asks() {
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
        assert!(first.complete, "{:?}", first.stopped);
        let record = first.record.unwrap();
        let asked = counting.asked.lock().unwrap().len();
        assert!(asked > 0);
        // The same day again: everything is done, nothing is asked.
        let options = Options::default();
        let again = run_with(
            &service,
            &library,
            today(),
            &settings,
            &record,
            None,
            &options,
        );
        assert!(again.complete);
        assert_eq!(counting.asked.lock().unwrap().len(), asked);
        assert_eq!(options.progress.counts(), (0, 0));
        // Check now: the whole set, whatever was asked today.
        let options = Options {
            force: true,
            ..Options::default()
        };
        let forced = run_with(
            &service,
            &library,
            today(),
            &settings,
            &record,
            None,
            &options,
        );
        assert!(forced.complete, "{:?}", forced.stopped);
        assert!(counting.asked.lock().unwrap().len() > asked);
        let (done, total) = options.progress.counts();
        assert!(total > 0 && done == total, "{done} of {total}");
        assert_eq!(forced.record.unwrap().new, 0, "nothing it had not found");
    }

    #[test]
    fn stop_ends_the_run_and_records_no_day() {
        let (counting, service) = counting();
        let library = library();
        let options = Options::default();
        options.progress.stop();
        let outcome = run_with(
            &service,
            &library,
            today(),
            &Settings::default(),
            &Record::default(),
            None,
            &options,
        );
        assert!(!outcome.complete);
        assert_eq!(outcome.stopped.as_deref(), Some("stopped"));
        assert!(counting.asked.lock().unwrap().is_empty(), "nothing asked");
        let record = outcome.record.unwrap();
        assert_eq!(record.last_run, None);
        assert!(record.done.is_empty());
    }

    #[test]
    fn someone_unfollowed_is_still_asked_whether_they_are_active() {
        let (counting, service) = counting();
        let library = library();
        let mut settings = Settings::default();
        unfollow(&mut settings, Follow::Artists, "miles davis");
        let outcome = run(
            &service,
            &library,
            today(),
            &settings,
            &Record::default(),
            None,
        );
        assert!(outcome.complete, "{:?}", outcome.stopped);
        let record = outcome.record.unwrap();
        // Wikidata was asked about him with the others, and nothing else
        // was: no search for him, no albums.
        let status = record
            .creators
            .get("artists/miles davis")
            .expect("his status is recorded");
        assert_eq!(status.inactive.as_deref(), Some("Died 1991"));
        assert_eq!(counting.count("find Miles Davis"), 0);
        assert!(
            !record
                .items
                .iter()
                .any(|i| i.owner == "artists/miles davis")
        );
        // The Following window lists him under No longer active.
        let follows = follows::Follows::derive(&library, &settings, &record.creators, true);
        let miles = follows.find(Follow::Artists, "miles davis").unwrap();
        assert!(!miles.followed);
        assert_eq!(miles.second_line(), "Died 1991 · Not checked");
        // And he is not asked again the same day, or for ninety days.
        let asked = counting.asked.lock().unwrap().len();
        let tomorrow = today().add_days(7);
        let _ = run(&service, &library, tomorrow, &settings, &record, None);
        let later: Vec<String> = counting.asked.lock().unwrap()[asked..].to_vec();
        assert!(
            !later.iter().any(|ask| ask.contains("Miles Davis")),
            "{later:?}"
        );
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
        // A real service: the fake's tagless fallback is off.
        let authors = follows::creators(&library, FollowKind::Authors, false);
        assert_eq!(authors.len(), 1);
        assert_eq!(authors[0].name, "J. K. Rowling");
        assert_eq!(authors[0].key(), "authors/j-k-rowling");
    }
}
