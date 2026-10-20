//! All plans: Active and Upcoming plans as cards, Finished ones as rows,
//! then the seasonal collections.

use iced::widget::{Column, Row, button, column, container, progress_bar, row, space, text};
use iced::{Background, Border, Center, Color, Element, Fill, Padding, Shadow, Vector};

use super::Message;
use crate::app::{App, Message as AppMessage};
use crate::model::Plan;
use crate::plans::{self as rules, PlanState, Season};
use crate::screens::shell::shell;
use crate::settings::Source;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Icon, seasonal::Art};

fn msg(message: Message) -> AppMessage {
    AppMessage::Plans(message)
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let header = row![
        text("All plans").size(30).font(font::EXTRABOLD),
        ui::mono(library.plans.len().to_string(), 13.0, tokens.sub),
        space::horizontal(),
        ui::add_button("New plan", &tokens, msg(Message::NewPlan)),
    ]
    .spacing(14)
    .align_y(Center);

    let mut page = Column::new().spacing(26).push(header);
    for (label, state) in [
        ("Active", PlanState::Active),
        ("Upcoming", PlanState::NotStarted),
    ] {
        let plans = super::by_state(app, state);
        if plans.is_empty() {
            continue;
        }
        let cards: Vec<Element<'_, AppMessage>> =
            plans.iter().map(|plan| card(app, plan, &tokens)).collect();
        page = page.push(group(label, grid(cards, 3), &tokens));
    }

    let finished = super::by_state(app, PlanState::Finished);
    if !finished.is_empty() {
        let rows = Column::with_children(finished.iter().map(|plan| finished_row(plan, &tokens)));
        page = page.push(group("Finished", rows.into(), &tokens));
    }

    let collections: Vec<Element<'_, AppMessage>> = [Season::Halloween, Season::Christmas]
        .into_iter()
        .filter_map(|season| collection_card(app, season, &tokens))
        .collect();
    if !collections.is_empty() {
        let body = column![
            text("Everything in your library tagged for the season, in a plan or not.")
                .size(13.5)
                .color(tokens.sub),
            grid(collections, 2),
        ]
        .spacing(12);
        page = page.push(group("Seasonal collections", body.into(), &tokens));
    }

    let content = column![
        ui::scroll(container(page).padding(Padding::new(32.0).top(24))).height(Fill),
        footer(app),
    ];
    shell(app, content)
}

fn group<'a>(
    label: &str,
    body: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![ui::section_header(label, tokens), body]
        .spacing(12)
        .into()
}

/// Rows of `columns` cells sharing the width.
fn grid(cells: Vec<Element<'_, AppMessage>>, columns: usize) -> Element<'_, AppMessage> {
    let mut rows = Column::new().spacing(20);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut row = Row::new().spacing(20);
        for _ in 0..columns {
            row = row.push(match cells.next() {
                Some(cell) => container(cell).width(Fill),
                None => container(space()).width(Fill),
            });
        }
        rows = rows.push(row);
    }
    rows.into()
}

/// The accent inside a plan: the season's, or the regular one.
pub fn plan_accent(app: &App, plan: &Plan, tokens: &Tokens) -> Color {
    match super::shown_season(app, plan) {
        Some(Season::Halloween) => theme::HALLOWEEN,
        Some(Season::Christmas) => theme::CHRISTMAS,
        None => tokens.accent,
    }
}

/// "Oct 1–31", or what the plan holds when it has no dates.
pub fn dates_or_subtitle(plan: &Plan) -> String {
    match rules::date_span(plan) {
        Some(span) => rules::format_when(span, plan.year),
        None => rules::subtitle(plan),
    }
}

pub fn card_style(tokens: &Tokens, radius: f32) -> container::Style {
    container::Style {
        text_color: Some(tokens.text),
        background: Some(Background::Color(theme::card_background(tokens))),
        border: Border::default().rounded(radius),
        shadow: if tokens.light {
            Shadow {
                color: Color::from_rgba8(20, 22, 28, 0.10),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 14.0,
            }
        } else {
            Shadow::default()
        },
        ..container::Style::default()
    }
}

/// A plan's card: its art (or five covers), name, year and state, what it
/// holds, and its progress.
fn card<'a>(app: &'a App, plan: &'a Plan, tokens: &Tokens) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let top: Element<'a, AppMessage> = match super::shown_season(app, plan) {
        Some(season) => ui::seasonal::art(season, Art::Card, Fill, 96, tokens),
        None => {
            let band = if tokens.light {
                Color::from_rgb8(0xF1, 0xF2, 0xF4)
            } else {
                theme::mix_white(theme::card_background(tokens), 0.04)
            };
            let thumbs = Row::with_children(plan.items.iter().take(5).map(|item| {
                let tint = theme::cover_color(&item.entry);
                container(space())
                    .width(30)
                    .height(45)
                    .style(move |_| {
                        container::background(tint).border(Border::default().rounded(4))
                    })
                    .into()
            }))
            .spacing(6);
            container(thumbs)
                .width(Fill)
                .height(96)
                .padding(Padding::new(0.0).left(18).bottom(12))
                .align_y(iced::alignment::Vertical::Bottom)
                .style(move |_| container::background(band))
                .into()
        }
    };
    let state = rules::plan_state(plan, app.today);
    let mut meta = dates_or_subtitle(plan);
    let next: Vec<&str> = plan
        .items
        .iter()
        .filter(|i| !i.done)
        .filter_map(|i| library.entry(&i.entry))
        .take(2)
        .map(|e| e.title.as_str())
        .collect();
    if state != PlanState::Finished && !next.is_empty() {
        meta = format!("{meta} · Next up: {}", next.join(", "));
    }
    let body = column![
        row![
            text(&plan.name).size(20).font(font::EXTRABOLD),
            text(plan.year.map(|y| y.to_string()).unwrap_or_default())
                .size(20)
                .font(font::MEDIUM)
                .color(tokens.sub),
            space::horizontal(),
            pill(state.label(), tokens),
        ]
        .spacing(8)
        .align_y(Center),
        text(meta).size(13).color(tokens.sub),
        row![
            progress(plan, plan_accent(app, plan, tokens), 6.0, tokens),
            text(rules::progress_label(plan))
                .size(13)
                .font(font::SEMIBOLD),
        ]
        .spacing(10)
        .align_y(Center),
    ]
    .spacing(8)
    .padding(Padding::new(18.0).top(14));
    let tokens = *tokens;
    button(
        container(column![top, body])
            .clip(true)
            .style(move |_| card_style(&tokens, 16.0)),
    )
    .padding(0)
    .on_press(msg(Message::ShowPlan(plan.id.clone())))
    .style(|_, _| button::Style::default())
    .into()
}

/// A small gray pill: a plan's state, "In plan".
pub fn pill<'a>(label: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    let selection = tokens.selection;
    container(text(label).size(12).font(font::SEMIBOLD))
        .height(22)
        .padding([0, 9])
        .center_y(22)
        .style(move |_| container::background(selection).border(Border::default().rounded(11)))
        .into()
}

/// The plan's progress bar in its accent.
pub fn progress<'a>(
    plan: &Plan,
    accent: Color,
    height: f32,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let track = tokens.track;
    progress_bar(
        0.0..=plan.items.len().max(1) as f32,
        rules::done_count(plan) as f32,
    )
    .girth(height)
    .style(move |_| progress_bar::Style {
        background: Background::Color(track),
        bar: Background::Color(accent),
        border: Border::default().rounded(height / 2.0),
    })
    .into()
}

fn finished_row<'a>(plan: &'a Plan, tokens: &Tokens) -> Element<'a, AppMessage> {
    let content = row![
        column![
            row![
                text(&plan.name).size(15).font(font::SEMIBOLD),
                text(plan.year.map(|y| y.to_string()).unwrap_or_default())
                    .size(15)
                    .font(font::MEDIUM)
                    .color(tokens.sub),
            ]
            .spacing(6),
            text(dates_or_subtitle(plan)).size(13).color(tokens.sub),
        ]
        .spacing(2)
        .width(Fill),
        ui::icon(Icon::ChevronRight, 18.0, tokens.sub),
    ]
    .align_y(Center);
    column![
        button(content)
            .width(Fill)
            .padding([9, 4])
            .on_press(msg(Message::ShowPlan(plan.id.clone())))
            .style(ui::row_style(tokens, false, 8.0)),
        ui::rule(tokens.line.scale_alpha(0.5)),
    ]
    .into()
}

/// A season's collection: everything tagged for it. Opens through the
/// newest plan linked to the season; none when no plan is.
fn collection_card<'a>(
    app: &'a App,
    season: Season,
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    let library = app.repo.library();
    let plan = library
        .plans
        .iter()
        .filter(|p| rules::season_for_tags(&p.tags) == Some(season))
        .max_by_key(|p| p.year.unwrap_or(0))?;
    let count = rules::collection(plan, library).len();
    let content = row![
        ui::seasonal::art(season, Art::Collection, 220, 96, tokens),
        column![
            text(season.name()).size(19).font(font::EXTRABOLD),
            text(format!(
                "Everything tagged #{} · {count} titles",
                season.tag()
            ))
            .size(13)
            .color(tokens.sub),
        ]
        .spacing(4)
        .width(Fill),
        ui::icon(Icon::ChevronRight, 18.0, tokens.sub),
    ]
    .spacing(18)
    .padding(Padding::new(0.0).right(22))
    .align_y(Center);
    let tokens = *tokens;
    Some(
        button(
            container(content)
                .height(96)
                .clip(true)
                .style(move |_| card_style(&tokens, 16.0)),
        )
        .padding(0)
        .on_press(msg(Message::ShowCollection(plan.id.clone())))
        .style(|_, _| button::Style::default())
        .into(),
    )
}

/// The repository and branch, quietly at the bottom of the content.
pub fn footer<'a>(app: &App) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let line = app.source.as_ref().map(Source::footer).unwrap_or_default();
    container(ui::mono(line, 12.0, tokens.sub))
        .height(30)
        .width(Fill)
        .padding([0, 28])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(Center)
        .into()
}
