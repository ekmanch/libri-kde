//! What the lookups keep on this computer, under `~/.cache/libri/`: the
//! cover images (`images/<sha1 of the url>.<jpg, png, webp or gif>`,
//! named by what the bytes are, since the image decoder goes by the
//! extension), the records of which
//! cover an entry got (`artwork.json`), album track lists (`tracks/`), a
//! season's episodes (`episodes/`) and a series' parts (`series/`), each
//! in the phone's file formats. Everything here can be fetched again, so
//! a cleared cache only means asking once more. Without a folder (the
//! tests) nothing is written and nothing is found.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use super::http::{self, int_at, objects_at, str_at};
use super::{Episode, FailureKind, SeriesPart, SeriesParts, TrackInfo};
use crate::clock::Timestamp;
use crate::model::{Date, Released};
use crate::storage;

/// A small JSON file, read whole and written atomically.
#[derive(Debug, Clone)]
pub struct JsonFile(pub PathBuf);

impl JsonFile {
    pub fn read(&self) -> Option<Value> {
        let text = storage::read_if_exists(&self.0).ok().flatten()?;
        serde_json::from_str(&text).ok()
    }

    pub fn write(&self, value: &Value) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(value).unwrap_or_default();
        storage::write_atomically(&self.0, &format!("{text}\n"))
    }

    pub fn delete(&self) {
        let _ = storage::remove_if_exists(&self.0);
    }
}

/// The cache folder's files, by kind.
#[derive(Debug, Clone, Default)]
pub struct Stores {
    dir: Option<PathBuf>,
}

/// One file per id: "tmdb-tv-1399.json".
pub fn file_name_for(source: &str) -> String {
    let safe: String = source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{safe}.json")
}

/// The `series/` file format: 5 since the parts carry their cover.
pub const SERIES_VERSION: i64 = 5;

/// A season's episodes as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEpisodes {
    pub fetched: Timestamp,
    pub provider: String,
    pub episodes: Vec<Episode>,
}

/// A series as stored, with when it was fetched and the entry's series
/// name it was asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSeries {
    pub fetched: Timestamp,
    pub asked: Option<String>,
    pub series: SeriesParts,
}

impl Stores {
    pub fn at(dir: Option<PathBuf>) -> Stores {
        Stores { dir }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    fn file(&self, folder: &str, source: &str) -> Option<JsonFile> {
        Some(JsonFile(
            self.dir.as_ref()?.join(folder).join(file_name_for(source)),
        ))
    }

    /// A file under the folder itself: `artwork.json`, `igdb_token.json`.
    pub fn top(&self, name: &str) -> Option<JsonFile> {
        Some(JsonFile(self.dir.as_ref()?.join(name)))
    }

    // Episodes: one file per show with its fetched seasons by number.

    pub fn read_episodes(&self, show_source: &str, season: u32) -> Option<StoredEpisodes> {
        let json = self.file("episodes", show_source)?.read()?;
        if str_at(&json, "source") != Some(show_source) {
            return None;
        }
        let stored = json.get("seasons")?.get(season.to_string())?;
        let fetched = Timestamp::parse(str_at(stored, "fetched")?)?;
        Some(StoredEpisodes {
            fetched,
            provider: str_at(stored, "provider").unwrap_or("TMDB").to_owned(),
            episodes: objects_at(stored, "episodes")
                .filter_map(|item| {
                    Some(Episode {
                        number: u32::try_from(int_at(item, "number")?).ok()?,
                        title: str_at(item, "title").map(str::to_owned),
                        aired: str_at(item, "aired").and_then(Date::parse),
                        runtime: int_at(item, "runtime").and_then(|r| u32::try_from(r).ok()),
                    })
                })
                .collect(),
        })
    }

    pub fn write_episodes(
        &self,
        show_source: &str,
        season: u32,
        episodes: &[Episode],
        fetched: Timestamp,
        provider: &str,
    ) {
        let Some(file) = self.file("episodes", show_source) else {
            return;
        };
        let mut seasons = file
            .read()
            .filter(|json| str_at(json, "source") == Some(show_source))
            .and_then(|json| json.get("seasons").cloned())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        seasons[season.to_string()] = json!({
            "fetched": fetched.to_rfc3339(),
            "provider": provider,
            "episodes": episodes.iter().map(|e| json!({
                "number": e.number,
                "title": e.title,
                "aired": e.aired.map(|d| d.to_string()),
                "runtime": e.runtime,
            })).collect::<Vec<_>>(),
        });
        let _ = file.write(&json!({"source": show_source, "seasons": seasons}));
    }

    // Series: one file per entry's lookup id.

    pub fn read_series(&self, source: &str) -> Option<StoredSeries> {
        let json = self.file("series", source)?.read()?;
        if str_at(&json, "source") != Some(source) {
            return None;
        }
        let mut fetched = Timestamp::parse(str_at(&json, "fetched")?)?;
        let name = str_at(&json, "name")?;
        // A list stored by older rules shows, and is fetched again at once.
        if int_at(&json, "version") != Some(SERIES_VERSION) {
            fetched = Timestamp(0);
        }
        Some(StoredSeries {
            fetched,
            asked: str_at(&json, "asked").map(str::to_owned),
            series: SeriesParts {
                name: name.to_owned(),
                parts: objects_at(&json, "parts")
                    .filter_map(|item| {
                        Some(SeriesPart {
                            number: int_at(item, "number").and_then(|n| u32::try_from(n).ok()),
                            title: str_at(item, "title")?.to_owned(),
                            year: int_at(item, "year").and_then(|y| u16::try_from(y).ok()),
                            released: str_at(item, "released").and_then(Released::parse),
                            source: str_at(item, "source").map(str::to_owned),
                            creator: str_at(item, "creator").map(str::to_owned),
                            cover: str_at(item, "cover").map(str::to_owned),
                            extra: item.get("extra") == Some(&Value::Bool(true)),
                        })
                    })
                    .collect(),
            },
        })
    }

    pub fn write_series(
        &self,
        source: &str,
        series: &SeriesParts,
        fetched: Timestamp,
        asked: Option<&str>,
    ) {
        let Some(file) = self.file("series", source) else {
            return;
        };
        let mut json = json!({
            "version": SERIES_VERSION,
            "source": source,
            "fetched": fetched.to_rfc3339(),
            "name": series.name,
            "parts": series.parts.iter().map(|p| {
                let mut part = json!({
                    "number": p.number,
                    "title": p.title,
                    "year": p.year,
                    "source": p.source,
                });
                if let Some(released) = p.released {
                    part["released"] = Value::String(released.to_string());
                }
                if let Some(creator) = &p.creator {
                    part["creator"] = Value::String(creator.clone());
                }
                if let Some(cover) = &p.cover {
                    part["cover"] = Value::String(cover.clone());
                }
                if p.extra {
                    part["extra"] = Value::Bool(true);
                }
                part
            }).collect::<Vec<_>>(),
        });
        if let Some(asked) = asked {
            json["asked"] = Value::String(asked.to_owned());
        }
        let _ = file.write(&json);
    }

    // Track lists: one file per album.

    pub fn read_tracks(&self, album_source: &str) -> Option<Vec<TrackInfo>> {
        let json = self.file("tracks", album_source)?.read()?;
        if str_at(&json, "source") != Some(album_source) {
            return None;
        }
        Some(
            objects_at(&json, "tracks")
                .filter_map(|item| {
                    Some(TrackInfo {
                        id: str_at(item, "id")?.to_owned(),
                        title: str_at(item, "title")?.to_owned(),
                    })
                })
                .collect(),
        )
    }

    pub fn write_tracks(&self, album_source: &str, tracks: &[TrackInfo]) {
        let Some(file) = self.file("tracks", album_source) else {
            return;
        };
        let _ = file.write(&json!({
            "source": album_source,
            "fetched": Timestamp::now().to_rfc3339(),
            "tracks": tracks.iter().map(|t| json!({"id": t.id, "title": t.title})).collect::<Vec<_>>(),
        }));
    }

    /// Drops every file in `folder` but those of `keep_sources`.
    pub fn prune(&self, folder: &str, keep_sources: &[String]) {
        let Some(dir) = &self.dir else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(dir.join(folder)) else {
            return;
        };
        let keep: Vec<String> = keep_sources.iter().map(|s| file_name_for(s)).collect();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !keep.contains(&name) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Why an image could not be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFailure {
    /// No network or no answer: tried again soon.
    Offline,
    /// The server answered, but not with an image, or the file could not
    /// be written; tried again much later.
    Other(String),
}

/// How long a failed download waits before it is tried again.
pub const IMAGE_RETRY: Duration = Duration::from_secs(6 * 3600);
pub const IMAGE_RETRY_OFFLINE: Duration = Duration::from_secs(30);

/// Cover images and artist photos on this computer, one file per URL.
pub struct ImageStore {
    dir: Option<PathBuf>,
    http: http::Http,
}

impl std::fmt::Debug for ImageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageStore")
            .field("dir", &self.dir)
            .finish()
    }
}

impl ImageStore {
    pub fn at(dir: Option<PathBuf>) -> ImageStore {
        ImageStore {
            dir: dir.map(|d| d.join("images")),
            http: http::Http::new(Duration::ZERO),
        }
    }

    /// The file an image lives in: the SHA-1 of its URL with the
    /// extension of its format.
    fn path_with(&self, url: &str, extension: &str) -> Option<PathBuf> {
        Some(
            self.dir
                .as_ref()?
                .join(format!("{}.{extension}", key_for(url))),
        )
    }

    /// The file, when it is on this computer already. A file from before
    /// the images were named by their format (`.img`, which the decoder
    /// cannot read) is renamed on the way.
    pub fn cached(&self, url: &str) -> Option<PathBuf> {
        for extension in IMAGE_EXTENSIONS {
            let path = self.path_with(url, extension)?;
            if path.is_file() {
                return Some(path);
            }
        }
        let old = self.path_with(url, "img")?;
        if !old.is_file() {
            return None;
        }
        let head = std::fs::read(&old).ok()?;
        match image_extension(&head) {
            Some(extension) => {
                let path = self.path_with(url, extension)?;
                std::fs::rename(&old, &path).ok()?;
                Some(path)
            }
            None => {
                let _ = std::fs::remove_file(&old);
                None
            }
        }
    }

    /// Downloads the image and writes it, or says why not.
    pub fn fetch(&self, url: &str) -> Result<PathBuf, ImageFailure> {
        if self.dir.is_none() {
            return Err(ImageFailure::Other("no cache folder".into()));
        }
        if let Some(path) = self.cached(url) {
            return Ok(path);
        }
        let response = self
            .http
            .get(url, &[])
            .map_err(|failure| match failure.kind {
                FailureKind::Offline => ImageFailure::Offline,
                _ => ImageFailure::Other(failure.message),
            })?;
        if response.status != 200 {
            return Err(ImageFailure::Other(format!("status {}", response.status)));
        }
        let Some(extension) = image_extension(&response.bytes) else {
            return Err(ImageFailure::Other(format!(
                "not an image ({})",
                response.content_type
            )));
        };
        let path = self
            .path_with(url, extension)
            .ok_or_else(|| ImageFailure::Other("no cache folder".into()))?;
        write_bytes(&path, &response.bytes).map_err(|e| ImageFailure::Other(e.to_string()))?;
        Ok(path)
    }

    /// Removes every image whose URL is not in `keep_urls`. Returns how
    /// many went.
    pub fn prune(&self, keep_urls: &[String]) -> usize {
        let Some(dir) = &self.dir else {
            return 0;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let keep: HashSet<String> = keep_urls.iter().map(|u| key_for(u)).collect();
        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let known = path
                .extension()
                .is_some_and(|e| e == "img" || IMAGE_EXTENSIONS.iter().any(|x| e == *x));
            if (!known || !keep.contains(&stem)) && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

/// The extensions an image file can have, one per format the decoder
/// reads.
const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "png", "webp", "gif"];

/// The extension for the image format `bytes` start with, from the
/// formats' magic numbers; none when it is not an image the decoder reads.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else {
        None
    }
}

/// The SHA-1 of the URL, in hex.
pub fn key_for(url: &str) -> String {
    use sha1::Digest;
    let digest = sha1::Sha1::digest(url.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes `bytes` whole to a sibling `.tmp` and renames it into place.
fn write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent folder"))?;
    std::fs::create_dir_all(parent)?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let written = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stores() -> (storage::TempDir, Stores) {
        let dir = storage::TempDir::new("lookup-store");
        let stores = Stores::at(Some(dir.0.clone()));
        (dir, stores)
    }

    #[test]
    fn episodes_round_trip_per_season_and_prune_by_show() {
        let (_dir, stores) = stores();
        let episodes = vec![Episode {
            number: 1,
            title: Some("Pilot".into()),
            aired: Date::parse("2016-07-15"),
            runtime: Some(49),
        }];
        let now = Timestamp::now();
        stores.write_episodes("tmdb:tv:66732", 1, &episodes, now, "TMDB");
        stores.write_episodes("tmdb:tv:66732", 2, &[], now, "TMDB");
        let stored = stores.read_episodes("tmdb:tv:66732", 1).unwrap();
        assert_eq!(stored.episodes, episodes);
        assert_eq!(stored.provider, "TMDB");
        assert_eq!(stored.fetched.to_rfc3339(), now.to_rfc3339());
        assert!(
            stores
                .read_episodes("tmdb:tv:66732", 2)
                .unwrap()
                .episodes
                .is_empty()
        );
        assert!(stores.read_episodes("tmdb:tv:1", 1).is_none());
        stores.prune("episodes", &["tmdb:tv:1".into()]);
        assert!(stores.read_episodes("tmdb:tv:66732", 1).is_none());
        assert_eq!(file_name_for("tmdb:tv:66732"), "tmdb-tv-66732.json");
    }

    #[test]
    fn series_and_tracks_round_trip_and_an_old_version_is_stale() {
        let (dir, stores) = stores();
        let series = SeriesParts {
            name: "The Lord of the Rings".into(),
            parts: vec![SeriesPart {
                number: Some(1),
                title: "The Fellowship of the Ring".into(),
                year: Some(2001),
                released: Released::parse("2001-12-19"),
                source: Some("tmdb:movie:120".into()),
                creator: None,
                cover: Some("https://x/p.jpg".into()),
                extra: false,
            }],
        };
        let now = Timestamp::now();
        stores.write_series(
            "tmdb:movie:121",
            &series,
            now,
            Some("The Lord of the Rings"),
        );
        let stored = stores.read_series("tmdb:movie:121").unwrap();
        assert_eq!(stored.series, series);
        assert_eq!(stored.asked.as_deref(), Some("The Lord of the Rings"));
        assert_eq!(stored.fetched.to_rfc3339(), now.to_rfc3339());

        // An older version shows but is fetched again at once.
        let file = JsonFile(dir.path("series/tmdb-movie-121.json"));
        let mut json = file.read().unwrap();
        json["version"] = Value::from(4);
        file.write(&json).unwrap();
        assert_eq!(
            stores.read_series("tmdb:movie:121").unwrap().fetched,
            Timestamp(0)
        );

        let tracks = vec![TrackInfo {
            id: "deezer:track:1".into(),
            title: "Them Bones".into(),
        }];
        stores.write_tracks("deezer:album:9", &tracks);
        assert_eq!(stores.read_tracks("deezer:album:9").unwrap(), tracks);
        assert!(stores.read_tracks("deezer:album:8").is_none());
        assert!(Stores::default().read_tracks("deezer:album:9").is_none());
    }

    #[test]
    fn images_are_named_by_their_format_and_pruned() {
        const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];
        let dir = storage::TempDir::new("images");
        let store = ImageStore::at(Some(dir.0.clone()));
        assert_eq!(key_for("x").len(), 40);
        assert_ne!(key_for("x"), key_for("y"));
        assert_eq!(image_extension(JPEG), Some("jpg"));
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_extension(b"GIF89a"), Some("gif"));
        assert_eq!(image_extension(b"<html>"), None);

        let a = "https://example.org/a.jpg";
        let path = store.path_with(a, "jpg").unwrap();
        write_bytes(&path, JPEG).unwrap();
        assert_eq!(store.cached(a), Some(path.clone()));
        assert!(store.cached("https://example.org/b.jpg").is_none());

        // A file from before the names had a format is renamed by its bytes.
        let c = "https://example.org/c";
        write_bytes(&store.path_with(c, "img").unwrap(), b"\x89PNG\r\n\x1a\n").unwrap();
        assert_eq!(store.cached(c), store.path_with(c, "png"));
        assert!(!store.path_with(c, "img").unwrap().exists());
        let d = "https://example.org/d";
        write_bytes(&store.path_with(d, "img").unwrap(), b"<html>").unwrap();
        assert!(store.cached(d).is_none());
        assert!(!store.path_with(d, "img").unwrap().exists());

        assert_eq!(store.prune(&[c.into()]), 1);
        assert!(store.cached(a).is_none());
        assert!(store.cached(c).is_some());
        assert_eq!(
            ImageStore::at(None).fetch(a),
            Err(ImageFailure::Other("no cache folder".into()))
        );
    }
}
