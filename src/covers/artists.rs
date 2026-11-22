//! Artist photos: the rules, with nothing fetched or stored here.
//!
//! An artist has one fixed file in the library repo,
//! `covers/artists/<slug>.jpg`, whoever put it there: fetched from the
//! artist's Apple Music page, picked in Pick a cover or uploaded. With no
//! file an artist shows a cover from the library, else their initials.
//! `covers/artists/_missing.json` records "tried, nothing found", so no
//! device asks Apple about the same artist again for ninety days.
//!
//! The phone follows the same rules, so every rule here is a pure
//! function and its cases are in `assets/fixtures/artist_photos.json`,
//! which both apps run (DESIGN.md "Artist photos").

use std::collections::{BTreeMap, BTreeSet};

use crate::model::Date;
use crate::model::text::slugify;

/// The folder under `covers/`.
pub const FOLDER: &str = "artists";
/// The record of artists tried with nothing found, in that folder.
pub const MISSING_FILE: &str = "_missing.json";
/// The last segment of a photo's URL as it is read from Apple's page and
/// kept: the 640 × 640 square JPEG. `photo_at` makes it the size that is
/// fetched.
pub const PHOTO_SIZE: &str = "640x640cc.jpg";
/// The side of an artist's photo, in pixels, until the record says
/// otherwise (`Missing::photo_size`).
pub const PHOTO_SIDE: u32 = 640;
/// The side once the existing photos were fetched again: what the first
/// batch of that run writes as `photo_size`, and the most a picked or
/// uploaded photo keeps.
pub const SHARP_SIDE: u32 = 1200;
/// Whether `photo_size` and `picked` are written to `_missing.json`. On
/// since 2026-11-22, when the phone build that keeps the keys it does
/// not know was installed: an older build rewrites the file from the
/// keys it knows and would erase them. With it off nothing adds to
/// `picked`, and the sharper photos of Settings are not offered, since
/// their first batch writes `photo_size`. Both keys are read and kept
/// either way.
pub const WRITES_PHOTO_KEYS: bool = true;
/// How long "nothing found" stands before the artist is tried again.
pub const RETRY_DAYS: i64 = 90;
/// The `_missing.json` format.
pub const MISSING_VERSION: i64 = 1;

/// A name as names and album titles are compared: case, accents and
/// punctuation aside, "&" as "and", a leading "The" ignored. It is the
/// slug of the name with " and " for each "&", without a leading "the-"
/// (kept when nothing else is left: "The The" is "the"). Empty for a
/// name with no letters or digits, which matches nothing.
pub fn name_key(name: &str) -> String {
    let key = slugify(&name.replace('&', " and "));
    match key.strip_prefix("the-") {
        Some(rest) if !rest.is_empty() => rest.to_owned(),
        _ => key,
    }
}

/// Whether two artist names are the same by `name_key`.
pub fn same_name(a: &str, b: &str) -> bool {
    let key = name_key(a);
    !key.is_empty() && key == name_key(b)
}

/// A title without the notes at its end: a trailing " - Single" or
/// " - EP" (whatever the case) and a trailing part in round or square
/// brackets, from its last opening bracket, taken off one after another
/// until none is left. A title that would end up empty keeps what it
/// had: "(What's the Story) Morning Glory?" and "(I)" stay whole.
pub fn title_core(title: &str) -> &str {
    let mut core = title.trim();
    loop {
        let shorter = without_suffix(core, " - single")
            .or_else(|| without_suffix(core, " - ep"))
            .or_else(|| without_bracket(core, '(', ')'))
            .or_else(|| without_bracket(core, '[', ']'))
            .map(str::trim_end);
        match shorter {
            Some(shorter) if !shorter.is_empty() => core = shorter,
            _ => return core,
        }
    }
}

fn without_suffix<'a>(title: &'a str, suffix: &str) -> Option<&'a str> {
    let at = title.len().checked_sub(suffix.len())?;
    (title.is_char_boundary(at) && title[at..].eq_ignore_ascii_case(suffix)).then(|| &title[..at])
}

fn without_bracket(title: &str, open: char, close: char) -> Option<&str> {
    title
        .ends_with(close)
        .then(|| title.rfind(open))
        .flatten()
        .map(|at| &title[..at])
}

/// The keys a title is compared by: the title's own and its core's
/// (`title_core`), the empty ones left out.
fn title_keys(title: &str) -> Vec<String> {
    let mut keys = vec![name_key(title), name_key(title_core(title))];
    keys.dedup();
    keys.retain(|key| !key.is_empty());
    keys
}

/// Whether two album titles, or two song titles, are the same: by
/// `name_key`, each as it is written or without the notes at its end, so
/// "Texas Moon" is "Texas Moon - EP", "Abbey Road" is "Abbey Road
/// (Remastered)" and "Another Green World (2004 Remaster)" is "Another
/// Green World". Words outside the brackets still have to agree.
pub fn same_title(a: &str, b: &str) -> bool {
    let b = title_keys(b);
    title_keys(a).iter().any(|key| b.contains(key))
}

/// The artist's slug, the entry ids' slug of the name; `None` for a name
/// with no letters or digits, which never gets a file.
pub fn slug(name: &str) -> Option<String> {
    let slug = slugify(name);
    (!slug.is_empty()).then_some(slug)
}

/// The artist's file under `covers/`: "artists/massive-attack.jpg".
pub fn file_path(name: &str) -> Option<String> {
    slug(name).map(|slug| format!("{FOLDER}/{slug}.jpg"))
}

/// `_missing.json` under `covers/`.
pub fn missing_path() -> String {
    format!("{FOLDER}/{MISSING_FILE}")
}

/// Whether a path under `covers/` is in the artists folder, which the
/// Unused covers check leaves alone.
pub fn is_artist_path(path: &str) -> bool {
    path.strip_prefix(FOLDER)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The names no photo is ever looked for: "Various Artists" and "Unknown
/// artist", compared like any name.
pub fn is_skipped(name: &str) -> bool {
    matches!(
        name_key(name).as_str(),
        "various-artists" | "unknown-artist"
    )
}

/// The `og:image` of an artist's `music.apple.com` page: the `content`
/// of the first `<meta>` whose `property` is exactly "og:image".
pub fn og_image(html: &str) -> Option<String> {
    let mut rest = html;
    while let Some(at) = rest.find("<meta") {
        let tag = &rest[at..];
        let end = tag.find('>')?;
        let (tag, after) = tag.split_at(end);
        rest = after;
        if attribute(tag, "property") == Some("og:image") {
            return attribute(tag, "content")
                .filter(|url| !url.is_empty())
                .map(|url| url.replace("&amp;", "&"));
        }
    }
    None
}

/// The value of `name="…"` in a tag.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before = rest[..at].chars().next_back();
        let value = rest[at + name.len()..].strip_prefix("=\"");
        rest = &rest[at + name.len()..];
        if before.is_some_and(char::is_whitespace)
            && let Some(value) = value
        {
            return value.split_once('"').map(|(value, _)| value);
        }
    }
}

/// What an `og:image` shows, told by the bucket in its URL (the segment
/// after `/image/thumb/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// `AMCArtistImages…` or `Features…`: a photo of the artist.
    Photo,
    /// `Music…`: an album cover Apple puts there for an artist without a
    /// photo.
    Album,
    /// Any other bucket, which counts as no photo.
    Unknown,
}

pub fn art_of(og_image: &str) -> Art {
    let bucket = og_image
        .split_once("/image/thumb/")
        .map_or("", |(_, rest)| rest.split('/').next().unwrap_or_default());
    if bucket.starts_with("AMCArtistImages") || bucket.starts_with("Features") {
        Art::Photo
    } else if bucket.starts_with("Music") {
        Art::Album
    } else {
        Art::Unknown
    }
}

/// The photo to fetch for an `og:image`: the same image as a 640 × 640
/// square JPEG (the share card's last segment, `1200x630cw.png`, replaced
/// by `640x640cc.jpg`); `None` unless the image is a photo of the artist.
pub fn photo_url(og_image: &str) -> Option<String> {
    if art_of(og_image) != Art::Photo {
        return None;
    }
    let (image, size) = og_image.rsplit_once('/')?;
    (!size.is_empty()).then(|| format!("{image}/{PHOTO_SIZE}"))
}

/// How many of an artist's saved songs Apple is asked about, at most.
pub const SONGS_ASKED: usize = 3;

/// An Apple artist a name search found, with the titles of their albums
/// and, when their songs were searched for (`choose_artist`), the names
/// of the tracks those searches found under this artist's id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub albums: Vec<String>,
    pub songs: Vec<String>,
}

/// How many of `saved` (titles saved for the artist) are among `listed`
/// by `same_title`. Saved titles that differ only in the notes at their
/// end ("Birthdays" and "Birthdays (Deluxe)") count once.
fn listed_count(listed: &[String], saved: &[&str]) -> usize {
    let listed: Vec<String> = listed.iter().flat_map(|title| title_keys(title)).collect();
    let mut counted: Vec<String> = Vec::new();
    for title in saved {
        let keys = title_keys(title);
        // The core's key comes last and names the title.
        let Some(named) = keys.last() else {
            continue;
        };
        if !counted.contains(named) && keys.iter().any(|key| listed.contains(key)) {
            counted.push(named.clone());
        }
    }
    counted.len()
}

/// How many of `owned` (the album titles saved for the artist) the
/// candidate lists, each owned title counted once.
pub fn owned_count(candidate: &Candidate, owned: &[&str]) -> usize {
    listed_count(&candidate.albums, owned)
}

/// The one with the highest count above zero among the candidates named
/// `name`; none when nobody counts or two tie for the most.
fn most<'a>(
    name: &str,
    candidates: &'a [Candidate],
    count: impl Fn(&Candidate) -> usize,
) -> Option<&'a Candidate> {
    let mut counted: Vec<(usize, &Candidate)> = candidates
        .iter()
        .filter(|candidate| same_name(&candidate.name, name))
        .map(|candidate| (count(candidate), candidate))
        .filter(|(count, _)| *count > 0)
        .collect();
    counted.sort_by_key(|counted| std::cmp::Reverse(counted.0));
    match counted.as_slice() {
        [(_, only)] => Some(only),
        [(best, first), (next, _), ..] if best > next => Some(first),
        _ => None,
    }
}

/// The Apple artist that is the library's `name`, by albums: among the
/// candidates of the same name, the one listing the most owned albums.
/// One that lists none is no match, even as the only namesake, and a tie
/// for the most is no match either.
pub fn choose<'a>(
    name: &str,
    owned: &[&str],
    candidates: &'a [Candidate],
) -> Option<&'a Candidate> {
    most(name, candidates, |candidate| owned_count(candidate, owned))
}

/// Whether some candidate of the name lists an owned album. When none
/// does, the artist's saved songs are searched for instead.
pub fn lists_owned(name: &str, owned: &[&str], candidates: &[Candidate]) -> bool {
    candidates
        .iter()
        .any(|candidate| same_name(&candidate.name, name) && owned_count(candidate, owned) > 0)
}

/// The saved songs Apple is asked about: the first `SONGS_ASKED` of
/// `songs` (the artist's saved song titles in the library's order), a
/// title counted once by `same_title` and one with no key left out.
pub fn songs_asked<'a>(songs: &[&'a str]) -> Vec<&'a str> {
    let mut asked: Vec<&str> = Vec::new();
    for &song in songs {
        if asked.len() == SONGS_ASKED {
            break;
        }
        if !name_key(song).is_empty() && !asked.iter().any(|&other| same_title(other, song)) {
            asked.push(song);
        }
    }
    asked
}

/// The Apple artist that is the library's `name`, by songs: among the
/// candidates of the same name, the one under whose id the searches
/// found the most of the songs asked about (`songs_asked`); none found
/// is no match, and so is a tie for the most.
pub fn choose_by_songs<'a>(
    name: &str,
    songs: &[&str],
    candidates: &'a [Candidate],
) -> Option<&'a Candidate> {
    let asked = songs_asked(songs);
    most(name, candidates, |candidate| {
        listed_count(&candidate.songs, &asked)
    })
}

/// The Apple artist that is the library's `name`: by albums (`choose`);
/// by songs (`choose_by_songs`) only when no candidate of the name lists
/// an owned album, so a tie between albums is never broken by a song.
pub fn choose_artist<'a>(
    name: &str,
    owned: &[&str],
    songs: &[&str],
    candidates: &'a [Candidate],
) -> Option<&'a Candidate> {
    if lists_owned(name, owned, candidates) {
        choose(name, owned, candidates)
    } else {
        choose_by_songs(name, songs, candidates)
    }
}

/// The photo at `url` (`photo_url`) as a `side` × `side` JPEG. Any other
/// URL is left as it is.
pub fn photo_at(url: &str, side: u32) -> String {
    match url.strip_suffix(PHOTO_SIZE) {
        Some(image) if side != PHOTO_SIDE && image.ends_with('/') => {
            format!("{image}{side}x{side}cc.jpg")
        }
        _ => url.to_owned(),
    }
}

/// Why an artist has no photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// No Apple artist of the name lists an owned album, or two tie.
    NoMatch,
    /// The matched artist's page shows no photo of the artist.
    NoPhoto,
}

impl Reason {
    pub fn key(self) -> &'static str {
        match self {
            Reason::NoMatch => "no-match",
            Reason::NoPhoto => "no-photo",
        }
    }

    fn parse(key: &str) -> Option<Reason> {
        match key {
            "no-match" => Some(Reason::NoMatch),
            "no-photo" => Some(Reason::NoPhoto),
            _ => None,
        }
    }
}

/// One artist of `_missing.json`: the name as the library spells it,
/// the day Apple was asked and what came of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tried {
    pub name: String,
    pub tried: Date,
    pub reason: Reason,
}

impl Tried {
    /// Whether the artist is asked about again on `today`: ninety days
    /// after the try, or sooner once an entry by the artist was added on
    /// a later day than the try (`added` is the latest `added` among the
    /// artist's entries). The device that adds an entry tries at once,
    /// whatever the day.
    pub fn is_due(&self, today: Date, added: Option<Date>) -> bool {
        self.tried.days_until(today) >= RETRY_DAYS || added.is_some_and(|added| added > self.tried)
    }
}

/// A top-level key of `_missing.json` this build does not know, with its
/// value's text exactly as the file had it and the known key it followed
/// (none at the start of the file).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Extra {
    after: Option<&'static str>,
    key: String,
    text: String,
}

/// The top-level keys this build reads and writes, in the order it writes
/// them.
const KNOWN: [&str; 5] = ["version", "backfilled", "photo_size", "picked", "artists"];

/// `_missing.json`: the artists tried with nothing found, by slug, the
/// day the desktop's backfill finished, the size photos are fetched at
/// and the artists whose photo the owner chose.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Missing {
    /// Written by the backfill's last batch; the phone fetches nothing
    /// until it is there.
    pub backfilled: Option<Date>,
    /// The side new photos are fetched at, written by the first batch of
    /// the sharper photos; `PHOTO_SIDE` while it is not there.
    pub photo_size: Option<u32>,
    /// The slugs of the artists whose photo the owner picked or
    /// uploaded: never replaced by a fetch.
    pub picked: BTreeSet<String>,
    pub artists: BTreeMap<String, Tried>,
    /// What this build does not know, written back as it was read.
    extra: Vec<Extra>,
}

/// The top-level keys of the JSON object `text` (valid JSON), each with
/// the text of its value as written. A key written twice keeps its first
/// place and its last value, as a JSON reader would have it.
fn top_level(text: &str) -> Vec<(String, &str)> {
    let bytes = text.as_bytes();
    let past = |mut at: usize| {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        at
    };
    // `at` is a string's opening quote; the index after its closing one.
    let after_string = |mut at: usize| {
        at += 1;
        while at < bytes.len() && bytes[at] != b'"' {
            if bytes[at] == b'\\' {
                at += 1;
            }
            at += 1;
        }
        (at + 1).min(bytes.len())
    };
    let mut out: Vec<(String, &str)> = Vec::new();
    let mut at = text.find('{').map_or(bytes.len(), |open| open + 1);
    loop {
        at = past(at);
        if bytes.get(at) != Some(&b'"') {
            break;
        }
        let key_end = after_string(at);
        let Ok(key) = serde_json::from_str::<String>(&text[at..key_end]) else {
            break;
        };
        // Past the colon.
        at = past(past(key_end) + 1);
        let start = at;
        let mut depth = 0usize;
        while at < bytes.len() {
            match bytes[at] {
                b'"' => {
                    at = after_string(at);
                    continue;
                }
                b'{' | b'[' => depth += 1,
                b'}' | b']' if depth == 0 => break,
                b'}' | b']' => depth -= 1,
                b',' if depth == 0 => break,
                _ => {}
            }
            at += 1;
        }
        let value = text[start..at.min(bytes.len())].trim_end();
        match out.iter_mut().find(|(known, _)| *known == key) {
            Some(slot) => slot.1 = value,
            None => out.push((key, value)),
        }
        if bytes.get(at) == Some(&b',') {
            at += 1;
        }
    }
    out
}

impl Missing {
    /// Reads the file. Anything but version 1 in the shape below is an
    /// error, and such a file is left as it is. A top-level key this
    /// build does not know is kept with its value as written (`extra`),
    /// and so is a `photo_size` that is no positive number or a `picked`
    /// that is no list of slugs.
    pub fn decode(text: &str) -> Result<Missing, String> {
        let json: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if json["version"].as_i64() != Some(MISSING_VERSION) {
            return Err(format!("not version {MISSING_VERSION}"));
        }
        let date = |value: &serde_json::Value, what: &str| {
            value
                .as_str()
                .and_then(Date::parse)
                .ok_or_else(|| format!("{what} is not a date"))
        };
        let backfilled = match json.get("backfilled") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(date(value, "backfilled")?),
        };
        let photo_size = json
            .get("photo_size")
            .and_then(serde_json::Value::as_u64)
            .and_then(|side| u32::try_from(side).ok())
            .filter(|side| *side > 0);
        let picked: Option<BTreeSet<String>> = json.get("picked").and_then(|list| {
            list.as_array()?
                .iter()
                .map(|slug| slug.as_str().map(str::to_owned))
                .collect()
        });
        let mut artists = BTreeMap::new();
        let listed = json["artists"].as_object().ok_or("no artists")?;
        for (slug, entry) in listed {
            let name = entry["name"]
                .as_str()
                .ok_or_else(|| format!("{slug} has no name"))?;
            let reason = entry["reason"]
                .as_str()
                .and_then(Reason::parse)
                .ok_or_else(|| format!("{slug} has no known reason"))?;
            artists.insert(
                slug.clone(),
                Tried {
                    name: name.to_owned(),
                    tried: date(&entry["tried"], "tried")?,
                    reason,
                },
            );
        }
        let mut extra = Vec::new();
        let mut after = None;
        for (key, value) in top_level(text) {
            let known = KNOWN.into_iter().find(|known| *known == key);
            let understood = match known {
                Some("photo_size") => photo_size.is_some(),
                Some("picked") => picked.is_some(),
                Some(_) => true,
                None => false,
            };
            if !understood {
                extra.push(Extra {
                    after,
                    key,
                    text: value.to_owned(),
                });
            }
            if known.is_some() {
                after = known;
            }
        }
        Ok(Missing {
            backfilled,
            photo_size,
            picked: picked.unwrap_or_default(),
            artists,
            extra,
        })
    }

    /// The file's bytes: two-space indent, `version`, then `backfilled`
    /// when set, `photo_size` when set, `picked` while it has a slug (one
    /// per line, in order), then `artists` with one artist per line in
    /// slug order (by code unit; a slug is ASCII), each with `name`,
    /// `tried` and `reason` in that order, and a newline at the end.
    /// Every key this build does not know keeps its value's text and its
    /// place after the known key it followed.
    pub fn encode(&self) -> String {
        let quoted = |text: &str| serde_json::Value::from(text).to_string();
        let mut parts: Vec<String> = Vec::new();
        let extras = |parts: &mut Vec<String>, after: Option<&str>| {
            for extra in self.extra.iter().filter(|extra| extra.after == after) {
                parts.push(format!("  {}: {}", quoted(&extra.key), extra.text));
            }
        };
        extras(&mut parts, None);
        for key in KNOWN {
            match key {
                "version" => parts.push(format!("  \"version\": {MISSING_VERSION}")),
                "backfilled" => {
                    if let Some(day) = self.backfilled {
                        parts.push(format!("  \"backfilled\": \"{day}\""));
                    }
                }
                "photo_size" => {
                    if let Some(side) = self.photo_size {
                        parts.push(format!("  \"photo_size\": {side}"));
                    }
                }
                "picked" if !self.picked.is_empty() => {
                    let slugs: Vec<String> = self
                        .picked
                        .iter()
                        .map(|slug| format!("    {}", quoted(slug)))
                        .collect();
                    parts.push(format!("  \"picked\": [\n{}\n  ]", slugs.join(",\n")));
                }
                "artists" if self.artists.is_empty() => parts.push("  \"artists\": {}".into()),
                "artists" => {
                    let lines: Vec<String> = self
                        .artists
                        .iter()
                        .map(|(slug, tried)| {
                            format!(
                                "    {}: {{\"name\": {}, \"tried\": \"{}\", \"reason\": \"{}\"}}",
                                quoted(slug),
                                quoted(&tried.name),
                                tried.tried,
                                tried.reason.key()
                            )
                        })
                        .collect();
                    parts.push(format!("  \"artists\": {{\n{}\n  }}", lines.join(",\n")));
                }
                _ => {}
            }
            extras(&mut parts, Some(key));
        }
        format!("{{\n{}\n}}\n", parts.join(",\n"))
    }

    /// Whether Apple is asked about the artist with `slug` on `today`:
    /// always without an entry here, else by `Tried::is_due`.
    pub fn is_due(&self, slug: &str, today: Date, added: Option<Date>) -> bool {
        self.artists
            .get(slug)
            .is_none_or(|tried| tried.is_due(today, added))
    }
}

/// One change to `_missing.json`, queued until the commit that carries
/// it: the file is never sent whole, since another device may have added
/// to it meanwhile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingEdit {
    /// Apple was asked about the artist and nothing was found.
    Set { slug: String, tried: Tried },
    /// The artist has a file now, or is asked about again.
    Remove { slug: String },
    /// The backfill finished on this day.
    Backfilled(Date),
    /// The owner picked or uploaded the artist's photo.
    Pick { slug: String },
    /// Use automatic, or the suggested photo picked again: the photo is
    /// no longer the owner's own choice.
    Unpick { slug: String },
    /// New photos are fetched at this side from now on.
    PhotoSize(u32),
}

impl MissingEdit {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            MissingEdit::Set { slug, tried } => serde_json::json!({
                "set": slug,
                "name": tried.name,
                "tried": tried.tried.to_string(),
                "reason": tried.reason.key(),
            }),
            MissingEdit::Remove { slug } => serde_json::json!({"remove": slug}),
            MissingEdit::Backfilled(day) => serde_json::json!({"backfilled": day.to_string()}),
            MissingEdit::Pick { slug } => serde_json::json!({"pick": slug}),
            MissingEdit::Unpick { slug } => serde_json::json!({"unpick": slug}),
            MissingEdit::PhotoSize(side) => serde_json::json!({"photo_size": side}),
        }
    }

    pub fn from_json(json: &serde_json::Value) -> Option<MissingEdit> {
        if let Some(slug) = json["set"].as_str() {
            return Some(MissingEdit::Set {
                slug: slug.to_owned(),
                tried: Tried {
                    name: json["name"].as_str()?.to_owned(),
                    tried: json["tried"].as_str().and_then(Date::parse)?,
                    reason: json["reason"].as_str().and_then(Reason::parse)?,
                },
            });
        }
        if let Some(slug) = json["remove"].as_str() {
            return Some(MissingEdit::Remove {
                slug: slug.to_owned(),
            });
        }
        for (key, edit) in [("pick", true), ("unpick", false)] {
            if let Some(slug) = json[key].as_str() {
                let slug = slug.to_owned();
                return Some(match edit {
                    true => MissingEdit::Pick { slug },
                    false => MissingEdit::Unpick { slug },
                });
            }
        }
        if let Some(side) = json["photo_size"].as_u64() {
            return u32::try_from(side).ok().map(MissingEdit::PhotoSize);
        }
        json["backfilled"]
            .as_str()
            .and_then(Date::parse)
            .map(MissingEdit::Backfilled)
    }
}

impl Missing {
    pub fn apply(&mut self, edit: &MissingEdit) {
        match edit {
            MissingEdit::Set { slug, tried } => {
                self.artists.insert(slug.clone(), tried.clone());
            }
            MissingEdit::Remove { slug } => {
                self.artists.remove(slug);
            }
            // The first day stands: the backfill is done once.
            MissingEdit::Backfilled(day) => {
                self.backfilled.get_or_insert(*day);
            }
            MissingEdit::Pick { slug } => {
                self.picked.insert(slug.clone());
            }
            MissingEdit::Unpick { slug } => {
                self.picked.remove(slug);
            }
            // The first size stands, like the day.
            MissingEdit::PhotoSize(side) => {
                self.photo_size.get_or_insert(*side);
            }
        }
    }

    /// The record after `edits` on top of this one, in their order, and
    /// without the artists that have a file (`has_file`, by slug) or are
    /// no longer in the library (`in_library`, by slug): an entry leaves
    /// when a file for the artist appears, whoever put it there, or when
    /// no entry names the artist any more, and for no other reason. The
    /// other way round for `picked`: a slug stays only while the artist
    /// has a file and is in the library.
    pub fn merged(
        mut self,
        edits: &[MissingEdit],
        has_file: impl Fn(&str) -> bool,
        in_library: impl Fn(&str) -> bool,
    ) -> Missing {
        for edit in edits {
            self.apply(edit);
        }
        self.artists
            .retain(|slug, _| !has_file(slug) && in_library(slug));
        self.picked
            .retain(|slug| has_file(slug) && in_library(slug));
        self
    }

    /// The side a new photo is fetched at.
    pub fn photo_side(&self) -> u32 {
        self.photo_size.unwrap_or(PHOTO_SIDE)
    }
}

/// `_missing.json` as a commit writes it: the repo's copy (`theirs`, none
/// when there is no file yet) with this device's `edits` on top, per
/// artist, without the artists that have a file or have left the library
/// (`Missing::merged`). `Ok(None)` when that changes nothing, and no file
/// is made for an empty record. A copy that cannot be read is an error
/// and is left as it is; the edits are then dropped.
pub fn merged_missing(
    theirs: Option<&str>,
    edits: &[MissingEdit],
    has_file: impl Fn(&str) -> bool,
    in_library: impl Fn(&str) -> bool,
) -> Result<Option<String>, String> {
    let base = match theirs {
        Some(text) => Missing::decode(text)?,
        None => Missing::default(),
    };
    let merged = base.merged(edits, has_file, in_library);
    if theirs.is_none() && merged == Missing::default() {
        return Ok(None);
    }
    let text = merged.encode();
    Ok((theirs != Some(text.as_str())).then_some(text))
}

/// The record's `picked` list once `edits` are merged into the repo's
/// copy (`theirs`): the artists whose photo a sharper copy must leave
/// alone. Slugs without a file are still on it here, which only errs on
/// the side of leaving a photo alone. An error for a copy that cannot be
/// read.
pub fn picked_after(
    theirs: Option<&str>,
    edits: &[MissingEdit],
) -> Result<BTreeSet<String>, String> {
    let mut record = match theirs {
        Some(text) => Missing::decode(text)?,
        None => Missing::default(),
    };
    for edit in edits {
        record.apply(edit);
    }
    Ok(record.picked)
}

/// The slug of an artist's file at `path` under `covers/`:
/// "artists/adele.jpg" is "adele".
pub fn slug_of_path(path: &str) -> Option<&str> {
    path.strip_prefix(FOLDER)?
        .strip_prefix('/')?
        .strip_suffix(".jpg")
        .filter(|slug| !slug.is_empty() && !slug.contains('/'))
}

/// One of an artist's entries, as the library cover is picked from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saved {
    pub song: bool,
    pub added: Date,
}

/// The order in which an artist's entries lend their cover when the
/// artist has no file, as indexes into `saved` (the artist's entries in
/// the file's order): the albums, most recently added first, then the
/// songs (a song shows its album's cover) the same way; of two added on
/// the same day the one later in the file comes first. The artist shows
/// the first of them that has a cover, else their initials.
pub fn cover_order(saved: &[Saved]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..saved.len()).collect();
    order.sort_by(|&a, &b| {
        saved[a]
            .song
            .cmp(&saved[b].song)
            .then(saved[b].added.cmp(&saved[a].added))
            .then(b.cmp(&a))
    });
    order
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    /// The cases both apps run.
    const CASES: &str = include_str!("../../assets/fixtures/artist_photos.json");

    fn cases(section: &str) -> Vec<Value> {
        let json: Value = serde_json::from_str(CASES).expect("the fixture is JSON");
        let cases = json[section].as_array().cloned().unwrap_or_default();
        assert!(!cases.is_empty(), "no cases under {section}");
        cases
    }

    fn text<'a>(case: &'a Value, key: &str) -> &'a str {
        case[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} missing in {case}"))
    }

    fn day(case: &Value, key: &str) -> Date {
        Date::parse(text(case, key)).expect("a date")
    }

    fn strings<'a>(case: &'a Value, key: &str) -> Vec<&'a str> {
        case[key]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    #[test]
    fn names_are_compared_by_their_key() {
        for case in cases("name_key") {
            assert_eq!(name_key(text(&case, "name")), text(&case, "key"), "{case}");
        }
        for case in cases("same_name") {
            assert_eq!(
                same_name(text(&case, "a"), text(&case, "b")),
                case["same"].as_bool().unwrap(),
                "{case}"
            );
        }
    }

    #[test]
    fn titles_are_compared_with_and_without_the_notes_at_their_end() {
        for case in cases("title_core") {
            assert_eq!(
                title_core(text(&case, "title")),
                text(&case, "core"),
                "{case}"
            );
        }
        for case in cases("same_title") {
            let (a, b) = (text(&case, "a"), text(&case, "b"));
            let same = case["same"].as_bool().unwrap();
            assert_eq!(same_title(a, b), same, "{case}");
            assert_eq!(same_title(b, a), same, "{case}");
        }
    }

    #[test]
    fn at_most_three_saved_songs_are_asked_about() {
        for case in cases("songs_asked") {
            assert_eq!(
                songs_asked(&strings(&case, "songs")),
                strings(&case, "asked"),
                "{case}"
            );
        }
    }

    #[test]
    fn an_artist_has_one_file_unless_the_name_has_no_letters_or_digits() {
        for case in cases("file_path") {
            assert_eq!(
                file_path(text(&case, "name")).as_deref(),
                case["path"].as_str(),
                "{case}"
            );
        }
        assert!(is_artist_path("artists/adele.jpg"));
        assert!(is_artist_path("artists/_missing.json"));
        assert!(!is_artist_path("music/artists.jpg"));
        assert!(!is_artist_path("artists-old/adele.jpg"));
    }

    #[test]
    fn two_names_are_never_looked_for() {
        for case in cases("skipped") {
            assert_eq!(
                is_skipped(text(&case, "name")),
                case["skipped"].as_bool().unwrap(),
                "{case}"
            );
        }
    }

    #[test]
    fn the_page_gives_its_share_image() {
        for case in cases("og_image") {
            assert_eq!(
                og_image(text(&case, "html")).as_deref(),
                case["url"].as_str(),
                "{case}"
            );
        }
    }

    #[test]
    fn the_bucket_tells_a_photo_from_album_art() {
        for case in cases("photo_url") {
            let art = match art_of(text(&case, "og_image")) {
                Art::Photo => "photo",
                Art::Album => "album",
                Art::Unknown => "unknown",
            };
            assert_eq!(art, text(&case, "art"), "{case}");
            assert_eq!(
                photo_url(text(&case, "og_image")).as_deref(),
                case["photo"].as_str(),
                "{case}"
            );
        }
    }

    #[test]
    fn the_namesake_with_the_most_owned_albums_is_the_artist() {
        for case in cases("choose") {
            let candidates: Vec<Candidate> = case["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    let owned = |key: &str| strings(c, key).into_iter().map(str::to_owned);
                    Candidate {
                        id: text(c, "id").to_owned(),
                        name: text(c, "name").to_owned(),
                        albums: owned("albums").collect(),
                        songs: owned("songs").collect(),
                    }
                })
                .collect();
            // `songs` are the artist's saved songs, and a candidate's
            // `songs` what the searches for them found under its id.
            let chosen = choose_artist(
                text(&case, "name"),
                &strings(&case, "owned"),
                &strings(&case, "songs"),
                &candidates,
            );
            assert_eq!(
                chosen.map(|c| c.id.as_str()),
                case["chosen"].as_str(),
                "{case}"
            );
        }
    }

    #[test]
    fn the_missing_file_is_written_in_one_form() {
        for case in cases("missing") {
            let written = text(&case, "text");
            let missing = Missing::decode(written).unwrap_or_else(|e| panic!("{e}: {case}"));
            assert_eq!(missing.encode(), written, "{case}");
            assert_eq!(
                missing.backfilled.map(|d| d.to_string()).as_deref(),
                case["backfilled"].as_str(),
                "{case}"
            );
            let slugs: Vec<&str> = missing.artists.keys().map(String::as_str).collect();
            assert_eq!(slugs, strings(&case, "slugs"), "{case}");
        }
        // Another order and other spacing read the same and are written
        // in the one form.
        for case in cases("missing_rewritten") {
            let missing = Missing::decode(text(&case, "read")).expect("readable");
            assert_eq!(missing.encode(), text(&case, "written"), "{case}");
        }
        for case in cases("missing_refused") {
            assert!(Missing::decode(text(&case, "text")).is_err(), "{case}");
        }
    }

    #[test]
    fn the_size_and_the_picked_list_have_one_form() {
        for case in cases("picked") {
            let written = text(&case, "text");
            let missing = Missing::decode(written).unwrap_or_else(|e| panic!("{e}: {case}"));
            assert_eq!(missing.encode(), written, "{case}");
            assert_eq!(
                missing.photo_size.map(u64::from),
                case["photo_size"].as_u64(),
                "{case}"
            );
            let picked: Vec<&str> = missing.picked.iter().map(String::as_str).collect();
            assert_eq!(picked, strings(&case, "picked"), "{case}");
            assert_eq!(
                missing.photo_side(),
                case["photo_size"].as_u64().map_or(PHOTO_SIDE, |s| s as u32)
            );
        }
        for case in cases("picked_rewritten") {
            let missing = Missing::decode(text(&case, "read")).expect("readable");
            assert_eq!(missing.encode(), text(&case, "written"), "{case}");
        }
        // A size or a list this build cannot read is kept as it is, like
        // a key it does not know.
        let odd = "{\n  \"version\": 1,\n  \"photo_size\": \"large\",\n  \"picked\": {\"a\": 1},\n  \"artists\": {}\n}\n";
        let missing = Missing::decode(odd).expect("readable");
        assert_eq!((missing.photo_size, missing.picked.len()), (None, 0));
        assert_eq!(missing.encode(), odd);
    }

    #[test]
    fn a_photo_is_fetched_at_the_side_asked_for() {
        for case in cases("photo_at") {
            let side = case["side"].as_u64().unwrap() as u32;
            assert_eq!(
                photo_at(text(&case, "url"), side),
                text(&case, "at"),
                "{case}"
            );
        }
    }

    #[test]
    fn the_missing_file_is_merged_per_artist() {
        for case in cases("missing_merge")
            .into_iter()
            .chain(cases("picked_merge"))
        {
            let edits: Vec<MissingEdit> = case["edits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|edit| MissingEdit::from_json(edit).unwrap_or_else(|| panic!("{edit}")))
                .collect();
            // The edits are queued in this form.
            for (edit, json) in edits.iter().zip(case["edits"].as_array().unwrap()) {
                assert_eq!(&edit.to_json(), json, "{case}");
            }
            let files = strings(&case, "files");
            // `library` lists the slugs of the library's artists; a case
            // without it has everyone in the library.
            let library = case["library"]
                .is_array()
                .then(|| strings(&case, "library"));
            let merged = merged_missing(
                case["theirs"].as_str(),
                &edits,
                |slug| files.contains(&slug),
                |slug| library.as_ref().is_none_or(|all| all.contains(&slug)),
            );
            match case["refused"].as_bool() {
                Some(true) => assert!(merged.is_err(), "{case}"),
                _ => assert_eq!(
                    merged.expect("readable").as_deref(),
                    case["written"].as_str(),
                    "{case}"
                ),
            }
        }
        assert_eq!(slug_of_path("artists/adele.jpg"), Some("adele"));
        assert_eq!(slug_of_path("artists/_missing.json"), None);
        assert_eq!(slug_of_path("music/adele.jpg"), None);
    }

    #[test]
    fn nothing_found_stands_for_ninety_days_or_until_something_is_added() {
        for case in cases("due") {
            let tried = Tried {
                name: "Adele".into(),
                tried: day(&case, "tried"),
                reason: Reason::NoMatch,
            };
            let added = case["added"].as_str().and_then(Date::parse);
            assert_eq!(
                tried.is_due(day(&case, "today"), added),
                case["due"].as_bool().unwrap(),
                "{case}"
            );
        }
        let none = Missing::default();
        assert!(none.is_due("adele", Date::parse("2026-11-15").unwrap(), None));
    }

    #[test]
    fn the_library_cover_is_the_latest_album_then_the_latest_song() {
        for case in cases("cover_order") {
            let saved: Vec<Saved> = case["saved"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| Saved {
                    song: text(entry, "kind") == "song",
                    added: day(entry, "added"),
                })
                .collect();
            let order: Vec<u64> = cover_order(&saved).into_iter().map(|i| i as u64).collect();
            let expected: Vec<u64> = case["order"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_u64)
                .collect();
            assert_eq!(order, expected, "{case}");
        }
    }
}
