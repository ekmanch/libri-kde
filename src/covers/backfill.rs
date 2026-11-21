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
//!
//! The sharper photos (2026-11-21) are a second run, started by hand in
//! Settings: every photo that was found automatically is fetched once
//! more at 1200 × 1200, the one exception to "a file that exists is
//! never fetched again". Before anything is replaced the record's
//! `picked` list is built (`picked_from`: what this computer remembers of
//! each photo, checked against the commit history), and an artist on it
//! is never asked about (`sharper_todo`). The run shares the batch, the
//! pace and Pause with the fetch, and what it has tried is kept in
//! `backfill.json` (`Sharper`), so it goes on where it stopped.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::artists::{self, MissingEdit};
use super::{Change, Covers, PhotoKind};
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
    /// The titles of their saved songs, in the library's order: asked
    /// about only when no album tells who the artist is.
    pub songs: Vec<String>,
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
                    songs: Vec::new(),
                },
                entry.added,
            ));
            out.len() - 1
        });
        let (work, added) = &mut out[index];
        *added = (*added).max(entry.added);
        let title = if music::is_song(entry) {
            if !work.songs.contains(&entry.title) {
                work.songs.push(entry.title.clone());
            }
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

/// The slugs of the artists in the library file `text`, for the missing
/// record's merge (an artist no entry names leaves the record); `None`
/// for a file that cannot be read, which then drops nobody.
pub fn slugs_in(text: &str) -> Option<HashSet<String>> {
    let library = crate::model::codec::decode(text).ok()?;
    Some(
        artists_of(&library)
            .into_iter()
            .map(|(work, _)| work.slug)
            .collect(),
    )
}

/// The library's artist `name` as Apple is asked about them, with the
/// album titles saved for them.
pub fn work_for(library: &Library, name: &str) -> Option<Work> {
    let slug = artists::slug(name)?;
    artists_of(library)
        .into_iter()
        .map(|(work, _)| work)
        .find(|work| work.slug == slug)
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
/// one that is due again (`Tried::is_due`); with `again` (Check again in
/// Settings) every artist without a file that this session has not asked
/// about, whatever the record says. `tried_now` are the slugs
/// asked about in this session; an artist recorded today whose newest
/// entry was added today is asked once more unless this session did the
/// asking, since the entry may be the reason to look again.
pub fn todo(
    library: &Library,
    covers: &Covers,
    today: Date,
    tried_now: &HashSet<String>,
    again: bool,
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
                    (again && !tried_now.contains(&work.slug))
                        || tried.is_due(today, Some(*added))
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

/// The artists whose photo can be fetched again at a sharper size, A to
/// Z: those with a file that is not the owner's own choice (`picked` in
/// the record with the queued edits, or remembered here as picked or
/// uploaded) and that this run has not tried yet.
pub fn sharper_todo(library: &Library, covers: &Covers, state: &State) -> Vec<Work> {
    let Some(sharper) = &state.sharper else {
        return Vec::new();
    };
    let mut out: Vec<Work> = artists_of(library)
        .into_iter()
        .map(|(work, _)| work)
        .filter(|work| {
            covers.artist_file(&work.name).is_some()
                && !covers.is_picked(&work.slug)
                && !sharper.tried.contains(&work.slug)
                && !state.sharper_skipped.contains(&work.slug)
        })
        .collect();
    out.sort_by_key(|work| music::artist_key(&work.name));
    out
}

/// How many of the library's artists have a photo that was found
/// automatically, and how many one the owner picked or uploaded, as far
/// as this computer knows.
pub fn sharper_counts(library: &Library, covers: &Covers) -> (usize, usize) {
    let (mut found, mut yours) = (0, 0);
    for (work, _) in artists_of(library) {
        if covers.artist_file(&work.name).is_none() {
            continue;
        }
        match covers.is_picked(&work.slug) {
            true => yours += 1,
            false => found += 1,
        }
    }
    (found, yours)
}

/// What the commit history last said of each artist's photo, by slug:
/// true for "Set the photo of <name>" (a pick or an upload, on either
/// app), false for "Remove the photo of <name>" and "Add a photo of
/// <name>" (the phone's fetch). `messages` are the commits' messages,
/// newest first; a commit of several changes lists them in its body, one
/// "- " line each.
pub fn history_says(messages: &[String]) -> HashMap<String, bool> {
    const SAYS: [(&str, bool); 3] = [
        ("Set the photo of ", true),
        ("Remove the photo of ", false),
        ("Add a photo of ", false),
    ];
    let mut says = HashMap::new();
    for message in messages {
        let listed: Vec<&str> = message
            .lines()
            .skip(1)
            .filter_map(|line| line.strip_prefix("- "))
            .collect();
        let lines = match listed.is_empty() {
            true => message.lines().take(1).collect(),
            false => listed,
        };
        // The last line of a commit is its newest change.
        for line in lines.into_iter().rev() {
            for (start, set) in SAYS {
                if let Some(slug) = line.strip_prefix(start).and_then(artists::slug) {
                    says.entry(slug).or_insert(set);
                }
            }
        }
    }
    says
}

/// The artists whose photo counts as the owner's own, by slug, and the
/// names of those where this computer's memory and the commit history
/// say different things, who count as picked: remembered as picked or
/// uploaded with no "Set the photo of" as the history's last word, or
/// remembered as found automatically where the history's last word is
/// "Set the photo of". Without a history (the library kept on this
/// computer) the memory alone decides. Only artists with a file.
pub fn picked_from(
    library: &Library,
    covers: &Covers,
    history: Option<&[String]>,
) -> (BTreeSet<String>, Vec<String>) {
    let says = history.map(history_says);
    let mut picked = BTreeSet::new();
    let mut disagreed = Vec::new();
    for (work, _) in artists_of(library) {
        if covers.artist_file(&work.name).is_none() {
            continue;
        }
        let set = says.as_ref().map(|says| says.get(&work.slug).copied());
        let remembered = covers.kind_of_slug(&work.slug);
        let (mine, differs) = match (remembered, set) {
            (Some(PhotoKind::Picked | PhotoKind::Uploaded), None) => (true, false),
            (Some(PhotoKind::Picked | PhotoKind::Uploaded), Some(set)) => (true, set != Some(true)),
            (Some(PhotoKind::Fetched), Some(Some(true))) => (true, true),
            (None, Some(Some(true))) => (true, false),
            _ => (false, false),
        };
        if mine || covers.missing().picked.contains(&work.slug) {
            picked.insert(work.slug.clone());
        }
        if differs {
            disagreed.push(work.name.clone());
        }
    }
    disagreed.sort_by_key(|name| music::artist_key(name));
    (picked, disagreed)
}

/// A state of Settings' Artist photos row forced for a look, with the
/// canvas's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Fetching,
    Paused,
    Stopped,
    Sharper,
    Sharpening,
}

impl Look {
    pub fn from_env(value: &str) -> Option<Look> {
        match value {
            "fetching" => Some(Look::Fetching),
            "paused" => Some(Look::Paused),
            "stopped" => Some(Look::Stopped),
            "sharper" => Some(Look::Sharper),
            "sharpening" => Some(Look::Sharpening),
            _ => None,
        }
    }
}

/// How long one artist takes on average: the search, some five album
/// lists three seconds apart, the page and the image.
const SECONDS_AN_ARTIST: usize = 18;
/// And a sharper copy of a photo: mostly the image alone, at times the
/// page too.
const SECONDS_A_SHARPER: usize = 3;

/// How the sharper photos stand, for the row: whether they are on offer
/// (the record has no `photo_size` yet and this build writes it), how
/// many photos were found automatically and how many are the owner's,
/// and how many the run has left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sharp {
    pub offered: bool,
    pub found: usize,
    pub yours: usize,
    pub left: usize,
}

/// Settings › Library source › Your covers › Artist photos, as it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// Apple is being asked: how many artists are settled, of how many,
    /// and how many are left to ask.
    Fetching {
        done: usize,
        of: usize,
        left: usize,
    },
    Paused {
        done: usize,
        of: usize,
    },
    /// Apple refused; nothing more is asked until Try again.
    Stopped {
        done: usize,
        of: usize,
    },
    Done(Totals),
    /// The photos found automatically can be fetched again at 1200 ×
    /// 1200: how many, and how many of the owner's own are left alone.
    Sharper {
        found: usize,
        yours: usize,
    },
    /// The history is read to see which photos are the owner's, before
    /// any is replaced.
    Checking,
    Sharpening {
        done: usize,
        of: usize,
        left: usize,
    },
}

impl Row {
    /// The row for the work's state: `active` says photos are fetched in
    /// this run at all, `left` how many artists are still to ask, and
    /// `sharp` how the sharper photos stand.
    pub fn of(state: &State, totals: Totals, left: usize, active: bool, sharp: Sharp) -> Row {
        let (done, of) = (totals.done(), totals.artists);
        match state.look {
            Some(Look::Sharper) => {
                return Row::Sharper {
                    found: 197,
                    yours: 120,
                };
            }
            Some(Look::Sharpening) => {
                return Row::Sharpening {
                    done: 60,
                    of: 197,
                    left: 137,
                };
            }
            _ => {}
        }
        match state.look {
            Some(Look::Fetching) => {
                return Row::Fetching {
                    done: 150,
                    of: 377,
                    left: 227,
                };
            }
            Some(Look::Paused) => return Row::Paused { done: 150, of: 377 },
            Some(Look::Stopped) => return Row::Stopped { done: 150, of: 377 },
            _ => {}
        }
        if !active {
            return Row::Done(totals);
        }
        // New artists come first; the sharper photos once nobody is left.
        if left == 0 && (state.running.is_none() || state.sharper.is_some()) {
            return match &state.sharper {
                Some(sharper) => {
                    let of = sharper.of.max(sharper.tried.len() + sharp.left);
                    let done = of - sharp.left;
                    if state.stopped {
                        Row::Stopped { done, of }
                    } else if state.paused {
                        Row::Paused { done, of }
                    } else if !sharper.checked {
                        Row::Checking
                    } else {
                        Row::Sharpening {
                            done,
                            of,
                            left: sharp.left,
                        }
                    }
                }
                None if sharp.offered && sharp.found > 0 => Row::Sharper {
                    found: sharp.found,
                    yours: sharp.yours,
                },
                None => Row::Done(totals),
            };
        }
        if state.stopped {
            Row::Stopped { done, of }
        } else if state.paused {
            Row::Paused { done, of }
        } else {
            Row::Fetching { done, of, left }
        }
    }

    pub fn detail(&self) -> String {
        match *self {
            Row::Fetching { done, of, left } => format!(
                "Fetching from Apple Music · {done} of {of} · {}. One artist every few seconds; \
                 it pauses while you look something up.",
                time_left(left * SECONDS_AN_ARTIST)
            ),
            Row::Sharper { found, yours } => {
                let can = match found {
                    1 => "1 photo can".to_owned(),
                    n => format!("{n} photos can"),
                };
                let kept = match yours {
                    0 => String::new(),
                    1 => " The one you picked or uploaded is left alone.".to_owned(),
                    n => format!(" The {n} you picked or uploaded are left alone."),
                };
                format!("{can} be fetched again at a sharper size (1200 × 1200).{kept}")
            }
            Row::Checking => {
                "Checking which photos you picked or uploaded, before any is replaced…".to_owned()
            }
            Row::Sharpening { done, of, left } => format!(
                "Replacing photos with sharper ones · {done} of {of} · {}. Yours are left alone.",
                time_left(left * SECONDS_A_SHARPER)
            ),
            Row::Paused { done, of } => {
                format!("Paused at {done} of {of}. Nothing is lost; it picks up where it stopped.")
            }
            Row::Stopped { done, of } => format!(
                "Apple Music stopped answering, so fetching is off for now. {done} of {of} done."
            ),
            Row::Done(totals) => {
                let photos = match totals.with_photo {
                    1 => "1 photo".to_owned(),
                    n => format!("{n} photos"),
                };
                let covers = match totals.artists - totals.with_photo - totals.no_match {
                    1 => "1 artist uses a cover from your library".to_owned(),
                    n => format!("{n} artists use a cover from your library"),
                };
                format!(
                    "{photos} · {covers} · {} not matched. Kept in covers/artists/; new \
                     artists are fetched as you add them.",
                    totals.no_match
                )
            }
        }
    }

    pub fn button(&self) -> &'static str {
        match self {
            Row::Fetching { .. } | Row::Checking | Row::Sharpening { .. } => "Pause",
            Row::Sharper { .. } => "Get sharper photos",
            Row::Paused { .. } => "Resume",
            Row::Stopped { .. } => "Try again",
            Row::Done(_) => "Check again",
        }
    }

    /// How far the bar is, while there is one.
    pub fn progress(&self) -> Option<f32> {
        match *self {
            Row::Fetching { done, of, .. }
            | Row::Paused { done, of }
            | Row::Sharpening { done, of, .. } => Some(done as f32 / of.max(1) as f32),
            Row::Checking => Some(0.0),
            _ => None,
        }
    }
}

/// "about 40 minutes left", "about an hour left".
pub fn time_left(seconds: usize) -> String {
    let minutes = seconds.div_ceil(60);
    match minutes {
        0..=1 => "about a minute left".to_owned(),
        2..=49 => format!("about {minutes} minutes left"),
        50..=89 => "about an hour left".to_owned(),
        _ => format!("about {} hours left", (minutes + 30) / 60),
    }
}

/// The sharper photos' run, kept in `backfill.json` until everyone was
/// tried.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sharper {
    /// The `picked` list was built and is in the batch (or the record):
    /// nothing is replaced before.
    pub checked: bool,
    /// The slugs tried, replaced or left as they were.
    pub tried: BTreeSet<String>,
    /// How many there were to try when the run started.
    pub of: usize,
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
    /// Check again: the artists recorded with nothing found are asked
    /// about once more, whatever their day.
    pub again: bool,
    /// A state shown whatever is going on (`LIBRI_PHOTOS`), for a look.
    pub look: Option<Look>,
    /// Whether an artist was passed over in this session after a failure
    /// that was not an answer: the backfill is then not done.
    passed_over: bool,
    /// The sharper photos' run, while it is on.
    pub sharper: Option<Sharper>,
    /// The commit history is being read for it.
    pub checking: bool,
    /// The artists it passed over in this session after a failure that
    /// was not an answer; they are tried again in the next.
    sharper_skipped: HashSet<String>,
    /// The artists counted as picked because this computer's memory and
    /// the commit history disagreed, by name, shown under the row until
    /// Check again.
    pub disagreed: Vec<String>,
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
            let strings = |value: &Value| -> Vec<String> {
                let list = value.as_array().into_iter().flatten();
                list.filter_map(Value::as_str).map(str::to_owned).collect()
            };
            if let Some(sharper) = json.get("sharper").filter(|value| value.is_object()) {
                state.sharper = Some(Sharper {
                    checked: sharper["checked"].as_bool().unwrap_or(false),
                    tried: strings(&sharper["tried"]).into_iter().collect(),
                    of: sharper["of"].as_u64().unwrap_or(0) as usize,
                });
            }
            state.disagreed = strings(&json["disagreed"]);
        }
        state
    }

    pub fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        if self.held.is_empty()
            && !self.paused
            && self.sharper.is_none()
            && self.disagreed.is_empty()
        {
            file.delete();
            return;
        }
        let changes: Vec<Value> = self.held.iter().map(Change::to_json).collect();
        let mut json = json!({
            "version": VERSION,
            "paused": self.paused,
            "tried": self.tried,
            "changes": changes,
        });
        if let Some(sharper) = &self.sharper {
            json["sharper"] = json!({
                "checked": sharper.checked,
                "tried": sharper.tried,
                "of": sharper.of,
            });
        }
        if !self.disagreed.is_empty() {
            json["disagreed"] = json!(self.disagreed);
        }
        let _ = file.write(&json);
    }

    /// Get sharper photos: the run starts, with the history to read
    /// first.
    pub fn start_sharper(&mut self) {
        self.sharper = Some(Sharper::default());
        self.sharper_skipped.clear();
        self.disagreed.clear();
        self.save();
    }

    /// The `picked` list is built: `changes` (its edits and the photo
    /// size) join the batch without counting as artists tried, `of`
    /// artists are to try, and `disagreed` are the names to show.
    pub fn sharper_checked(&mut self, changes: Vec<Change>, of: usize, disagreed: Vec<String>) {
        self.checking = false;
        self.held.extend(changes);
        self.disagreed = disagreed;
        if let Some(sharper) = &mut self.sharper {
            sharper.checked = true;
            sharper.of = of;
        }
        self.save();
    }

    /// An artist's photo was tried: `change` when it was replaced, which
    /// joins the batch; none when it was left as it is.
    pub fn sharpened(&mut self, slug: &str, change: Option<Change>) {
        if let Some(sharper) = &mut self.sharper {
            sharper.tried.insert(slug.to_owned());
        }
        if let Some(change) = change {
            self.held.push(change);
            self.tried += 1;
        }
        self.save();
    }

    /// Apple's answer about the artist's sharper photo could not be
    /// used: tried again in the next session.
    pub fn skip_sharper(&mut self, slug: &str) {
        self.sharper_skipped.insert(slug.to_owned());
    }

    /// Nobody is left to try. The run is over unless someone was passed
    /// over, who is tried in the next session.
    pub fn sharper_done(&mut self) {
        if self.sharper_skipped.is_empty() {
            self.sharper = None;
        }
        self.save();
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

    /// Check again: everyone without a file is asked about once more,
    /// also those this session asked already.
    pub fn check_again(&mut self) {
        self.again = true;
        self.tried_now.clear();
        self.passed_over = false;
    }

    /// Use automatic: the artist may be asked about again in this
    /// session.
    pub fn ask_again(&mut self, slug: &str) {
        self.tried_now.remove(slug);
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
        let count = |is: fn(&Change) -> bool| changes.iter().filter(|c| is(c)).count();
        let photos = count(|change| matches!(change, Change::Put { .. }));
        let sharper = count(|change| matches!(change, Change::Sharpen { .. }));
        let without = changes
            .iter()
            .filter(|change| matches!(change, Change::Missing(MissingEdit::Set { .. })))
            .count();
        let plural = |n: usize| if n == 1 { "artist" } else { "artists" };
        let backfilled = count(|c| matches!(c, Change::Missing(MissingEdit::Backfilled(_)))) > 0;
        let summary = match (photos, without) {
            (0, 0) if sharper > 0 => {
                format!(
                    "Replace the photos of {sharper} {} with sharper ones",
                    plural(sharper)
                )
            }
            (0, 0) if backfilled => "Mark the artist photos as fetched".to_owned(),
            (0, 0) => "Note which artist photos are picked".to_owned(),
            (0, n) => format!("Note {n} {} without a photo", plural(n)),
            (n, 0) => format!("Add photos of {n} {}", plural(n)),
            (n, m) => format!("Add photos of {n} {}, {m} without one", plural(n)),
        };
        let summary = match (photos + without, sharper) {
            (0, _) | (_, 0) => summary,
            (_, n) => format!("{summary}, {n} sharper"),
        };
        Some((changes, summary))
    }

    /// Forgets the batch (Disconnect).
    pub fn clear(&mut self) {
        self.held.clear();
        self.tried = 0;
        self.running = None;
        self.retry_at = None;
        self.sharper = None;
        self.checking = false;
        self.sharper_skipped.clear();
        self.disagreed.clear();
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
        let work = todo(&library, &covers, today, &none, false);
        assert_eq!(names(&work), ["Adele", "The Beatles", "Burial", "Can"]);
        // An artist's albums, and the albums their songs name.
        assert_eq!(work[0].owned, ["21", "Hello (the album)"]);
        assert_eq!(work[0].songs, ["Hello"]);
        assert!(work[1].songs.is_empty());
        // The record's merge knows the same artists by their slugs.
        let slugs = slugs_in(&crate::model::codec::encode(&library)).expect("readable");
        let mut slugs: Vec<&str> = slugs.iter().map(String::as_str).collect();
        slugs.sort_unstable();
        assert_eq!(slugs, ["adele", "burial", "can", "the-beatles"]);
        assert_eq!(slugs_in("not a library"), None);
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
        let work = todo(&library, &covers, today, &none, false);
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
            names(&todo(&library, &covers, today, &asked, false)),
            ["The Beatles"]
        );
        // Check again asks about everyone without a file once more.
        assert_eq!(
            names(&todo(&library, &covers, today, &none, true)),
            ["Adele", "The Beatles", "Burial"]
        );
        assert_eq!(
            names(&todo(&library, &covers, today, &asked, true)),
            ["The Beatles", "Burial"]
        );
    }

    #[test]
    fn the_settings_row_reads_as_the_work_stands() {
        let totals = Totals {
            artists: 377,
            with_photo: 312,
            no_photo: 50,
            no_match: 7,
        };
        let mut state = State::default();
        let row = Row::of(&state, totals, 8, true, Sharp::default());
        assert_eq!(
            row,
            Row::Fetching {
                done: 369,
                of: 377,
                left: 8
            }
        );
        assert_eq!(row.button(), "Pause");
        assert!(
            row.detail()
                .starts_with("Fetching from Apple Music · 369 of 377 · about 3 minutes left.")
        );
        assert!(row.progress().is_some());
        state.paused = true;
        let row = Row::of(&state, totals, 8, true, Sharp::default());
        assert_eq!(row.button(), "Resume");
        assert!(row.detail().starts_with("Paused at 369 of 377."));
        state.stopped = true;
        let row = Row::of(&state, totals, 8, true, Sharp::default());
        assert_eq!(row.button(), "Try again");
        assert_eq!(row.progress(), None);
        // Nobody left, or a run that fetches nothing: the totals. An
        // artist with neither a photo nor "not matched" shows a cover
        // from the library (or initials).
        let row = Row::of(&State::default(), totals, 0, true, Sharp::default());
        assert_eq!(row.button(), "Check again");
        assert_eq!(
            row.detail(),
            "312 photos · 58 artists use a cover from your library · 7 not matched. Kept in \
             covers/artists/; new artists are fetched as you add them."
        );
        assert_eq!(
            Row::of(&State::default(), totals, 8, false, Sharp::default()),
            Row::Done(totals)
        );
        // The canvas's numbers for a look.
        state.look = Some(Look::Fetching);
        assert!(
            Row::of(&state, totals, 0, false, Sharp::default())
                .detail()
                .contains("150 of 377 · about an hour left")
        );
        assert_eq!(time_left(0), "about a minute left");
        assert_eq!(time_left(377 * SECONDS_AN_ARTIST), "about 2 hours left");
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
        assert!(todo(&library, &covers, day("2026-11-15"), &none, false).is_empty());
        library.entries[0].added = day("2026-11-12");
        assert_eq!(
            todo(&library, &covers, day("2026-11-15"), &none, false).len(),
            1
        );
        // Added on the day of the try: once more on that day, by a
        // session that has not asked yet.
        library.entries[0].added = day("2026-11-10");
        assert_eq!(
            todo(&library, &covers, day("2026-11-10"), &none, false).len(),
            1
        );
        let asked: HashSet<String> = ["burial".to_owned()].into();
        assert!(todo(&library, &covers, day("2026-11-10"), &asked, false).is_empty());
        assert!(todo(&library, &covers, day("2026-11-11"), &none, false).is_empty());
    }

    /// Covers with a photo for each of `slugs`, remembered as `kind`.
    fn photos(with: &[(&str, Option<PhotoKind>)]) -> Covers {
        let listed = |slug: &&str| File {
            path: format!("artists/{slug}.jpg"),
            size: 10,
            sha: String::new(),
        };
        let mut covers = Covers::with_files(with.iter().map(|(slug, _)| listed(slug)).collect());
        for (slug, kind) in with {
            let path = format!("artists/{slug}.jpg");
            covers.store(&path, slug.as_bytes()).unwrap();
            if let Some(kind) = kind {
                covers.set_kind(&path, *kind);
            }
        }
        covers
    }

    #[test]
    fn the_history_and_this_computer_say_which_photos_are_picked() {
        let messages: Vec<String> = [
            "Remove the photo of Adele",
            "Set the photo of Adele",
            "Set the photo of Can and 1 other change\n\n- Rate Tago Mago 9 of 10\n- Set the photo of Can",
            "Add a photo of Burial",
            "Set the photo of Burial",
            "Add photos of 50 artists",
        ]
        .map(str::to_owned)
        .into();
        let says = history_says(&messages);
        assert_eq!(says.get("adele"), Some(&false));
        assert_eq!(says.get("can"), Some(&true));
        assert_eq!(says.get("burial"), Some(&false));
        assert_eq!(says.len(), 3);

        let library = library(&[
            ("Adele", "21", false, "2026-01-01"),
            ("Burial", "Untrue", false, "2026-01-01"),
            ("Can", "Tago Mago", false, "2026-01-01"),
            ("Dido", "No Angel", false, "2026-01-01"),
            ("Eels", "Souljacker", false, "2026-01-01"),
            ("Faust", "Faust IV", false, "2026-01-01"),
        ]);
        let covers = photos(&[
            // Remembered as picked, the history's last word is a removal.
            ("adele", Some(PhotoKind::Picked)),
            // Nothing remembered, and the phone's fetch came last.
            ("burial", None),
            // Remembered as found, but the history says it was set.
            ("can", Some(PhotoKind::Fetched)),
            // Uploaded in a commit that named something else.
            ("dido", Some(PhotoKind::Uploaded)),
            ("eels", Some(PhotoKind::Fetched)),
        ]);
        let (picked, disagreed) = picked_from(&library, &covers, Some(&messages));
        let slugs: Vec<&str> = picked.iter().map(String::as_str).collect();
        assert_eq!(slugs, ["adele", "can", "dido"]);
        // Where the two disagree the artist counts as picked, and is named.
        assert_eq!(disagreed, ["Adele", "Can", "Dido"]);
        // Without a history, what this computer remembers.
        let (picked, disagreed) = picked_from(&library, &covers, None);
        let slugs: Vec<&str> = picked.iter().map(String::as_str).collect();
        assert_eq!(slugs, ["adele", "dido"]);
        assert!(disagreed.is_empty());
        assert_eq!(sharper_counts(&library, &covers), (3, 2));
    }

    #[test]
    fn the_sharper_photos_leave_the_picked_alone_and_go_on_where_they_stopped() {
        let library = library(&[
            ("Can", "Tago Mago", false, "2026-01-01"),
            ("Adele", "21", false, "2026-01-01"),
            ("Burial", "Untrue", false, "2026-01-01"),
            ("Dido", "No Angel", false, "2026-01-01"),
            ("Eels", "Souljacker", false, "2026-01-01"),
        ]);
        // Burial is on the record's list, Dido remembered as uploaded
        // here, and Eels has no photo.
        let mut covers = photos(&[
            ("adele", Some(PhotoKind::Fetched)),
            ("burial", None),
            ("can", None),
            ("dido", Some(PhotoKind::Uploaded)),
        ]);
        let pick = Change::Missing(MissingEdit::Pick {
            slug: "burial".into(),
        });
        let dir = TempDir::new("sharper");
        let mut state = State::at(Some(&dir.0));
        // Not started: nobody is tried.
        assert!(sharper_todo(&library, &covers, &state).is_empty());
        state.start_sharper();
        state.sharper_checked(vec![pick.clone()], 2, vec!["Dido".into()]);
        covers.rebuild(state.held());
        assert_eq!(
            names(&sharper_todo(&library, &covers, &state)),
            ["Adele", "Can"]
        );

        let sharper = Change::Sharpen {
            path: "artists/adele.jpg".into(),
            seen: vec![Some("a1".into())],
        };
        state.sharpened("adele", Some(sharper.clone()));
        // Stopped here: the next start goes on with whoever is left.
        let mut again = State::at(Some(&dir.0));
        assert_eq!(again.held(), [pick.clone(), sharper.clone()]);
        assert_eq!(again.disagreed, ["Dido"]);
        assert!(
            again
                .sharper
                .as_ref()
                .is_some_and(|s| s.checked && s.of == 2)
        );
        covers.rebuild(again.held());
        assert_eq!(names(&sharper_todo(&library, &covers, &again)), ["Can"]);
        // A photo picked meanwhile is left alone from then on.
        covers.set_kind("artists/can.jpg", PhotoKind::Picked);
        assert!(sharper_todo(&library, &covers, &again).is_empty());
        covers.set_kind("artists/can.jpg", PhotoKind::Fetched);
        // Passed over after a failure: not now, but in the next session.
        again.skip_sharper("can");
        assert!(sharper_todo(&library, &covers, &again).is_empty());
        again.sharper_done();
        assert!(again.sharper.is_some());
        let mut later = State::at(Some(&dir.0));
        assert_eq!(names(&sharper_todo(&library, &covers, &later)), ["Can"]);
        // Nothing sharper for Can: tried, and nothing joins the batch.
        later.sharpened("can", None);
        assert!(sharper_todo(&library, &covers, &later).is_empty());
        later.sharper_done();
        assert!(later.sharper.is_none());
        let (changes, summary) = later.take(None).unwrap();
        assert_eq!(changes, [pick, sharper]);
        assert_eq!(summary, "Replace the photos of 1 artist with sharper ones");
        // The names stay to be read until Check again.
        assert_eq!(State::at(Some(&dir.0)).disagreed, ["Dido"]);
    }

    #[test]
    fn the_row_offers_the_sharper_photos_once_everyone_is_asked() {
        let totals = Totals {
            artists: 5,
            with_photo: 5,
            ..Totals::default()
        };
        let sharp = Sharp {
            offered: true,
            found: 3,
            yours: 2,
            left: 0,
        };
        let mut state = State::default();
        // New artists first.
        assert!(matches!(
            Row::of(&state, totals, 1, true, sharp),
            Row::Fetching { .. }
        ));
        let row = Row::of(&state, totals, 0, true, sharp);
        assert_eq!(row, Row::Sharper { found: 3, yours: 2 });
        assert_eq!(row.button(), "Get sharper photos");
        assert_eq!(
            row.detail(),
            "3 photos can be fetched again at a sharper size (1200 × 1200). The 2 you picked \
             or uploaded are left alone."
        );
        // Not offered (the keys are not written yet, or the size is
        // there): the totals.
        let off = Sharp {
            offered: false,
            ..sharp
        };
        assert!(matches!(
            Row::of(&state, totals, 0, true, off),
            Row::Done(_)
        ));

        state.start_sharper();
        assert_eq!(Row::of(&state, totals, 0, true, sharp), Row::Checking);
        state.sharper_checked(Vec::new(), 3, Vec::new());
        state.sharpened("adele", None);
        let left = Sharp { left: 2, ..sharp };
        let row = Row::of(&state, totals, 0, true, left);
        assert_eq!(
            row,
            Row::Sharpening {
                done: 1,
                of: 3,
                left: 2
            }
        );
        assert_eq!(
            row.detail(),
            "Replacing photos with sharper ones · 1 of 3 · about a minute left. Yours are \
             left alone."
        );
        assert_eq!(row.button(), "Pause");
        state.paused = true;
        assert_eq!(
            Row::of(&state, totals, 0, true, left),
            Row::Paused { done: 1, of: 3 }
        );
        // The canvas's numbers.
        state.look = Some(Look::Sharper);
        assert_eq!(
            Row::of(&state, totals, 0, true, left).detail(),
            "197 photos can be fetched again at a sharper size (1200 × 1200). The 120 you \
             picked or uploaded are left alone."
        );
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
