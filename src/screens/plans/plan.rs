//! One plan: the seasonal banner, the header (year menu, state, dates,
//! This plan or All, View, Edit plan), progress, then the checklist or the
//! timeline, and on the right the details or "Add to this plan". The
//! collection ("All Halloween") shares the header and lists everything
//! with the plan's tags.

use iced::widget::{Column, Row, Space, button, column, container, row, space, stack, svg, text};
use iced::{Background, Border, Center, Element, Fill, FillPortion, Padding};

use super::overview::{card_style, footer, pill, plan_accent, progress};
use super::{Message, Page, Popup, collection_title};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Entry, EntryStatus, MediaType, Plan, PlanItem};
use crate::plans::{self as rules, Season, TimelineState};
use crate::screens::entry;
use crate::screens::shell::{self, shell};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Align, ButtonKind, FocusExt, Icon, Popover, seasonal::Art};

fn msg(message: Message) -> AppMessage {
    AppMessage::Plans(message)
}

pub fn view<'a>(app: &'a App, plan: &'a Plan) -> Element<'a, AppMessage> {
    // Inside a seasonal plan the season's color is the accent.
    let base = app.tokens();
    let accent = plan_accent(app, plan, &base);
    let tokens = Tokens {
        accent,
        ink: if super::shown_season(app, plan).is_some() {
            accent
        } else {
            base.ink
        },
        ..base
    };
    let on_collection = matches!(app.plans.page, Page::Collection(_));
    let season = super::shown_season(app, plan);

    let mut main = Column::new();
    if let Some(season) = season {
        main = main.push(banner(season, &tokens));
    }
    main = main
        .push(header(app, plan, on_collection, &tokens))
        .push(ui::rule(tokens.line.scale_alpha(0.5)));
    let style = app.plans.style(&plan.id, tokens.light);
    let body: Element<'a, AppMessage> = if on_collection {
        collection(app, plan, style, &tokens)
    } else if plan.timeline {
        timeline(app, plan, style.covers, &tokens)
    } else {
        checklist(app, plan, style, &tokens)
    };
    main = main
        .push(ui::scroll(container(body).padding(Padding::new(28.0).top(20))).height(Fill))
        .push(footer(app));

    let side: Option<Element<'a, AppMessage>> = match app
        .plans
        .selected
        .as_deref()
        .and_then(|id| app.repo.library().entry(id))
    {
        Some(selected) => Some(entry::details::view(app, selected, Some(plan))),
        None if !on_collection => Some(pool(app, plan, style.covers, &tokens)),
        None => None,
    };
    let content: Element<'a, AppMessage> = match side {
        Some(side) => row![container(main).width(Fill), side].into(),
        None => main.into(),
    };
    shell(app, content)
}

/// The 120 px strip: its own row, nothing on top of it but Halloween's
/// cobweb in the corner.
fn banner<'a>(season: Season, tokens: &Tokens) -> Element<'a, AppMessage> {
    let art = ui::seasonal::art(season, Art::Banner, Fill, 120, tokens);
    if season != Season::Halloween {
        return art;
    }
    let web = svg::Svg::new(ui::seasonal::handle(season, Art::Web, tokens))
        .width(120)
        .height(110)
        .opacity(0.4_f32);
    stack![art, container(web).width(Fill).align_right(Fill)]
        .height(120)
        .into()
}

fn header<'a>(
    app: &'a App,
    plan: &'a Plan,
    on_collection: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.plans;
    let library = app.repo.library();
    let crumb_text = if on_collection {
        collection_title(plan)
    } else {
        plan.title()
    };
    let ink_sub = tokens.sub;
    let crumb = row![
        button(text("All plans").size(13))
            .padding(0)
            .on_press(msg(Message::ShowOverview))
            .style(move |_, _| button::Style {
                text_color: ink_sub,
                ..button::Style::default()
            })
            .focusable(4.0, tokens),
        text("›").size(13).color(tokens.sub),
        text(crumb_text).size(13).color(tokens.sub),
    ]
    .spacing(6);

    let mut title = Row::new().spacing(12).align_y(Center);
    if let Some(season) = super::shown_season(app, plan) {
        title = title.push(ui::seasonal::art(season, Art::Icon, 34, 34, tokens));
    }
    let name = if on_collection {
        collection_title(plan)
    } else {
        plan.name.clone()
    };
    title = title.push(text(name).size(30).font(font::EXTRABOLD));
    if let (false, Some(year)) = (on_collection, plan.year) {
        title = title.push(year_menu(app, plan, year, tokens));
    }
    if !on_collection {
        title = title.push(pill(rules::plan_state(plan, app.today).label(), tokens));
    }
    let dates = if on_collection {
        plan.tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        rules::date_span(plan)
            .map(|s| rules::format_when(s, plan.year))
            .unwrap_or_default()
    };
    title = title.push(ui::mono(dates, 13.0, tokens.sub));
    // The scope tabs, View and Edit plan: beside the title, or on a row of
    // their own when the window is narrow.
    let mut actions = Row::new().spacing(12).align_y(Center);
    if !plan.tags.is_empty() {
        let scope = [
            (
                "This plan",
                !on_collection,
                msg(Message::ShowPlan(plan.id.clone())),
            ),
            (
                "All",
                on_collection,
                msg(Message::ShowCollection(plan.id.clone())),
            ),
        ];
        actions = actions.push(container(ui::segmented(scope, 32.0, tokens)).width(200));
    }
    let view_button = ui::menu_button(
        Some(Icon::Sliders),
        text("View"),
        None,
        state.popup == Some(Popup::View),
        tokens,
        msg(Message::TogglePopup(Popup::View)),
    );
    let view_popover =
        (state.popup == Some(Popup::View)).then(|| view_options(app, plan, on_collection, tokens));
    actions = actions.push(
        Popover::new(view_button, view_popover)
            .align(Align::End)
            .on_dismiss(msg(Message::ClosePopup)),
    );
    if !on_collection {
        actions = actions.push(ui::labeled_button(
            "Edit plan",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::EditPlan)),
        ));
    }
    let narrow = shell::content_width(app, entry::details::WIDTH) < 1000.0;
    let mut header = Column::new().spacing(12).push(crumb);
    if narrow {
        header = header.push(title).push(row![space::horizontal(), actions]);
    } else {
        header = header.push(title.push(space::horizontal()).push(actions));
    }
    if on_collection {
        let all = rules::collection(plan, library).len();
        let tags = plan
            .tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(", ");
        header = header.push(
            text(format!(
                "Everything tagged {tags} · {all} titles · {} in {}",
                plan.items.len(),
                plan.title()
            ))
            .size(13.5)
            .color(tokens.sub),
        );
    } else {
        let linked = if plan.tags.is_empty() {
            "No tag".to_owned()
        } else {
            format!(
                "Linked to {}",
                plan.tags
                    .iter()
                    .map(|t| format!("#{t}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        header = header.push(
            row![
                progress_with_marker(app, plan, tokens),
                text(rules::progress_label(plan))
                    .size(14)
                    .font(font::SEMIBOLD),
                text(linked).size(13).color(tokens.sub),
            ]
            .spacing(14)
            .align_y(Center)
            .padding(Padding::new(0.0).top(4)),
        );
    }
    let top = if super::shown_season(app, plan).is_some() {
        8.0
    } else {
        20.0
    };
    container(header)
        .padding(Padding::new(28.0).top(top).bottom(16))
        .into()
}

/// The year chip: the other years of the same plan, then "New year…",
/// which copies this plan to the year after.
fn year_menu<'a>(
    app: &'a App,
    plan: &'a Plan,
    year: i32,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let open = app.plans.popup == Some(Popup::Year);
    let tokens_copy = *tokens;
    let chip = button(
        row![
            text(year.to_string()).size(17).font(font::SEMIBOLD),
            ui::icon(Icon::ChevronDown, 15.0, tokens.text)
        ]
        .spacing(4)
        .align_y(Center)
        .height(Fill),
    )
    .height(34)
    .padding(Padding::new(0.0).left(12).right(8))
    .on_press(msg(Message::TogglePopup(Popup::Year)))
    .style(move |_, _| button::Style {
        background: open.then_some(Background::Color(tokens_copy.selection)),
        text_color: tokens_copy.text,
        border: Border::default()
            .rounded(17)
            .width(1)
            .color(tokens_copy.line),
        ..button::Style::default()
    })
    .focusable(8.0, tokens);
    let menu = open.then(|| {
        let library = app.repo.library();
        let mut items = Column::new();
        for other in rules::plan_years(library, plan) {
            let label = other.year.map(|y| y.to_string()).unwrap_or_default();
            let state = rules::plan_state(other, app.today).label();
            items = items.push(
                button(
                    row![
                        text(label).size(14).font(font::SEMIBOLD).width(Fill),
                        text(state).size(12).color(tokens.sub)
                    ]
                    .align_y(Center)
                    .height(Fill),
                )
                .width(Fill)
                .height(36)
                .padding([0, 10])
                .on_press(msg(Message::ShowPlan(other.id.clone())))
                .style(ui::row_style(tokens, other.id == plan.id, 8.0))
                .focusable(8.0, tokens),
            );
        }
        let ink = tokens.ink;
        items = items.push(ui::rule(tokens.line)).push(
            button(
                text("New year…")
                    .size(14)
                    .font(font::SEMIBOLD)
                    .height(Fill)
                    .align_y(Center),
            )
            .width(Fill)
            .height(36)
            .padding([0, 10])
            .on_press(msg(Message::NewYear))
            .style(move |_, _| button::Style {
                text_color: ink,
                ..button::Style::default()
            })
            .focusable(8.0, tokens),
        );
        let tokens = *tokens;
        container(items.spacing(2))
            .width(180)
            .padding(6)
            .style(move |_| ui::panel_style(&tokens, 12.0))
            .into()
    });
    Popover::new(chip, menu)
        .align(Align::Start)
        .on_dismiss(msg(Message::ClosePopup))
        .into()
}

/// The View popover: layout (a plan), sections, entries.
fn view_options<'a>(
    app: &'a App,
    plan: &'a Plan,
    on_collection: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let style = app.plans.style(&plan.id, tokens.light);
    let group =
        |label: &str, options: [(&'a str, bool, AppMessage); 2]| -> Element<'a, AppMessage> {
            column![
                ui::section_header(label, tokens),
                ui::segmented(options, 32.0, tokens)
            ]
            .spacing(7)
            .into()
        };
    let mut panel = Column::new().spacing(14);
    if !on_collection {
        panel = panel.push(group(
            "Layout",
            [
                (
                    "Checklist",
                    !plan.timeline,
                    msg(Message::SetTimeline(false)),
                ),
                ("Timeline", plan.timeline, msg(Message::SetTimeline(true))),
            ],
        ));
    }
    panel = panel
        .push(group(
            "Sections",
            [
                ("Plain", !style.cards, msg(Message::SetCards(false))),
                ("Cards", style.cards, msg(Message::SetCards(true))),
            ],
        ))
        .push(group(
            "Entries",
            [
                ("Text", !style.covers, msg(Message::SetCovers(false))),
                ("With covers", style.covers, msg(Message::SetCovers(true))),
            ],
        ))
        .push(
            text("Remembered per plan. Seasonal touches are in Settings.")
                .size(12)
                .color(tokens.sub),
        );
    let tokens = *tokens;
    container(panel)
        .width(300)
        .padding(14)
        .style(move |_| ui::panel_style(&tokens, 12.0))
        .into()
}

/// The progress bar, with the season's marker riding on it.
fn progress_with_marker<'a>(app: &App, plan: &Plan, tokens: &Tokens) -> Element<'a, AppMessage> {
    let bar = container(progress(plan, tokens.accent, 8.0, tokens)).center_y(22);
    let marker = match super::shown_season(app, plan) {
        Some(Season::Halloween) => Icon::MarkerGhost,
        Some(Season::Christmas) => Icon::MarkerStar,
        None => return bar.width(Fill).into(),
    };
    let total = plan.items.len().max(1);
    let done = rules::done_count(plan);
    // FillPortion splits the bar; the marker sits at the end of the done
    // part, flush with the bar's left end at 0 and its right end at 100.
    let before = u16::try_from(done * 100 / total).unwrap_or(100);
    let mut marker_row = Row::new();
    if before > 0 {
        marker_row = marker_row.push(Space::new().width(FillPortion(before)));
    }
    marker_row = marker_row.push(ui::marker(marker, 17.0));
    if before < 100 {
        marker_row = marker_row.push(Space::new().width(FillPortion(100 - before)));
    }
    stack![bar, marker_row].width(Fill).into()
}

/// The checklist: one section per type, in two columns.
fn checklist<'a>(
    app: &'a App,
    plan: &'a Plan,
    style: super::Style,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let sections: Vec<Element<'a, AppMessage>> = MediaType::ALL
        .into_iter()
        .filter_map(|media| {
            let items: Vec<(&PlanItem, &Entry)> = plan
                .items
                .iter()
                .filter_map(|item| library.entry(&item.entry).map(|e| (item, e)))
                .filter(|(_, e)| e.media == media)
                .collect();
            if items.is_empty() {
                return None;
            }
            let done = items.iter().filter(|(i, _)| i.done).count();
            let rows = items
                .iter()
                .map(|&(item, entry)| plan_row(app, item, entry, style.covers, tokens));
            Some(section(
                media,
                format!("{done}/{}", items.len()),
                Column::with_children(rows),
                style.cards,
                tokens,
            ))
        })
        .collect();
    if sections.is_empty() {
        return ui::empty_state(
            Icon::Document,
            "Nothing in this plan yet".into(),
            "Add entries from the list on the right.".into(),
            None,
            tokens,
        );
    }
    two_columns(sections)
}

/// Sections into two columns, filled alternately.
fn two_columns(sections: Vec<Element<'_, AppMessage>>) -> Element<'_, AppMessage> {
    let (mut left, mut right) = (
        Column::new().spacing(22).width(Fill),
        Column::new().spacing(22).width(Fill),
    );
    for (index, section) in sections.into_iter().enumerate() {
        if index % 2 == 0 {
            left = left.push(section);
        } else {
            right = right.push(section);
        }
    }
    row![left, right].spacing(28).into()
}

/// A type's section: its name in its color with the count, then the rows.
fn section<'a>(
    media: MediaType,
    count: String,
    rows: Column<'a, AppMessage>,
    cards: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let heading = row![
        text(media.label())
            .size(18)
            .font(font::EXTRABOLD)
            .color(tokens.type_color(media)),
        ui::mono(count, 12.0, tokens.sub),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Bottom)
    .padding(Padding::new(0.0).left(4).right(4).bottom(6));
    let body = column![heading, rows];
    let tokens = *tokens;
    if cards {
        container(body)
            .width(Fill)
            .padding(Padding::new(18.0).top(16).bottom(8))
            .style(move |_| card_style(&tokens, 14.0))
            .into()
    } else {
        body.width(Fill).into()
    }
}

/// The round tick in the plan's accent.
fn tick<'a>(
    done: bool,
    size: f32,
    tokens: &Tokens,
    on_press: AppMessage,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let mark: Element<'a, AppMessage> = if done {
        ui::icon(Icon::Check, size * 0.55, tokens.on_accent)
    } else {
        Space::new().into()
    };
    button(container(mark).center(Fill))
        .width(size)
        .height(size)
        .padding(0)
        .on_press(on_press)
        .style(move |_, _| button::Style {
            background: done.then_some(Background::Color(tokens.accent)),
            border: Border::default()
                .rounded(size / 2.0)
                .width(2)
                .color(if done { tokens.accent } else { tokens.sub }),
            ..button::Style::default()
        })
        .focusable(size / 2.0, &tokens)
}

fn cover<'a>(app: &App, entry: &Entry, width: f32, height: f32) -> Element<'a, AppMessage> {
    let tint = theme::cover_color(&entry.id);
    ui::plain_cover(app.artwork.cover(entry), tint, width, height, 5.0)
}

/// A checklist row: the tick, an optional cover, the title over the year
/// (and "in progress"), and Rewatch for a repeat. A click opens the
/// details.
fn plan_row<'a>(
    app: &'a App,
    item: &'a PlanItem,
    entry: &'a Entry,
    covers: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let mut sub = entry.year().map(|y| y.to_string()).unwrap_or_default();
    if entry.status == EntryStatus::InProgress && !item.done {
        sub.push_str(" · in progress");
    }
    let mut content = Row::new().spacing(12).align_y(Center).push(tick(
        item.done,
        24.0,
        tokens,
        msg(Message::ToggleDone(entry.id.clone())),
    ));
    if covers {
        content = content.push(cover(app, entry, 38.0, 57.0));
    }
    content = content.push(
        column![
            text(&entry.title)
                .size(15)
                .font(font::SEMIBOLD)
                .color(if item.done { tokens.sub } else { tokens.text })
                .wrapping(text::Wrapping::None),
            text(sub).size(12.5).font(font::ITALIC).color(tokens.sub),
        ]
        .spacing(1)
        .width(Fill),
    );
    if item.repeat {
        content = content.push(
            text(rules::repeat_label(entry.media))
                .size(12)
                .color(tokens.sub),
        );
    }
    let selected = app.plans.selected.as_deref() == Some(entry.id.as_str());
    column![
        button(content)
            .width(Fill)
            .padding([6, 4])
            .on_press(msg(Message::Select(entry.id.clone())))
            .style(ui::row_style(tokens, selected, 8.0))
            .focusable(8.0, tokens),
        ui::rule(tokens.line.scale_alpha(0.5)),
    ]
    .into()
}

/// The timeline: one row per bucket, the date on the left, a rail filled
/// up to today, and the items as small cards.
fn timeline<'a>(
    app: &'a App,
    plan: &'a Plan,
    covers: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let groups = rules::timeline(plan, library, app.today);
    if groups.is_empty() {
        return ui::empty_state(
            Icon::Calendar,
            "Nothing on the timeline yet".into(),
            "Add entries, then give each a day or a period in its details.".into(),
            None,
            tokens,
        );
    }
    let rows = groups.into_iter().map(|group| {
        let reached = matches!(group.state, TimelineState::Past | TimelineState::Current);
        let (rail, dot) = if reached {
            (tokens.accent, tokens.accent)
        } else {
            (tokens.line, tokens.raised)
        };
        let date = column![
            text(group.title.clone())
                .size(13)
                .font(font::SEMIBOLD)
                .color(if reached { tokens.text } else { tokens.sub }),
            ui::mono(group.dates.clone(), 12.0, tokens.sub),
        ]
        .spacing(2)
        .align_x(iced::alignment::Horizontal::Right)
        .width(120)
        .padding(Padding::new(0.0).top(12));
        let content = tokens.content;
        let line = container(Space::new())
            .width(3)
            .height(Fill)
            .style(move |_| container::background(rail));
        let marker = container(Space::new())
            .width(13)
            .height(13)
            .style(move |_| {
                container::background(dot)
                    .border(Border::default().rounded(7).width(3).color(content))
            });
        let track = stack![
            container(line).center_x(22),
            container(marker)
                .center_x(22)
                .padding(Padding::new(0.0).top(14)),
        ]
        .width(22)
        .height(Fill);
        let items = Column::with_children(group.items.iter().filter_map(|item| {
            let entry = library.entry(&item.entry)?;
            Some(timeline_item(app, item, entry, covers, tokens))
        }))
        .spacing(6)
        .padding([6, 0])
        .width(Fill);
        row![date, track, items]
            .spacing(14)
            .height(iced::Shrink)
            .into()
    });
    Column::with_children(rows).into()
}

fn timeline_item<'a>(
    app: &'a App,
    item: &'a PlanItem,
    entry: &'a Entry,
    covers: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let when = item
        .when
        .map(|w| rules::format_when(w, None))
        .unwrap_or_default();
    let mut content = Row::new().spacing(12).align_y(Center);
    if covers {
        content = content.push(cover(app, entry, 30.0, 45.0));
    }
    content = content
        .push(ui::dot(tokens.type_color(entry.media), 8.0, 3.0))
        .push(
            text(&entry.title)
                .size(15)
                .font(font::SEMIBOLD)
                .color(if item.done { tokens.sub } else { tokens.text })
                .width(Fill),
        )
        .push(text(when).size(12.5).font(font::ITALIC).color(tokens.sub))
        .push(tick(
            item.done,
            22.0,
            tokens,
            msg(Message::ToggleDone(entry.id.clone())),
        ));
    let selected = app.plans.selected.as_deref() == Some(entry.id.as_str());
    let tokens = *tokens;
    button(content)
        .width(Fill)
        .padding([8, 12])
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(move |theme, status| {
            let mut style = ui::row_style(&tokens, selected, 8.0)(theme, status);
            if style.background.is_none() {
                style.background = Some(Background::Color(theme::card_background(&tokens)));
            }
            style
        })
        .focusable(8.0, &tokens)
}

/// All of the plan's tags: by type, with "In plan" or a + to add.
fn collection<'a>(
    app: &'a App,
    plan: &'a Plan,
    style: super::Style,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let all = rules::collection(plan, library);
    let sections: Vec<Element<'a, AppMessage>> = MediaType::ALL
        .into_iter()
        .filter_map(|media| {
            let entries: Vec<&Entry> = all.iter().copied().filter(|e| e.media == media).collect();
            if entries.is_empty() {
                return None;
            }
            let rows = entries.iter().map(|entry| {
                let in_plan = plan.item_for(&entry.id).is_some();
                let status = browse::status_label(Some(entry.media), entry.status);
                let sub = format!(
                    "{} · {}",
                    browse::kind_label(entry),
                    entry.year().map(|y| y.to_string()).unwrap_or_default()
                );
                let mut content = Row::new().spacing(12).align_y(Center);
                if style.covers {
                    content = content.push(cover(app, entry, 38.0, 57.0));
                }
                content = content
                    .push(
                        column![
                            text(&entry.title)
                                .size(15)
                                .font(font::SEMIBOLD)
                                .wrapping(text::Wrapping::None),
                            text(sub).size(12.5).font(font::ITALIC).color(tokens.sub),
                        ]
                        .spacing(1)
                        .width(Fill),
                    )
                    .push(text(status).size(12).color(tokens.sub));
                content = if in_plan {
                    content.push(pill("In plan", tokens))
                } else {
                    content.push(plus(tokens, msg(Message::AddItem(entry.id.clone()))))
                };
                column![
                    container(content).padding([6, 4]),
                    ui::rule(tokens.line.scale_alpha(0.5))
                ]
                .into()
            });
            Some(section(
                media,
                entries.len().to_string(),
                Column::with_children(rows),
                style.cards,
                tokens,
            ))
        })
        .collect();
    two_columns(sections)
}

/// A 30 px outlined +.
fn plus<'a>(tokens: &Tokens, on_press: AppMessage) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(container(ui::icon(Icon::Plus, 14.0, tokens.text)).center(Fill))
        .width(30)
        .height(30)
        .padding(0)
        .on_press(on_press)
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered)
                .then_some(Background::Color(tokens.hover)),
            border: Border::default().rounded(8).width(1).color(tokens.line),
            ..button::Style::default()
        })
        .focusable(8.0, &tokens)
}

/// "Add to this plan": entries with the plan's tags not in it yet (wanted
/// titles for a plan without tags); a search looks at the whole library.
fn pool<'a>(
    app: &'a App,
    plan: &'a Plan,
    covers: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let query = app.plans.pool_query.trim().to_lowercase();
    let entries: Vec<&Entry> = if query.is_empty() {
        rules::pool(plan, library)
    } else {
        library
            .entries
            .iter()
            .filter(|e| plan.item_for(&e.id).is_none() && browse::matches_search(e, &query))
            .collect()
    };
    let help = if plan.tags.is_empty() {
        "This plan has no tag, so here are titles you marked Want.".to_owned()
    } else {
        let tags: Vec<String> = plan.tags.iter().map(|t| format!("#{t}")).collect();
        format!(
            "Tagged {}, not in the plan yet. Use + to add one.",
            tags.join(" or ")
        )
    };
    let rows = Column::with_children(entries.into_iter().take(40).map(|entry| {
        let mut content = Row::new().spacing(10).align_y(Center);
        if covers {
            content = content.push(cover(app, entry, 28.0, 42.0));
        }
        let sub = format!(
            "{} · {}",
            browse::kind_label(entry),
            entry.year().map(|y| y.to_string()).unwrap_or_default()
        );
        content = content
            .push(ui::dot(tokens.type_color(entry.media), 8.0, 3.0))
            .push(
                column![
                    text(&entry.title)
                        .size(14)
                        .font(font::SEMIBOLD)
                        .wrapping(text::Wrapping::None),
                    text(sub).size(12).color(tokens.sub),
                ]
                .width(Fill)
                .clip(true),
            )
            .push(plus(tokens, msg(Message::AddItem(entry.id.clone()))));
        column![
            container(content).padding([6, 0]),
            ui::rule(tokens.line.scale_alpha(0.5))
        ]
        .into()
    }));
    let content_bg = tokens.content;
    container(
        column![
            column![
                text("Add to this plan").size(19).font(font::EXTRABOLD),
                text(help).size(13).color(tokens.sub),
            ]
            .spacing(4),
            ui::search_field(
                "Filter or search whole library",
                &app.plans.pool_query,
                |q| msg(Message::PoolQuery(q)),
                None,
                None,
                tokens
            ),
            ui::scroll(rows).height(Fill),
        ]
        .spacing(14),
    )
    .width(entry::details::WIDTH)
    .height(Fill)
    .padding([22, 20])
    .style(move |_| container::background(content_bg))
    .into()
}
