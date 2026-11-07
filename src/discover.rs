//! Discover: titles not in the library that the daily check finds for
//! things that are (decided 2026-11-07; the phone's DESIGN.md 10c has the
//! checks it follows): the next season of a show, the next part of a
//! followed series (a part Done or In progress), a new book by an author
//! in the library and a new album by an album artist. Each item carries
//! its type, title, an optional sub ("Season 3"), the creator, its release
//! as precise as the provider has it, a "why" line, a description, the
//! provider and the lookup id.
//!
//! The check runs once a day (`State::checked_today`): a little after the
//! start, then on the hour's timer until it has finished that day, whether
//! the window has focus or not. Its
//! answer is kept in `discover.json` under the cache folder with the time
//! of the check and the providers asked, so the page shows at once and
//! the footer can say "Checked today 09:12 · TMDB, IGDB, Open Library,
//! Deezer"; every run replaces the record, so the dates are refreshed.
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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::lookup::{self, Match, Reply, Service, Source};
use crate::model::{Date, Entry, EntryStatus, Library, MediaType, Released, Series};
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
        match &self.kind {
            Kind::Season { show, number } => library
                .entry(show)
                .is_none_or(|e| e.season_list().iter().any(|s| s.season == *number)),
            Kind::Part { .. } | Kind::Creator => library.entries.iter().any(|e| {
                e.source.as_deref() == Some(self.source.as_str())
                    || (e.media == self.media && lookup::text::same_title(&e.title, &self.title))
            }),
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
    /// closed, so the day is not recorded and a later open asks again.
    pub complete: bool,
    /// Why an incomplete run stopped, for the trace: "Open Library:
    /// Offline: …", or "the window closed".
    pub stopped: Option<String>,
}

impl Outcome {
    /// A run that gave nothing, for a task that did not finish.
    pub fn failed(why: impl Into<String>) -> Outcome {
        Outcome {
            items: Vec::new(),
            sources: Vec::new(),
            complete: false,
            stopped: Some(why.into()),
        }
    }
}

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
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "version": RECORD_VERSION,
            "checked": self.checked.map(Timestamp::to_rfc3339),
            "sources": self.sources,
            "items": self.items,
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
        self.record
            .items
            .iter()
            .filter(|item| !item.in_library(library) && !self.hidden.contains(&item.key))
            .collect()
    }

    /// The hidden items still worth restoring: not in the library.
    pub fn hidden_items(&self, library: &Library) -> Vec<&Found> {
        self.record
            .items
            .iter()
            .filter(|item| !item.in_library(library) && self.hidden.contains(&item.key))
            .collect()
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

    /// The check came back: a complete run replaces the record and is
    /// written; an incomplete one leaves it, so the next open tries again.
    pub fn finish(&mut self, outcome: Outcome, now: Timestamp) {
        self.running = false;
        if !outcome.complete {
            return;
        }
        self.record = Record {
            checked: Some(now),
            sources: outcome.sources,
            items: outcome.items,
        };
        if let Some(path) = &self.file {
            let mut text =
                serde_json::to_string_pretty(&self.record.to_json()).expect("record encode");
            text.push('\n');
            if let Err(error) = storage::write_atomically(path, &text) {
                eprintln!("libri: {} could not be written: {error}", path.display());
            }
        }
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

/// Runs the whole check against `service` for `library`: the next
/// seasons, the next parts of followed series, the creators' new work.
/// `previous` keeps the day each item was first found. Blocks on the
/// providers, so the app runs it on the blocking pool (the fake answers
/// at once). A provider that is offline, busy or down ends the run as
/// incomplete, with the previous items kept.
pub fn run(service: &Service, library: &Library, today: Date, previous: &[Found]) -> Outcome {
    let mut check = Check {
        service,
        library,
        today,
        previous,
        items: Vec::new(),
        sources: Vec::new(),
    };
    let stopped = check
        .seasons()
        .and_then(|()| check.series())
        .and_then(|()| check.creators(MediaType::Books))
        .and_then(|()| check.creators(MediaType::Music))
        .err();
    if let Some(Stopped(why)) = stopped {
        return Outcome {
            items: previous.to_vec(),
            sources: check.sources,
            complete: false,
            stopped: Some(why),
        };
    }
    let mut seen = BTreeSet::new();
    check.items.retain(|item| seen.insert(item.key.clone()));
    Outcome {
        items: check.items,
        sources: check.sources,
        complete: true,
        stopped: None,
    }
}

/// A run in progress.
struct Check<'a> {
    service: &'a Service,
    library: &'a Library,
    today: Date,
    previous: &'a [Found],
    items: Vec<Found>,
    sources: Vec<String>,
}

/// A provider gave no verdict for now, or the window closed: the run
/// stops, with why.
struct Stopped(String);

impl Check<'_> {
    /// Asks `reply` of `source` (lazily, so a cancelled run sends
    /// nothing), notes the provider as asked, and turns a transient
    /// failure into a stop; anything else is nothing learned.
    fn ask<T>(
        &mut self,
        source: &dyn Source,
        reply: impl FnOnce() -> Reply<T>,
    ) -> Result<Option<T>, Stopped> {
        if cancelled() {
            return Err(Stopped("the window closed".into()));
        }
        let name = source.name().to_owned();
        if !self.sources.contains(&name) {
            self.sources.push(name);
        }
        match reply() {
            Ok(value) => Ok(Some(value)),
            Err(failure) if failure.is_transient() => {
                Err(Stopped(format!("{}: {failure}", source.name())))
            }
            Err(_) => Ok(None),
        }
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
            if let Some(Some(found)) = self.ask(source, reply)? {
                return Ok(Some((source.name(), found.source)));
            }
        }
        Ok(None)
    }

    fn first_found(&self, key: &str) -> Date {
        self.previous
            .iter()
            .find(|item| item.key == key)
            .map_or(self.today, |item| item.found)
    }

    fn previous_description(&self, key: &str) -> Option<String> {
        self.previous
            .iter()
            .find(|item| item.key == key && !item.description.is_empty())
            .map(|item| item.description.clone())
    }

    /// The next season of every show and anime series not dropped.
    fn seasons(&mut self) -> Result<(), Stopped> {
        let shows: Vec<&Entry> = self
            .library
            .entries
            .iter()
            .filter(|e| e.uses_seasons() && e.status != EntryStatus::Dnf && !e.seasonal_only)
            .collect();
        for show in shows {
            let Some((provider, id)) = self.source_for(show)? else {
                continue;
            };
            let Some(source) = self.service.source_of(&id) else {
                continue;
            };
            let Some(Some(listed)) = self.ask(source, || source.seasons(&id))? else {
                continue;
            };
            let highest = show
                .season_list()
                .iter()
                .map(|s| s.season)
                .max()
                .unwrap_or(0);
            for season in listed.seasons {
                let Some(released) = season.released else {
                    continue;
                };
                if season.season <= highest || released.is_released_on(self.today) {
                    continue;
                }
                let key = crate::upcoming::key_of(&show.id, Some(season.season));
                self.items.push(Found {
                    media: show.media,
                    title: show.title.clone(),
                    sub: Some(format!("Season {}", season.season)),
                    creator: show.creator.clone(),
                    released: Some(released),
                    why: format!("Next season of {}", show.title),
                    description: show.description.clone(),
                    found: self.first_found(&key),
                    key,
                    source: id.clone(),
                    provider: provider.to_owned(),
                    cover_url: None,
                    kind: Kind::Season {
                        show: show.id.clone(),
                        number: season.season,
                    },
                });
            }
        }
        Ok(())
    }

    /// The next part of every followed series: one with a part Done or In
    /// progress (Want alone is not following, as on the phone).
    fn series(&mut self) -> Result<(), Stopped> {
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
            let followed = parts
                .iter()
                .any(|p| matches!(p.status, EntryStatus::Done | EntryStatus::InProgress));
            if !followed {
                continue;
            }
            let lead = parts
                .iter()
                .copied()
                .find(|p| p.source.is_some())
                .unwrap_or(parts[0]);
            // Through the lead part's lookup id, else by the series' name
            // from the sources that list parts by name.
            let mut listed = None;
            let mut provider = "";
            if let Some((name_of, id)) = self.source_for(lead)?
                && let Some(source) = self.service.source_of(&id)
            {
                listed = self
                    .ask(source, || source.series_parts(&id, Some(&name)))?
                    .flatten();
                provider = name_of;
            }
            if listed.is_none() {
                let sources: Vec<&dyn Source> = self.service.sources_for(media);
                for source in sources {
                    let reply = || source.series_parts("", Some(&name));
                    if let Some(Some(found)) = self.ask(source, reply)? {
                        listed = Some(found);
                        provider = source.name();
                        break;
                    }
                }
            }
            let Some(listed) = listed else {
                continue;
            };
            let Some(source) = self
                .service
                .sources_for(media)
                .into_iter()
                .find(|s| s.name() == provider)
            else {
                continue;
            };
            let display = Series::display_name(&listed.name);
            let in_library = parts.len();
            for part in listed.parts {
                if part.extra {
                    continue;
                }
                let released = part.release();
                if released.is_none_or(|r| r.is_released_on(self.today)) {
                    continue;
                }
                let owned = parts.iter().any(|p| {
                    (part.source.is_some() && p.source == part.source)
                        || lookup::text::same_title(&p.title, &part.title)
                });
                if owned {
                    continue;
                }
                let key = part.source.clone().unwrap_or_else(|| {
                    format!(
                        "part:{}:{}",
                        media.key(),
                        lookup::text::title_key(&part.title)
                    )
                });
                let description = match (&part.source, self.previous_description(&key)) {
                    (_, Some(description)) => description,
                    (Some(part_source), None) => {
                        let probe = Match::new(media, part.title.clone(), part_source.clone());
                        self.ask(source, || source.details(&probe))?
                            .map(|full| full.description)
                            .unwrap_or_default()
                    }
                    (None, None) => String::new(),
                };
                self.items.push(Found {
                    media,
                    title: part.title.clone(),
                    sub: None,
                    creator: part.creator.clone().or_else(|| lead.creator.clone()),
                    released,
                    why: format!("Part of {display} · {in_library} in your library"),
                    description,
                    found: self.first_found(&key),
                    source: key.clone(),
                    key,
                    provider: provider.to_owned(),
                    cover_url: part.cover.clone(),
                    kind: Kind::Part {
                        series: display.clone(),
                    },
                });
            }
        }
        Ok(())
    }

    /// New work by the authors (books) or album artists (music) in the
    /// library: a creator is followed while one of their entries is not
    /// dropped.
    fn creators(&mut self, media: MediaType) -> Result<(), Stopped> {
        let mut creators: Vec<(String, usize, bool)> = Vec::new();
        for entry in self.library.entries_of(media) {
            if entry.seasonal_only {
                continue;
            }
            let Some(creator) = &entry.creator else {
                continue;
            };
            let names = if media == MediaType::Books {
                lookup::text::split_creators(creator)
            } else {
                vec![creator.trim().to_owned()]
            };
            for name in names {
                if name.is_empty()
                    || (media == MediaType::Music
                        && lookup::NO_PHOTO.contains(&name.to_lowercase().as_str()))
                {
                    continue;
                }
                let active = entry.status != EntryStatus::Dnf;
                match creators
                    .iter_mut()
                    .find(|(n, _, _)| n.eq_ignore_ascii_case(&name))
                {
                    Some((_, count, any_active)) => {
                        *count += 1;
                        *any_active |= active;
                    }
                    None => creators.push((name, 1, active)),
                }
            }
        }
        let sources: Vec<&dyn Source> = if media == MediaType::Music {
            self.service.music().into_iter().collect()
        } else {
            self.service.sources_for(media)
        };
        let Some(&source) = sources.first() else {
            return Ok(());
        };
        let noun = |n: usize| match (media, n) {
            (MediaType::Books, 1) => "1 book".to_owned(),
            (MediaType::Books, n) => format!("{n} books"),
            (_, 1) => "1 title".to_owned(),
            (_, n) => format!("{n} titles"),
        };
        for (name, count, active) in creators {
            if !active {
                continue;
            }
            let Some(found) = self.ask(source, || source.creator_releases(&name, media))? else {
                continue;
            };
            for hit in found {
                if hit.released.is_some_and(|r| r.is_released_on(self.today)) {
                    continue;
                }
                let key = hit.source.clone();
                self.items.push(Found {
                    media,
                    title: hit.title.clone(),
                    sub: None,
                    creator: hit.creator.clone().or_else(|| Some(name.clone())),
                    released: hit.released,
                    why: format!("{name} is in your library · {}", noun(count)),
                    description: hit.description.clone(),
                    found: self.first_found(&key),
                    source: key.clone(),
                    key,
                    provider: source.name().to_owned(),
                    cover_url: hit.cover_url.clone(),
                    kind: Kind::Creator,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::codec;

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    fn today() -> Date {
        Date::parse("2026-10-19").unwrap()
    }

    #[test]
    fn the_fake_check_finds_seasons_parts_and_creators_work() {
        let library = library();
        let outcome = run(&Service::fake(), &library, today(), &[]);
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
        let mut earlier = outcome.items.clone();
        earlier[0].found = Date::parse("2026-10-01").unwrap();
        let first_key = earlier[0].key.clone();
        let again = run(&Service::fake(), &library, today(), &earlier);
        let kept = again.items.iter().find(|i| i.key == first_key).unwrap();
        assert_eq!(kept.found, Date::parse("2026-10-01").unwrap());
    }

    #[test]
    fn items_in_the_library_and_hidden_ones_are_left_out() {
        let mut library = library();
        let outcome = run(&Service::fake(), &library, today(), &[]);
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
        let outcome = run(&Service::fake(), &library(), today(), &[]);
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
        let outcome = run(&Service::fake(), &library(), today(), &[]);
        state.finish(outcome, hour_on);
        assert!(!state.is_due(hour_on), "finished today");
        assert!(state.is_due(Timestamp(now.0 + 86_400)));
    }
}
