use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{Date, Released, TagCategory};

/// The six kinds of entry in the library. JSON uses the lowercase names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movies,
    Shows,
    Books,
    Games,
    Music,
    Anime,
}

impl MediaType {
    pub const ALL: [MediaType; 6] = [
        MediaType::Movies,
        MediaType::Shows,
        MediaType::Books,
        MediaType::Games,
        MediaType::Music,
        MediaType::Anime,
    ];

    /// The type's name in `library.json` and in entry ids: "movies".
    pub fn key(self) -> &'static str {
        match self {
            MediaType::Movies => "movies",
            MediaType::Shows => "shows",
            MediaType::Books => "books",
            MediaType::Games => "games",
            MediaType::Music => "music",
            MediaType::Anime => "anime",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaType::Movies => "Movies",
            MediaType::Shows => "Shows",
            MediaType::Books => "Books",
            MediaType::Games => "Games",
            MediaType::Music => "Music",
            MediaType::Anime => "Anime",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    Want,
    InProgress,
    Done,
    Dnf,
}

impl EntryStatus {
    /// The statuses an entry of `media` can have. A movie or an album is
    /// finished in one sitting, so neither is ever "in progress", and music
    /// has no DNF.
    pub fn options_for(media: MediaType) -> &'static [EntryStatus] {
        match media {
            MediaType::Movies => &[EntryStatus::Want, EntryStatus::Done, EntryStatus::Dnf],
            MediaType::Music => &[EntryStatus::Want, EntryStatus::Done],
            _ => &[
                EntryStatus::Want,
                EntryStatus::InProgress,
                EntryStatus::Done,
                EntryStatus::Dnf,
            ],
        }
    }
}

/// Formats a book is owned in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    Audiobook,
    Physical,
}

/// The `kind` field: whether a music entry is an album or a single song,
/// and whether an anime is a series (with seasons, like a show) or a movie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Album,
    Song,
    Series,
    Movie,
}

/// The series an entry is part of and its number in the source's
/// numbering; `cover` marks the one part whose cover stands for the series
/// in the library list, written only while true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    #[serde(default)]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<bool>,
}

/// Which of a tag's categories the tag has on one entry, for tags the tag
/// map lists under more than one category: `{ "clint-eastwood": ["Actors"] }`.
/// One key per such tag, a non-empty list in category order. The keys keep
/// the order the file has them in, so a save changes nothing it need not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagPicks(pub Vec<(String, Vec<TagCategory>)>);

impl TagPicks {
    pub fn get(&self, tag: &str) -> Option<&[TagCategory]> {
        self.0
            .iter()
            .find(|(name, _)| name == tag)
            .map(|(_, categories)| categories.as_slice())
    }

    /// Sets the pick for `tag`, or drops it with an empty list.
    pub fn set(&mut self, tag: &str, categories: Vec<TagCategory>) {
        self.remove(tag);
        if !categories.is_empty() {
            self.0.push((tag.to_owned(), categories));
        }
    }

    pub fn remove(&mut self, tag: &str) {
        self.0.retain(|(name, _)| name != tag);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for TagPicks {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (tag, categories) in &self.0 {
            map.serialize_entry(tag, categories)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for TagPicks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PicksVisitor;

        impl<'de> Visitor<'de> for PicksVisitor {
            type Value = TagPicks;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map from tag to its categories")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<TagPicks, A::Error> {
                let mut picks = Vec::new();
                while let Some((tag, categories)) = map.next_entry::<String, Vec<TagCategory>>()? {
                    picks.push((tag, categories));
                }
                Ok(TagPicks(picks))
            }
        }

        deserializer.deserialize_map(PicksVisitor)
    }
}

/// One title in the library, of any type.
///
/// The field order here is the key order in `library.json`. Common fields
/// are always written, even when null, so an edit changes as few lines as
/// possible. Type-specific fields are omitted when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(rename = "type")]
    pub media: MediaType,
    pub title: String,
    #[serde(default)]
    pub released: Option<Released>,
    #[serde(default)]
    pub creator: Option<String>,
    pub status: EntryStatus,
    /// 1 to 10.
    #[serde(default)]
    pub rating: Option<u8>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Written only on an entry with a tag in more than one category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_categories: Option<TagPicks>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub description: String,
    /// Hidden from the main lists unless "Include seasonal-only" is on.
    #[serde(default)]
    pub seasonal_only: bool,
    pub added: Date,
    /// The lookup id the entry was found by, "tmdb:movie:694", or null.
    #[serde(default)]
    pub source: Option<String>,
    /// The lookup id of another match whose cover the entry shows (an
    /// edition, an ebook), left out while the cover is the match's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_from: Option<String>,
    // Games.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    // Books.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<Vec<BookFormat>>,
    /// The page count of a book.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
    // Movies, books, games and anime movies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Series>,
    // Music (album, song) and anime (series, movie).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntryKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Vec<Track>>,
    // Shows and anime series: in season number order, omitted when the
    // show has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seasons: Option<Vec<Season>>,
}

impl Entry {
    /// The release year. A show with seasons counts from its first season,
    /// so sorting and the decade filter place it where it began.
    pub fn year(&self) -> Option<u16> {
        self.first_released().map(|released| released.year)
    }

    /// The entry's own date, or its first season's for a show whose dates
    /// live in its seasons.
    pub fn first_released(&self) -> Option<Released> {
        self.seasons
            .as_deref()
            .and_then(<[Season]>::first)
            .and_then(|season| season.released)
            .or(self.released)
    }

    /// False only when a release date is known and still in the future.
    pub fn is_released_on(&self, today: Date) -> bool {
        self.released.is_none_or(|r| r.is_released_on(today))
    }

    /// Whether the entry keeps its dates and statuses in seasons: a show,
    /// or an anime series.
    pub fn uses_seasons(&self) -> bool {
        self.media == MediaType::Shows
            || (self.media == MediaType::Anime && self.kind == Some(EntryKind::Series))
    }

    /// Whether the entry can be part of a series: movies, books, games and
    /// anime that are not a series of seasons.
    pub fn can_have_series(&self) -> bool {
        match self.media {
            MediaType::Movies | MediaType::Books | MediaType::Games => true,
            MediaType::Anime => self.kind != Some(EntryKind::Series),
            MediaType::Shows | MediaType::Music => false,
        }
    }

    /// The seasons, when there are any.
    pub fn season_list(&self) -> &[Season] {
        self.seasons.as_deref().unwrap_or_default()
    }

    /// Whether a season the daily check added is out and still unset, so
    /// the row and the inspector show "New season".
    pub fn has_new_season(&self, today: Date) -> bool {
        self.season_list()
            .iter()
            .any(|s| s.is_new() && s.is_released_on(today))
    }

    /// The years a show ran, first to last season out ("2022–2026", or
    /// "2022"); the first season's year while none is out; an entry
    /// without seasons gives its own year.
    pub fn years(&self, today: Date) -> Option<String> {
        let seasons = self.season_list();
        if seasons.is_empty() {
            return self.year().map(|y| y.to_string());
        }
        let out: Vec<u16> = seasons
            .iter()
            .filter(|s| s.is_released_on(today))
            .filter_map(|s| s.released.map(|r| r.year))
            .collect();
        match (out.first(), out.last()) {
            (Some(first), Some(last)) if first == last => Some(first.to_string()),
            (Some(first), Some(last)) => Some(format!("{first}–{last}")),
            _ => seasons[0].released.map(|r| r.year.to_string()),
        }
    }

    /// "3 seasons".
    pub fn season_count_label(&self) -> String {
        let n = self.season_list().len();
        format!("{n} {}", if n == 1 { "season" } else { "seasons" })
    }

    /// The entry with its status set by its seasons, when it has any.
    pub fn refresh_status(&mut self, today: Date) {
        if self.uses_seasons() && !self.season_list().is_empty() {
            self.status = Season::show_status(self.season_list(), today);
        }
    }

    /// The categories `tag` is picked in here, for a tag in more than one
    /// category: the entry's own pick, else `all`, the tag's categories,
    /// which is also the answer for a tag in one category.
    pub fn categories_of(&self, tag: &str, all: &[TagCategory]) -> Vec<TagCategory> {
        if all.len() < 2 {
            return all.to_vec();
        }
        match self
            .tag_categories
            .as_ref()
            .and_then(|picks| picks.get(tag))
        {
            Some(picked) if !picked.is_empty() => {
                all.iter().copied().filter(|c| picked.contains(c)).collect()
            }
            _ => all.to_vec(),
        }
    }

    /// Whether `tag` is on this entry as `category`: on it at all, and, for
    /// a tag with a pick, picked in that category. An entry without a pick
    /// matches any category.
    pub fn has_tag_as(&self, tag: &str, category: TagCategory) -> bool {
        if !self.tags.iter().any(|t| t == tag) {
            return false;
        }
        match self
            .tag_categories
            .as_ref()
            .and_then(|picks| picks.get(tag))
        {
            Some(picked) if !picked.is_empty() => picked.contains(&category),
            _ => true,
        }
    }

    /// Sets the pick for `tag`, dropping the map when it empties.
    pub fn set_tag_pick(&mut self, tag: &str, categories: Vec<TagCategory>) {
        let picks = self.tag_categories.get_or_insert_with(TagPicks::default);
        picks.set(tag, categories);
        if picks.is_empty() {
            self.tag_categories = None;
        }
    }
}

/// A rated track on an album entry. Only rated tracks are stored; `id` is
/// the lookup id, when known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub rating: u8,
}

/// One season of a show. `status` is null when not set and never changes
/// the show's own status. `fresh` (`"new"` in the file) marks a season the
/// daily check added; `manual` one whose date the owner set. Both are
/// written only when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Season {
    pub season: u32,
    #[serde(default)]
    pub released: Option<Released>,
    #[serde(default)]
    pub status: Option<EntryStatus>,
    #[serde(rename = "new", default, skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual: Option<bool>,
}

impl Season {
    /// Shown with a "New" label, and the show with a "New season" badge.
    pub fn is_new(&self) -> bool {
        self.fresh == Some(true) && self.status.is_none()
    }

    /// False only when a release date is known and still in the future.
    pub fn is_released_on(&self, today: Date) -> bool {
        self.released.is_none_or(|r| r.is_released_on(today))
    }

    /// "Season 2".
    pub fn name(&self) -> String {
        format!("Season {}", self.season)
    }

    /// The date as the rows show it: "Oct 9, 2026", "Coming 2027" while it
    /// is not out, or "Release date not set".
    pub fn date_label(&self, today: Date) -> String {
        match self.released {
            Some(released) if released.is_released_on(today) => released.long(),
            Some(released) => format!("Coming {}", released.long()),
            None => "Release date not set".into(),
        }
    }

    /// A show's status from its seasons. One season: that season's status,
    /// Want while not set. More: only seasons that are out and have a
    /// status count; any In progress is In progress; else any DNF is DNF;
    /// all Done is Done; Done and Want together are In progress; all Want,
    /// or nothing set, is Want.
    pub fn show_status(seasons: &[Season], today: Date) -> EntryStatus {
        if let [only] = seasons {
            return only.status.unwrap_or(EntryStatus::Want);
        }
        let set: Vec<EntryStatus> = seasons
            .iter()
            .filter(|s| s.is_released_on(today))
            .filter_map(|s| s.status)
            .collect();
        if set.is_empty() {
            return EntryStatus::Want;
        }
        if set.contains(&EntryStatus::InProgress) {
            return EntryStatus::InProgress;
        }
        if set.contains(&EntryStatus::Dnf) {
            return EntryStatus::Dnf;
        }
        if set.iter().all(|s| *s == set[0]) {
            return set[0];
        }
        EntryStatus::InProgress
    }
}

impl Series {
    /// The series name as shown: TMDB's "Collection" suffix dropped.
    pub fn display_name(name: &str) -> String {
        let name = name.trim();
        let lower = name.to_lowercase();
        match lower.strip_suffix("collection") {
            Some(rest) if rest.ends_with(' ') => name[..rest.len()].trim_end().to_owned(),
            _ => name.to_owned(),
        }
    }

    /// A part's title without the series name in front: "The Fellowship
    /// of the Ring" for "The Lord of the Rings: The Fellowship of the
    /// Ring", "Vol. 43" for "Berserk, Vol. 43".
    pub fn part_title(title: &str, series: &str) -> String {
        let lower = title.to_lowercase();
        let series = series.trim().to_lowercase();
        if let Some(rest) = lower.strip_prefix(&series) {
            let trimmed = rest.trim_start();
            let after = trimmed
                .strip_prefix([':', ',', '–', '—', '-'])
                .map(str::trim_start);
            if let Some(after) = after
                && !after.is_empty()
            {
                return title[title.len() - after.len()..].to_owned();
            }
        }
        title.to_owned()
    }

    /// What the parts of a series of `media` are called: "volumes" for
    /// books, "games", "movies" for movies and anime, "parts" otherwise.
    pub fn nouns(media: MediaType) -> (&'static str, &'static str) {
        match media {
            MediaType::Books => ("volume", "volumes"),
            MediaType::Games => ("game", "games"),
            MediaType::Movies | MediaType::Anime => ("movie", "movies"),
            MediaType::Shows | MediaType::Music => ("part", "parts"),
        }
    }

    /// Whether two series names are the same series, case and the
    /// "Collection" suffix aside.
    pub fn same(a: &str, b: &str) -> bool {
        Series::display_name(a).to_lowercase() == Series::display_name(b).to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tags: &[&str], picks: Option<TagPicks>) -> Entry {
        Entry {
            id: "movies/x-2000".into(),
            media: MediaType::Movies,
            title: "X".into(),
            released: None,
            creator: None,
            status: EntryStatus::Want,
            rating: None,
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            tag_categories: picks,
            note: String::new(),
            description: String::new(),
            seasonal_only: false,
            added: Date::parse("2026-10-01").unwrap(),
            source: None,
            cover_from: None,
            platform: None,
            formats: None,
            pages: None,
            series: None,
            kind: None,
            album: None,
            tracks: None,
            seasons: None,
        }
    }

    #[test]
    fn a_pick_narrows_a_tag_in_several_categories_and_nothing_else() {
        let both = [TagCategory::Directors, TagCategory::Actors];
        let picked = entry(
            &["clint-eastwood"],
            Some(TagPicks(vec![(
                "clint-eastwood".into(),
                vec![TagCategory::Actors],
            )])),
        );
        assert_eq!(
            picked.categories_of("clint-eastwood", &both),
            [TagCategory::Actors]
        );
        assert!(picked.has_tag_as("clint-eastwood", TagCategory::Actors));
        assert!(!picked.has_tag_as("clint-eastwood", TagCategory::Directors));

        let unpicked = entry(&["clint-eastwood"], None);
        assert_eq!(unpicked.categories_of("clint-eastwood", &both), both);
        assert!(unpicked.has_tag_as("clint-eastwood", TagCategory::Directors));
        assert!(!unpicked.has_tag_as("horror", TagCategory::Genres));
        assert_eq!(
            unpicked.categories_of("horror", &[TagCategory::Genres]),
            [TagCategory::Genres]
        );
    }

    fn season(n: u32, released: &str, status: Option<EntryStatus>) -> Season {
        Season {
            season: n,
            released: Released::parse(released),
            status,
            fresh: None,
            manual: None,
        }
    }

    #[test]
    fn a_shows_status_follows_its_seasons() {
        let today = Date::parse("2026-10-19").unwrap();
        let status = |seasons: &[Season]| Season::show_status(seasons, today);
        assert_eq!(status(&[season(1, "2024", None)]), EntryStatus::Want);
        assert_eq!(
            status(&[season(1, "2024", Some(EntryStatus::Dnf))]),
            EntryStatus::Dnf
        );
        let done = Some(EntryStatus::Done);
        let want = Some(EntryStatus::Want);
        assert_eq!(
            status(&[season(1, "2024", done), season(2, "2025", done)]),
            EntryStatus::Done
        );
        assert_eq!(
            status(&[season(1, "2024", done), season(2, "2025", want)]),
            EntryStatus::InProgress,
            "Done and Want together"
        );
        assert_eq!(
            status(&[season(1, "2024", done), season(2, "2027", want)]),
            EntryStatus::Done,
            "a season not out does not count"
        );
        assert_eq!(
            status(&[
                season(1, "2024", done),
                season(2, "2025", Some(EntryStatus::Dnf)),
                season(3, "2026", want)
            ]),
            EntryStatus::Dnf
        );
        assert_eq!(
            status(&[season(1, "2024", None), season(2, "2025", None)]),
            EntryStatus::Want
        );
    }

    #[test]
    fn the_years_run_from_the_first_to_the_last_season_out() {
        let today = Date::parse("2026-10-19").unwrap();
        let mut show = entry(&[], None);
        show.media = MediaType::Shows;
        show.seasons = Some(vec![
            season(1, "2022-09-01", Some(EntryStatus::Done)),
            season(2, "2026-01-16", None),
            season(3, "2027", None),
        ]);
        assert_eq!(show.years(today).as_deref(), Some("2022–2026"));
        assert_eq!(show.season_count_label(), "3 seasons");
        show.seasons = Some(vec![season(1, "2027", None)]);
        assert_eq!(show.years(today).as_deref(), Some("2027"));
        assert_eq!(show.season_count_label(), "1 season");
        assert_eq!(show.season_list()[0].date_label(today), "Coming 2027");
    }

    #[test]
    fn series_names_and_part_titles_are_cleaned() {
        assert_eq!(
            Series::display_name("Kung Fu Panda Collection"),
            "Kung Fu Panda"
        );
        assert_eq!(Series::display_name("Berserk"), "Berserk");
        assert_eq!(
            Series::part_title(
                "The Lord of the Rings: The Fellowship of the Ring",
                "The Lord of the Rings"
            ),
            "The Fellowship of the Ring"
        );
        assert_eq!(Series::part_title("Berserk, Vol. 43", "Berserk"), "Vol. 43");
        assert_eq!(
            Series::part_title("Kung Fu Panda 2", "Kung Fu Panda"),
            "Kung Fu Panda 2"
        );
        assert!(Series::same("Kung Fu Panda Collection", "kung fu panda"));
        assert_eq!(Series::nouns(MediaType::Books).1, "volumes");
    }

    #[test]
    fn setting_a_pick_adds_and_removes_the_map() {
        let mut e = entry(&["clint-eastwood"], None);
        e.set_tag_pick("clint-eastwood", vec![TagCategory::Actors]);
        assert!(e.tag_categories.is_some());
        e.set_tag_pick("clint-eastwood", Vec::new());
        assert!(e.tag_categories.is_none());
    }
}
