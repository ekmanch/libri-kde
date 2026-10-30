//! Plans: the overview ("All plans"), one plan as a checklist or a
//! timeline, its "All <season>" collection, and the New and Edit plan
//! dialog.

mod form;
mod overview;
mod plan;

pub use overview::plan_accent;

use std::collections::HashMap;

use iced::{Element, Task};

use crate::app::{App, Message as AppMessage};
use crate::model::{Date, EntryStatus, MediaType, Plan, PlanItem, Season};
use crate::plans::{self as rules, PlanState};
use crate::repo::Operation;
use crate::tags::{self, PickerMessage, PickerState};
use crate::ui;

/// What the Plans screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Overview,
    Plan(String),
    /// Everything with the plan's tags: "All Halloween".
    Collection(String),
}

/// How a plan's sections look, remembered per plan on this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Sections as cards rather than plain.
    pub cards: bool,
    pub covers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    Year,
    View,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub page: Page,
    pub styles: HashMap<String, Style>,
    pub popup: Option<Popup>,
    pub pool_query: String,
    /// The entry selected in the plan, whose details show.
    pub selected: Option<String>,
    /// The details' When calendar, its month, and the first click of a
    /// period.
    pub when_open: bool,
    pub month: Option<Date>,
    pub anchor: Option<Date>,
    /// The details' plan note as typed, with its plan and entry, until
    /// Enter saves it, the pane moves on (which saves it too) or Esc drops
    /// it.
    pub item_note_draft: Option<(String, String, String)>,
    /// What a tick changed on an entry, by plan and entry, so the untick
    /// can put it back: the status, and a show's seasons. App memory, as
    /// on the phone; never saved.
    pub before_done: HashMap<(String, String), BeforeDone>,
}

/// An entry's status and seasons before a plan tick marked it Done.
pub type BeforeDone = (EntryStatus, Option<Vec<Season>>);

impl State {
    /// The plan open, on its page or its collection.
    pub fn plan_id(&self) -> Option<&str> {
        match &self.page {
            Page::Plan(id) | Page::Collection(id) => Some(id),
            Page::Overview => None,
        }
    }

    /// Cards on Light, plain on the dark themes, until changed.
    pub fn style(&self, id: &str, light: bool) -> Style {
        self.styles.get(id).copied().unwrap_or(Style {
            cards: light,
            covers: false,
        })
    }

    fn close_popovers(&mut self) {
        self.popup = None;
        self.when_open = false;
        self.anchor = None;
    }

    /// Returns to `page` (Back or Forward): the selection and the
    /// popovers close.
    pub fn go_to(&mut self, page: Page) {
        self.page = page;
        self.selected = None;
        self.close_popovers();
    }
}

/// A plan dialog open over the window.
#[derive(Debug, Clone)]
pub enum Dialog {
    Form(PlanForm),
    /// D5: delete a plan; its entries stay.
    Delete {
        id: String,
    },
    /// D4: remove an entry from this plan, or delete it everywhere.
    Remove {
        plan: String,
        entry: String,
    },
}

/// The New and Edit plan dialog's fields.
#[derive(Debug, Clone, Default)]
pub struct PlanForm {
    pub editing: Option<String>,
    pub name: String,
    pub year: String,
    pub started: bool,
    pub tags: Vec<String>,
    pub seasonal: bool,
    pub timeline: bool,
    /// "Start with: Copy a plan", and the plan to copy.
    pub copy: bool,
    pub source: Option<String>,
    /// The linked hashtags view, drilled into from the form.
    pub tag_view: Option<PickerState>,
}

impl PlanForm {
    /// A new plan: no tags, the seasonal theme on and the timeline as
    /// Settings → Plans has it.
    pub fn new_plan(timeline: bool) -> PlanForm {
        PlanForm {
            seasonal: true,
            timeline,
            ..PlanForm::default()
        }
    }

    pub fn edit(plan: &Plan) -> PlanForm {
        PlanForm {
            editing: Some(plan.id.clone()),
            name: plan.name.clone(),
            year: plan.year.map(|y| y.to_string()).unwrap_or_default(),
            started: plan.started,
            tags: plan.tags.clone(),
            seasonal: plan.seasonal_theme,
            timeline: plan.timeline,
            ..PlanForm::default()
        }
    }

    /// "New year…": a copy of `plan` for the year after.
    pub fn next_year(plan: &Plan, today: Date) -> PlanForm {
        PlanForm {
            name: plan.name.clone(),
            year: rules::next_year(plan, today).to_string(),
            tags: plan.tags.clone(),
            seasonal: plan.seasonal_theme,
            timeline: plan.timeline,
            copy: true,
            source: Some(plan.id.clone()),
            ..PlanForm::default()
        }
    }

    /// The year as a number, when it is one.
    pub fn year(&self) -> Option<i32> {
        self.year
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|y| (1000..=9999).contains(y))
    }

    /// No year is fine; anything else must be a four-digit year.
    pub fn year_is_valid(&self) -> bool {
        self.year.trim().is_empty() || self.year().is_some()
    }

    pub fn can_save(&self) -> bool {
        !self.name.trim().is_empty()
            && self.year_is_valid()
            && (!self.copy || self.source.is_some())
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ShowOverview,
    ShowPlan(String),
    ShowCollection(String),
    TogglePopup(Popup),
    ClosePopup,
    /// The View popover's Layout: a timeline or a checklist. Stored on the
    /// plan, so the phone shows it too.
    SetTimeline(bool),
    SetCards(bool),
    SetCovers(bool),
    NewPlan,
    NewYear,
    EditPlan,
    ToggleDone(String),
    AddItem(String),
    PoolQuery(String),
    Select(String),
    ToggleWhen,
    Month(i32),
    PickDay(Date),
    ClearWhen,
    /// The details' plan note as typed.
    ItemNote(String),
    /// Enter in the details' plan note.
    SaveItemNote,
    /// Opens D4 for the selected entry.
    RemoveItem(String),
    RemoveFromPlan,
    DeleteEverywhere,
    // The plan dialog.
    Name(String),
    Year(String),
    Started(bool),
    Seasonal(bool),
    Timeline(bool),
    StartWith {
        copy: bool,
    },
    Source(String),
    OpenTags,
    CloseTags,
    /// The linked hashtags picker; a tag is toggled by name, whatever its
    /// categories.
    Picker(PickerMessage),
    TagToggle(String),
    TagClear,
    Save,
    Close,
    DeletePlan,
    ConfirmDeletePlan,
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    if let Message::Picker(PickerMessage::Jump(offset)) = &message {
        return tags::jump(*offset);
    }
    let today = app.today;
    let state = &mut app.plans;
    match message {
        Message::ShowOverview => {
            state.page = Page::Overview;
            state.selected = None;
            state.close_popovers();
        }
        Message::ShowPlan(id) => {
            state.page = Page::Plan(id);
            state.selected = None;
            state.close_popovers();
        }
        Message::ShowCollection(id) => {
            state.page = Page::Collection(id);
            state.selected = None;
            state.close_popovers();
        }
        Message::TogglePopup(popup) => {
            state.popup = if state.popup == Some(popup) {
                None
            } else {
                Some(popup)
            };
        }
        Message::ClosePopup => state.popup = None,
        Message::SetTimeline(on) => {
            if let Some(mut plan) = current_plan(app).cloned() {
                plan.timeline = on;
                app.apply(Operation::UpdatePlan(plan));
            }
        }
        Message::SetCards(cards) => set_style(app, |style| style.cards = cards),
        Message::SetCovers(covers) => set_style(app, |style| style.covers = covers),
        Message::NewPlan => {
            state.close_popovers();
            app.plan_dialog = Some(Dialog::Form(PlanForm::new_plan(app.settings.timeline)));
        }
        Message::NewYear => {
            state.close_popovers();
            if let Some(plan) = current_plan(app) {
                app.plan_dialog = Some(Dialog::Form(PlanForm::next_year(plan, today)));
            }
        }
        Message::EditPlan => {
            state.close_popovers();
            if let Some(plan) = current_plan(app) {
                app.plan_dialog = Some(Dialog::Form(PlanForm::edit(plan)));
            }
        }
        Message::ToggleDone(entry) => {
            if let Some(plan) = current_plan(app) {
                let done = plan.item_for(&entry).is_some_and(|i| !i.done);
                let plan = plan.id.clone();
                toggle_done(app, plan, entry, done);
            }
        }
        Message::AddItem(entry) => {
            if let Some(plan) = current_plan(app) {
                let done = app
                    .repo
                    .library()
                    .entry(&entry)
                    .is_some_and(|e| e.status == EntryStatus::Done);
                let item = PlanItem {
                    entry,
                    done: false,
                    repeat: done,
                    note: String::new(),
                    when: None,
                };
                let plan = plan.id.clone();
                app.apply(Operation::SetPlanItem { plan, item });
            }
        }
        Message::PoolQuery(query) => state.pool_query = query,
        Message::Select(id) => {
            state.selected = if state.selected.as_deref() == Some(id.as_str()) {
                None
            } else {
                Some(id)
            };
            state.close_popovers();
            app.library.details_rating_open = false;
        }
        Message::ToggleWhen => {
            state.when_open = !state.when_open;
            state.anchor = None;
            state.month = None;
        }
        Message::Month(by) => {
            let shown = shown_month(app);
            app.plans.month = Some(ui::calendar::shift_month(shown, by));
        }
        Message::PickDay(day) => {
            let when = ui::calendar::pick(state.anchor, day);
            // The first click picks a day and waits for a possible end.
            state.anchor = if when.is_single_day() {
                Some(day)
            } else {
                None
            };
            update_selected_item(app, |item| item.when = Some(when));
        }
        Message::ClearWhen => {
            state.anchor = None;
            update_selected_item(app, |item| item.when = None);
        }
        Message::ItemNote(note) => {
            if let (Some(plan), Some(entry)) = (
                app.plans.plan_id().map(str::to_owned),
                app.plans.selected.clone(),
            ) {
                app.plans.item_note_draft = Some((plan, entry, note));
            }
        }
        Message::SaveItemNote => save_item_note(app),
        Message::RemoveItem(entry) => {
            if let Some(plan) = app.plans.plan_id() {
                let plan = plan.to_owned();
                app.plans.close_popovers();
                app.library.close_popovers();
                app.plan_dialog = Some(Dialog::Remove { plan, entry });
            }
        }
        Message::RemoveFromPlan | Message::DeleteEverywhere => {
            if let Some(Dialog::Remove { plan, entry }) = app.plan_dialog.take() {
                app.plans.selected = None;
                app.apply(match message {
                    Message::RemoveFromPlan => Operation::RemovePlanItem { plan, entry },
                    _ => Operation::DeleteEntry { id: entry },
                });
            }
        }
        Message::DeletePlan => {
            if let Some(Dialog::Form(PlanForm {
                editing: Some(id), ..
            })) = &app.plan_dialog
            {
                app.plan_dialog = Some(Dialog::Delete { id: id.clone() });
            }
        }
        Message::ConfirmDeletePlan => {
            if let Some(Dialog::Delete { id }) = app.plan_dialog.take() {
                if app.plans.plan_id() == Some(id.as_str()) {
                    app.plans.page = Page::Overview;
                    app.plans.selected = None;
                }
                app.apply(Operation::DeletePlan { id });
            }
        }
        Message::Close => app.plan_dialog = None,
        Message::Save => return save(app),
        message => {
            if let Some(Dialog::Form(form)) = &mut app.plan_dialog {
                update_form(form, message);
            }
        }
    }
    Task::none()
}

fn update_form(form: &mut PlanForm, message: Message) {
    match message {
        Message::Name(name) => form.name = name,
        Message::Year(year) => form.year = year,
        Message::Started(on) => form.started = on,
        Message::Seasonal(on) => form.seasonal = on,
        Message::Timeline(on) => form.timeline = on,
        Message::StartWith { copy } => form.copy = copy,
        Message::Source(id) => form.source = Some(id),
        Message::OpenTags => form.tag_view = Some(PickerState::default()),
        Message::CloseTags => form.tag_view = None,
        Message::Picker(message) => {
            if let Some(view) = &mut form.tag_view {
                view.update(&message);
            }
            if let PickerMessage::Toggle(tag, _) = message {
                update_form(form, Message::TagToggle(tag));
            }
        }
        Message::TagToggle(tag) => {
            if let Some(at) = form.tags.iter().position(|t| *t == tag) {
                form.tags.remove(at);
            } else {
                form.tags.push(tag);
            }
        }
        Message::TagClear => form.tags.clear(),
        _ => {}
    }
}

/// Saves the plan dialog: a new plan (empty or a copy) opens it; an edit
/// keeps its items.
fn save(app: &mut App) -> Task<AppMessage> {
    let Some(Dialog::Form(form)) = &app.plan_dialog else {
        return Task::none();
    };
    if !form.can_save() {
        return Task::none();
    }
    let library = app.repo.library();
    let name = form.name.trim().to_owned();
    let year = form.year();
    let (operation, id) = match form.editing.as_deref().and_then(|id| library.plan(id)) {
        Some(existing) => {
            let plan = Plan {
                name,
                year,
                tags: form.tags.clone(),
                timeline: form.timeline,
                seasonal_theme: form.seasonal,
                started: form.started,
                ..existing.clone()
            };
            (Operation::UpdatePlan(plan), existing.id.clone())
        }
        None => {
            let id = rules::plan_id(&name, year, library.plans.iter().map(|p| p.id.as_str()));
            let source = form
                .copy
                .then(|| form.source.as_deref().and_then(|s| library.plan(s)))
                .flatten();
            let mut plan = match source {
                Some(source) => rules::copy_plan(source, id.clone(), name, year),
                None => Plan {
                    id: id.clone(),
                    name,
                    year,
                    tags: Vec::new(),
                    timeline: false,
                    seasonal_theme: false,
                    started: false,
                    items: Vec::new(),
                },
            };
            plan.tags = form.tags.clone();
            plan.timeline = form.timeline;
            plan.seasonal_theme = form.seasonal;
            (Operation::AddPlan(plan), id)
        }
    };
    app.plan_dialog = None;
    app.apply(operation);
    app.plans.page = Page::Plan(id);
    app.plans.selected = None;
    Task::none()
}

/// Changes the open plan's look, kept on this device.
fn set_style(app: &mut App, change: impl FnOnce(&mut Style)) {
    let light = app.tokens().light;
    let state = &mut app.plans;
    if let Some(id) = state.plan_id().map(str::to_owned) {
        let mut style = state.style(&id, light);
        change(&mut style);
        state.styles.insert(id, style);
    }
}

fn current_plan(app: &App) -> Option<&Plan> {
    app.plans
        .plan_id()
        .and_then(|id| app.repo.library().plan(id))
}

/// The month the calendar shows: the one picked with the arrows, else the
/// item's date, else today's.
pub fn shown_month(app: &App) -> Date {
    if let Some(month) = app.plans.month {
        return month;
    }
    let item = selected_item(app);
    let day = item
        .and_then(|i| i.when.map(|w| w.from))
        .unwrap_or(app.today);
    ui::calendar::month_start(day)
}

pub fn selected_item(app: &App) -> Option<&PlanItem> {
    let entry = app.plans.selected.as_deref()?;
    current_plan(app)?.item_for(entry)
}

/// Saves the plan note typed in the details pane when it differs from the
/// saved one: on Enter, and before the pane moves on to another entry, a
/// page, a screen or a dialog.
pub fn save_item_note(app: &mut App) {
    let Some((plan, entry, note)) = app.plans.item_note_draft.take() else {
        return;
    };
    let note = note.trim().to_owned();
    let item = app
        .repo
        .library()
        .plan(&plan)
        .and_then(|p| p.item_for(&entry))
        .filter(|item| item.note != note)
        .cloned();
    if let Some(mut item) = item {
        item.note = note;
        app.apply(Operation::SetPlanItem { plan, item });
    }
}

fn update_selected_item(app: &mut App, change: impl FnOnce(&mut PlanItem)) {
    let (Some(plan), Some(mut item)) = (
        current_plan(app).map(|p| p.id.clone()),
        selected_item(app).cloned(),
    ) else {
        return;
    };
    change(&mut item);
    app.apply(Operation::SetPlanItem { plan, item });
}

/// Ticks or unticks an entry in a plan, as the phone does: a tick marks
/// the entry Done (a show: every season that is out), remembering what it
/// was; an untick puts that back while the entry is still Done.
fn toggle_done(app: &mut App, plan: String, entry: String, done: bool) {
    let today = app.today;
    let Some(found) = app.repo.library().entry(&entry).cloned() else {
        return;
    };
    app.apply(Operation::SetPlanItemDone {
        plan: plan.clone(),
        entry: entry.clone(),
        done,
    });
    let key = (plan, entry.clone());
    if done {
        if found.status == EntryStatus::Done {
            app.plans.before_done.remove(&key);
            return;
        }
        let seasons = (found.uses_seasons() && !found.season_list().is_empty())
            .then(|| found.season_list().to_vec());
        app.plans
            .before_done
            .insert(key, (found.status, seasons.clone()));
        match seasons {
            Some(seasons) => {
                let seasons = seasons
                    .into_iter()
                    .map(|mut season| {
                        if season.is_released_on(today) {
                            season.status = Some(EntryStatus::Done);
                            season.fresh = None;
                        }
                        season
                    })
                    .collect();
                app.apply(Operation::SetSeasons {
                    id: entry,
                    seasons,
                    today,
                });
            }
            None => app.apply(Operation::SetStatus {
                id: entry,
                status: EntryStatus::Done,
            }),
        }
    } else if let Some((status, seasons)) = app.plans.before_done.remove(&key)
        && found.status == EntryStatus::Done
    {
        match seasons {
            Some(seasons) => app.apply(Operation::SetSeasons {
                id: entry,
                seasons,
                today,
            }),
            None => app.apply(Operation::SetStatus { id: entry, status }),
        }
    }
}

/// Esc: the innermost layer first. False when nothing was open.
pub fn dismiss_top(app: &mut App) -> bool {
    match &mut app.plan_dialog {
        Some(Dialog::Form(form)) if form.tag_view.is_some() => {
            if !form
                .tag_view
                .as_mut()
                .is_some_and(PickerState::dismiss_inner)
            {
                form.tag_view = None;
            }
        }
        Some(_) => app.plan_dialog = None,
        None => {
            let state = &mut app.plans;
            if state.when_open {
                state.when_open = false;
            } else if state.popup.is_some() {
                state.popup = None;
            } else if state.item_note_draft.is_some() {
                state.item_note_draft = None;
            } else {
                return false;
            }
        }
    }
    true
}

/// Enter: Done in the linked hashtags, or the delete confirmation.
pub fn confirm(app: &mut App) -> Task<AppMessage> {
    let message = match &app.plan_dialog {
        Some(Dialog::Form(form)) if form.tag_view.is_some() => Message::CloseTags,
        Some(Dialog::Delete { .. }) => Message::ConfirmDeletePlan,
        _ => return Task::none(),
    };
    update(app, message)
}

/// The window title's part: "All plans", "Halloween 2026", "All Halloween".
pub fn title(app: &App) -> String {
    match (&app.plans.page, current_plan(app)) {
        (Page::Plan(_), Some(plan)) => plan.title(),
        (Page::Collection(_), Some(plan)) => collection_title(plan),
        _ => "All plans".into(),
    }
}

/// "All Halloween", or "All" and the plan's name without a season.
pub fn collection_title(plan: &Plan) -> String {
    let season = rules::season_for_tags(&plan.tags);
    format!("All {}", season.map_or(plan.name.as_str(), |s| s.name()))
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    match &app.plans.page {
        Page::Overview => overview::view(app),
        Page::Plan(_) | Page::Collection(_) => match current_plan(app) {
            Some(plan) => plan::view(app, plan),
            None => overview::view(app),
        },
    }
}

/// The season a plan shows: its own theme, unless Settings → Plans has
/// turned seasonal touches off.
pub fn shown_season(app: &App, plan: &Plan) -> Option<rules::Season> {
    if app.settings.seasonal {
        rules::season_of(plan)
    } else {
        None
    }
}

/// An arrow key on the plan page: moves the selection through the items
/// in the order shown, the checklist's (by type, then the plan's order)
/// or the timeline's.
pub fn step_selection(app: &mut App, step: isize) {
    let Some(plan) = current_plan(app) else {
        return;
    };
    let library = app.repo.library();
    let order: Vec<String> = if plan.timeline {
        rules::timeline(plan, library, app.today)
            .into_iter()
            .flat_map(|group| group.items.into_iter().map(|item| item.entry.clone()))
            .collect()
    } else {
        MediaType::ALL
            .into_iter()
            .flat_map(|media| {
                plan.items
                    .iter()
                    .filter(|item| library.entry(&item.entry).is_some_and(|e| e.media == media))
                    .map(|item| item.entry.clone())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    app.plans.selected = crate::app::next_in(&order, app.plans.selected.as_deref(), step);
}

/// The open plan dialog over `base`.
pub fn overlay<'a>(app: &'a App, base: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let dialog = app.plan_dialog.as_ref().map(|dialog| match dialog {
        Dialog::Form(form) => form::view(app, form),
        Dialog::Delete { id } => form::delete(app, id),
        Dialog::Remove { plan, entry } => form::remove(app, plan, entry),
    });
    ui::modal(base, dialog, AppMessage::Plans(Message::Close), &tokens)
}

/// Plans in the overview's groups, by state, in file order.
pub fn by_state(app: &App, state: PlanState) -> Vec<&Plan> {
    app.repo
        .library()
        .plans
        .iter()
        .filter(|p| rules::plan_state(p, app.today) == state)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;

    fn app() -> App {
        let mut app = App::new(Screen::Plans);
        app.today = Date::parse("2026-10-19").unwrap();
        app
    }

    fn send(app: &mut App, message: Message) {
        let _ = update(app, message);
    }

    #[test]
    fn new_year_copies_the_plan_and_opens_the_copy() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("halloween-2026".into()));
        send(&mut app, Message::NewYear);
        let Some(Dialog::Form(form)) = &app.plan_dialog else {
            panic!("no form")
        };
        assert_eq!((form.year.as_str(), form.copy), ("2027", true));
        send(&mut app, Message::Save);
        let library = app.repo.library();
        let copy = library.plan("halloween-2027").expect("saved");
        assert_eq!(copy.items.len(), 6, "the six not done");
        assert_eq!(app.plans.page, Page::Plan("halloween-2027".into()));
    }

    #[test]
    fn a_new_plan_needs_a_name_and_a_good_year() {
        let mut app = app();
        send(&mut app, Message::NewPlan);
        send(&mut app, Message::Year("26".into()));
        let can = |app: &App| matches!(&app.plan_dialog, Some(Dialog::Form(f)) if f.can_save());
        assert!(!can(&app));
        send(&mut app, Message::Name("Road trip".into()));
        assert!(!can(&app));
        send(&mut app, Message::Year(String::new()));
        assert!(can(&app));
        send(&mut app, Message::Save);
        assert!(app.repo.library().plan("road-trip").is_some());
    }

    #[test]
    fn ticking_adding_and_removing_items() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("christmas-2026".into()));
        send(&mut app, Message::ToggleDone("movies/die-hard-1988".into()));
        let plan = app.repo.library().plan("christmas-2026").unwrap();
        assert!(plan.item_for("movies/die-hard-1988").unwrap().done && plan.started);

        send(&mut app, Message::AddItem("movies/the-thing-1982".into()));
        let item = app
            .repo
            .library()
            .plan("christmas-2026")
            .unwrap()
            .item_for("movies/the-thing-1982")
            .cloned();
        assert!(item.is_some_and(|i| i.repeat));

        send(
            &mut app,
            Message::RemoveItem("movies/the-thing-1982".into()),
        );
        send(&mut app, Message::RemoveFromPlan);
        let library = app.repo.library();
        assert!(
            library
                .plan("christmas-2026")
                .unwrap()
                .item_for("movies/the-thing-1982")
                .is_none()
        );
        assert!(
            library.entry("movies/the-thing-1982").is_some(),
            "still in the library"
        );
    }

    #[test]
    fn the_calendar_picks_a_day_then_a_period() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("christmas-2026".into()));
        send(&mut app, Message::Select("movies/die-hard-1988".into()));
        send(&mut app, Message::ToggleWhen);
        let day = |t| Date::parse(t).unwrap();
        send(&mut app, Message::PickDay(day("2026-12-20")));
        send(&mut app, Message::PickDay(day("2026-12-24")));
        let when = selected_item(&app).unwrap().when.unwrap();
        assert_eq!((when.from, when.to), (day("2026-12-20"), day("2026-12-24")));
        send(&mut app, Message::ClearWhen);
        assert_eq!(selected_item(&app).unwrap().when, None);
    }

    /// The plan note is typed in the details pane: Enter saves it as one
    /// change, Esc puts the saved note back, moving on saves it.
    #[test]
    fn the_plan_note_saves_on_enter_and_esc_restores_it() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("christmas-2026".into()));
        send(&mut app, Message::Select("movies/die-hard-1988".into()));
        let saved = selected_item(&app).unwrap().note.clone();
        send(&mut app, Message::ItemNote("With the ".into()));
        assert_eq!(selected_item(&app).unwrap().note, saved, "not before Enter");
        send(&mut app, Message::ItemNote("With the family. ".into()));
        send(&mut app, Message::SaveItemNote);
        assert_eq!(selected_item(&app).unwrap().note, "With the family.");
        assert!(app.plans.item_note_draft.is_none());

        send(&mut app, Message::ItemNote("Something else".into()));
        assert!(dismiss_top(&mut app));
        assert!(app.plans.item_note_draft.is_none());
        assert_eq!(selected_item(&app).unwrap().note, "With the family.");

        // Selecting another entry saves what was typed.
        send(&mut app, Message::ItemNote("Alone".into()));
        let _ = app.update(AppMessage::Plans(Message::Select(
            "movies/the-mummy-1999".into(),
        )));
        assert_eq!(
            app.repo
                .library()
                .plan("christmas-2026")
                .unwrap()
                .item_for("movies/die-hard-1988")
                .unwrap()
                .note,
            "Alone"
        );
    }

    #[test]
    fn the_plan_page_shows_progress_and_opens_edit_plan() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("halloween-2026".into()));
        let messages: Vec<_> = {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            assert!(ui.find("2 of 8 done").is_ok());
            assert!(ui.find("Linked to #halloween").is_ok());
            let _ = ui.click("Edit plan");
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
        assert!(
            matches!(&app.plan_dialog, Some(Dialog::Form(f)) if f.editing.as_deref() == Some("halloween-2026"))
        );
    }

    #[test]
    fn deleting_a_plan_asks_and_goes_back_to_the_overview() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("halloween-2025".into()));
        send(&mut app, Message::EditPlan);
        send(&mut app, Message::DeletePlan);
        assert!(matches!(app.plan_dialog, Some(Dialog::Delete { .. })));
        send(&mut app, Message::ConfirmDeletePlan);
        assert!(app.repo.library().plan("halloween-2025").is_none());
        assert_eq!(app.plans.page, Page::Overview);
    }

    #[test]
    fn ticking_a_show_marks_its_seasons_out_done_and_unticking_puts_them_back() {
        let mut app = App::new(crate::screens::Screen::Plans);
        app.today = Date::parse("2026-10-19").unwrap();
        let id = "anime/frieren-beyond-journeys-end-2023";
        let plan = "halloween-2026";
        app.apply(Operation::SetPlanItem {
            plan: plan.into(),
            item: PlanItem {
                entry: id.into(),
                done: false,
                repeat: false,
                note: String::new(),
                when: None,
            },
        });
        let _ = update(&mut app, Message::ShowPlan(plan.into()));
        let before = app.repo.library().entry(id).unwrap().season_list().to_vec();
        assert_eq!(before[1].status, Some(EntryStatus::InProgress));

        let _ = update(&mut app, Message::ToggleDone(id.into()));
        let show = app.repo.library().entry(id).unwrap();
        assert_eq!(show.status, EntryStatus::Done);
        assert!(
            show.season_list()
                .iter()
                .all(|s| s.status == Some(EntryStatus::Done))
        );

        let _ = update(&mut app, Message::ToggleDone(id.into()));
        let show = app.repo.library().entry(id).unwrap();
        assert_eq!(show.season_list(), before.as_slice());
        assert_eq!(show.status, EntryStatus::InProgress);
    }
}
