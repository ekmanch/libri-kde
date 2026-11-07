//! The Coming soon page (decided 2026-11-07, canvas "Coming soon"): the
//! one upcoming list (`crate::upcoming`) in full, in three slices.
//! Library is what the owner has on the way; Discover what the daily
//! check found for things the owner has (`crate::discover`); All both,
//! library rows marked Want and Discover rows with "+ Add". A segmented
//! control beside the title switches them (Shift+← and Shift+→, and a
//! side-scroll or Shift+wheel anywhere on the page, too), and the list slides like a Plasma
//! desktop switch while the header, the chips, the rail and the panel
//! stay. Type chips narrow a slice, remembered per slice. The rows are
//! grouped by month, then year, then "Date not set", in the text, cover
//! and grid views, remembered for the page. The text and cover views have
//! pinned month headers; the grid is one continuous block, where the date
//! under each tile and the section rail are the only date cues. All three
//! have the section rail on the right.
//!
//! The panel is the type screens' (pinned by default, key "soon"): with
//! nothing selected a glance of Next up, New in Discover, By type and
//! Hidden; a library row selected shows the entry's details; a Discover
//! row a preview with Add to library and Not interested. The type panel's
//! Coming soon › See all opens a popover over the panel with that type's
//! upcoming titles, and "Open Coming soon ›" at its bottom opens the page.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use iced::mouse::ScrollDelta;
use iced::widget::{Column, Row, Stack, button, column, container, grid, row, space, stack, text};
use iced::{Background, Border, Center, Color, Element, Fill, FillPortion, Padding, Vector};

use super::{Message, State, View, entries, panel, rail};
use crate::animation::Spring;
use crate::app::{App, Message as AppMessage};
use crate::artwork::Wants;
use crate::discover::{Found, Kind};
use crate::model::{Date, Library, MediaType, Released};
use crate::screens::entry;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};
use crate::upcoming::{self, GroupKey, SortKey, Upcoming};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// Which titles the page lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Slice {
    #[default]
    Library,
    Discover,
    All,
}

impl Slice {
    pub const ALL: [Slice; 3] = [Slice::Library, Slice::Discover, Slice::All];

    pub fn label(self) -> &'static str {
        match self {
            Slice::Library => "Library",
            Slice::Discover => "Discover",
            Slice::All => "All",
        }
    }

    fn index(self) -> usize {
        Slice::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// The slice `by` steps on, wrapping at both ends.
    pub fn step(self, by: isize) -> Slice {
        let n = Slice::ALL.len() as isize;
        Slice::ALL[(self.index() as isize + by).rem_euclid(n) as usize]
    }

    /// Whether going from `self` to `to` reads as forward: the next
    /// segment, or the wrap from All to Library; the other way back.
    pub fn forward_to(self, to: Slice) -> bool {
        let n = Slice::ALL.len();
        (self.index() + 1) % n == to.index()
    }
}

/// How much longer the slide takes than KWin's desktop switch at the same
/// Plasma speed: twice (owner's request, 2026-11-07, after the KWin copy
/// felt quick next to the phone's swipe, which settles on Flutter's
/// softer scroll spring). At the owner's 0.25 that is KWin's spring at
/// normal speed.
pub const SLOWER: f32 = 2.0;

/// The space between two slices as they pass, KWin's gap between desktops
/// (`HorizontalGap`, 45 px by default).
pub const GAP: f32 = 45.0;

/// The list on its way to another slice, moved as KWin moves the desktops:
/// the page is a row of slices, Library, Discover, All and round again,
/// and the list's position along it (in slices) rides
/// `animation::Spring::desktop_switch`, slowed by `SLOWER`, toward the
/// slice picked. A switch
/// during a switch starts from where the list is, at the speed it has, as
/// KWin's does, so quick presses run on smoothly.
#[derive(Debug, Clone, PartialEq)]
pub struct SliceSlide {
    since: Instant,
    /// Where the list was at `since`, in slices from Library.
    start: f32,
    /// Its velocity then, in slices a second.
    velocity: f32,
    /// Where it comes to rest: the picked slice's place one step from the
    /// slice it was headed for, so All to Library wraps forward and
    /// Library to All back.
    pub anchor: f32,
    spring: Spring,
    end: Instant,
}

impl SliceSlide {
    /// A slide to `to` from `from` at rest, or from wherever `moving` has
    /// the list, keeping its speed. `slot` is a slice's width with the
    /// gap, in pixels: the slide ends as KWin's does, under a pixel from
    /// rest and slower than a pixel a second.
    pub fn toward(
        moving: Option<&SliceSlide>,
        from: Slice,
        to: Slice,
        spring: Spring,
        slot: f32,
        now: Instant,
    ) -> SliceSlide {
        let (start, velocity, heading) = match moving {
            Some(moving) => {
                let (start, velocity) = moving.at(now);
                (start, velocity, moving.anchor)
            }
            None => (from.index() as f32, 0.0, from.index() as f32),
        };
        // The step from the slice the list is headed for, so Discover to
        // All goes on forward even while the list is still nearer Library.
        let anchor = nearest(to, heading);
        let epsilon = 1.0 / slot.max(1.0);
        SliceSlide {
            since: now,
            start,
            velocity,
            anchor,
            spring,
            end: now + spring.settle(start - anchor, velocity, epsilon),
        }
    }

    fn at(&self, now: Instant) -> (f32, f32) {
        let t = now.saturating_duration_since(self.since).as_secs_f32();
        let (distance, velocity) = self.spring.at(self.start - self.anchor, self.velocity, t);
        (self.anchor + distance, velocity)
    }

    /// Where the list is at `now`, in slices from Library.
    pub fn position(&self, now: Instant) -> f32 {
        if self.done(now) {
            self.anchor
        } else {
            self.at(now).0
        }
    }

    /// Whether the slice it left lies ahead: the list moves left.
    pub fn forward(&self) -> bool {
        self.anchor > self.start
    }

    pub fn done(&self, now: Instant) -> bool {
        now >= self.end
    }
}

/// The place of `slice` in the endless row nearest `position`.
fn nearest(slice: Slice, position: f32) -> f32 {
    let n = Slice::ALL.len() as f32;
    let index = slice.index() as f32;
    index + ((position - index) / n).round() * n
}

/// One row of the page.
#[derive(Debug, Clone)]
pub enum SoonRow<'a> {
    Library(Upcoming<'a>),
    Found(&'a Found),
}

impl<'a> SoonRow<'a> {
    fn media(&self) -> MediaType {
        match self {
            SoonRow::Library(row) => row.entry.media,
            SoonRow::Found(found) => found.media,
        }
    }

    fn title(&self) -> &str {
        match self {
            SoonRow::Library(row) => &row.entry.title,
            SoonRow::Found(found) => &found.title,
        }
    }

    fn sub(&self) -> Option<String> {
        match self {
            SoonRow::Library(row) => row.sub(),
            SoonRow::Found(found) => found.sub.clone(),
        }
    }

    /// The line under the title: the kind and the creator, or why the
    /// item is here.
    fn line(&self) -> String {
        match self {
            SoonRow::Library(row) => row.line(),
            SoonRow::Found(found) => found.why.clone(),
        }
    }

    fn released(&self) -> Option<Released> {
        match self {
            SoonRow::Library(row) => row.released,
            SoonRow::Found(found) => found.released,
        }
    }

    fn sort_key(&self) -> SortKey {
        SortKey::of(self.released())
    }

    /// The key a click and the arrow keys go by.
    pub fn key(&self) -> String {
        match self {
            SoonRow::Library(row) => row.key(),
            SoonRow::Found(found) => found.key.clone(),
        }
    }
}

/// The widget id of a row, for `reveal`.
pub fn row_key(key: &str) -> String {
    entries::row_id(key)
}

/// A group of rows under one header.
pub struct Group<'a> {
    pub key: GroupKey,
    pub rows: Vec<SoonRow<'a>>,
    /// The last group of Discover and All once "Hidden · N ›" was pressed.
    pub hidden: bool,
}

impl Group<'_> {
    fn name(&self, today: Date) -> String {
        if self.hidden {
            "Hidden".into()
        } else {
            self.key.name(today)
        }
    }

    fn rail(&self, today: Date) -> String {
        if self.hidden {
            "Hidden".into()
        } else {
            self.key.rail(today)
        }
    }
}

/// The page's lists for `app`, worked out once per view.
pub struct Lists<'a> {
    pub library: Vec<Upcoming<'a>>,
    pub discover: Vec<&'a Found>,
    pub hidden: Vec<&'a Found>,
}

impl<'a> Lists<'a> {
    pub fn of(app: &'a App) -> Lists<'a> {
        let library = app.repo.library();
        Lists {
            library: upcoming::library_upcoming(library, app.today),
            discover: app.discover.visible(library),
            hidden: app.discover.hidden_items(library),
        }
    }

    /// The rows of `slice`, before the type chips, in the lists' order.
    pub fn pool(&self, slice: Slice) -> Vec<SoonRow<'a>> {
        let mut rows: Vec<SoonRow<'a>> = Vec::new();
        if slice != Slice::Discover {
            rows.extend(self.library.iter().copied().map(SoonRow::Library));
        }
        if slice != Slice::Library {
            rows.extend(self.discover.iter().copied().map(SoonRow::Found));
        }
        sort_rows(&mut rows);
        rows
    }

    /// `rows` narrowed to `types` (every type with none picked).
    pub fn typed(rows: Vec<SoonRow<'a>>, types: &BTreeSet<MediaType>) -> Vec<SoonRow<'a>> {
        if types.is_empty() {
            return rows;
        }
        rows.into_iter()
            .filter(|row| types.contains(&row.media()))
            .collect()
    }

    /// The hidden items narrowed to `types`.
    fn typed_hidden(&self, types: &BTreeSet<MediaType>) -> Vec<&'a Found> {
        self.hidden
            .iter()
            .copied()
            .filter(|found| types.is_empty() || types.contains(&found.media))
            .collect()
    }

    /// How many rows `slice` has with the chips of `types`.
    pub fn count(&self, slice: Slice, types: &BTreeSet<MediaType>) -> usize {
        Lists::typed(self.pool(slice), types).len()
    }
}

fn sort_rows(rows: &mut [SoonRow<'_>]) {
    rows.sort_by(|a, b| upcoming::compare(a.sort_key(), a.title(), b.sort_key(), b.title()));
}

/// `rows` in their groups (month, year, none), in order.
pub fn groups<'a>(rows: Vec<SoonRow<'a>>) -> Vec<Group<'a>> {
    let mut groups: Vec<Group<'a>> = Vec::new();
    for row in rows {
        let key = GroupKey::of(row.released());
        match groups.last_mut() {
            Some(group) if group.key == key => group.rows.push(row),
            _ => groups.push(Group {
                key,
                rows: vec![row],
                hidden: false,
            }),
        }
    }
    groups
}

/// The groups the page shows now: the slice's rows under the chips, and
/// the hidden items as a last group when asked for.
fn page_groups<'a>(state: &State, lists: &Lists<'a>) -> Vec<Group<'a>> {
    let types = state.soon_types();
    let mut groups = groups(Lists::typed(lists.pool(state.slice), &types));
    if state.show_hidden && state.slice != Slice::Library {
        let hidden = lists.typed_hidden(&types);
        if !hidden.is_empty() {
            let mut rows: Vec<SoonRow<'a>> = hidden.into_iter().map(SoonRow::Found).collect();
            sort_rows(&mut rows);
            groups.push(Group {
                key: GroupKey::None,
                rows,
                hidden: true,
            });
        }
    }
    groups
}

/// The entries of the page's library rows in order, once each, for the
/// arrow keys.
pub fn entry_order(state: &State, library: &Library, today: Date) -> Vec<String> {
    if state.slice == Slice::Discover {
        return Vec::new();
    }
    let rows = upcoming::library_upcoming(library, today);
    let types = state.soon_types();
    let mut ids: Vec<String> = Vec::new();
    for row in rows {
        if (types.is_empty() || types.contains(&row.entry.media)) && !ids.contains(&row.entry.id) {
            ids.push(row.entry.id.clone());
        }
    }
    ids
}

/// What the page draws from the lookups: the covers of its library rows
/// and the images of its Discover rows (the text view wants none), the
/// panel's, and the open preview's.
pub fn wanted(app: &App, wants: &mut Wants) {
    let state = &app.library;
    let lists = Lists::of(app);
    if state.view_mode() != View::Text {
        for group in page_groups(state, &lists) {
            for row in group.rows {
                match row {
                    SoonRow::Library(row) => wants.covers.push(row.entry.id.clone()),
                    SoonRow::Found(found) => want_found(found, wants),
                }
            }
        }
    }
    if let Some(found) = state
        .found
        .as_deref()
        .and_then(|key| app.discover.item(key))
    {
        want_found(found, wants);
    }
    if panel::panel(app) == panel::Panel::Pinned && !state.pane_open() {
        wants.covers.extend(
            lists
                .library
                .iter()
                .take(NEXT_UP)
                .map(|row| row.entry.id.clone()),
        );
    }
}

/// A Discover item's picture: its cover URL, or for a season its show's
/// cover.
fn want_found(found: &Found, wants: &mut Wants) {
    match (&found.cover_url, &found.kind) {
        (Some(url), _) => wants.images.push(url.clone()),
        (None, Kind::Season { show, .. }) => wants.covers.push(show.clone()),
        _ => {}
    }
}

/// The covers of the type panel's popover.
pub fn popover_wanted(app: &App, wants: &mut Wants) {
    let rows = upcoming::library_upcoming(app.repo.library(), app.today);
    let types = &app.library.types;
    wants.covers.extend(
        rows.iter()
            .filter(|row| types.is_empty() || types.contains(&row.entry.media))
            .map(|row| row.entry.id.clone()),
    );
}

/// A Discover item's image, once fetched: by its URL, or its show's cover.
fn found_image(app: &App, found: &Found) -> Option<iced::widget::image::Handle> {
    match (&found.cover_url, &found.kind) {
        (Some(url), _) => app.artwork.image(url),
        (None, Kind::Season { show, .. }) => app
            .repo
            .library()
            .entry(show)
            .and_then(|entry| app.artwork.cover(entry)),
        _ => None,
    }
}

/// The placeholder tint of a Discover item: its show's for a season, else
/// one picked from its key.
fn found_tint(found: &Found) -> Color {
    match &found.kind {
        Kind::Season { show, .. } => theme::cover_color(show),
        _ => theme::cover_color(&found.key),
    }
}

/// The footer's line: when the check ran and whom it asked.
pub fn check_line(app: &App) -> String {
    app.discover
        .check_line(crate::clock::Timestamp::now())
        .unwrap_or_else(|| "Libri checks for new releases and dates once a day.".into())
}

/// The page's two fixed rows (the header and the chips) and its list.
pub fn view<'a>(app: &'a App) -> (Element<'a, AppMessage>, Element<'a, AppMessage>) {
    let tokens = app.tokens();
    let state = &app.library;
    let lists = Lists::of(app);
    let types = state.soon_types();
    let top = column![
        header(app, &lists, &types, &tokens),
        chips(app, &lists, &types, &tokens)
    ];
    (top.into(), list_area(app, &lists, &tokens))
}

/// The header row: the title with the slice's count, the segmented
/// control, the view switch and the panel button. No Add, search or sort.
fn header<'a>(
    app: &'a App,
    lists: &Lists<'a>,
    types: &BTreeSet<MediaType>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    let count = lists.count(state.slice, types);
    let segments = segmented(
        Slice::ALL.map(|slice| (slice, lists.count(slice, types), slice == state.slice)),
        tokens,
    );
    let current = state.view_mode();
    let views = ui::icon_switch(
        [
            (Icon::ViewGrid, View::Grid),
            (Icon::ViewList, View::List),
            (Icon::ViewTable, View::Text),
        ]
        .map(|(which, view)| (which, view == current, msg(Message::SetView(view)))),
        tokens,
    );
    let bar = row![
        text("Coming soon")
            .size(theme::size::HEADING)
            .font(font::EXTRABOLD),
        ui::mono(count.to_string(), 13.0, tokens.sub),
        container(segments).padding(Padding::new(0.0).left(4)),
        space::horizontal(),
        views,
        row(panel::toggle(app)),
    ]
    .spacing(10)
    .align_y(Center);
    container(bar)
        .height(64)
        .padding(Padding::new(0.0).left(24).right(20))
        .align_y(Center)
        .into()
}

/// The silence that ends a wheel gesture.
const WHEEL_QUIET: Duration = Duration::from_millis(150);
/// A notch of a wheel in the pixels of a high-resolution wheel or a
/// touchpad (15 logical pixels on Plasma, at a scale of 2).
const WHEEL_NOTCH_PIXELS: f32 = 30.0;

/// The wheel events towards the next slice switch: a side-scroll, or the
/// wheel with Shift held. One gesture is one step however long it runs, or
/// a held tilt wheel and a touchpad's swipe would race through the slices.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelGesture {
    notches: f32,
    last: Option<Instant>,
    spent: bool,
}

impl WheelGesture {
    /// Takes in a wheel event and says which way to step, if it should.
    pub fn feed(&mut self, delta: ScrollDelta, shift: bool, now: Instant) -> Option<isize> {
        let (x, y, per_notch) = match delta {
            ScrollDelta::Lines { x, y } => (x, y, 1.0),
            ScrollDelta::Pixels { x, y } => (x, y, WHEEL_NOTCH_PIXELS),
        };
        let quiet = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= WHEEL_QUIET);
        self.last = Some(now);
        if quiet {
            *self = WheelGesture {
                last: self.last,
                ..WheelGesture::default()
            };
        }
        let amount = match shift {
            true if y != 0.0 => y,
            true => x,
            // Mostly vertical is plain scrolling, whatever the drift.
            false if x.abs() > y.abs() => x,
            false => {
                self.notches = 0.0;
                return None;
            }
        } / per_notch;
        if self.spent {
            return None;
        }
        if self.notches * amount < 0.0 {
            self.notches = 0.0;
        }
        self.notches += amount;
        if self.notches.abs() < 1.0 {
            return None;
        }
        self.spent = true;
        // Scrolling towards the right or down (a negative delta) goes on.
        Some(if self.notches < 0.0 { 1 } else { -1 })
    }
}

/// Library N | Discover N | All N, in the view switch's style.
fn segmented<'a>(items: [(Slice, usize, bool); 3], tokens: &Tokens) -> Element<'a, AppMessage> {
    let t = *tokens;
    let last = items.len() - 1;
    let buttons = items
        .into_iter()
        .enumerate()
        .map(|(index, (slice, count, on))| {
            let radius = ui::segmented::segment_radius(index, last, 7.0);
            let color = if on { t.text } else { t.sub };
            let content = row![
                text(slice.label())
                    .size(13.5)
                    .font(font::SEMIBOLD)
                    .color(color),
                ui::mono(count.to_string(), 12.0, t.sub),
            ]
            .spacing(7)
            .align_y(Center)
            .height(Fill);
            let segment = button(content)
                .height(34)
                .padding([0, 14])
                .on_press(msg(Message::SoonSlice(slice)))
                .style(move |_, status| button::Style {
                    background: (on || matches!(status, button::Status::Hovered))
                        .then_some(Background::Color(if on { t.selection } else { t.hover })),
                    text_color: color,
                    border: Border {
                        radius,
                        ..Border::default()
                    },
                    ..button::Style::default()
                })
                .focusable_inside(7.0, tokens);
            let line = t.line;
            let divider = container(space())
                .width(if index < last { 1 } else { 0 })
                .height(34)
                .style(move |_| container::background(line));
            Row::new().push(segment).push(divider).into()
        });
    let group = container(Row::with_children(buttons))
        .padding(1)
        .clip(true)
        .style(move |_| {
            container::Style::default().border(Border::default().rounded(8).width(1).color(t.line))
        });
    group.into()
}

/// The chips row: one chip per type present in the slice, with its dot
/// and count, and "All types" while any is picked.
fn chips<'a>(
    app: &'a App,
    lists: &Lists<'a>,
    types: &BTreeSet<MediaType>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let pool = lists.pool(app.library.slice);
    let mut chips = Row::new().spacing(8).align_y(Center);
    for media in MediaType::ALL {
        let count = pool.iter().filter(|row| row.media() == media).count();
        if count == 0 {
            continue;
        }
        chips = chips.push(ui::filter_chip(
            media.label().to_owned(),
            Some(tokens.type_color(media)),
            Some(count),
            types.contains(&media),
            tokens,
            msg(Message::SoonType(media)),
        ));
    }
    if !types.is_empty() {
        chips = chips.push(ui::header_link(
            "All types",
            false,
            tokens,
            msg(Message::SoonAllTypes),
        ));
    }
    container(chips)
        .height(44)
        .padding(Padding::new(0.0).left(24).right(20))
        .align_y(Center)
        .into()
}

/// The list, sliding between slices when a switch is under way: the
/// slices side by side with KWin's gap between them, the list's position
/// along the row from the spring; only the list moves, and nothing fades.
/// The picked slice is the live list, always the first layer so its
/// scroll state stays; the others pass as still copies at their own
/// scroll offsets.
fn list_area<'a>(app: &'a App, lists: &Lists<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.library;
    let current = list(app, lists, state.slice, false, tokens);
    let width = if state.list_width > 0.0 {
        state.list_width
    } else {
        crate::screens::shell::content_width(app, 0.0)
    };
    let slot = width + GAP;
    let position = state
        .slide
        .as_ref()
        .map_or(state.slice.index() as f32, |moving| {
            moving.position(Instant::now())
        });
    let live = nearest(state.slice, position);
    let mut layers = vec![ui::slide_by(current, Vector::new((live - position) * slot, 0.0)).into()];
    if state.slide.is_some() {
        let first = position.floor();
        for place in [first, first + 1.0] {
            let shift = (place - position) * slot;
            if place == live || shift.abs() >= slot {
                continue;
            }
            let n = Slice::ALL.len() as isize;
            let slice = Slice::ALL[(place as isize).rem_euclid(n) as usize];
            let scroll = state.slice_scroll.get(&slice).copied().unwrap_or(0.0);
            let still = ui::slide_by(
                list(app, lists, slice, true, tokens),
                Vector::new(0.0, -scroll),
            );
            layers.push(ui::slide_by(still, Vector::new(shift, 0.0)).into());
        }
    }
    container(Stack::with_children(layers).width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .clip(true)
        .into()
}

/// The rows of `slice` with the rail: in their groups with the pinned
/// header, or in the grid one continuous block; or the empty state. `still` builds the copy that slides out: a
/// plain column, since the scroll area is the incoming list's.
fn list<'a>(
    app: &'a App,
    lists: &Lists<'a>,
    slice: Slice,
    still: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    let types = state.slice_types.get(&slice).cloned().unwrap_or_default();
    let shown = State {
        slice,
        slice_types: state.slice_types.clone(),
        show_hidden: state.show_hidden && slice == state.slice,
        ..State::default()
    };
    let groups = page_groups(&shown, lists);
    if groups.is_empty() {
        return empty(app, lists, slice, &types, tokens);
    }
    let view = state.view_mode();
    let names: Vec<String> = groups.iter().map(|g| g.rail(app.today)).collect();
    let on_rail = (groups.len() >= 2).then(|| rail::Sections::new(names, false));
    let room = on_rail.as_ref().map_or(rail::LEFT, rail::Sections::room);
    let mut children: Vec<Element<'a, AppMessage>> = if view == View::Grid {
        vec![tiles(app, &groups, slice, tokens)]
    } else {
        groups
            .iter()
            .map(|group| {
                column![
                    entries::bar_header(
                        &group.name(app.today),
                        tokens.ink,
                        tokens.text,
                        group.rows.len(),
                        tokens
                    ),
                    block(app, group, slice, view == View::List, tokens)
                ]
                .into()
            })
            .collect()
    };
    let hidden = lists.typed_hidden(&types);
    if slice != Slice::Library && !hidden.is_empty() && !shown.show_hidden {
        let t = *tokens;
        children.push(
            container(
                button(
                    row![
                        text(format!("Hidden · {}", hidden.len())).size(13),
                        ui::icon(Icon::ChevronRight, 12.0, t.sub),
                    ]
                    .spacing(6)
                    .align_y(Center)
                    .height(Fill),
                )
                .height(32)
                .padding([0, 8])
                .on_press(msg(Message::ShowHidden))
                .style(move |_, _| button::Style {
                    text_color: t.sub,
                    ..button::Style::default()
                })
                .focusable(8.0, tokens),
            )
            .padding(Padding::new(0.0).top(8))
            .into(),
        );
    }
    if still {
        return container(Column::with_children(children))
            .padding(Padding::new(rail::LEFT).top(4).right(room))
            .width(Fill)
            .into();
    }
    let mut overlays = Vec::new();
    if view != View::Grid
        && let Some(index) = entries::current_group(state)
        && state.scroll.offset > state.group_tops[index] + 0.5
        && let Some(group) = groups.get(index)
    {
        overlays.push(entries::pinned_bar(
            entries::bar_header(
                &group.name(app.today),
                tokens.ink,
                tokens.text,
                group.rows.len(),
                tokens,
            ),
            tokens,
        ));
    }
    if let Some(on_rail) = on_rail {
        let on_rail = if view == View::Grid {
            let counts: Vec<usize> = groups.iter().map(|g| g.rows.len()).collect();
            on_rail.in_grid(state, &counts, grid_rows(state.list_width, room))
        } else {
            on_rail.in_list(state)
        };
        overlays.push(on_rail.view(state, tokens));
    }
    entries::scrolling(children, overlays, room)
}

/// A group's rows in the text view, or with `covers` the cover view.
fn block<'a>(
    app: &'a App,
    group: &Group<'a>,
    slice: Slice,
    covers: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let rows = group.rows.iter().map(|row| {
        let look = Look::of(app, row, slice, group.hidden, tokens);
        let id = row_key(&row.key());
        let element = if covers {
            cover_row(app, row, look, tokens)
        } else {
            text_row(row, look, tokens)
        };
        container(element).id(id).into()
    });
    if covers {
        Column::with_children(rows)
            .spacing(2)
            .padding(Padding::new(0.0).top(4).bottom(16))
            .into()
    } else {
        Column::with_children(rows)
            .padding(Padding::new(0.0).bottom(16))
            .into()
    }
}

/// Every group's rows as one continuous grid, in date order.
fn tiles<'a>(
    app: &'a App,
    groups: &[Group<'a>],
    slice: Slice,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let cards = groups.iter().flat_map(|group| {
        group.rows.iter().map(move |row| {
            let look = Look::of(app, row, slice, group.hidden, tokens);
            container(tile(app, row, look, tokens))
                .id(row_key(&row.key()))
                .into()
        })
    });
    container(
        grid(cards)
            .fluid(TILE_WIDTH)
            .spacing(TILE_SPACING)
            .height(grid::aspect_ratio(TILE_WIDTH, TILE_HEIGHT)),
    )
    .padding(Padding::new(0.0).top(8).bottom(24))
    .into()
}

/// The rows of the grid in a list `list_width` wide with `room` kept for
/// the rail at its right.
fn grid_rows(list_width: f32, room: f32) -> rail::GridRows {
    rail::GridRows::new(
        list_width - rail::LEFT - room,
        TILE_WIDTH,
        TILE_SPACING,
        TILE_WIDTH / TILE_HEIGHT,
    )
}

/// A grid tile's width at most (the canvas's tiles are at least 140 px
/// and grow; iced's grid shrinks from a largest), and its height: a 2:3
/// cover and the caption under it.
const TILE_WIDTH: f32 = 160.0;
const TILE_HEIGHT: f32 = TILE_WIDTH * 1.5 + 28.0;
const TILE_SPACING: f32 = 16.0;

/// What marks a row at its right: a library row's Want in All, a
/// Discover row's + Add, a hidden row's Restore, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Want,
    Add,
    Restore,
}

/// What a row shows, worked out once for the three views.
struct Look {
    color: Color,
    selected: bool,
    dimmed: bool,
    mark: Mark,
    date: String,
    date_color: Color,
    on_press: AppMessage,
    /// The key the mark acts on, for a Discover row.
    key: String,
}

impl Look {
    fn of(app: &App, row: &SoonRow<'_>, slice: Slice, hidden: bool, tokens: &Tokens) -> Look {
        let state = &app.library;
        let (selected, mark, on_press) = match row {
            SoonRow::Library(upcoming) => (
                state.selected.as_deref() == Some(upcoming.entry.id.as_str()),
                if slice == Slice::All {
                    Mark::Want
                } else {
                    Mark::None
                },
                msg(Message::Select(upcoming.entry.id.clone())),
            ),
            SoonRow::Found(found) => (
                state.found.as_deref() == Some(found.key.as_str()),
                if hidden { Mark::Restore } else { Mark::Add },
                msg(Message::SelectFound(found.key.clone())),
            ),
        };
        let released = row.released();
        Look {
            color: tokens.type_color(row.media()),
            selected,
            dimmed: hidden || (state.pane_open() && !selected),
            mark,
            date: upcoming::date_label(released, app.today),
            date_color: if released.is_some() {
                tokens.sub
            } else {
                tokens.line
            },
            on_press,
            key: row.key(),
        }
    }
}

/// The mark as a control: the Want pill, the + Add button or the Restore
/// link, at `height` px; `plain` draws Want and Restore as words alone
/// (the text view).
fn mark<'a>(look: &Look, height: f32, plain: bool, tokens: &Tokens) -> Element<'a, AppMessage> {
    let t = *tokens;
    match look.mark {
        Mark::None => space().width(0).into(),
        Mark::Want if plain => text("Want").size(12.5).color(t.sub).into(),
        Mark::Want => container(text("Want").size(12.5).font(font::SEMIBOLD).color(t.sub))
            .height(height - 2.0)
            .padding([0, 10])
            .align_y(Center)
            .style(move |_| {
                container::Style::default().border(
                    Border::default()
                        .rounded(height / 2.0)
                        .width(1)
                        .color(t.line),
                )
            })
            .into(),
        Mark::Add => button(
            text("+ Add")
                .size(if plain { 12.5 } else { 13.0 })
                .font(font::SEMIBOLD)
                .height(Fill)
                .align_y(Center),
        )
        .height(if plain { 24.0 } else { height })
        .padding([0, if plain { 10 } else { 12 }])
        .on_press(msg(Message::AddFound(look.key.clone())))
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered)
                .then_some(Background::Color(t.hover)),
            text_color: t.text,
            border: Border::default()
                .rounded(height / 2.0)
                .width(1)
                .color(t.line),
            ..button::Style::default()
        })
        .focusable(height / 2.0, tokens),
        Mark::Restore => button(
            text("Restore")
                .size(if plain { 12.5 } else { 13.0 })
                .font(font::SEMIBOLD)
                .height(Fill)
                .align_y(Center),
        )
        .height(if plain { 24.0 } else { height })
        .padding([0, if plain { 0 } else { 12 }])
        .on_press(msg(Message::RestoreFound(look.key.clone())))
        .style(move |_, _| button::Style {
            text_color: t.ink,
            border: if plain {
                Border::default()
            } else {
                Border::default()
                    .rounded(height / 2.0)
                    .width(1)
                    .color(t.line)
            },
            ..button::Style::default()
        })
        .focusable(height / 2.0, tokens),
    }
}

/// A title with its sub after it in the secondary color: "Severance ·
/// Season 3".
fn titled<'a>(
    row: &SoonRow<'_>,
    size: f32,
    weight: iced::Font,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let mut line = row![text(row.title().to_owned()).size(size).font(weight)]
        .spacing(0)
        .align_y(iced::alignment::Vertical::Bottom);
    if let Some(sub) = row.sub() {
        line = line.push(
            text(format!(" · {sub}"))
                .size(size)
                .font(font::MEDIUM)
                .color(tokens.sub),
        );
    }
    container(line).clip(true).into()
}

/// The veil that dims a row of the hidden group, or the grid's other
/// tiles while one is selected.
fn dim<'a>(
    content: Element<'a, AppMessage>,
    dimmed: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    if !dimmed {
        return content;
    }
    let veil = tokens.content.scale_alpha(0.45);
    stack![
        content,
        container(space())
            .width(Fill)
            .height(Fill)
            .style(move |_| container::background(veil))
    ]
    .into()
}

/// A text row, 42 px: the type dot, the title, the line, the mark and
/// the date, with a rule under it.
fn text_row<'a>(row: &SoonRow<'a>, look: Look, tokens: &Tokens) -> Element<'a, AppMessage> {
    let content = row![
        row![
            ui::dot(look.color, 8.0, 3.0),
            titled(row, 14.0, font::REGULAR, tokens)
        ]
        .spacing(12)
        .align_y(Center)
        .width(FillPortion(16)),
        container(
            text(row.line())
                .size(13)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None)
        )
        .width(FillPortion(14))
        .clip(true),
        container(mark(&look, 24.0, true, tokens))
            .width(70)
            .align_right(70),
        container(
            ui::mono(look.date.clone(), 12.5, look.date_color).wrapping(text::Wrapping::None)
        )
        .width(76)
        .align_right(76),
    ]
    .spacing(12)
    .align_y(Center)
    .height(Fill);
    let row_button = button(content)
        .width(Fill)
        .height(42)
        .padding([0, 12])
        .on_press(look.on_press.clone())
        .style(ui::row_style(tokens, look.selected, 0.0))
        .focusable(0.0, tokens);
    let dimmed = look.mark == Mark::Restore;
    column![
        dim(row_button, dimmed, tokens),
        ui::rule(tokens.line.scale_alpha(0.5))
    ]
    .into()
}

/// A cover row: a 44 × 66 cover, the title and the line with the type
/// dot, the mark and the date.
fn cover_row<'a>(
    app: &'a App,
    row: &SoonRow<'a>,
    look: Look,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let (image, tint) = match row {
        SoonRow::Library(upcoming) => (
            app.artwork.cover(upcoming.entry),
            theme::cover_color(&upcoming.entry.id),
        ),
        SoonRow::Found(found) => (found_image(app, found), found_tint(found)),
    };
    let words = column![
        titled(row, 15.0, font::SEMIBOLD, tokens),
        row![
            ui::dot(look.color, 8.0, 3.0),
            text(row.line())
                .size(13)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(7)
        .align_y(Center),
    ]
    .spacing(3)
    .width(Fill)
    .clip(true);
    let content = row![
        ui::plain_cover(image, tint, 44.0, 66.0, 5.0),
        words,
        mark(&look, 28.0, false, tokens),
        container(
            ui::mono(look.date.clone(), 12.5, look.date_color).wrapping(text::Wrapping::None)
        )
        .width(76)
        .align_right(76),
    ]
    .spacing(14)
    .align_y(Center);
    let row_button = button(content)
        .width(Fill)
        .padding([8, 12])
        .on_press(look.on_press.clone())
        .style(ui::row_style(tokens, look.selected, 10.0))
        .focusable(10.0, tokens);
    dim(row_button, look.mark == Mark::Restore, tokens)
}

/// A grid tile: the 2:3 cover with the title on its placeholder and the
/// mark top right, then the title and the date.
fn tile<'a>(
    app: &'a App,
    row: &SoonRow<'a>,
    look: Look,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let (image, tint) = match row {
        SoonRow::Library(upcoming) => (
            app.artwork.cover(upcoming.entry),
            theme::cover_color(&upcoming.entry.id),
        ),
        SoonRow::Found(found) => (found_image(app, found), found_tint(found)),
    };
    let white = Color::WHITE;
    let badge: Element<'a, AppMessage> = match look.mark {
        Mark::None => space().into(),
        Mark::Want => container(text("Want").size(11.5).font(font::SEMIBOLD).color(white))
            .height(22)
            .padding([0, 8])
            .align_y(Center)
            .style(|_| {
                container::background(Color::from_rgba(0.0, 0.0, 0.0, 0.45))
                    .border(Border::default().rounded(11))
            })
            .into(),
        Mark::Add | Mark::Restore => {
            let (label, message) = if look.mark == Mark::Add {
                ("+ Add", Message::AddFound(look.key.clone()))
            } else {
                ("Restore", Message::RestoreFound(look.key.clone()))
            };
            button(
                text(label)
                    .size(12)
                    .font(font::SEMIBOLD)
                    .color(white)
                    .wrapping(text::Wrapping::None)
                    .height(Fill)
                    .align_y(Center),
            )
            .height(24)
            .padding([0, 9])
            .on_press(msg(message))
            .style(move |_, status| button::Style {
                background: Some(Background::Color(Color::from_rgba(
                    0.0,
                    0.0,
                    0.0,
                    if matches!(status, button::Status::Hovered) {
                        0.7
                    } else {
                        0.5
                    },
                ))),
                text_color: white,
                border: Border::default()
                    .rounded(12)
                    .width(1)
                    .color(Color::from_rgba(1.0, 1.0, 1.0, 0.6)),
                ..button::Style::default()
            })
            .focusable(12.0, tokens)
        }
    };
    let top = row![space::horizontal(), badge].align_y(Center).height(24);
    let mut on_cover = column![top, space::vertical()].spacing(4);
    if image.is_none() {
        on_cover = on_cover.push(
            text(row.title().to_owned())
                .size(17)
                .font(font::EXTRABOLD)
                .color(white),
        );
        if let Some(sub) = row.sub() {
            on_cover = on_cover.push(text(sub).size(12.5).font(font::MEDIUM).color(white));
        }
    }
    let face = ui::cover(image, tint, Fill, Fill, 8.0, None);
    let ring = look.selected.then_some(tokens.accent);
    let layers = stack![
        face,
        container(on_cover)
            .width(Fill)
            .height(Fill)
            .padding(Padding::new(14.0).top(10).right(10))
    ];
    let cover = container(layers).style(move |_| {
        container::Style::default().border(
            Border::default()
                .rounded(8)
                .width(if ring.is_some() { 3.0 } else { 0.0 })
                .color(ring.unwrap_or(Color::TRANSPARENT)),
        )
    });
    let caption = row![
        container(
            text(row.title().to_owned())
                .size(14)
                .font(font::SEMIBOLD)
                .wrapping(text::Wrapping::None)
        )
        .width(Fill)
        .clip(true),
        ui::mono(look.date.clone(), 12.0, look.date_color),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Bottom)
    .height(20);
    let card = column![cover, caption].spacing(8);
    let text_color = tokens.text;
    button(dim(card.into(), look.dimmed, tokens))
        .padding(0)
        .on_press(look.on_press.clone())
        .style(move |_, _| button::Style {
            text_color,
            ..button::Style::default()
        })
        .focusable(8.0, tokens)
}

/// Nothing to list: nothing at all on the way, or nothing in this slice
/// or for these types.
fn empty<'a>(
    app: &'a App,
    lists: &Lists<'a>,
    slice: Slice,
    types: &BTreeSet<MediaType>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let nothing_at_all = lists.library.is_empty() && lists.discover.is_empty();
    let (title, body) = if nothing_at_all {
        (
            "Nothing coming up yet".to_owned(),
            "Add titles you’re waiting for as Want with a release date and they show up here. Libri also checks once a day for next seasons, next parts and new work by creators in your library, and lists what it finds under Discover.".to_owned(),
        )
    } else {
        let title = if !types.is_empty() {
            let names: Vec<String> = types.iter().map(|m| m.label().to_lowercase()).collect();
            format!("Nothing coming up for {}", names.join(", "))
        } else if slice == Slice::Discover {
            "Nothing new found".to_owned()
        } else {
            "Nothing in your library yet".to_owned()
        };
        let body = if slice == Slice::Discover {
            "Libri checks once a day. Hidden titles are under Hidden at the bottom."
        } else {
            "Titles you want with a release date ahead show here."
        };
        (title, body.to_owned())
    };
    let _ = app;
    ui::empty_state(Icon::Clock, title, body, None, tokens)
}

// The panel on the page.

/// How many rows Next up and New in Discover show at most.
const NEXT_UP: usize = 3;
const NEW_ROWS: usize = 3;
/// An item found within this many days counts as new.
const NEW_DAYS: i64 = 7;
/// The glance's measures: a section's header, a cover row, a text row, a
/// By type row, the gaps.
const HEADER_H: f32 = 22.0;
const COVER_ROW_H: f32 = 50.0;
const TEXT_ROW_H: f32 = 34.0;
const TYPE_ROW_H: f32 = 26.0;
const SECTION_GAP: f32 = 24.0;
const LINE_H: f32 = 40.0;
const PANE_PAD: f32 = entry::details::PAD + 18.0;

/// Pinned with nothing selected: Next up, New in Discover, By type and
/// Hidden, the empty ones left out, dropped from the bottom when they do
/// not fit `height`, and the check line.
pub fn glance<'a>(app: &'a App, height: f32) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let t = tokens;
    let state = &app.library;
    let lists = Lists::of(app);
    let today = app.today;

    let next: Vec<&Upcoming<'a>> = lists.library.iter().take(NEXT_UP).collect();
    let since = today.add_days(-NEW_DAYS);
    let mut fresh: Vec<&Found> = lists
        .discover
        .iter()
        .copied()
        .filter(|found| found.found >= since)
        .collect();
    fresh.sort_by_key(|found| std::cmp::Reverse(found.found));
    let pool = lists.pool(state.slice);
    let per_type: Vec<(MediaType, usize)> = MediaType::ALL
        .into_iter()
        .map(|media| {
            (
                media,
                pool.iter().filter(|row| row.media() == media).count(),
            )
        })
        .filter(|(_, n)| *n > 0)
        .collect();
    let most = per_type.iter().map(|(_, n)| *n).max().unwrap_or(1) as f32;
    let hidden = &lists.hidden;

    // The sections with their heights, dropped from the bottom while too
    // tall for the panel.
    let mut sections: Vec<(usize, f32)> = Vec::new();
    if !next.is_empty() {
        sections.push((0, HEADER_H + 4.0 + next.len() as f32 * COVER_ROW_H));
    }
    if !fresh.is_empty() {
        let rows = fresh.len().min(NEW_ROWS) as f32;
        sections.push((1, HEADER_H + 8.0 + 18.0 + 8.0 + rows * TEXT_ROW_H));
    }
    if !per_type.is_empty() {
        sections.push((2, HEADER_H + 8.0 + per_type.len() as f32 * TYPE_ROW_H));
    }
    if !hidden.is_empty() {
        sections.push((3, HEADER_H + 6.0 + hidden.len() as f32 * TEXT_ROW_H));
    }
    let avail = height - PANE_PAD - LINE_H - SECTION_GAP;
    let total = |parts: &[(usize, f32)]| {
        parts.iter().map(|(_, h)| h).sum::<f32>()
            + parts.len().saturating_sub(1) as f32 * SECTION_GAP
    };
    while !sections.is_empty() && total(&sections) > avail {
        sections.pop();
    }
    let shown = |which: usize| sections.iter().any(|(i, _)| *i == which);

    let mut items = Column::new().spacing(SECTION_GAP);
    if shown(0) {
        let rows = Column::with_children(next.iter().map(|row| {
            small_cover_row(
                app.artwork.cover(row.entry),
                theme::cover_color(&row.entry.id),
                row.full_title(),
                row.line(),
                upcoming::date_label(row.released, today),
                msg(Message::Select(row.entry.id.clone())),
                &tokens,
            )
        }));
        items = items.push(
            column![
                container(ui::section_header("Next up", &tokens)).height(HEADER_H),
                rows
            ]
            .spacing(4),
        );
    }
    if shown(1) {
        let oldest = fresh.iter().map(|f| f.found).min().unwrap_or(today);
        let line = format!(
            "{} {} found since {}",
            fresh.len(),
            if fresh.len() == 1 { "title" } else { "titles" },
            since_label(oldest, today)
        );
        let rows = Column::with_children(fresh.iter().take(NEW_ROWS).map(|found| {
            dot_row(
                tokens.type_color(found.media),
                found.full_title(),
                upcoming::date_label(found.released, today),
                Some(msg(Message::OpenFound(found.key.clone()))),
                None,
                &tokens,
            )
        }));
        items = items.push(
            column![
                row![
                    ui::section_header("New in Discover", &tokens),
                    space::horizontal(),
                    ui::header_link(
                        "Open",
                        true,
                        &tokens,
                        msg(Message::SoonSlice(Slice::Discover))
                    )
                ]
                .align_y(Center)
                .height(HEADER_H),
                text(line).size(13).color(t.sub),
                rows
            ]
            .spacing(8),
        );
    }
    if shown(2) {
        let rows = Column::with_children(per_type.iter().map(|&(media, n)| {
            let color = tokens.type_color(media);
            let (track, fill) = (t.line, color);
            let bar = row![container(space()).width(Fill).height(6).style(move |_| {
                container::background(fill).border(Border::default().rounded(3))
            }),];
            let share = n as f32 / most;
            let bar = container(
                row![
                    container(bar).width(FillPortion((share * 1000.0).max(1.0) as u16)),
                    space().width(FillPortion(((1.0 - share) * 1000.0).max(0.0) as u16)),
                ]
                .width(Fill),
            )
            .width(Fill)
            .height(6)
            .style(move |_| container::background(track).border(Border::default().rounded(3)));
            button(
                row![
                    container(
                        row![ui::dot(color, 8.0, 3.0), text(media.label()).size(13.5)]
                            .spacing(8)
                            .align_y(Center)
                    )
                    .width(90),
                    bar,
                    container(ui::mono(n.to_string(), 12.0, t.sub))
                        .width(36)
                        .align_right(36),
                ]
                .spacing(10)
                .align_y(Center)
                .height(Fill),
            )
            .width(Fill)
            .height(TYPE_ROW_H)
            .padding(0)
            .on_press(msg(Message::SoonType(media)))
            .style(ui::row_style(&tokens, false, 6.0))
            .focusable(6.0, &tokens)
        }));
        items = items.push(
            column![
                container(ui::section_header("By type", &tokens)).height(HEADER_H),
                rows
            ]
            .spacing(8),
        );
    }
    if shown(3) {
        let rows = Column::with_children(hidden.iter().map(|found| {
            dot_row(
                tokens.type_color(found.media),
                found.full_title(),
                String::new(),
                None,
                Some(("Restore", msg(Message::RestoreFound(found.key.clone())))),
                &tokens,
            )
        }));
        items = items.push(
            column![
                row![
                    ui::section_header("Hidden", &tokens),
                    space::horizontal(),
                    ui::mono(hidden.len().to_string(), 12.0, t.sub)
                ]
                .align_y(Center)
                .height(HEADER_H),
                rows
            ]
            .spacing(6),
        );
    }
    let last = match app.discover.record.checked {
        Some(checked) => format!(
            "Libri checks for new releases and dates once a day. Last check: {}.",
            checked.day_and_time(crate::clock::Timestamp::now())
        ),
        None => "Libri checks for new releases and dates once a day. Not checked yet.".into(),
    };
    items = items.push(text(last).size(12.5).line_height(1.5).color(t.sub));
    items.into()
}

/// "Tuesday" within the week, else "Nov 1".
fn since_label(day: Date, today: Date) -> String {
    const DAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let ago = day.days_until(today);
    if ago == 0 {
        "today".into()
    } else if ago == 1 {
        "yesterday".into()
    } else if (2..7).contains(&ago) {
        DAYS[day.weekday()].into()
    } else {
        format!("{} {}", &day.month_name()[..3], day.day)
    }
}

/// A 50 px row with a 26 × 39 cover, a title, a line under it and the
/// date, with a rule under it: Next up and the type popover's rows.
fn small_cover_row<'a>(
    image: Option<iced::widget::image::Handle>,
    tint: Color,
    title: String,
    line: String,
    date: String,
    on_press: AppMessage,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let content = row![
        ui::plain_cover(image, tint, 26.0, 39.0, 4.0),
        column![
            text(title)
                .size(14)
                .font(font::SEMIBOLD)
                .wrapping(text::Wrapping::None),
            text(line)
                .size(12)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None)
        ]
        .spacing(1)
        .width(Fill)
        .clip(true),
        ui::mono(date, 12.0, tokens.sub),
    ]
    .spacing(12)
    .align_y(Center);
    let line_color = tokens.line;
    column![
        button(content.height(Fill))
            .width(Fill)
            .height(COVER_ROW_H - 1.0)
            .padding([0, 4])
            .on_press(on_press)
            .style(ui::row_style(tokens, false, 0.0))
            .focusable(0.0, tokens),
        container(space())
            .width(Fill)
            .height(1)
            .style(move |_| container::background(line_color))
    ]
    .into()
}

/// A 34 px row with a type dot, a title and a date or a link at the
/// right, with a rule under it: New in Discover and Hidden.
fn dot_row<'a>(
    color: Color,
    title: String,
    date: String,
    on_press: Option<AppMessage>,
    link: Option<(&'a str, AppMessage)>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let t = *tokens;
    let dimmed = link.is_some();
    let mut content = row![
        ui::dot(color, 8.0, 3.0),
        container(
            text(title)
                .size(14)
                .color(if dimmed { t.sub } else { t.text })
                .wrapping(text::Wrapping::None)
        )
        .width(Fill)
        .clip(true),
    ]
    .spacing(10)
    .align_y(Center);
    if let Some((label, message)) = link {
        content = content.push(
            button(text(label).size(13).font(font::SEMIBOLD))
                .padding(0)
                .on_press(message)
                .style(move |_, _| button::Style {
                    text_color: t.ink,
                    ..button::Style::default()
                })
                .focusable(4.0, tokens),
        );
    } else {
        content = content.push(ui::mono(date, 12.0, t.sub));
    }
    let body: Element<'a, AppMessage> = match on_press {
        Some(message) => button(content.height(Fill))
            .width(Fill)
            .height(TEXT_ROW_H - 1.0)
            .padding([0, 2])
            .on_press(message)
            .style(ui::row_style(tokens, false, 0.0))
            .focusable(0.0, tokens),
        None => container(content)
            .width(Fill)
            .height(TEXT_ROW_H - 1.0)
            .padding([0, 2])
            .align_y(Center)
            .into(),
    };
    let line = t.line;
    column![
        body,
        container(space())
            .width(Fill)
            .height(1)
            .style(move |_| container::background(line))
    ]
    .into()
}

/// A Discover item in the panel: the cover, the kicker, the title, the
/// creator, the date, why it is here, About and the source, with Add to
/// library and Not interested at the bottom.
pub fn preview<'a>(app: &'a App, found: &'a Found) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let t = tokens;
    let color = tokens.type_color(found.media);
    let kicker = format!("Upcoming · {}", crate::browse::kind_label_of(found.media)).to_uppercase();
    let mut heading = column![
        text(found.title.clone())
            .size(22)
            .font(font::EXTRABOLD)
            .line_height(1.1)
    ]
    .spacing(2);
    if let Some(sub) = &found.sub {
        heading = heading.push(text(sub.clone()).size(15).font(font::SEMIBOLD).color(t.sub));
    }
    let mut facts = column![ui::mono(kicker, 11.0, color), heading].spacing(6);
    if let Some(creator) = &found.creator {
        facts = facts.push(text(creator.clone()).size(14).color(t.sub));
    }
    let released = found.released;
    facts = facts.push(ui::mono(
        upcoming::date_label(released, app.today),
        12.5,
        if released.is_some() { t.sub } else { t.line },
    ));
    let cover = ui::titled_cover(
        found_image(app, found),
        found_tint(found),
        &found.title,
        96.0,
        144.0,
        14.0,
    );
    let head = row![
        cover,
        container(facts)
            .width(Fill)
            .padding(Padding::new(0.0).top(2))
    ]
    .spacing(16);
    let section = |label: &str, body: Element<'a, AppMessage>| {
        column![ui::section_header(label, &tokens), body].spacing(6)
    };
    let mut body = column![
        head,
        section(
            "Why it's here",
            text(found.why.clone()).size(14).line_height(1.5).into()
        )
    ]
    .spacing(16);
    if !found.description.is_empty() {
        let (about, _) = ui::clamp(
            &found.description,
            14.0,
            1.55,
            font::REGULAR,
            entry::details::INNER,
            8,
        );
        body = body.push(section(
            "About",
            text(about).size(14).line_height(1.55).color(t.sub).into(),
        ));
    }
    body = body.push(
        text(format!("From {}", found.provider))
            .size(12.5)
            .color(t.sub),
    );
    let actions = row![
        container(ui::labeled_button(
            "Add to library",
            ButtonKind::Primary,
            tokens,
            Some(msg(Message::AddFound(found.key.clone()))),
        ))
        .width(Fill),
        ui::labeled_button(
            "Not interested",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::HideFound(found.key.clone()))),
        ),
    ]
    .spacing(8);
    entry::details::pane(body.into(), Some(actions.into()))
}

/// The type panel's Coming soon popover: that type's (or types')
/// upcoming titles by month as cover rows, and "Open Coming soon ›".
/// Laid over the glance; a click outside it, its X or Esc closes it.
pub fn type_popover<'a>(app: &'a App, height: f32) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let t = tokens;
    let state = &app.library;
    let rows: Vec<Upcoming<'a>> = upcoming::library_upcoming(app.repo.library(), app.today)
        .into_iter()
        .filter(|row| state.types.is_empty() || state.types.contains(&row.entry.media))
        .collect();
    let (label, color) = match state.single() {
        Some(media) => (media.label().to_owned(), tokens.type_color(media)),
        None => (state.title(), t.sub),
    };
    let head = row![
        text("Coming soon").size(18).font(font::EXTRABOLD),
        text(label).size(13).font(font::SEMIBOLD).color(color),
        ui::mono(rows.len().to_string(), 12.0, t.sub),
        space::horizontal(),
        button(container(ui::icon(Icon::Close, 15.0, t.sub)).center(Fill))
            .width(28)
            .height(28)
            .padding(0)
            .on_press(msg(Message::CloseTypePop))
            .style(ui::row_style(&tokens, false, 7.0))
            .focusable(7.0, &tokens),
    ]
    .spacing(8)
    .align_y(Center);
    let mut list = Column::new().spacing(6).push(head);
    for group in groups(rows.into_iter().map(SoonRow::Library).collect()) {
        list = list.push(
            container(ui::section_header(&group.key.name(app.today), &tokens))
                .padding(Padding::new(0.0).top(8).bottom(2)),
        );
        for row in group.rows {
            let SoonRow::Library(row) = row else {
                continue;
            };
            list = list.push(small_cover_row(
                app.artwork.cover(row.entry),
                theme::cover_color(&row.entry.id),
                row.full_title(),
                row.line(),
                upcoming::date_label(row.released, app.today),
                msg(Message::PickFromTypePop(row.entry.id.clone())),
                &tokens,
            ));
        }
    }
    list = list.push(
        container(ui::header_link(
            "Open Coming soon",
            true,
            &tokens,
            msg(Message::OpenSoonForTypes),
        ))
        .padding(Padding::new(0.0).top(6)),
    );
    let dialog = container(ui::scroll(list).height(iced::Shrink))
        .padding(Padding::new(14.0).bottom(10))
        .max_height(height - 20.0)
        .width(Fill)
        .style(move |_| ui::panel_style(&t, 12.0));
    let outside = button(space().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .padding(0)
        .on_press(msg(Message::CloseTypePop))
        .style(|_, _| button::Style::default());
    stack![
        outside,
        container(dialog)
            .width(Fill)
            .height(Fill)
            .padding(10)
            .align_y(iced::alignment::Vertical::Top)
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::*;
    use crate::app::{App, KeyPress};
    use crate::browse::Section;
    use crate::model::EntryStatus;
    use crate::screens::Screen;

    fn lib(message: Message) -> AppMessage {
        AppMessage::Library(message)
    }

    /// The app on the page, with Discover's rows from the fake.
    fn on_page() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app.window = crate::app::WINDOW_SIZE;
        let _ = app.update(AppMessage::Discover(crate::discover::Message::Check));
        let _ = app.update(lib(Message::ShowSoon));
        app
    }

    #[test]
    fn home_see_all_opens_the_page_and_the_sidebar_row_keeps_the_slice() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(lib(Message::SeeAll(Section::ComingSoon)));
        let state = &app.library;
        assert!(state.soon && state.slice == Slice::Library);
        assert!(state.soon_types().is_empty());
        assert!(!state.is_overview());
        assert_eq!(state.title(), "Coming soon");
        assert!(state.place().soon);
        let _ = app.update(lib(Message::SoonSlice(Slice::Discover)));
        // Any list leaves the page; the row comes back to the slice left.
        let _ = app.update(lib(Message::ShowType(Some(MediaType::Movies))));
        assert!(!app.library.soon);
        assert_eq!(app.library.title(), "Movies");
        let _ = app.update(lib(Message::ShowSoon));
        assert!(app.library.soon && app.library.slice == Slice::Discover);
        // Back returns to Movies, Forward to the page.
        let _ = app.update(AppMessage::History(crate::history::Direction::Back));
        assert!(!app.library.soon);
        let _ = app.update(AppMessage::History(crate::history::Direction::Forward));
        assert!(app.library.soon);
    }

    #[test]
    fn type_chips_are_remembered_per_slice_and_ctrl_adds() {
        let mut app = on_page();
        let types = |app: &App| app.library.soon_types();
        let _ = app.update(lib(Message::SoonType(MediaType::Games)));
        assert_eq!(types(&app), BTreeSet::from([MediaType::Games]));
        let _ = app.update(lib(Message::SoonType(MediaType::Games)));
        assert!(types(&app).is_empty(), "the only chip clicked again clears");
        let _ = app.update(lib(Message::SoonType(MediaType::Movies)));
        let _ = app.update(AppMessage::Modifiers(Modifiers::CTRL));
        let _ = app.update(lib(Message::SoonType(MediaType::Shows)));
        let _ = app.update(AppMessage::Modifiers(Modifiers::empty()));
        assert_eq!(
            types(&app),
            BTreeSet::from([MediaType::Movies, MediaType::Shows]),
            "Ctrl+click adds"
        );
        let _ = app.update(lib(Message::SoonSlice(Slice::Discover)));
        assert!(types(&app).is_empty(), "each slice has its own chips");
        let _ = app.update(lib(Message::SoonSlice(Slice::Library)));
        assert_eq!(types(&app).len(), 2);
        let _ = app.update(lib(Message::SoonAllTypes));
        assert!(types(&app).is_empty());
    }

    #[test]
    fn slices_keep_their_scroll_and_leaving_discover_closes_the_preview() {
        let mut app = on_page();
        app.library.update(Message::Scrolled(super::super::Scroll {
            offset: 300.0,
            viewport: 500.0,
            content: 3000.0,
        }));
        let _ = app.update(lib(Message::SoonSlice(Slice::Discover)));
        assert_eq!(app.library.scroll.offset, 0.0);
        let _ = app.update(lib(Message::SelectFound("fake:book:king-2027".into())));
        assert!(app.library.found.is_some() && app.library.pane_open());
        let _ = app.update(lib(Message::SoonSlice(Slice::All)));
        assert_eq!(
            app.library.found, None,
            "leaving Discover closes the preview"
        );
        let _ = app.update(lib(Message::SoonSlice(Slice::Library)));
        assert_eq!(
            app.library.scroll.offset, 300.0,
            "Library is where it was left"
        );
        // A second click on the open item closes it; Esc too.
        let _ = app.update(lib(Message::SoonSlice(Slice::Discover)));
        let _ = app.update(lib(Message::SelectFound("fake:book:king-2027".into())));
        let _ = app.update(lib(Message::SelectFound("fake:book:king-2027".into())));
        assert_eq!(app.library.found, None);
        let _ = app.update(lib(Message::SelectFound("fake:book:king-2027".into())));
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.library.found, None);
        assert!(app.library.soon, "Esc closes the preview, not the page");
    }

    #[test]
    fn the_keys_and_the_wheel_step_the_slices_only_on_the_page() {
        let mut app = on_page();
        let key = |app: &mut App, named: Named, modifiers: Modifiers| {
            let _ = app.update(AppMessage::KeyPressed(Key::Named(named), modifiers));
        };
        key(&mut app, Named::ArrowRight, Modifiers::SHIFT);
        assert_eq!(app.library.slice, Slice::Discover);
        key(&mut app, Named::ArrowRight, Modifiers::SHIFT);
        key(&mut app, Named::ArrowRight, Modifiers::SHIFT);
        assert_eq!(app.library.slice, Slice::Library, "wraps forward");
        key(&mut app, Named::ArrowLeft, Modifiers::SHIFT);
        assert_eq!(app.library.slice, Slice::All, "wraps back");
        // The wheel anywhere on the page: Shift, or sideways; plain
        // scrolling leaves the slice alone.
        let start = Instant::now();
        let wheel = |app: &mut App, x: f32, y: f32, after: u64| {
            let delta = ScrollDelta::Lines { x, y };
            let _ = app.update_at(
                lib(Message::SoonWheel(delta)),
                start + Duration::from_millis(after),
            );
        };
        wheel(&mut app, 0.0, -1.0, 0);
        assert_eq!(app.library.slice, Slice::All, "plain scrolling");
        let _ = app.update(AppMessage::Modifiers(Modifiers::SHIFT));
        wheel(&mut app, 0.0, -1.0, 1000);
        assert_eq!(app.library.slice, Slice::Library);
        wheel(&mut app, 0.0, 1.0, 2000);
        assert_eq!(app.library.slice, Slice::All);
        let _ = app.update(AppMessage::Modifiers(Modifiers::empty()));
        wheel(&mut app, -1.0, 0.0, 3000);
        assert_eq!(app.library.slice, Slice::Library, "sideways");
        wheel(&mut app, 1.0, 0.0, 4000);
        assert_eq!(app.library.slice, Slice::All);
        // Off the page the keys do nothing.
        let _ = app.update(lib(Message::ShowType(Some(MediaType::Movies))));
        key(&mut app, Named::ArrowRight, Modifiers::SHIFT);
        assert!(!app.library.soon && app.library.single() == Some(MediaType::Movies));
        let _ = app.update(lib(Message::ShowSoon));
        assert_eq!(app.library.slice, Slice::All, "kept meanwhile");
    }

    #[test]
    fn the_type_panel_popover_opens_from_see_all_and_opens_the_page_on_the_type() {
        let mut app = on_page();
        let _ = app.update(lib(Message::ShowType(Some(MediaType::Movies))));
        let _ = app.update(lib(Message::GlanceSeeAll(Section::ComingSoon)));
        assert!(app.library.type_pop && !app.library.soon);
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert!(!app.library.type_pop, "Esc closes it");
        let _ = app.update(lib(Message::OpenTypePop));
        let _ = app.update(lib(Message::PickFromTypePop(
            "movies/the-batman-part-ii-2027".into(),
        )));
        assert!(!app.library.type_pop);
        assert_eq!(
            app.library.selected.as_deref(),
            Some("movies/the-batman-part-ii-2027")
        );
        let _ = app.update(lib(Message::OpenTypePop));
        let _ = app.update(lib(Message::OpenSoonForTypes));
        assert!(app.library.soon && !app.library.type_pop);
        assert_eq!(app.library.slice, Slice::Library);
        assert_eq!(
            app.library.soon_types(),
            BTreeSet::from([MediaType::Movies])
        );
        // The in-progress link still narrows the list.
        let _ = app.update(lib(Message::ShowType(Some(MediaType::Shows))));
        let _ = app.update(lib(Message::GlanceSeeAll(Section::InProgress)));
        assert!(!app.library.type_pop && !app.library.soon);
        assert_eq!(app.library.status, Some(EntryStatus::InProgress));
    }

    #[test]
    fn adding_a_found_season_puts_it_on_the_show_and_a_title_opens_the_add_form() {
        let mut app = on_page();
        let visible = app.discover.visible(app.repo.library()).len();
        let _ = app.update(lib(Message::AddFound("shows/severance-2022#3".into())));
        let show = app.repo.library().entry("shows/severance-2022").unwrap();
        let third = show.season_list().iter().find(|s| s.season == 3).unwrap();
        assert_eq!(third.status, Some(EntryStatus::Want));
        assert_eq!(third.released, Released::parse("2027"));
        assert_eq!(app.discover.visible(app.repo.library()).len(), visible - 1);
        assert!(app.dialog.is_none());
        // It is in the Library slice now.
        let rows = Lists::of(&app);
        assert!(
            rows.library
                .iter()
                .any(|r| r.key() == "shows/severance-2022#3")
        );

        let _ = app.update(lib(Message::AddFound("fake:movie:hunt-for-gollum".into())));
        let Some(entry::Dialog::Form(form)) = &app.dialog else {
            panic!("the Add dialog");
        };
        assert_eq!(form.title, "The Lord of the Rings: The Hunt for Gollum");
        assert_eq!(form.media, MediaType::Movies);
        assert_eq!(form.creator, "Andy Serkis");
        assert_eq!(form.released, "2027-12-17");
        assert_eq!(form.source.as_deref(), Some("fake:movie:hunt-for-gollum"));
        assert_eq!(form.status, EntryStatus::Want);
        assert!(form.description.text().starts_with("Andy Serkis"));
    }

    #[test]
    fn not_interested_hides_an_item_and_restore_brings_it_back() {
        let mut app = on_page();
        let _ = app.update(lib(Message::SoonSlice(Slice::Discover)));
        let visible = app.discover.visible(app.repo.library()).len();
        let _ = app.update(lib(Message::SelectFound("fake:book:king-2027".into())));
        let _ = app.update(lib(Message::HideFound("fake:book:king-2027".into())));
        assert_eq!(app.library.found, None);
        assert!(app.discover.hidden.contains("fake:book:king-2027"));
        assert_eq!(app.discover.visible(app.repo.library()).len(), visible - 1);
        assert_eq!(app.discover.hidden_items(app.repo.library()).len(), 1);
        let _ = app.update(lib(Message::ShowHidden));
        assert!(app.library.show_hidden);
        let _ = app.update(lib(Message::RestoreFound("fake:book:king-2027".into())));
        assert_eq!(app.discover.visible(app.repo.library()).len(), visible);
        assert!(app.discover.hidden.is_empty());
    }

    #[test]
    fn the_list_slides_at_plasmas_speed_and_swaps_at_instant() {
        use std::time::{Duration, Instant};

        use crate::animation::Speed;

        let mut app = on_page();
        assert_eq!(app.library.slide, None, "instant in the tests");
        app.speed = Speed::factor(0.5);
        let start = Instant::now();
        let _ = app.update_at(lib(Message::SoonSlice(Slice::Discover)), start);
        let moving = app.library.slide.clone().expect("a slide");
        assert_eq!(moving.position(start), 0.0, "from Library");
        assert_eq!(moving.anchor, 1.0);
        assert!(moving.forward());
        // KWin's desktop-switch spring at Plasma's half speed, slowed by
        // `SLOWER`: past a second to the last pixel.
        assert!(!moving.done(start + Duration::from_millis(900)));
        assert!(moving.done(start + Duration::from_millis(1500)));
        let halfway = moving.position(start + Duration::from_millis(140));
        assert!((0.3..0.7).contains(&halfway), "{halfway}");
        // A switch on the way starts where the list is, at its speed.
        let mid = start + Duration::from_millis(140);
        let _ = app.update_at(lib(Message::SoonSlice(Slice::All)), mid);
        let again = app.library.slide.clone().unwrap();
        assert!((again.position(mid) - halfway).abs() < 1e-4);
        assert_eq!(again.anchor, 2.0, "on along the row, not back round");
        assert!(again.position(mid + Duration::from_millis(16)) > halfway);
        let _ = app.update_at(AppMessage::Frame(start), mid + Duration::from_secs(3));
        assert_eq!(app.library.slide, None, "settled on a frame after it");
        // Back is the other way; All to Library wraps forward.
        let _ = app.update_at(lib(Message::SoonSlice(Slice::Discover)), start);
        assert!(!app.library.slide.as_ref().unwrap().forward());
        app.library.slide = None;
        let _ = app.update_at(lib(Message::SoonSlice(Slice::Library)), start);
        app.library.slide = None;
        let _ = app.update_at(lib(Message::SoonSlice(Slice::All)), start);
        assert!(!app.library.slide.as_ref().unwrap().forward());
        app.library.slide = None;
        let _ = app.update_at(lib(Message::SoonStep(1)), start);
        assert_eq!(app.library.slice, Slice::Library);
        assert!(app.library.slide.as_ref().unwrap().forward());
        // The same slice again, and type chips, never slide.
        app.library.slide = None;
        let _ = app.update_at(lib(Message::SoonSlice(Slice::Library)), start);
        let _ = app.update_at(lib(Message::SoonType(MediaType::Games)), start);
        assert_eq!(app.library.slide, None);
    }

    #[test]
    fn the_page_renders_its_rows_with_their_marks() {
        let mut app = on_page();
        let _ = app.update(lib(Message::SoonSlice(Slice::All)));
        let _ = app.update(lib(Message::SetView(View::List)));
        {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            assert!(ui.find("Coming soon").is_ok());
            assert!(ui.find("Discover").is_ok());
            assert!(ui.find("Grand Theft Auto VI").is_ok());
            assert!(ui.find("Want").is_ok(), "a library row in All");
            assert!(ui.find("+ Add").is_ok(), "a Discover row");
            assert!(ui.find("NEXT UP").is_ok());
            assert!(ui.find("BY TYPE").is_ok());
            assert!(ui.find("Add").is_err(), "no Add button in the header");
        }
        // The Library slice has no marks.
        let _ = app.update(lib(Message::SoonSlice(Slice::Library)));
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        assert!(ui.find("+ Add").is_err());
        assert!(ui.find("November").is_ok(), "this year's month alone");
    }

    /// The list measured into `app` as a real frame would.
    fn measured(app: &mut App) {
        measured_at(app, crate::app::WINDOW_SIZE);
    }

    fn measured_at(app: &mut App, size: iced::Size) {
        let messages: Vec<AppMessage> = {
            let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
            let redraw = iced::window::Event::RedrawRequested(std::time::Instant::now());
            let _ = ui.simulate([iced::Event::Window(redraw)]);
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
    }

    #[test]
    fn the_grid_has_no_month_headers_or_tags_and_the_other_views_keep_their_headers() {
        let mut app = on_page();
        let _ = app.update(lib(Message::SoonSlice(Slice::All)));
        let _ = app.update(lib(Message::SetView(View::Grid)));
        measured(&mut app);
        {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            assert!(ui.find("November").is_err(), "no month header");
            assert!(ui.find("NOV").is_err(), "no month tag");
            assert!(ui.find("Nov").is_ok(), "the rail");
            assert!(ui.find("+ Add").is_ok(), "the badges stay");
            assert!(ui.find("UPCOMING").is_err(), "no type on a tile");
        }
        for view in [View::List, View::Text] {
            let _ = app.update(lib(Message::SetView(view)));
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            assert!(ui.find("November").is_ok(), "{view:?} keeps its header");
        }
    }

    #[test]
    fn a_tile_shows_no_type() {
        let mut app = on_page();
        let _ = app.update(lib(Message::SetView(View::Grid)));
        let _ = app.update(lib(Message::SoonType(MediaType::Games)));
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        assert!(ui.find("Grand Theft Auto VI").is_ok());
        assert!(ui.find("GAME").is_err(), "no type on the tile");
    }

    #[test]
    fn the_grids_rail_finds_the_rows_the_tiles_are_in() {
        let mut app = on_page();
        let _ = app.update(lib(Message::SoonSlice(Slice::All)));
        let _ = app.update(lib(Message::SetView(View::Grid)));
        measured(&mut app);
        let state = &app.library;
        let lists = Lists::of(&app);
        let groups = page_groups(state, &lists);
        assert!(groups.len() >= 2);
        let names = groups.iter().map(|g| g.rail(app.today)).collect();
        let room = rail::Sections::new(names, false).room();
        let rows = grid_rows(state.list_width, room);
        let keys: Vec<String> = groups
            .iter()
            .flat_map(|g| g.rows.iter().map(SoonRow::key))
            .collect();
        assert!(rows.columns > 1 && keys.len() > rows.columns);
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        let mut tile_at = |key: &str| {
            ui.find(iced::advanced::widget::Id::from(row_key(key)))
                .expect("the tile")
                .bounds()
        };
        let first = tile_at(&keys[0]);
        assert!((first.height - rows.cell).abs() < 0.01);
        for (index, key) in keys.iter().enumerate() {
            let top = tile_at(key).y - first.y;
            assert!((top - rows.top(index)).abs() < 0.01, "tile {index}");
        }
    }

    #[test]
    fn a_click_on_the_rail_jumps_to_the_row_of_the_groups_first_tile() {
        use iced::mouse::{Button, Event as Mouse};

        // Narrow enough that the group's first tile is past the first row.
        let size = iced::Size::new(1100.0, 800.0);
        let mut app = on_page();
        let _ = app.update(lib(Message::SoonSlice(Slice::All)));
        let _ = app.update(lib(Message::SetView(View::Grid)));
        measured_at(&mut app, size);
        let state = &app.library;
        let lists = Lists::of(&app);
        let groups = page_groups(state, &lists);
        let names: Vec<String> = groups.iter().map(|g| g.rail(app.today)).collect();
        let room = rail::Sections::new(names.clone(), false).room();
        let rows = grid_rows(state.list_width, room);
        // A month, since a year or TBD can also be the date under a tile.
        let group = names
            .iter()
            .rposition(|name| *name != name.to_uppercase())
            .expect("a month");
        let first: usize = groups[..group].iter().map(|g| g.rows.len()).sum();
        assert!(first >= rows.columns, "{first} of {}", rows.columns);
        let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
        let label = ui
            .find(names[group].as_str())
            .expect("on the rail")
            .bounds();
        ui.point_at(label.center());
        let _ = ui.simulate([iced::Event::Mouse(Mouse::ButtonPressed(Button::Left))]);
        let jump = ui.into_messages().find_map(|message| match message {
            AppMessage::Library(Message::JumpToSection { label, offset, .. }) => {
                Some((label, offset))
            }
            _ => None,
        });
        assert_eq!(jump, Some((names[group].clone(), rows.top(first))));
    }

    #[test]
    fn the_arrow_keys_walk_the_library_rows_once_each() {
        let mut app = on_page();
        let _ = app.update(lib(Message::SoonSlice(Slice::All)));
        let order = app.library.display_order(app.repo.library(), app.today);
        assert_eq!(
            order,
            [
                "games/grand-theft-auto-vi-2026",
                "movies/the-batman-part-ii-2027"
            ]
        );
        let _ = app.update(AppMessage::Key(KeyPress::Down));
        assert_eq!(app.library.selected.as_deref(), Some(order[0].as_str()));
        let _ = app.update(lib(Message::SoonSlice(Slice::Discover)));
        assert!(
            app.library
                .display_order(app.repo.library(), app.today)
                .is_empty()
        );
    }

    #[test]
    fn the_shortcuts_are_shift_and_the_arrows() {
        use crate::keymap::{Action, Combo, Keymap};
        let map = Keymap::default();
        assert_eq!(map.combo(Action::SoonPrev).to_string(), "Shift+←");
        assert_eq!(map.combo(Action::SoonNext).to_string(), "Shift+→");
        let combo = Combo::of(&Key::Named(Named::ArrowLeft), Modifiers::SHIFT).unwrap();
        assert_eq!(map.action_for(&combo), Some(Action::SoonPrev));
    }

    #[test]
    fn alt_arrows_stay_back_and_forward_on_the_page() {
        let mut app = on_page();
        let _ = app.update(lib(Message::ShowType(Some(MediaType::Movies))));
        let _ = app.update(lib(Message::ShowSoon));
        let _ = app.update(AppMessage::KeyPressed(
            Key::Named(Named::ArrowLeft),
            Modifiers::ALT,
        ));
        assert!(!app.library.soon, "Alt+← leaves the page");
    }

    /// Shift and an arrow reach the app through the page's widgets: none
    /// of them captures the press, so the keyboard subscription hears it.
    #[test]
    fn no_widget_on_the_page_takes_shift_and_an_arrow() {
        use iced::advanced::clipboard::Null;
        use iced::advanced::renderer::Headless;
        use iced::event::Status;
        use iced::keyboard::{self, Location};
        use iced::mouse;
        use iced_test::runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced_test::futures::futures::executor::block_on(
            <iced::Renderer as Headless>::new(iced::Font::default(), 14.into(), None),
        )
        .expect("a headless renderer");
        let size = crate::app::WINDOW_SIZE;
        let app = on_page();
        let mut ui = UserInterface::build(app.view(), size, Cache::default(), &mut renderer);
        let center = iced::Point::new(size.width / 2.0, size.height / 2.0);
        let press = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(Named::ArrowRight),
            modified_key: Key::Named(Named::ArrowRight),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::ArrowRight),
            location: Location::Standard,
            modifiers: Modifiers::SHIFT,
            text: None,
            repeat: false,
        });
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            &[
                iced::Event::Mouse(mouse::Event::CursorMoved { position: center }),
                iced::Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::SHIFT)),
                press,
            ],
            mouse::Cursor::Available(center),
            &mut renderer,
            &mut Null,
            &mut messages,
        );
        assert_eq!(statuses[2], Status::Ignored, "{messages:?}");
    }

    #[test]
    fn a_popover_keeps_the_slice() {
        let mut app = on_page();
        app.library.details_rating_open = true;
        let _ = app.update(AppMessage::KeyPressed(
            Key::Named(Named::ArrowRight),
            Modifiers::SHIFT,
        ));
        assert_eq!(app.library.slice, Slice::Library);
    }

    #[test]
    fn one_wheel_gesture_is_one_step() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let side = |x| ScrollDelta::Lines { x, y: 0.0 };
        let mut gesture = WheelGesture::default();
        assert_eq!(gesture.feed(side(-1.0), false, at(0)), Some(1));
        assert_eq!(gesture.feed(side(-1.0), false, at(60)), None, "still going");
        assert_eq!(gesture.feed(side(-1.0), false, at(120)), None);
        assert_eq!(
            gesture.feed(side(1.0), false, at(400)),
            Some(-1),
            "a new one"
        );
        // A high-resolution wheel or a touchpad adds up to a notch.
        let mut gesture = WheelGesture::default();
        let pixels = |x, y| ScrollDelta::Pixels { x, y };
        assert_eq!(gesture.feed(pixels(-10.0, 0.0), false, at(0)), None);
        assert_eq!(gesture.feed(pixels(-10.0, 0.0), false, at(10)), None);
        assert_eq!(gesture.feed(pixels(-10.0, 0.0), false, at(20)), Some(1));
        // A vertical scroll that drifts sideways never gets there.
        let mut gesture = WheelGesture::default();
        for tick in 0..50 {
            let drift = pixels(-6.0, if tick % 2 == 0 { -40.0 } else { -2.0 });
            let step = gesture.feed(drift, false, at(tick * 10));
            assert_eq!(step, None, "tick {tick}");
        }
        // With Shift the vertical wheel counts.
        let mut gesture = WheelGesture::default();
        assert_eq!(gesture.feed(pixels(0.0, -30.0), true, at(0)), Some(1));
    }

    #[test]
    fn slices_step_and_wrap_both_ways() {
        assert_eq!(Slice::Library.step(1), Slice::Discover);
        assert_eq!(Slice::All.step(1), Slice::Library, "wraps forward");
        assert_eq!(Slice::Library.step(-1), Slice::All, "wraps back");
        assert!(Slice::Library.forward_to(Slice::Discover));
        assert!(Slice::All.forward_to(Slice::Library));
        assert!(!Slice::Library.forward_to(Slice::All));
        assert!(!Slice::Discover.forward_to(Slice::Library));
    }

    #[test]
    fn since_reads_as_a_weekday_within_the_week() {
        let today = Date::parse("2026-11-07").unwrap();
        assert_eq!(since_label(today, today), "today");
        assert_eq!(since_label(today.add_days(-1), today), "yesterday");
        assert_eq!(since_label(today.add_days(-4), today), "Tuesday");
        assert_eq!(since_label(today.add_days(-9), today), "Oct 29");
    }
}
