//! Home: all with nothing narrowing it, a "now and next" overview. In
//! the text and list views two columns of short lists; in the grid,
//! cover shelves with a side column holding the plan and Recently added
//! (rows with covers, as the list view's).
//! Every list has a "See all" that opens the full, filtered list.
//! Recently added ends the right column in every view and has no fixed
//! length: it shows the whole rows that fit down to the footer
//! (`filling_section`), counted again whenever the window is resized.
//!
//! Home has no details panel of its own: a selected title's details take
//! the place of the right column (the side column in the grid, Coming
//! soon and Recently added in the lists), so the layout never changes.
//! The left part scrolls on its own, since the details scroll too; the
//! right column never scrolls.

use iced::widget::{
    Column, Row, button, column, container, opaque, progress_bar, responsive, row, space, text,
};
use iced::{Background, Border, Center, Color, Element, Fill, Padding};

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

/// The shelves in the grid, top to bottom, and how many covers each
/// shows at most.
const SHELVES: [Section; 3] = [Section::ComingSoon, Section::TopRated, Section::InProgress];
const SHELF_COVERS: usize = 7;
/// A shelf cover's width and the gap between covers; the cover is 150 tall.
const COVER_WIDTH: f32 = 100.0;
const COVER_GAP: f32 = 14.0;
/// The overview's padding, the gap before the side column, and the side
/// column's widest.
const PADDING: f32 = 24.0;
const COLUMN_GAP: f32 = 40.0;
const SIDE_MAX: f32 = 560.0;

/// How many covers a shelf shows: the seven of the mockup, fewer when the
/// side column would be narrower than the details it shows on a
/// selection. The same with or without one, so selecting moves nothing.
fn shelf_covers(app: &App) -> usize {
    let width = shell::content_width(app, 0.0) - 2.0 * PADDING - COLUMN_GAP - entry::details::WIDTH;
    (((width + COVER_GAP) / (COVER_WIDTH + COVER_GAP)).floor() as usize).clamp(1, SHELF_COVERS)
}

/// The covers the overview shows: the shelves' first seven and Recently
/// added's rows in the grid, the lists' rows in the list view, nothing in
/// the text view. Recently added's rows are counted when it is laid out,
/// so its covers are asked for up to the rows a window this tall could
/// hold: a few more than are shown, never fewer.
pub fn wanted(app: &App, view: View, wants: &mut crate::artwork::Wants) {
    let overview = Overview::of(app.repo.library(), app.today);
    match view {
        View::Grid => {
            let covers = shelf_covers(app);
            for section in SHELVES {
                let list = overview.list(section);
                let shown = &list[..covers.min(list.len())];
                wants.covers.extend(shown.iter().map(|e| e.id.clone()));
                // The albums of a shelf mixing types sit in poster frames.
                if !shown.iter().all(|e| ui::cover::is_square(e.media)) {
                    wants.backdrops.extend(
                        shown
                            .iter()
                            .filter(|e| ui::cover::is_square(e.media))
                            .filter_map(|e| app.artwork.cover_url(e).map(str::to_owned)),
                    );
                }
            }
            wants.covers.extend(
                overview
                    .list(Section::RecentlyAdded)
                    .iter()
                    .take(RowStyle::Cover.whole_rows(app.window.height))
                    .map(|e| e.id.clone()),
            );
        }
        View::List => {
            for section in [
                Section::InProgress,
                Section::TopRated,
                Section::ComingSoon,
                Section::RecentlyAdded,
            ] {
                let cap = match section {
                    Section::TopRated => 3,
                    Section::RecentlyAdded => RowStyle::Cover.whole_rows(app.window.height),
                    _ => 4,
                };
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

pub fn body<'a>(app: &'a App) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let overview = Overview::of(app.repo.library(), app.today);
    // The details are part of Home's body here: a click on their empty
    // space stays in them, rather than closing them as the list's does.
    let details = super::panel::details(app).map(opaque);
    let padding = Padding::new(0.0).top(10).bottom(PADDING);
    let scroll = |content: Element<'a, AppMessage>| {
        ui::scroll(container(content).padding(padding)).height(Fill)
    };
    // The right column is as tall as the body and never scrolls:
    // Recently added, its last list, shows the rows that fit.
    let fitted = |content: Element<'a, AppMessage>| -> Element<'a, AppMessage> {
        container(content).padding(padding).height(Fill).into()
    };
    let row = match app.library.view_mode() {
        View::Grid => {
            let left = scroll(shelves(app, &overview, &tokens))
                .id(super::HOME_ID)
                .on_scroll(|viewport| msg(Message::HomeScrolled(viewport.absolute_offset().y)));
            let side = match details {
                Some(details) => details,
                None => fitted(side(app, &overview, &tokens)),
            };
            row![left, container(side).width(Fill).max_width(SIDE_MAX)].spacing(COLUMN_GAP)
        }
        view => {
            let (left, right) = columns(app, &overview, view, &tokens);
            let left = scroll(left)
                .width(Fill)
                .id(super::HOME_ID)
                .on_scroll(|viewport| msg(Message::HomeScrolled(viewport.absolute_offset().y)));
            let right = match details {
                Some(details) => details,
                None => fitted(right),
            };
            row![left, container(right).width(Fill)].spacing(36)
        }
    };
    container(row)
        .padding(Padding::new(0.0).left(PADDING).right(PADDING))
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
) -> (Element<'a, AppMessage>, Element<'a, AppMessage>) {
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
    let right = Column::new().spacing(28).width(Fill).height(Fill).extend(
        [
            list(Section::ComingSoon),
            filling_section(app, overview, Section::RecentlyAdded, style, tokens),
        ]
        .into_iter()
        .flatten(),
    );
    (left.into(), right.into())
}

/// Grid view: cover shelves for Coming soon, Top rated and In progress.
fn shelves<'a>(app: &'a App, overview: &Overview<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
    let covers = shelf_covers(app);
    Column::new()
        .spacing(26)
        .extend(
            SHELVES
                .into_iter()
                .filter_map(|section| shelf(app, overview, section, covers, tokens)),
        )
        .into()
}

/// The grid's side column: the plan and Recently added, in the list
/// view's rows with covers. It starts on the line of the first shelf's
/// header, as the right column of the lists starts on the line of the
/// left one's: the top of the plan card, or with no active plan the
/// Recently added header, whose first row is then level with the covers.
fn side<'a>(app: &'a App, overview: &Overview<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
    let mut side = Column::new().spacing(28).width(Fill).height(Fill);
    if let Some(plan) = &overview.plan {
        side = side.push(plan_card(plan, tokens));
    }
    side.extend(filling_section(
        app,
        overview,
        Section::RecentlyAdded,
        RowStyle::Cover,
        tokens,
    ))
    .into()
}

/// One shelf: its header (the title, the count and a chevron, opening the
/// full list), then up to `covers` covers.
fn shelf<'a>(
    app: &'a App,
    overview: &Overview<'a>,
    section: Section,
    covers: usize,
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
            ui::figure(list.len().to_string(), 12.0, tokens.sub),
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
    let shown = &list[..covers.min(list.len())];
    // A shelf of albums only is as tall as its squares; one mixing types
    // keeps the 2:3 slot, its albums in poster frames, so the details
    // stay on one line.
    let all_square = shown.iter().all(|e| ui::cover::is_square(e.media));
    let row = Row::with_children(shown.iter().map(|entry| {
        let dimmed = selected.is_some_and(|id| id != entry.id);
        let meta = meta(entry, section, app.today);
        shelf_cover(app, entry, meta, all_square, dimmed, tokens)
    }))
    .spacing(COVER_GAP);
    Some(column![header, row].spacing(10).into())
}

/// A 100 × 150 cover with the title on it, the detail under it. An
/// album's 100 × 100 sits in a poster frame of the same slot (its
/// placeholder fills the slot until the image is here), so the details
/// stay on one line; on a shelf of albums only (`all_square`) the
/// slot is the square itself.
fn shelf_cover<'a>(
    app: &'a App,
    entry: &'a Entry,
    meta: String,
    all_square: bool,
    dimmed: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let tint = theme::cover_color(&entry.id);
    let square = ui::cover::is_square(entry.media);
    let image = app.artwork.cover(entry);
    let framed = square && !all_square;
    // Among posters an album without its image is a plain 2:3
    // placeholder, as a poster's: a flat color has no shape to keep.
    let poster = !square || (framed && image.is_none());
    // On a poster frame the frame casts the shadow, not the square.
    let titled = if framed && !poster {
        ui::cover::flat_titled_cover
    } else {
        ui::titled_cover
    };
    let face = titled(
        image.clone(),
        tint,
        &entry.title,
        COVER_WIDTH,
        ui::cover::height_for(COVER_WIDTH, !poster),
        14.0,
    );
    let backdrop = app
        .artwork
        .cover_url(entry)
        .and_then(|url| app.artwork.backdrop(url))
        .or(image);
    let face = match backdrop {
        Some(backdrop) if framed && !poster => ui::cover::poster_frame(face, backdrop, 8.0, true),
        _ => face,
    };
    let cover = container(face)
        .width(COVER_WIDTH)
        .height(ui::cover::height_for(COVER_WIDTH, all_square));
    let content = column![
        cover,
        ui::figure(meta, 12.0, tokens.sub).wrapping(text::Wrapping::None)
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
    let cover = button(content)
        .padding(0)
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(text_button(tokens.text));
    let marked = app.library.menu_on_entry(&entry.id);
    super::menu::area(
        super::menu::Target::Entry(entry.id.clone()),
        ui::Focusable::new(cover, tokens.accent, 8.0).backed(marked.then_some(tokens.selection)),
    )
}

/// How an overview row starts: a type dot (text view) or a small cover
/// with the type under the title (list view, and Recently added beside
/// the grid's shelves).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RowStyle {
    Dot,
    Cover,
}

impl RowStyle {
    /// A row's height, without the line under it.
    fn height(self) -> f32 {
        match self {
            RowStyle::Dot => 38.0,
            RowStyle::Cover => 56.0,
        }
    }

    /// How many rows, each with its line, fit in `height` whole.
    fn whole_rows(self, height: f32) -> usize {
        (height / (self.height() + 1.0)).floor().max(0.0) as usize
    }
}

/// A titled list: the title in gray caps and "See all N" on the right,
/// then up to `cap` rows. Nothing when the list is empty.
pub(super) fn list_section<'a>(
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
    let rows = section_rows(app, &list[..cap.min(list.len())], section, style, tokens);
    Some(
        column![section_header(section, list.len(), tokens), rows]
            .spacing(4)
            .into(),
    )
}

/// A titled list with no fixed length, for the end of a column: it takes
/// the height that is left and shows the rows that fit in it whole, so
/// never half a row and never a scroll of its own; fewer entries leave
/// the space under them empty. Nothing when the list is empty.
fn filling_section<'a>(
    app: &'a App,
    overview: &Overview<'a>,
    section: Section,
    style: RowStyle,
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    let list = overview.list(section).to_vec();
    if list.is_empty() {
        return None;
    }
    let header = section_header(section, list.len(), tokens);
    let rows = responsive(move |size| {
        let fit = style.whole_rows(size.height).min(list.len());
        section_rows(app, &list[..fit], section, style, &app.tokens())
    });
    Some(column![header, rows].spacing(4).height(Fill).into())
}

/// A list's header: the title in gray caps and "See all N" on the right.
fn section_header<'a>(section: Section, count: usize, tokens: &Tokens) -> Element<'a, AppMessage> {
    let see_all = button(
        row![
            text(format!("See all {count}"))
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
    row![
        ui::section_header(section.title(), tokens),
        space::horizontal(),
        see_all
    ]
    .align_y(Center)
    .height(24)
    .into()
}

/// The rows of a list, one under the other.
fn section_rows<'a>(
    app: &'a App,
    entries: &[&'a Entry],
    section: Section,
    style: RowStyle,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let selected = app.library.selected.as_deref();
    Column::with_children(entries.iter().map(|&entry| {
        let is_selected = selected == Some(entry.id.as_str());
        overview_row(
            app,
            entry,
            meta(entry, section, app.today),
            style,
            is_selected,
            tokens,
        )
    }))
    .into()
}

pub(super) fn overview_row<'a>(
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
                .push(ui::cover::thumb(
                    app.artwork.cover(entry),
                    tint,
                    30.0,
                    entry.media,
                    4.0,
                ))
                .push(
                    column![
                        title,
                        text(browse::kind_label(entry))
                            .size(12)
                            .color(tokens.type_text(entry.media))
                    ]
                    .spacing(2)
                    .width(Fill)
                    .clip(true),
                )
        }
    };
    content = content.push(ui::figure(meta, 12.0, tokens.sub));
    let line = tokens.line;
    let row_button = button(content.height(Fill))
        .width(Fill)
        .height(style.height())
        .padding([4, 8])
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(ui::row_style(
            tokens,
            selected || app.library.menu_on_entry(&entry.id),
            0.0,
        ))
        .focusable(0.0, tokens);
    let row_button = super::menu::area(super::menu::Target::Entry(entry.id.clone()), row_button);
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
pub(super) fn plan_card<'a>(active: &ActivePlan<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
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
pub(super) fn short_day(date: Date, today: Date) -> String {
    let month = MONTHS[usize::from(date.month) - 1];
    if date.year == today.year {
        format!("{month} {}", date.day)
    } else {
        format!("{month} {}", date.year)
    }
}

/// A release at its own precision: "Nov 19", "Oct 2027" or "2027".
pub fn short_release(released: Released, today: Date) -> String {
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

    #[test]
    fn shelves_leave_the_side_column_room_for_details() {
        let mut app = App::new(Screen::Library);
        app.window = iced::Size::new(1600.0, 900.0);
        assert_eq!(shelf_covers(&app), 7);
        // 1280 - 232 - 48 - 40 - 352 leaves 608 px: five covers.
        app.window = iced::Size::new(1280.0, 720.0);
        assert_eq!(shelf_covers(&app), 5);
    }

    #[test]
    fn rows_are_counted_whole() {
        // A cover row is 56 px and its line, a text row 38 and its line.
        assert_eq!(RowStyle::Cover.whole_rows(0.0), 0);
        assert_eq!(RowStyle::Cover.whole_rows(56.9), 0);
        assert_eq!(RowStyle::Cover.whole_rows(57.0), 1);
        assert_eq!(RowStyle::Cover.whole_rows(8.0 * 57.0 + 56.0), 8);
        assert_eq!(RowStyle::Dot.whole_rows(8.0 * 57.0 + 56.0), 13);
    }

    /// Recently added shows the rows that fit the window: seven of the
    /// fixture's eight beside the shelves at the smallest window, all
    /// eight at the mockup's size, counted again for each size.
    #[test]
    fn recently_added_fills_the_height_it_has() {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        let _ = app.update(msg(Message::SetView(View::Grid)));
        let mut shows = |size: iced::Size, label: &str| {
            app.window = size;
            let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
            ui.find(label).is_ok()
        };
        // The seventh row was added on Sep 20; the eighth is the only
        // show among them, and only these rows name a title's type.
        let min = crate::app::MIN_WINDOW_SIZE;
        assert!(shows(min, "Sep 20"));
        assert!(!shows(min, "Show"));
        assert!(shows(crate::app::WINDOW_SIZE, "Show"));
    }

    /// The right column starts on the line of the left one's first
    /// header in every view: the top of the plan card when a plan is
    /// active, else the Recently added header (the grid) or the Coming
    /// soon one (the lists), with no room kept for a missing card.
    #[test]
    fn the_right_column_starts_on_the_first_headers_line() {
        // A header is 24 px tall; the card's own is 18 px, 14 px down.
        let header_top = |bounds: iced::Rectangle| bounds.center_y() - 12.0;
        let card_top = |bounds: iced::Rectangle| bounds.center_y() - 9.0 - 14.0;
        for planned in [true, false] {
            for view in [View::Grid, View::List, View::Text] {
                let mut app = App::new(Screen::Library);
                app.today = Date::parse("2026-10-19").unwrap();
                app.window = crate::app::WINDOW_SIZE;
                let _ = app.update(msg(Message::SetView(view)));
                if !planned {
                    let ids: Vec<String> =
                        (app.repo.library().plans.iter().map(|p| p.id.clone())).collect();
                    for id in ids {
                        app.apply(crate::repo::Operation::DeletePlan { id });
                    }
                }
                let mut ui = iced_test::Simulator::with_size(
                    crate::app::settings(),
                    crate::app::WINDOW_SIZE,
                    app.view(),
                );
                let mut place = |label: &str| ui.find(label).expect(label).bounds();
                let (left, right) = match (view, planned) {
                    (View::Grid, true) => ("COMING SOON", card_top(place("ACTIVE PLAN"))),
                    (View::Grid, false) => ("COMING SOON", header_top(place("RECENTLY ADDED"))),
                    (_, true) => ("ACTIVE PLAN", header_top(place("COMING SOON"))),
                    (_, false) => ("IN PROGRESS", header_top(place("COMING SOON"))),
                };
                let left = match left {
                    "ACTIVE PLAN" => card_top(place(left)),
                    _ => header_top(place(left)),
                };
                assert!(
                    (left - right).abs() < 0.6,
                    "{view:?}, plan {planned}: {left} {right}"
                );
                assert_eq!(ui.find("ACTIVE PLAN").is_ok(), planned);
            }
        }
    }

    #[test]
    fn the_grid_asks_for_recently_added_covers() {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        let overview = Overview::of(app.repo.library(), app.today);
        let recent = overview.list(Section::RecentlyAdded);
        assert!(!recent.is_empty());
        let mut wants = crate::artwork::Wants::default();
        wanted(&app, View::Grid, &mut wants);
        for entry in recent.iter().take(8) {
            assert!(wants.covers.contains(&entry.id), "{}", entry.id);
        }
    }
}
