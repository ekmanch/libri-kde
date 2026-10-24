//! An entry's details and everything that changes one: the details pane
//! beside the list, the Add and Edit dialog with its tag picker, Add to
//! plan and the delete confirmation.

pub mod details;
mod dialogs;
mod form;

pub use form::LOOKUP_ID;

use std::collections::{BTreeMap, BTreeSet};

use iced::widget::text_editor;
use iced::{Element, Task};

use crate::app::{App, Message as AppMessage};
use crate::lookup::{self, Match};
use crate::model::{
    BookFormat, Entry, EntryKind, EntryStatus, Library, MediaType, PlanItem, Released, TagCategory,
    TagPicks, text,
};
use crate::music;
use crate::repo::Operation;
use crate::tags::{self, PickerMessage, PickerState};
use crate::ui;

/// The dialog open over the library, if any.
#[derive(Debug)]
pub enum Dialog {
    Form(Box<Form>),
    Delete { id: String },
    AddToPlan { id: String },
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
    pub suggested: Vec<String>,
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
    Edit(String),
    /// Edit with the tag picker open: the details' "+ Tag".
    EditTags(String),
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
    /// The replace question's answers.
    ReplaceKeepBoth,
    ReplaceConfirm,
    ReplaceCancel,
    // The other dialogs.
    ConfirmDelete,
    TogglePlan(String),
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
            status: EntryStatus::Want,
            rating: None,
            rating_open: false,
            tags: Vec::new(),
            tag_categories: BTreeMap::new(),
            new_tags: Vec::new(),
            recategorized: BTreeMap::new(),
            suggested: Vec::new(),
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
            title: entry.title.clone(),
            creator: entry.creator.clone().unwrap_or_default(),
            released: entry.released.map(|r| r.to_string()).unwrap_or_default(),
            platforms_on: platforms.iter().cloned().collect(),
            platforms,
            formats: entry.formats.iter().flatten().copied().collect(),
            kind: entry.kind.unwrap_or(EntryKind::Album),
            album: entry.album.clone().unwrap_or_default(),
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
        lookup::search(self.media, &self.query)
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
        self.platforms = found
            .platforms
            .iter()
            .map(|p| text::normalize_platform(p))
            .collect();
        self.platforms_on = match default_platform {
            Some(default) if self.platforms.iter().any(|p| p == default) => {
                BTreeSet::from([default.to_owned()])
            }
            _ => self.platforms.iter().cloned().collect(),
        };
        // Tags the library has go on at once; new ones are offered.
        self.tags.clear();
        self.tag_categories.clear();
        self.suggested.clear();
        for tag in found.tags {
            if library.all_tags().any(|t| t == *tag) {
                self.tick(tag, None, library);
            } else {
                self.suggested.push((*tag).to_owned());
            }
        }
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
        !self.title.trim().is_empty() && self.released_is_valid()
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
        let creator = Some(self.creator.trim().to_owned()).filter(|c| !c.is_empty());
        let id = match base {
            Some(entry) => entry.id.clone(),
            None => text::entry_id(
                self.media,
                self.title.trim(),
                released.map(|r| r.year),
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
        } else if base.is_none() {
            entry.source = self.source.clone();
        }
        entry.media = self.media;
        entry.title = self.title.trim().to_owned();
        entry.creator = creator;
        entry.released = released;
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
        // An anime keeps its series-or-movie kind (edited in phase 1i); any
        // other type but music has none.
        entry.kind = match self.media {
            MediaType::Music => Some(self.kind),
            MediaType::Anime => base
                .and_then(|b| b.kind)
                .filter(|k| matches!(k, EntryKind::Series | EntryKind::Movie)),
            _ => None,
        };
        let album = self.album.trim();
        entry.album = (is(MediaType::Music) && self.kind == EntryKind::Song && !album.is_empty())
            .then(|| album.to_owned());
        if !is(MediaType::Music) || self.kind == EntryKind::Song {
            entry.tracks = None;
        }
        if !entry.uses_seasons() {
            entry.seasons = None;
        }
        if !is(MediaType::Books) {
            entry.pages = None;
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
        Message::Edit(id) | Message::EditTags(id) if app.repo.library().entry(&id).is_none() => {}
        Message::Edit(id) => {
            let entry = app.repo.library().entry(&id).expect("checked above");
            app.dialog = Some(Dialog::Form(Box::new(Form::edit(entry))));
        }
        Message::EditTags(id) => {
            let entry = app.repo.library().entry(&id).expect("checked above");
            let mut form = Form::edit(entry);
            form.picker = Some(PickerState::default());
            app.dialog = Some(Dialog::Form(Box::new(form)));
        }
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
            let Some(Dialog::Form(form)) = &mut app.dialog else {
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
        }
        Message::Query(query) => {
            form.query = query;
            form.picked = None;
        }
        Message::Pick(index) => {
            if let Some(found) = form.matches().get(index).copied() {
                form.picked = Some(index);
                form.fill(found, library, platform);
            }
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
        Message::Released(released) => form.released = released,
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
        Message::Status(status) => form.status = status,
        Message::ToggleRating => form.rating_open = !form.rating_open,
        Message::Rate(rating) => {
            form.rating = rating;
            form.rating_open = false;
        }
        Message::RemoveTag(tag, category) => form.untick(&tag, category, library),
        Message::AddSuggested(tag) => {
            form.suggested.retain(|t| *t != tag);
            if library.all_tags().any(|t| t == tag) {
                form.tick(&tag, None, library);
            } else {
                // A new tag still goes through the confirmation.
                form.new_tag = Some(NewTag {
                    name: tag,
                    categories: vec![TagCategory::default_for(form.media)],
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

            // New tags are filed, then category changes made in the picker
            // are applied, right before the entry is saved, as on the phone.
            let mut operations: Vec<Operation> = form
                .new_tags
                .iter()
                .filter(|(name, _)| form.tags.contains(name))
                .map(|(name, categories)| Operation::CreateTag {
                    name: name.clone(),
                    categories: categories.clone(),
                })
                .collect();
            operations.extend(
                form.recategorized
                    .iter()
                    .filter(|(name, categories)| library.categories_of(name) != **categories)
                    .map(|(name, categories)| Operation::SetTagCategories {
                        name: name.clone(),
                        categories: categories.clone(),
                    }),
            );
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
        Some(Dialog::Form(form)) => {
            if form.replace.take().is_none()
                && form.new_tag.take().is_none()
                && form.categories.take().is_none()
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
        Some(Dialog::Form(form)) if form.new_tag.is_some() => Message::ConfirmNewTag,
        Some(Dialog::Form(form)) if form.categories.is_some() => Message::SaveCategories,
        Some(Dialog::Form(form)) if form.picker.is_some() => Message::ClosePicker,
        Some(Dialog::Form(form)) if form.picked.is_none() && !form.matches().is_empty() => {
            Message::Pick(0)
        }
        Some(Dialog::Delete { .. }) => Message::ConfirmDelete,
        Some(Dialog::AddToPlan { .. }) => Message::Close,
        _ => return Task::none(),
    };
    update(app, message)
}

/// The open dialog over `base`.
pub fn overlay<'a>(app: &'a App, base: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let dialog = app.dialog.as_ref().map(|dialog| match dialog {
        Dialog::Form(form) => form::view(app, form),
        Dialog::Delete { id } => dialogs::delete(app, id),
        Dialog::AddToPlan { id } => dialogs::add_to_plan(app, id),
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
            Some(Dialog::Form(form)) => form,
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
            filled.suggested.contains(&"soulslike".to_owned()),
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

    fn edit_baby(app: &mut App) {
        send(
            app,
            Message::EditTags("movies/million-dollar-baby-2004".into()),
        );
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
}
