//! The library: everything or one type, in a grid, a list or a text table,
//! narrowed by search, status, the Filter popover and the tag pickers.

mod entries;
mod music;
mod overview;
mod page;
mod tags;
mod toolbar;

use std::collections::{BTreeSet, HashMap};

use iced::widget::{column, container, row};
use iced::{Element, Fill};

pub use tags::{all_tags_popover, long_group};

use super::entry;
use super::shell::shell;
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, Decade, Filter, Row, Section, Sort};
use crate::model::{Date, Entry, EntryStatus, Library, MediaType, TagCategory};
use crate::settings::Source;
use crate::tags::{PickerMessage, PickerState};
use crate::ui;

/// The search field's id, for Ctrl+K and Ctrl+F.
pub const SEARCH_ID: &str = "library-search";
/// The entry list's scroll area, for the type rail's jumps.
pub const LIST_ID: &str = "library-list";
/// How many tags can be pinned to the sidebar.
pub const MAX_PINS: usize = 8;
/// How many of the most used tags the sidebar and the Filter popover list.
pub const TOP_TAGS: usize = 6;

/// The three views, remembered per type, for several types and for All.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum View {
    #[default]
    Grid,
    List,
    Text,
}

/// Music's three tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MusicTab {
    #[default]
    Artists,
    Albums,
    Songs,
}

/// What a view choice is remembered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewKey {
    All,
    Several,
    Type(MediaType),
}

/// The popovers the library opens; one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    Filter,
    Sort,
    /// From the sidebar's "All tags".
    AllTags,
}

/// A page in the content pane, in place of the list: all of a show's
/// seasons, a season's episodes, or a series (decided 2026-10-24: the
/// content pane, with the inspector kept, not a drill-in inside it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Seasons {
        id: String,
    },
    Season {
        id: String,
        number: u32,
    },
    Series {
        media: MediaType,
        name: String,
        /// The part the page was opened from ("All 43 volumes"), marked
        /// "This one".
        highlight: Option<String>,
    },
}

/// Which season's status menu is open: in the inspector, or on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeasonMenu {
    pub id: String,
    pub season: u32,
    pub in_page: bool,
}

/// What the sidebar's TAGS section lists (a Settings choice in 1f).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTags {
    #[default]
    Pinned,
    MostUsed,
}

/// Where the entry list is scrolled to, in px.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Scroll {
    pub offset: f32,
    pub viewport: f32,
    pub content: f32,
}

/// What the library shows, as the history remembers it: the types, the
/// status, the filter, the sort, the search, the music tab and the artist
/// page. Not the selection, the popovers or the scroll position.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Place {
    pub types: BTreeSet<MediaType>,
    pub status: Option<EntryStatus>,
    pub filter: Filter,
    pub sort: Sort,
    pub query: String,
    pub music_tab: MusicTab,
    pub artist: Option<String>,
    pub page: Option<Page>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The types shown, in type order; none is All.
    pub types: BTreeSet<MediaType>,
    pub status: Option<EntryStatus>,
    pub filter: Filter,
    pub sort: Sort,
    pub query: String,
    pub views: HashMap<ViewKey, View>,
    /// The selected entry's id.
    pub selected: Option<String>,
    pub popup: Option<Popup>,
    /// The tag picker dialog, opened from the Filter popover's "All tags…".
    pub picker: Option<PickerState>,
    pub all_tags_query: String,
    pub pins: Vec<String>,
    pub sidebar_tags: SidebarTags,
    /// Checkboxes on the sidebar's types (a Settings choice in 1f).
    pub type_boxes: bool,
    /// The details pane's rating picker.
    pub details_rating_open: bool,
    /// The details pane's note as typed, with its entry's id, until Enter
    /// saves it, the pane moves on (which saves it too) or Esc drops it.
    pub note_draft: Option<(String, String)>,
    /// The album track whose rating picker is open, by title.
    pub track_rating_open: Option<String>,
    pub music_tab: MusicTab,
    /// The artist whose page is open, within Music.
    pub artist: Option<String>,
    /// A page in the content pane, over the list.
    pub page: Option<Page>,
    pub season_menu: Option<SeasonMenu>,
    /// The Series cover dialog, for a series of a type.
    pub cover_dialog: Option<(MediaType, String)>,
    pub scroll: Scroll,
    /// Where each group or section starts in the list, from its top, and
    /// how tall the list's visible part is.
    pub group_tops: Vec<f32>,
    pub viewport: f32,
    /// `LIBRI_LONG_LISTS=1`: section headers and the rail on every list,
    /// to look at them with the small fixture.
    pub force_long: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// A click on a sidebar type: shows only that type, or All.
    ShowType(Option<MediaType>),
    /// Ctrl+click, a type checkbox or a Filter chip: adds or removes it.
    ToggleType(MediaType),
    /// An overview list's "See all".
    SeeAll(Section),
    /// A sidebar tag: filters by it alone, or clears it when it is the only
    /// tag picked.
    PickTag(String),
    Status(Option<EntryStatus>),
    Search(String),
    SetView(View),
    /// Selects an entry, or deselects it when it is selected.
    Select(String),
    Toggle(Popup),
    ClosePopup,
    Sort(Sort),
    ToggleDecade(Decade),
    /// A tag, as a category for a tag in several (`None` toggles it by
    /// name, which matches whatever category it has on an entry).
    ToggleTag(String, Option<TagCategory>),
    PickRating(u8),
    ClearRating,
    ToggleSeasonal,
    ClearNotReleased,
    ClearRecent,
    /// The Filter popover's Reset: the filter, not the status.
    ResetFilter,
    /// Clear all: the filter and the status.
    ClearAll,
    OpenPicker,
    ClosePicker,
    Picker(PickerMessage),
    ClearTags,
    AllTagsSearch(String),
    /// A tag in the All tags popover: filters by it alone, as the category
    /// it was listed under when it is in several.
    PickFromAllTags(String, Option<TagCategory>),
    TogglePin(String),
    UnpinAll,
    /// Opens the Add entry dialog.
    Add,
    MusicTab(MusicTab),
    OpenArtist(String),
    CloseArtist,
    Scrolled(Scroll),
    GroupsMeasured(ui::Measure),
    /// A click on the type rail: the app scrolls the list there.
    JumpToGroup(usize),
    OpenPage(Page),
    ClosePage,
    SeasonMenu(Option<SeasonMenu>),
    OpenSeriesCover(MediaType, String),
    CloseSeriesCover,
    /// A part in the Series cover dialog: its cover stands for the series.
    PickSeriesCover(String),
}

impl Message {
    /// Whether the message shows the library, from the sidebar or the All
    /// tags popover on another screen.
    pub fn shows_library(&self) -> bool {
        matches!(
            self,
            Message::ShowType(_)
                | Message::ToggleType(_)
                | Message::PickTag(_)
                | Message::PickFromAllTags(..)
        )
    }
}

impl State {
    /// Where the library is, for the history.
    pub fn place(&self) -> Place {
        Place {
            types: self.types.clone(),
            status: self.status,
            filter: self.filter.clone(),
            sort: self.sort,
            query: self.query.clone(),
            music_tab: self.music_tab,
            artist: self.artist.clone(),
            page: self.page.clone(),
        }
    }

    /// Returns to `place` (Back or Forward): the selection and the
    /// popovers close, the pins and the views stay.
    pub fn go_to(&mut self, place: Place) {
        self.types = place.types;
        self.status = place.status;
        self.filter = place.filter;
        self.sort = place.sort;
        self.query = place.query;
        self.music_tab = place.music_tab;
        self.artist = place.artist;
        self.page = place.page;
        self.selected = None;
        self.popup = None;
        self.picker = None;
        self.season_menu = None;
        self.cover_dialog = None;
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::ShowType(media) => {
                self.types = media.into_iter().collect();
                self.artist = None;
                self.status = None;
                self.selected = None;
                self.popup = None;
                self.page = None;
            }
            Message::ToggleType(media) => {
                if !self.types.remove(&media) {
                    self.types.insert(media);
                }
                self.selected = None;
                self.page = None;
            }
            Message::SeeAll(section) => {
                self.types.clear();
                self.selected = None;
                self.page = None;
                match section {
                    Section::InProgress => self.status = Some(EntryStatus::InProgress),
                    Section::ComingSoon => {
                        self.filter.not_released = true;
                        self.sort = Sort::YearOldest;
                    }
                    Section::RecentlyAdded => {
                        self.filter.recent = true;
                        self.sort = Sort::NewestAdded;
                    }
                    Section::TopRated => {
                        self.filter.rating = Some((browse::TOP_RATED, 10));
                        self.sort = Sort::RatingHighest;
                    }
                }
            }
            Message::PickTag(tag) => {
                self.page = None;
                self.filter.tag_categories.clear();
                self.filter.tags = if self.filter.tags == [tag.as_str()] {
                    Vec::new()
                } else {
                    vec![tag]
                };
                self.selected = None;
            }
            Message::Status(status) => {
                self.status = status;
                self.selected = None;
            }
            Message::Search(query) => self.query = query,
            Message::SetView(view) => {
                self.views.insert(self.view_key(), view);
            }
            Message::Select(id) => {
                self.details_rating_open = false;
                self.track_rating_open = None;
                self.season_menu = None;
                self.selected = if self.selected.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    Some(id)
                };
            }
            Message::Toggle(popup) => {
                self.popup = if self.popup == Some(popup) {
                    None
                } else {
                    Some(popup)
                };
            }
            Message::ClosePopup => self.popup = None,
            Message::Sort(sort) => {
                self.sort = sort;
                self.popup = None;
            }
            Message::ToggleDecade(decade) => self.filter.toggle_decade(decade),
            Message::ToggleTag(tag, category) => self.filter.toggle_tag(&tag, category),
            Message::PickRating(n) => self.filter.pick_rating(n),
            Message::ClearRating => self.filter.rating = None,
            Message::ToggleSeasonal => self.filter.include_seasonal = !self.filter.include_seasonal,
            Message::ClearNotReleased => self.filter.not_released = false,
            Message::ClearRecent => self.filter.recent = false,
            Message::ResetFilter => {
                self.filter = Filter::default();
                self.clear_several_types();
            }
            Message::ClearAll => {
                self.filter = Filter::default();
                self.status = None;
                self.clear_several_types();
            }
            Message::OpenPicker => {
                self.close_popovers();
                self.picker = Some(PickerState::default());
            }
            Message::ClosePicker => self.picker = None,
            Message::Picker(message) => {
                if let Some(picker) = &mut self.picker {
                    picker.update(&message);
                }
                if let PickerMessage::Toggle(tag, category) = message {
                    self.filter.toggle_tag(&tag, category);
                }
            }
            Message::ClearTags => {
                self.filter.tags.clear();
                self.filter.tag_categories.clear();
            }
            Message::AllTagsSearch(query) => self.all_tags_query = query,
            Message::PickFromAllTags(tag, category) => {
                self.filter.tags.clear();
                self.filter.tag_categories.clear();
                self.filter.toggle_tag(&tag, category);
                self.selected = None;
                self.popup = None;
                self.all_tags_query.clear();
            }
            Message::TogglePin(tag) => match self.pins.iter().position(|t| *t == tag) {
                Some(at) => {
                    self.pins.remove(at);
                }
                None if self.pins.len() < MAX_PINS => self.pins.push(tag),
                None => {}
            },
            Message::UnpinAll => self.pins.clear(),
            Message::Add | Message::JumpToGroup(_) | Message::PickSeriesCover(_) => {}
            Message::OpenPage(page) => {
                self.close_popovers();
                // A page opened from a row keeps the row's entry selected;
                // a series page from a series row has nothing selected.
                if matches!(
                    page,
                    Page::Series {
                        highlight: None,
                        ..
                    }
                ) {
                    self.selected = None;
                }
                self.page = Some(page);
            }
            Message::ClosePage => {
                self.page = None;
                self.season_menu = None;
                self.cover_dialog = None;
            }
            Message::SeasonMenu(menu) => self.season_menu = menu,
            Message::OpenSeriesCover(media, name) => self.cover_dialog = Some((media, name)),
            Message::CloseSeriesCover => self.cover_dialog = None,
            Message::Scrolled(scroll) => self.scroll = scroll,
            Message::MusicTab(tab) => {
                self.music_tab = tab;
                self.artist = None;
            }
            Message::OpenArtist(name) => {
                self.artist = Some(name);
                self.selected = None;
            }
            Message::CloseArtist => self.artist = None,
            Message::GroupsMeasured(measure) => {
                self.group_tops = measure.tops;
                self.viewport = measure.viewport;
            }
        }
    }

    /// Esc: the tag picker first, then a popover. False when nothing was
    /// open.
    pub fn dismiss_top(&mut self) -> bool {
        if self.season_menu.take().is_some() || self.cover_dialog.take().is_some() {
            return true;
        }
        if let Some(picker) = &mut self.picker {
            if !picker.dismiss_inner() {
                self.picker = None;
            }
            return true;
        }
        self.popup.take().is_some() || self.note_draft.take().is_some()
    }

    /// Closes every popover: the Filter, Sort and All tags popovers and
    /// the details pane's rating pickers.
    pub fn close_popovers(&mut self) {
        self.popup = None;
        self.details_rating_open = false;
        self.track_rating_open = None;
        self.season_menu = None;
    }

    /// Enter: the tag picker's Done.
    pub fn confirm(&mut self) {
        self.picker = None;
    }

    /// Several picked types count as a filter, so Reset and Clear all drop
    /// them; one type is where the library is, so they keep it.
    fn clear_several_types(&mut self) {
        if self.types.len() > 1 {
            self.types.clear();
        }
    }

    /// The one type shown, when exactly one is.
    pub fn single(&self) -> Option<MediaType> {
        match self.types.len() {
            1 => self.types.first().copied(),
            _ => None,
        }
    }

    /// All with nothing narrowing it shows the overview, not a list.
    pub fn is_overview(&self) -> bool {
        self.types.is_empty()
            && self.filter.is_empty()
            && self.status.is_none()
            && self.query.trim().is_empty()
    }

    /// "Library", "Movies" or "Movies + Shows".
    pub fn title(&self) -> String {
        if let (Some(artist), Some(MediaType::Music)) = (&self.artist, self.single()) {
            return artist.clone();
        }
        if self.types.is_empty() {
            return "Library".into();
        }
        let names: Vec<&str> = self.types.iter().map(|m| m.label()).collect();
        names.join(" + ")
    }

    fn view_key(&self) -> ViewKey {
        match (self.types.len(), self.single()) {
            (0, _) => ViewKey::All,
            (_, Some(media)) => ViewKey::Type(media),
            _ => ViewKey::Several,
        }
    }

    pub fn view_mode(&self) -> View {
        self.views
            .get(&self.view_key())
            .copied()
            .unwrap_or_default()
    }

    /// The filter's count with several picked types, for the badge.
    pub fn filter_count(&self) -> usize {
        self.filter.count()
            + if self.types.len() > 1 {
                self.types.len()
            } else {
                0
            }
    }

    /// The tags the sidebar lists, with their counts.
    pub fn sidebar_tags<'a>(&'a self, counts: &[(&'a str, usize)]) -> Vec<(&'a str, usize)> {
        match self.sidebar_tags {
            SidebarTags::MostUsed => counts.iter().take(TOP_TAGS).copied().collect(),
            SidebarTags::Pinned => self
                .pins
                .iter()
                .map(|pin| {
                    let count = counts
                        .iter()
                        .find(|(tag, _)| tag == pin)
                        .map_or(0, |(_, n)| *n);
                    (pin.as_str(), count)
                })
                .collect(),
        }
    }

    /// The phone's long-list rule: `rows` of `view` make a list at least
    /// three screens tall (and the sort gives two sections or more, which
    /// the caller checks).
    pub fn is_long(&self, rows: usize, view: View) -> bool {
        let row_height = match view {
            View::Grid => return false,
            View::List => 68.0,
            View::Text => 43.0,
        };
        self.force_long || (self.viewport > 0.0 && rows as f32 * row_height >= 3.0 * self.viewport)
    }

    /// Whether the list folds the parts of a series into one row: one
    /// type, the status segment All, Done or DNF, no search, and a list at
    /// least three screens tall measured on the single entries (a grid
    /// counts a row of cards as four entries).
    pub fn groups_series(&self, rows: usize, view: View) -> bool {
        let long = match view {
            View::Grid => {
                self.force_long
                    || (self.viewport > 0.0 && rows as f32 * 75.0 >= 3.0 * self.viewport)
            }
            _ => self.is_long(rows, view),
        };
        self.single().is_some_and(|m| m != MediaType::Music)
            && browse::groups_series_for(self.status)
            && self.query.trim().is_empty()
            && long
    }

    /// The ids in the order the list shows them, for the arrow keys: one
    /// type in its sort, All and several types by type first. Music
    /// follows its tab; the artist lists have no selection.
    pub fn display_order(&self, library: &Library, today: Date) -> Vec<String> {
        let listing = self.listing(library, today);
        if self.is_overview() {
            return Vec::new();
        }
        let ids = |entries: &[&Entry]| entries.iter().map(|e| e.id.clone()).collect();
        if self.single() == Some(MediaType::Music) {
            return match (self.artist.as_deref(), self.music_tab) {
                (Some(name), _) => {
                    let artists = crate::music::artists(&listing.shown);
                    match artists.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
                        Some(artist) => artist
                            .albums
                            .iter()
                            .chain(&artist.songs)
                            .map(|e| e.id.clone())
                            .collect(),
                        None => Vec::new(),
                    }
                }
                (None, MusicTab::Artists) => Vec::new(),
                (None, MusicTab::Albums) => listing
                    .shown
                    .iter()
                    .filter(|e| !crate::music::is_song(e))
                    .map(|e| e.id.clone())
                    .collect(),
                (None, MusicTab::Songs) => crate::music::songs(&listing.shown)
                    .iter()
                    .filter_map(|song| match song {
                        crate::music::Song::Entry(e) => Some(e.id.clone()),
                        crate::music::Song::Track { .. } => None,
                    })
                    .collect(),
            };
        }
        match self.single() {
            Some(_) => ids(&listing.shown),
            None => MediaType::ALL
                .into_iter()
                .flat_map(|media| {
                    listing
                        .shown
                        .iter()
                        .filter(move |e| e.media == media)
                        .map(|e| e.id.clone())
                        .collect::<Vec<_>>()
                })
                .collect(),
        }
    }

    /// The entries the library shows: `base` is the type, the filter and
    /// the search; `shown` adds the status and the sort.
    pub fn listing<'a>(&self, library: &'a Library, today: Date) -> Listing<'a> {
        // A search looks at seasonal-only entries too, as on the phone.
        let filter = if self.query.trim().is_empty() {
            self.filter.clone()
        } else {
            Filter {
                include_seasonal: true,
                ..self.filter.clone()
            }
        };
        let of_type = |e: &&Entry| self.types.is_empty() || self.types.contains(&e.media);
        let base: Vec<&Entry> = library
            .entries
            .iter()
            .filter(of_type)
            .filter(|e| filter.matches(e, today) && browse::matches_search(e, &self.query))
            .collect();
        let mut shown: Vec<&Entry> = base
            .iter()
            .copied()
            .filter(|e| self.status.is_none_or(|s| e.status == s))
            .collect();
        self.sort.apply(&mut shown);
        let in_type = library.entries.iter().filter(of_type).count();
        let rows = if self.groups_series(shown.len(), self.view_mode()) {
            browse::group_series(&shown)
        } else {
            shown.iter().map(|&e| Row::Entry(e)).collect()
        };
        Listing {
            base,
            shown,
            rows,
            in_type,
        }
    }
}

pub struct Listing<'a> {
    pub base: Vec<&'a Entry>,
    pub shown: Vec<&'a Entry>,
    /// `shown` as the list lays it out: a series as one row in a long
    /// list.
    pub rows: Vec<Row<'a>>,
    /// Every entry of the type, whatever the filters.
    pub in_type: usize,
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let library = app.repo.library();
    let listing = state.listing(library, app.today);

    let content = if let Some(page) = &state.page {
        column![page::view(app, page), footer(app)]
    } else if state.is_overview() {
        column![
            toolbar::toolbar(app, &listing),
            overview::body(app),
            footer(app)
        ]
    } else {
        column![
            toolbar::toolbar(app, &listing),
            toolbar::chips(app, &listing),
            entries::body(app, &listing),
            footer(app),
        ]
    };
    let picker = match (&state.picker, &state.cover_dialog) {
        (_, Some((media, name))) => Some(page::cover_dialog(app, *media, name)),
        (Some(picker), None) => Some(tags::picker(app, picker, &listing)),
        (None, None) => None,
    };
    let close = if state.cover_dialog.is_some() {
        AppMessage::Library(Message::CloseSeriesCover)
    } else {
        AppMessage::Library(Message::ClosePicker)
    };
    // The details pane shows beside the list while an entry is selected.
    let selected = state.selected.as_deref().and_then(|id| library.entry(id));
    let main: Element<'_, AppMessage> = match selected {
        Some(entry) => row![
            container(content).width(Fill),
            entry::details::view(app, entry, None)
        ]
        .into(),
        None => content.into(),
    };
    ui::modal(shell(app, main), picker, close, &tokens)
}

/// The repository and branch, quietly at the bottom of the content.
fn footer<'a>(app: &App) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let line = app.source.as_ref().map(Source::footer).unwrap_or_default();
    container(ui::mono(line, 12.0, tokens.sub))
        .height(30)
        .width(Fill)
        .padding([0, 24])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::codec;

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    fn today() -> Date {
        Date::parse("2026-10-19").unwrap()
    }

    #[test]
    fn a_sidebar_tag_is_picked_alone_and_cleared_by_a_second_click() {
        let mut state = State::default();
        state.filter.tags = vec!["horror".into(), "dark".into()];
        state.update(Message::PickTag("dark".into()));
        assert_eq!(state.filter.tags, ["dark"]);
        state.update(Message::PickTag("dark".into()));
        assert!(state.filter.tags.is_empty());
    }

    #[test]
    fn the_view_is_remembered_per_type() {
        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Books)));
        state.update(Message::SetView(View::Text));
        state.update(Message::ShowType(Some(MediaType::Games)));
        assert_eq!(state.view_mode(), View::Grid);
        state.update(Message::ShowType(Some(MediaType::Books)));
        assert_eq!(state.view_mode(), View::Text);
    }

    #[test]
    fn changing_type_resets_the_status_and_the_selection_but_keeps_the_filter() {
        let mut state = State::default();
        state.update(Message::Status(Some(EntryStatus::Done)));
        state.update(Message::Select("movies/the-mummy-1999".into()));
        state.update(Message::ToggleTag("horror".into(), None));
        state.update(Message::ShowType(Some(MediaType::Movies)));
        assert_eq!((state.status, state.selected.as_deref()), (None, None));
        assert_eq!(state.filter.tags, ["horror"]);
    }

    #[test]
    fn the_listing_narrows_by_type_status_filter_and_search() {
        let library = library();
        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Movies)));
        let all = state.listing(&library, today());
        assert_eq!(all.shown.len(), library.count(Some(MediaType::Movies)));

        state.status = Some(EntryStatus::Done);
        let done = state.listing(&library, today());
        assert!(done.shown.iter().all(|e| e.status == EntryStatus::Done));
        assert_eq!(
            done.base.len(),
            all.base.len(),
            "the status leaves the base alone"
        );

        state.query = "mummy".into();
        let found = state.listing(&library, today());
        assert_eq!(found.shown.len(), 1);
    }

    #[test]
    fn a_search_finds_seasonal_only_entries() {
        let library = library();
        let seasonal = library.entries.iter().find(|e| e.seasonal_only).unwrap();
        let mut state = State::default();
        assert!(
            !state
                .listing(&library, today())
                .shown
                .iter()
                .any(|e| e.id == seasonal.id)
        );
        state.query = seasonal.title.clone();
        assert!(
            state
                .listing(&library, today())
                .shown
                .iter()
                .any(|e| e.id == seasonal.id)
        );
    }

    #[test]
    fn pins_stop_at_eight() {
        let mut state = State::default();
        for n in 0..10 {
            state.update(Message::TogglePin(format!("tag{n}")));
        }
        assert_eq!(state.pins.len(), MAX_PINS);
        state.update(Message::TogglePin("tag0".into()));
        assert_eq!(state.pins.len(), MAX_PINS - 1);
    }

    #[test]
    fn esc_closes_the_picker_before_a_popover() {
        let mut state = State::default();
        state.update(Message::Toggle(Popup::Sort));
        state.picker = Some(PickerState::default());
        assert!(state.dismiss_top());
        assert_eq!(state.popup, Some(Popup::Sort));
        assert!(state.dismiss_top());
        assert!(!state.dismiss_top());
    }

    #[test]
    fn ctrl_click_builds_several_types_and_clear_all_drops_them() {
        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Movies)));
        state.update(Message::ToggleType(MediaType::Shows));
        assert_eq!(state.title(), "Movies + Shows");
        assert_eq!(state.filter_count(), 2);
        state.update(Message::ClearAll);
        assert!(state.types.is_empty());

        state.update(Message::ShowType(Some(MediaType::Books)));
        state.update(Message::ClearAll);
        assert_eq!(state.single(), Some(MediaType::Books), "one type is kept");
    }

    #[test]
    fn all_is_an_overview_until_something_narrows_it() {
        let mut state = State::default();
        assert!(state.is_overview());
        state.update(Message::Search("dune".into()));
        assert!(!state.is_overview());
        state.update(Message::Search(String::new()));
        state.update(Message::SeeAll(Section::TopRated));
        assert!(!state.is_overview());
        assert_eq!(
            (state.filter.rating, state.sort),
            (Some((9, 10)), Sort::RatingHighest)
        );
    }

    #[test]
    fn the_overview_and_several_types_remember_their_own_view() {
        let mut state = State::default();
        state.update(Message::SetView(View::Text));
        state.update(Message::ShowType(Some(MediaType::Movies)));
        state.update(Message::ToggleType(MediaType::Shows));
        state.update(Message::SetView(View::List));
        state.update(Message::ShowType(None));
        assert_eq!(state.view_mode(), View::Text);
        state.update(Message::ToggleType(MediaType::Movies));
        state.update(Message::ToggleType(MediaType::Shows));
        assert_eq!(state.view_mode(), View::List);
    }

    #[test]
    fn an_artist_page_closes_with_another_type_or_tab() {
        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Music)));
        state.update(Message::OpenArtist("Metallica".into()));
        assert_eq!(state.title(), "Metallica");
        state.update(Message::MusicTab(MusicTab::Songs));
        assert_eq!(state.artist, None);
        state.update(Message::OpenArtist("Metallica".into()));
        state.update(Message::ShowType(Some(MediaType::Books)));
        assert_eq!(
            (state.artist.as_deref(), state.title().as_str()),
            (None, "Books")
        );
    }

    #[test]
    fn the_picker_toggles_a_tag_per_category_and_all_tags_picks_as_listed() {
        let mut state = State::default();
        state.update(Message::OpenPicker);
        state.update(Message::Picker(PickerMessage::Toggle(
            "clint-eastwood".into(),
            Some(TagCategory::Directors),
        )));
        assert_eq!(state.filter.tags, ["clint-eastwood"]);
        assert!(
            state
                .filter
                .has_tag("clint-eastwood", Some(TagCategory::Directors))
        );
        assert!(
            !state
                .filter
                .has_tag("clint-eastwood", Some(TagCategory::Actors))
        );
        state.update(Message::Picker(PickerMessage::Drill(
            TagCategory::Directors,
        )));
        assert!(state.dismiss_top(), "Esc leaves the long group first");
        assert!(state.picker.as_ref().is_some_and(|p| p.drill.is_none()));
        assert!(state.dismiss_top());
        assert!(state.picker.is_none());

        state.update(Message::PickFromAllTags(
            "clint-eastwood".into(),
            Some(TagCategory::Actors),
        ));
        assert_eq!(
            state.filter.tag_chips(),
            [("clint-eastwood".to_owned(), Some(TagCategory::Actors))]
        );
        state.update(Message::ClearTags);
        assert!(state.filter.tag_categories.is_empty());
    }

    #[test]
    fn a_long_movie_list_groups_series_unless_picking_what_to_watch() {
        let library = library();
        let mut state = State {
            force_long: true,
            ..State::default()
        };
        state.update(Message::ShowType(Some(MediaType::Movies)));
        let listing = state.listing(&library, today());
        assert!(listing.rows.iter().any(|r| matches!(r, Row::Series(_))));
        assert_eq!(listing.shown.len(), listing.rows.len() + 1);

        state.update(Message::Status(Some(EntryStatus::Want)));
        let listing = state.listing(&library, today());
        assert!(listing.rows.iter().all(|r| matches!(r, Row::Entry(_))));

        state.update(Message::Status(None));
        state.update(Message::Search("lord".into()));
        let listing = state.listing(&library, today());
        assert!(listing.rows.iter().all(|r| matches!(r, Row::Entry(_))));

        state.force_long = false;
        state.update(Message::Search(String::new()));
        let listing = state.listing(&library, today());
        assert!(
            listing.rows.iter().all(|r| matches!(r, Row::Entry(_))),
            "a short list never groups"
        );
    }

    #[test]
    fn a_page_is_a_place_and_closes_with_the_list_changes() {
        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Shows)));
        let page = Page::Seasons {
            id: "shows/x".into(),
        };
        state.update(Message::OpenPage(page.clone()));
        assert_eq!(state.page, Some(page.clone()));
        assert_eq!(state.place().page, Some(page));
        state.update(Message::SeasonMenu(Some(SeasonMenu {
            id: "shows/x".into(),
            season: 1,
            in_page: true,
        })));
        assert!(state.dismiss_top(), "Esc closes the menu first");
        assert!(state.page.is_some());
        state.update(Message::ShowType(Some(MediaType::Movies)));
        assert!(state.page.is_none());
    }
}
