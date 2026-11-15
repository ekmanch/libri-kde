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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, int_at, objects_at, str_at};
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

pub struct AppleArtists {
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
        AppleArtists {
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
        match self.photo_on(page)? {
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
    fn the_spacing_stays_below_its_bound() {
        assert!(jitter(2000) < 2000);
        assert_eq!(jitter(0), 0);
    }
}
