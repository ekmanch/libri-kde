//! The library: everything or one type, in a grid, a list or a text table,
//! narrowed by search, status, the Filter popover and the tag pickers.

mod entries;
mod music;
mod overview;
mod page;
mod panel;
mod rail;
mod reveal;
mod tags;
mod toolbar;

use std::collections::{BTreeSet, HashMap};

use iced::widget::{column, container, row};
use iced::{Element, Fill};

pub use tags::{all_tags_popover, long_group};

use super::entry;
use super::shell::shell;
use crate::app::{App, Message as AppMessage};
use crate::artwork::Wants;
use crate::browse::{self, Decade, Filter, Row, Section, Sort};
use crate::model::{Date, Entry, EntryStatus, Library, MediaType, Series, TagCategory};
use crate::settings::Source;
use crate::tags::{PickerMessage, PickerState};
use crate::ui;

pub use panel::{Motion, Panel, Shown, closes_drawer, drawer_state, narrow, panel, slide_drawer};

/// The search field's id, for Ctrl+K and Ctrl+F.
pub const SEARCH_ID: &str = "library-search";
/// The entry list's scroll area, for the type rail's jumps.
pub const LIST_ID: &str = "library-list";
/// Home's scroll area, to return to where it was after a page.
pub const HOME_ID: &str = "library-home";
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
/// seasons or a season's episodes (decided 2026-10-24: the content pane,
/// with the inspector kept, not a drill-in inside it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Seasons { id: String },
    Season { id: String, number: u32 },
}

/// A series open in the details pane, with nothing selected (decided
/// 2026-10-31: the pane, not a page over the list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSeries {
    pub media: MediaType,
    pub name: String,
    /// The part it was opened from ("All 43 volumes", or ‹ back from a
    /// part), marked and scrolled to.
    pub highlight: Option<String>,
}

/// The series a selected part was opened from, for the ‹ series name at
/// the top of its details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesBack {
    pub id: String,
    pub media: MediaType,
    pub name: String,
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
    /// The details panel unpinned (`false`), per view key like the view;
    /// a key not in it is pinned. Home always has the panel.
    pub panels: HashMap<ViewKey, bool>,
    /// Turns At a glance's "Pick something": "Another" moves it on.
    pub pick: u64,
    /// The selected entry's id.
    pub selected: Option<String>,
    /// The unpinned drawer sliding in or out.
    pub motion: Motion,
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
    /// The entry whose cover in details the pointer went over, so its
    /// sharper copy for the viewer is fetched while it stays selected.
    pub cover_warm: Option<String>,
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
    /// A series in the details pane, while nothing is selected.
    pub series: Option<OpenSeries>,
    /// The series the selected part was picked from.
    pub series_back: Option<SeriesBack>,
    pub season_menu: Option<SeasonMenu>,
    /// The Series cover dialog, for a series of a type.
    pub cover_dialog: Option<(MediaType, String)>,
    pub scroll: Scroll,
    /// How far Home is scrolled, in px, to return to after a page.
    pub home_offset: f32,
    /// Where each group or section starts in the list, from its top, and
    /// how tall the list's visible part is.
    pub group_tops: Vec<f32>,
    pub viewport: f32,
    /// How wide the list's scroll area is, for the rows of a grid.
    pub list_width: f32,
    /// The section last jumped to from the section rail, by its label:
    /// the rail marks it while the list rests where the jump left it.
    pub rail_pick: Option<String>,
    /// The mouse button is held on the section rail: the bubble shows.
    pub rail_drag: bool,
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
    /// The panel button by the view switch, the drawer's pin or Ctrl+I:
    /// pins or unpins the details panel for this view.
    TogglePanel,
    /// The drawer's ✕: nothing selected, the drawer gone.
    ClosePanel,
    /// At a glance's "Another": a different pick from Want.
    AnotherPick,
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
    /// A label of the section rail, by a press, the held button moving
    /// onto it (`held`) or Enter: the app scrolls the list to `offset`.
    JumpToSection {
        label: String,
        offset: f32,
        held: bool,
    },
    /// The button let go after a press on the section rail.
    RailReleased,
    OpenPage(Page),
    ClosePage,
    /// A series row, or an entry's "All N volumes" with that entry to
    /// mark: the series in the details pane. A second click on its row
    /// closes it.
    OpenSeries(OpenSeries),
    /// A part in the series pane: its details, with ‹ back to the series.
    OpenSeriesPart(String),
    /// The ‹ series name above a part's details.
    BackToSeries,
    /// Home scrolled to this offset.
    HomeScrolled(f32),
    SeasonMenu(Option<SeasonMenu>),
    OpenSeriesCover(MediaType, String),
    CloseSeriesCover,
    /// A part in the Series cover dialog: its cover stands for the series.
    PickSeriesCover(String),
}

/// How many of a long list's covers are asked for at a time: the ones
/// scrolled to and about a screen either side; the rest follow as the
/// list scrolls.
const COVERS_AT_A_TIME: usize = 80;

/// The positions of a list of `count` covers to ask for, scrolled as
/// `scroll` says: the ones on screen first, then the next screen down,
/// then the one above, at most `COVERS_AT_A_TIME`. The rows are spread
/// evenly over the content's height, close enough with the section
/// headers since a screen's worth either side is asked for too. Before
/// the list has said how it is scrolled, it is at the top.
fn covers_on_screen(count: usize, scroll: Scroll) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    let (first, last) = if scroll.content > 0.0 && scroll.viewport > 0.0 {
        let at = |y: f32| ((y / scroll.content).clamp(0.0, 1.0) * count as f32) as usize;
        let first = at(scroll.offset).min(count - 1);
        let last = at(scroll.offset + scroll.viewport).clamp(first + 1, count);
        (first, last)
    } else {
        (0, count.min(COVERS_AT_A_TIME))
    };
    let screen = (last - first).max(1);
    let below = last..(last + screen).min(count);
    let above = first.saturating_sub(screen)..first;
    (first..last)
        .chain(below)
        .chain(above.rev())
        .take(COVERS_AT_A_TIME)
        .collect()
}

/// What the library draws from the lookups now: the inspector's entry,
/// a page's episodes or series, and the covers or photos of the list on
/// show in a grid or list view (a text view fetches nothing).
pub fn wanted(app: &App, wants: &mut Wants) {
    let library = app.repo.library();
    let state = &app.library;
    if let Some(id) = &state.selected {
        super::details_wants(app, id, wants);
    }
    match &state.page {
        Some(Page::Season { id, number }) => {
            wants.episodes.push((id.clone(), *number));
            return;
        }
        Some(Page::Seasons { .. }) => return,
        None => {}
    }
    if let Some(open) = state.series.as_ref().filter(|_| state.selected.is_none()) {
        let parts = crate::series::entries_in(library, open.media, &open.name);
        if let Some(first) = parts.first() {
            wants.series.push(first.id.clone());
        }
        if let Some(lead) = crate::series::cover_entry(&parts) {
            wants.covers.push(lead.id.clone());
        }
    }
    if let Some((media, name)) = &state.cover_dialog {
        let parts = crate::series::entries_in(library, *media, name);
        wants.covers.extend(parts.iter().map(|e| e.id.clone()));
    }
    let view = state.view_mode();
    if state.is_overview() {
        overview::wanted(app, view, wants);
        return;
    }
    // At a glance's pick, while the pinned panel shows it.
    if !state.pane_open() && panel(app) == Panel::Pinned {
        let listing = state.listing(library, app.today);
        if let Some(entry) = panel::pick(&listing, state.pick) {
            wants.covers.push(entry.id.clone());
        }
    }
    if view == View::Text {
        return;
    }
    if state.single() == Some(MediaType::Music)
        && state.artist.is_none()
        && state.music_tab == MusicTab::Artists
    {
        let listing = state.listing(library, app.today);
        let artists = crate::music::artists(&listing.shown);
        wants.artists.extend(
            covers_on_screen(artists.len(), state.scroll)
                .into_iter()
                .map(|i| artists[i].name.to_owned()),
        );
        return;
    }
    let order = state.cover_order(library, app.today);
    wants.covers.extend(
        covers_on_screen(order.len(), state.scroll)
            .into_iter()
            .map(|i| order[i].clone()),
    );
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
        self.deselect();
        self.popup = None;
        self.picker = None;
        self.season_menu = None;
        self.cover_dialog = None;
    }

    pub fn update(&mut self, message: Message) {
        // The rail's bubble is there while the button is held on it:
        // anything but the list following along ends it, should the
        // release never arrive.
        if !matches!(
            message,
            Message::JumpToSection { .. } | Message::Scrolled(_) | Message::GroupsMeasured(_)
        ) {
            self.rail_drag = false;
        }
        match message {
            Message::ShowType(media) => {
                self.types = media.into_iter().collect();
                self.artist = None;
                self.status = None;
                self.deselect();
                self.popup = None;
                self.page = None;
            }
            Message::ToggleType(media) => {
                if !self.types.remove(&media) {
                    self.types.insert(media);
                }
                self.deselect();
                self.page = None;
            }
            Message::SeeAll(section) => {
                self.types.clear();
                self.deselect();
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
                self.deselect();
            }
            Message::Status(status) => {
                self.status = status;
                self.deselect();
            }
            Message::Search(query) => self.query = query,
            Message::SetView(view) => {
                self.views.insert(self.view_key(), view);
            }
            Message::TogglePanel => {
                let pinned = self.pinned();
                self.panels.insert(self.view_key(), !pinned);
            }
            Message::ClosePanel => {
                self.deselect();
                self.close_popovers();
            }
            Message::AnotherPick => self.pick = self.pick.wrapping_add(1),
            Message::Select(id) => {
                self.series = None;
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
                self.deselect();
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
            Message::Add
            | Message::JumpToGroup(_)
            | Message::PickSeriesCover(_)
            | Message::RailReleased => {}
            Message::JumpToSection {
                label,
                offset,
                held,
            } => {
                self.rail_pick = Some(label);
                self.rail_drag = held;
                // Where the list will rest, so the rail marks the section
                // before the list reports back.
                self.scroll.offset = self.resting(offset);
            }
            Message::OpenPage(page) => {
                // A page keeps the entry it was opened from selected.
                self.close_popovers();
                self.page = Some(page);
            }
            Message::ClosePage => {
                self.page = None;
                self.season_menu = None;
                self.cover_dialog = None;
            }
            Message::OpenSeries(open) => {
                self.close_popovers();
                let showing = |name: &str| Series::same(name, &open.name);
                let again = open.highlight.is_none()
                    && (self.series.as_ref().is_some_and(|s| showing(&s.name))
                        || self.back_series().is_some_and(|b| showing(&b.name)));
                self.deselect();
                if !again {
                    self.series = Some(open);
                }
            }
            Message::OpenSeriesPart(id) => {
                if let Some(open) = self.series.take() {
                    self.close_popovers();
                    self.series_back = Some(SeriesBack {
                        id: id.clone(),
                        media: open.media,
                        name: open.name,
                    });
                    self.selected = Some(id);
                }
            }
            Message::BackToSeries => {
                if let Some(back) = self.back_series().cloned() {
                    self.deselect();
                    self.close_popovers();
                    self.series = Some(OpenSeries {
                        media: back.media,
                        name: back.name,
                        highlight: Some(back.id),
                    });
                }
            }
            Message::HomeScrolled(offset) => self.home_offset = offset,
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
                self.deselect();
            }
            Message::CloseArtist => self.artist = None,
            Message::GroupsMeasured(measure) => {
                self.group_tops = measure.tops;
                self.viewport = measure.viewport;
                self.list_width = measure.width;
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

    /// Nothing in the details pane: no entry and no series.
    pub fn deselect(&mut self) {
        self.selected = None;
        self.series = None;
        self.series_back = None;
    }

    /// The selection moved (a click, the arrow keys, a save): an entry in
    /// the pane closes the series, and ‹ back to a series stays only on
    /// the part it was picked from.
    pub fn selection_moved(&mut self) {
        if self.selected.is_some() {
            self.series = None;
        }
        if self.back_series().is_none() {
            self.series_back = None;
        }
    }

    /// The series the selected part was picked from, while it is selected.
    pub fn back_series(&self) -> Option<&SeriesBack> {
        self.series_back
            .as_ref()
            .filter(|b| self.selected.as_deref() == Some(b.id.as_str()))
    }

    /// The series the details pane shows: one is open and nothing is
    /// selected.
    pub fn open_series(&self) -> Option<&OpenSeries> {
        self.series.as_ref().filter(|_| self.selected.is_none())
    }

    /// Whether the details pane is open, for an entry or a series.
    pub fn pane_open(&self) -> bool {
        self.selected.is_some() || self.series.is_some()
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

    /// Whether the details panel is pinned for this view (the owner's
    /// choice; a narrow window unpins it anyway, see [`panel`]).
    pub fn pinned(&self) -> bool {
        self.panels.get(&self.view_key()).copied().unwrap_or(true)
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

    /// The long-list rule for a grid `height` px tall, which has no row
    /// height to go by: three screens or more.
    pub fn is_tall(&self, height: f32) -> bool {
        self.force_long || (self.viewport > 0.0 && height >= 3.0 * self.viewport)
    }

    /// Where the list rests when asked to scroll to `offset`: there, or
    /// at its end when that comes first.
    fn resting(&self, offset: f32) -> f32 {
        if self.scroll.content > 0.0 {
            offset.min((self.scroll.content - self.scroll.viewport).max(0.0))
        } else {
            offset
        }
    }

    /// Whether the list rests where a jump to `offset` leaves it.
    fn rests_at(&self, offset: f32) -> bool {
        (self.resting(offset) - self.scroll.offset).abs() < 1.0
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

    /// The covers the list draws, in its order: as [`Self::display_order`],
    /// but a series folded into one row wants only the part whose cover
    /// stands for it (the one picked, else the first in the series).
    pub fn cover_order(&self, library: &Library, today: Date) -> Vec<String> {
        if self.page.is_none()
            && !self.is_overview()
            && self.single().is_some_and(|m| m != MediaType::Music)
        {
            let listing = self.listing(library, today);
            return listing
                .rows
                .iter()
                .map(|row| match row {
                    Row::Entry(entry) => entry.id.clone(),
                    Row::Series(group) => group.cover_entry().id.clone(),
                })
                .collect();
        }
        self.display_order(library, today)
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

pub fn view<'a>(app: &'a App) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let library = app.repo.library();
    let listing = state.listing(library, app.today);

    let mode = panel(app);
    // The toolbar and the chips (or a page's back row) span the content's
    // full width and never move; the details panel starts under them.
    let (top, body): (Element<'a, AppMessage>, Element<'a, AppMessage>) =
        if let Some(page) = &state.page {
            page::view(app, page)
        } else if state.is_overview() {
            (toolbar::toolbar(app, &listing), overview::body(app))
        } else {
            (
                column![
                    toolbar::toolbar(app, &listing),
                    toolbar::chips(app, &listing)
                ]
                .into(),
                entries::body(app, &listing),
            )
        };
    // Unpinned, the details slide in as a drawer over the list's right
    // side; the layer is there either way, so the list keeps its place in
    // the tree. Pinned, only the list and the footer narrow for the panel.
    let drawer = match mode {
        Panel::Drawer => panel::drawer(app),
        _ => None,
    };
    let under = iced::widget::stack![
        column![body, footer(app)],
        drawer.unwrap_or_else(|| iced::widget::space().into())
    ];
    let mut lower = row![container(under).width(Fill)];
    if mode == Panel::Pinned {
        lower = lower.push(panel::side(app, &listing));
    }
    let main: Element<'a, AppMessage> = column![top, lower].into();
    let picker = match (&state.picker, &state.cover_dialog) {
        (_, Some((media, name))) => Some(entry::series::cover_dialog(app, *media, name)),
        (Some(picker), None) => Some(tags::picker(app, picker, &listing)),
        (None, None) => None,
    };
    let close = if state.cover_dialog.is_some() {
        AppMessage::Library(Message::CloseSeriesCover)
    } else {
        AppMessage::Library(Message::ClosePicker)
    };
    ui::modal(shell(app, main), picker, close, &tokens)
}

/// Scrolls the list, or Home, back to where it was before a page took
/// its place: the scroll area is a new one when it comes back.
pub fn restore_scroll<T: Send + 'static>(state: &State) -> iced::Task<T> {
    let (id, y) = if state.is_overview() {
        (HOME_ID, state.home_offset)
    } else {
        (LIST_ID, state.scroll.offset)
    };
    let offset = iced::widget::scrollable::AbsoluteOffset {
        x: None,
        y: Some(y),
    };
    iced::widget::operation::scroll_to(id, offset)
}

/// Keeps entry `id` in view in the list after the arrow keys moved the
/// selection, or the panel was pinned or unpinned: the list keeps its
/// offset, but the grid's columns change with its width, so the entry may
/// have moved. Scrolls only when
/// it is partly out of view, just enough to show it clear of the pinned
/// header.
pub fn reveal<T: Send + 'static>(state: &State, id: &str) -> iced::Task<T> {
    if state.is_overview() || state.page.is_some() {
        return iced::Task::none();
    }
    iced::advanced::widget::operate(reveal::Reveal::new(LIST_ID, &entries::row_id(id)))
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
    fn a_folded_series_wants_the_cover_that_stands_for_it() {
        let library = library();
        let mut state = State {
            force_long: true,
            ..State::default()
        };
        state.update(Message::ShowType(Some(MediaType::Movies)));
        let listing = state.listing(&library, today());
        let Some(Row::Series(group)) = listing.rows.iter().find(|r| matches!(r, Row::Series(_)))
        else {
            panic!("no series row");
        };
        let order = state.cover_order(&library, today());
        assert_eq!(order.len(), listing.rows.len());
        assert!(order.contains(&group.parts[0].id));
        for part in &group.parts[1..] {
            assert!(!order.contains(&part.id));
        }
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

    #[test]
    fn a_series_opens_in_the_pane_and_a_part_picked_from_it_comes_back() {
        use crate::app::{KeyPress, Message as App};
        const FELLOWSHIP: &str = "movies/the-lord-of-the-rings-the-fellowship-of-the-ring-2001";
        let lotr = |highlight: Option<&str>| {
            App::Library(Message::OpenSeries(OpenSeries {
                media: MediaType::Movies,
                name: "The Lord of the Rings".into(),
                highlight: highlight.map(Into::into),
            }))
        };
        let mut app = crate::app::App::new(crate::screens::Screen::Library);
        let _ = app.update(App::Library(Message::ShowType(Some(MediaType::Movies))));
        let _ = app.update(App::Library(Message::Select(
            "movies/the-mummy-1999".into(),
        )));
        let _ = app.update(lotr(None));
        assert_eq!(app.library.selected, None);
        assert!(app.library.open_series().is_some());
        let _ = app.update(lotr(None));
        assert!(
            !app.library.pane_open(),
            "a second click on its row closes it"
        );

        let _ = app.update(lotr(None));
        let _ = app.update(App::Library(Message::OpenSeriesPart(FELLOWSHIP.into())));
        assert_eq!(app.library.selected.as_deref(), Some(FELLOWSHIP));
        assert!(app.library.open_series().is_none());
        assert!(app.library.back_series().is_some());
        let _ = app.update(App::Library(Message::BackToSeries));
        let open = app.library.open_series().unwrap();
        assert_eq!(open.highlight.as_deref(), Some(FELLOWSHIP));
        assert_eq!(app.library.selected, None);

        // Another entry drops the way back; the arrow keys close the
        // series.
        let _ = app.update(App::Library(Message::OpenSeriesPart(FELLOWSHIP.into())));
        let _ = app.update(App::Library(Message::Select(
            "movies/the-mummy-1999".into(),
        )));
        assert!(app.library.series_back.is_none());
        let _ = app.update(lotr(None));
        let _ = app.update(App::Key(KeyPress::Down));
        assert!(app.library.selected.is_some());
        assert!(app.library.series.is_none());

        // "All N volumes" from a part opens the series marking it.
        let _ = app.update(App::Library(Message::Select(FELLOWSHIP.into())));
        let _ = app.update(lotr(Some(FELLOWSHIP)));
        assert!(app.library.open_series().is_some());
    }

    #[test]
    fn a_page_closed_scrolls_back_where_the_list_was() {
        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Shows)));
        state.update(Message::Scrolled(Scroll {
            offset: 640.0,
            viewport: 500.0,
            content: 3000.0,
        }));
        state.update(Message::OpenPage(Page::Seasons {
            id: "shows/x".into(),
        }));
        state.update(Message::ClosePage);
        assert_eq!(state.scroll.offset, 640.0, "kept for the way back");
    }

    #[test]
    fn the_covers_asked_for_follow_the_scroll() {
        let scroll = |offset| Scroll {
            offset,
            viewport: 1000.0,
            content: 10_000.0,
        };
        // At the top: the first screen, then the next one down.
        assert_eq!(
            covers_on_screen(1000, scroll(0.0)),
            (0..200).take(80).collect::<Vec<_>>()
        );
        // Halfway down a list of 400: the screen on show first, then the
        // one below, then the one above, nearest first.
        let wanted = covers_on_screen(400, scroll(5000.0));
        let expected: Vec<usize> = (200..240).chain(240..280).chain((160..200).rev()).collect();
        assert_eq!(wanted, expected[..80]);
        // At the bottom nothing past the end, and the screen above instead.
        let wanted = covers_on_screen(400, scroll(9000.0));
        assert_eq!(wanted[..40], (360..400).collect::<Vec<_>>());
        assert_eq!(wanted[40], 359);
        // Before the list has measured itself, the top.
        assert_eq!(
            covers_on_screen(10, Scroll::default()),
            (0..10).collect::<Vec<_>>()
        );
        assert!(covers_on_screen(0, scroll(0.0)).is_empty());
    }

    #[test]
    fn a_scrolled_list_asks_for_the_covers_scrolled_to() {
        let mut app = App::new(crate::screens::Screen::Library);
        app.library
            .update(Message::ShowType(Some(MediaType::Movies)));
        // Unpinned, so At a glance's pick adds no cover of its own.
        app.library.update(Message::TogglePanel);
        let order = app.library.cover_order(app.repo.library(), app.today);
        let last = order.last().unwrap().clone();
        assert_eq!(order.len(), 13, "the fixture's movies");
        app.library.update(Message::Scrolled(Scroll {
            offset: 900.0,
            viewport: 100.0,
            content: 1000.0,
        }));
        let wants = crate::screens::wanted(&app);
        // The bottom tenth on screen, then a screen's worth above it; the
        // top of the list waits for the scroll.
        assert!(wants.covers.contains(&last));
        assert!(!wants.covers.contains(&order[0]));
        assert_eq!(wants.covers.len(), 4);
    }
}
