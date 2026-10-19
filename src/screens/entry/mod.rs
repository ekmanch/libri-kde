//! An entry's details and everything that changes one: the details pane
//! beside the list, the Add and Edit dialog with its tag picker, Add to
//! plan and the delete confirmation.

pub mod details;
mod dialogs;
mod form;

pub use form::LOOKUP_ID;

use std::collections::BTreeSet;

use iced::widget::text_editor;
use iced::{Element, Task};

use crate::app::{App, Message as AppMessage};
use crate::lookup::{self, Match};
use crate::model::{
    BookFormat, Entry, EntryStatus, Library, MediaType, MusicKind, PlanItem, Released, TagCategory,
    text,
};
use crate::repo::Operation;
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
    pub kind: MusicKind,
    pub album: String,
    pub status: EntryStatus,
    pub rating: Option<u8>,
    pub rating_open: bool,
    pub tags: Vec<String>,
    /// Tags created in this form, filed when it is saved.
    pub new_tags: Vec<(String, TagCategory)>,
    /// The match's tag suggestions not taken yet.
    pub suggested: Vec<String>,
    pub description: text_editor::Content,
    pub note: String,
    /// "Also add to plan", a plan id.
    pub also_plan: Option<String>,
    pub picker: Option<TagPicker>,
    pub new_tag: Option<NewTag>,
}

/// The form's tag picker.
#[derive(Debug, Default)]
pub struct TagPicker {
    pub query: String,
    /// Groups folded or unfolded by hand, against their default.
    pub toggled: BTreeSet<TagCategory>,
}

/// Confirming a new tag and picking its category.
#[derive(Debug, Clone)]
pub struct NewTag {
    pub name: String,
    pub category: TagCategory,
}

#[derive(Debug, Clone)]
pub enum Message {
    // The details pane.
    SetStatus(String, EntryStatus),
    SetRating(String, Option<u8>),
    ToggleDetailsRating,
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
    Kind(MusicKind),
    Album(String),
    Status(EntryStatus),
    ToggleRating,
    Rate(Option<u8>),
    RemoveTag(String),
    AddSuggested(String),
    Description(text_editor::Action),
    Note(String),
    AlsoPlan(Option<String>),
    Save,
    // The form's tag picker and new-tag confirmation.
    OpenPicker,
    ClosePicker,
    PickerQuery(String),
    PickerToggleGroup(TagCategory),
    PickerToggleTag(String),
    PickerClear,
    CreateTag,
    NewTagCategory(TagCategory),
    ConfirmNewTag,
    CancelNewTag,
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
            kind: MusicKind::Album,
            album: String::new(),
            status: EntryStatus::Want,
            rating: None,
            rating_open: false,
            tags: Vec::new(),
            new_tags: Vec::new(),
            suggested: Vec::new(),
            description: text_editor::Content::new(),
            note: String::new(),
            also_plan: None,
            picker: None,
            new_tag: None,
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
            kind: entry.kind.unwrap_or(MusicKind::Album),
            album: entry.album.clone().unwrap_or_default(),
            status: entry.status,
            rating: entry.rating,
            tags: entry.tags.clone(),
            description: text_editor::Content::with_text(&entry.description),
            note: entry.note.clone(),
            ..Form::add(entry.media)
        }
    }

    /// The lookup's results for the current query and type.
    pub fn matches(&self) -> Vec<&'static Match> {
        lookup::search(self.media, &self.query)
    }

    /// Fills the form from a match. Every field stays editable.
    fn fill(&mut self, found: &Match, library: &Library) {
        self.source = Some(found.source.to_owned());
        self.title = found.title.to_owned();
        self.creator = found.creator.to_owned();
        self.released = found.released.to_owned();
        self.description = text_editor::Content::with_text(found.description);
        self.kind = found.kind.unwrap_or(MusicKind::Album);
        self.platforms = found
            .platforms
            .iter()
            .map(|p| text::normalize_platform(p))
            .collect();
        self.platforms_on = self.platforms.iter().cloned().collect();
        // Tags the library has go on at once; new ones are offered.
        self.tags.clear();
        self.suggested.clear();
        for tag in found.tags {
            if library.all_tags().any(|t| t == *tag) {
                self.tags.push((*tag).to_owned());
            } else {
                self.suggested.push((*tag).to_owned());
            }
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
            note: String::new(),
            description: String::new(),
            seasonal_only: false,
            added: today,
            source: None,
            platform: None,
            formats: None,
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
        entry.note = self.note.trim().to_owned();
        entry.description = self.description.text().trim().to_owned();
        entry.platform =
            (is(MediaType::Games) && !platform.is_empty()).then(|| platform.join(", "));
        entry.formats = (is(MediaType::Books) && !self.formats.is_empty())
            .then(|| self.formats.iter().copied().collect());
        entry.kind = is(MediaType::Music).then_some(self.kind);
        let album = self.album.trim();
        entry.album = (is(MediaType::Music) && self.kind == MusicKind::Song && !album.is_empty())
            .then(|| album.to_owned());
        if !is(MediaType::Music) || self.kind == MusicKind::Song {
            entry.tracks = None;
        }
        if !is(MediaType::Shows) {
            entry.seasons = None;
        }
        entry
    }
}

/// Opens the dialog for `message` or changes the open one. Returns any
/// task to run (none for now).
pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::SetStatus(id, status) => app.apply(Operation::SetStatus { id, status }),
        Message::SetRating(id, rating) => {
            app.apply(Operation::SetRating { id, rating });
            app.library.details_rating_open = false;
        }
        Message::ToggleDetailsRating => {
            app.library.details_rating_open = !app.library.details_rating_open
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
            form.picker = Some(TagPicker::default());
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
            let Some(Dialog::Form(form)) = &mut app.dialog else {
                return Task::none();
            };
            if let Some(operations) = update_form(form, message, &library, today) {
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
fn update_form(
    form: &mut Form,
    message: Message,
    library: &Library,
    today: crate::model::Date,
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
                form.fill(found, library);
            }
        }
        Message::MovePick(step) => {
            let count = form.matches().len();
            if count > 0 {
                let next = form.picked.map_or(0, |i| {
                    (i as isize + step).rem_euclid(count as isize) as usize
                });
                return update_form(form, Message::Pick(next), library, today);
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
        Message::RemoveTag(tag) => {
            form.tags.retain(|t| *t != tag);
            form.new_tags.retain(|(t, _)| *t != tag);
        }
        Message::AddSuggested(tag) => {
            form.suggested.retain(|t| *t != tag);
            if library.all_tags().any(|t| t == tag) {
                form.tags.push(tag);
            } else {
                // A new tag still goes through the confirmation.
                form.new_tag = Some(NewTag {
                    name: tag,
                    category: TagCategory::default_for(form.media),
                });
            }
        }
        Message::Description(action) => form.description.perform(action),
        Message::Note(note) => form.note = note,
        Message::AlsoPlan(plan) => form.also_plan = plan,
        Message::OpenPicker => form.picker = Some(TagPicker::default()),
        Message::ClosePicker => form.picker = None,
        Message::PickerQuery(query) => {
            if let Some(picker) = &mut form.picker {
                picker.query = query;
            }
        }
        Message::PickerToggleGroup(category) => {
            if let Some(picker) = &mut form.picker
                && !picker.toggled.remove(&category)
            {
                picker.toggled.insert(category);
            }
        }
        Message::PickerToggleTag(tag) => {
            if form.tags.contains(&tag) {
                form.tags.retain(|t| *t != tag);
                form.new_tags.retain(|(t, _)| *t != tag);
            } else {
                form.tags.push(tag);
            }
        }
        Message::PickerClear => {
            form.tags.clear();
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
                    category: TagCategory::default_for(form.media),
                });
            }
        }
        Message::NewTagCategory(category) => {
            if let Some(new_tag) = &mut form.new_tag {
                new_tag.category = category;
            }
        }
        Message::ConfirmNewTag => {
            if let Some(NewTag { name, category }) = form.new_tag.take() {
                if !form.tags.contains(&name) {
                    form.tags.push(name.clone());
                }
                form.new_tags.push((name, category));
                if let Some(picker) = &mut form.picker {
                    picker.query.clear();
                }
            }
        }
        Message::CancelNewTag => form.new_tag = None,
        Message::Save => {
            if !form.can_save() {
                return None;
            }
            let base = form.editing.as_deref().and_then(|id| library.entry(id));
            let entry = form.entry(base, library, today);
            let mut operations: Vec<Operation> = form
                .new_tags
                .iter()
                .filter(|(name, _)| form.tags.contains(name))
                .map(|(name, category)| Operation::CreateTag {
                    name: name.clone(),
                    category: *category,
                })
                .collect();
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
            if form.new_tag.take().is_none()
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
        assert_eq!(
            entry.platform.as_deref(),
            Some("PC, PlayStation 5, Xbox Series X|S, Switch")
        );
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
        send(&mut app, Message::PickerQuery("B Movie".into()));
        send(&mut app, Message::CreateTag);
        assert_eq!(
            form(&app).new_tag.as_ref().unwrap().category,
            TagCategory::Genres
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
        send(&mut app, Message::PickerQuery("eerie".into()));
        send(&mut app, Message::CreateTag);
        assert!(dismiss_top(&mut app));
        assert!(form(&app).new_tag.is_none() && form(&app).picker.is_some());
        assert!(dismiss_top(&mut app));
        assert!(form(&app).picker.is_none());
        assert!(dismiss_top(&mut app));
        assert!(app.dialog.is_none());
        assert!(!dismiss_top(&mut app));
    }
}
