//! An entry's details and everything that changes one: the details pane
//! beside the list, the Add and Edit dialog with its tag picker, Add to
//! plan and the delete confirmation.

pub mod details;
mod dialogs;
mod form;
pub mod seasons;

pub use form::LOOKUP_ID;

use std::collections::{BTreeMap, BTreeSet};

use iced::widget::text_editor;
use iced::{Element, Task};

use crate::app::{App, Message as AppMessage};
use crate::lookup::{self, Match};
use crate::model::{
    BookFormat, Date, Entry, EntryKind, EntryStatus, Library, MediaType, PlanItem, Released,
    Season, Series, TagCategory, TagPicks, text,
};
use crate::music;
use crate::repo::Operation;
use crate::tags::{self, PickerMessage, PickerState};
use crate::ui;

/// The dialog open over the library, if any.
#[derive(Debug)]
pub enum Dialog {
    Form(Box<Form>),
    /// The details pane's "+ Tag": the tag picker alone, over the library,
    /// on a form that holds the entry's tags. Done saves them at once;
    /// X, Esc or Close drops the changes.
    Tags(Box<Form>),
    Delete {
        id: String,
    },
    AddToPlan {
        id: String,
    },
    /// "Season 2 of Carrie is no longer listed": the lookup dropped a
    /// season the owner had set.
    SeasonDropped {
        id: String,
        season: u32,
    },
}

/// The Add and Edit dialog's fields. Nothing reaches the library until
/// Save.
#[derive(Debug)]
pub struct Form {
    /// The entry being edited; `None` adds a new one.
    pub editing: Option<String>,
    pub media: MediaType,
    /// The lookup's query, and the match picked from its results.
    pub query: String,
    pub picked: Option<usize>,
    pub source: Option<String>,
    pub title: String,
    pub creator: String,
    pub released: String,
    /// Every platform chip offered, and the ones picked.
    pub platforms: Vec<String>,
    pub platforms_on: BTreeSet<String>,
    /// The "+ Other" platform being typed.
    pub other_platform: Option<String>,
    pub formats: BTreeSet<BookFormat>,
    pub kind: EntryKind,
    pub album: String,
    /// A show's seasons (and an anime series'), edited in place.
    pub seasons: Vec<SeasonDraft>,
    /// The season whose status menu is open.
    pub season_menu: Option<u32>,
    /// An anime's "Series or movie"; `None` for an anime from before kinds
    /// until the owner picks one.
    pub anime_kind: Option<EntryKind>,
    /// A book's page count, digits only.
    pub pages: String,
    pub series_name: String,
    pub series_number: String,
    pub status: EntryStatus,
    pub rating: Option<u8>,
    pub rating_open: bool,
    pub tags: Vec<String>,
    /// Which of a tag's categories it is ticked in on this entry, for tags
    /// in more than one category; a tag with no pick counts in all of them.
    pub tag_categories: BTreeMap<String, Vec<TagCategory>>,
    /// Tags created in this form with their categories, filed when it is
    /// saved.
    pub new_tags: Vec<(String, Vec<TagCategory>)>,
    /// Existing tags whose categories change when the form is saved
    /// ("+ Add to Actors", the categories dialog).
    pub recategorized: BTreeMap<String, Vec<TagCategory>>,
    /// The match's tag suggestions not taken yet.
    pub suggested: Vec<Suggestion>,
    /// The lookup id of another match whose cover the entry shows.
    pub cover_from: Option<String>,
    /// Pick a cover, open over the form.
    pub cover_dialog: bool,
    /// A game lookup narrowed to one platform, and its picker.
    pub platform_filter: Option<String>,
    pub platform_open: bool,
    pub platform_query: String,
    /// "Look up again": the lookup pane open in Edit.
    pub again: bool,
    /// A pick in Edit replaced the lookup's fields ("Details fetched
    /// online").
    pub fetched: bool,
    /// The year came from the ebook edition, so it may be a reprint's.
    pub ebook_note: bool,
    pub description: text_editor::Content,
    pub note: String,
    /// "Also add to plan", a plan id.
    pub also_plan: Option<String>,
    pub picker: Option<PickerState>,
    pub new_tag: Option<NewTag>,
    /// A tag's categories being edited (right-click › Categories… or Move
    /// to another group…).
    pub categories: Option<CategoriesDialog>,
    /// The phone's replace question, asked on Save (D12, D13).
    pub replace: Option<Replace>,
    /// The answer, once given: replace (true) or keep both (false).
    pub replace_answer: Option<bool>,
}

/// Saving music that overlaps what is saved.
#[derive(Debug, Clone)]
pub enum Replace {
    /// D12: an album whose songs are saved as entries.
    Songs { album: String, songs: Vec<String> },
    /// D13: a song whose album is saved.
    Album {
        album: String,
        song: String,
        adding: bool,
    },
}

/// A tag the lookup suggests: the tag, and the category it would go in
/// when it is a person, a language or a platform (`None` for a genre).
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub tag: String,
    pub category: Option<TagCategory>,
}

/// One season in the form: its number, the date as typed, and its
/// status. A season added here, or whose date was changed here, is saved
/// as manual.
#[derive(Debug, Clone, PartialEq)]
pub struct SeasonDraft {
    pub number: u32,
    pub released: String,
    pub status: Option<EntryStatus>,
    pub fresh: Option<bool>,
    pub manual: Option<bool>,
    /// The date as loaded, to tell a change.
    pub loaded: String,
}

impl SeasonDraft {
    fn from_season(season: &Season) -> SeasonDraft {
        let released = season.released.map(|r| r.to_string()).unwrap_or_default();
        SeasonDraft {
            number: season.season,
            released: released.clone(),
            status: season.status,
            fresh: season.fresh,
            manual: season.manual,
            loaded: released,
        }
    }

    /// A season added in the form: the next number, Not set, no date.
    fn new(number: u32) -> SeasonDraft {
        SeasonDraft {
            number,
            released: String::new(),
            status: None,
            fresh: None,
            manual: Some(true),
            loaded: String::new(),
        }
    }

    pub fn released(&self) -> Option<Released> {
        Released::parse(self.released.trim())
    }

    pub fn is_valid(&self) -> bool {
        self.released.trim().is_empty() || self.released().is_some()
    }

    fn to_season(&self) -> Season {
        let changed = self.released.trim() != self.loaded;
        Season {
            season: self.number,
            released: self.released(),
            status: self.status,
            fresh: self.fresh,
            manual: (self.manual == Some(true) || changed).then_some(true),
        }
    }
}

/// Confirming a new tag and picking its categories: one, or several of
/// one kind.
#[derive(Debug, Clone)]
pub struct NewTag {
    pub name: String,
    pub categories: Vec<TagCategory>,
}

/// Editing a tag's categories: the categories of its kind as chips, at
/// least one picked; or, for a tag of a kind with one category each,
/// exactly one of those groups, to move it.
#[derive(Debug, Clone)]
pub struct CategoriesDialog {
    pub tag: String,
    pub draft: Vec<TagCategory>,
    pub single: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    // The details pane.
    SetStatus(String, EntryStatus),
    SetRating(String, Option<u8>),
    ToggleDetailsRating,
    /// An album track's rating picker, by the track's title.
    ToggleTrackRating(String),
    RateTrack {
        album: String,
        title: String,
        rating: Option<u8>,
    },
    Add,
    /// Add with the type and the title typed: a series' "+ Add".
    AddWithTitle(MediaType, String),
    // The lookup pane: Look up again, the platform filter, the cover.
    LookAgain,
    CancelLookAgain,
    OpenCover,
    CloseCover,
    /// The cover of another match, or `None` for the match's own.
    PickCover(Option<String>),
    TogglePlatformPicker,
    PlatformQuery(String),
    PickPlatform(Option<String>),
    /// "Try again" in the lookup pane.
    RetryLookup,
    Edit(String),
    /// The details' "+ Tag": the tag picker alone.
    EditTags(String),
    /// The details' picker's Done: the tags as one change.
    SaveTags,
    /// The details' note as typed.
    NoteDraft {
        id: String,
        note: String,
    },
    /// Enter in the details' note.
    SaveNote,
    Delete(String),
    AddToPlan(String),
    Close,
    // The form.
    Type(MediaType),
    Query(String),
    Pick(usize),
    /// Moves the picked match up or down (the arrow keys).
    MovePick(isize),
    Manual,
    Title(String),
    Creator(String),
    Released(String),
    TogglePlatform(String),
    OpenOtherPlatform,
    OtherPlatform(String),
    AddOtherPlatform,
    ToggleFormat(BookFormat),
    Kind(EntryKind),
    Album(String),
    Status(EntryStatus),
    ToggleRating,
    Rate(Option<u8>),
    /// A tag's chip in the Tags box: the whole tag, or one role of a tag
    /// in several categories.
    RemoveTag(String, Option<TagCategory>),
    AddSuggested(String),
    Description(text_editor::Action),
    Note(String),
    AlsoPlan(Option<String>),
    Save,
    // The form's tag picker, the new-tag confirmation and the categories
    // dialog.
    OpenPicker,
    ClosePicker,
    Picker(PickerMessage),
    PickerClear,
    CreateTag,
    /// A category chip in the new-tag dialog: toggled within its kind; one
    /// of another kind starts over.
    NewTagCategory(TagCategory),
    ConfirmNewTag,
    CancelNewTag,
    /// A category chip in the categories dialog.
    CategoryDraft(TagCategory),
    SaveCategories,
    CancelCategories,
    // The season editor, the anime kind, pages and series.
    SeasonDate(u32, String),
    SeasonStatus(u32, Option<EntryStatus>),
    SeasonCatchUp(u32),
    FormSeasonMenu(Option<u32>),
    RemoveSeasonDraft(u32),
    AddSeason,
    AnimeKind(EntryKind),
    Pages(String),
    SeriesName(String),
    SeriesNumber(String),
    /// The replace question's answers.
    ReplaceKeepBoth,
    ReplaceConfirm,
    ReplaceCancel,
    // The other dialogs.
    ConfirmDelete,
    TogglePlan(String),
    // Seasons, from the inspector and the pages.
    SetSeasonStatus {
        id: String,
        season: u32,
        status: Option<EntryStatus>,
    },
    CatchUp {
        id: String,
        season: u32,
    },
    /// The dropped-season dialog's answers.
    KeepSeason,
    RemoveSeason,
}

impl Form {
    /// An empty form for a new entry of `media`.
    pub fn add(media: MediaType) -> Form {
        Form {
            editing: None,
            media,
            query: String::new(),
            picked: None,
            source: None,
            title: String::new(),
            creator: String::new(),
            released: String::new(),
            platforms: Vec::new(),
            platforms_on: BTreeSet::new(),
            other_platform: None,
            formats: BTreeSet::new(),
            kind: EntryKind::Album,
            album: String::new(),
            // A show filled in by hand starts with Season 1.
            seasons: if media == MediaType::Shows {
                vec![SeasonDraft::new(1)]
            } else {
                Vec::new()
            },
            season_menu: None,
            anime_kind: None,
            pages: String::new(),
            series_name: String::new(),
            series_number: String::new(),
            status: EntryStatus::Want,
            rating: None,
            rating_open: false,
            tags: Vec::new(),
            tag_categories: BTreeMap::new(),
            new_tags: Vec::new(),
            recategorized: BTreeMap::new(),
            suggested: Vec::new(),
            cover_from: None,
            cover_dialog: false,
            platform_filter: None,
            platform_open: false,
            platform_query: String::new(),
            again: false,
            fetched: false,
            ebook_note: false,
            description: text_editor::Content::new(),
            note: String::new(),
            also_plan: None,
            picker: None,
            new_tag: None,
            categories: None,
            replace: None,
            replace_answer: None,
        }
    }

    /// The form filled in from an existing entry.
    pub fn edit(entry: &Entry) -> Form {
        let platforms = text::split_platforms(entry.platform.as_deref());
        Form {
            editing: Some(entry.id.clone()),
            source: entry.source.clone(),
            cover_from: entry.cover_from.clone(),
            title: entry.title.clone(),
            creator: entry.creator.clone().unwrap_or_default(),
            released: entry.released.map(|r| r.to_string()).unwrap_or_default(),
            platforms_on: platforms.iter().cloned().collect(),
            platforms,
            formats: entry.formats.iter().flatten().copied().collect(),
            kind: entry.kind.unwrap_or(EntryKind::Album),
            album: entry.album.clone().unwrap_or_default(),
            seasons: entry
                .season_list()
                .iter()
                .map(SeasonDraft::from_season)
                .collect(),
            anime_kind: entry
                .kind
                .filter(|k| matches!(k, EntryKind::Series | EntryKind::Movie)),
            pages: entry.pages.map(|p| p.to_string()).unwrap_or_default(),
            series_name: entry
                .series
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            series_number: entry
                .series
                .as_ref()
                .and_then(|s| s.number)
                .map(|n| n.to_string())
                .unwrap_or_default(),
            status: entry.status,
            rating: entry.rating,
            tags: entry.tags.clone(),
            tag_categories: entry
                .tag_categories
                .iter()
                .flat_map(|picks| picks.0.iter().cloned())
                .collect(),
            description: text_editor::Content::with_text(&entry.description),
            note: entry.note.clone(),
            ..Form::add(entry.media)
        }
    }

    /// The lookup's results for the current query and type.
    pub fn matches(&self) -> Vec<&'static Match> {
        lookup::search_on(self.media, &self.query, self.platform_filter.as_deref())
    }

    /// The matches whose covers Pick a cover offers: the match's own
    /// first, then the other matches for the title.
    pub fn cover_choices(&self) -> Vec<&'static Match> {
        let own = self
            .source
            .as_deref()
            .and_then(|source| lookup::CATALOG.iter().find(|m| m.source == source));
        own.into_iter()
            .chain(lookup::cover_choices(
                self.media,
                self.title.trim(),
                self.source.as_deref(),
            ))
            .collect()
    }

    /// The platforms "Your platforms" lists: the default from Settings,
    /// then the library's games' platforms, the most used first.
    pub fn your_platforms(library: &Library, default: Option<&str>) -> Vec<String> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for entry in library.entries_of(MediaType::Games) {
            for platform in text::split_platforms(entry.platform.as_deref()) {
                match counts.iter_mut().find(|(p, _)| *p == platform) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((platform, 1)),
                }
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut yours: Vec<String> = default.map(str::to_owned).into_iter().collect();
        for (platform, _) in counts {
            if !yours.contains(&platform) {
                yours.push(platform);
            }
        }
        yours
    }

    /// Replaces the lookup's fields with a match's ("Look up again"): the
    /// creator, the date, the description, the kind and the platforms;
    /// the title, note, status, tags and formats stay. New tags are
    /// offered, and a show whose seasons are all manual gets the match's.
    fn refill(&mut self, found: &Match, library: &Library) {
        self.source = Some(found.source.to_owned());
        self.creator = found.creator.to_owned();
        self.released = found.released.to_owned();
        self.description = text_editor::Content::with_text(found.description);
        if self.media == MediaType::Music {
            self.kind = found.kind.unwrap_or(EntryKind::Album);
        }
        for platform in found.platforms {
            let platform = text::normalize_platform(platform);
            if !self.platforms.contains(&platform) {
                self.platforms.push(platform);
            }
        }
        if self.media == MediaType::Shows && self.seasons.iter().all(|s| s.manual == Some(true)) {
            self.seasons = vec![SeasonDraft {
                released: found.released.to_owned(),
                manual: None,
                ..SeasonDraft::new(1)
            }];
        }
        self.suggest(found, library);
        self.fetched = true;
    }

    /// Ticks the match's tags the library has and offers the rest: the
    /// genres, the top billed cast as actors (a person the library has
    /// under another people category is offered with the new role), the
    /// language (never English, never Japanese on an anime) and the
    /// streaming platform (movies and shows; a show's network only when
    /// the library has its tag).
    fn suggest(&mut self, found: &Match, library: &Library) {
        self.suggested.clear();
        let offer = |form: &mut Form, tag: String, category: Option<TagCategory>| {
            if form.tags.contains(&tag) || form.suggested.iter().any(|s| s.tag == tag) {
                return;
            }
            let categories = library.categories_of(&tag);
            match category {
                Some(category) if categories.contains(&category) => {
                    form.tick(&tag, Some(category), library);
                }
                Some(_) if !categories.is_empty() => {
                    form.suggested.push(Suggestion { tag, category });
                }
                None if !categories.is_empty() => form.tick(&tag, None, library),
                _ => form.suggested.push(Suggestion { tag, category }),
            }
        };
        for tag in found.tags {
            offer(self, (*tag).to_owned(), None);
        }
        for name in found.cast {
            offer(self, text::slugify(name), Some(TagCategory::Actors));
        }
        if let Some(language) = found.language
            && language != "english"
            && !(self.media == MediaType::Anime && language == "japanese")
        {
            offer(self, language.to_owned(), Some(TagCategory::Languages));
        }
        if let Some(network) = found.network
            && TagCategory::Streaming.used_for(self.media)
        {
            let known = library
                .categories_of(network)
                .contains(&TagCategory::Streaming);
            if known {
                self.tick(network, Some(TagCategory::Streaming), library);
            } else if self.media == MediaType::Movies {
                offer(self, network.to_owned(), Some(TagCategory::Streaming));
            }
        }
    }

    /// Whether the entry keeps its dates and statuses in seasons: a show,
    /// or an anime series.
    pub fn uses_seasons(&self) -> bool {
        self.media == MediaType::Shows
            || (self.media == MediaType::Anime && self.anime_kind == Some(EntryKind::Series))
    }

    /// Whether the entry can be in a series: movies, books, games and
    /// anime movies (an anime without a kind too).
    pub fn can_have_series(&self) -> bool {
        match self.media {
            MediaType::Movies | MediaType::Books | MediaType::Games => true,
            MediaType::Anime => self.anime_kind != Some(EntryKind::Series),
            MediaType::Shows | MediaType::Music => false,
        }
    }

    /// The seasons as they would be saved.
    pub fn season_list(&self) -> Vec<Season> {
        self.seasons.iter().map(SeasonDraft::to_season).collect()
    }

    /// The status the seasons give the show, for the read-only row.
    pub fn status_from_seasons(&self, today: Date) -> EntryStatus {
        Season::show_status(&self.season_list(), today)
    }

    /// Season 1 added when the type turns into one with seasons.
    fn ensure_season(&mut self) {
        if self.uses_seasons() && self.seasons.is_empty() {
            self.seasons.push(SeasonDraft::new(1));
        }
    }

    /// Fills the form from a match. Every field stays editable. A game
    /// starts with the default platform picked when it came out on it,
    /// and with every platform otherwise.
    fn fill(&mut self, found: &Match, library: &Library, default_platform: Option<&str>) {
        self.source = Some(found.source.to_owned());
        self.title = found.title.to_owned();
        self.creator = found.creator.to_owned();
        self.released = found.released.to_owned();
        self.description = text_editor::Content::with_text(found.description);
        self.kind = found.kind.unwrap_or(EntryKind::Album);
        // A show from the lookup starts with Season 1 on the match's date
        // (phase 4 lists its TMDB seasons here).
        if self.media == MediaType::Shows {
            self.seasons = vec![SeasonDraft {
                released: found.released.to_owned(),
                manual: None,
                ..SeasonDraft::new(1)
            }];
        }
        self.platforms = found
            .platforms
            .iter()
            .map(|p| text::normalize_platform(p))
            .collect();
        // The platform the lookup was narrowed to is picked; else the
        // default from Settings when the game came out on it; else all.
        let searched = self
            .platform_filter
            .clone()
            .filter(|p| self.platforms.contains(p));
        self.platforms_on = match (searched, default_platform) {
            (Some(platform), _) => BTreeSet::from([platform]),
            (None, Some(default)) if self.platforms.iter().any(|p| p == default) => {
                BTreeSet::from([default.to_owned()])
            }
            _ => self.platforms.iter().cloned().collect(),
        };
        self.cover_from = None;
        self.ebook_note = found.ebook_dated;
        // Tags the library has go on at once; new ones are offered.
        self.tags.clear();
        self.tag_categories.clear();
        self.suggest(found, library);
    }

    /// The categories of `tag` as they will be once the entry is saved.
    pub fn categories_of(&self, tag: &str, library: &Library) -> Vec<TagCategory> {
        if let Some((_, categories)) = self.new_tags.iter().find(|(t, _)| t == tag) {
            return categories.clone();
        }
        if let Some(categories) = self.recategorized.get(tag) {
            return categories.clone();
        }
        library.categories_of(tag)
    }

    /// Whether `tag` is on the entry as `category` (`None`: at all).
    pub fn has_tag_as(&self, tag: &str, category: Option<TagCategory>, library: &Library) -> bool {
        if !self.tags.iter().any(|t| t == tag) {
            return false;
        }
        let Some(category) = category else {
            return true;
        };
        let categories = self.categories_of(tag, library);
        if !categories.contains(&category) {
            return false;
        }
        categories.len() < 2
            || self
                .tag_categories
                .get(tag)
                .is_none_or(|picked| picked.contains(&category))
    }

    /// Puts `tag` on the entry, as `category` when one is given: a tag in
    /// more than one category is ticked in that one; a tag the library has
    /// only under other categories of the same kind gets this one too, as
    /// "+ Add to Actors" in the picker does; any other tag goes on as it
    /// is.
    pub fn tick(&mut self, tag: &str, category: Option<TagCategory>, library: &Library) {
        let mut categories = self.categories_of(tag, library);
        if let Some(category) = category
            && let Some(&first) = categories.first()
            && !categories.contains(&category)
            && first.shares_kind_with(category)
        {
            // On the entry already: it keeps the categories it had here.
            if self.tags.iter().any(|t| t == tag) && !self.tag_categories.contains_key(tag) {
                self.tag_categories
                    .insert(tag.to_owned(), categories.clone());
            }
            categories = TagCategory::ALL
                .into_iter()
                .filter(|c| *c == category || categories.contains(c))
                .collect();
            match self.new_tags.iter_mut().find(|(t, _)| t == tag) {
                Some((_, filed)) => *filed = categories.clone(),
                None => {
                    self.recategorized
                        .insert(tag.to_owned(), categories.clone());
                }
            }
        }
        if !self.tags.iter().any(|t| t == tag) {
            self.tags.push(tag.to_owned());
        }
        if categories.len() < 2 {
            self.tag_categories.remove(tag);
            return;
        }
        let picked = match (self.tag_categories.get(tag), category) {
            (Some(picked), _) => picked.clone(),
            (None, None) => categories.clone(),
            (None, Some(_)) => Vec::new(),
        };
        self.tag_categories.insert(
            tag.to_owned(),
            categories
                .iter()
                .copied()
                .filter(|&c| Some(c) == category || picked.contains(&c))
                .collect(),
        );
    }

    /// Takes `tag` off the entry, or one of its roles: the last role
    /// unticked takes the tag off.
    pub fn untick(&mut self, tag: &str, category: Option<TagCategory>, library: &Library) {
        let categories = self.categories_of(tag, library);
        if let Some(category) = category
            && categories.len() > 1
        {
            let mut picked = self
                .tag_categories
                .get(tag)
                .cloned()
                .unwrap_or_else(|| categories.clone());
            picked.retain(|c| *c != category);
            if !picked.is_empty() {
                self.tag_categories.insert(tag.to_owned(), picked);
                return;
            }
        }
        self.tags.retain(|t| t != tag);
        self.tag_categories.remove(tag);
        self.new_tags.retain(|(t, _)| t != tag);
    }

    /// The roles shown after `tag` in the Tags box: its ticked categories
    /// for a tag in several, nothing for a tag in one.
    pub fn roles_on(&self, tag: &str, library: &Library) -> Vec<TagCategory> {
        let categories = self.categories_of(tag, library);
        if categories.len() < 2 {
            return Vec::new();
        }
        match self.tag_categories.get(tag) {
            Some(picked) => categories
                .into_iter()
                .filter(|c| picked.contains(c))
                .collect(),
            None => categories,
        }
    }

    /// Every tag per category as the picker lists them: the library's,
    /// with the tags created or recategorized in this form.
    pub fn picker_groups(&self, library: &Library) -> Vec<(TagCategory, Vec<String>)> {
        let mut groups: BTreeMap<TagCategory, Vec<String>> = library.tags.clone();
        for (tag, categories) in self.recategorized.iter().chain(&self.new_tags_map()) {
            for names in groups.values_mut() {
                names.retain(|n| n != tag);
            }
            for &category in categories {
                groups.entry(category).or_default().push(tag.clone());
            }
        }
        groups
            .into_iter()
            .filter(|(_, names)| !names.is_empty())
            .collect()
    }

    fn new_tags_map(&self) -> BTreeMap<String, Vec<TagCategory>> {
        self.new_tags.iter().cloned().collect()
    }

    /// Where `tag` is used, for the categories dialog.
    pub fn usage(&self, tag: &str, library: &Library) -> String {
        if self.new_tags.iter().any(|(t, _)| t == tag) {
            return "Not used on any entry yet.".into();
        }
        library.tag_usage(tag)
    }

    /// The tag changes made in the picker, applied right before the entry
    /// is saved, as on the phone: new tags on the entry are filed, then
    /// the categories changed are set.
    fn tag_operations(&self, library: &Library) -> Vec<Operation> {
        let mut operations: Vec<Operation> = self
            .new_tags
            .iter()
            .filter(|(name, _)| self.tags.contains(name))
            .map(|(name, categories)| Operation::CreateTag {
                name: name.clone(),
                categories: categories.clone(),
            })
            .collect();
        operations.extend(
            self.recategorized
                .iter()
                .filter(|(name, categories)| library.categories_of(name) != **categories)
                .map(|(name, categories)| Operation::SetTagCategories {
                    name: name.clone(),
                    categories: categories.clone(),
                }),
        );
        operations
    }

    /// Opens `category` in the picker, after a tag was created or filed
    /// into it.
    fn open_group(&mut self, category: TagCategory) {
        if let Some(picker) = &mut self.picker {
            // A group holding a tag on this entry is open by default.
            picker.open(category, true);
        }
    }

    /// Re-keys the entry's pick for `tag` after its categories change from
    /// `before` to `after`, as the phone does on save: the roles it kept,
    /// else the tag comes off (one category left keeps it as that one).
    fn rekey(&mut self, tag: &str, before: &[TagCategory], after: &[TagCategory]) {
        if !self.tags.iter().any(|t| t == tag) {
            return;
        }
        if after.len() == 1 {
            self.tag_categories.remove(tag);
            return;
        }
        let had = self
            .tag_categories
            .get(tag)
            .cloned()
            .unwrap_or_else(|| before.to_vec());
        let kept: Vec<TagCategory> = after.iter().copied().filter(|c| had.contains(c)).collect();
        if kept.is_empty() {
            self.tags.retain(|t| t != tag);
            self.tag_categories.remove(tag);
        } else {
            self.tag_categories.insert(tag.to_owned(), kept);
        }
    }

    /// The release text as a date, when it is one.
    pub fn released(&self) -> Option<Released> {
        Released::parse(self.released.trim())
    }

    /// An empty release field is fine; anything else must be a date.
    pub fn released_is_valid(&self) -> bool {
        self.released.trim().is_empty() || self.released().is_some()
    }

    pub fn can_save(&self) -> bool {
        !self.title.trim().is_empty()
            && self.released_is_valid()
            && self.seasons.iter().all(SeasonDraft::is_valid)
            && self.pages.trim().bytes().all(|b| b.is_ascii_digit())
            && self
                .series_number
                .trim()
                .bytes()
                .all(|b| b.is_ascii_digit())
    }

    /// Platform chips: the form's own, then any the library uses.
    pub fn platform_options(&self, library: &Library) -> Vec<String> {
        let mut options = self.platforms.clone();
        for entry in &library.entries {
            for platform in text::split_platforms(entry.platform.as_deref()) {
                if !options.iter().any(|p| p.eq_ignore_ascii_case(&platform)) {
                    options.push(platform);
                }
            }
        }
        options
    }

    /// The entry the form describes, on top of `base` when editing.
    fn entry(&self, base: Option<&Entry>, library: &Library, today: crate::model::Date) -> Entry {
        let released = self.released();
        // A new show's id takes Season 1's year, since it has no date of
        // its own.
        let id_year = if self.uses_seasons() {
            self.seasons
                .first()
                .and_then(|s| s.released())
                .map(|r| r.year)
        } else {
            released.map(|r| r.year)
        };
        let creator = Some(self.creator.trim().to_owned()).filter(|c| !c.is_empty());
        let id = match base {
            Some(entry) => entry.id.clone(),
            None => text::entry_id(
                self.media,
                self.title.trim(),
                id_year,
                creator.as_deref(),
                library.entries.iter().map(|e| e.id.as_str()),
            ),
        };
        // Picked platforms keep the order of their chips.
        let platform: Vec<&str> = self
            .platforms
            .iter()
            .filter(|p| self.platforms_on.contains(*p))
            .map(String::as_str)
            .collect();
        let is = |media| self.media == media;
        let mut entry = base.cloned().unwrap_or(Entry {
            id: id.clone(),
            media: self.media,
            title: String::new(),
            released: None,
            creator: None,
            status: self.status,
            rating: None,
            tags: Vec::new(),
            tag_categories: None,
            note: String::new(),
            description: String::new(),
            seasonal_only: false,
            added: today,
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
        });
        // Changing the type clears the lookup id; editing a title never
        // changes the id.
        if base.is_some_and(|b| b.media != self.media) {
            entry.source = None;
        } else {
            // The form's: the entry's own, or a "Look up again" pick.
            entry.source = self.source.clone();
        }
        entry.cover_from = self.cover_from.clone();
        entry.media = self.media;
        entry.title = self.title.trim().to_owned();
        entry.creator = creator;
        // A show has no date of its own: its seasons carry the dates.
        entry.released = if self.uses_seasons() { None } else { released };
        entry.status = self.status;
        entry.rating = self.rating;
        entry.tags = self.tags.clone();
        // The pick is written for every tag in several categories, as the
        // phone does whenever it saves such an entry.
        let picks: Vec<(String, Vec<TagCategory>)> = self
            .tags
            .iter()
            .filter_map(|tag| {
                let categories = self.categories_of(tag, library);
                (categories.len() > 1).then(|| {
                    let picked = self
                        .tag_categories
                        .get(tag)
                        .cloned()
                        .unwrap_or_else(|| categories.clone());
                    (tag.clone(), picked)
                })
            })
            .collect();
        entry.tag_categories = (!picks.is_empty()).then_some(TagPicks(picks));
        entry.note = self.note.trim().to_owned();
        entry.description = self.description.text().trim().to_owned();
        entry.platform =
            (is(MediaType::Games) && !platform.is_empty()).then(|| platform.join(", "));
        entry.formats = (is(MediaType::Books) && !self.formats.is_empty())
            .then(|| self.formats.iter().copied().collect());
        // Music is an album or a song; an anime a series or a movie, once
        // picked; any other type has no kind.
        entry.kind = match self.media {
            MediaType::Music => Some(self.kind),
            MediaType::Anime => self.anime_kind,
            _ => None,
        };
        entry.pages = (is(MediaType::Books))
            .then(|| self.pages.trim().parse::<u32>().ok())
            .flatten()
            .filter(|p| *p > 0);
        let series_name = self.series_name.trim();
        entry.series = (self.can_have_series() && !series_name.is_empty()).then(|| {
            // The cover flag stays while the series does.
            let cover = base
                .and_then(|b| b.series.as_ref())
                .filter(|s| Series::same(&s.name, series_name))
                .and_then(|s| s.cover);
            Series {
                name: series_name.to_owned(),
                number: self.series_number.trim().parse().ok(),
                cover,
            }
        });
        let album = self.album.trim();
        entry.album = (is(MediaType::Music) && self.kind == EntryKind::Song && !album.is_empty())
            .then(|| album.to_owned());
        if !is(MediaType::Music) || self.kind == EntryKind::Song {
            entry.tracks = None;
        }
        if entry.uses_seasons() {
            let mut seasons = self.season_list();
            seasons.sort_by_key(|s| s.season);
            entry.seasons = (!seasons.is_empty()).then_some(seasons);
            entry.refresh_status(today);
        } else {
            entry.seasons = None;
        }
        entry
    }
}

/// Opens the dialog for `message` or changes the open one. Returns any
/// task to run (none for now).
pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    if let Message::Picker(PickerMessage::Jump(offset)) = &message {
        return tags::jump(*offset);
    }
    match message {
        Message::SetStatus(id, status) => app.apply(Operation::SetStatus { id, status }),
        Message::SetRating(id, rating) => {
            app.apply(Operation::SetRating { id, rating });
            app.library.details_rating_open = false;
        }
        Message::ToggleDetailsRating => {
            app.library.details_rating_open = !app.library.details_rating_open
        }
        Message::ToggleTrackRating(title) => {
            let open = &mut app.library.track_rating_open;
            *open = if open.as_deref() == Some(title.as_str()) {
                None
            } else {
                Some(title)
            };
        }
        Message::RateTrack {
            album,
            title,
            rating,
        } => {
            app.apply(Operation::SetTrackRating {
                album,
                title,
                rating,
            });
            app.library.track_rating_open = None;
        }
        Message::Add => {
            let media = app.library.single().unwrap_or(MediaType::Movies);
            app.dialog = Some(Dialog::Form(Box::new(Form::add(media))));
        }
        Message::AddWithTitle(media, title) => {
            let mut form = Form::add(media);
            form.query = title.clone();
            form.title = title;
            app.dialog = Some(Dialog::Form(Box::new(form)));
        }
        Message::SetSeasonStatus { id, season, status } => {
            app.library.season_menu = None;
            app.apply(Operation::SetSeasonStatus {
                id,
                season,
                status,
                today: app.today,
            });
        }
        Message::CatchUp { id, season } => {
            app.library.season_menu = None;
            app.apply(Operation::MarkSeasonsDone {
                id,
                before: season,
                today: app.today,
            });
        }
        Message::RetryLookup => app.lookup = lookup::LookupState::Results,
        Message::KeepSeason => {
            if let Some(Dialog::SeasonDropped { id, season }) = app.dialog.take() {
                app.apply(Operation::KeepSeason { id, season });
            }
        }
        Message::RemoveSeason => {
            if let Some(Dialog::SeasonDropped { id, season }) = app.dialog.take() {
                app.apply(Operation::RemoveSeason {
                    id,
                    season,
                    today: app.today,
                });
            }
        }
        Message::Edit(id) | Message::EditTags(id) if app.repo.library().entry(&id).is_none() => {}
        Message::Edit(id) => {
            let entry = app.repo.library().entry(&id).expect("checked above");
            app.dialog = Some(Dialog::Form(Box::new(Form::edit(entry))));
        }
        Message::EditTags(id) => {
            let entry = app.repo.library().entry(&id).expect("checked above");
            let mut form = Form::edit(entry);
            form.picker = Some(PickerState::default());
            app.dialog = Some(Dialog::Tags(Box::new(form)));
        }
        Message::SaveTags => {
            if let Some(Dialog::Tags(form)) = app.dialog.take() {
                save_tags(app, &form);
            }
        }
        // Ctrl+Enter in the details' picker is Done.
        Message::Save if matches!(app.dialog, Some(Dialog::Tags(_))) => {
            return update(app, Message::SaveTags);
        }
        Message::NoteDraft { id, note } => app.library.note_draft = Some((id, note)),
        Message::SaveNote => save_note(app),
        Message::Delete(id) => app.dialog = Some(Dialog::Delete { id }),
        Message::AddToPlan(id) => app.dialog = Some(Dialog::AddToPlan { id }),
        Message::Close => app.dialog = None,
        Message::ConfirmDelete => {
            if let Some(Dialog::Delete { id }) = app.dialog.take() {
                if app.library.selected.as_deref() == Some(id.as_str()) {
                    app.library.selected = None;
                }
                app.apply(Operation::DeleteEntry { id });
            }
        }
        Message::TogglePlan(plan) => {
            if let Some(Dialog::AddToPlan { id }) = &app.dialog {
                let id = id.clone();
                let library = app.repo.library();
                let in_plan = library
                    .plan(&plan)
                    .is_some_and(|p| p.item_for(&id).is_some());
                let done = library
                    .entry(&id)
                    .is_some_and(|e| e.status == EntryStatus::Done);
                let operation = if in_plan {
                    Operation::RemovePlanItem { plan, entry: id }
                } else {
                    Operation::SetPlanItem {
                        plan,
                        item: new_item(id, done),
                    }
                };
                app.apply(operation);
            }
        }
        message => {
            let library = app.repo.library().clone();
            let today = app.today;
            let platform = app.settings.platform.clone();
            // The details' picker takes the picker's messages the same way;
            // it never saves from here.
            let Some(Dialog::Form(form) | Dialog::Tags(form)) = &mut app.dialog else {
                return Task::none();
            };
            if let Some(operations) =
                update_form(form, message, &library, today, platform.as_deref())
            {
                let selected = operations.iter().find_map(|op| match op {
                    Operation::AddEntry(entry) | Operation::UpdateEntry(entry) => {
                        Some(entry.id.clone())
                    }
                    _ => None,
                });
                app.dialog = None;
                for operation in operations {
                    app.apply(operation);
                }
                app.library.selected = selected;
            }
        }
    }
    // Popovers draw above everything, so none stays open under a dialog.
    if app.dialog.is_some() {
        app.library.close_popovers();
    }
    Task::none()
}

/// The details' picker's Done: the tags the form holds go on the entry
/// as one change, after the new tags are filed and the categories changed
/// in the picker are applied, as the form does on Save. Nothing is written
/// when nothing changed.
fn save_tags(app: &mut App, form: &Form) {
    let library = app.repo.library().clone();
    let Some(base) = form.editing.as_deref().and_then(|id| library.entry(id)) else {
        return;
    };
    let entry = form.entry(Some(base), &library, app.today);
    let mut operations = form.tag_operations(&library);
    if !operations.is_empty()
        || entry.tags != base.tags
        || entry.tag_categories != base.tag_categories
    {
        operations.push(Operation::SetTags {
            id: base.id.clone(),
            tags: entry.tags,
            tag_categories: entry.tag_categories,
        });
    }
    for operation in operations {
        app.apply(operation);
    }
}

/// Saves the note typed in the details pane when it differs from the
/// saved one: on Enter, and before the pane moves on to another entry, a
/// page, a screen or the form.
pub fn save_note(app: &mut App) {
    let Some((id, note)) = app.library.note_draft.take() else {
        return;
    };
    let note = note.trim().to_owned();
    if app
        .repo
        .library()
        .entry(&id)
        .is_some_and(|entry| entry.note != note)
    {
        app.apply(Operation::SetNote { id, note });
    }
}

/// A new plan item; an entry already done is a repeat (a rewatch or
/// re-read).
fn new_item(entry: String, done: bool) -> PlanItem {
    PlanItem {
        entry,
        done: false,
        repeat: done,
        note: String::new(),
        when: None,
    }
}

/// Changes the form; on Save, returns the operations that make the change.
/// `platform` is the default platform from Settings → Games.
fn update_form(
    form: &mut Form,
    message: Message,
    library: &Library,
    today: crate::model::Date,
    platform: Option<&str>,
) -> Option<Vec<Operation>> {
    match message {
        Message::Type(media) => {
            form.media = media;
            form.picked = None;
            // The lookup id, a picked cover and the platform go with the
            // type.
            form.cover_from = None;
            form.platform_filter = None;
            form.platform_open = false;
            form.ebook_note = false;
            form.ensure_season();
        }
        Message::Query(query) => {
            form.query = query;
            form.picked = None;
        }
        Message::Pick(index) => {
            if let Some(found) = form.matches().get(index).copied() {
                form.picked = Some(index);
                if form.editing.is_some() {
                    form.refill(found, library);
                } else {
                    form.fill(found, library, platform);
                }
            }
        }
        Message::LookAgain => {
            form.again = true;
            form.query = form.title.clone();
            form.picked = None;
            form.platform_filter = form
                .platforms
                .iter()
                .find(|p| form.platforms_on.contains(*p))
                .cloned();
        }
        Message::CancelLookAgain => {
            form.again = false;
            form.picked = None;
            form.platform_open = false;
        }
        Message::OpenCover => {
            if !form.title.trim().is_empty() && form.source.is_some() {
                form.cover_dialog = true;
            }
        }
        Message::CloseCover => form.cover_dialog = false,
        Message::PickCover(id) => form.cover_from = id,
        Message::TogglePlatformPicker => {
            form.platform_open = !form.platform_open;
            form.platform_query.clear();
        }
        Message::PlatformQuery(query) => form.platform_query = query,
        Message::PickPlatform(platform) => {
            form.platform_filter = platform;
            form.platform_open = false;
            form.platform_query.clear();
            form.picked = None;
        }
        Message::MovePick(step) => {
            let count = form.matches().len();
            if count > 0 {
                let next = form.picked.map_or(0, |i| {
                    (i as isize + step).rem_euclid(count as isize) as usize
                });
                return update_form(form, Message::Pick(next), library, today, platform);
            }
        }
        Message::Manual => {
            form.query.clear();
            form.picked = None;
            form.source = None;
        }
        Message::Title(title) => form.title = title,
        Message::Creator(creator) => form.creator = creator,
        Message::Released(released) => {
            form.released = released;
            form.ebook_note = false;
        }
        Message::TogglePlatform(platform) => {
            if !form.platforms_on.remove(&platform) {
                if !form.platforms.contains(&platform) {
                    form.platforms.push(platform.clone());
                }
                form.platforms_on.insert(platform);
            }
        }
        Message::OpenOtherPlatform => form.other_platform = Some(String::new()),
        Message::OtherPlatform(name) => form.other_platform = Some(name),
        Message::AddOtherPlatform => {
            let name = form
                .other_platform
                .take()
                .map(|n| text::normalize_platform(&n))
                .unwrap_or_default();
            if !name.is_empty() {
                if !form.platforms.contains(&name) {
                    form.platforms.push(name.clone());
                }
                form.platforms_on.insert(name);
            }
        }
        Message::ToggleFormat(format) => {
            if !form.formats.remove(&format) {
                form.formats.insert(format);
            }
        }
        Message::Kind(kind) => form.kind = kind,
        Message::Album(album) => form.album = album,
        Message::Status(status) => {
            form.status = status;
            // A one-season show's status is Season 1's.
            if form.uses_seasons()
                && let [only] = form.seasons.as_mut_slice()
            {
                only.status = Some(status);
                only.fresh = None;
            }
        }
        Message::SeasonDate(number, date) => {
            if let Some(season) = form.seasons.iter_mut().find(|s| s.number == number) {
                season.released = date;
            }
        }
        Message::SeasonStatus(number, status) => {
            form.season_menu = None;
            if let Some(season) = form.seasons.iter_mut().find(|s| s.number == number) {
                season.status = status;
                if status.is_some() {
                    season.fresh = None;
                }
            }
            if form.seasons.len() == 1 {
                form.status = status.unwrap_or(EntryStatus::Want);
            }
        }
        Message::SeasonCatchUp(number) => {
            form.season_menu = None;
            for season in &mut form.seasons {
                if season.number < number
                    && season.released().is_none_or(|r| r.is_released_on(today))
                {
                    season.status = Some(EntryStatus::Done);
                    season.fresh = None;
                }
            }
        }
        Message::FormSeasonMenu(menu) => form.season_menu = menu,
        Message::RemoveSeasonDraft(number) => {
            form.seasons.retain(|s| s.number != number);
            form.season_menu = None;
        }
        Message::AddSeason => {
            let next = form.seasons.iter().map(|s| s.number).max().unwrap_or(0) + 1;
            form.seasons.push(SeasonDraft::new(next));
        }
        Message::AnimeKind(kind) => {
            form.anime_kind = Some(kind);
            form.ensure_season();
        }
        Message::Pages(pages) => form.pages = pages,
        Message::SeriesName(name) => form.series_name = name,
        Message::SeriesNumber(number) => form.series_number = number,
        Message::ToggleRating => form.rating_open = !form.rating_open,
        Message::Rate(rating) => {
            form.rating = rating;
            form.rating_open = false;
        }
        Message::RemoveTag(tag, category) => form.untick(&tag, category, library),
        Message::AddSuggested(tag) => {
            let category = form
                .suggested
                .iter()
                .find(|s| s.tag == tag)
                .and_then(|s| s.category);
            form.suggested.retain(|s| s.tag != tag);
            if library.all_tags().any(|t| t == tag) {
                form.tick(&tag, category, library);
            } else {
                // A new tag still goes through the confirmation, on the
                // category the suggestion names.
                form.new_tag = Some(NewTag {
                    name: tag,
                    categories: vec![category.unwrap_or(TagCategory::default_for(form.media))],
                });
            }
        }
        Message::Description(action) => form.description.perform(action),
        Message::Note(note) => form.note = note,
        Message::AlsoPlan(plan) => form.also_plan = plan,
        Message::OpenPicker => form.picker = Some(PickerState::default()),
        Message::ClosePicker => form.picker = None,
        Message::Picker(message) => {
            if let Some(picker) = &mut form.picker {
                picker.update(&message);
            }
            match message {
                PickerMessage::Toggle(tag, category) => {
                    if form.has_tag_as(&tag, category, library) {
                        form.untick(&tag, category, library);
                    } else {
                        form.tick(&tag, category, library);
                    }
                }
                PickerMessage::AddTo(tag, category) => {
                    form.tick(&tag, Some(category), library);
                    form.open_group(category);
                }
                PickerMessage::Categories(tag) => {
                    let draft = form.categories_of(&tag, library);
                    let single = draft.first().is_none_or(|c| c.kind().is_none());
                    form.categories = Some(CategoriesDialog { tag, draft, single });
                }
                _ => {}
            }
        }
        Message::PickerClear => {
            form.tags.clear();
            form.tag_categories.clear();
            form.new_tags.clear();
        }
        Message::CreateTag => {
            let name = form
                .picker
                .as_ref()
                .map(|p| text::normalize_tag(&p.query))
                .unwrap_or_default();
            if !name.is_empty() {
                form.new_tag = Some(NewTag {
                    name,
                    categories: vec![TagCategory::default_for(form.media)],
                });
            }
        }
        Message::NewTagCategory(category) => {
            if let Some(new_tag) = &mut form.new_tag {
                let same_kind = new_tag
                    .categories
                    .first()
                    .is_some_and(|first| first.shares_kind_with(category));
                if new_tag.categories.contains(&category) {
                    new_tag.categories.retain(|c| *c != category);
                } else if same_kind {
                    new_tag.categories.push(category);
                    new_tag.categories.sort();
                } else {
                    new_tag.categories = vec![category];
                }
            }
        }
        Message::ConfirmNewTag => {
            if form
                .new_tag
                .as_ref()
                .is_some_and(|n| n.categories.is_empty())
            {
                return None;
            }
            if let Some(NewTag { name, categories }) = form.new_tag.take() {
                if !form.tags.contains(&name) {
                    form.tags.push(name.clone());
                }
                if categories.len() > 1 {
                    form.tag_categories.insert(name.clone(), categories.clone());
                }
                for &category in &categories {
                    form.open_group(category);
                }
                form.new_tags.push((name, categories));
                if let Some(picker) = &mut form.picker {
                    picker.query.clear();
                }
            }
        }
        Message::CancelNewTag => form.new_tag = None,
        Message::CategoryDraft(category) => {
            if let Some(dialog) = &mut form.categories {
                if dialog.single {
                    dialog.draft = vec![category];
                } else if dialog.draft.contains(&category) {
                    if dialog.draft.len() > 1 {
                        dialog.draft.retain(|c| *c != category);
                    }
                } else {
                    dialog.draft.push(category);
                    dialog.draft.sort();
                }
            }
        }
        Message::SaveCategories => {
            if let Some(CategoriesDialog { tag, draft, .. }) = form.categories.take() {
                let before = form.categories_of(&tag, library);
                if before != draft {
                    match form.new_tags.iter_mut().find(|(t, _)| *t == tag) {
                        Some((_, filed)) => *filed = draft.clone(),
                        None if library.categories_of(&tag) == draft => {
                            form.recategorized.remove(&tag);
                        }
                        None => {
                            form.recategorized.insert(tag.clone(), draft.clone());
                        }
                    }
                    form.rekey(&tag, &before, &draft);
                }
                if let Some(&first) = draft.first() {
                    form.open_group(first);
                }
            }
        }
        Message::CancelCategories => form.categories = None,
        Message::ReplaceKeepBoth | Message::ReplaceConfirm => {
            form.replace = None;
            form.replace_answer = Some(matches!(message, Message::ReplaceConfirm));
            return update_form(form, Message::Save, library, today, platform);
        }
        Message::ReplaceCancel => {
            form.replace = None;
            form.replace_answer = None;
        }
        Message::Save => {
            if !form.can_save() {
                return None;
            }
            let base = form.editing.as_deref().and_then(|id| library.entry(id));
            let mut entry = form.entry(base, library, today);

            // Music that overlaps what is saved asks first.
            let songs = if music::is_song(&entry) {
                Vec::new()
            } else {
                music::songs_from_album(&library.entries, &entry)
            };
            let album = if music::is_song(&entry) {
                music::album_of_song(&library.entries, &entry)
            } else {
                None
            };
            let overlaps =
                entry.media == MediaType::Music && (!songs.is_empty() || album.is_some());
            if overlaps && form.replace_answer.is_none() {
                form.replace = Some(match album {
                    Some(album) => Replace::Album {
                        album: album.title.clone(),
                        song: entry.title.clone(),
                        adding: base.is_none(),
                    },
                    None => Replace::Songs {
                        album: entry.title.clone(),
                        songs: songs.iter().map(|s| s.title.clone()).collect(),
                    },
                });
                return None;
            }
            let mut removals = Vec::new();
            if overlaps && form.replace_answer == Some(true) {
                if entry.media == MediaType::Music && !music::is_song(&entry) {
                    entry = music::absorb_songs(entry, &songs);
                }
                let gone = songs.iter().copied().chain(album);
                removals = gone
                    .map(|e| Operation::DeleteEntry { id: e.id.clone() })
                    .collect();
            }

            let mut operations = form.tag_operations(library);
            let id = entry.id.clone();
            let done = entry.status == EntryStatus::Done;
            operations.push(if base.is_some() {
                Operation::UpdateEntry(entry)
            } else {
                Operation::AddEntry(entry)
            });
            if let Some(plan) = form.also_plan.clone()
                && library
                    .plan(&plan)
                    .is_some_and(|p| p.item_for(&id).is_none())
            {
                operations.push(Operation::SetPlanItem {
                    plan,
                    item: new_item(id, done),
                });
            }
            operations.extend(removals);
            return Some(operations);
        }
        _ => {}
    }
    None
}

/// Esc: the innermost layer of the open dialog first. False when no
/// dialog was open.
pub fn dismiss_top(app: &mut App) -> bool {
    match &mut app.dialog {
        Some(Dialog::Tags(form)) => {
            if form.new_tag.take().is_none()
                && form.categories.take().is_none()
                && !form.picker.as_mut().is_some_and(PickerState::dismiss_inner)
            {
                app.dialog = None;
            }
            true
        }
        Some(Dialog::Form(form)) => {
            if form.replace.take().is_none()
                && form.new_tag.take().is_none()
                && form.categories.take().is_none()
                && form.season_menu.take().is_none()
                && !std::mem::take(&mut form.cover_dialog)
                && !std::mem::take(&mut form.platform_open)
                && !form.picker.as_mut().is_some_and(PickerState::dismiss_inner)
                && form.picker.take().is_none()
                && !std::mem::take(&mut form.rating_open)
            {
                app.dialog = None;
            }
            true
        }
        Some(_) => {
            app.dialog = None;
            true
        }
        None => std::mem::take(&mut app.library.details_rating_open),
    }
}

/// Enter: the open dialog's main action. In the form only the tag
/// picker's layers take Enter; Ctrl+Enter saves.
pub fn confirm(app: &mut App) -> Task<AppMessage> {
    let message = match &app.dialog {
        Some(Dialog::Form(form) | Dialog::Tags(form)) if form.new_tag.is_some() => {
            Message::ConfirmNewTag
        }
        Some(Dialog::Form(form) | Dialog::Tags(form)) if form.categories.is_some() => {
            Message::SaveCategories
        }
        Some(Dialog::Tags(_)) => Message::SaveTags,
        Some(Dialog::Form(form)) if form.picker.is_some() => Message::ClosePicker,
        Some(Dialog::Form(form)) if form.picked.is_none() && !form.matches().is_empty() => {
            Message::Pick(0)
        }
        Some(Dialog::Delete { .. }) => Message::ConfirmDelete,
        Some(Dialog::AddToPlan { .. }) => Message::Close,
        Some(Dialog::SeasonDropped { .. }) => Message::KeepSeason,
        _ => return Task::none(),
    };
    update(app, message)
}

/// The open dialog over `base`.
pub fn overlay<'a>(app: &'a App, base: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let dialog = app.dialog.as_ref().map(|dialog| match dialog {
        Dialog::Form(form) => form::view(app, form),
        Dialog::Tags(form) => form::tags_view(app, form),
        Dialog::Delete { id } => dialogs::delete(app, id),
        Dialog::AddToPlan { id } => dialogs::add_to_plan(app, id),
        Dialog::SeasonDropped { id, season } => dialogs::season_dropped(app, id, *season),
    });
    ui::modal(base, dialog, AppMessage::Entry(Message::Close), &tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Date;
    use crate::screens::Screen;

    fn app() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app
    }

    fn send(app: &mut App, message: Message) {
        let _ = update(app, message);
    }

    fn form(app: &App) -> &Form {
        match &app.dialog {
            Some(Dialog::Form(form) | Dialog::Tags(form)) => form,
            other => panic!("no form: {other:?}"),
        }
    }

    #[test]
    fn without_a_default_platform_every_fetched_platform_is_picked() {
        let mut app = app();
        app.settings.platform = None;
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Games));
        send(&mut app, Message::Query("castlevania".into()));
        send(&mut app, Message::Pick(0));
        assert_eq!(form(&app).platforms_on.len(), 4);

        // A default the game did not come out on changes nothing either.
        app.settings.platform = Some("Steam Deck".into());
        send(&mut app, Message::Pick(0));
        assert_eq!(form(&app).platforms_on.len(), 4);
    }

    #[test]
    fn a_picked_match_fills_the_form_and_saves_a_new_entry() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Games));
        send(&mut app, Message::Query("castlevania".into()));
        send(&mut app, Message::Pick(0));
        let filled = form(&app);
        assert_eq!(filled.title, "Castlevania: Belmont's Curse");
        assert!(
            filled.tags.contains(&"metroidvania".to_owned()),
            "a known tag goes on"
        );
        assert!(
            filled.suggested.iter().any(|s| s.tag == "soulslike"),
            "a new one is offered"
        );
        send(&mut app, Message::AlsoPlan(Some("halloween-2026".into())));
        send(&mut app, Message::Save);

        assert!(app.dialog.is_none());
        let id = "games/castlevania-belmonts-curse-2026";
        let library = app.repo.library();
        let entry = library.entry(id).expect("saved");
        assert_eq!(entry.added, app.today);
        assert_eq!(entry.source.as_deref(), Some("igdb:305611"));
        // The default platform from Settings, since the game came out on it.
        assert_eq!(entry.platform.as_deref(), Some("PC"));
        assert!(
            library
                .plan("halloween-2026")
                .unwrap()
                .item_for(id)
                .is_some()
        );
        assert_eq!(app.library.selected.as_deref(), Some(id));
    }

    #[test]
    fn a_new_tag_is_filed_under_its_category_only_on_save() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Title("Night of the Comet".into()));
        send(&mut app, Message::OpenPicker);
        send(
            &mut app,
            Message::Picker(PickerMessage::Query("B Movie".into())),
        );
        send(&mut app, Message::CreateTag);
        assert_eq!(
            form(&app).new_tag.as_ref().unwrap().categories,
            [TagCategory::Genres]
        );
        send(&mut app, Message::NewTagCategory(TagCategory::Other));
        send(&mut app, Message::ConfirmNewTag);
        assert!(
            app.repo.library().category_of("b-movie").is_none(),
            "not before Save"
        );
        send(&mut app, Message::ClosePicker);
        send(&mut app, Message::Save);
        assert_eq!(
            app.repo.library().category_of("b-movie"),
            Some(TagCategory::Other)
        );
    }

    #[test]
    fn editing_keeps_the_id_and_a_type_change_clears_the_source() {
        let mut app = app();
        let id = "movies/the-mummy-1999";
        send(&mut app, Message::Edit(id.into()));
        send(&mut app, Message::Title("The Mummy (1999)".into()));
        send(&mut app, Message::Save);
        assert_eq!(
            app.repo.library().entry(id).unwrap().title,
            "The Mummy (1999)"
        );

        let library = app.repo.library().clone();
        let mut edited = library.entry(id).unwrap().clone();
        edited.source = Some("tmdb:movie:564".into());
        app.apply(Operation::UpdateEntry(edited));
        send(&mut app, Message::Edit(id.into()));
        send(&mut app, Message::Type(MediaType::Shows));
        send(&mut app, Message::Save);
        let entry = app.repo.library().entry(id).unwrap();
        assert_eq!(
            (entry.media, entry.source.as_deref()),
            (MediaType::Shows, None)
        );
    }

    #[test]
    fn a_bad_release_date_or_no_title_blocks_save() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Released("2026-13".into()));
        assert!(!form(&app).can_save());
        send(&mut app, Message::Title("Something".into()));
        assert!(!form(&app).can_save());
        send(&mut app, Message::Released("2026-12".into()));
        assert!(form(&app).can_save());
    }

    #[test]
    fn delete_removes_the_entry_everywhere_after_confirming() {
        let mut app = app();
        let id = "movies/the-thing-1982";
        app.library.selected = Some(id.into());
        send(&mut app, Message::Delete(id.into()));
        assert!(
            app.repo.library().entry(id).is_some(),
            "not before confirming"
        );
        send(&mut app, Message::ConfirmDelete);
        let library = app.repo.library();
        assert!(library.entry(id).is_none());
        assert!(library.plans.iter().all(|p| p.item_for(id).is_none()));
        assert_eq!(app.library.selected, None);
    }

    #[test]
    fn add_to_plan_toggles_at_once_and_marks_a_repeat() {
        let mut app = app();
        let id = "movies/the-thing-1982";
        send(&mut app, Message::AddToPlan(id.into()));
        send(&mut app, Message::TogglePlan("christmas-2026".into()));
        let item = app
            .repo
            .library()
            .plan("christmas-2026")
            .unwrap()
            .item_for(id)
            .cloned();
        assert!(
            item.is_some_and(|i| i.repeat),
            "The Thing is done, so it is a rewatch"
        );
        send(&mut app, Message::TogglePlan("christmas-2026".into()));
        assert!(
            app.repo
                .library()
                .plan("christmas-2026")
                .unwrap()
                .item_for(id)
                .is_none()
        );
    }

    #[test]
    fn esc_closes_the_innermost_layer_first() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::OpenPicker);
        send(
            &mut app,
            Message::Picker(PickerMessage::Query("eerie".into())),
        );
        send(&mut app, Message::CreateTag);
        assert!(dismiss_top(&mut app));
        assert!(form(&app).new_tag.is_none() && form(&app).picker.is_some());
        assert!(dismiss_top(&mut app));
        assert!(form(&app).picker.is_none());
        assert!(dismiss_top(&mut app));
        assert!(app.dialog.is_none());
        assert!(!dismiss_top(&mut app));
    }

    #[test]
    fn saving_an_album_with_its_songs_saved_asks_and_can_absorb_them() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Music));
        send(&mut app, Message::Title("Bad Music for Bad People".into()));
        send(&mut app, Message::Creator("The Cramps".into()));
        send(&mut app, Message::Save);
        assert!(
            matches!(&form(&app).replace, Some(Replace::Songs { songs, .. }) if songs == &["Human Fly"])
        );
        send(&mut app, Message::ReplaceConfirm);
        let library = app.repo.library();
        assert!(
            library.entry("music/human-fly-the-cramps").is_none(),
            "the song goes"
        );
        let album = library
            .entry("music/bad-music-for-bad-people-the-cramps")
            .unwrap();
        assert_eq!(album.tracks.as_ref().unwrap()[0].title, "Human Fly");
    }

    #[test]
    fn saving_a_song_whose_album_is_saved_asks_and_can_keep_both() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Music));
        send(&mut app, Message::Kind(EntryKind::Song));
        send(&mut app, Message::Title("Rooster".into()));
        send(&mut app, Message::Creator("Alice in Chains".into()));
        send(&mut app, Message::Album("Dirt".into()));
        send(&mut app, Message::Save);
        assert!(
            matches!(&form(&app).replace, Some(Replace::Album { album, adding: true, .. }) if album == "Dirt")
        );
        send(&mut app, Message::ReplaceKeepBoth);
        let library = app.repo.library();
        assert!(library.entry("music/dirt-alice-in-chains").is_some());
        assert!(library.entry("music/rooster-alice-in-chains").is_some());
    }

    #[test]
    fn opening_a_dialog_closes_the_popovers_under_it() {
        let mut app = app();
        app.library.selected = Some("music/dirt-alice-in-chains".into());
        send(&mut app, Message::ToggleTrackRating("Rooster".into()));
        send(&mut app, Message::ToggleDetailsRating);
        send(&mut app, Message::Add);
        assert_eq!(app.library.track_rating_open, None);
        assert!(!app.library.details_rating_open);
    }

    /// Edit Million Dollar Baby with the form's tag picker open.
    fn edit_baby(app: &mut App) {
        send(app, Message::Edit("movies/million-dollar-baby-2004".into()));
        send(app, Message::OpenPicker);
    }

    #[test]
    fn a_tag_in_two_categories_is_ticked_per_group_and_the_last_untick_takes_it_off() {
        let mut app = app();
        edit_baby(&mut app);
        let library = app.repo.library().clone();
        let f = form(&app);
        assert!(f.has_tag_as("clint-eastwood", Some(TagCategory::Directors), &library));
        assert!(f.has_tag_as("clint-eastwood", Some(TagCategory::Actors), &library));
        assert_eq!(
            f.roles_on("clint-eastwood", &library),
            [TagCategory::Directors, TagCategory::Actors]
        );
        assert!(f.roles_on("drama", &library).is_empty());

        send(
            &mut app,
            Message::Picker(PickerMessage::Toggle(
                "clint-eastwood".into(),
                Some(TagCategory::Directors),
            )),
        );
        let f = form(&app);
        assert!(!f.has_tag_as("clint-eastwood", Some(TagCategory::Directors), &library));
        assert_eq!(
            f.roles_on("clint-eastwood", &library),
            [TagCategory::Actors]
        );
        send(
            &mut app,
            Message::RemoveTag("clint-eastwood".into(), Some(TagCategory::Actors)),
        );
        assert!(!form(&app).tags.iter().any(|t| t == "clint-eastwood"));

        // Ticked again under one group, saved with that pick alone.
        send(
            &mut app,
            Message::Picker(PickerMessage::Toggle(
                "clint-eastwood".into(),
                Some(TagCategory::Actors),
            )),
        );
        send(&mut app, Message::ClosePicker);
        send(&mut app, Message::Save);
        let baby = app
            .repo
            .library()
            .entry("movies/million-dollar-baby-2004")
            .unwrap();
        assert_eq!(
            baby.tag_categories.as_ref().unwrap().get("clint-eastwood"),
            Some(&[TagCategory::Actors][..])
        );
    }

    #[test]
    fn a_new_tag_can_take_two_categories_of_one_kind_and_another_kind_starts_over() {
        let mut app = app();
        edit_baby(&mut app);
        send(
            &mut app,
            Message::Picker(PickerMessage::Query("gothic".into())),
        );
        send(&mut app, Message::CreateTag);
        send(&mut app, Message::NewTagCategory(TagCategory::MusicGenres));
        assert_eq!(
            form(&app).new_tag.as_ref().unwrap().categories,
            [TagCategory::Genres, TagCategory::MusicGenres]
        );
        send(&mut app, Message::NewTagCategory(TagCategory::Actors));
        assert_eq!(
            form(&app).new_tag.as_ref().unwrap().categories,
            [TagCategory::Actors],
            "another kind starts over"
        );
        send(&mut app, Message::NewTagCategory(TagCategory::Actors));
        assert!(form(&app).new_tag.as_ref().unwrap().categories.is_empty());
        send(&mut app, Message::ConfirmNewTag);
        assert!(form(&app).new_tag.is_some(), "Create needs a category");
        send(&mut app, Message::NewTagCategory(TagCategory::Genres));
        send(&mut app, Message::NewTagCategory(TagCategory::MusicGenres));
        send(&mut app, Message::ConfirmNewTag);
        let library = app.repo.library().clone();
        assert_eq!(
            form(&app).roles_on("gothic", &library),
            [TagCategory::Genres, TagCategory::MusicGenres],
            "ticked in both on this entry"
        );
        send(&mut app, Message::ClosePicker);
        send(&mut app, Message::Save);
        let library = app.repo.library();
        assert_eq!(
            library.categories_of("gothic"),
            [TagCategory::Genres, TagCategory::MusicGenres]
        );
        let baby = library.entry("movies/million-dollar-baby-2004").unwrap();
        assert_eq!(
            baby.tag_categories.as_ref().unwrap().get("gothic"),
            Some(&[TagCategory::Genres, TagCategory::MusicGenres][..])
        );
    }

    #[test]
    fn add_to_directors_files_the_person_there_on_save() {
        let mut app = app();
        edit_baby(&mut app);
        send(
            &mut app,
            Message::Picker(PickerMessage::AddTo(
                "morgan-freeman".into(),
                TagCategory::Directors,
            )),
        );
        let library = app.repo.library().clone();
        let f = form(&app);
        assert_eq!(
            f.categories_of("morgan-freeman", &library),
            [TagCategory::Directors, TagCategory::Actors]
        );
        assert_eq!(
            f.roles_on("morgan-freeman", &library),
            [TagCategory::Directors, TagCategory::Actors],
            "he was on the entry already, as an actor, and is now a director too"
        );
        assert_eq!(
            library.categories_of("morgan-freeman"),
            [TagCategory::Actors],
            "nothing changes before Save"
        );
        send(&mut app, Message::ClosePicker);
        send(&mut app, Message::Save);
        assert_eq!(
            app.repo.library().categories_of("morgan-freeman"),
            [TagCategory::Directors, TagCategory::Actors]
        );
    }

    #[test]
    fn the_categories_dialog_takes_a_role_away_everywhere_and_moves_a_tag() {
        let mut app = app();
        edit_baby(&mut app);
        send(
            &mut app,
            Message::Picker(PickerMessage::Categories("clint-eastwood".into())),
        );
        let dialog = form(&app).categories.as_ref().unwrap();
        assert!(!dialog.single);
        assert_eq!(dialog.draft, [TagCategory::Directors, TagCategory::Actors]);
        send(&mut app, Message::CategoryDraft(TagCategory::Actors));
        send(&mut app, Message::CategoryDraft(TagCategory::Directors));
        assert_eq!(
            form(&app).categories.as_ref().unwrap().draft,
            [TagCategory::Directors],
            "at least one stays"
        );
        send(&mut app, Message::SaveCategories);
        let library = app.repo.library().clone();
        assert_eq!(
            form(&app).categories_of("clint-eastwood", &library),
            [TagCategory::Directors]
        );
        assert!(form(&app).roles_on("clint-eastwood", &library).is_empty());

        // Move #classic from Other to Languages.
        send(
            &mut app,
            Message::Picker(PickerMessage::Categories("classic".into())),
        );
        assert!(form(&app).categories.as_ref().unwrap().single);
        send(&mut app, Message::CategoryDraft(TagCategory::Languages));
        send(&mut app, Message::SaveCategories);
        send(&mut app, Message::ClosePicker);
        send(&mut app, Message::Save);
        let library = app.repo.library();
        assert_eq!(
            library.categories_of("clint-eastwood"),
            [TagCategory::Directors]
        );
        assert!(
            !library
                .entry("movies/the-good-the-bad-and-the-ugly-1966")
                .unwrap()
                .tags
                .iter()
                .any(|t| t == "clint-eastwood"),
            "an actor-only entry loses him"
        );
        assert_eq!(library.categories_of("classic"), [TagCategory::Languages]);
    }

    #[test]
    fn the_picker_offers_add_to_directors_for_a_person_found_under_actors() {
        let mut app = app();
        edit_baby(&mut app);
        send(
            &mut app,
            Message::Picker(PickerMessage::Query("freeman".into())),
        );
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        assert!(ui.find("Not a director tag yet").is_ok());
        assert!(ui.find("+ Add to Directors").is_ok());
        assert!(
            ui.find("+ Add to Authors").is_ok(),
            "Authors has no match of its own either"
        );
    }

    #[test]
    fn the_details_pane_shows_the_roles_merged() {
        let mut app = app();
        let _ = app.update(AppMessage::Library(
            crate::screens::library::Message::ShowType(Some(MediaType::Movies)),
        ));
        let _ = app.update(AppMessage::Library(
            crate::screens::library::Message::Select("movies/million-dollar-baby-2004".into()),
        ));
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        assert!(ui.find("· Director, Actor").is_ok());
    }

    #[test]
    fn a_shows_seasons_are_edited_in_the_form_and_set_its_status() {
        let mut app = app();
        let id = "anime/frieren-beyond-journeys-end-2023";
        send(&mut app, Message::Edit(id.into()));
        assert_eq!(form(&app).seasons.len(), 2);
        assert!(form(&app).uses_seasons());
        send(&mut app, Message::SeasonStatus(2, Some(EntryStatus::Done)));
        send(&mut app, Message::AddSeason);
        send(&mut app, Message::SeasonDate(3, "2027".into()));
        send(&mut app, Message::Save);
        let show = app.repo.library().entry(id).unwrap();
        let seasons = show.season_list();
        assert_eq!(seasons.len(), 3);
        assert_eq!(seasons[1].status, Some(EntryStatus::Done));
        assert_eq!(seasons[1].manual, None, "a date left alone is not manual");
        assert_eq!(seasons[2].manual, Some(true), "a season added here is");
        assert_eq!(show.status, EntryStatus::Done, "set by the seasons out");
        assert_eq!(show.released, None);
    }

    #[test]
    fn a_new_show_starts_with_season_one_whose_status_is_the_shows() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Shows));
        send(&mut app, Message::Title("Adolescence".into()));
        send(&mut app, Message::SeasonDate(1, "2025-03-13".into()));
        send(&mut app, Message::Status(EntryStatus::Done));
        assert_eq!(form(&app).seasons[0].status, Some(EntryStatus::Done));
        send(&mut app, Message::Save);
        let show = app.repo.library().entry("shows/adolescence-2025").unwrap();
        assert_eq!(show.season_list()[0].manual, Some(true));
        assert_eq!(show.status, EntryStatus::Done);
        assert_eq!(show.year(), Some(2025), "from Season 1");
    }

    #[test]
    fn pages_series_and_the_anime_kind_are_saved() {
        let mut app = app();
        let book = "books/berserk-vol-43-2025";
        send(&mut app, Message::Edit(book.into()));
        assert_eq!(form(&app).series_name, "Berserk");
        send(&mut app, Message::Pages("256".into()));
        send(&mut app, Message::SeriesNumber("44".into()));
        send(&mut app, Message::Save);
        let saved = app.repo.library().entry(book).unwrap();
        assert_eq!(saved.pages, Some(256));
        assert_eq!(saved.series.as_ref().unwrap().number, Some(44));

        let anime = "anime/spirited-away-2001";
        send(&mut app, Message::Edit(anime.into()));
        assert_eq!(form(&app).anime_kind, None, "from before kinds");
        send(&mut app, Message::AnimeKind(EntryKind::Series));
        assert_eq!(form(&app).seasons.len(), 1, "Season 1 appears");
        send(&mut app, Message::AnimeKind(EntryKind::Movie));
        send(&mut app, Message::SeriesName("Ghibli".into()));
        send(&mut app, Message::Save);
        let saved = app.repo.library().entry(anime).unwrap();
        assert_eq!(saved.kind, Some(EntryKind::Movie));
        assert!(saved.seasons.is_none());
        assert_eq!(saved.series.as_ref().unwrap().name, "Ghibli");
    }

    #[test]
    fn the_inspector_and_the_pages_show_seasons_and_series() {
        let mut app = app();
        let _ = app.update(AppMessage::Library(
            crate::screens::library::Message::ShowType(Some(MediaType::Anime)),
        ));
        let _ = app.update(AppMessage::Library(
            crate::screens::library::Message::Select(
                "anime/frieren-beyond-journeys-end-2023".into(),
            ),
        ));
        {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            assert!(ui.find("· from seasons").is_ok());
            assert!(ui.find("2 seasons").is_ok());
        }

        let _ = app.update(AppMessage::Library(
            crate::screens::library::Message::OpenPage(crate::screens::library::Page::Series {
                media: MediaType::Movies,
                name: "The Lord of the Rings".into(),
                highlight: None,
            }),
        ));
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        assert!(ui.find("3 movies · 2 in your library").is_ok());
        assert!(ui.find("+ Add").is_ok());
    }

    #[test]
    fn look_up_again_replaces_the_lookups_fields_and_keeps_the_owners() {
        let mut app = app();
        let id = "books/dracula-1897";
        send(&mut app, Message::Edit(id.into()));
        send(&mut app, Message::Note("Read every October.".into()));
        send(&mut app, Message::LookAgain);
        let f = form(&app);
        assert!(f.again && f.query == "Dracula");
        assert_eq!(f.matches().len(), 3, "the lookup finds the editions");
        send(&mut app, Message::Pick(1));
        let f = form(&app);
        assert!(f.fetched);
        assert_eq!(f.released, "2003-02-25");
        assert_eq!(f.source.as_deref(), Some("openlibrary:OL24241M"));
        assert_eq!(f.title, "Dracula");
        assert_eq!(f.note, "Read every October.");
        assert!(!f.ebook_note, "never while editing");
        send(&mut app, Message::CancelLookAgain);
        send(&mut app, Message::Save);
        let saved = app.repo.library().entry(id).unwrap();
        assert_eq!(saved.source.as_deref(), Some("openlibrary:OL24241M"));
        assert_eq!(saved.released.map(|r| r.year), Some(2003));
        assert_eq!(saved.note, "Read every October.");
    }

    #[test]
    fn a_picked_cover_is_stored_and_goes_with_the_type() {
        let mut app = app();
        let id = "books/dracula-1897";
        send(&mut app, Message::Edit(id.into()));
        send(&mut app, Message::OpenCover);
        assert!(
            !form(&app).cover_dialog,
            "no match yet, so no cover to pick"
        );
        send(&mut app, Message::LookAgain);
        send(&mut app, Message::Pick(0));
        send(&mut app, Message::CancelLookAgain);
        send(&mut app, Message::OpenCover);
        assert!(form(&app).cover_dialog);
        assert_eq!(form(&app).cover_choices().len(), 3, "the match's own first");
        send(
            &mut app,
            Message::PickCover(Some("itunes:498187584".into())),
        );
        send(&mut app, Message::CloseCover);
        send(&mut app, Message::Save);
        let saved = app.repo.library().entry(id).unwrap();
        assert_eq!(saved.cover_from.as_deref(), Some("itunes:498187584"));

        send(&mut app, Message::Edit(id.into()));
        send(&mut app, Message::PickCover(None));
        assert!(form(&app).cover_from.is_none(), "the match's cover again");
        send(
            &mut app,
            Message::PickCover(Some("itunes:498187584".into())),
        );
        send(&mut app, Message::Type(MediaType::Movies));
        assert!(form(&app).cover_from.is_none(), "cleared with the type");
    }

    #[test]
    fn a_platform_narrows_the_game_lookup_and_is_picked_with_the_match() {
        let mut app = app();
        app.settings.platform = Some("PC".into());
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Games));
        send(&mut app, Message::Query("street fighter".into()));
        assert_eq!(form(&app).matches().len(), 3);
        send(&mut app, Message::TogglePlatformPicker);
        send(&mut app, Message::PlatformQuery("snes".into()));
        send(
            &mut app,
            Message::PickPlatform(Some("Super Nintendo".into())),
        );
        let f = form(&app);
        assert!(!f.platform_open && f.platform_query.is_empty());
        assert_eq!(f.matches().len(), 3);
        send(&mut app, Message::PickPlatform(Some("Mega Drive".into())));
        assert_eq!(form(&app).matches().len(), 2);
        send(&mut app, Message::Pick(0));
        let f = form(&app);
        assert_eq!(
            f.platforms_on.iter().collect::<Vec<_>>(),
            ["Mega Drive"],
            "the searched platform, not the default"
        );
        send(&mut app, Message::PickPlatform(None));
        assert_eq!(form(&app).matches().len(), 3);
    }

    #[test]
    fn the_lookup_suggests_actors_a_language_and_a_known_network() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Shows));
        send(&mut app, Message::Query("adolescence".into()));
        send(&mut app, Message::Pick(0));
        let library = app.repo.library().clone();
        let f = form(&app);
        assert!(
            f.has_tag_as("netflix", Some(TagCategory::Streaming), &library),
            "a network the library has goes on at once"
        );
        let graham = f
            .suggested
            .iter()
            .find(|s| s.tag == "stephen-graham")
            .unwrap();
        assert_eq!(graham.category, Some(TagCategory::Actors));
        send(&mut app, Message::AddSuggested("stephen-graham".into()));
        assert_eq!(
            form(&app).new_tag.as_ref().unwrap().categories,
            [TagCategory::Actors],
            "a new actor starts on Actors"
        );
        send(&mut app, Message::CancelNewTag);

        send(&mut app, Message::Type(MediaType::Movies));
        send(&mut app, Message::Query("parasite".into()));
        send(&mut app, Message::Pick(0));
        let korean = form(&app)
            .suggested
            .iter()
            .find(|s| s.tag == "korean")
            .unwrap();
        assert_eq!(korean.category, Some(TagCategory::Languages));

        // A person the library has under another people category is
        // offered with the new role and not as new.
        send(&mut app, Message::Query("weapons".into()));
        send(&mut app, Message::Pick(0));
        send(&mut app, Message::OpenPicker);
        send(&mut app, Message::ClosePicker);
        let f = form(&app);
        assert!(f.suggested.iter().any(|s| s.tag == "julia-garner"));
        assert!(!f.ebook_note);
    }

    #[test]
    fn the_ebook_year_note_shows_while_adding_until_the_date_changes() {
        let mut app = app();
        send(&mut app, Message::Add);
        send(&mut app, Message::Type(MediaType::Books));
        send(&mut app, Message::Query("a christmas carol".into()));
        send(&mut app, Message::Pick(0));
        assert!(form(&app).ebook_note);
        send(&mut app, Message::Released("1843".into()));
        assert!(!form(&app).ebook_note);
        assert_eq!(
            lookup::LookupState::from_env("partial-none"),
            Some(lookup::LookupState::PartialNone)
        );
        assert_eq!(lookup::LookupState::from_env("nope"), None);
    }

    const BABY: &str = "movies/million-dollar-baby-2004";

    /// "+ Tag" in the details pane: the picker alone, and Done writes the
    /// tags as one change; X drops them.
    #[test]
    fn the_details_picker_saves_the_tags_as_one_change_and_close_drops_them() {
        let mut app = app();
        send(&mut app, Message::EditTags(BABY.into()));
        assert!(matches!(app.dialog, Some(Dialog::Tags(_))));
        send(
            &mut app,
            Message::Picker(PickerMessage::Toggle("horror".into(), None)),
        );
        send(
            &mut app,
            Message::Picker(PickerMessage::Toggle(
                "clint-eastwood".into(),
                Some(TagCategory::Directors),
            )),
        );
        send(&mut app, Message::SaveTags);
        assert!(app.dialog.is_none());
        let baby = app.repo.library().entry(BABY).unwrap();
        assert!(baby.tags.iter().any(|t| t == "horror"));
        assert_eq!(
            baby.tag_categories.as_ref().unwrap().get("clint-eastwood"),
            Some(&[TagCategory::Actors][..])
        );
        let change = Operation::SetTags {
            id: BABY.into(),
            tags: baby.tags.clone(),
            tag_categories: baby.tag_categories.clone(),
        };
        assert_eq!(
            change.summary(app.repo.library()),
            "Change the tags on Million Dollar Baby"
        );

        // Closed without Done: nothing changes.
        let before = app.repo.library().clone();
        send(&mut app, Message::EditTags(BABY.into()));
        send(
            &mut app,
            Message::Picker(PickerMessage::Toggle("comedy".into(), None)),
        );
        send(&mut app, Message::Close);
        assert!(app.dialog.is_none());
        assert_eq!(*app.repo.library(), before);

        // Done with nothing changed writes nothing.
        send(&mut app, Message::EditTags(BABY.into()));
        send(&mut app, Message::SaveTags);
        assert_eq!(*app.repo.library(), before);
    }

    /// A new tag from the details' picker still asks for its category,
    /// and is filed right before the tags are saved.
    #[test]
    fn a_new_tag_from_the_details_asks_for_its_category_and_is_filed_on_done() {
        let mut app = app();
        send(&mut app, Message::EditTags(BABY.into()));
        send(
            &mut app,
            Message::Picker(PickerMessage::Query("gothic".into())),
        );
        send(&mut app, Message::CreateTag);
        assert!(form(&app).new_tag.is_some());
        send(&mut app, Message::NewTagCategory(TagCategory::Mood));
        // Enter confirms the new tag, then Esc closes the picker.
        let _ = confirm(&mut app);
        assert!(form(&app).new_tag.is_none());
        assert!(app.repo.library().category_of("gothic").is_none());
        let _ = confirm(&mut app);
        assert!(app.dialog.is_none());
        assert_eq!(
            app.repo.library().category_of("gothic"),
            Some(TagCategory::Mood)
        );
        assert!(
            app.repo
                .library()
                .entry(BABY)
                .unwrap()
                .tags
                .iter()
                .any(|t| t == "gothic")
        );
    }

    /// Esc in the details' picker closes its layers first, then drops the
    /// changes.
    #[test]
    fn esc_in_the_details_picker_closes_the_layers_then_the_picker() {
        let mut app = app();
        send(&mut app, Message::EditTags(BABY.into()));
        send(
            &mut app,
            Message::Picker(PickerMessage::Query("gothic".into())),
        );
        send(&mut app, Message::CreateTag);
        assert!(dismiss_top(&mut app));
        assert!(form(&app).new_tag.is_none());
        assert!(dismiss_top(&mut app));
        assert!(app.dialog.is_none());
    }

    /// The note is typed in the details pane: Enter saves it as one change,
    /// Esc puts the saved note back.
    #[test]
    fn the_details_note_saves_on_enter_and_esc_restores_it() {
        let mut app = app();
        let draft = |note: &str| Message::NoteDraft {
            id: BABY.into(),
            note: note.into(),
        };
        let saved = app.repo.library().entry(BABY).unwrap().note.clone();
        send(&mut app, draft("Rewatch in "));
        assert_eq!(
            app.repo.library().entry(BABY).unwrap().note,
            saved,
            "not before Enter"
        );
        send(&mut app, draft("Rewatch in October. "));
        send(&mut app, Message::SaveNote);
        assert_eq!(
            app.repo.library().entry(BABY).unwrap().note,
            "Rewatch in October."
        );
        assert!(app.library.note_draft.is_none());
        let change = Operation::SetNote {
            id: BABY.into(),
            note: String::new(),
        };
        assert_eq!(
            change.summary(app.repo.library()),
            "Clear the note on Million Dollar Baby"
        );

        send(&mut app, draft("Something else"));
        assert!(app.library.dismiss_top());
        assert!(app.library.note_draft.is_none());
        assert_eq!(
            app.repo.library().entry(BABY).unwrap().note,
            "Rewatch in October."
        );

        // Enter with the saved note typed again writes nothing.
        let before = app.repo.library().clone();
        send(&mut app, draft("Rewatch in October."));
        send(&mut app, Message::SaveNote);
        assert_eq!(*app.repo.library(), before);
    }
}
