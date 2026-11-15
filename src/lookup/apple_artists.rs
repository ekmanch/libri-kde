//! An artist's photo from Apple Music, for `covers::artists`.
//!
//! Three kinds of request, each with its own pace. The match goes through
//! the iTunes Search API on the books lookup's client and limiter (the
//! same host and the same limit of about twenty requests a minute): one
//! search for the artists of the name, then each namesake's album list.
//! The photo is read from the matched artist's `music.apple.com` page,
//! the one exception to "no scraping" (DESIGN.md "Artist photos"): one
//! page every two to four seconds with random spacing. The image itself
//! comes from Apple's image host.
//!
//! Nothing here tries a refused request again: a 403 or a 429 comes back
//! as rate limited at once, and the caller stops the photo work for the
//! session. The Add dialog's lookups keep their own back-off.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, int_at, objects_at, str_at};
use super::store::JsonFile;
use super::{Failure, FailureKind};
use crate::covers::artists::{self, Candidate, Reason};

type Reply<T> = Result<T, Failure>;

const ITUNES: &str = "https://itunes.apple.com";
/// The namesakes whose album lists are asked for, at most.
const NAMESAKES: usize = 10;
/// The least time between two artist pages; up to `PAGE_JITTER` more is
/// added at random.
const PAGE_GAP: Duration = Duration::from_secs(2);
const PAGE_JITTER_MS: u64 = 2000;

/// Set when the window closes: a search under way stops before its next
/// request, so the exit never waits for it.
static CANCELLED: AtomicBool = AtomicBool::new(false);

pub fn cancel() {
    CANCELLED.store(true, Ordering::Relaxed);
}

/// What asking Apple about an artist came to. Only these are real
/// answers; a failure (offline, refused, a server error) is neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// The photo as Apple serves it: a 640 × 640 JPEG.
    Photo(Vec<u8>),
    Missing(Reason),
}

/// An Apple artist of the name: its id and its page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namesake {
    pub id: String,
    pub name: String,
    pub page: String,
}

/// Another Apple artist of the name, for the artist's Pick a cover: its
/// page, and one of its albums to tell it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Other {
    pub id: String,
    pub page: String,
    pub album: Option<String>,
}

/// What Pick a cover offers from Apple for an artist: the matched
/// artist's photo, and the other artists of the name, whose pages are
/// read only when asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choices {
    pub suggested: Option<String>,
    pub others: Vec<Other>,
}

/// What was asked before, kept in `apple_artists.json` under the cache
/// folder, so Pick a cover reads no page twice: the namesakes with their
/// albums by name key, and each artist's photo URL (none for a page
/// without one) by id.
#[derive(Debug, Default)]
struct Kept {
    names: HashMap<String, Vec<(Candidate, String)>>,
    photos: HashMap<String, Option<String>>,
}

const KEPT_VERSION: i64 = 1;

impl Kept {
    fn read(json: &Value) -> Kept {
        let mut kept = Kept::default();
        if json["version"].as_i64() != Some(KEPT_VERSION) {
            return kept;
        }
        for (key, list) in json["names"].as_object().into_iter().flatten() {
            let list = list
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|item| {
                    Some((
                        Candidate {
                            id: str_at(item, "id")?.to_owned(),
                            name: str_at(item, "name")?.to_owned(),
                            albums: http::strings_at(item, "albums")
                                .map(str::to_owned)
                                .collect(),
                        },
                        str_at(item, "page")?.to_owned(),
                    ))
                })
                .collect();
            kept.names.insert(key.clone(), list);
        }
        for (id, url) in json["photos"].as_object().into_iter().flatten() {
            kept.photos
                .insert(id.clone(), url.as_str().map(str::to_owned));
        }
        kept
    }

    fn to_json(&self) -> Value {
        let names: serde_json::Map<String, Value> = self
            .names
            .iter()
            .map(|(key, list)| {
                let list: Vec<Value> = list
                    .iter()
                    .map(|(c, page)| {
                        serde_json::json!({
                            "id": c.id, "name": c.name, "page": page, "albums": c.albums,
                        })
                    })
                    .collect();
                (key.clone(), Value::Array(list))
            })
            .collect();
        let photos: serde_json::Map<String, Value> = self
            .photos
            .iter()
            .map(|(id, url)| (id.clone(), serde_json::json!(url)))
            .collect();
        serde_json::json!({"version": KEPT_VERSION, "names": names, "photos": photos})
    }
}

pub struct AppleArtists {
    file: Option<JsonFile>,
    kept: Mutex<Kept>,
    api: Arc<Http>,
    pages: Http,
    images: Http,
    /// Held while a lookup in the Add dialog runs: the next request here
    /// waits, so the dialog has Apple's limit to itself.
    hold: AtomicBool,
}

impl std::fmt::Debug for AppleArtists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppleArtists").finish_non_exhaustive()
    }
}

impl Default for AppleArtists {
    fn default() -> AppleArtists {
        AppleArtists::new()
    }
}

impl AppleArtists {
    pub fn new() -> AppleArtists {
        AppleArtists::at(None)
    }

    /// With `apple_artists.json` in `cache`, the cache folder.
    pub fn at(cache: Option<&Path>) -> AppleArtists {
        let file = cache.map(|cache| JsonFile(cache.join("apple_artists.json")));
        let kept = file
            .as_ref()
            .and_then(JsonFile::read)
            .map(|json| Kept::read(&json))
            .unwrap_or_default();
        AppleArtists {
            file,
            kept: Mutex::new(kept),
            api: super::books::apple_http(),
            pages: Http::with_rate_limit_statuses(PAGE_GAP, &[429, 403]),
            images: Http::new(Duration::from_millis(200)),
            hold: AtomicBool::new(false),
        }
    }

    /// Holds the photo work back, or lets it go on.
    pub fn hold(&self, held: bool) {
        self.hold.store(held, Ordering::Relaxed);
    }

    /// Waits while the work is held; an error once the app is closing.
    fn ready(&self) -> Reply<()> {
        loop {
            if CANCELLED.load(Ordering::Relaxed) {
                return Err(Failure::other("the app is closing"));
            }
            if !self.hold.load(Ordering::Relaxed) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// One request to the Search API, never sent twice.
    fn api(&self, path: &str, query: &[(&str, &str)]) -> Reply<Value> {
        self.ready()?;
        let url = http::url(&format!("{ITUNES}{path}"), query);
        let response = self.api.get_once(&url, &[("Accept", "application/json")])?;
        self.api.check(&response, &[200])?;
        response.object()
    }

    /// The Apple artists whose name is `name` by `artists::same_name`.
    pub fn namesakes(&self, name: &str) -> Reply<Vec<Namesake>> {
        let json = self.api(
            "/search",
            &[
                ("term", name),
                ("media", "music"),
                ("entity", "musicArtist"),
                ("attribute", "artistTerm"),
                ("limit", "50"),
            ],
        )?;
        Ok(namesakes_in(&json, name))
    }

    /// The titles of the artist's albums.
    pub fn albums(&self, id: &str) -> Reply<Vec<String>> {
        let json = self.api(
            "/lookup",
            &[("id", id), ("entity", "album"), ("limit", "200")],
        )?;
        Ok(albums_in(&json))
    }

    fn keep(&self, change: impl FnOnce(&mut Kept)) {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut kept);
        if let Some(file) = &self.file {
            let _ = file.write(&kept.to_json());
        }
    }

    /// `candidates` as kept from an earlier ask, else asked now.
    fn candidates_kept(&self, name: &str) -> Reply<Vec<(Candidate, String)>> {
        let key = artists::name_key(name);
        let known = {
            let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
            kept.names.get(&key).cloned()
        };
        match known {
            Some(list) => Ok(list),
            None => self.candidates(name),
        }
    }

    /// The photo on the page of the artist `id`, read once and kept.
    pub fn photo_of(&self, id: &str, page: &str) -> Reply<Option<String>> {
        let known = {
            let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
            kept.photos.get(id).cloned()
        };
        if let Some(url) = known {
            return Ok(url);
        }
        let url = self.photo_on(page)?;
        self.keep(|kept| {
            kept.photos.insert(id.to_owned(), url.clone());
        });
        Ok(url)
    }

    /// What the artist's Pick a cover offers from Apple: the matched
    /// artist's photo, and the other artists of the name with one album
    /// each, their pages unread.
    pub fn choices(&self, name: &str, owned: &[String]) -> Reply<Choices> {
        let owned: Vec<&str> = owned.iter().map(String::as_str).collect();
        let listed = self.candidates_kept(name)?;
        let candidates: Vec<Candidate> = listed.iter().map(|(c, _)| c.clone()).collect();
        let chosen = artists::choose(name, &owned, &candidates).map(|c| c.id.clone());
        let mut choices = Choices::default();
        for (candidate, page) in &listed {
            if Some(&candidate.id) == chosen.as_ref() {
                choices.suggested = self.photo_of(&candidate.id, page)?;
            } else {
                choices.others.push(Other {
                    id: candidate.id.clone(),
                    page: page.clone(),
                    album: candidate.albums.first().cloned(),
                });
            }
        }
        Ok(choices)
    }

    /// The namesakes with their album lists, for `artists::choose`.
    pub fn candidates(&self, name: &str) -> Reply<Vec<(Candidate, String)>> {
        let mut out = Vec::new();
        for namesake in self.namesakes(name)?.into_iter().take(NAMESAKES) {
            let albums = self.albums(&namesake.id)?;
            out.push((
                Candidate {
                    id: namesake.id,
                    name: namesake.name,
                    albums,
                },
                namesake.page,
            ));
        }
        self.keep(|kept| {
            kept.names.insert(artists::name_key(name), out.clone());
        });
        Ok(out)
    }

    /// The URL of the photo on the artist's page, or `None` when the
    /// page shows none (an album cover, another kind of image, no page).
    pub fn photo_on(&self, page: &str) -> Reply<Option<String>> {
        self.ready()?;
        std::thread::sleep(Duration::from_millis(jitter(PAGE_JITTER_MS)));
        let response = self.pages.get_once(page, &[("Accept", "text/html")])?;
        if response.status == 404 {
            return Ok(None);
        }
        self.pages.check(&response, &[200])?;
        let html = String::from_utf8_lossy(&response.bytes);
        Ok(artists::og_image(&html).and_then(|image| artists::photo_url(&image)))
    }

    /// The image at `url`, which must be a JPEG.
    pub fn image(&self, url: &str) -> Reply<Vec<u8>> {
        self.ready()?;
        let response = self.images.get_once(url, &[])?;
        self.images.check(&response, &[200])?;
        if !response.bytes.starts_with(&[0xFF, 0xD8]) {
            return Err(Failure::other("the photo is not a JPEG"));
        }
        Ok(response.bytes)
    }

    /// Asks Apple about the library's artist `name`, whose saved album
    /// titles are `owned`: the match, its page, the photo.
    pub fn find(&self, name: &str, owned: &[String]) -> Reply<Found> {
        let owned: Vec<&str> = owned.iter().map(String::as_str).collect();
        let listed = self.candidates(name)?;
        let candidates: Vec<Candidate> = listed.iter().map(|(c, _)| c.clone()).collect();
        let Some(chosen) = artists::choose(name, &owned, &candidates) else {
            return Ok(Found::Missing(Reason::NoMatch));
        };
        let page = listed
            .iter()
            .find(|(candidate, _)| candidate.id == chosen.id)
            .map(|(_, page)| page.as_str())
            .unwrap_or_default();
        let photo = self.photo_on(page)?;
        self.keep(|kept| {
            kept.photos.insert(chosen.id.clone(), photo.clone());
        });
        match photo {
            Some(url) => Ok(Found::Photo(self.image(&url)?)),
            None => Ok(Found::Missing(Reason::NoPhoto)),
        }
    }
}

/// Whether a failure is Apple refusing (a 403 or a 429), which stops the
/// photo work for the session.
pub fn refused(failure: &Failure) -> bool {
    failure.kind == FailureKind::RateLimited
}

/// A number below `below` that differs from call to call, from the
/// clock: enough to space the pages unevenly.
fn jitter(below: u64) -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| u64::from(since.subsec_nanos()));
    (nanos / 1000) % below.max(1)
}

/// The artists of a search answer whose name is `name`.
pub fn namesakes_in(json: &Value, name: &str) -> Vec<Namesake> {
    objects_at(json, "results")
        .filter_map(|item| {
            let found = str_at(item, "artistName")?;
            let id = int_at(item, "artistId")?.to_string();
            if !artists::same_name(found, name) {
                return None;
            }
            // The link carries a tracking query the page does not need.
            let page = str_at(item, "artistLinkUrl")
                .map(|link| link.split('?').next().unwrap_or(link).to_owned())
                .filter(|link| link.starts_with("https://music.apple.com/"))
                .unwrap_or_else(|| format!("https://music.apple.com/us/artist/{id}"));
            Some(Namesake {
                id,
                name: found.to_owned(),
                page,
            })
        })
        .collect()
}

/// The album titles of a lookup answer (its first row is the artist).
pub fn albums_in(json: &Value) -> Vec<String> {
    objects_at(json, "results")
        .filter(|item| str_at(item, "wrapperType") == Some("collection"))
        .filter_map(|item| str_at(item, "collectionName").map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_search_gives_the_artists_of_that_name_with_their_pages() {
        let answer = json!({"resultCount": 3, "results": [
            {"wrapperType": "artist", "artistName": "Adele", "artistId": 262836961,
             "artistLinkUrl": "https://music.apple.com/us/artist/adele/262836961?uo=4"},
            {"wrapperType": "artist", "artistName": "Adele Roberts", "artistId": 7,
             "artistLinkUrl": "https://music.apple.com/us/artist/adele-roberts/7?uo=4"},
            {"wrapperType": "artist", "artistName": "ADELE", "artistId": 99},
        ]});
        let found = namesakes_in(&answer, "adele");
        assert_eq!(
            found,
            [
                Namesake {
                    id: "262836961".into(),
                    name: "Adele".into(),
                    page: "https://music.apple.com/us/artist/adele/262836961".into(),
                },
                Namesake {
                    id: "99".into(),
                    name: "ADELE".into(),
                    page: "https://music.apple.com/us/artist/99".into(),
                },
            ]
        );
    }

    #[test]
    fn a_lookup_gives_the_album_titles_without_the_artist_row() {
        let answer = json!({"results": [
            {"wrapperType": "artist", "artistName": "Adele"},
            {"wrapperType": "collection", "collectionName": "21"},
            {"wrapperType": "collection", "collectionName": "25"},
            {"wrapperType": "track", "trackName": "Hello"},
        ]});
        assert_eq!(albums_in(&answer), ["21", "25"]);
    }

    #[test]
    fn what_was_asked_is_kept_and_read_back() {
        let mut kept = Kept::default();
        kept.names.insert(
            "adele".into(),
            vec![(
                Candidate {
                    id: "1".into(),
                    name: "Adele".into(),
                    albums: vec!["21".into(), "25".into()],
                },
                "https://music.apple.com/us/artist/adele/1".into(),
            )],
        );
        kept.photos
            .insert("1".into(), Some("https://x/640x640cc.jpg".into()));
        kept.photos.insert("2".into(), None);
        let again = Kept::read(&kept.to_json());
        assert_eq!(again.names, kept.names);
        assert_eq!(again.photos, kept.photos);
        assert!(Kept::read(&json!({"version": 9})).names.is_empty());
    }

    #[test]
    fn the_spacing_stays_below_its_bound() {
        assert!(jitter(2000) < 2000);
        assert_eq!(jitter(0), 0);
    }
}
