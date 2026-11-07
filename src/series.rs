//! A series' parts as the inspector's Series section and the series in
//! the details pane list them: the lookup's list of parts matched to the library's
//! entries, or the library's own entries of the series when the lookup
//! has no list.

use crate::lookup::{SeriesPart, SeriesParts};
use crate::model::{Entry, Library, MediaType, Series};

/// One part in the list: the lookup's part, the library's entry for it
/// when there is one, and whether it is the entry being looked at.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesRow<'a> {
    pub number: Option<u32>,
    /// The title without the series name in front.
    pub title: String,
    pub year: Option<u16>,
    pub entry: Option<&'a Entry>,
    pub is_this: bool,
}

impl SeriesRow<'_> {
    pub fn owned(&self) -> bool {
        self.entry.is_some()
    }

    /// "2001", or "2003 · Not in your library". Without a year: "Release
    /// date not set" for a part in the library (its entry has no date),
    /// just "Not in your library" for one that is not.
    pub fn subtitle(&self) -> String {
        match (self.year, self.owned()) {
            (Some(year), true) => year.to_string(),
            (Some(year), false) => format!("{year} · Not in your library"),
            (None, true) => "Release date not set".into(),
            (None, false) => "Not in your library".into(),
        }
    }
}

/// The library's entries of `media` in the series `name`, in series order.
pub fn entries_in<'a>(library: &'a Library, media: MediaType, name: &str) -> Vec<&'a Entry> {
    let parts = library
        .entries
        .iter()
        .filter(|e| e.media == media && e.can_have_series())
        .filter(|e| {
            e.series
                .as_ref()
                .is_some_and(|s| Series::same(&s.name, name))
        })
        .collect();
    crate::browse::in_series_order(parts)
}

/// The part whose cover stands for the series: the one marked, else the
/// first in series order.
pub fn cover_entry<'a>(parts: &[&'a Entry]) -> Option<&'a Entry> {
    parts
        .iter()
        .copied()
        .find(|p| p.series.as_ref().is_some_and(|s| s.cover == Some(true)))
        .or(parts.first().copied())
}

/// Every part of the series, in order. The lookup's parts (`fetched`, when
/// it has a list) are matched to the library's entries by lookup id, then
/// by number, then by title (one pass per rule, so a weaker rule never
/// takes an entry a stronger one would give another part); entries the
/// lookup does not list come after. Parts the automatic rules count as
/// extras are left out.
pub fn rows<'a>(
    library: &'a Library,
    media: MediaType,
    name: &str,
    this: Option<&str>,
    fetched: Option<&SeriesParts>,
) -> Vec<SeriesRow<'a>> {
    let owned = entries_in(library, media, name);
    let display = Series::display_name(name);
    let fetched: Option<Vec<&SeriesPart>> =
        fetched.map(|parts| parts.parts.iter().filter(|p| !p.extra).collect());
    let Some(fetched) = fetched else {
        return owned
            .into_iter()
            .map(|entry| SeriesRow {
                number: entry.series.as_ref().and_then(|s| s.number),
                title: Series::part_title(&entry.title, &display),
                year: entry.year(),
                entry: Some(entry),
                is_this: this == Some(entry.id.as_str()),
            })
            .collect();
    };
    let mut matched: Vec<Option<&Entry>> = vec![None; fetched.len()];
    let mut left: Vec<&Entry> = owned.clone();
    let mut pass = |test: &dyn Fn(&SeriesPart, &Entry) -> bool,
                    matched: &mut Vec<Option<&'a Entry>>| {
        for (i, part) in fetched.iter().enumerate() {
            if matched[i].is_some() {
                continue;
            }
            if let Some(at) = left.iter().position(|e| test(part, e)) {
                matched[i] = Some(left.remove(at));
            }
        }
    };
    pass(
        &|part, e| part.source.is_some() && part.source == e.source,
        &mut matched,
    );
    pass(
        &|part, e| part.number.is_some() && part.number == e.series.as_ref().and_then(|s| s.number),
        &mut matched,
    );
    pass(
        &|part, e| part.title.eq_ignore_ascii_case(&e.title),
        &mut matched,
    );
    let mut rows: Vec<SeriesRow<'a>> = fetched
        .into_iter()
        .zip(matched)
        .map(|(part, entry)| SeriesRow {
            number: part.number,
            title: Series::part_title(entry.map_or(&part.title, |e| &e.title), &display),
            year: entry.and_then(Entry::year).or(part.year),
            entry,
            is_this: entry.is_some_and(|e| this == Some(e.id.as_str())),
        })
        .collect();
    for entry in left {
        rows.push(SeriesRow {
            number: entry.series.as_ref().and_then(|s| s.number),
            title: Series::part_title(&entry.title, &display),
            year: entry.year(),
            entry: Some(entry),
            is_this: this == Some(entry.id.as_str()),
        });
    }
    rows
}

/// How many parts the inspector's Series section lists at most
/// (2026-11-07, three as the Seasons section; it was five).
pub const WINDOW: usize = 3;

/// The window the inspector shows: every part up to three, else the one
/// before, this one and the one after (the first three when this is the
/// first, the last three when it is the last).
pub fn window<'a>(rows: &[SeriesRow<'a>]) -> Vec<SeriesRow<'a>> {
    if rows.len() <= WINDOW {
        return rows.to_vec();
    }
    let this = rows.iter().position(|r| r.is_this).unwrap_or(0);
    let start = this.saturating_sub(1).min(rows.len() - 3);
    rows[start..start + 3].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::codec;

    #[test]
    fn the_lookups_parts_are_matched_to_the_librarys_entries() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        let fetched = crate::lookup::fake::series_parts(MediaType::Movies, "The Lord of the Rings");
        let rows = rows(
            &library,
            MediaType::Movies,
            "The Lord of the Rings",
            Some("movies/the-lord-of-the-rings-the-two-towers-2002"),
            fetched.as_ref(),
        );
        // The fake lists The Hunt for Gollum (2027) as a fourth part.
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].title, "The Fellowship of the Ring");
        assert!(rows[0].owned() && !rows[0].is_this);
        assert!(rows[1].is_this);
        assert_eq!(rows[2].subtitle(), "2003 · Not in your library");
        assert_eq!(window(&rows).len(), 3);
    }

    #[test]
    fn a_part_without_a_year_says_so_instead_of_an_empty_line() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        let entry = library.entries.first().unwrap();
        let row = SeriesRow {
            number: Some(8),
            title: "Untitled sequel".into(),
            year: None,
            entry: Some(entry),
            is_this: false,
        };
        assert_eq!(row.subtitle(), "Release date not set");
        let row = SeriesRow { entry: None, ..row };
        assert_eq!(row.subtitle(), "Not in your library");
    }

    #[test]
    fn a_long_series_shows_three_around_this_one() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        let fetched = crate::lookup::fake::series_parts(MediaType::Books, "Berserk");
        let rows = rows(
            &library,
            MediaType::Books,
            "Berserk",
            Some("books/berserk-vol-43-2025"),
            fetched.as_ref(),
        );
        let own_only = super::rows(&library, MediaType::Books, "Berserk", None, None);
        assert_eq!(
            own_only.len(),
            3,
            "without a list, the library's own entries"
        );
        assert_eq!(rows.len(), 44, "volume 44 is on the way");
        assert_eq!(rows.iter().filter(|r| r.owned()).count(), 3);
        let shown = window(&rows);
        assert_eq!(
            shown.iter().map(|r| r.number).collect::<Vec<_>>(),
            [Some(42), Some(43), Some(44)],
            "this one in the middle, with the volume on its way after it"
        );
        assert_eq!(shown[1].title, "Vol. 43");
        let first = rows
            .iter()
            .cloned()
            .map(|mut r| {
                r.is_this = r.number == Some(1);
                r
            })
            .collect::<Vec<_>>();
        assert_eq!(window(&first)[0].number, Some(1));
    }
}
