//! Which artists Apple is asked about, and the batch that waits for its
//! commit.
//!
//! Apple is asked only about an artist with no file and no fresh entry
//! in `_missing.json` (`todo`), one at a time, alphabetically. What comes
//! of it (a photo, or "nothing found") is not queued for GitHub at once:
//! it is held here, in `backfill.json` beside `pending.json`, until fifty
//! artists are tried or none is left, and then queued whole, so a batch
//! is one commit with its part of `_missing.json` in it and a library
//! change made meanwhile does not carry half a batch. The photos
//! themselves are in `covers/artists/` from the moment they come, and the
//! screens show them at once. Stopping anywhere loses nothing: the next
//! start reads the held batch and goes on with whoever is left.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::artists::{self, MissingEdit};
use super::{Change, Covers};
use crate::lookup::store::JsonFile;
use crate::model::{Date, Library, MediaType};
use crate::music;

/// How many artists are tried before their batch is queued.
pub const BATCH: usize = 50;
/// How long the work waits after a failure that is not Apple refusing
/// (offline, a server error).
pub const RETRY: Duration = Duration::from_secs(10 * 60);
/// The `backfill.json` format.
const VERSION: i64 = 1;

/// One artist to ask Apple about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    /// The name as the library spells it (the first entry's spelling).
    pub name: String,
    pub slug: String,
    /// The album titles saved for the artist: their albums, and the
    /// albums their saved songs name.
    pub owned: Vec<String>,
}

/// Every artist of the library that can have a file (a name with a slug
/// that is not skipped), by slug, in the library's order, with the latest
/// day an entry of theirs was added.
fn artists_of(library: &Library) -> Vec<(Work, Date)> {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<(Work, Date)> = Vec::new();
    for entry in &library.entries {
        if entry.media != MediaType::Music {
            continue;
        }
        let name = music::artist_of(entry);
        if artists::is_skipped(name) {
            continue;
        }
        let Some(slug) = artists::slug(name) else {
            continue;
        };
        let index = *at.entry(slug.clone()).or_insert_with(|| {
            out.push((
                Work {
                    name: name.to_owned(),
                    slug,
                    owned: Vec::new(),
                },
                entry.added,
            ));
            out.len() - 1
        });
        let (work, added) = &mut out[index];
        *added = (*added).max(entry.added);
        let title = if music::is_song(entry) {
            entry.album.as_deref().unwrap_or_default()
        } else {
            entry.title.as_str()
        };
        if !title.trim().is_empty() && !work.owned.iter().any(|owned| owned == title) {
            work.owned.push(title.to_owned());
        }
    }
    out
}

/// How the library's artists stand: with a file, recorded as tried with
/// nothing found, or still to ask.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub artists: usize,
    pub with_photo: usize,
    pub no_photo: usize,
    pub no_match: usize,
}

impl Totals {
    /// The artists settled one way or the other.
    pub fn done(&self) -> usize {
        self.with_photo + self.no_photo + self.no_match
    }
}

pub fn totals(library: &Library, covers: &Covers) -> Totals {
    let mut totals = Totals::default();
    for (work, _) in artists_of(library) {
        totals.artists += 1;
        if covers.artist_file(&work.name).is_some() {
            totals.with_photo += 1;
        } else {
            match covers.missing().artists.get(&work.slug).map(|t| t.reason) {
                Some(artists::Reason::NoPhoto) => totals.no_photo += 1,
                Some(artists::Reason::NoMatch) => totals.no_match += 1,
                None => {}
            }
        }
    }
    totals
}

/// The artists to ask Apple about on `today`, A to Z as the Artists tab
/// orders names: those with no file and no entry in `_missing.json`, or
/// one that is due again (`Tried::is_due`). `tried_now` are the slugs
/// asked about in this session; an artist recorded today whose newest
/// entry was added today is asked once more unless this session did the
/// asking, since the entry may be the reason to look again.
pub fn todo(
    library: &Library,
    covers: &Covers,
    today: Date,
    tried_now: &HashSet<String>,
) -> Vec<Work> {
    let mut out: Vec<Work> = artists_of(library)
        .into_iter()
        .filter(|(work, added)| {
            if covers.artist_file(&work.name).is_some() {
                return false;
            }
            match covers.missing().artists.get(&work.slug) {
                None => !tried_now.contains(&work.slug),
                Some(tried) => {
                    tried.is_due(today, Some(*added))
                        || (tried.tried == today
                            && *added == today
                            && !tried_now.contains(&work.slug))
                }
            }
        })
        .map(|(work, _)| work)
        .collect();
    out.sort_by_key(|work| music::artist_key(&work.name));
    out
}

/// The photo work of this session and the batch held for its commit.
#[derive(Debug, Default)]
pub struct State {
    file: Option<JsonFile>,
    /// The changes of the batch so far, in the order they were made.
    held: Vec<Change>,
    /// How many artists the batch has tried.
    tried: usize,
    /// Paused in Settings; kept over a restart.
    pub paused: bool,
    /// Apple refused (a 403 or a 429): nothing more is asked in this
    /// session, until Try again.
    pub stopped: bool,
    /// The artist being asked about.
    pub running: Option<String>,
    /// After a failure that was not a refusal: when to go on.
    pub retry_at: Option<Instant>,
    tried_now: HashSet<String>,
    /// Whether an artist was passed over in this session after a failure
    /// that was not an answer: the backfill is then not done.
    passed_over: bool,
}

impl State {
    /// The state over `backfill.json` in `dir`, the library's folder.
    pub fn at(dir: Option<&Path>) -> State {
        let mut state = State {
            file: dir.map(|dir| JsonFile(dir.join("backfill.json"))),
            ..State::default()
        };
        if let Some(json) = state.file.as_ref().and_then(JsonFile::read)
            && json["version"].as_i64() == Some(VERSION)
        {
            state.paused = json["paused"].as_bool().unwrap_or(false);
            state.tried = json["tried"].as_u64().unwrap_or(0) as usize;
            state.held = json["changes"]
                .as_array()
                .map(|changes| changes.iter().filter_map(Change::from_json).collect())
                .unwrap_or_default();
        }
        state
    }

    pub fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        if self.held.is_empty() && !self.paused {
            file.delete();
            return;
        }
        let changes: Vec<Value> = self.held.iter().map(Change::to_json).collect();
        let _ = file.write(&json!({
            "version": VERSION,
            "paused": self.paused,
            "tried": self.tried,
            "changes": changes,
        }));
    }

    /// The batch's changes, which the screens count as queued.
    pub fn held(&self) -> &[Change] {
        &self.held
    }

    pub fn tried_now(&self) -> &HashSet<String> {
        &self.tried_now
    }

    /// Whether the batch is whole.
    pub fn is_full(&self) -> bool {
        self.tried >= BATCH
    }

    /// An artist was asked about and Apple gave a real answer: `change`
    /// (the photo's `Put`, or the `_missing.json` entry) joins the batch.
    pub fn tried(&mut self, slug: &str, change: Change) {
        self.tried_now.insert(slug.to_owned());
        self.held.push(change);
        self.tried += 1;
        self.save();
    }

    /// An artist was asked about with nothing to keep in a batch (the
    /// library is kept on this computer, where a change is made at once).
    pub fn asked(&mut self, slug: &str) {
        self.tried_now.insert(slug.to_owned());
    }

    /// Apple's answer about the artist could not be used (a server
    /// error, a broken image): not recorded, and not asked again in this
    /// session, so one artist never holds up the rest.
    pub fn pass_over(&mut self, slug: &str) {
        self.tried_now.insert(slug.to_owned());
        self.passed_over = true;
    }

    /// Whether everyone left was asked with an answer: with nobody to
    /// ask, the backfill is done.
    pub fn none_passed_over(&self) -> bool {
        !self.passed_over
    }

    /// Takes the batch for the queue: its changes and the summary its
    /// commit is named after. `done` says no artist is left, so the
    /// backfill's day is written unless the record has one.
    pub fn take(&mut self, done: Option<Date>) -> Option<(Vec<Change>, String)> {
        let mut changes = std::mem::take(&mut self.held);
        self.tried = 0;
        if let Some(day) = done {
            changes.push(Change::Missing(MissingEdit::Backfilled(day)));
        }
        self.save();
        if changes.is_empty() {
            return None;
        }
        let photos = changes
            .iter()
            .filter(|change| matches!(change, Change::Put { .. }))
            .count();
        let without = changes
            .iter()
            .filter(|change| matches!(change, Change::Missing(MissingEdit::Set { .. })))
            .count();
        let plural = |n: usize| if n == 1 { "artist" } else { "artists" };
        let summary = match (photos, without) {
            (0, 0) => "Mark the artist photos as fetched".to_owned(),
            (0, n) => format!("Note {n} {} without a photo", plural(n)),
            (n, 0) => format!("Add photos of {n} {}", plural(n)),
            (n, m) => format!("Add photos of {n} {}, {m} without one", plural(n)),
        };
        Some((changes, summary))
    }

    /// Forgets the batch (Disconnect).
    pub fn clear(&mut self) {
        self.held.clear();
        self.tried = 0;
        self.running = None;
        self.retry_at = None;
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::covers::File;
    use crate::covers::artists::{Reason, Tried};
    use crate::model::{Entry, EntryKind};
    use crate::storage::TempDir;

    fn day(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    /// A library of the fixture's first album, copied under other names.
    fn library(made: &[(&str, &str, bool, &str)]) -> Library {
        let mut library = crate::model::codec::decode(crate::fixtures::LIBRARY_JSON).unwrap();
        let base: Entry = library
            .entries
            .iter()
            .find(|e| e.media == MediaType::Music && !music::is_song(e))
            .cloned()
            .unwrap();
        library.entries = made
            .iter()
            .enumerate()
            .map(|(i, (artist, title, song, added))| {
                let mut entry = base.clone();
                entry.id = format!("music/{i}");
                entry.creator = (!artist.is_empty()).then(|| (*artist).to_owned());
                entry.title = (*title).to_owned();
                entry.kind = song.then_some(EntryKind::Song);
                entry.album = song.then(|| format!("{title} (the album)"));
                entry.added = day(added);
                entry
            })
            .collect();
        library
    }

    fn names(work: &[Work]) -> Vec<&str> {
        work.iter().map(|w| w.name.as_str()).collect()
    }

    #[test]
    fn apple_is_asked_about_artists_with_no_file_and_no_fresh_record() {
        let library = library(&[
            ("The Beatles", "Abbey Road", false, "2026-01-01"),
            ("Adele", "21", false, "2026-01-01"),
            ("adele", "Hello", true, "2026-02-01"),
            ("Various Artists", "Drive", false, "2026-01-01"),
            ("", "Untitled", false, "2026-01-01"),
            ("!!!", "Louden Up Now", false, "2026-01-01"),
            ("Can", "Tago Mago", false, "2026-01-01"),
            ("Burial", "Untrue", false, "2026-01-01"),
        ]);
        let today = day("2026-11-15");
        let none = HashSet::new();
        let mut covers = Covers::with_files(Vec::new());
        // A to Z with a leading "The" aside; the skipped names and the
        // name with no letters are never asked about.
        let work = todo(&library, &covers, today, &none);
        assert_eq!(names(&work), ["Adele", "The Beatles", "Burial", "Can"]);
        // An artist's albums, and the albums their songs name.
        assert_eq!(work[0].owned, ["21", "Hello (the album)"]);
        assert_eq!(
            totals(&library, &covers),
            Totals {
                artists: 4,
                ..Totals::default()
            }
        );

        // A file, and two records: one fresh, one ninety days old.
        covers = Covers::with_files(vec![File {
            path: "artists/can.jpg".into(),
            size: 1,
            sha: "sha".into(),
        }]);
        let tried = |name: &str, on: &str, reason| {
            Change::Missing(MissingEdit::Set {
                slug: artists::slug(name).unwrap(),
                tried: Tried {
                    name: name.into(),
                    tried: day(on),
                    reason,
                },
            })
        };
        covers.rebuild(&[
            tried("Burial", "2026-11-01", Reason::NoPhoto),
            tried("The Beatles", "2026-08-01", Reason::NoMatch),
        ]);
        let work = todo(&library, &covers, today, &none);
        assert_eq!(names(&work), ["Adele", "The Beatles"]);
        let totals = totals(&library, &covers);
        assert_eq!(
            (totals.with_photo, totals.no_photo, totals.no_match),
            (1, 1, 1)
        );
        assert_eq!(totals.done(), 3);
        // Asked in this session with no answer kept: not again.
        let asked: HashSet<String> = ["adele".to_owned()].into();
        assert_eq!(
            names(&todo(&library, &covers, today, &asked)),
            ["The Beatles"]
        );
    }

    #[test]
    fn an_entry_added_after_the_try_asks_again() {
        let mut library = library(&[("Burial", "Untrue", false, "2026-11-01")]);
        let mut covers = Covers::with_files(Vec::new());
        covers.rebuild(&[Change::Missing(MissingEdit::Set {
            slug: "burial".into(),
            tried: Tried {
                name: "Burial".into(),
                tried: day("2026-11-10"),
                reason: Reason::NoMatch,
            },
        })]);
        let none = HashSet::new();
        assert!(todo(&library, &covers, day("2026-11-15"), &none).is_empty());
        library.entries[0].added = day("2026-11-12");
        assert_eq!(todo(&library, &covers, day("2026-11-15"), &none).len(), 1);
        // Added on the day of the try: once more on that day, by a
        // session that has not asked yet.
        library.entries[0].added = day("2026-11-10");
        assert_eq!(todo(&library, &covers, day("2026-11-10"), &none).len(), 1);
        let asked: HashSet<String> = ["burial".to_owned()].into();
        assert!(todo(&library, &covers, day("2026-11-10"), &asked).is_empty());
        assert!(todo(&library, &covers, day("2026-11-11"), &none).is_empty());
    }

    #[test]
    fn a_batch_is_held_over_a_restart_and_taken_whole() {
        let dir = TempDir::new("backfill");
        let mut state = State::at(Some(&dir.0));
        let put = |slug: &str| Change::Put {
            path: format!("artists/{slug}.jpg"),
            seen: vec![None],
            picked: false,
        };
        let missing = Change::Missing(MissingEdit::Set {
            slug: "burial".into(),
            tried: Tried {
                name: "Burial".into(),
                tried: day("2026-11-15"),
                reason: Reason::NoPhoto,
            },
        });
        state.tried("adele", put("adele"));
        state.tried("burial", missing.clone());
        assert!(!state.is_full());

        let mut again = State::at(Some(&dir.0));
        assert_eq!(again.held(), [put("adele"), missing.clone()]);
        // Not the last batch: no backfill day.
        let (changes, summary) = again.take(None).unwrap();
        assert_eq!(changes, [put("adele"), missing]);
        assert_eq!(summary, "Add photos of 1 artist, 1 without one");
        assert!(again.held().is_empty());
        assert!(!dir.path("backfill.json").exists());
        assert!(State::at(Some(&dir.0)).held().is_empty());

        // The last batch writes the day, even with nothing else in it.
        let (changes, summary) = again.take(Some(day("2026-11-16"))).unwrap();
        assert_eq!(
            changes,
            [Change::Missing(MissingEdit::Backfilled(day("2026-11-16")))]
        );
        assert_eq!(summary, "Mark the artist photos as fetched");
        assert_eq!(again.take(None), None);

        for i in 0..BATCH {
            state.tried(&format!("a{i}"), put(&format!("a{i}")));
        }
        assert!(state.is_full());
    }
}
