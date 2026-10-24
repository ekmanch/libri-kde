//! Choosing and ordering what the library shows: the filter, the sort and
//! the search. The rules are the phone app's (library_filter.dart,
//! library_sort.dart, library_search.dart).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::model::{
    Date, Entry, EntryKind, EntryStatus, Library, MediaType, Plan, Series, TagCategory, tag_label,
};

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
/// entries. Seasonal-only entries are hidden unless included. The last two
/// come from the overview's "See all" links.
///
/// A tag in more than one category is chosen per category: Directors ›
/// #clint-eastwood matches only entries where he is picked as a director.
/// Such a choice is the tag in `tags` plus its categories in
/// `tag_categories`; a tag there with no categories (one in a single
/// category, or chosen by name from the sidebar) matches whatever category
/// it has on the entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub decades: BTreeSet<Decade>,
    /// In the order they were picked, as the chips show them.
    pub tags: Vec<String>,
    /// The categories a tag in `tags` is chosen as, for tags chosen per
    /// category. Never an empty set: a tag with none is not in the map.
    pub tag_categories: BTreeMap<String, BTreeSet<TagCategory>>,
    /// Lowest and highest rating, inclusive.
    pub rating: Option<(u8, u8)>,
    pub include_seasonal: bool,
    /// Wanted and not out yet: the overview's Coming soon.
    pub not_released: bool,
    /// Added in the past month: the overview's Recently added.
    pub recent: bool,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        *self == Filter::default()
    }

    /// The number on the Filter button's badge.
    pub fn count(&self) -> usize {
        self.decades.len()
            + self.tag_chips().len()
            + usize::from(self.rating.is_some())
            + usize::from(self.include_seasonal)
            + usize::from(self.not_released)
            + usize::from(self.recent)
    }

    pub fn matches(&self, entry: &Entry, today: Date) -> bool {
        if self.not_released && !is_coming(entry, today) {
            return false;
        }
        if self.recent && entry.added < today.month_before() {
            return false;
        }
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
        self.tags.iter().all(|tag| {
            entry.tags.contains(tag)
                && self
                    .tag_categories
                    .get(tag)
                    .is_none_or(|categories| categories.iter().all(|&c| entry.has_tag_as(tag, c)))
        })
    }

    pub fn toggle_decade(&mut self, decade: Decade) {
        if !self.decades.remove(&decade) {
            self.decades.insert(decade);
        }
    }

    /// Toggles a tag, as `category` for a tag chosen per category: the
    /// first category chooses the tag, the last one taken away drops it.
    /// With no category the tag is toggled by name, which also clears its
    /// categories. A tag chosen by name and then toggled as a category
    /// keeps only that category.
    pub fn toggle_tag(&mut self, tag: &str, category: Option<TagCategory>) {
        let Some(category) = category else {
            self.tag_categories.remove(tag);
            match self.tags.iter().position(|t| t == tag) {
                Some(at) => {
                    self.tags.remove(at);
                }
                None => self.tags.push(tag.to_owned()),
            }
            return;
        };
        let by_name = !self.tag_categories.contains_key(tag) && self.tags.iter().any(|t| t == tag);
        let mut chosen = if by_name {
            BTreeSet::from([category])
        } else {
            let mut chosen = self.tag_categories.remove(tag).unwrap_or_default();
            if !chosen.remove(&category) {
                chosen.insert(category);
            }
            chosen
        };
        if chosen.is_empty() {
            self.tags.retain(|t| t != tag);
            return;
        }
        if !self.tags.iter().any(|t| t == tag) {
            self.tags.push(tag.to_owned());
        }
        self.tag_categories
            .insert(tag.to_owned(), std::mem::take(&mut chosen));
    }

    /// Whether `tag` is chosen, as `category` when one is given: chosen in
    /// that category, or chosen by name alone (which counts in every one).
    pub fn has_tag(&self, tag: &str, category: Option<TagCategory>) -> bool {
        self.tags.iter().any(|t| t == tag)
            && match (category, self.tag_categories.get(tag)) {
                (Some(category), Some(categories)) => categories.contains(&category),
                _ => true,
            }
    }

    /// The removable chips for the chosen tags: "#horror", and for a tag
    /// chosen per category one per category ("#clint-eastwood · Director");
    /// each the tag and the category a click toggles.
    pub fn tag_chips(&self) -> Vec<(String, Option<TagCategory>)> {
        let mut chips = Vec::new();
        for tag in &self.tags {
            match self.tag_categories.get(tag) {
                Some(categories) => {
                    chips.extend(categories.iter().map(|&c| (tag.clone(), Some(c))));
                }
                None => chips.push((tag.clone(), None)),
            }
        }
        chips
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
    pub fn no_matches_text(
        &self,
        types: &BTreeSet<MediaType>,
        status: Option<EntryStatus>,
    ) -> String {
        let subject = if types.is_empty() {
            "entries".to_owned()
        } else {
            let names: Vec<String> = types.iter().map(|m| m.label().to_lowercase()).collect();
            join(&names, "or")
        };
        let mut conditions = Vec::new();
        if self.not_released {
            conditions.push("not out yet".to_owned());
        }
        if self.recent {
            conditions.push("added in the past month".to_owned());
        }
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
            let tags: Vec<String> = self
                .tag_chips()
                .iter()
                .map(|(tag, category)| tag_label(tag, category.as_slice()))
                .collect();
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

/// The parts of one series that are in the library, shown as one row in
/// a long list: the series name, the part the row sits where (the first
/// in the list's sort) and every part in series order.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesGroup<'a> {
    pub name: String,
    pub media: MediaType,
    pub lead: &'a Entry,
    pub parts: Vec<&'a Entry>,
}

impl<'a> SeriesGroup<'a> {
    /// The series name as shown, "Collection" dropped.
    pub fn title(&self) -> String {
        Series::display_name(&self.name)
    }

    pub fn done_count(&self) -> usize {
        self.parts
            .iter()
            .filter(|p| p.status == EntryStatus::Done)
            .count()
    }

    pub fn all_done(&self) -> bool {
        self.done_count() == self.parts.len()
    }

    /// "2008–2024", or "2008".
    pub fn year_span(&self) -> Option<String> {
        let mut years: Vec<u16> = self.parts.iter().filter_map(|p| p.year()).collect();
        years.sort_unstable();
        match (years.first(), years.last()) {
            (Some(first), Some(last)) if first == last => Some(first.to_string()),
            (Some(first), Some(last)) => Some(format!("{first}–{last}")),
            _ => None,
        }
    }

    /// "4 movies".
    pub fn count_label(&self) -> String {
        format!("{} {}", self.parts.len(), Series::nouns(self.media).1)
    }

    /// "4 movies · 2008–2024".
    pub fn subtitle(&self) -> String {
        match self.year_span() {
            Some(span) => format!("{} · {span}", self.count_label()),
            None => self.count_label(),
        }
    }

    /// "2 of 4 done", or "All done".
    pub fn done_label(&self) -> String {
        if self.all_done() {
            "All done".into()
        } else {
            format!("{} of {} done", self.done_count(), self.parts.len())
        }
    }

    /// The part whose cover stands for the series: the one marked, else
    /// the first in series order.
    pub fn cover_entry(&self) -> &'a Entry {
        self.parts
            .iter()
            .copied()
            .find(|p| p.series.as_ref().is_some_and(|s| s.cover == Some(true)))
            .unwrap_or(self.parts[0])
    }
}

/// One row of the library list: an entry, or a series as one row.
#[derive(Debug, Clone, PartialEq)]
pub enum Row<'a> {
    Entry(&'a Entry),
    Series(SeriesGroup<'a>),
}

impl<'a> Row<'a> {
    /// The entry the row sorts and files by.
    pub fn lead(&self) -> &'a Entry {
        match self {
            Row::Entry(entry) => entry,
            Row::Series(group) => group.lead,
        }
    }
}

/// Whether a status segment groups series: All, Done and DNF do; Want and
/// In progress show the single entries, since there you are picking what
/// to watch next.
pub fn groups_series_for(status: Option<EntryStatus>) -> bool {
    matches!(
        status,
        None | Some(EntryStatus::Done) | Some(EntryStatus::Dnf)
    )
}

/// `sorted` with the parts of a series that are in the library (two or
/// more) folded into one row, sitting where its first-sorting part would.
pub fn group_series<'a>(sorted: &[&'a Entry]) -> Vec<Row<'a>> {
    let key_of = |entry: &Entry| {
        entry
            .series
            .as_ref()
            .filter(|_| entry.can_have_series())
            .map(|s| (entry.media, Series::display_name(&s.name).to_lowercase()))
    };
    let mut rows = Vec::new();
    let mut done: Vec<(MediaType, String)> = Vec::new();
    for &entry in sorted {
        let Some(key) = key_of(entry) else {
            rows.push(Row::Entry(entry));
            continue;
        };
        let parts: Vec<&Entry> = sorted
            .iter()
            .copied()
            .filter(|e| key_of(e).as_ref() == Some(&key))
            .collect();
        if parts.len() < 2 {
            rows.push(Row::Entry(entry));
            continue;
        }
        if done.contains(&key) {
            continue;
        }
        done.push(key);
        rows.push(Row::Series(SeriesGroup {
            name: entry.series.as_ref().expect("keyed").name.clone(),
            media: entry.media,
            lead: entry,
            parts: in_series_order(parts),
        }));
    }
    rows
}

/// `parts` by their number in the series, unnumbered ones last, ties by
/// year and title.
pub fn in_series_order(mut parts: Vec<&Entry>) -> Vec<&Entry> {
    parts.sort_by(|a, b| {
        let number = |e: &Entry| e.series.as_ref().and_then(|s| s.number);
        match (number(a), number(b)) {
            (Some(x), Some(y)) if x != y => x.cmp(&y),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            _ => a
                .year()
                .cmp(&b.year())
                .then(a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        }
    });
    parts
}

/// A section of a long list: its header, the rail's short label and its
/// entries, in the list's order.
#[derive(Debug, Clone, PartialEq)]
pub struct ListSection<'a> {
    pub label: String,
    pub short: String,
    pub entries: Vec<&'a Entry>,
}

/// A section of a long list of rows (series folded), by each row's lead.
#[derive(Debug, Clone, PartialEq)]
pub struct RowSection<'a> {
    pub label: String,
    pub short: String,
    pub rows: Vec<Row<'a>>,
}

/// [`sections`] over rows: a series row files where its lead does.
pub fn row_sections<'a>(sort: Sort, rows: &[Row<'a>], today: Date) -> Vec<RowSection<'a>> {
    let mut sections: Vec<RowSection<'a>> = Vec::new();
    for row in rows {
        let (label, short) = section_key(sort, row.lead(), today);
        match sections.last_mut() {
            Some(last) if last.label == label => last.rows.push(row.clone()),
            _ => sections.push(RowSection {
                label,
                short,
                rows: vec![row.clone()],
            }),
        }
    }
    sections
}

/// The phone's long-list sections of `entries`, already in `sort` order:
/// decades for the year orders (Older before 1960, undated last), months
/// of this year and then years for newest added, first letters for the
/// title order (# for digits), and the ratings down to Unrated. Entries in
/// a row with the same key share a section.
pub fn sections<'a>(sort: Sort, entries: &[&'a Entry], today: Date) -> Vec<ListSection<'a>> {
    let mut sections: Vec<ListSection<'a>> = Vec::new();
    for &entry in entries {
        let (label, short) = section_key(sort, entry, today);
        match sections.last_mut() {
            Some(last) if last.label == label => last.entries.push(entry),
            _ => sections.push(ListSection {
                label,
                short,
                entries: vec![entry],
            }),
        }
    }
    sections
}

fn section_key(sort: Sort, entry: &Entry, today: Date) -> (String, String) {
    match sort {
        Sort::TitleAz => {
            let initial = title_initial(&entry.title).to_string();
            (initial.clone(), initial)
        }
        Sort::YearNewest | Sort::YearOldest => match entry.year() {
            None => ("Not released yet".into(), "None".into()),
            Some(year) if year < 1960 => ("Older".into(), "Older".into()),
            Some(year) => {
                let decade = format!("{}s", year / 10 * 10);
                (decade.clone(), decade)
            }
        },
        Sort::NewestAdded if entry.added.year == today.year => {
            let month = entry.added.month_name();
            (month.into(), month[..3].into())
        }
        Sort::NewestAdded => {
            let year = entry.added.year.to_string();
            (year.clone(), year)
        }
        Sort::RatingHighest => match entry.rating {
            Some(rating) => (rating.to_string(), rating.to_string()),
            None => ("Unrated".into(), "None".into()),
        },
    }
}

/// The letter a title files under: its first letter or digit, in capitals,
/// with "#" for a digit; punctuation in front is skipped.
pub fn title_initial(title: &str) -> char {
    match title.chars().find(|c| c.is_alphanumeric()) {
        Some(c) if c.is_ascii_digit() => '#',
        Some(c) => c.to_uppercase().next().unwrap_or(c),
        None => '#',
    }
}

/// The lowest rating Top rated lists.
pub const TOP_RATED: u8 = 9;

/// The overview's lists, each with a "See all".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    InProgress,
    ComingSoon,
    RecentlyAdded,
    TopRated,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::InProgress => "In progress",
            Section::ComingSoon => "Coming soon",
            Section::RecentlyAdded => "Recently added",
            Section::TopRated => "Top rated",
        }
    }
}

/// What All shows with nothing narrowing it: "now and next".
pub struct Overview<'a> {
    /// Newest added first.
    pub in_progress: Vec<&'a Entry>,
    /// Soonest release first.
    pub coming: Vec<&'a Entry>,
    /// Added in the past month, newest first.
    pub recent: Vec<&'a Entry>,
    /// Rated 9 or 10, newest added first.
    pub top: Vec<&'a Entry>,
    pub plan: Option<ActivePlan<'a>>,
}

impl<'a> Overview<'a> {
    pub fn of(library: &'a Library, today: Date) -> Overview<'a> {
        let main = || library.entries.iter().filter(|e| !e.seasonal_only);
        let newest = |mut list: Vec<&'a Entry>| {
            Sort::NewestAdded.apply(&mut list);
            list
        };
        let mut coming: Vec<&Entry> = library
            .entries
            .iter()
            .filter(|e| is_coming(e, today))
            .collect();
        coming.sort_by_key(|e| e.released.map(|r| r.available_from()));
        let since = today.month_before();
        Overview {
            in_progress: newest(
                main()
                    .filter(|e| e.status == EntryStatus::InProgress)
                    .collect(),
            ),
            coming,
            recent: newest(main().filter(|e| e.added >= since).collect()),
            top: newest(
                main()
                    .filter(|e| e.rating.is_some_and(|r| r >= TOP_RATED))
                    .collect(),
            ),
            plan: ActivePlan::of(library, today),
        }
    }

    pub fn list(&self, section: Section) -> &[&'a Entry] {
        match section {
            Section::InProgress => &self.in_progress,
            Section::ComingSoon => &self.coming,
            Section::RecentlyAdded => &self.recent,
            Section::TopRated => &self.top,
        }
    }
}

/// The plan the overview's card shows: the first active one (started, or
/// its first date has come, and not finished).
pub struct ActivePlan<'a> {
    pub plan: &'a Plan,
    pub done: usize,
    pub total: usize,
    /// The next two items not done: dated ones by date first, then the
    /// rest in plan order.
    pub next_up: Vec<&'a Entry>,
    /// Days until the plan's last date, when it has dates and it is ahead.
    pub days_left: Option<i64>,
}

impl<'a> ActivePlan<'a> {
    pub fn of(library: &'a Library, today: Date) -> Option<ActivePlan<'a>> {
        let plan = library
            .plans
            .iter()
            .find(|p| crate::plans::plan_state(p, today) == crate::plans::PlanState::Active)?;
        let mut open: Vec<_> = plan.items.iter().filter(|i| !i.done).collect();
        // A stable sort keeps plan order among undated items.
        open.sort_by_key(|i| i.when.map_or((1, None), |w| (0, Some(w.from))));
        let next_up = open
            .iter()
            .filter_map(|i| library.entry(&i.entry))
            .take(2)
            .collect();
        let last = plan.items.iter().filter_map(|i| i.when.map(|w| w.to)).max();
        Some(ActivePlan {
            plan,
            done: plan.items.iter().filter(|i| i.done).count(),
            total: plan.items.len(),
            next_up,
            days_left: last.map(|d| today.days_until(d)).filter(|&n| n >= 0),
        })
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

/// Wanted and not released yet, whatever the date's precision.
pub fn is_coming(entry: &Entry, today: Date) -> bool {
    entry.status == EntryStatus::Want && !entry.is_released_on(today)
}

/// The status chips for the picked types: a status shows when any of them
/// can have it, and every status shows with no type picked.
pub fn status_options(types: &BTreeSet<MediaType>) -> Vec<EntryStatus> {
    let all = EntryStatus::options_for(MediaType::Shows);
    all.iter()
        .copied()
        .filter(|s| {
            types.is_empty()
                || types
                    .iter()
                    .any(|&t| EntryStatus::options_for(t).contains(s))
        })
        .collect()
}

/// "Movie", "Album", "Song": an entry's kind, on covers and under titles.
pub fn kind_label(entry: &Entry) -> &'static str {
    match entry.media {
        MediaType::Movies => "Movie",
        MediaType::Shows => "Show",
        MediaType::Books => "Book",
        MediaType::Games => "Game",
        MediaType::Music if entry.kind == Some(EntryKind::Song) => "Song",
        MediaType::Music => "Album",
        MediaType::Anime if entry.kind == Some(EntryKind::Movie) => "Anime movie",
        MediaType::Anime => "Anime",
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

    fn today() -> Date {
        Date::parse("2026-10-19").unwrap()
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
        filter.toggle_tag("horror", None);
        filter.toggle_tag("halloween", None);
        assert!(
            library
                .entries
                .iter()
                .filter(|e| filter.matches(e, today()))
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
            .filter(|e| filter.matches(e, today()))
            .map(|e| e.year().unwrap())
            .collect();
        assert!(!years.is_empty() && years.iter().all(|y| (1980..2000).contains(y)));
    }

    #[test]
    fn seasonal_only_entries_need_the_switch() {
        let library = library();
        let seasonal = library.entries.iter().find(|e| e.seasonal_only).unwrap();
        let mut filter = Filter::default();
        assert!(!filter.matches(seasonal, today()));
        filter.include_seasonal = true;
        assert!(filter.matches(seasonal, today()));
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
        filter.toggle_tag("christmas", None);
        assert_eq!(
            filter.no_matches_text(
                &BTreeSet::from([MediaType::Books]),
                Some(EntryStatus::InProgress)
            ),
            "No books are both in progress and tagged #christmas. Remove a filter to see more."
        );
        filter.toggle_decade(Decade::Older);
        filter.toggle_decade(Decade::From(1990));
        assert_eq!(
            filter.no_matches_text(&BTreeSet::new(), None),
            "No entries are both from before 1960 or from the 1990s and tagged #christmas. \
             Remove a filter to see more."
        );
    }

    #[test]
    fn sections_follow_the_sort() {
        let library = library();
        let today = Date::parse("2026-10-19").unwrap();
        let mut entries: Vec<&Entry> = library.entries.iter().collect();

        Sort::TitleAz.apply(&mut entries);
        let by_title = sections(Sort::TitleAz, &entries, today);
        let labels: Vec<&str> = by_title.iter().map(|s| s.label.as_str()).collect();
        assert!(
            labels.windows(2).all(|w| w[0] < w[1]),
            "letters in order: {labels:?}"
        );
        assert!(by_title.iter().all(|s| s.label.len() == 1));

        Sort::YearNewest.apply(&mut entries);
        let by_year = sections(Sort::YearNewest, &entries, today);
        let labels: Vec<&str> = by_year.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"2020s"), "{labels:?}");
        assert!(labels.ends_with(&["Older"]) || labels.ends_with(&["Not released yet"]));

        Sort::RatingHighest.apply(&mut entries);
        let by_rating = sections(Sort::RatingHighest, &entries, today);
        assert_eq!(by_rating.first().map(|s| s.label.as_str()), Some("10"));
        let last = by_rating.last().unwrap();
        assert_eq!(
            (last.label.as_str(), last.short.as_str()),
            ("Unrated", "None")
        );

        Sort::NewestAdded.apply(&mut entries);
        let by_added = sections(Sort::NewestAdded, &entries, today);
        assert_eq!(by_added[0].label, "October");
        assert_eq!(by_added[0].short, "Oct");
    }

    #[test]
    fn titles_file_under_their_first_letter_or_hash() {
        assert_eq!(title_initial("the thing"), 'T');
        assert_eq!(title_initial("1984"), '#');
        assert_eq!(title_initial("\"Weird\" tales"), 'W');
        assert_eq!(title_initial("éclair"), 'É');
        assert_eq!(title_initial(""), '#');
    }

    #[test]
    fn title_sort_ignores_case_and_year_sorts_put_undated_last() {
        let library = library();
        let mut entries: Vec<&Entry> = library.entries.iter().collect();
        Sort::TitleAz.apply(&mut entries);
        let titles: Vec<String> = entries.iter().map(|e| e.title.to_lowercase()).collect();
        assert!(titles.windows(2).all(|w| w[0] <= w[1]), "{titles:?}");

        Sort::YearNewest.apply(&mut entries);
        let years: Vec<Option<u16>> = entries.iter().map(|e| e.year()).collect();
        let dated: Vec<u16> = years.iter().flatten().copied().collect();
        assert!(dated.windows(2).all(|w| w[0] >= w[1]), "{dated:?}");
        let first_undated = years.iter().position(Option::is_none);
        assert!(first_undated.is_none_or(|i| years[i..].iter().all(Option::is_none)));

        Sort::YearOldest.apply(&mut entries);
        let dated: Vec<u16> = entries.iter().filter_map(|e| e.year()).collect();
        assert!(dated.windows(2).all(|w| w[0] <= w[1]), "{dated:?}");
        // Same year: titles A to Z.
        let same: Vec<&&Entry> = entries.iter().filter(|e| e.year() == Some(2023)).collect();
        assert!(same.len() >= 2, "the fixture has several 2023 entries");
        assert!(
            same.windows(2)
                .all(|w| w[0].title.to_lowercase() <= w[1].title.to_lowercase())
        );
    }

    #[test]
    fn filters_combine_type_status_decade_and_rating() {
        let library = library();
        let today = Date::parse("2026-10-19").unwrap();
        let filter = Filter {
            decades: [Decade::From(2020)].into_iter().collect(),
            rating: Some((9, 10)),
            ..Filter::default()
        };
        let shown: Vec<&Entry> = library
            .entries
            .iter()
            .filter(|e| filter.matches(e, today))
            .collect();
        assert!(!shown.is_empty());
        for entry in shown {
            assert!(entry.year().is_some_and(|y| (2020..2030).contains(&y)));
            assert!(entry.rating.is_some_and(|r| r >= 9));
            assert!(
                !entry.seasonal_only,
                "seasonal-only stays out without the switch"
            );
        }
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

    #[test]
    fn coming_soon_and_recently_added_filter_by_date() {
        let library = library();
        let filter = Filter {
            not_released: true,
            ..Filter::default()
        };
        let coming: Vec<_> = library
            .entries
            .iter()
            .filter(|e| filter.matches(e, today()))
            .collect();
        assert!(!coming.is_empty());
        assert!(
            coming
                .iter()
                .all(|e| e.status == EntryStatus::Want && !e.is_released_on(today()))
        );

        let filter = Filter {
            recent: true,
            ..Filter::default()
        };
        let since = Date::parse("2026-09-19").unwrap();
        assert!(
            library
                .entries
                .iter()
                .filter(|e| filter.matches(e, today()))
                .all(|e| e.added >= since)
        );
    }

    #[test]
    fn status_options_cover_every_picked_type() {
        let movies = BTreeSet::from([MediaType::Movies]);
        assert!(!status_options(&movies).contains(&EntryStatus::InProgress));
        let music = BTreeSet::from([MediaType::Music]);
        assert_eq!(
            status_options(&music),
            [EntryStatus::Want, EntryStatus::Done]
        );
        let both = BTreeSet::from([MediaType::Movies, MediaType::Books]);
        assert_eq!(status_options(&both).len(), 4);
        assert_eq!(status_options(&BTreeSet::new()).len(), 4);
    }

    #[test]
    fn several_types_read_as_a_list() {
        let filter = Filter {
            recent: true,
            ..Filter::default()
        };
        let types = BTreeSet::from([MediaType::Movies, MediaType::Shows]);
        assert_eq!(
            filter.no_matches_text(&types, None),
            "No movies or shows are added in the past month. Remove a filter to see more."
        );
    }

    #[test]
    fn the_overview_lists_now_and_next() {
        let library = library();
        let overview = Overview::of(&library, today());
        assert!(
            overview
                .in_progress
                .iter()
                .all(|e| e.status == EntryStatus::InProgress)
        );
        assert_eq!(
            overview.coming.first().map(|e| e.title.as_str()),
            Some("Grand Theft Auto VI")
        );
        assert!(overview.recent.windows(2).all(|w| w[0].added >= w[1].added));
        assert!(overview.top.iter().all(|e| e.rating >= Some(9)));
    }

    #[test]
    fn the_active_plan_is_the_first_started_one_with_work_left() {
        let library = library();
        let active = ActivePlan::of(&library, today()).unwrap();
        assert_eq!(active.plan.id, "halloween-2026");
        assert_eq!((active.done, active.total), (2, 8));
        assert_eq!(active.days_left, Some(12));
        let next: Vec<_> = active.next_up.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(next, ["Mononoke", "The Mummy"]);
    }

    #[test]
    fn a_tag_chosen_per_category_matches_only_that_role() {
        let library = library();
        let baby = "movies/million-dollar-baby-2004";
        let ugly = "movies/the-good-the-bad-and-the-ugly-1966";
        let ids = |filter: &Filter| -> Vec<&str> {
            library
                .entries
                .iter()
                .filter(|e| filter.matches(e, today()))
                .map(|e| e.id.as_str())
                .collect()
        };
        let mut filter = Filter::default();
        filter.toggle_tag("clint-eastwood", Some(TagCategory::Directors));
        assert_eq!(ids(&filter), [baby]);
        assert_eq!(filter.count(), 1);
        assert!(filter.has_tag("clint-eastwood", Some(TagCategory::Directors)));
        assert!(!filter.has_tag("clint-eastwood", Some(TagCategory::Actors)));

        filter.toggle_tag("clint-eastwood", Some(TagCategory::Actors));
        assert_eq!(ids(&filter), [baby], "both roles: where he is both");
        assert_eq!(filter.count(), 2);
        assert_eq!(
            filter.no_matches_text(&BTreeSet::new(), None),
            "No entries are tagged #clint-eastwood · Director and #clint-eastwood · Actor. \
             Remove a filter to see more."
        );

        filter.toggle_tag("clint-eastwood", Some(TagCategory::Directors));
        assert_eq!(ids(&filter), [baby, ugly]);

        // The last role taken away drops the tag; by name it matches any role.
        filter.toggle_tag("clint-eastwood", Some(TagCategory::Actors));
        assert!(filter.is_empty());
        filter.toggle_tag("clint-eastwood", None);
        assert_eq!(ids(&filter), [baby, ugly]);
        assert!(filter.has_tag("clint-eastwood", Some(TagCategory::Directors)));
        filter.toggle_tag("clint-eastwood", Some(TagCategory::Actors));
        assert_eq!(
            filter.tag_chips(),
            [("clint-eastwood".to_owned(), Some(TagCategory::Actors))],
            "a tag chosen by name keeps only the role toggled"
        );
    }

    #[test]
    fn a_series_in_the_library_folds_into_one_row_where_its_first_part_sorts() {
        let library = library();
        let mut movies: Vec<&Entry> = library.entries_of(MediaType::Movies).collect();
        Sort::TitleAz.apply(&mut movies);
        let rows = group_series(&movies);
        assert_eq!(rows.len(), movies.len() - 1, "two parts became one row");
        let at = rows
            .iter()
            .position(|r| matches!(r, Row::Series(_)))
            .unwrap();
        let Row::Series(group) = &rows[at] else {
            unreachable!()
        };
        assert_eq!(group.title(), "The Lord of the Rings");
        assert_eq!(group.subtitle(), "2 movies · 2001–2002");
        assert_eq!(group.done_label(), "1 of 2 done");
        assert_eq!(group.parts[0].series.as_ref().unwrap().number, Some(1));
        // Where "The Lord of the Rings: The Fellowship…" sorts by title.
        assert_eq!(group.lead.id, movies[at].id);

        let mut books: Vec<&Entry> = library.entries_of(MediaType::Books).collect();
        Sort::YearNewest.apply(&mut books);
        let rows = group_series(&books);
        let Row::Series(berserk) = rows.iter().find(|r| matches!(r, Row::Series(_))).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(berserk.count_label(), "3 volumes");
        assert_eq!(berserk.lead.id, "books/berserk-vol-43-2025", "newest first");
        assert!(groups_series_for(None) && groups_series_for(Some(EntryStatus::Dnf)));
        assert!(!groups_series_for(Some(EntryStatus::Want)));
    }
}
