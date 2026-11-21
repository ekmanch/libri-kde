//! The owner's own covers, kept as files in the library repo.
//!
//! They live in `covers/<type>/` in the folder of the library file
//! (`covers/movies/` beside `library.json`), one folder per type, named
//! as the type is in entry ids: movies, shows, books, games, anime, music.
//! All in lower case, like every other folder the app keeps. A file is named
//! `<slug>[.<label>].<ext>`, the slug being the entry id without its type
//! (`movies/the-mummy-1999` is `covers/movies/the-mummy-1999.steelbook.jpg`),
//! and belongs to a title when its name is the slug followed by a dot.
//! The name only makes a file a candidate: the cover an entry shows is
//! the one its `cover_from` names, `repo:movies/the-mummy-1999.a3f9c2e.jpg`
//! (the path under `covers/`), so a file committed by hand is offered in
//! Pick a cover and never applied on its own.
//!
//! [`covers`] is what the app knows of the folder: the list GitHub gave
//! (one tree call, kept in `covers.json` under the cache folder for the
//! next start), with the changes this computer has queued on top, and the
//! copies on this computer in `covers/` beside the local library. A copy
//! is fetched when a screen shows it; an upload is written there first
//! and pushed with the commit that sets `cover_from` (`sync`). With the
//! library kept on this computer the folder is all there is.

pub mod artists;
pub mod backfill;
pub mod picture;
pub mod source;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use self::artists::{Missing, MissingEdit};
use crate::github::{self, blob_sha};
use crate::lookup::store::JsonFile;
use crate::model::{Entry, Library, MediaType};

/// What a `cover_from` naming one of the owner's files starts with.
pub const SCHEME: &str = "repo:";
/// The folder beside the library file.
pub const FOLDER: &str = "covers";
/// How long a download that failed waits before it is tried again.
const RETRY: Duration = Duration::from_secs(60);
/// How long a listing is trusted before Pick a cover asks again.
pub const LISTING_AGE: Duration = Duration::from_secs(60);
/// The `covers.json` format.
const VERSION: i64 = 1;

/// The type's folder under `covers/`: "movies", the type's name in
/// entry ids.
pub fn folder(media: MediaType) -> &'static str {
    media.key()
}

/// The entry id without its type: `movies/the-mummy-1999` is
/// `the-mummy-1999`.
pub fn slug(id: &str) -> &str {
    id.split_once('/').map_or(id, |(_, slug)| slug)
}

/// The folder of the type the id was made for, which stays when the
/// entry's type changes: `movies/the-mummy-1999` is "movies".
pub fn id_folder(id: &str) -> Option<&'static str> {
    let (key, _) = id.split_once('/')?;
    MediaType::ALL
        .into_iter()
        .find(|media| media.key() == key)
        .map(folder)
}

/// The path under `covers/` a `cover_from` names, when it names one of
/// the owner's files.
pub fn repo_path(cover_from: &str) -> Option<&str> {
    cover_from.strip_prefix(SCHEME)
}

/// Whether the entry shows one of the owner's files.
pub fn is_repo(cover_from: Option<&str>) -> bool {
    cover_from.is_some_and(|cover_from| cover_from.starts_with(SCHEME))
}

/// The `cover_from` for the file at `path` under `covers/`.
pub fn cover_from(path: &str) -> String {
    format!("{SCHEME}{path}")
}

/// "movies/the-mummy-1999.jpg" is ("movies", "the-mummy-1999.jpg"); a
/// file right under `covers/` has no folder.
pub fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// The file's name without its folder.
pub fn name_of(path: &str) -> &str {
    split(path).1
}

/// The slug a file name is for: all of it before the first dot, since a
/// slug has none.
pub fn slug_of_name(name: &str) -> &str {
    name.split_once('.').map_or(name, |(slug, _)| slug)
}

/// Whether the file `name` belongs to the title with `slug`: the slug
/// followed by a dot.
pub fn belongs(name: &str, slug: &str) -> bool {
    !slug.is_empty()
        && name
            .strip_prefix(slug)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// The label between the slug and the extension, and the extension:
/// "the-mummy-1999.steelbook.jpg" is ("steelbook", "jpg"), and
/// "the-mummy-1999.jpg" is ("", "jpg").
pub fn label_and_extension(name: &str) -> (&str, &str) {
    let rest = name.split_once('.').map_or("", |(_, rest)| rest);
    match rest.rsplit_once('.') {
        Some((label, extension)) => (label, extension),
        None => ("", rest),
    }
}

/// Whether the file is an image the app can draw, by its extension.
pub fn is_image(name: &str) -> bool {
    let extension = name.rsplit_once('.').map_or("", |(_, e)| e);
    ["jpg", "jpeg", "png", "webp", "gif"]
        .iter()
        .any(|known| extension.eq_ignore_ascii_case(known))
}

/// The name an upload gets: the slug, the first seven characters of the
/// file's git blob sha and "jpg", so two computers never pick the same
/// name for different images and a new image never hides behind a cached
/// one. All in lower case, like the folders.
pub fn upload_name(slug: &str, sha: &str) -> String {
    format!("{slug}.{}.jpg", &sha[..sha.len().min(7)]).to_lowercase()
}

/// Whether a label is the short sha of an upload from the app.
pub fn is_upload_label(label: &str) -> bool {
    label.len() == 7
        && label
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The folder in the repo: `covers` beside the library file, so
/// `data/library.json` has `data/covers`.
pub fn root(library_file: &str) -> String {
    match library_file.rsplit_once('/') {
        Some((dir, _)) if !dir.is_empty() => format!("{dir}/{FOLDER}"),
        _ => FOLDER.to_owned(),
    }
}

/// "1.4 MB", "240 KB".
pub fn size_label(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else {
        format!("{} KB", bytes.div_ceil(1000).max(1))
    }
}

/// How an artist's photo came to be the artist's file, as this computer
/// remembers it (`covers.json`): the file itself says nothing, so on
/// another computer, or with the cache cleared, it is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoKind {
    /// Apple's photo of the matched artist: the fetch's, or the
    /// Suggested tile.
    Fetched,
    /// Another picture picked in Pick a cover.
    Picked,
    Uploaded,
}

impl PhotoKind {
    fn key(self) -> &'static str {
        match self {
            PhotoKind::Fetched => "fetched",
            PhotoKind::Picked => "picked",
            PhotoKind::Uploaded => "uploaded",
        }
    }

    fn parse(key: &str) -> Option<PhotoKind> {
        match key {
            "fetched" => Some(PhotoKind::Fetched),
            "picked" => Some(PhotoKind::Picked),
            "uploaded" => Some(PhotoKind::Uploaded),
            _ => None,
        }
    }
}

/// One file under `covers/`: its path there ("movies/the-mummy-1999.jpg"),
/// its size and git's blob sha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub size: u64,
    pub sha: String,
}

impl File {
    pub fn folder(&self) -> &str {
        split(&self.path).0
    }

    pub fn name(&self) -> &str {
        name_of(&self.path)
    }

    /// The image's key in `artwork`, and what `cover_from` holds.
    pub fn url(&self) -> String {
        cover_from(&self.path)
    }
}

/// A change to the files, queued for the next commit (`sync`): paths
/// under `covers/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The file this computer wrote to its own `covers/`.
    Upload {
        path: String,
    },
    Delete {
        path: String,
    },
    /// The same blob at another path: a type change moves a title's
    /// files to the new type's folder.
    Move {
        from: String,
        to: String,
    },
    /// A file under a fixed name (an artist's photo), written only over
    /// what this computer saw there: `seen` lists the blobs the path may
    /// hold on GitHub, `None` for no file. Over anything else the upload
    /// is dropped and GitHub's file taken, so of two devices that put a
    /// photo there before syncing the first to push wins. `picked` says
    /// the owner chose the picture, who is then told it was dropped.
    Put {
        path: String,
        seen: Vec<Option<String>>,
        picked: bool,
    },
    /// The fixed-name file deleted (Use automatic), only while it is
    /// still one of the blobs `seen`.
    Remove {
        path: String,
        seen: Vec<Option<String>>,
    },
    /// An edit to `artists/_missing.json`, merged per artist into the
    /// repo's copy when the commit is made.
    Missing(MissingEdit),
    /// A change to `settings.json` beside the library, which is no cover
    /// file but goes out the same way: merged into the repo's copy when
    /// the commit is made (`shared::merged`).
    Settings(crate::shared::Edit),
}

impl Change {
    pub fn to_json(&self) -> Value {
        match self {
            Change::Upload { path } => json!({"change": "upload", "path": path}),
            Change::Delete { path } => json!({"change": "delete", "path": path}),
            Change::Move { from, to } => json!({"change": "move", "path": from, "to": to}),
            Change::Put { path, seen, picked } => {
                json!({"change": "put", "path": path, "seen": seen, "picked": picked})
            }
            Change::Remove { path, seen } => {
                json!({"change": "remove", "path": path, "seen": seen})
            }
            Change::Missing(edit) => {
                let mut json = edit.to_json();
                json["change"] = json!("missing");
                json
            }
            Change::Settings(edit) => {
                let mut json = edit.to_json();
                json["change"] = json!("settings");
                json
            }
        }
    }

    pub fn from_json(json: &Value) -> Option<Change> {
        let path = || json["path"].as_str().map(str::to_owned);
        let seen = || -> Option<Vec<Option<String>>> {
            Some(
                json["seen"]
                    .as_array()?
                    .iter()
                    .map(|sha| sha.as_str().map(str::to_owned))
                    .collect(),
            )
        };
        match json["change"].as_str()? {
            "upload" => Some(Change::Upload { path: path()? }),
            "delete" => Some(Change::Delete { path: path()? }),
            "move" => Some(Change::Move {
                from: path()?,
                to: json["to"].as_str()?.to_owned(),
            }),
            "put" => Some(Change::Put {
                path: path()?,
                seen: seen()?,
                picked: json["picked"].as_bool().unwrap_or(false),
            }),
            "remove" => Some(Change::Remove {
                path: path()?,
                seen: seen()?,
            }),
            "missing" => MissingEdit::from_json(json).map(Change::Missing),
            "settings" => crate::shared::Edit::from_json(json).map(Change::Settings),
            _ => None,
        }
    }

    /// The fixed-name path the change writes or deletes on a condition.
    pub fn guarded_path(&self) -> Option<&str> {
        match self {
            Change::Put { path, .. } | Change::Remove { path, .. } => Some(path),
            _ => None,
        }
    }
}

/// The files of `covers/` as the app knows them, and the copies on this
/// computer.
#[derive(Debug, Default)]
pub struct Covers {
    /// `covers/` beside the local library; none keeps nothing on disk
    /// (the tests, the mock runs).
    dir: Option<PathBuf>,
    /// `covers.json` under the cache folder.
    file: Option<JsonFile>,
    /// The list as GitHub last gave it.
    remote: Vec<File>,
    /// The list the app shows: GitHub's with the queued changes applied,
    /// or the folder itself with the library kept on this computer.
    files: Vec<File>,
    /// The blob sha of each copy on this computer, by path.
    have: HashMap<String, String>,
    fetching: HashSet<String>,
    failed: HashMap<String, Instant>,
    /// The pictures uploaded in this session as they were picked, so Fit
    /// your cover crops the picture itself and not its scaled copy.
    originals: Vec<(String, std::sync::Arc<Vec<u8>>)>,
    /// How each artist's photo on this computer came to be, by path:
    /// the blob it was said of, and the kind.
    kinds: HashMap<String, (String, PhotoKind)>,
    /// `artists/_missing.json` as GitHub has it (the copy on this
    /// computer) and as the app reads it, with the queued edits on top.
    missing_remote: Missing,
    missing: Missing,
    /// Where the artists' photos are drawn from: copies named after
    /// their blob under the cache folder.
    shown_dir: Option<PathBuf>,
    /// A listing is on its way, and when the last one came.
    pub listing: bool,
    pub listed: Option<Instant>,
}

impl Covers {
    /// The state over `dir` (`covers/` beside the library) and the cache
    /// folder's `covers.json`, with the list of the last session.
    pub fn at(dir: Option<PathBuf>, cache: Option<&Path>) -> Covers {
        let mut covers = Covers {
            dir,
            file: cache.map(|cache| JsonFile(cache.join("covers.json"))),
            shown_dir: cache.map(|cache| cache.join(artists::FOLDER)),
            ..Covers::default()
        };
        if let Some(json) = covers.file.as_ref().and_then(JsonFile::read)
            && json["version"].as_i64() == Some(VERSION)
        {
            covers.remote = json["files"]
                .as_array()
                .map(|files| files.iter().filter_map(file_from_json).collect())
                .unwrap_or_default();
            if let Some(Value::Object(have)) = json.get("have") {
                covers.have = have
                    .iter()
                    .filter_map(|(path, sha)| Some((path.clone(), sha.as_str()?.to_owned())))
                    .collect();
            }
            if let Some(Value::Object(kinds)) = json.get("kinds") {
                covers.kinds = kinds
                    .iter()
                    .filter_map(|(path, kind)| {
                        let sha = kind["sha"].as_str()?.to_owned();
                        let kind = PhotoKind::parse(kind["kind"].as_str()?)?;
                        Some((path.clone(), (sha, kind)))
                    })
                    .collect();
            }
            covers.files = covers.remote.clone();
        }
        covers.load_missing();
        covers
    }

    /// A list for the mock runs and the captures, with nothing on disk.
    pub fn with_files(files: Vec<File>) -> Covers {
        let mut covers = Covers {
            remote: files,
            ..Covers::default()
        };
        covers.rebuild(&[]);
        covers
    }

    fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        let files: Vec<Value> = self.remote.iter().map(file_to_json).collect();
        let have: serde_json::Map<String, Value> = self
            .have
            .iter()
            .map(|(path, sha)| (path.clone(), Value::String(sha.clone())))
            .collect();
        let kinds: serde_json::Map<String, Value> = self
            .kinds
            .iter()
            .map(|(path, (sha, kind))| (path.clone(), json!({"sha": sha, "kind": kind.key()})))
            .collect();
        let _ =
            file.write(&json!({"version": VERSION, "files": files, "have": have, "kinds": kinds}));
    }

    pub fn files(&self) -> &[File] {
        &self.files
    }

    pub fn get(&self, path: &str) -> Option<&File> {
        self.files.iter().find(|file| file.path == path)
    }

    /// Where the copy of `path` is, or would be, on this computer.
    pub fn local_path(&self, path: &str) -> Option<PathBuf> {
        let dir = self.dir.as_ref()?;
        // A path from `library.json` is text from outside: nothing that
        // climbs out of the folder is followed.
        if path.split('/').any(|part| part == ".." || part.is_empty()) {
            return None;
        }
        Some(dir.join(path))
    }

    /// The copy of `path` on this computer, when it is there and is the
    /// file GitHub lists (a file replaced there under the same name is
    /// fetched again).
    pub fn ready(&self, path: &str) -> Option<PathBuf> {
        let local = self.local_path(path)?;
        let have = self.have.get(path)?;
        if let Some(file) = self.get(path)
            && !file.sha.is_empty()
            && file.sha != *have
        {
            return None;
        }
        if !local.is_file() {
            return None;
        }
        Some(self.shown_copy(path, have).unwrap_or(local))
    }

    /// An artist's photo keeps one name whatever the picture, and iced
    /// keeps a decoded image by its path, so a replaced photo would go
    /// on showing the old one. It is drawn from a copy named after its
    /// blob instead, `artists/<slug>.<sha>.jpg` under the cache folder,
    /// made the first time it is asked for.
    fn shown_copy(&self, path: &str, sha: &str) -> Option<PathBuf> {
        let slug = artists::slug_of_path(path)?;
        let dir = self.shown_dir.as_ref()?;
        let local = self.local_path(path)?;
        let copy = dir.join(format!("{slug}.{}.jpg", &sha[..sha.len().min(12)]));
        if copy.is_file() {
            return Some(copy);
        }
        std::fs::create_dir_all(dir).ok()?;
        // The copies of the pictures the artist had before.
        for old in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = old.file_name().to_string_lossy().into_owned();
            if belongs(&name, slug) {
                let _ = std::fs::remove_file(old.path());
            }
        }
        std::fs::hard_link(&local, &copy)
            .or_else(|_| std::fs::copy(&local, &copy).map(|_| ()))
            .ok()?;
        Some(copy)
    }

    // The artists' photos (`artists`).

    /// The artists tried with nothing found, with this computer's queued
    /// edits.
    pub fn missing(&self) -> &Missing {
        &self.missing
    }

    /// The artist's file, when there is one.
    pub fn artist_file(&self, name: &str) -> Option<&File> {
        self.get(&artists::file_path(name)?)
    }

    /// Remembers how the photo at `path`, as it is on this computer now,
    /// came to be.
    pub fn set_kind(&mut self, path: &str, kind: PhotoKind) {
        if let Some(sha) = self.have.get(path).cloned() {
            self.kinds.insert(path.to_owned(), (sha, kind));
            self.save();
        }
    }

    /// How the photo at `path` came to be, when this computer made it
    /// and it is still that picture.
    pub fn kind(&self, path: &str) -> Option<PhotoKind> {
        let (sha, kind) = self.kinds.get(path)?;
        (self.have.get(path) == Some(sha)).then_some(*kind)
    }

    /// The blobs a fixed-name path may hold on GitHub when a change made
    /// now is pushed: what GitHub listed, the copy on this computer, and
    /// what the changes still queued for the path allowed.
    pub fn seen(&self, path: &str, queued: &[Change]) -> Vec<Option<String>> {
        let mut seen = vec![
            self.remote
                .iter()
                .find(|file| file.path == path)
                .map(|file| file.sha.clone()),
        ];
        seen.extend(self.have.get(path).cloned().map(Some));
        for change in queued {
            if let Change::Put {
                path: at, seen: s, ..
            }
            | Change::Remove { path: at, seen: s } = change
                && at == path
            {
                seen.extend(s.iter().cloned());
            }
        }
        let mut unique = Vec::new();
        for sha in seen {
            if !unique.contains(&sha) {
                unique.push(sha);
            }
        }
        unique
    }

    /// Reads GitHub's `_missing.json` from its copy on this computer. A
    /// copy that cannot be read counts as empty here and is never
    /// written over (`artists::merged_missing`).
    pub fn load_missing(&mut self) {
        let path = artists::missing_path();
        self.missing_remote = self
            .remote
            .iter()
            .any(|file| file.path == path)
            .then(|| self.local_path(&path))
            .flatten()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Missing::decode(&text).ok())
            .unwrap_or_default();
        self.missing = self.missing_remote.clone();
    }

    /// A push dropped this computer's file at `path` for GitHub's: the
    /// copy here goes, and GitHub's is fetched when it is next shown.
    pub fn dropped(&mut self, path: &str) {
        if let Some(local) = self.local_path(path) {
            let _ = crate::storage::remove_if_exists(&local);
        }
        self.have.remove(path);
        self.files.retain(|file| file.path != path);
        self.save();
    }

    /// Whether the copy of `path` is being fetched, or failed a moment
    /// ago.
    pub fn failed(&self, path: &str) -> bool {
        self.failed.get(path).is_some_and(|at| at.elapsed() < RETRY)
    }

    /// Of `paths`, the ones to fetch now: listed, not on this computer,
    /// not on their way and not just failed. They count as on their way
    /// from here on.
    pub fn to_fetch(&mut self, paths: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for path in paths {
            if self.dir.is_none()
                || self.fetching.contains(path)
                || self.failed(path)
                || self.ready(path).is_some()
                || self.get(path).is_none()
            {
                continue;
            }
            self.fetching.insert(path.clone());
            out.push(path.clone());
        }
        out
    }

    /// A download came back: the copy's sha on success.
    pub fn fetched(&mut self, path: &str, outcome: &Result<String, String>) {
        self.fetching.remove(path);
        match outcome {
            Ok(sha) => {
                self.failed.remove(path);
                self.have.insert(path.to_owned(), sha.clone());
                self.save();
            }
            Err(error) => {
                eprintln!("libri: could not fetch the cover {path}: {error}");
                self.failed.insert(path.to_owned(), Instant::now());
            }
        }
    }

    /// GitHub's list, with the changes still queued applied on top.
    pub fn listed(&mut self, remote: Vec<File>, queued: &[Change]) {
        self.remote = remote;
        self.listing = false;
        self.listed = Some(Instant::now());
        // A copy of a file that is gone from GitHub and not queued here
        // is forgotten; the file itself stays until the next prune.
        self.rebuild(queued);
        let known: HashSet<&str> = self.files.iter().map(|f| f.path.as_str()).collect();
        self.have.retain(|path, _| known.contains(path.as_str()));
        self.save();
    }

    /// The shown list from GitHub's and the queued changes.
    pub fn rebuild(&mut self, queued: &[Change]) {
        let mut files = self.remote.clone();
        let mut edits = Vec::new();
        for change in queued {
            self.apply_to(&mut files, change);
            if let Change::Missing(edit) = change {
                edits.push(edit.clone());
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        self.files = files;
        self.missing = self.merged_missing(&edits);
    }

    /// GitHub's record with `edits` on top, without the artists that
    /// have a file in the shown list. An artist who left the library
    /// stays here until a commit writes the record (`sync::commit`); no
    /// screen counts one.
    fn merged_missing(&self, edits: &[MissingEdit]) -> Missing {
        let with_file: HashSet<&str> = self
            .files
            .iter()
            .filter_map(|file| artists::slug_of_path(&file.path))
            .collect();
        self.missing_remote
            .clone()
            .merged(edits, |slug| with_file.contains(slug), |_| true)
    }

    fn apply_to(&self, files: &mut Vec<File>, change: &Change) {
        match change {
            Change::Upload { path } | Change::Put { path, .. } => {
                files.retain(|file| file.path != *path);
                let size = self
                    .local_path(path)
                    .and_then(|local| std::fs::metadata(local).ok())
                    .map_or(0, |meta| meta.len());
                files.push(File {
                    path: path.clone(),
                    size,
                    sha: self.have.get(path).cloned().unwrap_or_default(),
                });
            }
            Change::Delete { path } | Change::Remove { path, .. } => {
                files.retain(|file| file.path != *path)
            }
            Change::Missing(_) | Change::Settings(_) => {}
            Change::Move { from, to } => {
                if let Some(at) = files.iter().position(|file| file.path == *from) {
                    let mut moved = files.remove(at);
                    moved.path = to.clone();
                    files.retain(|file| file.path != *to);
                    files.push(moved);
                }
            }
        }
    }

    /// Writes an upload to this computer's folder and lists it; the
    /// change that pushes it is queued by the caller. In memory alone
    /// (the tests) the file is only listed.
    pub fn store(&mut self, path: &str, bytes: &[u8]) -> std::io::Result<File> {
        let sha = blob_sha(bytes);
        if let Some(local) = self.local_path(path) {
            write_bytes(&local, bytes)?;
        }
        self.have.insert(path.to_owned(), sha.clone());
        let file = File {
            path: path.to_owned(),
            size: bytes.len() as u64,
            sha,
        };
        self.files.retain(|listed| listed.path != path);
        self.files.push(file.clone());
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.save();
        Ok(file)
    }

    /// Takes a delete or a move on board at once: the shown list and the
    /// copy on this computer. `settled` says the change needs no commit
    /// (the library is kept on this computer), so GitHub's list follows.
    pub fn apply(&mut self, change: &Change, settled: bool) {
        match change {
            Change::Upload { .. } | Change::Put { .. } | Change::Settings(_) => {}
            Change::Delete { path } | Change::Remove { path, .. } => {
                if let Some(local) = self.local_path(path) {
                    let _ = crate::storage::remove_if_exists(&local);
                }
                self.have.remove(path);
            }
            Change::Missing(edit) => {
                self.missing.apply(edit);
                if settled {
                    // The library is kept on this computer: the file here
                    // is the record.
                    self.missing_remote.apply(edit);
                    self.write_missing();
                }
            }
            Change::Move { from, to } => {
                if let (Some(old), Some(new)) = (self.local_path(from), self.local_path(to))
                    && old.is_file()
                {
                    if let Some(parent) = new.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::rename(old, new);
                }
                if let Some(sha) = self.have.remove(from) {
                    self.have.insert(to.clone(), sha);
                }
            }
        }
        let mut files = std::mem::take(&mut self.files);
        self.apply_to(&mut files, change);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        self.files = files;
        if settled {
            self.remote = self.files.clone();
        }
        // An artist with a file is no longer missing.
        let with_file: HashSet<&str> = self
            .files
            .iter()
            .filter_map(|file| artists::slug_of_path(&file.path))
            .collect();
        self.missing
            .artists
            .retain(|slug, _| !with_file.contains(slug.as_str()));
        self.save();
    }

    /// Writes the record to this computer's folder (the library kept
    /// here, with no GitHub to merge into).
    fn write_missing(&mut self) {
        let path = artists::missing_path();
        let Some(local) = self.local_path(&path) else {
            return;
        };
        let text = self.missing_remote.encode();
        if write_bytes(&local, text.as_bytes()).is_ok() {
            self.have.insert(path, blob_sha(text.as_bytes()));
        }
    }

    /// The folder on this computer as the whole list: the library is
    /// kept here, so there is no GitHub to ask.
    pub fn scan(&mut self) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        let mut files = Vec::new();
        let folders = std::fs::read_dir(&dir).into_iter().flatten().flatten();
        for folder in folders.filter(|entry| entry.path().is_dir()) {
            let folder_name = folder.file_name().to_string_lossy().into_owned();
            for file in std::fs::read_dir(folder.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                let name = file.file_name().to_string_lossy().into_owned();
                if !file.path().is_file() || name.ends_with(".tmp") {
                    continue;
                }
                let path = format!("{folder_name}/{name}");
                let size = file.metadata().map_or(0, |meta| meta.len());
                let sha = match self.have.get(&path) {
                    Some(sha) => sha.clone(),
                    None => match std::fs::read(file.path()) {
                        Ok(bytes) => blob_sha(&bytes),
                        Err(_) => continue,
                    },
                };
                self.have.insert(path.clone(), sha.clone());
                files.push(File { path, size, sha });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        self.remote = files.clone();
        self.files = files;
        self.listed = Some(Instant::now());
        self.load_missing();
        self.save();
    }

    /// Forgets the list and the copies' record (Disconnect without
    /// keeping the library); the files on this computer go too.
    pub fn clear(&mut self, delete_files: bool) {
        self.remote.clear();
        self.files.clear();
        self.fetching.clear();
        self.failed.clear();
        self.originals.clear();
        self.missing_remote = Missing::default();
        self.missing = Missing::default();
        self.listed = None;
        self.listing = false;
        if delete_files {
            self.have.clear();
            if let Some(dir) = &self.dir {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        match &self.file {
            Some(file) if delete_files => file.delete(),
            _ => self.save(),
        }
    }

    /// Keeps the picture an upload was made from, for Fit your cover; the
    /// last few of the session.
    pub fn keep_original(&mut self, path: &str, bytes: std::sync::Arc<Vec<u8>>) {
        self.originals.retain(|(kept, _)| kept != path);
        self.originals.push((path.to_owned(), bytes));
        if self.originals.len() > 4 {
            self.originals.remove(0);
        }
    }

    pub fn original(&self, path: &str) -> Option<std::sync::Arc<Vec<u8>>> {
        self.originals
            .iter()
            .find(|(kept, _)| kept == path)
            .map(|(_, bytes)| bytes.clone())
    }

    // What the screens ask.

    /// The files named after the entry, in its type's folder and, after
    /// a type change, in the folder of the type its id was made for.
    pub fn candidates(&self, entry: &Entry) -> Vec<&File> {
        self.candidates_of(entry.media, &entry.id)
    }

    /// The same for an entry of type `media` with the id `id`.
    pub fn candidates_of(&self, media: MediaType, id: &str) -> Vec<&File> {
        let slug = slug(id);
        let here = folder(media);
        let before = id_folder(id);
        self.files
            .iter()
            .filter(|file| {
                (file.folder() == here || Some(file.folder()) == before)
                    && is_image(file.name())
                    && belongs(file.name(), slug)
            })
            .collect()
    }

    /// Every (folder, slug) a file can be named after.
    fn titles(library: &Library) -> HashSet<(&'static str, &str)> {
        let mut titles = HashSet::new();
        for entry in &library.entries {
            let slug = slug(&entry.id);
            titles.insert((folder(entry.media), slug));
            if let Some(before) = id_folder(&entry.id) {
                titles.insert((before, slug));
            }
        }
        titles
    }

    fn matches_a_title(file: &File, titles: &HashSet<(&'static str, &str)>) -> bool {
        folder_key(file.folder())
            .is_some_and(|folder| titles.contains(&(folder, slug_of_name(file.name()))))
    }

    /// The image files in the type's folder whose name matches no title.
    pub fn others(&self, media: MediaType, library: &Library) -> Vec<&File> {
        let titles = Covers::titles(library);
        let here = folder(media);
        self.files
            .iter()
            .filter(|file| {
                file.folder() == here
                    && is_image(file.name())
                    && !Covers::matches_a_title(file, &titles)
            })
            .collect()
    }

    /// The files no title uses as its cover and whose name matches no
    /// title.
    pub fn unused(&self, library: &Library) -> Vec<&File> {
        let titles = Covers::titles(library);
        let used: HashSet<&str> = library
            .entries
            .iter()
            .filter_map(|entry| entry.cover_from.as_deref().and_then(repo_path))
            .collect();
        self.files
            .iter()
            .filter(|file| {
                // The artists' folder is not the titles': its photos and
                // `_missing.json` are never unused covers.
                !artists::is_artist_path(&file.path)
                    && !used.contains(file.path.as_str())
                    && !Covers::matches_a_title(file, &titles)
            })
            .collect()
    }

    /// Whether another entry than `id` shows the file at `path`.
    pub fn used_elsewhere(library: &Library, path: &str, id: &str) -> bool {
        library.entries.iter().any(|entry| {
            entry.id != id && entry.cover_from.as_deref().and_then(repo_path) == Some(path)
        })
    }
}

/// The files of the mockup's demo, for the mock runs and the captures:
/// a steelbook cover for Million Dollar Baby, a deluxe one for Dirt, and
/// a few files in the type folders that are named after no title.
pub fn demo() -> Vec<File> {
    [
        ("movies/million-dollar-baby-2004.steelbook.jpg", 412_000),
        ("music/dirt-alice-in-chains.deluxe.jpg", 388_000),
        ("movies/old-poster.png", 911_000),
        ("movies/bluray-front.jpg", 240_000),
        ("movies/scan-2.jpg", 198_000),
        ("movies/untitled.png", 76_000),
        ("books/shogun-scan.jpg", 301_000),
        ("music/mezz-front.jpg", 265_000),
        ("music/back-cover.png", 540_000),
    ]
    .into_iter()
    .map(|(path, size)| File {
        path: path.to_owned(),
        size,
        sha: blob_sha(path.as_bytes()),
    })
    .collect()
}

/// The entries that show one of the owner's files.
pub fn entries_using(library: &Library) -> impl Iterator<Item = &Entry> {
    library
        .entries
        .iter()
        .filter(|entry| is_repo(entry.cover_from.as_deref()))
}

/// The folder name as the constant the titles are keyed by.
fn folder_key(name: &str) -> Option<&'static str> {
    MediaType::ALL
        .into_iter()
        .map(folder)
        .find(|known| *known == name)
}

fn file_to_json(file: &File) -> Value {
    json!({"path": file.path, "size": file.size, "sha": file.sha})
}

fn file_from_json(json: &Value) -> Option<File> {
    Some(File {
        path: json["path"].as_str()?.to_owned(),
        size: json["size"].as_u64().unwrap_or(0),
        sha: json["sha"].as_str().unwrap_or_default().to_owned(),
    })
}

/// Writes `bytes` to `path` through a sibling `.tmp` file and a rename,
/// creating the folders, so a crash never leaves half an image.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut name = path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(".tmp");
    let temp = path.with_file_name(name);
    let written = std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// The files of `covers/` beside `library_file` on `branch`, from one
/// tree call. A repo without the folder has none.
pub fn list(client: &github::Client, branch: &str, library_file: &str) -> github::Reply<Vec<File>> {
    let prefix = format!("{}/", root(library_file));
    Ok(client
        .files(branch, "")?
        .into_iter()
        .filter_map(|file| {
            let path = file.path.strip_prefix(&prefix)?;
            // One folder deep: `covers/<type>/<file>`.
            (path.matches('/').count() == 1).then(|| File {
                path: path.to_owned(),
                size: file.size,
                sha: file.sha,
            })
        })
        .collect())
}

/// Fetches the file at `path` under `covers/` to `to` and gives its blob
/// sha.
pub fn download(
    client: &github::Client,
    branch: &str,
    library_file: &str,
    path: &str,
    to: &Path,
) -> Result<String, String> {
    let full = format!("{}/{path}", root(library_file));
    let raw = client
        .file_raw(&full, branch, None)
        .map_err(|failure| failure.to_string())?
        .ok_or("no content")?;
    write_bytes(to, &raw.bytes).map_err(|error| error.to_string())?;
    Ok(raw.blob_sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::storage::TempDir;

    fn library() -> Library {
        crate::model::codec::decode(fixtures::LIBRARY_JSON).expect("the fixture decodes")
    }

    fn file(path: &str) -> File {
        File {
            path: path.into(),
            size: 1000,
            sha: format!("sha-{path}"),
        }
    }

    #[test]
    fn a_file_belongs_to_a_title_by_its_slug_and_a_dot() {
        assert_eq!(slug("movies/the-mummy-1999"), "the-mummy-1999");
        assert_eq!(id_folder("movies/the-mummy-1999"), Some("movies"));
        assert_eq!(id_folder("nope/x"), None);
        assert!(belongs("the-mummy-1999.jpg", "the-mummy-1999"));
        assert!(belongs("the-mummy-1999.steelbook.jpg", "the-mummy-1999"));
        // The second entry of the same title and year is another slug.
        assert!(!belongs("the-mummy-1999-2.jpg", "the-mummy-1999"));
        assert!(!belongs("the-mummy-1999", "the-mummy-1999"));
        assert_eq!(
            label_and_extension("the-mummy-1999.steelbook.jpg"),
            ("steelbook", "jpg")
        );
        assert_eq!(label_and_extension("the-mummy-1999.jpg"), ("", "jpg"));
        assert_eq!(slug_of_name("the-mummy-1999.a3f9c2e.jpg"), "the-mummy-1999");
        assert_eq!(
            upload_name("the-mummy-1999", "a3f9c2e41b0d"),
            "the-mummy-1999.a3f9c2e.jpg"
        );
        assert!(is_upload_label("a3f9c2e") && !is_upload_label("steelbook"));
        // Every name the app makes is in lower case, folder and all.
        for media in MediaType::ALL {
            let path = format!(
                "{FOLDER}/{}/{}",
                folder(media),
                upload_name(slug("movies/the-mummy-1999"), "A3F9C2E41B0D")
            );
            assert_eq!(path, path.to_lowercase());
        }
        assert_eq!(
            repo_path("repo:movies/the-mummy-1999.jpg"),
            Some("movies/the-mummy-1999.jpg")
        );
        assert_eq!(repo_path("tmdb:movie:1"), None);
    }

    #[test]
    fn the_folder_sits_beside_the_library_file() {
        assert_eq!(root("library.json"), "covers");
        assert_eq!(root("data/library.json"), "data/covers");
        assert_eq!(size_label(1_400_000), "1.4 MB");
        assert_eq!(size_label(240_123), "241 KB");
    }

    #[test]
    fn candidates_others_and_unused_follow_the_names() {
        let library = library();
        let entry = library
            .entry("movies/million-dollar-baby-2004")
            .expect("in the fixture");
        let covers = Covers::with_files(vec![
            file("movies/million-dollar-baby-2004.steelbook.jpg"),
            file("movies/million-dollar-baby-2004.notes.txt"),
            file("movies/old-poster.png"),
            file("books/million-dollar-baby-2004.jpg"),
            file("movies/gone-title-1990.a91c0d2.jpg"),
        ]);
        let names =
            |files: Vec<&File>| -> Vec<String> { files.iter().map(|f| f.path.clone()).collect() };
        assert_eq!(
            names(covers.candidates(entry)),
            ["movies/million-dollar-baby-2004.steelbook.jpg"]
        );
        assert_eq!(
            names(covers.others(MediaType::Movies, &library)),
            [
                "movies/gone-title-1990.a91c0d2.jpg",
                "movies/old-poster.png"
            ]
        );
        // The book's folder has no such title; the text file is unused
        // too, though no picker offers it.
        assert_eq!(
            names(covers.unused(&library)),
            [
                "books/million-dollar-baby-2004.jpg",
                "movies/gone-title-1990.a91c0d2.jpg",
                "movies/old-poster.png"
            ]
        );
        // A file an entry shows is in use whatever its name.
        let mut library = library;
        let id = "movies/million-dollar-baby-2004";
        let mut changed = library.entry(id).unwrap().clone();
        changed.cover_from = Some("repo:movies/old-poster.png".into());
        *library.entries.iter_mut().find(|e| e.id == id).unwrap() = changed;
        assert!(!names(covers.unused(&library)).contains(&"movies/old-poster.png".to_owned()));
        assert!(Covers::used_elsewhere(
            &library,
            "movies/old-poster.png",
            "movies/x"
        ));
        assert!(!Covers::used_elsewhere(
            &library,
            "movies/old-poster.png",
            id
        ));
    }

    #[test]
    fn a_type_change_still_finds_the_files_of_the_ids_first_type() {
        let mut library = library();
        let entry = library
            .entries
            .iter_mut()
            .find(|e| e.id == "movies/million-dollar-baby-2004")
            .unwrap();
        entry.media = MediaType::Shows;
        let entry = entry.clone();
        let covers = Covers::with_files(vec![
            file("movies/million-dollar-baby-2004.a.jpg"),
            file("shows/million-dollar-baby-2004.b.jpg"),
        ]);
        assert_eq!(covers.candidates(&entry).len(), 2);
        assert!(covers.unused(&library).is_empty());
    }

    #[test]
    fn the_new_changes_are_queued_in_json() {
        let tried = artists::Tried {
            name: "Adele".into(),
            tried: crate::model::Date::parse("2026-11-15").unwrap(),
            reason: artists::Reason::NoMatch,
        };
        let changes = [
            Change::Put {
                path: "artists/adele.jpg".into(),
                seen: vec![None, Some("abc".into())],
                picked: true,
            },
            Change::Remove {
                path: "artists/adele.jpg".into(),
                seen: vec![Some("abc".into())],
            },
            Change::Missing(MissingEdit::Set {
                slug: "adele".into(),
                tried,
            }),
            Change::Missing(MissingEdit::Remove {
                slug: "adele".into(),
            }),
            Change::Missing(MissingEdit::Backfilled(
                crate::model::Date::parse("2026-11-16").unwrap(),
            )),
        ];
        for change in changes {
            assert_eq!(Change::from_json(&change.to_json()), Some(change));
        }
    }

    #[test]
    fn an_artists_photo_is_drawn_from_a_copy_named_after_its_blob() {
        let dir = TempDir::new("covers");
        let mut covers = Covers::at(Some(dir.path("covers")), Some(&dir.0));
        covers.store("artists/adele.jpg", b"first").unwrap();
        let first = covers.ready("artists/adele.jpg").unwrap();
        assert!(first.starts_with(dir.path("artists")), "{first:?}");
        assert_eq!(std::fs::read(&first).unwrap(), b"first");
        // Replaced under the same name: another path, so no decoded
        // image is reused, and the old copy is gone.
        covers.store("artists/adele.jpg", b"second").unwrap();
        let second = covers.ready("artists/adele.jpg").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&second).unwrap(), b"second");
        assert!(!first.exists());
        // A title's cover is drawn from the folder itself.
        covers.store("music/a.1234567.jpg", b"cover").unwrap();
        assert_eq!(
            covers.ready("music/a.1234567.jpg").unwrap(),
            dir.path("covers").join("music/a.1234567.jpg")
        );
    }

    #[test]
    fn a_fixed_name_remembers_what_it_may_replace() {
        let dir = TempDir::new("covers");
        let mut covers = Covers::at(Some(dir.path("covers")), Some(&dir.0));
        assert_eq!(covers.seen("artists/adele.jpg", &[]), [None]);
        covers.store("artists/adele.jpg", b"first").unwrap();
        let first = Change::Put {
            path: "artists/adele.jpg".into(),
            seen: vec![None],
            picked: true,
        };
        // A second pick before the first is pushed may meet no file or
        // the first.
        assert_eq!(
            covers.seen("artists/adele.jpg", std::slice::from_ref(&first)),
            [None, Some(blob_sha(b"first"))]
        );
        // GitHub lists another device's photo: that is what was seen
        covers.listed(
            vec![File {
                path: "artists/adele.jpg".into(),
                size: 5,
                sha: "theirs".into(),
            }],
            &[],
        );
        // (with the copy here, which a push of this computer's made).
        assert_eq!(
            covers.seen("artists/adele.jpg", &[]),
            [Some("theirs".to_owned()), Some(blob_sha(b"first"))]
        );
        // A push dropped this computer's file: GitHub's is fetched.
        covers.store("artists/adele.jpg", b"mine").unwrap();
        covers.dropped("artists/adele.jpg");
        assert!(covers.ready("artists/adele.jpg").is_none());
        assert!(!dir.path("covers").join("artists/adele.jpg").exists());
    }

    #[test]
    fn the_missing_record_is_githubs_with_the_queued_edits() {
        let dir = TempDir::new("covers");
        let mut covers = Covers::at(Some(dir.path("covers")), Some(&dir.0));
        let day = crate::model::Date::parse("2026-11-15").unwrap();
        let tried = |name: &str| artists::Tried {
            name: name.into(),
            tried: day,
            reason: artists::Reason::NoPhoto,
        };
        let mut theirs = Missing::default();
        theirs.artists.insert("can".into(), tried("Can"));
        theirs.artists.insert("adele".into(), tried("Adele"));
        let text = theirs.encode();
        write_bytes(
            &dir.path("covers").join("artists/_missing.json"),
            text.as_bytes(),
        )
        .unwrap();
        let listing = |extra: &[&str]| {
            let mut files = vec![File {
                path: artists::missing_path(),
                size: text.len() as u64,
                sha: blob_sha(text.as_bytes()),
            }];
            files.extend(extra.iter().map(|path| File {
                path: (*path).to_owned(),
                size: 1,
                sha: "photo".into(),
            }));
            files
        };
        let queued = [Change::Missing(MissingEdit::Set {
            slug: "burial".into(),
            tried: tried("Burial"),
        })];
        covers.listed(listing(&[]), &queued);
        covers.load_missing();
        covers.rebuild(&queued);
        let slugs =
            |covers: &Covers| -> Vec<String> { covers.missing().artists.keys().cloned().collect() };
        assert_eq!(slugs(&covers), ["adele", "burial", "can"]);
        assert!(covers.missing().is_due("low", day, None));
        assert!(!covers.missing().is_due("can", day, None));
        // A photo for Adele appears on GitHub: no longer missing.
        covers.listed(listing(&["artists/adele.jpg"]), &queued);
        assert_eq!(slugs(&covers), ["burial", "can"]);
        // The record is gone from GitHub: only the queued edit is left.
        covers.listed(Vec::new(), &queued);
        covers.load_missing();
        covers.rebuild(&queued);
        assert_eq!(slugs(&covers), ["burial"]);
    }

    #[test]
    fn the_artists_folder_is_never_unused_covers() {
        let covers = Covers::with_files(
            [
                "artists/adele.jpg",
                "artists/_missing.json",
                "music/stray.jpg",
            ]
            .into_iter()
            .map(|path| File {
                path: path.to_owned(),
                size: 1,
                sha: "sha".into(),
            })
            .collect(),
        );
        let library = library();
        let unused: Vec<&str> = covers
            .unused(&library)
            .into_iter()
            .map(|file| file.path.as_str())
            .collect();
        assert_eq!(unused, ["music/stray.jpg"]);
    }

    #[test]
    fn queued_changes_show_on_top_of_githubs_list() {
        let dir = TempDir::new("covers");
        let mut covers = Covers::at(Some(dir.path("covers")), Some(&dir.0));
        let stored = covers.store("movies/a.1234567.jpg", b"jpeg bytes").unwrap();
        assert_eq!(stored.sha, blob_sha(b"jpeg bytes"));
        assert!(dir.path("covers").join("movies/a.1234567.jpg").is_file());
        assert_eq!(
            covers.ready("movies/a.1234567.jpg").unwrap(),
            dir.path("covers").join("movies/a.1234567.jpg")
        );

        let queued = [
            Change::Upload {
                path: "movies/a.1234567.jpg".into(),
            },
            Change::Delete {
                path: "movies/old.png".into(),
            },
            Change::Move {
                from: "movies/b.jpg".into(),
                to: "shows/b.jpg".into(),
            },
        ];
        covers.listed(vec![file("movies/old.png"), file("movies/b.jpg")], &queued);
        let paths: Vec<&str> = covers.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["movies/a.1234567.jpg", "shows/b.jpg"]);
        assert_eq!(covers.get("shows/b.jpg").unwrap().sha, "sha-movies/b.jpg");

        // The list and the copies' shas are there after a restart.
        let again = Covers::at(Some(dir.path("covers")), Some(&dir.0));
        assert_eq!(again.remote.len(), 2);
        assert!(again.have.contains_key("movies/a.1234567.jpg"));

        // A file replaced on GitHub under the same name is fetched again.
        covers.listed(
            vec![File {
                path: "movies/a.1234567.jpg".into(),
                size: 5,
                sha: "another".into(),
            }],
            &[],
        );
        assert!(covers.ready("movies/a.1234567.jpg").is_none());
        assert_eq!(
            covers.to_fetch(&["movies/a.1234567.jpg".into(), "movies/unlisted.jpg".into()]),
            ["movies/a.1234567.jpg"]
        );
        assert!(
            covers.to_fetch(&["movies/a.1234567.jpg".into()]).is_empty(),
            "on its way"
        );
        covers.fetched("movies/a.1234567.jpg", &Err("offline".into()));
        assert!(covers.failed("movies/a.1234567.jpg"));
    }

    #[test]
    fn a_delete_and_a_move_reach_the_copies_on_this_computer() {
        let dir = TempDir::new("covers");
        let mut covers = Covers::at(Some(dir.path("covers")), Some(&dir.0));
        covers.store("movies/a.jpg", b"a").unwrap();
        covers.store("movies/b.jpg", b"b").unwrap();
        covers.apply(
            &Change::Move {
                from: "movies/a.jpg".into(),
                to: "shows/a.jpg".into(),
            },
            true,
        );
        covers.apply(
            &Change::Delete {
                path: "movies/b.jpg".into(),
            },
            true,
        );
        assert!(dir.path("covers").join("shows/a.jpg").is_file());
        assert!(!dir.path("covers").join("movies/a.jpg").exists());
        assert!(!dir.path("covers").join("movies/b.jpg").exists());
        assert_eq!(covers.files().len(), 1);
        assert!(covers.ready("shows/a.jpg").is_some());

        // With the library kept on this computer the folder is the list.
        let mut local = Covers::at(Some(dir.path("covers")), None);
        local.scan();
        assert_eq!(local.files().len(), 1);
        assert_eq!(local.files()[0].sha, blob_sha(b"a"));
        // Nothing climbs out of the folder.
        assert!(local.local_path("../library.json").is_none());
    }

    #[test]
    fn changes_round_trip_through_json() {
        for change in [
            Change::Upload {
                path: "movies/a.jpg".into(),
            },
            Change::Delete {
                path: "movies/a.jpg".into(),
            },
            Change::Move {
                from: "movies/a.jpg".into(),
                to: "shows/a.jpg".into(),
            },
        ] {
            assert_eq!(Change::from_json(&change.to_json()), Some(change));
        }
    }
}
