//! The window shell shared by Library and Plans: the sidebar on the
//! chrome, and the content pane with its rounded top-left corner.

use iced::border::Radius;
use iced::widget::{Column, button, column, container, progress_bar, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Screen, library, plans, setup};
use crate::app::{App, Message};
use crate::model::MediaType;
use crate::plans as rules;
use crate::settings::SyncState;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Icon, Placement, Popover};

pub fn shell<'a>(app: &'a App, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let tokens = app.tokens();
    let pane = container(content).width(Fill).height(Fill).style(move |_| {
        container::background(tokens.content).border(Border {
            radius: Radius::default().top_left(12),
            ..Border::default()
        })
    });
    let chrome = tokens.chrome;
    container(row![sidebar(app, &tokens), pane])
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(chrome).color(tokens.text))
        .into()
}

fn sidebar<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, Message> {
    let library = app.repo.library();
    let on_library = app.screen == Screen::Library;
    let mut items = Column::new().spacing(2);

    items = items.push(header("Library", 8.0, tokens));
    let kinds = std::iter::once(None).chain(MediaType::ALL.map(Some));
    for media in kinds {
        let (label, color) = match media {
            Some(media) => (media.label(), tokens.type_color(media)),
            None => ("All", tokens.sub),
        };
        let state = &app.library;
        let selected = on_library
            && match media {
                Some(media) => state.types.contains(&media),
                None => state.types.is_empty(),
            };
        // With type checkboxes on, a box adds or removes the type and the
        // name shows only it.
        let lead = match media {
            Some(media) if state.type_boxes => ui::type_checkbox(
                color,
                selected,
                tokens,
                Message::Library(library::Message::ToggleType(media)),
            ),
            _ => ui::dot(color, 9.0, 3.0),
        };
        let row = row![
            lead,
            item_label(label, selected, tokens),
            ui::mono(library.count(media).to_string(), 12.0, tokens.sub),
        ];
        items = items.push(item(
            row,
            34.0,
            selected,
            tokens,
            Message::Library(library::Message::ShowType(media)),
        ));
    }

    // Plans opens New plan from its header; finished plans wait under All
    // plans.
    let on_plans = app.screen == Screen::Plans;
    let open_plan = if on_plans { app.plans.plan_id() } else { None };
    let new_plan = on_plans.then_some(Message::Plans(plans::Message::NewPlan));
    items = items.push(plans_header(tokens, new_plan));
    let shown = library
        .plans
        .iter()
        .filter(|plan| rules::plan_state(plan, app.today) != rules::PlanState::Finished);
    for plan in shown {
        let done = rules::done_count(plan);
        let total = plan.items.len();
        let year = plan.year.map(|year| year.to_string()).unwrap_or_default();
        let selected = open_plan == Some(plan.id.as_str());
        let weight = if selected {
            font::SEMIBOLD
        } else {
            font::MEDIUM
        };
        let title = row![
            text(&plan.name).size(14).font(weight),
            text(year).size(13).font(font::MEDIUM).color(tokens.sub),
            space::horizontal(),
            ui::mono(format!("{done}/{total}"), 11.5, tokens.sub),
        ]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Bottom);
        // On Plans the bars take each plan's own accent.
        let (track, fill) = if on_plans {
            (tokens.track, plans::plan_accent(app, plan, tokens))
        } else {
            (tokens.line, tokens.accent)
        };
        let bar = progress_bar(0.0..=total.max(1) as f32, done as f32)
            .girth(4)
            .style(move |_| progress_bar::Style {
                background: Background::Color(track),
                bar: Background::Color(fill),
                border: Border::default().rounded(2),
            });
        let content = column![title, bar].spacing(6);
        items = items.push(
            button(content)
                .width(Fill)
                .padding([7, 10])
                .on_press(Message::Plans(plans::Message::ShowPlan(plan.id.clone())))
                .style(ui::row_style(tokens, selected, 8.0)),
        );
    }
    let on_overview = on_plans && open_plan.is_none();
    items = items.push(more_row(
        "All plans",
        library.plans.len(),
        on_overview,
        tokens,
        Some(Message::Plans(plans::Message::ShowOverview)),
    ));

    // The tags filter the library, so they stay on its sidebar.
    if !on_plans {
        items = tags(app, items, tokens);
    }

    let column = column![ui::scroll(items).height(Fill), sync_row(app, tokens)];
    container(column)
        .width(232)
        .height(Fill)
        .padding(Padding::new(0.0).top(6).left(12).right(12))
        .into()
}

/// The sidebar's Tags: pinned or most used, then All tags.
fn tags<'a>(app: &'a App, mut items: Column<'a, Message>, tokens: &Tokens) -> Column<'a, Message> {
    let library = app.repo.library();
    let on_library = app.screen == Screen::Library;

    items = items.push(header("Tags", 20.0, tokens));
    let tag_counts = library.tag_counts();
    let state = &app.library;
    let sidebar_tags = state.sidebar_tags(&tag_counts);
    if sidebar_tags.is_empty() && state.sidebar_tags == library::SidebarTags::Pinned {
        items = items.push(
            container(
                text("Nothing pinned yet. Pin tags from All tags below.")
                    .size(12.5)
                    .color(tokens.sub),
            )
            .padding(Padding::new(0.0).top(2).bottom(6).left(10).right(10)),
        );
    }
    for (tag, count) in sidebar_tags {
        let selected = on_library && state.filter.tags.iter().any(|t| t == tag);
        let row = row![
            container(text("#").size(13).font(font::MONO_MEDIUM).color(tokens.ink)).width(9),
            item_label(tag, selected, tokens),
            ui::mono(count.to_string(), 12.0, tokens.sub),
        ];
        let message = Message::Library(library::Message::PickTag(tag.to_owned()));
        items = items.push(item(row, 32.0, selected, tokens, message));
    }
    let open = state.popup == Some(library::Popup::AllTags);
    let all_tags = more_row(
        "All tags",
        tag_counts.len(),
        false,
        tokens,
        Some(Message::Library(library::Message::Toggle(
            library::Popup::AllTags,
        ))),
    );
    items.push(
        Popover::new(all_tags, open.then(|| library::all_tags_popover(app)))
            .placement(Placement::Right)
            .gap(16.0)
            .on_dismiss(Message::Library(library::Message::ClosePopup)),
    )
}

/// "Plans", with the + for a new plan on the Plans screen.
fn plans_header<'a>(tokens: &Tokens, new_plan: Option<Message>) -> Element<'a, Message> {
    let mut content =
        row![ui::section_header("Plans", tokens), space::horizontal()].align_y(Center);
    if let Some(message) = new_plan {
        content = content.push(
            button(container(ui::icon(Icon::Plus, 14.0, tokens.sub)).center(Fill))
                .width(22)
                .height(22)
                .padding(0)
                .on_press(message)
                .style(ui::row_style(tokens, false, 6.0)),
        );
    }
    container(content)
        .height(48)
        .padding(Padding::new(0.0).top(20).bottom(6).left(10).right(10))
        .into()
}

fn header<'a>(label: &str, top: f32, tokens: &Tokens) -> Element<'a, Message> {
    container(ui::section_header(label, tokens))
        .padding(Padding::new(0.0).top(top).bottom(6).left(10).right(10))
        .into()
}

fn item_label<'a>(label: &'a str, selected: bool, tokens: &Tokens) -> Element<'a, Message> {
    let (color, weight) = if selected {
        (tokens.text, font::SEMIBOLD)
    } else {
        (tokens.sub, font::MEDIUM)
    };
    text(label)
        .size(14)
        .font(weight)
        .color(color)
        .width(Fill)
        .into()
}

/// One sidebar row: 34 px for types, 32 px for tags and the "All" rows.
fn item<'a>(
    content: iced::widget::Row<'a, Message>,
    height: f32,
    selected: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    button(content.spacing(10).align_y(Center).height(Fill))
        .width(Fill)
        .height(height)
        .padding([0, 10])
        .on_press(on_press)
        .style(ui::row_style(tokens, selected, 8.0))
        .into()
}

/// "All plans" and "All tags": the more icon, the label and the total.
fn more_row<'a>(
    label: &'a str,
    count: usize,
    selected: bool,
    tokens: &Tokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let content = row![
        ui::icon(Icon::More, 12.0, tokens.sub),
        item_label(label, selected, tokens),
        ui::mono(count.to_string(), 12.0, tokens.sub),
    ]
    .spacing(10)
    .align_y(Center)
    .height(Fill);
    button(content)
        .width(Fill)
        .height(32)
        .padding([0, 10])
        .on_press_maybe(on_press)
        .style(ui::row_style(tokens, selected, 8.0))
        .into()
}

/// The sync state and the Settings gear, pinned to the bottom. The gear
/// is marked while Settings is open.
fn sync_row<'a>(app: &App, tokens: &Tokens) -> Element<'a, Message> {
    let sync = app.sync;
    let (dot, label, color) = match sync {
        SyncState::Synced | SyncState::ExpiresIn(_) => {
            (theme::SYNC_OK, "Synced 2 min ago", tokens.sub)
        }
        SyncState::TokenExpired => (theme::SYNC_ERROR, "Token expired", tokens.text),
    };
    let mut content = row![
        ui::dot(dot, 8.0, 4.0),
        text(label)
            .size(12)
            .color(color)
            .wrapping(text::Wrapping::None)
            .width(Fill)
    ]
    .spacing(5)
    .align_y(Center);
    if sync == SyncState::TokenExpired {
        let ink = tokens.ink;
        content = content.push(
            button(
                text("Replace token")
                    .size(12)
                    .font(font::SEMIBOLD)
                    .wrapping(text::Wrapping::None),
            )
            .padding(0)
            .on_press(Message::Setup(setup::Message::ReplaceToken))
            .style(move |_, _| button::Style {
                text_color: ink,
                ..button::Style::default()
            }),
        );
    }
    let on_settings = app.screen == Screen::Settings;
    content = content.push(
        button(container(ui::icon(Icon::Settings, 18.0, tokens.sub)).center(Fill))
            .width(26)
            .height(26)
            .padding(0)
            .on_press(Message::Show(Screen::Settings))
            .style(ui::row_style(tokens, on_settings, 8.0)),
    );
    container(content)
        .height(40)
        .padding(Padding::new(0.0).left(6).right(2))
        .align_y(Center)
        .into()
}
