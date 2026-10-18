//! Choosing and ordering what the library shows: the filter, the sort and
//! the search. The rules are the phone app's (library_filter.dart,
//! library_sort.dart, library_search.dart).

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use crate::model::{Entry, EntryStatus, MediaType};

/// A release decade in the filter: "Older" for everything before 1960,
/// then one per decade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Decade {
    Older,
    From(u16),
}

impl Decade {
    pub const OLDEST_NAMED: u16 = 1960;

    pub fn of(year: Option<u16>) -> Option<Decade> {
        let year = year?;
        Some(if year < Decade::OLDEST_NAMED {
            Decade::Older
        } else {
            Decade::From(year / 10 * 10)
        })
    }

    /// The chips for entries from `years`: from the earliest to the latest
    /// decade that has an entry with the gaps filled in, and Older first
    /// when anything is older than 1960.
    pub fn options_for(years: impl IntoIterator<Item = Option<u16>>) -> Vec<Decade> {
        let mut older = false;
        let mut range: Option<(u16, u16)> = None;
        for year in years.into_iter().flatten() {
            match Decade::of(Some(year)) {
                Some(Decade::From(start)) => {
                    range = Some(
                        range.map_or((start, start), |(lo, hi)| (lo.min(start), hi.max(start))),
                    );
                }
                _ => older = true,
            }
        }
        let named = range
            .into_iter()
            .flat_map(|(lo, hi)| (lo..=hi).step_by(10).map(Decade::From));
        older
            .then_some(Decade::Older)
            .into_iter()
            .chain(named)
            .collect()
    }

    pub fn label(self) -> String {
        match self {
            Decade::Older => "Older".into(),
            Decade::From(start) => format!("{start}s"),
        }
    }
}

/// The Filter popover's choices. Within decades any choice matches; every
/// chosen tag must be on the entry; the rating range leaves out unrated
/// entries. Seasonal-only entries are hidden unless included.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub decades: BTreeSet<Decade>,
    /// In the order they were picked, as the chips show them.
    pub tags: Vec<String>,
    /// Lowest and highest rating, inclusive.
    pub rating: Option<(u8, u8)>,
    pub include_seasonal: bool,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        *self == Filter::default()
    }

    /// The number on the Filter button's badge.
    pub fn count(&self) -> usize {
        self.decades.len()
            + self.tags.len()
            + usize::from(self.rating.is_some())
            + usize::from(self.include_seasonal)
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        if entry.seasonal_only && !self.include_seasonal {
            return false;
        }
        if !self.decades.is_empty()
            && !Decade::of(entry.year()).is_some_and(|d| self.decades.contains(&d))
        {
            return false;
        }
        if let Some((low, high)) = self.rating
            && !entry.rating.is_some_and(|r| (low..=high).contains(&r))
        {
            return false;
        }
        self.tags.iter().all(|tag| entry.tags.contains(tag))
    }

    pub fn toggle_decade(&mut self, decade: Decade) {
        if !self.decades.remove(&decade) {
            self.decades.insert(decade);
        }
    }

    pub fn toggle_tag(&mut self, tag: &str) {
        match self.tags.iter().position(|t| t == tag) {
            Some(at) => {
                self.tags.remove(at);
            }
            None => self.tags.push(tag.to_owned()),
        }
    }

    /// A tap on a rating: the first picks one rating, a second makes a
    /// range with it, and a tap on a single picked rating clears it.
    pub fn pick_rating(&mut self, n: u8) {
        self.rating = match self.rating {
            Some((low, high)) if low == high && low == n => None,
            Some((low, high)) if low == high => Some((low.min(n), low.max(n))),
            _ => Some((n, n)),
        };
    }

    /// "Rated 8" or "Rated 7–9".
    pub fn rating_label(&self) -> Option<String> {
        self.rating.map(|(low, high)| {
            if low == high {
                format!("Rated {low}")
            } else {
                format!("Rated {low}–{high}")
            }
        })
    }

    /// Spells out a filter that matches nothing: "No books are both in
    /// progress and tagged #christmas. Remove a filter to see more."
    pub fn no_matches_text(&self, media: Option<MediaType>, status: Option<EntryStatus>) -> String {
        let subject = media.map_or("entries".to_owned(), |m| m.label().to_lowercase());
        let mut conditions = Vec::new();
        if let Some(status) = status {
            conditions.push(
                match status {
                    EntryStatus::Want => "wanted",
                    EntryStatus::InProgress => "in progress",
                    EntryStatus::Done => "done",
                    EntryStatus::Dnf => "not finished",
                }
                .to_owned(),
            );
        }
        if !self.decades.is_empty() {
            let decades: Vec<String> = self
                .decades
                .iter()
                .map(|d| match d {
                    Decade::Older => format!("from before {}", Decade::OLDEST_NAMED),
                    Decade::From(_) => format!("from the {}", d.label()),
                })
                .collect();
            conditions.push(join(&decades, "or"));
        }
        if let Some((low, high)) = self.rating {
            conditions.push(if low == high {
                format!("rated {low}")
            } else {
                format!("rated {low} to {high}")
            });
        }
        if !self.tags.is_empty() {
            let tags: Vec<String> = self.tags.iter().map(|t| format!("#{t}")).collect();
            conditions.push(format!("tagged {}", join(&tags, "and")));
        }
        const TAIL: &str = "Remove a filter to see more.";
        let what = match conditions.len() {
            0 => return TAIL.into(),
            1 => conditions[0].clone(),
            2 => format!("both {} and {}", conditions[0], conditions[1]),
            _ => join(&conditions, "and"),
        };
        format!("No {subject} are {what}. {TAIL}")
    }
}

/// "a, b or c".
fn join(parts: &[String], word: &str) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} {word} {last}", rest.join(", ")),
    }
}

/// The sort choices, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Sort {
    #[default]
    NewestAdded,
    TitleAz,
    YearNewest,
    YearOldest,
    RatingHighest,
}

impl Sort {
    pub const ALL: [Sort; 5] = [
        Sort::NewestAdded,
        Sort::TitleAz,
        Sort::YearNewest,
        Sort::YearOldest,
        Sort::RatingHighest,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Sort::NewestAdded => "Newest added",
            Sort::TitleAz => "Title, A to Z",
            Sort::YearNewest => "Year, newest first",
            Sort::YearOldest => "Year, oldest first",
            Sort::RatingHighest => "Rating, highest first",
        }
    }

    /// The Sort button's label.
    pub fn short(self) -> &'static str {
        match self {
            Sort::NewestAdded => "Newest added",
            Sort::TitleAz => "Title A–Z",
            Sort::YearNewest => "Year ↓",
            Sort::YearOldest => "Year ↑",
            Sort::RatingHighest => "Rating ↓",
        }
    }

    /// Sorts `entries`, which are in library order.
    pub fn apply(self, entries: &mut [&Entry]) {
        let position: HashMap<&str, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.as_str(), i))
            .collect();
        let by_title = |a: &&Entry, b: &&Entry| a.title.to_lowercase().cmp(&b.title.to_lowercase());
        // Missing values go last in every order.
        let last = |a: Option<u16>, b: Option<u16>, newest: bool| match (a, b) {
            (Some(a), Some(b)) if newest => b.cmp(&a),
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        match self {
            // Entries added the same day: the later in the library first.
            Sort::NewestAdded => entries.sort_by(|a, b| {
                b.added
                    .cmp(&a.added)
                    .then(position[b.id.as_str()].cmp(&position[a.id.as_str()]))
            }),
            Sort::TitleAz => entries.sort_by(by_title),
            Sort::YearNewest => {
                entries.sort_by(|a, b| last(a.year(), b.year(), true).then(by_title(a, b)))
            }
            Sort::YearOldest => {
                entries.sort_by(|a, b| last(a.year(), b.year(), false).then(by_title(a, b)))
            }
            Sort::RatingHighest => entries.sort_by(|a, b| {
                let rating = |e: &Entry| e.rating.map(u16::from);
                last(rating(a), rating(b), true).then(by_title(a, b))
            }),
        }
    }
}

/// Case-insensitive search over titles and tags; a leading "#" is ignored.
pub fn matches_search(entry: &Entry, query: &str) -> bool {
    let query = query.trim().trim_start_matches('#').to_lowercase();
    query.is_empty()
        || entry.title.to_lowercase().contains(&query)
        || entry.tags.iter().any(|tag| tag.contains(&query))
}

/// How a status reads for a type: "Watching" for a show in progress,
/// "Saved" for music that is done.
pub fn status_label(media: Option<MediaType>, status: EntryStatus) -> &'static str {
    match status {
        EntryStatus::Want => "Want",
        EntryStatus::InProgress => match media {
            Some(MediaType::Shows | MediaType::Anime) => "Watching",
            Some(MediaType::Books) => "Reading",
            Some(MediaType::Games) => "Playing",
            _ => "In progress",
        },
        EntryStatus::Done if media == Some(MediaType::Music) => "Saved",
        EntryStatus::Done => "Done",
        EntryStatus::Dnf => "DNF",
    }
}

/// The status chips for a type, or every status for everything.
pub fn status_options(media: Option<MediaType>) -> &'static [EntryStatus] {
    match media {
        Some(media) => EntryStatus::options_for(media),
        None => EntryStatus::options_for(MediaType::Shows),
    }
}

/// What the creator column is called: "Director", "Author".
pub fn creator_label(media: Option<MediaType>) -> &'static str {
    match media {
        Some(MediaType::Movies) => "Director",
        Some(MediaType::Books) => "Author",
        Some(MediaType::Games) => "Studio",
        Some(MediaType::Music) => "Artist",
        _ => "Creator",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{Library, codec};

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    fn titles(entries: &[&Entry]) -> Vec<String> {
        entries.iter().map(|e| e.title.clone()).collect()
    }

    #[test]
    fn decade_options_fill_gaps_and_start_with_older() {
        let years = [Some(1897), Some(1982), Some(2013), None];
        let options = Decade::options_for(years);
        let labels: Vec<_> = options.iter().map(|d| d.label()).collect();
        assert_eq!(labels, ["Older", "1980s", "1990s", "2000s", "2010s"]);
    }

    #[test]
    fn every_tag_must_match_and_decades_any() {
        let library = library();
        let mut filter = Filter::default();
        filter.toggle_tag("horror");
        filter.toggle_tag("halloween");
        assert!(
            library
                .entries
                .iter()
                .filter(|e| filter.matches(e))
                .all(|e| {
                    e.tags.iter().any(|t| t == "horror") && e.tags.iter().any(|t| t == "halloween")
                })
        );
        filter = Filter::default();
        filter.toggle_decade(Decade::From(1980));
        filter.toggle_decade(Decade::From(1990));
        let years: Vec<_> = library
            .entries
            .iter()
            .filter(|e| filter.matches(e))
            .map(|e| e.year().unwrap())
            .collect();
        assert!(!years.is_empty() && years.iter().all(|y| (1980..2000).contains(y)));
    }

    #[test]
    fn seasonal_only_entries_need_the_switch() {
        let library = library();
        let seasonal = library.entries.iter().find(|e| e.seasonal_only).unwrap();
        let mut filter = Filter::default();
        assert!(!filter.matches(seasonal));
        filter.include_seasonal = true;
        assert!(filter.matches(seasonal));
    }

    #[test]
    fn rating_taps_make_a_single_rating_a_range_or_nothing() {
        let mut filter = Filter::default();
        filter.pick_rating(8);
        assert_eq!(filter.rating, Some((8, 8)));
        filter.pick_rating(6);
        assert_eq!(filter.rating_label().as_deref(), Some("Rated 6–8"));
        filter.pick_rating(9);
        assert_eq!(filter.rating, Some((9, 9)));
        filter.pick_rating(9);
        assert_eq!(filter.rating, None);
    }

    #[test]
    fn a_filter_that_matches_nothing_spells_itself_out() {
        let mut filter = Filter::default();
        filter.toggle_tag("christmas");
        assert_eq!(
            filter.no_matches_text(Some(MediaType::Books), Some(EntryStatus::InProgress)),
            "No books are both in progress and tagged #christmas. Remove a filter to see more."
        );
        filter.toggle_decade(Decade::Older);
        filter.toggle_decade(Decade::From(1990));
        assert_eq!(
            filter.no_matches_text(None, None),
            "No entries are both from before 1960 or from the 1990s and tagged #christmas. \
             Remove a filter to see more."
        );
    }

    #[test]
    fn rating_sort_puts_unrated_last_and_ties_by_title() {
        let library = library();
        let mut entries: Vec<&Entry> = library.entries_of(MediaType::Movies).collect();
        Sort::RatingHighest.apply(&mut entries);
        let ratings: Vec<_> = entries.iter().map(|e| e.rating).collect();
        let rated = ratings.iter().take_while(|r| r.is_some()).count();
        assert!(ratings[rated..].iter().all(Option::is_none));
        assert!(ratings[..rated].windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn newest_added_puts_the_later_of_a_same_day_pair_first() {
        let library = library();
        let first = library.entry("movies/the-mummy-1999").unwrap().clone();
        let second = Entry {
            id: "movies/second".into(),
            title: "A second".into(),
            ..first.clone()
        };
        let mut entries = vec![&first, &second];
        Sort::NewestAdded.apply(&mut entries);
        assert_eq!(titles(&entries), ["A second", "The Mummy"]);

        let mut all: Vec<&Entry> = library.entries.iter().collect();
        Sort::NewestAdded.apply(&mut all);
        assert!(all.windows(2).all(|w| w[0].added >= w[1].added));
    }

    #[test]
    fn search_matches_titles_and_tags_case_insensitively() {
        let library = library();
        let mummy = library.entry("movies/the-mummy-1999").unwrap();
        assert!(matches_search(mummy, "MUMMY"));
        assert!(matches_search(mummy, "#lightheart"));
        assert!(!matches_search(mummy, "dracula"));
    }

    #[test]
    fn status_labels_follow_the_type() {
        assert_eq!(
            status_label(Some(MediaType::Books), EntryStatus::InProgress),
            "Reading"
        );
        assert_eq!(
            status_label(Some(MediaType::Music), EntryStatus::Done),
            "Saved"
        );
        assert_eq!(status_label(None, EntryStatus::InProgress), "In progress");
    }
}
