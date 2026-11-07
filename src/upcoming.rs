//! What is on its way: one list for Home's Coming soon, the type panels'
//! Coming soon, the type-panel popover and the Coming soon page (decided
//! 2026-11-07), so they can never disagree. The rows and their order are
//! the phone's Library › Coming soon: Library tab, rule for rule
//! (`lib/features/library/upcoming.dart` and `home_sections.dart`;
//! owner's decision, 2026-11-07, after the canvas's order put every
//! year-only date after every month). Each entry is one row: a show or an
//! anime series its nearest coming season ("Title · Season N"), wanted
//! when the show or that season is Want; any other entry its own release
//! when that is still ahead, wanted when the entry is Want; and an entry
//! set Want with no date at all a "Date not set" row. Seasonal-only
//! entries stay out, as in every main list. The order is by the first day
//! the release counts as out (`Released::available_from`: a month's exact
//! dates before the month alone, a year alone after that year's months),
//! undated rows last, then by title; the page groups that order under the
//! month ("November", "January 2027" outside this year), the year alone
//! ("2027") and "Date not set", consecutive rows only, as the phone does.

use std::cmp::Ordering;

use crate::model::{Date, Entry, EntryStatus, Library, MediaType, Released};

/// One row on its way: an entry, or one of its seasons.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Upcoming<'a> {
    pub entry: &'a Entry,
    /// The season's number, for an announced season.
    pub season: Option<u32>,
    pub released: Option<Released>,
}

impl<'a> Upcoming<'a> {
    /// "Season 3", for a season row.
    pub fn sub(&self) -> Option<String> {
        self.season.map(|n| format!("Season {n}"))
    }

    /// "Title · Season 3", or the title alone.
    pub fn full_title(&self) -> String {
        match self.season {
            Some(n) => format!("{} · Season {n}", self.entry.title),
            None => self.entry.title.clone(),
        }
    }

    /// The key the row goes by: the entry's id, or "id#season".
    pub fn key(&self) -> String {
        key_of(&self.entry.id, self.season)
    }

    /// "Movie · Denis Villeneuve", "Show · Dan Erickson".
    pub fn line(&self) -> String {
        let kind = crate::browse::kind_label(self.entry);
        match &self.entry.creator {
            Some(creator) if !creator.trim().is_empty() => format!("{kind} · {creator}"),
            _ => kind.to_owned(),
        }
    }

    /// The order the lists use.
    pub fn sort_key(&self) -> SortKey {
        SortKey::of(self.released)
    }
}

/// The key of an entry row or a season row.
pub fn key_of(id: &str, season: Option<u32>) -> String {
    match season {
        Some(n) => format!("{id}#{n}"),
        None => id.to_owned(),
    }
}

/// Where a release sorts: the first day it counts as out, with no date
/// after every date (the phone's `compareUpcoming`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey {
    undated: bool,
    from: Date,
}

impl SortKey {
    pub fn of(released: Option<Released>) -> SortKey {
        match released {
            Some(released) => SortKey {
                undated: false,
                from: released.available_from(),
            },
            None => SortKey {
                undated: true,
                from: Date {
                    year: 0,
                    month: 1,
                    day: 1,
                },
            },
        }
    }
}

/// A group of the page: a month, a year, or no date. The page groups the
/// sorted rows by it, consecutive rows only, as the phone's
/// `upcomingGroups` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKey {
    Month { year: u16, month: u8 },
    Year(u16),
    None,
}

impl GroupKey {
    pub fn of(released: Option<Released>) -> GroupKey {
        match released {
            Some(Released {
                year,
                month: Some(month),
                ..
            }) => GroupKey::Month { year, month },
            Some(Released { year, .. }) => GroupKey::Year(year),
            None => GroupKey::None,
        }
    }

    /// "November" this year, "January 2027" outside it, "2027", "Date not
    /// set" (the phone's `upcomingGroupName`).
    pub fn name(self, today: Date) -> String {
        match self {
            GroupKey::Month { year, month } if year == today.year => {
                MONTHS[usize::from(month) - 1].to_owned()
            }
            GroupKey::Month { year, month } => format!("{} {year}", MONTHS[usize::from(month) - 1]),
            GroupKey::Year(year) => year.to_string(),
            GroupKey::None => "Date not set".into(),
        }
    }

    /// The rail's label: "Nov" this year, "Mar ’27" another year, "2027",
    /// "TBD".
    pub fn rail(self, today: Date) -> String {
        match self {
            GroupKey::Month { year, month } if year == today.year => {
                MONTHS[usize::from(month) - 1][..3].to_owned()
            }
            GroupKey::Month { year, month } => {
                format!(
                    "{} ’{:02}",
                    &MONTHS[usize::from(month) - 1][..3],
                    year % 100
                )
            }
            GroupKey::Year(year) => year.to_string(),
            GroupKey::None => "TBD".into(),
        }
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A release as the rows show it: "Nov 19" this year, "Dec 2027" for a
/// month, "2027" for a year, "Date not set" for none.
pub fn date_label(released: Option<Released>, today: Date) -> String {
    match released {
        Some(released) => crate::screens::library::short_release(released, today),
        None => "Date not set".into(),
    }
}

/// Everything the owner has on the way in `library`, in the lists'
/// order: the phone's `upcomingItems` kept to its wanted rows.
pub fn library_upcoming(library: &Library, today: Date) -> Vec<Upcoming<'_>> {
    let mut rows = Vec::new();
    for entry in &library.entries {
        if entry.seasonal_only {
            continue;
        }
        if entry.uses_seasons() {
            // The nearest coming season, wanted when the show or that
            // season is Want.
            let next = entry
                .season_list()
                .iter()
                .filter(|s| s.released.is_some_and(|r| !r.is_released_on(today)))
                .min_by_key(|s| s.released.map(Released::available_from));
            if let Some(season) = next
                && (entry.status == EntryStatus::Want || season.status == Some(EntryStatus::Want))
            {
                rows.push(Upcoming {
                    entry,
                    season: Some(season.season),
                    released: season.released,
                });
            }
            continue;
        }
        if entry.status != EntryStatus::Want {
            continue;
        }
        match entry.released {
            Some(released) if released.is_released_on(today) => {}
            released => rows.push(Upcoming {
                entry,
                season: None,
                released,
            }),
        }
    }
    sort(&mut rows);
    rows
}

/// The rows of `media` alone.
pub fn of_type<'a>(rows: &[Upcoming<'a>], media: MediaType) -> Vec<Upcoming<'a>> {
    rows.iter()
        .copied()
        .filter(|row| row.entry.media == media)
        .collect()
}

/// In the lists' order: the sort key, then the title.
pub fn sort(rows: &mut [Upcoming<'_>]) {
    rows.sort_by(|a, b| compare(a.sort_key(), &a.entry.title, b.sort_key(), &b.entry.title));
}

/// The order two rows with these keys and titles go in (the phone's
/// `compareUpcoming`: the date, then the titles in lowercase).
pub fn compare(a: SortKey, a_title: &str, b: SortKey, b_title: &str) -> Ordering {
    a.cmp(&b)
        .then_with(|| a_title.to_lowercase().cmp(&b_title.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{EntryStatus, Season, codec};

    fn today() -> Date {
        Date::parse("2026-11-07").unwrap()
    }

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    #[test]
    fn the_order_is_the_phones_by_the_first_day_a_release_counts_as_out() {
        let key = |text: Option<&str>| SortKey::of(text.and_then(Released::parse));
        let mut keys = vec![
            key(Some("2027")),
            key(None),
            key(Some("2026-11")),
            key(Some("2026")),
            key(Some("2028-03")),
            key(Some("2026-12-18")),
            key(Some("2027-03-12")),
            key(Some("2026-11-19")),
        ];
        keys.sort();
        assert_eq!(
            keys,
            [
                key(Some("2026-11-19")),
                key(Some("2026-11")),
                key(Some("2026-12-18")),
                key(Some("2026")),
                key(Some("2027-03-12")),
                key(Some("2027")),
                key(Some("2028-03")),
                key(None),
            ]
        );
        // A month alone counts from the next month's first day, a year
        // alone from the next year's: the same day for December and its
        // year, as on the phone.
        assert_eq!(key(Some("2026-12")), key(Some("2026")));
    }

    #[test]
    fn groups_are_named_as_on_the_phone() {
        let today = today();
        let group = |text: Option<&str>| GroupKey::of(text.and_then(Released::parse));
        assert_eq!(group(Some("2026-12-18")).name(today), "December");
        assert_eq!(group(Some("2027-01")).name(today), "January 2027");
        assert_eq!(group(Some("2027")).name(today), "2027");
        assert_eq!(group(None).name(today), "Date not set");
        assert_eq!(group(Some("2026-11-19")).rail(today), "Nov");
        assert_eq!(group(Some("2027-03")).rail(today), "Mar ’27");
        assert_eq!(group(Some("2027")).rail(today), "2027");
        assert_eq!(group(None).rail(today), "TBD");
    }

    #[test]
    fn the_library_list_has_entries_and_announced_seasons_in_order() {
        let mut library = library();
        let show = library
            .entries
            .iter_mut()
            .find(|e| e.id == "shows/stranger-things-2016")
            .unwrap();
        show.seasons = Some(vec![
            Season {
                season: 1,
                released: Released::parse("2016-07-15"),
                status: Some(EntryStatus::Done),
                fresh: None,
                manual: None,
            },
            Season {
                season: 2,
                released: Released::parse("2027"),
                status: None,
                fresh: None,
                manual: None,
            },
        ]);
        // An undated Want entry is a "Date not set" row, last; a show that
        // is DNF with its announced season unset is not wanted.
        library
            .entries
            .iter_mut()
            .find(|e| e.id == "books/piranesi-2020")
            .unwrap()
            .released = None;
        let rows = library_upcoming(&library, today());
        let keys: Vec<String> = rows.iter().map(Upcoming::key).collect();
        assert_eq!(
            keys,
            [
                "games/grand-theft-auto-vi-2026",
                "movies/the-batman-part-ii-2027",
                "books/piranesi-2020",
            ]
        );
        library
            .entries
            .iter_mut()
            .find(|e| e.id == "shows/stranger-things-2016")
            .unwrap()
            .seasons
            .as_mut()
            .unwrap()[1]
            .status = Some(EntryStatus::Want);
        let rows = library_upcoming(&library, today());
        let keys: Vec<String> = rows.iter().map(Upcoming::key).collect();
        assert_eq!(
            keys[2], "shows/stranger-things-2016#2",
            "its season set Want"
        );
        assert_eq!(keys[3], "books/piranesi-2020");
        assert_eq!(rows[2].full_title(), "Stranger Things · Season 2");
        assert_eq!(rows[2].sub().as_deref(), Some("Season 2"));
        assert_eq!(rows[0].line(), "Game · Rockstar Games");
        assert_eq!(date_label(rows[0].released, today()), "Nov 19");
        assert_eq!(date_label(rows[1].released, today()), "Oct 2027");
        assert_eq!(date_label(rows[3].released, today()), "Date not set");
        assert_eq!(of_type(&rows, MediaType::Shows).len(), 1);
        // Nothing seasonal-only, nothing out.
        assert!(rows.iter().all(|r| !r.entry.seasonal_only));
    }
}
