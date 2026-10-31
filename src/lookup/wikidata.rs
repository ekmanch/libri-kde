//! Which series a book belongs to, and its number, from Wikidata
//! (`wikidata_series.dart`). No key. The book is found by its title with
//! the entity search (case does not matter there, and Open Library titles
//! are often in sentence case), then one SPARQL query reads its "part of
//! the series" statement (P179) with the series ordinal (P1545), and every
//! other part of that series with its ordinal and publication date
//! (P577). The author (P50) tells the book from a film or a game of the
//! same name. The parts the series page should not list at first are
//! marked as extras.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use super::http::{self, Http, objects_at, str_at};
use super::text::core_title;
use super::{Reply, SeriesPart, SeriesParts};
use crate::model::Released;

const API: &str = "https://www.wikidata.org/w/api.php";
const SPARQL: &str = "https://query.wikidata.org/sparql";
/// How many entity-search hits are looked at for a title.
const CANDIDATES: usize = 10;

pub struct Wikidata {
    http: Http,
}

/// One part of a series as Wikidata has it, before the extras are marked:
/// its item id, its label, its ordinal, its earliest year and date, the
/// items it is part of (P361) and its authors' names.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Part {
    pub id: String,
    pub title: String,
    pub number: Option<u32>,
    pub year: Option<u16>,
    pub released: Option<Released>,
    pub part_of: Vec<String>,
    pub authors: Vec<String>,
}

impl Wikidata {
    pub fn new() -> Wikidata {
        Wikidata {
            http: Http::new(Duration::from_secs(1)),
        }
    }

    /// The item ids the entity search finds for `title`.
    fn search(&self, title: &str) -> Reply<Vec<String>> {
        let url = http::url(
            API,
            &[
                ("action", "wbsearchentities"),
                ("search", title),
                ("language", "en"),
                ("type", "item"),
                ("limit", &CANDIDATES.to_string()),
                ("format", "json"),
            ],
        );
        let json = self
            .http
            .get_object(&url, &[("Accept", "application/json")])?;
        Ok(objects_at(&json, "search")
            .filter_map(|hit| str_at(hit, "id"))
            .filter(|id| {
                id.strip_prefix('Q')
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .map(str::to_owned)
            .collect())
    }

    /// One query for the candidates: each one's series statement with its
    /// ordinal and the names of its authors, and every part of the series
    /// with its ordinal, date, authors and what it is a part of.
    pub fn query(ids: &[String]) -> String {
        let values: Vec<String> = ids.iter().map(|id| format!("wd:{id}")).collect();
        format!(
            "SELECT ?book ?authorLabel ?series ?seriesLabel ?ordinal ?part ?partLabel ?partOrdinal \
             ?date ?partOf ?partAuthorLabel WHERE {{\n  VALUES ?book {{ {} }}\n  \
             ?book p:P179 ?statement . ?statement ps:P179 ?series .\n  \
             OPTIONAL {{ ?statement pq:P1545 ?ordinal }}\n  OPTIONAL {{ ?book wdt:P50 ?author }}\n  \
             ?part p:P179 ?partStatement . ?partStatement ps:P179 ?series .\n  \
             OPTIONAL {{ ?partStatement pq:P1545 ?partOrdinal }}\n  OPTIONAL {{ ?part wdt:P577 ?date }}\n  \
             OPTIONAL {{ ?part wdt:P361 ?partOf }}\n  OPTIONAL {{ ?part wdt:P50 ?partAuthor }}\n  \
             SERVICE wikibase:label {{ bd:serviceParam wikibase:language \"en\". }}\n}}\nLIMIT 5000",
            values.join(" ")
        )
    }

    fn sparql(&self, query: &str) -> Reply<Vec<Value>> {
        let url = http::url(SPARQL, &[("query", query), ("format", "json")]);
        let json = self
            .http
            .get_object(&url, &[("Accept", "application/json")])?;
        Ok(objects_at(&json["results"], "bindings").cloned().collect())
    }

    fn value<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
        str_at(&row[key], "value")
    }

    fn item(row: &Value, key: &str) -> Option<String> {
        Wikidata::value(row, key).map(|v| v.rsplit('/').next().unwrap_or(v).to_owned())
    }

    /// Whether a Wikidata author name is the entry's creator: a word of
    /// three letters or more in common (a surname), case aside.
    pub fn same_author<'a>(
        creator: Option<&str>,
        authors: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        let Some(creator) = creator.map(str::trim).filter(|c| !c.is_empty()) else {
            return true;
        };
        let words = |text: &str| -> Vec<String> {
            text.to_lowercase()
                .split(|c: char| !c.is_alphabetic())
                .filter(|w| w.chars().count() >= 3)
                .map(str::to_owned)
                .collect()
        };
        let wanted = words(creator);
        authors
            .into_iter()
            .any(|author| words(author).iter().any(|w| wanted.contains(w)))
    }

    /// The series the book `title` by `creator` is in, with its parts in
    /// ordinal order (a part without an ordinal by date, after the rest),
    /// and the book's own number; `None` when Wikidata knows no series for
    /// it. A title that names nothing is asked again without what an
    /// edition tacks on after it.
    pub fn series_of(
        &self,
        title: &str,
        creator: Option<&str>,
    ) -> Reply<Option<(SeriesParts, Option<u32>)>> {
        if let Some(found) = self.series_of_title(title, creator)? {
            return Ok(Some(found));
        }
        let core = core_title(title);
        if core == title {
            return Ok(None);
        }
        self.series_of_title(&core, creator)
    }

    fn series_of_title(
        &self,
        title: &str,
        creator: Option<&str>,
    ) -> Reply<Option<(SeriesParts, Option<u32>)>> {
        let ids = self.search(title)?;
        if ids.is_empty() {
            return Ok(None);
        }
        let rows = self.sparql(&Wikidata::query(&ids))?;
        Ok(Wikidata::series_from_rows(&ids, &rows, creator))
    }

    /// The first candidate, in search order, that is a book by the author,
    /// with its series' parts cleaned and sorted.
    pub fn series_from_rows(
        ids: &[String],
        rows: &[Value],
        creator: Option<&str>,
    ) -> Option<(SeriesParts, Option<u32>)> {
        let mut authors_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut series_of: HashMap<String, String> = HashMap::new();
        let mut ordinal_of: HashMap<String, Option<u32>> = HashMap::new();
        let mut series_name: HashMap<String, String> = HashMap::new();
        let mut parts_of: HashMap<String, Vec<Part>> = HashMap::new();
        for row in rows {
            let (Some(book), Some(series)) =
                (Wikidata::item(row, "book"), Wikidata::item(row, "series"))
            else {
                continue;
            };
            if let Some(author) = Wikidata::value(row, "authorLabel") {
                let authors = authors_of.entry(book.clone()).or_default();
                if !authors.iter().any(|a| a == author) {
                    authors.push(author.to_owned());
                }
            }
            series_of.insert(book.clone(), series.clone());
            let ordinal = Wikidata::ordinal(Wikidata::value(row, "ordinal"));
            let known = ordinal_of.entry(book).or_insert(None);
            if ordinal.is_some() {
                *known = ordinal;
            }
            if let Some(label) = Wikidata::value(row, "seriesLabel") {
                series_name.insert(series.clone(), label.to_owned());
            }
            let (Some(part), Some(part_title)) = (
                Wikidata::item(row, "part"),
                Wikidata::value(row, "partLabel"),
            ) else {
                continue;
            };
            let parts = parts_of.entry(series).or_default();
            let released = Wikidata::released(Wikidata::value(row, "date"));
            let part_of = Wikidata::item(row, "partOf");
            let author = Wikidata::value(row, "partAuthorLabel");
            let known = match parts.iter_mut().find(|p| p.id == part) {
                Some(known) => known,
                None => {
                    parts.push(Part {
                        id: part,
                        title: part_title.to_owned(),
                        ..Part::default()
                    });
                    parts.last_mut().expect("just pushed")
                }
            };
            if let Some(number) = Wikidata::ordinal(Wikidata::value(row, "partOrdinal")) {
                known.number = Some(number);
            }
            known.released = earliest(released, known.released);
            known.year = known.released.map(|r| r.year);
            if let Some(part_of) = part_of
                && !known.part_of.contains(&part_of)
            {
                known.part_of.push(part_of);
            }
            if let Some(author) = author
                && !known.authors.iter().any(|a| a == author)
            {
                known.authors.push(author.to_owned());
            }
        }
        for id in ids {
            let Some(series) = series_of.get(id) else {
                continue;
            };
            let authors = authors_of.get(id).cloned().unwrap_or_default();
            if !Wikidata::same_author(creator, authors.iter().map(String::as_str)) {
                continue;
            }
            let mut all = parts_of.get(series).cloned().unwrap_or_default();
            let extras = Wikidata::extra_parts(&all);
            all.sort_by(|a, b| match (a.number, b.number) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => match (a.year, b.year) {
                    (Some(p), Some(q)) => p.cmp(&q),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                },
            });
            let parts = SeriesParts {
                name: series_name
                    .get(series)
                    .cloned()
                    .unwrap_or_else(|| series.clone()),
                parts: all
                    .into_iter()
                    .map(|p| SeriesPart {
                        extra: extras.contains(&p.id),
                        number: p.number,
                        year: p.year,
                        released: p.released,
                        source: Some(format!("wikidata:{}", p.id)),
                        creator: (!p.authors.is_empty()).then(|| p.authors.join(", ")),
                        cover: None,
                        title: p.title,
                    })
                    .collect(),
            };
            return Some((parts, ordinal_of.get(id).copied().flatten()));
        }
        None
    }

    /// The ids of the parts of a series the series page hides until the
    /// owner shows them: a part that is itself part of another part, a
    /// part with neither a number nor a date, and, when most of what is
    /// left is numbered, the unnumbered rest. A series that numbers few of
    /// its parts keeps them all.
    pub fn extra_parts(parts: &[Part]) -> Vec<String> {
        let ids: Vec<&str> = parts.iter().map(|p| p.id.as_str()).collect();
        let mut extras: Vec<String> = parts
            .iter()
            .filter(|p| {
                p.part_of.iter().any(|o| ids.contains(&o.as_str()))
                    || (p.number.is_none() && p.year.is_none())
            })
            .map(|p| p.id.clone())
            .collect();
        let kept: Vec<&Part> = parts.iter().filter(|p| !extras.contains(&p.id)).collect();
        let numbered = kept.iter().filter(|p| p.number.is_some()).count();
        if numbered * 2 > kept.len() {
            extras.extend(
                kept.iter()
                    .filter(|p| p.number.is_none())
                    .map(|p| p.id.clone()),
            );
        }
        extras
    }

    /// A publication date as Wikidata's query gives it
    /// ("1954-07-29T00:00:00Z") as a release: the full date, or the year
    /// alone when it falls on January 1, since a date known to the year is
    /// written so.
    pub fn released(value: Option<&str>) -> Option<Released> {
        let date = Released::parse(value?.get(..10)?)?;
        Some(if date.month == Some(1) && date.day == Some(1) {
            Released {
                year: date.year,
                month: None,
                day: None,
            }
        } else {
            date
        })
    }

    /// "3" as 3; "3.5" or "III" as nothing.
    fn ordinal(text: Option<&str>) -> Option<u32> {
        text?.trim().parse().ok()
    }
}

impl Default for Wikidata {
    fn default() -> Self {
        Wikidata::new()
    }
}

/// The earlier of two releases, either of which may be missing.
fn earliest(a: Option<Released>, b: Option<Released>) -> Option<Released> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if a.available_from() <= b.available_from() {
            a
        } else {
            b
        }),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(fields: &[(&str, &str)]) -> Value {
        let mut map = serde_json::Map::new();
        for (key, value) in fields {
            map.insert((*key).to_owned(), serde_json::json!({"value": value}));
        }
        Value::Object(map)
    }

    #[test]
    fn the_series_comes_from_the_first_candidate_by_the_author() {
        let wd = "http://www.wikidata.org/entity/";
        let rows = vec![
            // A film of the same name, by nobody the entry names.
            row(&[
                ("book", &format!("{wd}Q9")),
                ("authorLabel", "Peter Jackson"),
                ("series", &format!("{wd}Q90")),
                ("seriesLabel", "Films"),
                ("part", &format!("{wd}Q9")),
                ("partLabel", "Film"),
            ]),
            row(&[
                ("book", &format!("{wd}Q1")),
                ("authorLabel", "J. R. R. Tolkien"),
                ("series", &format!("{wd}Q10")),
                ("seriesLabel", "The Lord of the Rings"),
                ("ordinal", "2"),
                ("part", &format!("{wd}Q1")),
                ("partLabel", "The Two Towers"),
                ("partOrdinal", "2"),
                ("date", "1954-11-11T00:00:00Z"),
                ("partAuthorLabel", "J. R. R. Tolkien"),
            ]),
            row(&[
                ("book", &format!("{wd}Q1")),
                ("series", &format!("{wd}Q10")),
                ("part", &format!("{wd}Q2")),
                ("partLabel", "The Fellowship of the Ring"),
                ("partOrdinal", "1"),
                ("date", "1954-07-29T00:00:00Z"),
            ]),
            row(&[
                ("book", &format!("{wd}Q1")),
                ("series", &format!("{wd}Q10")),
                ("part", &format!("{wd}Q3")),
                ("partLabel", "Appendices"),
            ]),
            row(&[
                ("book", &format!("{wd}Q1")),
                ("series", &format!("{wd}Q10")),
                ("part", &format!("{wd}Q4")),
                ("partLabel", "The First Journey"),
                ("partOf", &format!("{wd}Q2")),
                ("date", "1999-01-01T00:00:00Z"),
            ]),
        ];
        let ids = vec!["Q9".to_owned(), "Q1".to_owned()];
        let (parts, number) = Wikidata::series_from_rows(&ids, &rows, Some("Tolkien")).unwrap();
        assert_eq!(parts.name, "The Lord of the Rings");
        assert_eq!(number, Some(2));
        let titles: Vec<&str> = parts.parts.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "The Fellowship of the Ring",
                "The Two Towers",
                "The First Journey",
                "Appendices"
            ]
        );
        assert!(parts.parts[2].extra, "part of a part");
        assert!(parts.parts[3].extra, "neither number nor date");
        assert_eq!(
            parts.parts[2].released.unwrap().to_string(),
            "1999",
            "January 1 is a year"
        );
        assert_eq!(parts.parts[1].creator.as_deref(), Some("J. R. R. Tolkien"));
        assert_eq!(parts.parts[1].source.as_deref(), Some("wikidata:Q1"));
        assert!(Wikidata::series_from_rows(&ids, &rows, Some("Clavell")).is_none());
        assert!(Wikidata::query(&ids).contains("VALUES ?book { wd:Q9 wd:Q1 }"));
        assert!(Wikidata::same_author(None, ["X"]));
        assert!(!Wikidata::same_author(Some("Jo Nesbø"), ["Peter Jackson"]));
    }

    #[test]
    fn extras_leave_a_mostly_unnumbered_series_alone() {
        let part = |id: &str, number: Option<u32>, year: Option<u16>| Part {
            id: id.into(),
            title: id.into(),
            number,
            year,
            ..Part::default()
        };
        let saga = [
            part("a", Some(1), Some(1966)),
            part("b", None, Some(1975)),
            part("c", None, Some(1981)),
        ];
        assert!(
            Wikidata::extra_parts(&saga).is_empty(),
            "Wikidata numbers only one"
        );
        let numbered = [
            part("a", Some(1), Some(1966)),
            part("b", Some(2), Some(1975)),
            part("c", None, Some(1981)),
        ];
        assert_eq!(Wikidata::extra_parts(&numbered), ["c"]);
    }
}
