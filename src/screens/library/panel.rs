//! The details panel at the library's right (decided 2026-10-31):
//! selecting a title never moves the list. Home keeps its own layout,
//! the details taking the place of its right column (`overview`).
//! Elsewhere the panel is pinned (always there, the list at a glance while
//! nothing is selected) or unpinned (the list gets the full width and a
//! selection slides a drawer over the list's right side), remembered per
//! view key like the view. A narrow window unpins it.
//!
//! The glance (2026-11-07) never scrolls: it is built for the panel's
//! height, with Coming soon, what is in progress, the active plan's items
//! left for the type, the most used tags and Pick something, the empty
//! sections left out and the rows fitted to the height (`rows`).

use std::time::{Duration, Instant};

use iced::widget::{Column, Row, button, column, container, opaque, responsive, row, space, text};
use iced::{Background, Border, Center, Color, Element, Fill, Padding, Shadow, Vector};

use super::{Listing, Message, OpenSeries, View, overview, soon};
use crate::animation::{Easing, Slide};
use crate::app::{App, KeyPress, Message as AppMessage};
use crate::browse::{self, ActivePlan, Section};
use crate::history::Direction;
use crate::model::{Date, Entry, EntryStatus, MediaType};
use crate::screens::{entry, shell};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// Under this many px of list beside a pinned panel the window counts as
/// narrow, and the panel is a drawer: at the minimum window (1280 px) a
/// pinned panel would leave the list 696 px.
pub const NARROW: f32 = 720.0;

/// How the details panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Home: the details in its right column (`overview`).
    Home,
    /// Always there, the list at a glance when empty.
    Pinned,
    /// Only with a selection, over the list.
    Drawer,
}

/// Whether the window is too narrow for a pinned panel.
pub fn narrow(app: &App) -> bool {
    shell::content_width(app, entry::details::WIDTH) < NARROW
}

/// How the panel shows now.
pub fn panel(app: &App) -> Panel {
    let state = &app.library;
    if state.is_overview() && state.page.is_none() {
        Panel::Home
    } else if state.pinned() && !narrow(app) {
        Panel::Pinned
    } else {
        Panel::Drawer
    }
}

/// How long the drawer slides in and out at Plasma's normal speed.
pub const OPEN: Duration = Duration::from_millis(540);
pub const CLOSE: Duration = Duration::from_millis(410);

/// What the drawer shows, kept while it slides out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    Entry(String),
    Series(OpenSeries),
    /// A Discover item's preview, by its key.
    Found(String),
}

/// The drawer's slide (2026-11-04): in from the window's right edge when
/// a selection opens it, out to the edge when it closes. Only the drawer
/// moves; nothing in the list resizes.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Motion {
    #[default]
    Still,
    Opening(Slide),
    /// Sliding out, with what the drawer showed.
    Closing(Slide, Shown),
}

impl Motion {
    pub fn running(&self) -> bool {
        !matches!(self, Motion::Still)
    }

    /// Still once the slide is over.
    pub fn settle(&mut self, now: Instant) {
        let done = match self {
            Motion::Still => false,
            Motion::Opening(slide) | Motion::Closing(slide, _) => slide.done(now),
        };
        if done {
            *self = Motion::Still;
        }
    }
}

/// What the drawer shows: a selection's details or a series, while the
/// panel is a drawer.
fn shown(app: &App) -> Option<Shown> {
    if panel(app) != Panel::Drawer {
        return None;
    }
    let state = &app.library;
    match (&state.selected, &state.found) {
        (Some(id), _) => Some(Shown::Entry(id.clone())),
        (None, Some(key)) => Some(Shown::Found(key.clone())),
        (None, None) => state.open_series().cloned().map(Shown::Series),
    }
}

/// Before a message: the panel's mode and what the drawer shows, for
/// `slide_drawer` after it.
pub fn drawer_state(app: &App) -> (Panel, Option<Shown>) {
    (panel(app), shown(app))
}

/// Whether a message is one of the drawer's closing actions: its ✕, a
/// click on the open title or the list's empty space, Esc, or Back (the
/// mouse's button, Alt+Left).
pub fn closes_drawer(message: &AppMessage) -> bool {
    matches!(
        message,
        AppMessage::Library(
            Message::ClosePanel
                | Message::Select(_)
                | Message::SelectFound(_)
                | Message::HideFound(_)
        ) | AppMessage::Key(KeyPress::Escape)
            | AppMessage::History(Direction::Back)
    )
}

/// After a message moved the library from `before`: starts the slide in
/// when a selection opened the drawer, or out when one of the closing
/// actions (`closes`) closed it. Another title while it is open changes
/// its content alone, and pinning, unpinning or a resize never slides:
/// the mode has to be the drawer before and after, and leaving it ends
/// any slide. At Plasma's "Instant" nothing moves.
pub fn slide_drawer(app: &mut App, closes: bool, before: (Panel, Option<Shown>), now: Instant) {
    if app.speed.instant() {
        app.library.motion = Motion::Still;
        return;
    }
    let after = shown(app);
    let motion = match (before, after) {
        ((Panel::Drawer, None), Some(_)) => {
            Motion::Opening(Slide::start(now, app.speed.scale(OPEN), Easing::STANDARD))
        }
        ((Panel::Drawer, Some(was)), None) if closes && panel(app) == Panel::Drawer => {
            Motion::Closing(
                Slide::start(now, app.speed.scale(CLOSE), Easing::STANDARD),
                was,
            )
        }
        // Pinned, or on Home, there is no drawer to slide.
        _ if panel(app) != Panel::Drawer => Motion::Still,
        _ => return,
    };
    app.library.motion = motion;
}

/// What the selection puts in the panel: an entry's details or a series.
pub(super) fn details<'a>(app: &'a App) -> Option<Element<'a, AppMessage>> {
    let state = &app.library;
    let library = app.repo.library();
    if let Some(entry) = state.selected.as_deref().and_then(|id| library.entry(id)) {
        return Some(entry::details::view(app, entry, None));
    }
    if let Some(found) = state
        .found
        .as_deref()
        .and_then(|key| app.discover.item(key))
    {
        return Some(soon::preview(app, found));
    }
    state
        .open_series()
        .and_then(|open| entry::series::view(app, open))
}

/// The pinned panel beside the list: the selection's details, else the
/// list at a glance (the Coming soon page's own on that page), built for
/// the panel's height; the type panel's Coming soon popover over it.
pub fn side<'a>(app: &'a App) -> Element<'a, AppMessage> {
    let content = details(app).unwrap_or_else(|| {
        responsive(move |size| {
            let body = if app.library.soon {
                soon::glance(app, size.height)
            } else {
                glance(app, size.height)
            };
            let pane = entry::details::pane(body, None);
            if app.library.type_pop && !app.library.soon {
                iced::widget::stack![pane, soon::type_popover(app, size.height)].into()
            } else {
                pane
            }
        })
        .into()
    });
    container(content).width(entry::details::WIDTH).into()
}

/// What a closing drawer still shows.
fn show<'a>(app: &'a App, shown: &'a Shown) -> Option<Element<'a, AppMessage>> {
    match shown {
        Shown::Entry(id) => {
            let entry = app.repo.library().entry(id)?;
            Some(entry::details::view(app, entry, None))
        }
        Shown::Series(open) => entry::series::view(app, open),
        Shown::Found(key) => Some(soon::preview(app, app.discover.item(key)?)),
    }
}

/// The drawer over the list while something is selected: the details
/// with a pin and a close button above them, a shadow to the left and no
/// scrim. It takes the clicks over it, never the list under it. While it
/// slides it sits `progress` of the way in from the window's right edge,
/// its shadow fading with it.
pub fn drawer<'a>(app: &'a App) -> Option<Element<'a, AppMessage>> {
    let now = Instant::now();
    let (details, progress) = match &app.library.motion {
        Motion::Still => (details(app)?, 1.0),
        Motion::Opening(slide) => (details(app)?, slide.progress(now)),
        Motion::Closing(slide, shown) => (show(app, shown)?, 1.0 - slide.progress(now)),
    };
    let tokens = app.tokens();
    let mut buttons = Row::new().spacing(4);
    if !narrow(app) {
        buttons = buttons.push(small_button(
            Icon::Panel,
            &tokens,
            msg(Message::TogglePanel),
        ));
    }
    buttons = buttons.push(small_button(Icon::Close, &tokens, msg(Message::ClosePanel)));
    let head = container(buttons)
        .width(Fill)
        .align_x(iced::Right)
        .padding(Padding::new(0.0).top(10).right(10));
    let t = tokens;
    // No outline: the shadow alone sets the drawer off the list.
    let face = container(column![head, details])
        .width(entry::details::WIDTH)
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(t.drawer)),
            border: Border {
                radius: iced::border::Radius::default().top_left(12),
                ..Border::default()
            },
            shadow: Shadow {
                color: Color {
                    a: t.drawer_shadow.a * progress,
                    ..t.drawer_shadow
                },
                offset: Vector::new(-18.0, 0.0),
                blur_radius: 44.0,
            },
            ..container::Style::default()
        });
    let offset = entry::details::WIDTH * (1.0 - progress);
    Some(
        container(ui::slide(opaque(face), offset))
            .width(Fill)
            .height(Fill)
            .align_x(iced::Right)
            .into(),
    )
}

/// A 32 px borderless icon button in the secondary color.
fn small_button<'a>(which: Icon, tokens: &Tokens, on_press: AppMessage) -> Element<'a, AppMessage> {
    button(container(ui::icon(which, 17.0, tokens.sub)).center(Fill))
        .width(32)
        .height(32)
        .padding(0)
        .on_press(on_press)
        .style(ui::row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
}

/// The toolbar's panel button, beside the view switch: filled while
/// pinned. None on Home and in a narrow window, where it cannot change
/// anything.
pub fn toggle<'a>(app: &App) -> Option<Element<'a, AppMessage>> {
    if panel(app) == Panel::Home || narrow(app) {
        return None;
    }
    let tokens = app.tokens();
    let pinned = app.library.pinned();
    let (which, color) = if pinned {
        (Icon::PanelPinned, tokens.text)
    } else {
        (Icon::Panel, tokens.sub)
    };
    let t = tokens;
    Some(
        button(container(ui::icon(which, 17.0, color)).center(Fill))
            .width(38)
            .height(36)
            .padding(0)
            .on_press(msg(Message::TogglePanel))
            .style(move |_, status| button::Style {
                background: if pinned {
                    Some(Background::Color(t.selection))
                } else if matches!(status, button::Status::Hovered) {
                    Some(Background::Color(t.hover))
                } else {
                    None
                },
                border: Border::default().rounded(8).width(1).color(t.line),
                ..button::Style::default()
            })
            .focusable(8.0, &tokens),
    )
}

/// How many rows the glance's sections show at most, and how many tags.
const SOON_MAX: usize = 4;
const NOW_MAX: usize = 3;
const PLAN_MAX: usize = 3;
const TAGS: usize = 6;
/// The glance's measures: the gap between sections, a section's header
/// row and the space under it, a text row and a cover row, the plan's
/// line under its header, the tag chips, and the Pick something card with
/// and without its cover.
const SECTION_GAP: f32 = 26.0;
const HEADER_H: f32 = 24.0;
const HEADER_GAP: f32 = 2.0;
const TEXT_ROW_H: f32 = 36.0;
const COVER_ROW_H: f32 = 50.0;
const PLAN_LINE_H: f32 = 23.0;
const TAGS_HEADER_H: f32 = 18.0;
const TAGS_HEADER_GAP: f32 = 9.0;
const TAG_CHIP_H: f32 = 28.0;
const TAG_GAP: f32 = 6.0;
const PICK_GAP: f32 = 9.0;
const PICK_COVER_CARD: f32 = 86.0;
const PICK_TEXT_CARD: f32 = 56.0;
/// The pane's padding over and under the glance.
const PANE_PAD: f32 = entry::details::PAD + 18.0;

/// How many rows each section of the glance shows for the height it
/// has (2026-11-07), so the panel never scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rows {
    pub soon: usize,
    pub now: usize,
    pub plan: usize,
    pub tags: bool,
    pub pick: bool,
}

/// The rows for `avail` px with rows `row_h` tall: every section with
/// something to show gets one row (`counts` is what Coming soon, the
/// in-progress list and the plan have; `tags_h` and `pick_h` the heights
/// of Most used tags and Pick something, when they have something), and
/// when even that is too tall the sections go from the bottom. Then
/// Coming soon grows to four rows, the in-progress list to three and the
/// plan to three, a row at a time while it fits, and Pick something shows
/// only if it fits after them.
pub fn rows(
    avail: f32,
    row_h: f32,
    counts: [usize; 3],
    tags_h: Option<f32>,
    pick_h: Option<f32>,
) -> Rows {
    let maxes = [SOON_MAX, NOW_MAX, PLAN_MAX];
    let base = [
        HEADER_H + HEADER_GAP,
        HEADER_H + HEADER_GAP,
        HEADER_H + PLAN_LINE_H + HEADER_GAP,
    ];
    // The sections in order, each with its height at one row.
    let mut parts: Vec<(usize, f32)> = (0..3)
        .filter(|&i| counts[i] > 0)
        .map(|i| (i, base[i] + row_h))
        .collect();
    if let Some(tags_h) = tags_h {
        parts.push((3, tags_h));
    }
    let total = |parts: &[(usize, f32)]| {
        parts.iter().map(|(_, h)| h).sum::<f32>()
            + parts.len().saturating_sub(1) as f32 * SECTION_GAP
    };
    while !parts.is_empty() && total(&parts) > avail {
        parts.pop();
    }
    let mut rows = Rows::default();
    let mut used = total(&parts);
    for &(i, _) in &parts {
        match i {
            0 => rows.soon = 1,
            1 => rows.now = 1,
            2 => rows.plan = 1,
            _ => rows.tags = true,
        }
    }
    loop {
        let mut grew = false;
        for i in 0..3 {
            let have = match i {
                0 => &mut rows.soon,
                1 => &mut rows.now,
                _ => &mut rows.plan,
            };
            if *have > 0 && *have < counts[i].min(maxes[i]) && used + row_h <= avail {
                *have += 1;
                used += row_h;
                grew = true;
                break;
            }
        }
        if !grew {
            break;
        }
    }
    if let Some(pick_h) = pick_h {
        let gap = if parts.is_empty() { 0.0 } else { SECTION_GAP };
        rows.pick = used + gap + pick_h <= avail;
    }
    rows
}

/// A title on its way: an entry not out yet, or an announced season of a
/// show.
struct Soon<'a> {
    entry: &'a Entry,
    sub: String,
    when: Date,
    meta: String,
}

/// What the list at a glance has to show, before the fit.
struct Glance<'a> {
    soon: Vec<Soon<'a>>,
    now: Vec<&'a Entry>,
    now_title: &'static str,
    /// The active plan, and its items left to do among the list's titles.
    plan: Option<(ActivePlan<'a>, Vec<&'a Entry>)>,
    tags: Vec<(&'a str, usize)>,
    pick: Option<&'a Entry>,
}

/// "Watching now" for the types shown, "Right now" when they differ.
fn now_title(types: &std::collections::BTreeSet<MediaType>) -> &'static str {
    let verb = |media: MediaType| match media {
        MediaType::Movies | MediaType::Shows | MediaType::Anime => "Watching now",
        MediaType::Books => "Reading now",
        MediaType::Games => "Playing now",
        MediaType::Music => "Listening now",
    };
    let mut verbs = types.iter().map(|&m| verb(m));
    match verbs.next() {
        Some(first) if verbs.all(|v| v == first) => first,
        _ => "Right now",
    }
}

/// "3 movies left", "1 title left".
fn left_label(n: usize, single: Option<MediaType>) -> String {
    let noun = match (single, n == 1) {
        (Some(MediaType::Movies), one) => {
            if one {
                "movie"
            } else {
                "movies"
            }
        }
        (Some(MediaType::Shows), one) => {
            if one {
                "show"
            } else {
                "shows"
            }
        }
        (Some(MediaType::Books), one) => {
            if one {
                "book"
            } else {
                "books"
            }
        }
        (Some(MediaType::Games), one) => {
            if one {
                "game"
            } else {
                "games"
            }
        }
        (Some(MediaType::Music), one) => {
            if one {
                "album"
            } else {
                "albums"
            }
        }
        (Some(MediaType::Anime), _) => "anime",
        (None, one) => {
            if one {
                "title"
            } else {
                "titles"
            }
        }
    };
    format!("{n} {noun} left")
}

fn gather<'a>(app: &'a App, listing: &Listing<'a>) -> Glance<'a> {
    let state = &app.library;
    let today = app.today;
    let shown = &listing.shown;
    let mut soon = Vec::new();
    for &entry in shown {
        if browse::is_coming(entry, today)
            && let Some(released) = entry.first_released()
        {
            let sub = match &entry.creator {
                Some(creator) => format!("{} · {creator}", browse::kind_label(entry)),
                None => browse::kind_label(entry).to_owned(),
            };
            soon.push(Soon {
                entry,
                sub,
                when: released.available_from(),
                meta: overview::short_release(released, today),
            });
        }
        for season in entry.season_list() {
            if let Some(released) = season.released
                && !released.is_released_on(today)
            {
                soon.push(Soon {
                    entry,
                    sub: season.name(),
                    when: released.available_from(),
                    meta: overview::short_release(released, today),
                });
            }
        }
    }
    soon.sort_by_key(|s| s.when);
    let now: Vec<&Entry> = shown
        .iter()
        .copied()
        .filter(|e| e.status == EntryStatus::InProgress)
        .collect();
    let plan = ActivePlan::of(app.repo.library(), today).map(|active| {
        let left: Vec<&Entry> = active
            .plan
            .items
            .iter()
            .filter(|item| !item.done)
            .filter_map(|item| shown.iter().copied().find(|e| e.id == item.entry))
            .collect();
        (active, left)
    });
    let mut tags = std::collections::BTreeMap::<&str, usize>::new();
    for tag in shown.iter().flat_map(|e| &e.tags) {
        *tags.entry(tag).or_default() += 1;
    }
    let mut tags: Vec<_> = tags.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    tags.truncate(TAGS);
    Glance {
        soon,
        now,
        now_title: now_title(&state.types),
        plan: plan.filter(|(_, left)| !left.is_empty()),
        tags,
        pick: pick(listing, state.pick, today),
    }
}

/// The pick from Want for `pick` (moved on by "Another"), among the
/// titles that are out, from the list's own order so it is the same on
/// every frame.
pub fn pick<'a>(listing: &Listing<'a>, pick: u64, today: Date) -> Option<&'a crate::model::Entry> {
    let wants: Vec<_> = listing
        .shown
        .iter()
        .copied()
        .filter(|e| e.status == EntryStatus::Want && e.is_released_on(today))
        .collect();
    if wants.is_empty() {
        return None;
    }
    // A spread, so "Another" does not just step to the neighbor.
    let mixed = pick.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17);
    Some(wants[(mixed % wants.len() as u64) as usize])
}

/// The covers the glance may show, for the lookups: whatever its
/// sections could list, whether or not the height lets them.
pub fn glance_covers(app: &App, listing: &Listing<'_>) -> Vec<String> {
    let glance = gather(app, listing);
    let mut ids: Vec<String> = glance
        .soon
        .iter()
        .take(SOON_MAX)
        .map(|s| s.entry.id.clone())
        .collect();
    ids.extend(glance.now.iter().take(NOW_MAX).map(|e| e.id.clone()));
    if let Some((_, left)) = &glance.plan {
        ids.extend(left.iter().take(PLAN_MAX).map(|e| e.id.clone()));
    }
    ids.extend(glance.pick.map(|e| e.id.clone()));
    ids
}

/// The width of a Most used tags chip: "#horror 12".
fn tag_chip_width(tag: &str, n: usize) -> f32 {
    ui::text_width(&format!("#{tag}"), 13.0, font::REGULAR)
        + 6.0
        + ui::text_width(&n.to_string(), 11.5, font::MONO)
        + 20.0
}

/// Pinned with nothing selected: the list at a glance, for `height` px of
/// panel. Coming soon, what is in progress, the active plan's items left
/// for this type, the most used tags and Pick something, the empty ones
/// left out and the rows fitted to the height (`rows`). Rows follow the
/// view: text rows in the text view, small-cover rows in list and grid.
fn glance<'a>(app: &'a App, height: f32) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let listing = state.listing(app.repo.library(), app.today);
    let glance = gather(app, &listing);
    let covers = state.view_mode() != View::Text;
    let row_h = if covers { COVER_ROW_H } else { TEXT_ROW_H };
    let inner = entry::details::INNER;

    let tag_rows = ui::wrapped_rows(
        &glance
            .tags
            .iter()
            .map(|(tag, n)| tag_chip_width(tag, *n))
            .collect::<Vec<_>>(),
        inner,
        TAG_GAP,
    );
    let tags_h = (tag_rows > 0).then(|| {
        TAGS_HEADER_H
            + TAGS_HEADER_GAP
            + tag_rows as f32 * TAG_CHIP_H
            + (tag_rows - 1) as f32 * TAG_GAP
    });
    let pick_h = glance.pick.map(|_| {
        HEADER_H
            + PICK_GAP
            + if covers {
                PICK_COVER_CARD
            } else {
                PICK_TEXT_CARD
            }
    });
    let plan_left = glance.plan.as_ref().map_or(0, |(_, left)| left.len());
    let fit = rows(
        height - PANE_PAD,
        row_h,
        [glance.soon.len(), glance.now.len(), plan_left],
        tags_h,
        pick_h,
    );

    let t = tokens;
    let see_all = |shown: usize, total: usize, section: Section| {
        let label = if total > shown {
            format!("See all {total}")
        } else {
            "See all".to_owned()
        };
        ui::header_link(label, true, &t, msg(Message::GlanceSeeAll(section)))
    };
    let header = |label: &str, right: Element<'a, AppMessage>| {
        row![ui::section_header(label, &t), space::horizontal(), right]
            .align_y(Center)
            .height(HEADER_H)
    };
    let mut items = Column::new().spacing(SECTION_GAP);

    if fit.soon > 0 {
        let rows = Column::with_children(glance.soon.iter().take(fit.soon).map(|soon| {
            glance_row(
                app,
                soon.entry,
                &soon.sub,
                soon.meta.clone(),
                covers,
                &tokens,
            )
        }));
        items = items.push(
            column![
                header(
                    "Coming soon",
                    see_all(fit.soon, glance.soon.len(), Section::ComingSoon)
                ),
                rows
            ]
            .spacing(HEADER_GAP),
        );
    }
    if fit.now > 0 {
        let rows = Column::with_children(glance.now.iter().take(fit.now).map(|entry| {
            // The season under way, else the creator.
            let sub = entry
                .season_list()
                .iter()
                .rev()
                .find(|s| s.status == Some(EntryStatus::InProgress))
                .map(|s| s.name())
                .or_else(|| entry.creator.clone())
                .unwrap_or_else(|| browse::kind_label(entry).to_owned());
            glance_row(app, entry, &sub, String::new(), covers, &tokens)
        }));
        items = items.push(
            column![
                header(
                    glance.now_title,
                    see_all(fit.now, glance.now.len(), Section::InProgress)
                ),
                rows
            ]
            .spacing(HEADER_GAP),
        );
    }
    if fit.plan > 0
        && let Some((active, left)) = &glance.plan
    {
        let mut line = left_label(left.len(), state.single());
        match active.days_left {
            Some(0) => line.push_str(" · last day"),
            Some(1) => line.push_str(" · 1 day to go"),
            Some(n) => line.push_str(&format!(" · {n} days to go")),
            None => {}
        }
        let open = ui::header_link(
            "Open",
            true,
            &tokens,
            AppMessage::Plans(crate::screens::plans::Message::ShowPlan(
                active.plan.id.clone(),
            )),
        );
        let rows = Column::with_children(left.iter().take(fit.plan).map(|entry| {
            let sub = match entry.years(app.today) {
                Some(years) => format!("{} · {years}", browse::kind_label(entry)),
                None => browse::kind_label(entry).to_owned(),
            };
            glance_row(app, entry, &sub, String::new(), covers, &tokens)
        }));
        items = items.push(
            column![
                header(&active.plan.title(), open),
                container(text(line).size(13).color(tokens.sub))
                    .padding(Padding::new(0.0).top(2).bottom(4)),
                rows
            ]
            .spacing(HEADER_GAP),
        );
    }
    if fit.tags {
        let chips = Row::with_children(glance.tags.iter().map(|&(tag, n)| {
            button(
                container(
                    row![
                        text(format!("#{tag}")).size(13),
                        ui::mono(n.to_string(), 11.5, t.sub)
                    ]
                    .spacing(6)
                    .align_y(Center),
                )
                .height(Fill)
                .align_y(Center),
            )
            .height(TAG_CHIP_H)
            .padding([0, 10])
            .on_press(msg(Message::PickTag(tag.to_owned())))
            .style(move |_, status| button::Style {
                background: Some(Background::Color(
                    if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        t.raised
                    },
                )),
                border: Border::default().rounded(14),
                text_color: t.text,
                ..button::Style::default()
            })
            .focusable(14.0, &tokens)
        }))
        .spacing(TAG_GAP)
        .wrap()
        .vertical_spacing(TAG_GAP);
        items = items.push(
            column![ui::section_header("Most used tags", &tokens), chips].spacing(TAGS_HEADER_GAP),
        );
    }
    if fit.pick
        && let Some(entry) = glance.pick
    {
        let another = ui::header_link("Another", false, &tokens, msg(Message::AnotherPick));
        let sub = match entry.years(app.today) {
            Some(years) => format!("{} · {years}", browse::kind_label(entry)),
            None => browse::kind_label(entry).to_owned(),
        };
        let words = column![
            text(&entry.title).size(14.5).font(font::SEMIBOLD),
            text(sub).size(12.5).color(t.sub),
        ]
        .spacing(3)
        .width(Fill);
        let mut content = Row::new().spacing(12).align_y(Center);
        if covers {
            content = content.push(ui::plain_cover(
                app.artwork.cover(entry),
                theme::cover_color(&entry.id),
                44.0,
                66.0,
                5.0,
            ));
        }
        let card = button(content.push(words))
            .width(Fill)
            .padding(10)
            .on_press(msg(Message::Select(entry.id.clone())))
            .style(move |_, status| button::Style {
                background: Some(Background::Color(
                    if matches!(status, button::Status::Hovered) {
                        theme::mix_white(t.raised, if t.light { 0.0 } else { 0.04 })
                    } else {
                        t.raised
                    },
                )),
                border: Border::default().rounded(10),
                text_color: t.text,
                ..button::Style::default()
            })
            .focusable(10.0, &tokens);
        items = items.push(column![header("Pick something", another), card].spacing(PICK_GAP));
    }
    items.into()
}

/// A row of the glance: a type dot (text view) or a 26 × 39 cover with
/// `sub` under the title (list and grid), `meta` in mono at the right,
/// and a line under it; a click selects the title.
fn glance_row<'a>(
    app: &'a App,
    entry: &'a Entry,
    sub: &str,
    meta: String,
    cover: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let color = tokens.type_color(entry.media);
    let title = text(&entry.title)
        .size(14)
        .font(if cover { font::SEMIBOLD } else { font::MEDIUM })
        .wrapping(text::Wrapping::None);
    let mut content = Row::new().spacing(12).align_y(Center);
    content = if cover {
        content
            .push(ui::plain_cover(
                app.artwork.cover(entry),
                theme::cover_color(&entry.id),
                26.0,
                39.0,
                4.0,
            ))
            .push(
                column![
                    title,
                    text(sub.to_owned())
                        .size(12)
                        .color(tokens.sub)
                        .wrapping(text::Wrapping::None)
                ]
                .spacing(1)
                .width(Fill)
                .clip(true),
            )
    } else {
        content
            .push(ui::dot(color, 8.0, 3.0))
            .push(container(title).width(Fill).clip(true))
    };
    content = content.push(ui::mono(meta, 12.0, tokens.sub));
    let height = if cover { COVER_ROW_H } else { TEXT_ROW_H };
    let line = tokens.line;
    let row_button = button(content.height(Fill))
        .width(Fill)
        .height(height - 1.0)
        .padding([0, 4])
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(ui::row_style(tokens, false, 0.0))
        .focusable(0.0, tokens);
    column![
        row_button,
        container(space())
            .width(Fill)
            .height(1)
            .style(move |_| container::background(line))
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers};

    use super::*;
    use crate::app::KeyPress;
    use crate::browse::{Overview, Section};
    use crate::history::Direction;
    use crate::model::{Date, MediaType};
    use crate::screens::Screen;
    use crate::screens::library::View;

    fn app() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app.window = iced::Size::new(1600.0, 900.0);
        app
    }

    fn movies(app: &mut App) -> Vec<String> {
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Movies))));
        app.library
            .listing(app.repo.library(), app.today)
            .shown
            .iter()
            .map(|e| e.id.clone())
            .collect()
    }

    #[test]
    fn the_panel_is_pinned_per_view_and_home_always_has_it() {
        let mut app = app();
        assert_eq!(panel(&app), Panel::Home);
        // Ctrl+I on Home changes nothing.
        let _ = app.update(AppMessage::KeyPressed(
            Key::Character("i".into()),
            Modifiers::CTRL,
        ));
        assert!(app.library.panels.is_empty());

        movies(&mut app);
        assert_eq!(panel(&app), Panel::Pinned, "pinned by default");
        let _ = app.update(AppMessage::KeyPressed(
            Key::Character("i".into()),
            Modifiers::CTRL,
        ));
        assert_eq!(panel(&app), Panel::Drawer);
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Books))));
        assert_eq!(panel(&app), Panel::Pinned, "remembered per type");
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Movies))));
        assert_eq!(panel(&app), Panel::Drawer);
        let _ = app.update(msg(Message::TogglePanel));
        assert_eq!(panel(&app), Panel::Pinned);
    }

    #[test]
    fn a_narrow_window_unpins_the_panel_and_hides_the_button() {
        let mut app = app();
        movies(&mut app);
        app.window = iced::Size::new(1280.0, 720.0);
        assert_eq!(panel(&app), Panel::Drawer);
        assert!(toggle(&app).is_none());
        let _ = app.update(msg(Message::TogglePanel));
        assert!(app.library.pinned(), "the choice stays for a wider window");
        app.window = iced::Size::new(1600.0, 900.0);
        assert_eq!(panel(&app), Panel::Pinned);
    }

    #[test]
    fn esc_and_back_close_the_panel_before_going_back() {
        let mut app = app();
        let ids = movies(&mut app);
        let _ = app.update(msg(Message::TogglePanel));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.library.selected, None, "Esc closes the drawer");

        let _ = app.update(msg(Message::Select(ids[1].clone())));
        let _ = app.update(AppMessage::History(Direction::Back));
        assert_eq!(app.library.selected, None, "Back closes it first");
        assert_eq!(app.library.single(), Some(MediaType::Movies));
        let _ = app.update(AppMessage::History(Direction::Back));
        assert!(app.library.is_overview(), "then goes back");

        // Home: Esc brings the plan back.
        let id = Overview::of(app.repo.library(), app.today).list(Section::RecentlyAdded)[0]
            .id
            .clone();
        let _ = app.update(msg(Message::Select(id)));
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.library.selected, None);
    }

    #[test]
    fn the_glance_picks_from_want_among_what_is_out_and_another_moves_on() {
        let mut app = app();
        movies(&mut app);
        let listing = app.library.listing(app.repo.library(), app.today);
        let first = pick(&listing, 0, app.today).expect("the fixture has movies to want");
        assert_eq!(first.status, EntryStatus::Want);
        assert!(first.is_released_on(app.today));
        let wants = listing
            .shown
            .iter()
            .filter(|e| e.status == EntryStatus::Want && e.is_released_on(app.today))
            .count();
        if wants > 1 {
            assert!(
                (1..8).any(|n| pick(&listing, n, app.today).map(|e| &e.id) != Some(&first.id)),
                "Another finds a different one"
            );
        }
    }

    /// The glance's rows for a height: one row each, then Coming soon to
    /// four, the in-progress list to three and the plan to three, Pick
    /// something only when it fits, and sections dropped from the bottom
    /// when even one row each is too tall.
    #[test]
    fn the_glance_fits_its_rows_to_the_height() {
        let row = COVER_ROW_H;
        let tags_h = TAGS_HEADER_H + TAGS_HEADER_GAP + TAG_CHIP_H;
        let tags = Some(tags_h);
        let pick = Some(HEADER_H + PICK_GAP + PICK_COVER_CARD);
        // Room for everything: every section at its most.
        let roomy = rows(2000.0, row, [6, 5, 4], tags, pick);
        assert_eq!(
            roomy,
            Rows {
                soon: 4,
                now: 3,
                plan: 3,
                tags: true,
                pick: true
            }
        );
        // Never more rows than there are titles.
        assert_eq!(rows(2000.0, row, [1, 0, 2], None, None).soon, 1);
        assert_eq!(rows(2000.0, row, [1, 0, 2], None, None).now, 0);
        assert_eq!(rows(2000.0, row, [1, 0, 2], None, None).plan, 2);
        // One row each takes this much; the next row goes to Coming soon.
        let one_each = (HEADER_H + HEADER_GAP + row) * 2.0
            + (HEADER_H + PLAN_LINE_H + HEADER_GAP + row)
            + tags_h
            + 3.0 * SECTION_GAP;
        let snug = rows(one_each + row + 1.0, row, [6, 5, 4], tags, pick);
        assert_eq!((snug.soon, snug.now, snug.plan), (2, 1, 1));
        assert!(snug.tags && !snug.pick);
        // Coming soon fills up before the in-progress list grows.
        let more = rows(one_each + 4.0 * row, row, [6, 5, 4], tags, pick);
        assert_eq!((more.soon, more.now, more.plan), (4, 2, 1));
        // Too tight for one row each: the tags go (and the room they free
        // goes to Coming soon), then the plan.
        let tight = rows(one_each - 1.0, row, [6, 5, 4], tags, pick);
        assert!(!tight.tags);
        assert_eq!((tight.soon, tight.now, tight.plan), (2, 1, 1));
        let tighter = rows(
            one_each - tags_h - SECTION_GAP - 1.0,
            row,
            [6, 5, 4],
            tags,
            pick,
        );
        assert_eq!((tighter.soon, tighter.now, tighter.plan), (3, 1, 0));
        // Nothing at all fits nothing.
        assert_eq!(rows(10.0, row, [6, 5, 4], tags, pick), Rows::default());
        // Pick something alone, when there is nothing else.
        let alone = rows(200.0, row, [0, 0, 0], None, pick);
        assert!(alone.pick);
    }

    #[test]
    fn the_glance_names_what_is_under_way_by_type() {
        use std::collections::BTreeSet;
        let types = |list: &[MediaType]| list.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(now_title(&types(&[MediaType::Books])), "Reading now");
        assert_eq!(now_title(&types(&[MediaType::Games])), "Playing now");
        assert_eq!(now_title(&types(&[MediaType::Music])), "Listening now");
        assert_eq!(
            now_title(&types(&[MediaType::Movies, MediaType::Shows])),
            "Watching now"
        );
        assert_eq!(
            now_title(&types(&[MediaType::Movies, MediaType::Books])),
            "Right now"
        );
        assert_eq!(now_title(&types(&[])), "Right now");
        assert_eq!(left_label(3, Some(MediaType::Movies)), "3 movies left");
        assert_eq!(left_label(1, Some(MediaType::Books)), "1 book left");
        assert_eq!(left_label(2, None), "2 titles left");
    }

    /// The glance's sections, in order, follow the list: Coming soon with
    /// announced seasons, what is in progress, the tags; the ones with
    /// nothing are left out.
    #[test]
    fn the_glance_lists_what_the_type_has_and_leaves_empty_sections_out() {
        let mut app = app();
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Shows))));
        // Stranger Things with a second season announced for 2027.
        let season = |n, released: &str, status| crate::model::Season {
            season: n,
            released: crate::model::Released::parse(released),
            status,
            fresh: None,
            manual: None,
        };
        app.apply(crate::repo::Operation::SetSeasons {
            id: "shows/stranger-things-2016".into(),
            seasons: vec![
                season(1, "2016-07-15", Some(EntryStatus::Done)),
                season(2, "2027", None),
            ],
            today: app.today,
        });
        let listing = app.library.listing(app.repo.library(), app.today);
        let glance = gather(&app, &listing);
        assert!(
            glance.soon.iter().any(|s| s.sub.starts_with("Season ")),
            "an announced season counts as coming"
        );
        assert!(glance.soon.windows(2).all(|w| w[0].when <= w[1].when));
        assert!(
            glance
                .now
                .iter()
                .all(|e| e.status == EntryStatus::InProgress)
        );
        assert_eq!(glance.now_title, "Watching now");
        assert!(!glance.tags.is_empty());

        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        assert!(ui.find("COMING SOON").is_ok());
        assert!(ui.find("WATCHING NOW").is_ok());
        assert!(ui.find("MOST USED TAGS").is_ok());
        assert!(ui.find("AT A GLANCE").is_err());
    }

    #[test]
    fn a_click_in_the_drawer_never_reaches_the_list() {
        let mut app = app();
        let ids = movies(&mut app);
        let _ = app.update(msg(Message::SetView(View::List)));
        let _ = app.update(msg(Message::TogglePanel));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        let size = crate::app::WINDOW_SIZE;
        app.window = size;
        let click = |x: f32, y: f32| {
            let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
            ui.point_at(iced::Point::new(x, y));
            let _ = ui.simulate(iced_test::simulator::click());
            ui.into_messages()
                .filter(|m| matches!(m, AppMessage::Library(Message::Select(_))))
                .count()
        };
        // The list rows run under the drawer; a click on the drawer's empty
        // part selects nothing, a click left of it selects a row.
        let row_y = 250.0;
        assert_eq!(click(size.width - 40.0, size.height - 60.0), 0);
        assert_eq!(click(size.width - 200.0, row_y), 0);
        assert_eq!(click(shell::SIDEBAR_WIDTH + 200.0, row_y), 1);
    }

    /// Whether a click at the point asks to close the details.
    fn click_closes(app: &App, x: f32, y: f32) -> bool {
        let mut ui =
            iced_test::Simulator::with_size(crate::app::settings(), app.window, app.view());
        ui.point_at(iced::Point::new(x, y));
        let _ = ui.simulate(iced_test::simulator::click());
        ui.into_messages()
            .any(|m| matches!(m, AppMessage::Library(Message::ClosePanel)))
    }

    #[test]
    fn a_click_on_the_lists_empty_space_closes_the_details() {
        let mut app = app();
        let ids = movies(&mut app);
        let _ = app.update(msg(Message::SetView(View::List)));
        let _ = app.update(msg(Message::TogglePanel));
        let size = app.window;
        // The list's left padding, and a row beside it.
        let empty = shell::SIDEBAR_WIDTH + 6.0;
        let row_x = shell::SIDEBAR_WIDTH + 200.0;
        let row_y = 250.0;

        assert_eq!(panel(&app), Panel::Drawer);
        assert!(!click_closes(&app, empty, row_y), "nothing selected");
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        assert!(click_closes(&app, empty, row_y));
        assert!(!click_closes(&app, row_x, row_y), "a row");
        assert!(!click_closes(&app, size.width - 40.0, size.height - 60.0));

        // The first click away from a popover only closes the popover.
        let _ = app.update(msg(Message::Toggle(super::super::Popup::Sort)));
        assert!(!click_closes(&app, empty, row_y));
        let _ = app.update(msg(Message::ClosePopup));

        // The same pinned, where the panel is beside the list.
        let _ = app.update(msg(Message::TogglePanel));
        assert_eq!(panel(&app), Panel::Pinned);
        assert!(click_closes(&app, empty, row_y));
        assert!(!click_closes(&app, size.width - 40.0, size.height - 60.0));

        let _ = app.update(msg(Message::ClosePanel));
        assert_eq!(app.library.selected, None);
    }

    #[test]
    fn a_click_on_homes_details_never_closes_them() {
        let mut app = app();
        let id = movies(&mut app)[0].clone();
        let _ = app.update(msg(Message::ShowType(None)));
        let _ = app.update(msg(Message::Select(id)));
        assert_eq!(panel(&app), Panel::Home);
        let size = app.window;
        assert!(!click_closes(&app, size.width - 60.0, size.height - 100.0));
        assert!(click_closes(
            &app,
            shell::SIDEBAR_WIDTH + 8.0,
            size.height - 100.0
        ));
    }

    #[test]
    fn the_drawer_slides_in_on_a_selection_and_out_on_its_closing_actions() {
        use std::time::Duration;

        use crate::animation::Speed;

        let mut app = app();
        let ids = movies(&mut app);
        app.speed = Speed::factor(0.5);
        let start = std::time::Instant::now();
        let at = |app: &mut App, message: AppMessage, ms: u64| {
            let _ = app.update_at(message, start + Duration::from_millis(ms));
        };
        // Pinned: a selection never slides.
        at(&mut app, msg(Message::Select(ids[0].clone())), 0);
        assert_eq!(app.library.motion, Motion::Still);
        // Unpinning shows the drawer without a slide.
        at(&mut app, msg(Message::TogglePanel), 0);
        assert_eq!(panel(&app), Panel::Drawer);
        assert_eq!(app.library.motion, Motion::Still);
        // The open title closes it: out, at half of 410 ms.
        at(&mut app, msg(Message::Select(ids[0].clone())), 10);
        let Motion::Closing(slide, Shown::Entry(id)) = app.library.motion.clone() else {
            panic!("closing, got {:?}", app.library.motion);
        };
        assert_eq!(id, ids[0]);
        assert!(slide.done(start + Duration::from_millis(215)));
        assert!(!slide.done(start + Duration::from_millis(214)));
        // A frame after it is over settles it.
        at(&mut app, AppMessage::Frame(start), 250);
        assert_eq!(app.library.motion, Motion::Still);

        // A selection opens it: in, at half of 540 ms.
        at(&mut app, msg(Message::Select(ids[1].clone())), 200);
        let Motion::Opening(slide) = app.library.motion else {
            panic!("opening, got {:?}", app.library.motion);
        };
        assert!(slide.done(start + Duration::from_millis(470)));
        assert!(!slide.done(start + Duration::from_millis(469)));
        at(&mut app, AppMessage::Frame(start), 500);
        assert_eq!(app.library.motion, Motion::Still);
        // Another title while it is open: the content changes, no slide.
        at(&mut app, msg(Message::Select(ids[2].clone())), 600);
        assert_eq!(app.library.motion, Motion::Still);
        assert_eq!(app.library.selected.as_deref(), Some(ids[2].as_str()));
        // Esc, the ✕ and a click on the list's empty space close it; Back too.
        at(&mut app, AppMessage::Key(KeyPress::Escape), 600);
        assert!(matches!(app.library.motion, Motion::Closing(..)));
        at(&mut app, msg(Message::Select(ids[2].clone())), 1000);
        at(&mut app, msg(Message::ClosePanel), 2000);
        assert!(matches!(app.library.motion, Motion::Closing(..)));
        at(&mut app, msg(Message::Select(ids[2].clone())), 3000);
        at(&mut app, AppMessage::History(Direction::Back), 4000);
        assert!(matches!(app.library.motion, Motion::Closing(..)));
        assert_eq!(app.library.selected, None);

        // Pinning with the drawer open: no slide, the panel is just there.
        at(&mut app, msg(Message::Select(ids[2].clone())), 5000);
        at(&mut app, msg(Message::TogglePanel), 5000);
        assert_eq!(panel(&app), Panel::Pinned);
        assert_eq!(app.library.motion, Motion::Still);
        // A type change with the drawer open is not a closing action.
        at(&mut app, msg(Message::TogglePanel), 6000);
        at(&mut app, msg(Message::Select(ids[2].clone())), 6000);
        at(
            &mut app,
            msg(Message::ShowType(Some(MediaType::Books))),
            7000,
        );
        assert_eq!(app.library.motion, Motion::Still);

        // Plasma's "Instant": nothing moves.
        app.speed = Speed::INSTANT;
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Movies))));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        assert_eq!(app.library.motion, Motion::Still);
    }

    #[test]
    fn a_sliding_drawer_starts_off_the_window_and_the_list_stays_put() {
        use std::time::Duration;

        use crate::animation::{Easing, Slide};

        let mut app = app();
        let ids = movies(&mut app);
        let _ = app.update(msg(Message::SetView(View::List)));
        let _ = app.update(msg(Message::TogglePanel));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        let size = crate::app::WINDOW_SIZE;
        app.window = size;
        let place = |app: &App, label: &str| {
            let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
            ui.find(label).ok().map(|target| target.bounds())
        };
        let still = place(&app, "Add").unwrap();
        let shown = place(&app, "RATING").expect("the details' Rating section");
        assert!(shown.x + shown.width <= size.width);
        // A slide not started yet (set in the future) sits at the edge.
        app.library.motion = Motion::Opening(Slide::start(
            std::time::Instant::now() + Duration::from_secs(60),
            Duration::from_millis(200),
            Easing::STANDARD,
        ));
        // Only its drawing moves (`ui::slide`), so its layout stays put and
        // nothing in it reports a new place on every frame.
        let start = place(&app, "RATING").expect("still in the tree");
        assert_eq!(start, shown, "the layout never moves");
        assert_eq!(place(&app, "Add"), Some(still), "the toolbar never moves");
    }

    #[test]
    fn the_toolbar_never_moves_with_the_panel() {
        let mut app = app();
        let ids = movies(&mut app);
        app.window = crate::app::WINDOW_SIZE;
        let place = |app: &App, label: &str| {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            ui.find(label).expect(label).bounds()
        };
        let pinned = (place(&app, "Add"), place(&app, "Movies"));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        assert_eq!((place(&app, "Add"), place(&app, "Movies")), pinned);
        let _ = app.update(msg(Message::TogglePanel));
        assert_eq!(
            (place(&app, "Add"), place(&app, "Movies")),
            pinned,
            "drawer open"
        );
        let _ = app.update(msg(Message::ClosePanel));
        assert_eq!(
            (place(&app, "Add"), place(&app, "Movies")),
            pinned,
            "unpinned"
        );
        // The pinned panel starts under the toolbar and the chips.
        let _ = app.update(msg(Message::TogglePanel));
        let glance = place(&app, "MOST USED TAGS");
        assert!(glance.y > place(&app, "All").y + 20.0);
    }
}
