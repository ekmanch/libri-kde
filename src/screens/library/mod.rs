//! The library: everything or one type, in a grid, a list or a text table,
//! narrowed by search, status, the Filter popover and the tag pickers.

mod entries;
mod overview;
mod tags;
mod toolbar;

use std::collections::{BTreeSet, HashMap};

use iced::widget::{column, container};
use iced::{Element, Fill};

pub use tags::all_tags_popover;

use super::shell::shell;
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, Decade, Filter, Section, Sort};
use crate::model::{Date, Entry, EntryStatus, Library, MediaType, TagCategory};
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

/// The tag picker dialog, opened from the Filter popover's "All tags…".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TagPicker {
    pub query: String,
    /// Groups folded or unfolded by hand, against their default.
    pub toggled: BTreeSet<TagCategory>,
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
    pub picker: Option<TagPicker>,
    pub all_tags_query: String,
    pub pins: Vec<String>,
    pub sidebar_tags: SidebarTags,
    /// Checkboxes on the sidebar's types (a Settings choice in 1f).
    pub type_boxes: bool,
    pub scroll: Scroll,
    /// Where each type group starts in the list, from its top.
    pub group_tops: Vec<f32>,
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
    ToggleTag(String),
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
    PickerSearch(String),
    TogglePickerGroup(TagCategory),
    ClearTags,
    AllTagsSearch(String),
    /// A tag in the All tags popover: filters by it alone.
    PickFromAllTags(String),
    TogglePin(String),
    UnpinAll,
    /// Opens the Add entry dialog, built in 1c.
    Add,
    Scrolled(Scroll),
    GroupsMeasured(Vec<f32>),
    /// A click on the type rail: the app scrolls the list there.
    JumpToGroup(usize),
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
                | Message::PickFromAllTags(_)
        )
    }
}

impl State {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::ShowType(media) => {
                self.types = media.into_iter().collect();
                self.status = None;
                self.selected = None;
                self.popup = None;
            }
            Message::ToggleType(media) => {
                if !self.types.remove(&media) {
                    self.types.insert(media);
                }
                self.selected = None;
            }
            Message::SeeAll(section) => {
                self.types.clear();
                self.selected = None;
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
            Message::ToggleTag(tag) => self.filter.toggle_tag(&tag),
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
                self.popup = None;
                self.picker = Some(TagPicker::default());
            }
            Message::ClosePicker => self.picker = None,
            Message::PickerSearch(query) => {
                if let Some(picker) = &mut self.picker {
                    picker.query = query;
                }
            }
            Message::TogglePickerGroup(category) => {
                if let Some(picker) = &mut self.picker
                    && !picker.toggled.remove(&category)
                {
                    picker.toggled.insert(category);
                }
            }
            Message::ClearTags => self.filter.tags.clear(),
            Message::AllTagsSearch(query) => self.all_tags_query = query,
            Message::PickFromAllTags(tag) => {
                self.filter.tags = vec![tag];
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
            Message::Add | Message::JumpToGroup(_) => {}
            Message::Scrolled(scroll) => self.scroll = scroll,
            Message::GroupsMeasured(tops) => self.group_tops = tops,
        }
    }

    /// Esc: the tag picker first, then a popover. False when nothing was
    /// open.
    pub fn dismiss_top(&mut self) -> bool {
        if self.picker.take().is_some() {
            return true;
        }
        self.popup.take().is_some()
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
        Listing {
            base,
            shown,
            in_type,
        }
    }
}

pub struct Listing<'a> {
    pub base: Vec<&'a Entry>,
    pub shown: Vec<&'a Entry>,
    /// Every entry of the type, whatever the filters.
    pub in_type: usize,
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let library = app.repo.library();
    let listing = state.listing(library, app.today);

    let content = if state.is_overview() {
        column![
            toolbar::toolbar(app, &listing),
            overview::body(app),
            footer(&tokens)
        ]
    } else {
        column![
            toolbar::toolbar(app, &listing),
            toolbar::chips(app, &listing),
            entries::body(app, &listing),
            footer(&tokens),
        ]
    };
    let picker = state
        .picker
        .as_ref()
        .map(|picker| tags::picker(app, picker, &listing));
    let close = AppMessage::Library(Message::ClosePicker);
    ui::modal(shell(app, content), picker, close, &tokens)
}

fn footer<'a>(tokens: &crate::theme::Tokens) -> Element<'a, AppMessage> {
    container(ui::mono("ekmanch/library · main", 12.0, tokens.sub))
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
        state.update(Message::ToggleTag("horror".into()));
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
        state.picker = Some(TagPicker::default());
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
}
