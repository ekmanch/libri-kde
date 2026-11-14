//! Plans: the overview ("All plans"), one plan as a checklist or a
//! timeline, its All tab (everything with the plan's tags), the details
//! panel beside them and the New and Edit plan dialog.

mod form;
mod glance;
mod overview;
mod plan;

pub use overview::plan_accent;

use std::collections::HashMap;
use std::time::Instant;

use iced::mouse::ScrollDelta;
use iced::widget::{operation, scrollable};
use iced::{Element, Task};

use super::library::soon::WheelGesture;
use super::library::{self, Motion, Shown, SliceSlide};
use crate::animation::{Easing, Slide, Spring};
use crate::app::{App, KeyPress, Message as AppMessage};
use crate::artwork::Wants;
use crate::history::Direction;
use crate::model::{Date, EntryStatus, MediaType, Plan, PlanItem, Season, TagCategory};
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
    /// The plan's All tab: everything with its tags.
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

/// A "See all" popover over the All tab's panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlancePop {
    /// What's left: every item not done.
    Left,
    /// By type: every type's progress.
    Types,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub page: Page,
    /// The details panel unpinned: one choice for every plan and both
    /// tabs, kept in the preferences. Pinned until changed.
    pub unpinned: bool,
    /// The drawer's slide, as the library's.
    pub motion: Motion,
    /// The All tab's panel: its open popover, and which title Pick
    /// something shows ("Another" moves it on).
    pub glance_pop: Option<GlancePop>,
    pub pick: u64,
    /// The list's slide between This plan and All, as Music's between
    /// its tabs; where the list is scrolled, and where each tab of the
    /// open plan was left (This plan, All).
    pub tab_slide: Option<SliceSlide>,
    pub scroll: f32,
    pub tab_scroll: [f32; 2],
    /// The side-scroll or Shift+wheel gesture towards the other tab, and
    /// the way a gesture or Shift+arrow asked the next switch to slide.
    pub wheel: WheelGesture,
    pub tab_step: Option<isize>,
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

    /// Whether a plan is open with the panel unpinned, so a selection
    /// shows in a drawer.
    pub fn drawer_mode(&self) -> bool {
        self.unpinned && self.plan_id().is_some()
    }

    fn close_popovers(&mut self) {
        self.popup = None;
        self.glance_pop = None;
        self.when_open = false;
        self.anchor = None;
    }

    /// Opens `page`. The other tab of the same plan keeps each tab's
    /// place in its list; another plan starts at the top.
    fn turn_to(&mut self, page: Page) {
        match (tab_of(&self.page), tab_of(&page)) {
            (Some((was, from)), Some((id, to))) if was == id => {
                if from != to {
                    self.tab_scroll[from] = self.scroll;
                    self.scroll = self.tab_scroll[to];
                }
            }
            _ => {
                self.scroll = 0.0;
                self.tab_scroll = [0.0; 2];
            }
        }
        self.page = page;
    }

    /// Returns to `page` (Back or Forward): the selection and the
    /// popovers close.
    pub fn go_to(&mut self, page: Page) {
        self.turn_to(page);
        self.selected = None;
        self.close_popovers();
    }
}

/// The plan page's scrolling list, for the scroll back to a tab's place.
pub const LIST_ID: &str = "plan-list";

/// A plan page's plan and its tab's place: This plan 0, All 1.
fn tab_of(page: &Page) -> Option<(&str, usize)> {
    match page {
        Page::Overview => None,
        Page::Plan(id) => Some((id, 0)),
        Page::Collection(id) => Some((id, 1)),
    }
}

/// The switch to the other tab, when This plan | All is on screen with
/// nothing open over it: what a side-scroll, Shift+wheel or Shift+arrow
/// does there, as on Music's tabs.
pub fn other_tab(app: &App) -> Option<Message> {
    let state = &app.plans;
    let free = app.screen == super::Screen::Plans
        && app.dialog.is_none()
        && app.plan_dialog.is_none()
        && !app.token_warning
        && state.popup.is_none()
        && !state.when_open
        && state.glance_pop.is_none()
        && app.library.details_pop.is_none()
        && !app.library.details_rating_open;
    let plan = current_plan(app).filter(|plan| free && !plan.tags.is_empty())?;
    Some(match state.page {
        Page::Collection(_) => Message::ShowPlan(plan.id.clone()),
        _ => Message::ShowCollection(plan.id.clone()),
    })
}

/// After a message, with the page before it (None from another screen):
/// a switch between a plan's tabs slides the list to the other one, as
/// Music's tabs do, toward All to the left and back to the right unless a
/// gesture said which way (`tab_step`); any new page is scrolled to where
/// it was left.
pub fn page_turned(app: &mut App, before: Option<Page>, now: Instant) -> Task<AppMessage> {
    let step = app.plans.tab_step.take();
    if app.screen != super::Screen::Plans {
        app.plans.tab_slide = None;
        return Task::none();
    }
    if before.as_ref() == Some(&app.plans.page) {
        return Task::none();
    }
    let tabs = match (before.as_ref().and_then(tab_of), tab_of(&app.plans.page)) {
        (Some((was, from)), Some((id, to))) if was == id => Some((from, to)),
        _ => None,
    };
    app.plans.tab_slide = match (tabs, Spring::desktop_switch(app.speed)) {
        (Some((from, to)), Some(spring)) => {
            let step = step.unwrap_or(if to > from { 1 } else { -1 });
            Some(SliceSlide::step(
                app.plans.tab_slide.as_ref(),
                from,
                step,
                spring.slowed(library::soon::SLOWER),
                (list_width(app) + library::soon::GAP) * app.scale,
                now,
            ))
        }
        _ => None,
    };
    let offset = scrollable::AbsoluteOffset {
        x: None,
        y: Some(app.plans.scroll),
    };
    operation::scroll_to(LIST_ID, offset)
}

/// How wide the plan page's list is: the content, less the pinned panel.
pub fn list_width(app: &App) -> f32 {
    let side = if app.plans.unpinned {
        0.0
    } else {
        super::entry::details::WIDTH
    };
    super::shell::content_width(app, side)
}

/// What the Plans screen draws from the lookups now: the details' entry,
/// the five covers on each plan card, the All tab's Pick something, and
/// with covers in its view a plan's rows and "Add to this plan", or every
/// row of its All tab and of What's left.
pub fn wanted(app: &App, wants: &mut Wants) {
    let library = app.repo.library();
    if let Some(id) = &app.plans.selected {
        super::details_wants(app, id, wants);
    }
    match &app.plans.page {
        Page::Overview => {
            for plan in &library.plans {
                if shown_season(app, plan).is_none() {
                    wants
                        .covers
                        .extend(plan.items.iter().take(5).map(|i| i.entry.clone()));
                }
            }
        }
        Page::Plan(id) | Page::Collection(id) => {
            let Some(plan) = library.plan(id) else {
                return;
            };
            let on_collection = matches!(app.plans.page, Page::Collection(_));
            if on_collection && let Some(pick) = glance::pick(app, plan) {
                wants.covers.push(pick.id.clone());
            }
            if !app.plans.style(id, app.tokens().light).covers {
                return;
            }
            if on_collection {
                // Everything with the plan's tags, in the plan or not,
                // and the items left, tagged or not.
                wants
                    .covers
                    .extend(rules::left(plan).map(|item| item.entry.clone()));
                wants.covers.extend(
                    plan::collection_sections(app, plan)
                        .iter()
                        .flat_map(|(_, entries)| entries)
                        .map(|e| e.id.clone()),
                );
            } else {
                wants
                    .covers
                    .extend(plan.items.iter().map(|i| i.entry.clone()));
                wants
                    .covers
                    .extend(plan::pool_entries(app, plan).iter().map(|e| e.id.clone()));
            }
        }
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
    /// The plan page's list was scrolled to this offset.
    Scrolled(f32),
    /// The wheel anywhere on a plan page; `App::update_at` turns a
    /// side-scroll or Shift+wheel into the other tab.
    Wheel(ScrollDelta),
    Select(String),
    /// The header's panel button, the drawer's pin and Ctrl+I.
    TogglePanel,
    /// The drawer's ✕.
    ClosePanel,
    /// "See all N ›" in the All tab's panel.
    ToggleGlancePop(GlancePop),
    CloseGlancePop,
    /// "Another" in Pick something.
    AnotherPick,
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
            state.turn_to(Page::Overview);
            state.selected = None;
            state.close_popovers();
        }
        Message::ShowPlan(id) => {
            state.turn_to(Page::Plan(id));
            state.selected = None;
            state.close_popovers();
        }
        Message::ShowCollection(id) => {
            state.turn_to(Page::Collection(id));
            state.selected = None;
            state.close_popovers();
        }
        Message::Scrolled(offset) => state.scroll = offset,
        Message::Wheel(_) => {}
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
            // Only an item of the plan has a tick: Ctrl+click on a title
            // the All tab lists from outside it does nothing.
            if let Some(plan) = current_plan(app)
                && let Some(item) = plan.item_for(&entry)
            {
                let done = !item.done;
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
        Message::TogglePanel => {
            state.unpinned = !state.unpinned;
            state.glance_pop = None;
        }
        Message::ClosePanel => {
            state.selected = None;
            state.close_popovers();
            app.library.details_rating_open = false;
        }
        Message::ToggleGlancePop(pop) => {
            state.glance_pop = if state.glance_pop == Some(pop) {
                None
            } else {
                Some(pop)
            };
        }
        Message::CloseGlancePop => state.glance_pop = None,
        Message::AnotherPick => state.pick = state.pick.wrapping_add(1),
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
                    app.plans.turn_to(Page::Overview);
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
        Message::OpenTags => form.tag_view = Some(PickerState::opening([TagCategory::Season])),
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
    app.plans.turn_to(Page::Plan(id));
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
            } else if state.glance_pop.is_some() {
                state.glance_pop = None;
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

/// The window title's part: "All plans", or "Halloween 2026" on both of
/// a plan's tabs.
pub fn title(app: &App) -> String {
    match current_plan(app) {
        Some(plan) => plan.title(),
        None => "All plans".into(),
    }
}

/// Before a message: the plan page and the drawer's title while the panel
/// is a drawer, for `slide_drawer` after it. None with the panel pinned,
/// on the overview and on another screen.
pub fn drawer_state(app: &App) -> Option<(Page, Option<String>)> {
    (app.screen == super::Screen::Plans && app.plans.drawer_mode())
        .then(|| (app.plans.page.clone(), app.plans.selected.clone()))
}

/// Whether a message is one of the drawer's closing actions: its ✕, a
/// click on the open title, Esc, or Back (the mouse's button, Alt+Left).
pub fn closes_drawer(message: &AppMessage) -> bool {
    matches!(
        message,
        AppMessage::Plans(Message::ClosePanel | Message::Select(_))
            | AppMessage::Key(KeyPress::Escape)
            | AppMessage::History(Direction::Back)
    )
}

/// After a message moved the plan page from `before`: starts the drawer's
/// slide in when a selection opened it, or out when one of the closing
/// actions (`closes`) closed it, with the library's timing. Another title
/// while it is open changes its content alone; pinning, unpinning and
/// another tab or plan never slide.
pub fn slide_drawer(
    app: &mut App,
    closes: bool,
    before: Option<(Page, Option<String>)>,
    now: Instant,
) {
    if app.speed.instant() {
        app.plans.motion = Motion::Still;
        return;
    }
    let start = |time| Slide::start(now, app.speed.scale(time), Easing::STANDARD);
    let motion = match (before, drawer_state(app)) {
        (Some((was, None)), Some((page, Some(_)))) if was == page => {
            Motion::Opening(start(library::OPEN))
        }
        (Some((was, Some(id))), Some((page, None))) if closes && was == page => {
            Motion::Closing(start(library::CLOSE), Shown::Entry(id))
        }
        (Some((was, _)), Some((page, _))) if was == page => return,
        _ => Motion::Still,
    };
    app.plans.motion = motion;
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

/// An arrow key on the plan page: moves the selection through the rows
/// in the order shown, the All tab's (by type), the checklist's (by type,
/// then the plan's order) or the timeline's.
pub fn step_selection(app: &mut App, step: isize) {
    let Some(plan) = current_plan(app) else {
        return;
    };
    let library = app.repo.library();
    let order: Vec<String> = if matches!(app.plans.page, Page::Collection(_)) {
        plan::collection_sections(app, plan)
            .iter()
            .flat_map(|(_, entries)| entries)
            .map(|e| e.id.clone())
            .collect()
    } else if plan.timeline {
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
    use iced::keyboard::Modifiers;

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
    fn the_collection_with_covers_wants_every_row_not_only_the_plan() {
        let mut app = app();
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        send(&mut app, Message::SetCovers(true));
        let library = app.repo.library();
        let plan = library.plan("halloween-2026").unwrap();
        let all = rules::collection(plan, library);
        assert!(all.len() > plan.items.len(), "the fixture tags more");
        let mut wants = Wants::default();
        wanted(&app, &mut wants);
        for entry in all {
            assert!(wants.covers.contains(&entry.id), "{}", entry.id);
        }
    }

    #[test]
    fn a_pool_search_wants_the_covers_it_finds() {
        let mut app = app();
        send(&mut app, Message::ShowPlan("halloween-2026".into()));
        send(&mut app, Message::SetCovers(true));
        let library = app.repo.library();
        let plan = library.plan("halloween-2026").unwrap();
        let outside = library
            .entries
            .iter()
            .find(|e| !e.tags.iter().any(|t| plan.tags.contains(t)))
            .unwrap();
        let (id, title) = (outside.id.clone(), outside.title.clone());
        app.plans.pool_query = title;
        let mut wants = Wants::default();
        wanted(&app, &mut wants);
        assert!(wants.covers.contains(&id));
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
            assert!(ui.find("Linked to #halloween · 16 tagged").is_ok());
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

    fn see(app: &App, there: &[&str], gone: &[&str]) {
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        for text in there {
            assert!(ui.find(*text).is_ok(), "{text} is missing");
        }
        for text in gone {
            assert!(ui.find(*text).is_err(), "{text} shows");
        }
    }

    /// 2026-11-14: nothing in the header differs between the tabs, so
    /// nothing moves; the panel under it changes its contents.
    #[test]
    fn the_header_is_the_same_on_this_plan_and_on_all() {
        let mut app = app();
        let tagged = {
            let library = app.repo.library();
            rules::collection(library.plan("halloween-2026").unwrap(), library).len()
        };
        let linked = format!("Linked to #halloween · {tagged} tagged");
        let header = ["Halloween", "2 of 8 done", linked.as_str(), "Edit plan"];
        send(&mut app, Message::ShowPlan("halloween-2026".into()));
        see(&app, &header, &["WHAT'S LEFT"]);
        see(&app, &["Add to this plan"], &[]);
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        see(&app, &header, &["All Halloween", "Add to this plan"]);
        see(&app, &["WHAT'S LEFT", "BY TYPE", "See all 6"], &[]);
        assert_eq!(title(&app), "Halloween 2026");
    }

    /// The pin is one choice for the plan screen: every plan, both tabs,
    /// kept in the preferences. Unpinned, the panel is only the drawer of
    /// an open title.
    #[test]
    fn the_panel_pin_is_one_setting_for_the_plan_screen() {
        use iced::keyboard::{Key, Modifiers};

        let mut app = app();
        send(&mut app, Message::ShowPlan("halloween-2026".into()));
        let _ = app.update(AppMessage::KeyPressed(
            Key::Character("i".into()),
            Modifiers::CTRL,
        ));
        assert!(app.plans.unpinned, "Ctrl+I unpins");
        see(&app, &["2 of 8 done"], &["Add to this plan"]);
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        assert!(app.plans.unpinned);
        see(&app, &["2 of 8 done"], &["WHAT'S LEFT"]);
        send(&mut app, Message::ShowPlan("christmas-2026".into()));
        assert!(app.plans.unpinned);

        let saved = crate::preferences::Preferences::of(&app);
        assert!(!saved.plan_panel);
        let mut again = App::new(Screen::Plans);
        assert!(!again.plans.unpinned, "pinned until changed");
        saved.apply_to(&mut again);
        assert!(again.plans.unpinned);
    }

    /// A row of the All tab opens its details, in the plan or not; the
    /// same row again, Esc, Back and the drawer's ✕ close them. The +
    /// only adds.
    #[test]
    fn a_row_on_all_opens_its_details_and_the_closing_actions_close_them() {
        let mut app = app();
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        let outside = {
            let library = app.repo.library();
            let plan = library.plan("halloween-2026").unwrap();
            rules::collection(plan, library)
                .into_iter()
                .find(|e| plan.item_for(&e.id).is_none())
                .unwrap()
                .id
                .clone()
        };
        let select = |app: &mut App| {
            let _ = app.update(AppMessage::Plans(Message::Select(outside.clone())));
        };
        select(&mut app);
        assert_eq!(app.plans.selected.as_deref(), Some(outside.as_str()));
        see(&app, &["Not in this plan"], &["WHAT'S LEFT"]);
        select(&mut app);
        assert_eq!(app.plans.selected, None, "the open row closes it");

        select(&mut app);
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.plans.selected, None, "Esc");
        select(&mut app);
        let _ = app.update(AppMessage::History(Direction::Back));
        assert_eq!(app.plans.selected, None, "Back");
        assert_eq!(app.plans.page, Page::Collection("halloween-2026".into()));
        select(&mut app);
        send(&mut app, Message::ClosePanel);
        assert_eq!(app.plans.selected, None, "the ✕");

        // Adding from the row leaves the details closed, and Ctrl+click
        // on a title outside the plan ticks nothing.
        let _ = app.update(AppMessage::Modifiers(Modifiers::CTRL));
        select(&mut app);
        let _ = app.update(AppMessage::Modifiers(Modifiers::empty()));
        let item = |app: &App| {
            app.repo
                .library()
                .plan("halloween-2026")
                .unwrap()
                .item_for(&outside)
                .cloned()
        };
        assert!(item(&app).is_none() && app.plans.selected.is_none());
        send(&mut app, Message::AddItem(outside.clone()));
        assert!(item(&app).is_some() && app.plans.selected.is_none());
    }

    #[test]
    fn the_drawer_slides_as_the_librarys_and_only_while_unpinned() {
        use std::time::Duration;

        let mut app = app();
        app.speed = crate::animation::Speed::factor(1.0);
        let start = Instant::now();
        let at = |app: &mut App, message: Message, ms: u64| {
            let _ = app.update_at(
                AppMessage::Plans(message),
                start + Duration::from_millis(ms),
            );
        };
        let id = "movies/the-thing-1982";
        at(&mut app, Message::ShowPlan("halloween-2026".into()), 0);
        at(&mut app, Message::Select(id.into()), 0);
        assert_eq!(app.plans.motion, Motion::Still, "pinned");
        at(&mut app, Message::TogglePanel, 0);
        assert_eq!(app.plans.motion, Motion::Still, "unpinning never slides");
        at(&mut app, Message::ClosePanel, 10);
        assert!(
            matches!(&app.plans.motion, Motion::Closing(_, Shown::Entry(was)) if was == id),
            "{:?}",
            app.plans.motion
        );
        at(&mut app, Message::Select(id.into()), 2000);
        assert!(matches!(app.plans.motion, Motion::Opening(_)));
        // Another tab closes it at once.
        at(
            &mut app,
            Message::ShowCollection("halloween-2026".into()),
            2010,
        );
        assert_eq!(app.plans.motion, Motion::Still);
        assert_eq!(app.plans.selected, None);
    }

    /// "See all N ›" opens the whole list over the panel; Esc or a row
    /// closes it, and the row opens its details.
    #[test]
    fn see_all_opens_a_popover_whose_rows_open_details() {
        let mut app = app();
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        let messages: Vec<_> = {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            let _ = ui.click("See all 6");
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.plans.glance_pop, Some(GlancePop::Left));
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.plans.glance_pop, None);
        assert_eq!(app.plans.page, Page::Collection("halloween-2026".into()));

        send(&mut app, Message::ToggleGlancePop(GlancePop::Left));
        let left = {
            let plan = app.repo.library().plan("halloween-2026").unwrap();
            rules::left(plan).last().unwrap().entry.clone()
        };
        send(&mut app, Message::Select(left.clone()));
        assert_eq!(app.plans.glance_pop, None);
        assert_eq!(app.plans.selected, Some(left));
    }

    /// Pick something offers a tagged Want title that is not in the
    /// plan; a finished plan has nothing left to list.
    #[test]
    fn pick_something_is_a_tagged_want_title_outside_the_plan() {
        let mut app = app();
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        for _ in 0..4 {
            let library = app.repo.library();
            let plan = library.plan("halloween-2026").unwrap();
            if let Some(pick) = glance::pick(&app, plan) {
                assert_eq!(pick.status, EntryStatus::Want);
                assert!(plan.item_for(&pick.id).is_none());
                assert!(pick.tags.iter().any(|t| plan.tags.contains(t)));
            }
            send(&mut app, Message::AnotherPick);
        }
        let left: Vec<String> = {
            let plan = app.repo.library().plan("halloween-2026").unwrap();
            rules::left(plan).map(|i| i.entry.clone()).collect()
        };
        for entry in left {
            send(&mut app, Message::ToggleDone(entry));
        }
        see(&app, &["8 of 8 done", "BY TYPE"], &["WHAT'S LEFT"]);
    }

    /// This plan | All switch as Music's tabs do: a side-scroll,
    /// Shift+wheel or Shift+arrows, the list sliding to the other tab and
    /// each tab keeping its place.
    #[test]
    fn the_tabs_switch_by_side_scroll_and_shift_arrows_and_slide() {
        use std::time::Duration;

        use iced::keyboard::{Key, key::Named};

        let mut app = app();
        let plan = || Page::Plan("halloween-2026".into());
        let all = || Page::Collection("halloween-2026".into());
        let start = Instant::now();
        let wheel = |app: &mut App, x: f32, y: f32, ms: u64| {
            let delta = ScrollDelta::Lines { x, y };
            let _ = app.update_at(
                AppMessage::Plans(Message::Wheel(delta)),
                start + Duration::from_millis(ms),
            );
        };
        let arrow = |app: &mut App, named: Named, ms: u64| {
            let _ = app.update_at(
                AppMessage::KeyPressed(Key::Named(named), Modifiers::SHIFT),
                start + Duration::from_millis(ms),
            );
        };
        send(&mut app, Message::ShowPlan("halloween-2026".into()));
        wheel(&mut app, 0.0, -1.0, 0);
        assert_eq!(app.plans.page, plan(), "plain scrolling");
        wheel(&mut app, -1.0, 0.0, 1000);
        assert_eq!(app.plans.page, all(), "a side-scroll");
        wheel(&mut app, -1.0, 0.0, 1050);
        assert_eq!(app.plans.page, all(), "one gesture");
        let _ = app.update(AppMessage::Modifiers(Modifiers::SHIFT));
        wheel(&mut app, 0.0, 1.0, 2000);
        assert_eq!(app.plans.page, plan(), "Shift+wheel");
        let _ = app.update(AppMessage::Modifiers(Modifiers::empty()));
        arrow(&mut app, Named::ArrowRight, 3000);
        assert_eq!(app.plans.page, all(), "Shift+Right");
        assert_eq!(app.plans.tab_slide, None, "instant in the tests");

        // Each tab keeps its place in its list.
        send(&mut app, Message::Scrolled(240.0));
        arrow(&mut app, Named::ArrowLeft, 4000);
        assert_eq!((app.plans.page.clone(), app.plans.scroll), (plan(), 0.0));
        send(&mut app, Message::Scrolled(80.0));
        send(&mut app, Message::ShowCollection("halloween-2026".into()));
        assert_eq!(app.plans.scroll, 240.0);
        assert_eq!(app.plans.tab_scroll, [80.0, 240.0]);

        // With animations the list slides: a click toward All to the
        // left and back to the right, a gesture the way it points.
        app.speed = crate::animation::Speed::factor(1.0);
        let at = |app: &mut App, message: Message, ms: u64| {
            let _ = app.update_at(
                AppMessage::Plans(message),
                start + Duration::from_millis(ms),
            );
        };
        at(&mut app, Message::ShowPlan("halloween-2026".into()), 10_000);
        assert!(!app.plans.tab_slide.as_ref().expect("a slide").forward());
        at(&mut app, Message::Scrolled(80.0), 30_000);
        assert_eq!(app.plans.tab_slide, None, "settled");
        at(
            &mut app,
            Message::ShowCollection("halloween-2026".into()),
            30_000,
        );
        assert!(app.plans.tab_slide.as_ref().unwrap().forward());
        wheel(&mut app, 1.0, 0.0, 60_000);
        assert_eq!(app.plans.page, plan());
        assert!(!app.plans.tab_slide.as_ref().unwrap().forward());
        wheel(&mut app, 1.0, 0.0, 90_000);
        assert_eq!(app.plans.page, all(), "round again, the same way");
        assert!(!app.plans.tab_slide.as_ref().unwrap().forward());

        // Another plan starts at the top without a slide; a plan without
        // tags has one tab, and a dialog takes the keys.
        at(
            &mut app,
            Message::ShowPlan("christmas-2026".into()),
            120_000,
        );
        assert_eq!(app.plans.tab_slide, None);
        assert_eq!((app.plans.scroll, app.plans.tab_scroll), (0.0, [0.0; 2]));
        send(&mut app, Message::EditPlan);
        assert!(other_tab(&app).is_none());
        send(&mut app, Message::Close);
        assert!(other_tab(&app).is_some());
        send(&mut app, Message::TogglePopup(Popup::View));
        assert!(other_tab(&app).is_none());
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
