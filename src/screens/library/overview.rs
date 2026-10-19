//! All with nothing narrowing it: a "now and next" overview. In the text
//! and list views two columns of short lists; in the grid, cover shelves
//! with a side column. Every list has a "See all" that opens the full,
//! filtered list.

use iced::widget::{Column, Row, button, column, container, progress_bar, row, space, text};
use iced::{Background, Border, Center, Color, Element, Fill, Padding};

use super::{Message, View};
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, ActivePlan, Overview, Section};
use crate::model::{Date, Entry, Released};
use crate::screens::Screen;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
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
    let shelves = Column::new().spacing(26).extend(
        [Section::ComingSoon, Section::TopRated, Section::InProgress]
            .into_iter()
            .filter_map(|section| shelf(app, overview, section, tokens)),
    );
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
    row![shelves, container(side).max_width(560)]
        .spacing(40)
        .into()
}

/// One shelf: its header (the title, the count and a chevron, opening the
/// full list), then up to seven covers.
fn shelf<'a>(
    app: &'a App,
    overview: &Overview<'a>,
    section: Section,
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
    .style(text_button(tokens.sub));
    let selected = app.library.selected.as_deref();
    let covers = Row::with_children(list.iter().take(7).map(|entry| {
        let dimmed = selected.is_some_and(|id| id != entry.id);
        shelf_cover(entry, meta(entry, section, app.today), dimmed, tokens)
    }))
    .spacing(14);
    Some(column![header, covers].spacing(10).into())
}

/// A 100 × 150 cover with the title on it, the detail under it.
fn shelf_cover<'a>(
    entry: &'a Entry,
    meta: String,
    dimmed: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let tint = theme::cover_color(&entry.id);
    let cover = container(
        text(&entry.title)
            .size(14)
            .font(font::EXTRABOLD)
            .color(Color::WHITE),
    )
    .width(100)
    .height(150)
    .padding(10)
    .align_y(iced::alignment::Vertical::Bottom)
    .style(move |_| container::background(tint).border(Border::default().rounded(8)));
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
        .into()
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
    .style(text_button(tokens.ink));
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
                .push(container(space()).width(30).height(45).style(move |_| {
                    container::background(tint).border(Border::default().rounded(4))
                }))
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
        .style(ui::row_style(tokens, selected, 0.0));
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
    .style(text_button(tokens.ink));
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
