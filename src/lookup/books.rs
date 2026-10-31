//! Books, from Open Library and Apple Books together
//! (`open_library_source.dart`, `itunes_books.dart`, `google_books.dart`).
//! Open Library issues the ids, "openlibrary:OL45804W" (the work), and
//! gives the year (the first publication's), the author, the subjects and
//! the page count; Apple is asked in parallel through the iTunes Search
//! API (no key): it finds the books Open Library does not have yet, which
//! keep its id, "itunes:6756269989", and gives every book its description
//! and cover, preferred to Open Library's. A book both have is one row
//! with Open Library's id. Google Books is read by id only, for the
//! "googlebooks:" ids stored while it was a search source. One source
//! failing never fails a lookup the other answered. A book's series comes
//! from Wikidata by title and author, else from a volume number in the
//! title; the volumes of such a series are then Open Library's works of
//! that name by that author.

use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, int_at, objects_at, str_at, strings_at};
use super::text::{
    MAX_BOOK_TAGS, core_title, edition_label, first_publish_year, is_tag_like, same_book,
    same_title, share_author, strip_html, title_key, volume_of,
};
use super::wikidata::Wikidata;
use super::{Failure, FailureKind, Hits, Match, Reply, SeriesPart, SeriesParts, Source};
use crate::model::{Date, MediaType, Released, Series};

const OPEN_LIBRARY: &str = "https://openlibrary.org";
const COVERS: &str = "https://covers.openlibrary.org";
const ITUNES: &str = "https://itunes.apple.com";
const GOOGLE: &str = "https://www.googleapis.com/books/v1/volumes";

/// `editions.cover_i` is the cover of the edition that best matches the
/// query, an English one when there is one; the work's own `cover_i` can
/// be any edition's.
const FIELDS: &str = "key,title,author_name,first_publish_year,publish_year,cover_i,subject,\
    number_of_pages_median,language,editions,editions.cover_i,editions.key,editions.format,\
    editions.publish_date";
/// How many works are read when listing a series' volumes by name.
const VOLUMES_LIMIT: usize = 100;
/// Apple's artwork sizes: a row's, and the cache's.
const ROW_ARTWORK: u32 = 200;
const LARGE_ARTWORK: u32 = 600;

/// Open Library's search index and work records.
pub struct OpenLibrary {
    http: Http,
}

impl OpenLibrary {
    fn new() -> OpenLibrary {
        OpenLibrary {
            http: Http::new(Duration::from_millis(500)),
        }
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> Reply<Value> {
        let url = http::url(&format!("{OPEN_LIBRARY}{path}"), query);
        self.http
            .get_object(&url, &[("Accept", "application/json")])
    }

    /// "-M" for a row (180 px wide), "-L" for the cache (500 px wide).
    fn cover(cover_id: i64, size: &str) -> String {
        format!("{COVERS}/b/id/{cover_id}{size}.jpg")
    }

    pub fn hit(doc: &Value) -> Option<Match> {
        let key = str_at(doc, "key")?;
        let title = str_at(doc, "title")?;
        let work_id = key.rsplit('/').next().unwrap_or(key);
        let authors: Vec<&str> = strings_at(doc, "author_name").collect();
        let years = doc
            .get("publish_year")
            .and_then(Value::as_array)
            .map(|ys| {
                ys.iter()
                    .filter_map(Value::as_i64)
                    .filter_map(|y| u16::try_from(y).ok())
            })
            .map(|ys| first_publish_year(ys, 10))
            .unwrap_or(None)
            .or_else(|| int_at(doc, "first_publish_year").and_then(|y| u16::try_from(y).ok()));
        // A work's languages are its editions' (translations included), so
        // only a work in one language says which it was written in.
        let languages: Vec<&str> = strings_at(doc, "language").collect();
        let edition = objects_at(&doc["editions"], "docs").next();
        let edition_cover = edition
            .and_then(|e| int_at(e, "cover_i"))
            .filter(|c| *c > 0);
        let from_edition = edition_cover.is_some();
        let cover_id = edition_cover.or_else(|| int_at(doc, "cover_i").filter(|c| *c > 0));
        let edition_id = from_edition
            .then(|| edition.and_then(|e| str_at(e, "key")))
            .flatten()
            .map(|k| k.rsplit('/').next().unwrap_or(k).to_owned());
        let edition_date = from_edition
            .then(|| edition.and_then(|e| strings_at(e, "publish_date").next()))
            .flatten();
        let edition_year = edition_date.and_then(|d| {
            d.split(|c: char| !c.is_ascii_digit())
                .find(|p| p.len() == 4)
                .and_then(|y| y.parse::<u16>().ok())
        });
        let format = from_edition
            .then(|| edition.and_then(|e| strings_at(e, "format").next()))
            .flatten();
        Some(Match {
            released: years.map(|year| Released {
                year,
                month: None,
                day: None,
            }),
            creator: (!authors.is_empty()).then(|| authors.join(", ")),
            tags: strings_at(doc, "subject")
                .filter(|s| is_tag_like(s))
                .take(MAX_BOOK_TAGS)
                .map(str::to_owned)
                .collect(),
            pages: int_at(doc, "number_of_pages_median")
                .filter(|p| *p > 0)
                .and_then(|p| u32::try_from(p).ok()),
            language: match languages.as_slice() {
                [only] => Some((*only).to_owned()),
                _ => None,
            },
            cover_url: cover_id.map(|c| OpenLibrary::cover(c, "-M")),
            large_cover_url: cover_id.map(|c| OpenLibrary::cover(c, "-L")),
            cover_source: edition_id.map(|id| format!("openlibrary:{id}")),
            edition: from_edition
                .then(|| edition_label(title, format, None))
                .flatten(),
            edition_year,
            ..Match::new(MediaType::Books, title, format!("openlibrary:{work_id}"))
        })
    }

    fn search(&self, query: &[(&str, &str)], limit: usize) -> Reply<Vec<Match>> {
        let limit = limit.to_string();
        let mut params: Vec<(&str, &str)> = query.to_vec();
        params.extend([
            ("fields", FIELDS),
            ("lang", "en"),
            ("limit", limit.as_str()),
        ]);
        let json = self.get("/search.json", &params)?;
        Ok(objects_at(&json, "docs")
            .filter_map(OpenLibrary::hit)
            .collect())
    }

    fn search_title(&self, title: &str, author: Option<&str>, limit: usize) -> Reply<Vec<Match>> {
        match author {
            Some(author) => self.search(&[("title", title), ("author", author)], limit),
            None => self.search(&[("title", title)], limit),
        }
    }

    /// The work record: its description and its first cover id.
    fn work(&self, source: &str) -> Reply<Work> {
        let json = self.get(&format!("/works/{}.json", work_id(source)?), &[])?;
        let description = match &json["description"] {
            Value::String(text) => Some(text.clone()),
            value if value.is_object() => str_at(value, "value").map(str::to_owned),
            _ => None,
        };
        Ok(Work {
            description: description.map(|d| strip_html(&d)),
            cover_id: json
                .get("covers")
                .and_then(Value::as_array)
                .and_then(|cs| cs.iter().filter_map(Value::as_i64).find(|c| *c > 0)),
            title: str_at(&json, "title").map(str::to_owned),
        })
    }

    /// `hit` with its work's description and cover where it lacks them.
    fn with_work(&self, mut hit: Match) -> Reply<Match> {
        let work = self.work(&hit.source)?;
        if hit.description.is_empty()
            && let Some(description) = work.description
        {
            hit.description = description;
        }
        if hit.cover_url.is_none()
            && let Some(cover_id) = work.cover_id
        {
            hit.cover_url = Some(OpenLibrary::cover(cover_id, "-M"));
            hit.large_cover_url = Some(OpenLibrary::cover(cover_id, "-L"));
        }
        Ok(hit)
    }

    /// The search index's own record of the work `source`.
    fn by_key(&self, source: &str) -> Reply<Option<Match>> {
        let key = format!("key:/works/{}", work_id(source)?);
        Ok(self
            .search(&[("q", &key)], 1)?
            .into_iter()
            .find(|h| h.source == source))
    }

    /// The works titled `<name>, Vol. N` by `author` in the index, one per
    /// number, in number order.
    fn volumes(&self, name: &str, author: Option<&str>) -> Reply<Option<SeriesParts>> {
        let hits = self.search_title(name, author, VOLUMES_LIMIT)?;
        let mut by_number: Vec<SeriesPart> = Vec::new();
        for hit in hits {
            let Some((volume_name, number)) = volume_of(&hit.title) else {
                continue;
            };
            if !same_title(&volume_name, name) {
                continue;
            }
            let year = hit.year();
            match by_number.iter_mut().find(|p| p.number == Some(number)) {
                None => by_number.push(SeriesPart {
                    number: Some(number),
                    title: hit.title.clone(),
                    year,
                    released: hit.released,
                    source: Some(hit.source.clone()),
                    creator: None,
                    cover: hit.cache_cover_url().map(str::to_owned),
                    extra: false,
                }),
                Some(known) => {
                    if let Some(year) = year
                        && known.year.is_none_or(|k| year < k)
                    {
                        known.year = Some(year);
                        known.released = hit.released;
                    }
                }
            }
        }
        if by_number.is_empty() {
            return Ok(None);
        }
        by_number.sort_by_key(|p| p.number);
        Ok(Some(SeriesParts {
            name: name.to_owned(),
            parts: by_number,
        }))
    }
}

struct Work {
    description: Option<String>,
    cover_id: Option<i64>,
    title: Option<String>,
}

fn work_id(source: &str) -> Reply<&str> {
    source
        .strip_prefix("openlibrary:")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Failure::other(format!("not an Open Library id: {source}")))
}

/// Apple Books through the iTunes Search API. Apple allows about twenty
/// requests a minute per address, so requests keep three seconds apart and
/// a 403 counts as rate limited.
pub struct ITunes {
    http: Http,
}

impl ITunes {
    fn new() -> ITunes {
        ITunes {
            http: Http::with_rate_limit_statuses(Duration::from_secs(3), &[429, 403]),
        }
    }

    fn results(&self, path: &str, query: &[(&str, &str)]) -> Reply<Vec<Match>> {
        let url = http::url(&format!("{ITUNES}{path}"), query);
        let response = self.http.get(&url, &[("Accept", "application/json")])?;
        if response.status == 403 {
            return Err(Failure {
                kind: FailureKind::RateLimited,
                status: Some(403),
                message: "Apple refused the request (its rate limit)".into(),
            });
        }
        self.http.check(&response, &[200])?;
        let json = response.object()?;
        Ok(objects_at(&json, "results")
            .filter_map(ITunes::hit)
            .collect())
    }

    /// One result as a match. The genres list "Books" among them, which no
    /// book needs as a tag.
    pub fn hit(item: &Value) -> Option<Match> {
        let track_id = int_at(item, "trackId")?;
        let title = str_at(item, "trackName")?;
        let artwork = str_at(item, "artworkUrl100");
        let released = str_at(item, "releaseDate")
            .and_then(|d| d.split('T').next())
            .and_then(Released::parse);
        Some(Match {
            released,
            ebook_dated: released.is_some(),
            creator: str_at(item, "artistName").map(str::to_owned),
            description: str_at(item, "description")
                .map(strip_html)
                .unwrap_or_default(),
            tags: strings_at(item, "genres")
                .filter(|g| *g != "Books" && is_tag_like(g))
                .take(MAX_BOOK_TAGS)
                .map(str::to_owned)
                .collect(),
            cover_url: artwork.map(|a| ITunes::artwork_at(a, ROW_ARTWORK)),
            large_cover_url: artwork.map(|a| ITunes::artwork_at(a, LARGE_ARTWORK)),
            ..Match::new(MediaType::Books, title, format!("itunes:{track_id}"))
        })
    }

    /// `url` ("…/100x100bb.jpg") at `px` pixels square: Apple's image
    /// server renders any size on request. A URL of another shape is left
    /// as it is.
    pub fn artwork_at(url: &str, px: u32) -> String {
        let Some((head, tail)) = url.rsplit_once('/') else {
            return url.to_owned();
        };
        let Some((file, ext)) = tail.rsplit_once('.') else {
            return url.to_owned();
        };
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let size_end = file
            .find(|c: char| c.is_ascii_alphabetic() && c != 'x')
            .unwrap_or(file.len());
        let (size, suffix) = file.split_at(size_end);
        let Some((w, h)) = size.split_once('x') else {
            return url.to_owned();
        };
        if !digits(w) || !digits(h) || !ext.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return url.to_owned();
        }
        format!("{head}/{px}x{px}{suffix}.{ext}")
    }

    /// Apple lists one book per edition and language; only the first of
    /// the same title by the same author is kept.
    fn search(&self, query: &str, limit: usize) -> Reply<Vec<Match>> {
        let limit = limit.clamp(1, 200).to_string();
        let hits = self.results(
            "/search",
            &[
                ("term", query),
                ("entity", "ebook"),
                ("media", "ebook"),
                ("limit", &limit),
            ],
        )?;
        let mut out: Vec<Match> = Vec::new();
        for hit in hits {
            if !out.iter().any(|o| same_book(o, &hit)) {
                out.push(hit);
            }
        }
        Ok(out)
    }

    fn term(title: &str, author: Option<&str>) -> String {
        match author.filter(|a| !a.is_empty()) {
            Some(author) => format!("{title} {author}"),
            None => title.to_owned(),
        }
    }

    fn search_title(&self, title: &str, author: Option<&str>, limit: usize) -> Reply<Vec<Match>> {
        self.search(&ITunes::term(title, author), limit)
    }

    /// Every ebook for `title`, editions and languages each on their own,
    /// for Pick a cover.
    fn editions(&self, title: &str, author: Option<&str>, limit: usize) -> Reply<Vec<Match>> {
        let limit = limit.clamp(1, 200).to_string();
        self.results(
            "/search",
            &[
                ("term", &ITunes::term(title, author)),
                ("entity", "ebook"),
                ("media", "ebook"),
                ("limit", &limit),
            ],
        )
    }

    /// The book's own record, or `None` when Apple has no book with that id.
    fn book(&self, source: &str) -> Reply<Option<Match>> {
        let id = source
            .strip_prefix("itunes:")
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Failure::other(format!("not an Apple Books id: {source}")))?;
        Ok(self
            .results("/lookup", &[("id", id), ("entity", "ebook")])?
            .into_iter()
            .next())
    }
}

/// Google Books, read by id only.
pub struct Google {
    key: String,
    http: Http,
}

impl Google {
    fn new(key: &str) -> Google {
        Google {
            key: key.trim().to_owned(),
            http: Http::new(Duration::from_millis(500)),
        }
    }

    fn volume(&self, source: &str) -> Reply<Option<Match>> {
        let id = source
            .strip_prefix("googlebooks:")
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Failure::other(format!("not a Google Books id: {source}")))?;
        let base = format!("{GOOGLE}/{}", http::encode(id));
        let url = if self.key.is_empty() {
            base
        } else {
            http::url(&base, &[("key", &self.key)])
        };
        let json = self
            .http
            .get_object(&url, &[("Accept", "application/json")])?;
        Ok(Google::hit(&json))
    }

    /// One volume as a match: the thumbnail for a row, and the cache's
    /// from "medium" down. Google's thumbnails come over http with a
    /// page-curl effect baked in.
    pub fn hit(item: &Value) -> Option<Match> {
        let id = str_at(item, "id")?;
        let info = &item["volumeInfo"];
        let title = str_at(info, "title")?;
        let links = &info["imageLinks"];
        let clean = |url: &str| {
            let https = url
                .strip_prefix("http://")
                .map_or(url.to_owned(), |rest| format!("https://{rest}"));
            https.replace("?edge=curl&", "?").replace("&edge=curl", "")
        };
        let authors: Vec<&str> = strings_at(info, "authors")
            .filter(|a| !a.is_empty())
            .collect();
        let mut tags: Vec<String> = Vec::new();
        for category in strings_at(info, "categories") {
            for part in category.split('/').map(str::trim) {
                if !part.eq_ignore_ascii_case("general")
                    && is_tag_like(part)
                    && !tags.iter().any(|t| t == part)
                {
                    tags.push(part.to_owned());
                }
            }
        }
        tags.truncate(MAX_BOOK_TAGS);
        Some(Match {
            released: str_at(info, "publishedDate")
                .and_then(|d| d.split('T').next())
                .and_then(Released::parse),
            creator: (!authors.is_empty()).then(|| authors.join(", ")),
            description: str_at(info, "description")
                .map(strip_html)
                .unwrap_or_default(),
            tags,
            pages: int_at(info, "pageCount")
                .filter(|p| *p > 0)
                .and_then(|p| u32::try_from(p).ok()),
            cover_url: str_at(links, "thumbnail").map(clean),
            large_cover_url: str_at(links, "medium")
                .or_else(|| str_at(links, "small"))
                .or_else(|| str_at(links, "large"))
                .map(clean),
            ..Match::new(MediaType::Books, title, format!("googlebooks:{id}"))
        })
    }
}

/// The books source: Open Library and Apple Books together, Google Books
/// by id, Wikidata for the series.
pub struct Books {
    open_library: OpenLibrary,
    apple: ITunes,
    google: Google,
    wikidata: Wikidata,
}

impl Books {
    pub fn new(google_key: &str) -> Books {
        Books {
            open_library: OpenLibrary::new(),
            apple: ITunes::new(),
            google: Google::new(google_key),
            wikidata: Wikidata::new(),
        }
    }

    fn is_apple(source: &str) -> bool {
        Match::provider_of(source) == "itunes"
    }

    fn is_google(source: &str) -> bool {
        Match::provider_of(source) == "googlebooks"
    }

    /// One list from both sources' hits: each Open Library hit keeps its
    /// place and takes Apple's record of the same book; Apple's other hits
    /// take turns with Open Library's, so a long Open Library list cannot
    /// push out the book only Apple has. At most `limit`.
    pub fn merge_hits(open_library: Vec<Match>, apple: Vec<Match>, limit: usize) -> Vec<Match> {
        let mut ol = open_library;
        let mut only_apple: Vec<Match> = Vec::new();
        for hit in apple {
            if let Some(known) = ol.iter_mut().find(|o| same_book(o, &hit)) {
                *known = Books::with_apple(known.clone(), Some(&hit));
            } else if !only_apple.iter().any(|o| same_book(o, &hit)) {
                only_apple.push(hit);
            }
        }
        let mut out = Vec::new();
        let mut ol = ol.into_iter();
        let mut ab = only_apple.into_iter();
        loop {
            let (a, b) = (ol.next(), ab.next());
            if a.is_none() && b.is_none() {
                break;
            }
            out.extend(a);
            out.extend(b);
        }
        out.truncate(limit);
        out
    }

    /// `found` with Apple's record of the same book: Apple's description
    /// and cover, when it has them, replace what `found` has; its year,
    /// author and genres fill gaps only.
    pub fn with_apple(mut found: Match, apple: Option<&Match>) -> Match {
        let Some(apple) = apple else {
            return found;
        };
        if found.released.is_none() {
            found.released = apple.released;
            found.ebook_dated = apple.ebook_dated;
        }
        if found.creator.is_none() {
            found.creator = apple.creator.clone();
        }
        if !apple.description.is_empty() {
            found.description = apple.description.clone();
        }
        if found.tags.is_empty() && !apple.tags.is_empty() {
            found.tags = apple.tags.clone();
        }
        if found.pages.is_none() {
            found.pages = apple.pages;
        }
        if apple.cover_url.is_some() {
            found.cover_url = apple.cover_url.clone();
            found.large_cover_url = apple.large_cover_url.clone().or(apple.cover_url.clone());
            found.cover_source = Some(apple.source.clone());
            found.edition = apple.edition.clone();
        }
        found
    }

    /// `found` (an Apple or Google hit) with Open Library's record of the
    /// same book: Open Library's year, author and page count, when it has
    /// them, replace what `found` has; its description, cover and subjects
    /// fill gaps only.
    pub fn with_open_library(mut found: Match, ol: Option<&Match>) -> Match {
        let Some(ol) = ol else {
            return found;
        };
        if ol.released.is_some() {
            found.released = ol.released;
            found.ebook_dated = false;
        }
        if ol.creator.is_some() {
            found.creator = ol.creator.clone();
        }
        if ol.pages.is_some() {
            found.pages = ol.pages;
        }
        if found.description.is_empty() && !ol.description.is_empty() {
            found.description = ol.description.clone();
        }
        if found.tags.is_empty() && !ol.tags.is_empty() {
            found.tags = ol.tags.clone();
        }
        if found.cover_url.is_none() {
            found.cover_url = ol.cover_url.clone();
            if found.large_cover_url.is_none() {
                found.large_cover_url = ol.large_cover_url.clone();
            }
            if ol.cover_url.is_some() {
                found.cover_source = Some(ol.cover_from().to_owned());
            }
        }
        found
    }

    /// `found` with the fields it lacks taken from `other`.
    fn fill_gaps(mut found: Match, other: Option<&Match>) -> Match {
        let Some(other) = other else {
            return found;
        };
        if found.released.is_none() {
            found.released = other.released;
            found.ebook_dated = other.ebook_dated;
        }
        if found.creator.is_none() {
            found.creator = other.creator.clone();
        }
        if found.description.is_empty() {
            found.description = other.description.clone();
        }
        if found.tags.is_empty() {
            found.tags = other.tags.clone();
        }
        if found.pages.is_none() {
            found.pages = other.pages;
        }
        if found.cover_url.is_none() {
            found.cover_url = other.cover_url.clone();
            if found.large_cover_url.is_none() {
                found.large_cover_url = other.large_cover_url.clone();
            }
            if other.cover_url.is_some() {
                found.cover_source = Some(other.cover_from().to_owned());
            }
        }
        found
    }

    /// The first of `hits` that is the book `title` by `author` (the same
    /// title alone when no author is known).
    fn the_book<'a>(hits: &'a [Match], title: &str, author: Option<&str>) -> Option<&'a Match> {
        hits.iter().find(|h| match author {
            None => same_title(&h.title, title),
            Some(author) => {
                same_title(&h.title, title) && share_author(h.creator.as_deref(), Some(author))
            }
        })
    }

    /// Apple's record of the book `title` (by `author`, when known); `None`
    /// when Apple has none or does not answer.
    fn apple_fill(&self, title: &str, author: Option<&str>) -> Option<Match> {
        let hits = self.apple.search_title(title, author, 5).ok()?;
        Books::the_book(&hits, title, author).cloned()
    }

    /// Open Library's record of the book `title` (by `author`, when known),
    /// with its work's description and cover when `with_work`; `None` when
    /// it has none or does not answer.
    fn open_library_fill(
        &self,
        title: &str,
        author: Option<&str>,
        with_work: bool,
    ) -> Option<Match> {
        let hits = self.open_library.search_title(title, author, 5).ok()?;
        let hit = Books::the_book(&hits, title, author)?.clone();
        if !with_work {
            return Some(hit);
        }
        self.open_library.with_work(hit).ok()
    }

    fn pick(hits: &[Match], title: &str, year: Option<u16>, author: Option<&str>) -> Option<Match> {
        let same: Vec<&Match> = hits
            .iter()
            .filter(|h| same_title(&h.title, title))
            .collect();
        if let Some(year) = year
            && let Some(hit) = same.iter().find(|h| h.year() == Some(year))
        {
            return Some((*hit).clone());
        }
        // The same title by the same author is the same book whatever year
        // the library recorded; without an author, only an exact year is
        // sure.
        if (author.is_some() || year.is_none()) && !same.is_empty() {
            return Some(same[0].clone());
        }
        None
    }

    /// Both searches at once, merged, with the failure of the one that did
    /// not answer. Fails when neither answered.
    fn both(
        &self,
        open_library: impl FnOnce() -> Reply<Vec<Match>> + Send,
        apple: impl FnOnce() -> Reply<Vec<Match>> + Send,
        limit: usize,
        mut early: Option<&mut dyn FnMut(Vec<Match>)>,
    ) -> Reply<Hits> {
        let (tx, rx) = std::sync::mpsc::channel::<(bool, Reply<Vec<Match>>)>();
        let (ol, ab) = std::thread::scope(|scope| {
            let tx_ol = tx.clone();
            scope.spawn(move || {
                let _ = tx_ol.send((false, open_library()));
            });
            scope.spawn(move || {
                let _ = tx.send((true, apple()));
            });
            let (mut ol, mut ab) = (None, None);
            for (is_apple, result) in rx {
                if is_apple {
                    // Apple's hits as soon as Apple answers before Open
                    // Library, which can take ten seconds or more.
                    if ol.is_none()
                        && let Ok(hits) = &result
                        && !hits.is_empty()
                        && let Some(early) = early.as_deref_mut()
                    {
                        early(Books::merge_hits(Vec::new(), hits.clone(), limit));
                    }
                    ab = Some(result);
                } else {
                    ol = Some(result);
                }
            }
            (ol, ab)
        });
        let ol = ol.unwrap_or_else(|| Err(Failure::other("no answer")));
        let ab = ab.unwrap_or_else(|| Err(Failure::other("no answer")));
        match (ol, ab) {
            (Ok(ol), Ok(ab)) => Ok(Hits::of(Books::merge_hits(ol, ab, limit))),
            (Ok(ol), Err(failure)) => Ok(Hits {
                matches: Books::merge_hits(ol, Vec::new(), limit),
                failure: Some(failure),
            }),
            (Err(failure), Ok(ab)) => Ok(Hits {
                matches: Books::merge_hits(Vec::new(), ab, limit),
                failure: Some(failure),
            }),
            (Err(failure), Err(_)) => Err(failure),
        }
    }

    fn book_by_id(&self, source: &str) -> Reply<Option<Match>> {
        if Books::is_apple(source) {
            self.apple.book(source)
        } else if Books::is_google(source) {
            self.google.volume(source)
        } else {
            self.open_library.by_key(source)
        }
    }

    /// The series of the book `title` by `author`: Wikidata's, else the one
    /// a volume number in the title names. Wikidata not answering is not a
    /// failure of the pick.
    fn series_of(&self, title: &str, author: Option<&str>) -> Option<Series> {
        if let Ok(Some((parts, number))) = self.wikidata.series_of(title, author) {
            return Some(Series {
                name: parts.name,
                number,
                cover: None,
            });
        }
        volume_of(title).map(|(name, number)| Series {
            name,
            number: Some(number),
            cover: None,
        })
    }

    /// `parts` with Apple's artwork as the cover of every part still to
    /// come, asked for by title and author: Apple lists a new title long
    /// before Open Library has a cover for it, and Wikidata's parts carry
    /// none.
    fn with_upcoming_covers(&self, mut parts: SeriesParts, author: Option<&str>) -> SeriesParts {
        let today = Date::today();
        for part in &mut parts.parts {
            let out = part.release().is_none_or(|r| r.is_released_on(today));
            if out {
                continue;
            }
            let apple = self.apple_fill(&part.title, part.creator.as_deref().or(author));
            if let Some(cover) = apple.as_ref().and_then(Match::cache_cover_url) {
                part.cover = Some(cover.to_owned());
            }
        }
        parts
    }
}

impl Source for Books {
    fn id(&self) -> &'static str {
        "openlibrary"
    }

    fn name(&self) -> &'static str {
        "Open Library"
    }

    fn types(&self) -> &'static [MediaType] {
        &[MediaType::Books]
    }

    fn issued(&self, source: &str) -> bool {
        matches!(
            Match::provider_of(source),
            "openlibrary" | "itunes" | "googlebooks"
        )
    }

    fn search(&self, query: &str, media: MediaType, limit: usize) -> Reply<Hits> {
        self.search_steps(query, media, limit, &mut |_| {})
    }

    fn search_steps(
        &self,
        query: &str,
        _media: MediaType,
        limit: usize,
        step: &mut dyn FnMut(Vec<Match>),
    ) -> Reply<Hits> {
        self.both(
            || self.open_library.search(&[("q", query)], limit),
            || self.apple.search(query, limit),
            limit,
            Some(step),
        )
    }

    /// An Apple hit shown before Open Library answered is, in the merged
    /// list, the Open Library row of the same book when there is one.
    fn same_match(&self, early: &Match, complete: &[Match]) -> Option<Match> {
        complete
            .iter()
            .find(|m| m.source == early.source)
            .or_else(|| complete.iter().find(|m| same_book(m, early)))
            .cloned()
    }

    /// No match while one source failed is not a miss: the failure is
    /// returned, so the cover is asked for again rather than given up on.
    fn match_title(
        &self,
        title: &str,
        _media: MediaType,
        year: Option<u16>,
        creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        let author = creator.filter(|c| !c.is_empty());
        let hits = self.both(
            || self.open_library.search_title(title, author, 5),
            || self.apple.search_title(title, author, 5),
            10,
            None,
        )?;
        let hit = Books::pick(&hits.matches, title, year, author);
        match (hit, hits.failure) {
            (None, Some(failure)) => Err(failure),
            (hit, _) => Ok(hit),
        }
    }

    /// An Open Library pick takes Apple's description and cover (the
    /// work's where Apple has none); an Apple pick takes Open Library's
    /// year, author and page count; a Google pick reads its volume, then
    /// both. Every pick then gets its series.
    fn details(&self, found: &Match) -> Reply<Match> {
        let full = if Books::is_apple(&found.source) {
            Books::with_open_library(
                found.clone(),
                self.open_library_fill(
                    &found.title,
                    found.creator.as_deref(),
                    found.description.is_empty(),
                )
                .as_ref(),
            )
        } else if Books::is_google(&found.source) {
            let full = Books::fill_gaps(found.clone(), self.google.volume(&found.source)?.as_ref());
            let with_both = Books::with_apple(
                full.clone(),
                self.apple_fill(&full.title, full.creator.as_deref())
                    .as_ref(),
            );
            Books::with_open_library(
                with_both.clone(),
                self.open_library_fill(
                    &with_both.title,
                    with_both.creator.as_deref(),
                    with_both.description.is_empty(),
                )
                .as_ref(),
            )
        } else {
            let mut full = found.clone();
            if full.description.is_empty()
                || full.cover_url.is_none()
                || full.released.is_none()
                || full.creator.is_none()
            {
                full = Books::with_apple(
                    full.clone(),
                    self.apple_fill(&full.title, full.creator.as_deref())
                        .as_ref(),
                );
            }
            if full.description.is_empty() || full.cover_url.is_none() {
                full = self.open_library.with_work(full)?;
            }
            full
        };
        let series = self.series_of(&full.title, full.creator.as_deref());
        Ok(Match { series, ..full })
    }

    /// A book's series with its parts: Wikidata's for the book's title and
    /// author, else the volumes of the manga the title (or `name`, the
    /// series the entry carries) names, read from Open Library by that
    /// name and the author.
    fn series_parts(&self, source: &str, name: Option<&str>) -> Reply<Option<SeriesParts>> {
        let Some(book) = self.book_by_id(source)? else {
            return Ok(None);
        };
        let mut parts = self
            .wikidata
            .series_of(&book.title, book.creator.as_deref())?
            .map(|(parts, _)| parts);
        if parts.is_none() {
            let series_name = name
                .map(str::to_owned)
                .or_else(|| volume_of(&book.title).map(|(name, _)| name));
            let Some(series_name) = series_name else {
                return Ok(None);
            };
            parts = self
                .open_library
                .volumes(&series_name, book.creator.as_deref())?;
        }
        Ok(parts.map(|parts| self.with_upcoming_covers(parts, book.creator.as_deref())))
    }

    /// An Open Library id: Apple's artwork for the work, found by its title
    /// and author, else Open Library's own cover. An Apple id: its record's
    /// artwork, else Open Library's cover by title. A Google id: its
    /// volume's cover, else the others' by title.
    fn cover_url(&self, source: &str, _media: MediaType) -> Reply<Option<String>> {
        if Books::is_apple(source) {
            let Some(book) = self.apple.book(source)? else {
                return Ok(None);
            };
            if let Some(url) = book.cache_cover_url() {
                return Ok(Some(url.to_owned()));
            }
            return Ok(self
                .open_library_fill(&book.title, book.creator.as_deref(), true)
                .and_then(|ol| ol.cache_cover_url().map(str::to_owned)));
        }
        if Books::is_google(source) {
            let Some(volume) = self.google.volume(source)? else {
                return Ok(None);
            };
            if let Some(url) = volume.cache_cover_url() {
                return Ok(Some(url.to_owned()));
            }
            if let Some(url) = self
                .apple_fill(&volume.title, volume.creator.as_deref())
                .and_then(|a| a.cache_cover_url().map(str::to_owned))
            {
                return Ok(Some(url));
            }
            return Ok(self
                .open_library_fill(&volume.title, volume.creator.as_deref(), true)
                .and_then(|ol| ol.cache_cover_url().map(str::to_owned)));
        }
        let doc = self.open_library.by_key(source)?;
        if let Some(doc) = &doc {
            if let Some(url) = self
                .apple_fill(&doc.title, doc.creator.as_deref())
                .and_then(|a| a.cache_cover_url().map(str::to_owned))
            {
                return Ok(Some(url));
            }
            if let Some(url) = doc.cache_cover_url() {
                return Ok(Some(url.to_owned()));
            }
        }
        let work = self.open_library.work(source)?;
        if let Some(cover_id) = work.cover_id {
            return Ok(Some(OpenLibrary::cover(cover_id, "-L")));
        }
        if doc.is_none()
            && let Some(title) = &work.title
        {
            return Ok(self
                .apple_fill(title, None)
                .and_then(|a| a.cache_cover_url().map(str::to_owned)));
        }
        Ok(None)
    }

    /// Covers for the book `title` by `creator`, unmerged so every edition
    /// with a cover of its own is offered: Open Library's works with their
    /// best edition's cover, taking turns with Apple's ebooks; the title
    /// compared without what follows it, the author where both know one,
    /// and one of each image.
    fn cover_choices(&self, title: &str, _media: MediaType, creator: Option<&str>) -> Reply<Hits> {
        let author = creator.filter(|c| !c.is_empty());
        let core = core_title(title);
        let (tx, rx) = std::sync::mpsc::channel::<(bool, Reply<Vec<Match>>)>();
        std::thread::scope(|scope| {
            let tx_ol = tx.clone();
            let core_ol = core.clone();
            scope.spawn(move || {
                let _ = tx_ol.send((false, self.open_library.search_title(&core_ol, author, 20)));
            });
            let core_ab = core.clone();
            scope.spawn(move || {
                let _ = tx.send((true, self.apple.editions(&core_ab, author, 25)));
            });
        });
        let (mut ol, mut ab) = (None, None);
        for (is_apple, result) in rx {
            if is_apple {
                ab = Some(result);
            } else {
                ol = Some(result);
            }
        }
        let ol = ol.unwrap_or_else(|| Err(Failure::other("no answer")));
        let ab = ab.unwrap_or_else(|| Err(Failure::other("no answer")));
        if let (Err(failure), Err(_)) = (&ol, &ab) {
            return Err(failure.clone());
        }
        let lists: Vec<Vec<Match>> = [
            ol.as_ref().ok().cloned(),
            ab.as_ref().ok().map(|hits| {
                hits.iter()
                    .map(|hit| Match {
                        edition: edition_label(&hit.title, None, Some("Ebook")),
                        ..hit.clone()
                    })
                    .collect()
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        let key = title_key(&core);
        let mut seen: Vec<String> = Vec::new();
        let mut out = Vec::new();
        let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
        for i in 0..longest {
            for list in &lists {
                let Some(hit) = list.get(i) else {
                    continue;
                };
                let Some(url) = hit.cache_cover_url() else {
                    continue;
                };
                if seen.iter().any(|s| s == url) {
                    continue;
                }
                if title_key(&core_title(&hit.title)) != key {
                    continue;
                }
                if let Some(author) = author
                    && hit.creator.is_some()
                    && !share_author(Some(author), hit.creator.as_deref())
                {
                    continue;
                }
                seen.push(url.to_owned());
                out.push(hit.clone());
            }
        }
        let failure = ol.err().or(ab.err());
        Ok(Hits {
            matches: out,
            failure,
        })
    }

    /// A cover picked in `cover_choices`: an Open Library edition's own
    /// cover ("openlibrary:OL7518154M"), a work's own cover, an Apple
    /// ebook's artwork, a Google volume's; never another source's cover of
    /// the same book.
    fn picked_cover_url(&self, id: &str, _media: MediaType) -> Reply<Option<String>> {
        if Books::is_apple(id) {
            return Ok(self
                .apple
                .book(id)?
                .and_then(|b| b.cache_cover_url().map(str::to_owned)));
        }
        if Books::is_google(id) {
            return Ok(self
                .google
                .volume(id)?
                .and_then(|v| v.cache_cover_url().map(str::to_owned)));
        }
        let olid = work_id(id)?;
        if olid.ends_with('M') {
            return Ok(Some(format!("{COVERS}/b/olid/{olid}-L.jpg?default=false")));
        }
        Ok(self
            .open_library
            .work(id)?
            .cover_id
            .map(|c| OpenLibrary::cover(c, "-L")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(title: &str, author: &str, source: &str) -> Match {
        Match {
            creator: Some(author.into()),
            ..Match::new(MediaType::Books, title, source)
        }
    }

    #[test]
    fn an_open_library_doc_becomes_a_match_with_its_editions_cover() {
        let doc: Value = serde_json::from_str(
            r#"{"key": "/works/OL17405W", "title": "Dracula", "author_name": ["Bram Stoker"],
                "first_publish_year": 1890, "publish_year": [1897, 1950, 2003],
                "cover_i": 11, "subject": ["Horror", "Fiction, gothic, general"],
                "number_of_pages_median": 418, "language": ["eng"],
                "editions": {"docs": [{"key": "/books/OL7518154M", "cover_i": 22,
                    "format": ["Mass Market Paperback"], "publish_date": ["June 2003"]}]}}"#,
        )
        .unwrap();
        let found = OpenLibrary::hit(&doc).unwrap();
        assert_eq!(found.source, "openlibrary:OL17405W");
        assert_eq!(
            found.year(),
            Some(1897),
            "the editions' years, not first_publish_year"
        );
        assert_eq!(found.pages, Some(418));
        assert_eq!(found.language.as_deref(), Some("eng"));
        assert_eq!(found.tags, ["Horror"]);
        assert_eq!(
            found.cover_url.as_deref(),
            Some("https://covers.openlibrary.org/b/id/22-M.jpg")
        );
        assert_eq!(
            found.cover_source.as_deref(),
            Some("openlibrary:OL7518154M")
        );
        assert_eq!(found.edition.as_deref(), Some("Paperback"));
        assert_eq!(found.edition_year, Some(2003));
    }

    #[test]
    fn an_apple_result_becomes_a_match_sized_for_the_cache() {
        let item: Value = serde_json::from_str(
            r#"{"trackId": 498187584, "trackName": "Dracula", "artistName": "Bram Stoker",
                "releaseDate": "2012-03-13T07:00:00Z", "description": "<p>The ebook.</p>",
                "artworkUrl100": "https://is1.mzstatic.com/image/thumb/x/100x100bb.jpg",
                "genres": ["Books", "Horror", "Classics"]}"#,
        )
        .unwrap();
        let found = ITunes::hit(&item).unwrap();
        assert_eq!(found.source, "itunes:498187584");
        assert!(found.ebook_dated);
        assert_eq!(found.description, "The ebook.");
        assert_eq!(found.tags, ["Horror", "Classics"]);
        assert_eq!(
            found.cover_url.as_deref(),
            Some("https://is1.mzstatic.com/image/thumb/x/200x200bb.jpg")
        );
        assert_eq!(
            found.cache_cover_url(),
            Some("https://is1.mzstatic.com/image/thumb/x/600x600bb.jpg")
        );
        assert_eq!(
            ITunes::artwork_at("https://x/odd.jpg", 600),
            "https://x/odd.jpg"
        );
    }

    #[test]
    fn a_google_volume_becomes_a_match_over_https() {
        let item: Value = serde_json::from_str(
            r#"{"id": "eOTEEQAAQBAJ", "volumeInfo": {"title": "Piranesi", "authors": ["Susanna Clarke"],
                "publishedDate": "2020-09-15", "pageCount": 272, "categories": ["Fiction / Fantasy / General"],
                "imageLinks": {"thumbnail": "http://books.google.com/x?edge=curl&zoom=1", "medium": "http://books.google.com/m"}}}"#,
        )
        .unwrap();
        let found = Google::hit(&item).unwrap();
        assert_eq!(found.source, "googlebooks:eOTEEQAAQBAJ");
        assert_eq!(found.tags, ["Fiction", "Fantasy"]);
        assert_eq!(
            found.cover_url.as_deref(),
            Some("https://books.google.com/x?zoom=1")
        );
        assert_eq!(found.pages, Some(272));
    }

    #[test]
    fn the_merge_keeps_open_librarys_rows_and_takes_apples_description() {
        let ol = vec![
            book("Dracula", "Bram Stoker", "openlibrary:OL17405W"),
            book("Dracula's Guest", "Bram Stoker", "openlibrary:OL2W"),
        ];
        let apple = vec![
            Match {
                description: "The ebook.".into(),
                cover_url: Some("a.jpg".into()),
                large_cover_url: Some("a600.jpg".into()),
                edition: Some("Ebook".into()),
                ..book("Dracula", "Bram Stoker", "itunes:1")
            },
            book("Carmilla", "Sheridan Le Fanu", "itunes:2"),
            book("Carmilla", "sheridan le fanu", "itunes:3"),
        ];
        let merged = Books::merge_hits(ol, apple, 10);
        let sources: Vec<&str> = merged.iter().map(|m| m.source.as_str()).collect();
        assert_eq!(
            sources,
            ["openlibrary:OL17405W", "itunes:2", "openlibrary:OL2W"],
            "Apple's own rows take turns; the same book twice is one"
        );
        assert_eq!(merged[0].description, "The ebook.");
        assert_eq!(merged[0].cover_source.as_deref(), Some("itunes:1"));
        assert_eq!(merged[0].cache_cover_url(), Some("a600.jpg"));

        let apple_pick = Match {
            released: Released::parse("2012"),
            ebook_dated: true,
            ..book("Dracula", "Bram Stoker", "itunes:1")
        };
        let ol_record = Match {
            released: Released::parse("1897"),
            pages: Some(418),
            ..book("Dracula", "Bram Stoker", "openlibrary:OL17405W")
        };
        let filled = Books::with_open_library(apple_pick, Some(&ol_record));
        assert_eq!(filled.year(), Some(1897));
        assert!(!filled.ebook_dated);
        assert_eq!(filled.pages, Some(418));
        assert_eq!(filled.source, "itunes:1", "the pick keeps its id");
    }

    #[test]
    fn the_cover_match_wants_the_author_or_an_exact_year() {
        let hits = vec![
            Match {
                released: Released::parse("1897"),
                ..book("Dracula", "Bram Stoker", "openlibrary:OL17405W")
            },
            Match {
                released: Released::parse("2012"),
                ..book("Dracula", "Someone Else", "itunes:1")
            },
        ];
        assert_eq!(
            Books::pick(&hits, "Dracula", Some(2012), None).map(|m| m.source),
            Some("itunes:1".to_owned())
        );
        assert_eq!(
            Books::pick(&hits, "Dracula", Some(1990), Some("Bram Stoker")).map(|m| m.source),
            Some("openlibrary:OL17405W".to_owned()),
            "an author makes the title enough"
        );
        assert_eq!(Books::pick(&hits, "Dracula", Some(1990), None), None);
        assert!(Books::new("").issued("itunes:1") && Books::new("").issued("googlebooks:x"));
        assert!(!Books::new("").issued("tmdb:movie:1"));
    }
}
