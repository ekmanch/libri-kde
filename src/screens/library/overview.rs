//! All with nothing narrowing it: a "now and next" overview. In the text
//! and list views two columns of short lists; in the grid, cover shelves
//! with a side column. Every list has a "See all" that opens the full,
//! filtered list.
//!
//! With details open, the grid's shelves scroll sideways instead of
//! squeezing the side column: the same seven covers, a fade and a round
//! button at an edge with more to see, the scrollbar hidden (Shift+wheel
//! and a touchpad scroll them; a plain wheel scrolls the page), and the
//! selected cover always in view.

use iced::gradient::Linear;
use iced::widget::scrollable::{self, AbsoluteOffset, Direction, Scrollbar};
use iced::widget::{
    Column, Row, Space, button, column, container, operation, progress_bar, row, space, stack, text,
};
use iced::{
    Background, Border, Center, Color, Degrees, Element, Fill, Gradient, Padding, Shadow, Task,
    Vector,
};

use super::{Message, View};
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, ActivePlan, Overview, Section};
use crate::model::{Date, Entry, Released};
use crate::screens::{Screen, entry, shell};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// The shelves in the grid, top to bottom, and how many covers each shows.
const SHELVES: [Section; 3] = [Section::ComingSoon, Section::TopRated, Section::InProgress];
const SHELF_COVERS: usize = 7;
/// A shelf cover's width and the gap between covers; the cover is 150 tall.
const COVER_WIDTH: f32 = 100.0;
const COVER_GAP: f32 = 14.0;
const COVER_HEIGHT: f32 = 150.0;
/// The overview's padding, the gap before the side column, and the side
/// column's width while details are open.
const PADDING: f32 = 24.0;
const COLUMN_GAP: f32 = 40.0;
const SIDE_OPEN: f32 = 340.0;
/// The fades at a shelf's edges, the room kept for them when a cover is
/// scrolled into view, and how much of a row a nudge leaves in view.
const FADE_LEFT: f32 = 70.0;
const FADE_RIGHT: f32 = 90.0;
const NUDGE_KEEP: f32 = 80.0;
/// The round ‹ and › buttons.
const ARROW: f32 = 34.0;

/// How far a shelf is scrolled and how wide it shows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShelfScroll {
    pub offset: f32,
    pub viewport: f32,
}

fn shelf_id(section: Section) -> &'static str {
    match section {
        Section::ComingSoon => "shelf-coming-soon",
        Section::TopRated => "shelf-top-rated",
        Section::InProgress => "shelf-in-progress",
        Section::RecentlyAdded => "shelf-recently-added",
    }
}

/// The width of `count` covers in a row.
fn row_width(count: usize) -> f32 {
    let count = count.min(SHELF_COVERS) as f32;
    (count * COVER_WIDTH + (count - 1.0).max(0.0) * COVER_GAP).max(0.0)
}

/// Whether the grid's shelves scroll: Home in the grid with details open.
fn scrolling(app: &App) -> bool {
    let state = &app.library;
    state.is_overview()
        && state.page.is_none()
        && state.view_mode() == View::Grid
        && state
            .selected
            .as_deref()
            .is_some_and(|id| app.repo.library().entry(id).is_some())
}

/// How a shelf is scrolled: as it last said, else at the start and as wide
/// as the layout leaves it.
fn scroll_of(app: &App, section: Section) -> ShelfScroll {
    app.library
        .shelves
        .get(&section)
        .copied()
        .unwrap_or_else(|| ShelfScroll {
            offset: 0.0,
            viewport: (shell::content_width(app, entry::details::WIDTH)
                - 2.0 * PADDING
                - COLUMN_GAP
                - SIDE_OPEN)
                .max(COVER_WIDTH),
        })
}

/// Scrolls `section`'s shelf to `offset` and remembers it.
fn scroll_shelf(app: &mut App, section: Section, offset: f32) -> Task<AppMessage> {
    let mut scroll = scroll_of(app, section);
    scroll.offset = offset;
    app.library.shelves.insert(section, scroll);
    operation::scroll_to(
        shelf_id(section),
        AbsoluteOffset {
            x: Some(offset),
            y: None,
        },
    )
}

/// After the selection changed: the shelves holding the selected entry
/// scroll it into view, clear of the fades; the others stay. With details
/// closed every shelf goes back to the start.
pub fn follow_selection(app: &mut App) -> Task<AppMessage> {
    let state = &app.library;
    if !state.is_overview() || state.page.is_some() || state.view_mode() != View::Grid {
        return Task::none();
    }
    if !scrolling(app) {
        app.library.shelves.clear();
        return Task::batch(SHELVES.map(|section| {
            operation::scroll_to(
                shelf_id(section),
                AbsoluteOffset {
                    x: Some(0.0),
                    y: None,
                },
            )
        }));
    }
    let Some(selected) = app.library.selected.clone() else {
        return Task::none();
    };
    let overview = Overview::of(app.repo.library(), app.today);
    let mut moves = Vec::new();
    for section in SHELVES {
        let list = overview.list(section);
        let Some(index) = list
            .iter()
            .take(SHELF_COVERS)
            .position(|e| e.id == selected)
        else {
            continue;
        };
        let content = row_width(list.len());
        let scroll = scroll_of(app, section);
        if let Some(offset) = reveal(index, content, scroll) {
            moves.push((section, offset));
        }
    }
    Task::batch(
        moves
            .into_iter()
            .map(|(section, offset)| scroll_shelf(app, section, offset)),
    )
}

/// The offset that brings cover `index` into view clear of the fades, or
/// `None` when it is in view already.
fn reveal(index: usize, content: f32, scroll: ShelfScroll) -> Option<f32> {
    let x = index as f32 * (COVER_WIDTH + COVER_GAP);
    let max = (content - scroll.viewport).max(0.0);
    let offset = if x < scroll.offset + FADE_LEFT {
        (x - FADE_LEFT).max(0.0)
    } else if x + COVER_WIDTH > scroll.offset + scroll.viewport - FADE_RIGHT {
        x + COVER_WIDTH - scroll.viewport + FADE_RIGHT
    } else {
        return None;
    };
    let offset = offset.clamp(0.0, max);
    ((offset - scroll.offset).abs() > 0.5).then_some(offset)
}

/// ‹ or ›: about a row's width, less a cover's worth kept in view.
pub fn nudge(app: &mut App, section: Section, direction: i8) -> Task<AppMessage> {
    let overview = Overview::of(app.repo.library(), app.today);
    let content = row_width(overview.list(section).len());
    let scroll = scroll_of(app, section);
    let step = (scroll.viewport - NUDGE_KEEP).max(COVER_WIDTH);
    let max = (content - scroll.viewport).max(0.0);
    let offset = (scroll.offset + f32::from(direction) * step).clamp(0.0, max);
    scroll_shelf(app, section, offset)
}

/// The covers the overview shows: the shelves' first seven in the grid,
/// the lists' rows in the list view, nothing in the text view.
pub fn wanted(app: &App, view: View, wants: &mut crate::artwork::Wants) {
    let overview = Overview::of(app.repo.library(), app.today);
    match view {
        View::Grid => {
            for section in [Section::ComingSoon, Section::TopRated, Section::InProgress] {
                wants
                    .covers
                    .extend(overview.list(section).iter().take(7).map(|e| e.id.clone()));
            }
        }
        View::List => {
            for section in [
                Section::InProgress,
                Section::TopRated,
                Section::ComingSoon,
                Section::RecentlyAdded,
            ] {
                let cap = if section == Section::TopRated { 3 } else { 4 };
                wants.covers.extend(
                    overview
                        .list(section)
                        .iter()
                        .take(cap)
                        .map(|e| e.id.clone()),
                );
            }
        }
        View::Text => {}
    }
}

pub fn body(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let overview = Overview::of(app.repo.library(), app.today);
    let content: Element<'_, AppMessage> = match app.library.view_mode() {
        View::Grid => shelves(app, &overview, &tokens),
        view => columns(app, &overview, view, &tokens),
    };
    ui::scroll(container(content).padding(Padding::new(24.0).top(10)))
        .height(Fill)
        .into()
}

/// Text and list views: the plan, In progress and Top rated on the left,
/// Coming soon and Recently added on the right.
fn columns<'a>(
    app: &'a App,
    overview: &Overview<'a>,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let text_view = view == View::Text;
    let cap = |section| match (section, text_view) {
        (_, true) => 6,
        (Section::TopRated, false) => 3,
        (_, false) => 4,
    };
    let style = if text_view {
        RowStyle::Dot
    } else {
        RowStyle::Cover
    };
    let list = |section: Section| list_section(app, overview, section, cap(section), style, tokens);

    let mut left = Column::new().spacing(28).width(Fill);
    if let Some(plan) = &overview.plan {
        left = left.push(plan_card(plan, tokens));
    }
    let left = left.extend(
        [list(Section::InProgress), list(Section::TopRated)]
            .into_iter()
            .flatten(),
    );
    let right = Column::new().spacing(28).width(Fill).extend(
        [list(Section::ComingSoon), list(Section::RecentlyAdded)]
            .into_iter()
            .flatten(),
    );
    row![left, right].spacing(36).into()
}

/// Grid view: cover shelves for Coming soon, Top rated and In progress,
/// and a side column with the plan and Recently added.
fn shelves<'a>(app: &'a App, overview: &Overview<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
    let open = scrolling(app);
    let mut shelves = Column::new().spacing(26).extend(
        SHELVES
            .into_iter()
            .filter_map(|section| shelf(app, overview, section, open, tokens)),
    );
    if open {
        shelves = shelves.width(Fill);
    }
    let mut side = Column::new()
        .spacing(28)
        .width(Fill)
        .padding(Padding::new(0.0).top(34));
    if let Some(plan) = &overview.plan {
        side = side.push(plan_card(plan, tokens));
    }
    side = side.extend(list_section(
        app,
        overview,
        Section::RecentlyAdded,
        12,
        RowStyle::Dot,
        tokens,
    ));
    // The side column keeps its width while details are open; the shelves
    // scroll instead.
    let side = if open {
        container(side).width(SIDE_OPEN)
    } else {
        container(side).max_width(560)
    };
    row![shelves, side].spacing(COLUMN_GAP).into()
}

/// One shelf: its header (the title, the count and a chevron, opening the
/// full list), then up to seven covers; a sideways scroll with its edge
/// cues while `open`. The tree keeps its shape either way, so the scroll
/// position survives.
fn shelf<'a>(
    app: &'a App,
    overview: &Overview<'a>,
    section: Section,
    open: bool,
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    let list = overview.list(section);
    if list.is_empty() {
        return None;
    }
    let header = button(
        row![
            text(section.title().to_uppercase())
                .size(theme::size::SECTION)
                .font(font::BOLD),
            ui::mono(list.len().to_string(), 12.0, tokens.sub),
            ui::icon(Icon::ChevronRight, 12.0, tokens.sub),
        ]
        .spacing(8)
        .align_y(Center)
        .height(Fill),
    )
    .height(24)
    .padding(0)
    .on_press(msg(Message::SeeAll(section)))
    .style(text_button(tokens.sub))
    .focusable(6.0, tokens);
    let selected = app.library.selected.as_deref();
    let covers = Row::with_children(list.iter().take(SHELF_COVERS).map(|entry| {
        let dimmed = selected.is_some_and(|id| id != entry.id);
        shelf_cover(app, entry, meta(entry, section, app.today), dimmed, tokens)
    }))
    .spacing(COVER_GAP);
    let row =
        scrollable::Scrollable::with_direction(covers, Direction::Horizontal(Scrollbar::hidden()))
            .id(shelf_id(section))
            .width(if open { Fill } else { iced::Shrink })
            .on_scroll(move |viewport| {
                msg(Message::ShelfScrolled(
                    section,
                    ShelfScroll {
                        offset: viewport.absolute_offset().x,
                        viewport: viewport.bounds().width,
                    },
                ))
            });

    // The cues show only where there is more to see.
    let scroll = scroll_of(app, section);
    let content = row_width(list.len());
    let more_before = open && scroll.offset > 2.0;
    let more_after = open && scroll.offset < content - scroll.viewport - 2.0;
    let back = tokens.content;
    let fade = |show: bool, width: f32, to_left: bool| -> Element<'a, AppMessage> {
        if !show {
            return Space::new().into();
        }
        let clear = Color { a: 0.0, ..back };
        let (from, to) = if to_left {
            (back, clear)
        } else {
            (clear, back)
        };
        let gradient = Linear::new(Degrees(90.0))
            .add_stop(0.0, from)
            .add_stop(1.0, to);
        let face = container(space())
            .width(width)
            .height(Fill)
            .style(move |_| container::background(Gradient::Linear(gradient)));
        let place = container(face).width(Fill).height(Fill);
        if to_left {
            place
        } else {
            place.align_x(iced::Right)
        }
        .into()
    };
    let arrow = |show: bool, before: bool| -> Element<'a, AppMessage> {
        if !show {
            return Space::new().into();
        }
        let which = if before {
            Icon::ChevronLeft
        } else {
            Icon::ChevronRight
        };
        let t = *tokens;
        let face = button(container(ui::icon(which, 16.0, t.text)).center(Fill))
            .width(ARROW)
            .height(ARROW)
            .padding(0)
            .on_press(msg(Message::NudgeShelf(
                section,
                if before { -1 } else { 1 },
            )))
            .style(move |_, status| button::Style {
                background: Some(Background::Color(
                    if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        t.popover
                    },
                )),
                border: Border::default()
                    .rounded(ARROW / 2.0)
                    .width(1)
                    .color(t.line),
                shadow: Shadow {
                    color: t.shadow,
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 14.0,
                },
                text_color: t.text,
                ..button::Style::default()
            })
            .focusable(ARROW / 2.0, tokens);
        let place = container(face).width(Fill).padding(
            Padding::new(0.0)
                .top((COVER_HEIGHT - ARROW) / 2.0)
                .left(4)
                .right(4),
        );
        if before {
            place
        } else {
            place.align_x(iced::Right)
        }
        .into()
    };
    let covers = stack![
        row,
        fade(more_before, FADE_LEFT, true),
        fade(more_after, FADE_RIGHT, false),
        arrow(more_before, true),
        arrow(more_after, false),
    ];
    Some(column![header, covers].spacing(10).into())
}

/// A 100 × 150 cover with the title on it, the detail under it.
fn shelf_cover<'a>(
    app: &'a App,
    entry: &'a Entry,
    meta: String,
    dimmed: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let tint = theme::cover_color(&entry.id);
    let cover = ui::titled_cover(
        app.artwork.cover(entry),
        tint,
        &entry.title,
        100.0,
        150.0,
        14.0,
    );
    let content = column![
        cover,
        ui::mono(meta, 12.0, tokens.sub).wrapping(text::Wrapping::None)
    ]
    .spacing(6)
    .width(100);
    let veil = tokens.content.scale_alpha(if dimmed { 0.4 } else { 0.0 });
    let content = iced::widget::stack![
        content,
        container(space())
            .width(Fill)
            .height(Fill)
            .style(move |_| container::background(veil))
    ];
    button(content)
        .padding(0)
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(text_button(tokens.text))
        .focusable(8.0, tokens)
}

/// How an overview row starts: a type dot (text view) or a small cover
/// with the type under the title (list view).
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowStyle {
    Dot,
    Cover,
}

/// A titled list: the title in gray caps and "See all N" on the right,
/// then up to `cap` rows. Nothing when the list is empty.
fn list_section<'a>(
    app: &'a App,
    overview: &Overview<'a>,
    section: Section,
    cap: usize,
    style: RowStyle,
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    let list = overview.list(section);
    if list.is_empty() {
        return None;
    }
    let see_all = button(
        row![
            text(format!("See all {}", list.len()))
                .size(13)
                .font(font::SEMIBOLD),
            ui::icon(Icon::ChevronRight, 12.0, tokens.ink),
        ]
        .spacing(4)
        .align_y(Center)
        .height(Fill),
    )
    .height(24)
    .padding([0, 2])
    .on_press(msg(Message::SeeAll(section)))
    .style(text_button(tokens.ink))
    .focusable(6.0, tokens);
    let header = row![
        ui::section_header(section.title(), tokens),
        space::horizontal(),
        see_all
    ]
    .align_y(Center)
    .height(24);
    let selected = app.library.selected.as_deref();
    let rows = Column::with_children(list.iter().take(cap).map(|entry| {
        let is_selected = selected == Some(entry.id.as_str());
        overview_row(
            app,
            entry,
            meta(entry, section, app.today),
            style,
            is_selected,
            tokens,
        )
    }));
    Some(column![header, rows].spacing(4).into())
}

fn overview_row<'a>(
    app: &'a App,
    entry: &'a Entry,
    meta: String,
    style: RowStyle,
    selected: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let color = tokens.type_color(entry.media);
    let title = text(&entry.title).size(14).wrapping(text::Wrapping::None);
    let mut content = Row::new().spacing(12).align_y(Center);
    content = match style {
        RowStyle::Dot => content
            .push(ui::dot(color, 8.0, 3.0))
            .push(container(title).width(Fill).clip(true)),
        RowStyle::Cover => {
            let tint = theme::cover_color(&entry.id);
            content
                .push(ui::plain_cover(
                    app.artwork.cover(entry),
                    tint,
                    30.0,
                    45.0,
                    4.0,
                ))
                .push(
                    column![title, text(browse::kind_label(entry)).size(12).color(color)]
                        .spacing(2)
                        .width(Fill)
                        .clip(true),
                )
        }
    };
    content = content.push(ui::mono(meta, 12.0, tokens.sub));
    let height = if style == RowStyle::Dot { 38.0 } else { 56.0 };
    let line = tokens.line;
    let row_button = button(content.height(Fill))
        .width(Fill)
        .height(height)
        .padding([4, 8])
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(ui::row_style(tokens, selected, 0.0))
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

/// The active plan: its name and year, "n of m done", a progress bar in
/// the regular accent (never the seasonal look), and what is next.
fn plan_card<'a>(active: &ActivePlan<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
    let open = button(
        row![
            text("Open").size(13).font(font::SEMIBOLD),
            ui::icon(Icon::ChevronRight, 12.0, tokens.ink)
        ]
        .spacing(4)
        .align_y(Center),
    )
    .padding(0)
    .on_press(AppMessage::Show(Screen::Plans))
    .style(text_button(tokens.ink))
    .focusable(6.0, tokens);
    let year = active.plan.year.map(|y| y.to_string()).unwrap_or_default();
    let title = row![
        text(&active.plan.name).size(20).font(font::EXTRABOLD),
        text(year).size(14).color(tokens.sub),
        space::horizontal(),
        text(format!("{} of {} done", active.done, active.total))
            .size(13.5)
            .font(font::SEMIBOLD),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Bottom);
    let (line, accent) = (tokens.line, tokens.accent);
    let bar = progress_bar(0.0..=active.total.max(1) as f32, active.done as f32)
        .girth(5)
        .style(move |_| progress_bar::Style {
            background: Background::Color(line),
            bar: Background::Color(accent),
            border: Border::default().rounded(3),
        });
    let mut next = Vec::new();
    if !active.next_up.is_empty() {
        let titles: Vec<&str> = active.next_up.iter().map(|e| e.title.as_str()).collect();
        next.push(format!("Next up: {}", titles.join(", ")));
    }
    match active.days_left {
        Some(0) => next.push("last day".into()),
        Some(1) => next.push("1 day left".into()),
        Some(n) => next.push(format!("{n} days left")),
        None => {}
    }
    let raised = tokens.raised;
    container(
        column![
            row![
                ui::section_header("Active plan", tokens),
                space::horizontal(),
                open
            ]
            .align_y(Center)
            .height(18),
            title,
            bar,
            text(next.join(" · ")).size(12.5).color(tokens.sub),
        ]
        .spacing(9),
    )
    .width(Fill)
    .padding([14, 16])
    .style(move |_| container::background(raised).border(Border::default().rounded(12)))
    .into()
}

/// A borderless text button in one color: "See all", "Open", shelf
/// headers.
fn text_button(color: Color) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, _| button::Style {
        text_color: color,
        ..button::Style::default()
    }
}

/// The detail on the right of a row or under a cover: the year, the
/// release day, the day it was added, or the rating.
fn meta(entry: &Entry, section: Section, today: Date) -> String {
    match section {
        Section::InProgress => entry.year().map(|y| y.to_string()).unwrap_or_default(),
        Section::ComingSoon => entry
            .released
            .map(|r| short_release(r, today))
            .unwrap_or_default(),
        Section::RecentlyAdded => short_day(entry.added, today),
        Section::TopRated => entry.rating.map(|r| format!("{r}/10")).unwrap_or_default(),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// "Oct 3" this year, "Oct 2025" another year.
fn short_day(date: Date, today: Date) -> String {
    let month = MONTHS[usize::from(date.month) - 1];
    if date.year == today.year {
        format!("{month} {}", date.day)
    } else {
        format!("{month} {}", date.year)
    }
}

/// A release at its own precision: "Nov 19", "Oct 2027" or "2027".
fn short_release(released: Released, today: Date) -> String {
    match (released.month, released.day) {
        (Some(month), Some(day)) => short_day(
            Date {
                year: released.year,
                month,
                day,
            },
            today,
        ),
        (Some(month), None) => format!("{} {}", MONTHS[usize::from(month) - 1], released.year),
        _ => released.year.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_short() {
        let today = Date::parse("2026-10-19").unwrap();
        assert_eq!(
            short_day(Date::parse("2026-10-03").unwrap(), today),
            "Oct 3"
        );
        assert_eq!(
            short_day(Date::parse("2025-12-24").unwrap(), today),
            "Dec 2025"
        );
        assert_eq!(
            short_release(Released::parse("2026-11-19").unwrap(), today),
            "Nov 19"
        );
        assert_eq!(
            short_release(Released::parse("2027-10").unwrap(), today),
            "Oct 2027"
        );
        assert_eq!(
            short_release(Released::parse("2027").unwrap(), today),
            "2027"
        );
    }
}

#[cfg(test)]
mod shelf_tests {
    use super::*;
    use crate::screens::library;

    fn app() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app
    }

    #[test]
    fn a_cover_scrolls_into_view_clear_of_the_fades() {
        let wide = ShelfScroll {
            offset: 0.0,
            viewport: 500.0,
        };
        let content = row_width(7);
        assert_eq!(content, 7.0 * 100.0 + 6.0 * 14.0);
        assert_eq!(reveal(0, content, wide), None, "in view");
        // The fifth cover ends at 556, past 500 less the 90 px fade.
        assert_eq!(reveal(4, content, wide), Some(556.0 - 500.0 + 90.0));
        // The last one stops at the end of the row.
        assert_eq!(reveal(6, content, wide), Some(content - 500.0));
        let scrolled = ShelfScroll {
            offset: 284.0,
            ..wide
        };
        assert_eq!(reveal(2, content, scrolled), Some(228.0 - 70.0));
        assert_eq!(reveal(0, content, scrolled), Some(0.0));
    }

    #[test]
    fn shelves_follow_the_selection_and_reset_when_details_close() {
        let mut app = app();
        app.window = iced::Size::new(1280.0, 800.0);
        let overview = Overview::of(app.repo.library(), app.today);
        let top = overview.list(Section::TopRated);
        assert!(top.len() >= 7, "the fixture fills the shelf");
        let last = top[6].id.clone();
        drop(overview);

        let _ = app.update(AppMessage::Library(library::Message::Select(last.clone())));
        assert!(scrolling(&app));
        let offset = app.library.shelves[&Section::TopRated].offset;
        assert!(offset > 0.0, "scrolled to the seventh cover");
        assert!(!app.library.shelves.contains_key(&Section::ComingSoon));

        let _ = app.update(AppMessage::Library(library::Message::NudgeShelf(
            Section::TopRated,
            -1,
        )));
        let shelf = app.library.shelves[&Section::TopRated];
        assert_eq!(
            shelf.offset,
            offset - (shelf.viewport - NUDGE_KEEP),
            "back by a row, less a cover"
        );

        let _ = app.update(AppMessage::Library(library::Message::Select(last)));
        assert!(!scrolling(&app));
        assert!(app.library.shelves.is_empty(), "back to the start");
    }
}
