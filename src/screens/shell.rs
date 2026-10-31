//! The window shell shared by Library, Plans and Settings: the sidebar on
//! the chrome, and the content pane with its rounded top-left corner.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use iced::widget::{
    Column, button, column, container, image, progress_bar, row, space, stack, text,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length, Padding};

use super::{Screen, library, plans, setup};
use crate::app::{App, Message, SyncMessage};
use crate::model::MediaType;
use crate::plans as rules;
use crate::settings::{Missing, SyncState};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, FocusExt, Icon, Placement, Popover};

/// The sidebar's width, so the content knows what is left of the window.
pub const SIDEBAR_WIDTH: f32 = 232.0;

/// How wide the content pane is, with a side pane `side` px wide open.
pub fn content_width(app: &App, side: f32) -> f32 {
    app.window.width - SIDEBAR_WIDTH - side
}

pub fn shell<'a>(app: &'a App, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let tokens = app.tokens();
    // A sync state that needs the owner gets a banner over the content on
    // Library and Plans; Offline and Saving fix themselves and stay in the
    // row.
    let content: Element<'a, Message> = match banner(app, &tokens) {
        Some(banner) if matches!(app.screen, Screen::Library | Screen::Plans) => {
            column![banner, container(content).height(Fill)].into()
        }
        _ => content.into(),
    };
    // The pane's surface: the content fill, with the top-left corner as
    // one 12 px tile that holds the chrome outside the arc and the content
    // inside it. The fills meet the tile on straight, whole-pixel edges, so
    // no two see-through fills ever overlap, not even by the width of an
    // antialiased edge; on Glass such an overlap stacked into a dark seam
    // along the corner. The chrome lies under the sidebar alone, never
    // under the pane.
    let fill = tokens.content;
    let content_fill = move |height: Length| {
        container(space())
            .width(Fill)
            .height(height)
            .style(move |_| container::background(fill))
    };
    let surface = column![
        row![
            image(corner_tile(tokens.chrome, tokens.content))
                .width(CORNER)
                .height(CORNER),
            content_fill(CORNER.into()),
        ],
        content_fill(Fill),
    ];
    let pane = stack![surface, container(content).width(Fill).height(Fill)];
    container(row![sidebar(app, &tokens), pane])
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style::default().color(tokens.text))
        .into()
}

/// The radius of the pane's top-left corner, in px.
const CORNER: f32 = 12.0;

/// The corner tile for a chrome and a content color: the arc's coverage of
/// each pixel is blended into that pixel's alpha and color, so the tile
/// composites exactly as the two fills would, with no seam between them.
/// Drawn at four times the size, so it stays smooth at every scale, and
/// kept per color pair, since a raw image gets a new id on every call.
fn corner_tile(chrome: Color, content: Color) -> image::Handle {
    const SIZE: usize = 48;
    const SAMPLES: usize = 4;
    type Tiles = HashMap<([u8; 4], [u8; 4]), image::Handle>;
    static TILES: OnceLock<Mutex<Tiles>> = OnceLock::new();
    let key = (chrome.into_rgba8(), content.into_rgba8());
    let mut tiles = TILES.get_or_init(Default::default).lock().unwrap();
    tiles
        .entry(key)
        .or_insert_with(|| {
            let radius = SIZE as f32;
            let mut pixels = Vec::with_capacity(SIZE * SIZE * 4);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    // The arc's center is the tile's bottom-right corner.
                    let inside = (0..SAMPLES * SAMPLES)
                        .filter(|&sample| {
                            let offset = |i: usize| (i as f32 + 0.5) / SAMPLES as f32;
                            let px = x as f32 + offset(sample % SAMPLES) - radius;
                            let py = y as f32 + offset(sample / SAMPLES) - radius;
                            px * px + py * py <= radius * radius
                        })
                        .count();
                    let coverage = inside as f32 / (SAMPLES * SAMPLES) as f32;
                    let (chrome_part, content_part) =
                        (chrome.a * (1.0 - coverage), content.a * coverage);
                    let alpha = chrome_part + content_part;
                    let mix = |from_chrome: f32, from_content: f32| {
                        if alpha > 0.0 {
                            (from_chrome * chrome_part + from_content * content_part) / alpha
                        } else {
                            0.0
                        }
                    };
                    let byte = |value: f32| (value * 255.0).round() as u8;
                    pixels.extend([
                        byte(mix(chrome.r, content.r)),
                        byte(mix(chrome.g, content.g)),
                        byte(mix(chrome.b, content.b)),
                        byte(alpha),
                    ]);
                }
            }
            image::Handle::from_rgba(SIZE as u32, SIZE as u32, pixels)
        })
        .clone()
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
            None => ("Home", tokens.sub),
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
                .style(ui::row_style(tokens, selected, 8.0))
                .focusable(8.0, tokens),
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
    let chrome = tokens.chrome;
    container(column)
        .width(SIDEBAR_WIDTH)
        .height(Fill)
        .padding(Padding::new(0.0).top(6).left(12).right(12))
        .style(move |_| container::background(chrome))
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
                .style(ui::row_style(tokens, false, 6.0))
                .focusable(6.0, tokens),
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

/// One sidebar row: 34 px for types, 32 px for tags and the "All plans" and "All tags" rows.
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
        .focusable(8.0, tokens)
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
        .focusable(8.0, tokens)
}

/// The sync row's parts for a state: the dot, the text, the line under
/// it, and the action's label and message.
fn sync_parts(
    app: &App,
    tokens: &Tokens,
) -> (iced::Color, String, String, Option<(&'static str, Message)>) {
    let changes = |n: u32| {
        format!(
            "{n} {} kept on this computer",
            if n == 1 { "change" } else { "changes" }
        )
    };
    match &app.sync {
        _ if app.kept_locally => (
            tokens.sub,
            "Not connected".into(),
            "Library kept on this computer".into(),
            None,
        ),
        SyncState::Synced | SyncState::ExpiresIn(_) => {
            (theme::SYNC_OK, app.synced_label(), String::new(), None)
        }
        SyncState::Saving { changes } => (
            tokens.accent,
            format!(
                "Saving {changes} {}…",
                if *changes == 1 { "change" } else { "changes" }
            ),
            String::new(),
            None,
        ),
        SyncState::Offline { kept } => (tokens.sub, "Offline".into(), changes(*kept), None),
        SyncState::Failed { kept } => (
            theme::SYNC_ERROR,
            "Could not save".into(),
            changes(*kept),
            Some(("Retry", Message::Sync(SyncMessage::Retry))),
        ),
        SyncState::NotFound(missing) => (
            theme::SYNC_ERROR,
            "Library not found".into(),
            match missing {
                Missing::File => "library.json is gone on GitHub".into(),
                Missing::Branch => "The branch is gone on GitHub".into(),
                Missing::Repo => "The repo is gone, or unseen".into(),
            },
            Some(("Fix", Message::Sync(SyncMessage::Fix))),
        ),
        SyncState::Conflict { .. } => (
            theme::SYNC_ERROR,
            "Sync conflict".into(),
            "Changed here and on GitHub".into(),
            Some(("Choose", Message::Sync(SyncMessage::OpenConflict))),
        ),
        SyncState::Unreadable { .. } => (
            theme::SYNC_ERROR,
            "Broken on GitHub".into(),
            "The file isn't a valid library".into(),
            Some(("Choose", Message::Sync(SyncMessage::OpenConflict))),
        ),
        SyncState::TokenExpired => (
            theme::SYNC_ERROR,
            "Token expired".into(),
            String::new(),
            Some((
                "Replace token",
                Message::Setup(setup::Message::ReplaceToken),
            )),
        ),
    }
}

/// The banner over the content for a state that needs the owner: its
/// title, text and action.
fn banner<'a>(app: &App, tokens: &Tokens) -> Option<Element<'a, Message>> {
    if app.kept_locally {
        return None;
    }
    let repo = app
        .source
        .as_ref()
        .map_or("the repository", |s| s.repo.as_str());
    let (title, body, action, message) = match app.sync {
        SyncState::Failed { kept } => (
            "Could not save",
            format!(
                "GitHub didn't answer. Your {kept} {} kept here and Libri tries again by itself.",
                if kept == 1 {
                    "change is"
                } else {
                    "changes are"
                }
            ),
            "Retry",
            Message::Sync(SyncMessage::Retry),
        ),
        SyncState::NotFound(missing) => (
            "Library not found on GitHub",
            format!(
                "{} Your edits stay here; pick the file again or restore it.",
                SyncState::missing_text(missing, app.source.as_ref())
            ),
            "Fix",
            Message::Sync(SyncMessage::Fix),
        ),
        SyncState::Conflict { .. } => (
            "Changed here and on GitHub",
            format!("Syncing with {repo} has stopped until you pick which copy to keep."),
            "Choose",
            Message::Sync(SyncMessage::OpenConflict),
        ),
        SyncState::Unreadable { .. } => (
            "GitHub's copy can't be read",
            format!(
                "{} on {repo} is no longer a valid library. Fix it there, or replace it with this computer's copy.",
                app.source
                    .as_ref()
                    .map_or("library.json", |s| s.file.as_str())
            ),
            "Choose",
            Message::Sync(SyncMessage::OpenConflict),
        ),
        _ => return None,
    };
    let (fill, line, danger) = (
        tokens.danger_fill,
        tokens.danger.scale_alpha(0.35),
        tokens.danger,
    );
    let content = row![
        ui::dot(danger, 9.0, 4.5),
        column![
            text(title).size(14).font(font::SEMIBOLD),
            text(body).size(12.5).color(tokens.sub),
        ]
        .spacing(1)
        .width(Fill),
        ui::sized_button(
            action,
            ui::ButtonKind::Outline,
            32.0,
            *tokens,
            Some(message)
        ),
    ]
    .spacing(12)
    .align_y(Center);
    Some(
        container(
            container(content)
                .width(Fill)
                .padding([10, 14])
                .style(move |_| {
                    container::background(fill)
                        .border(Border::default().rounded(10).width(1).color(line))
                }),
        )
        .padding(Padding::new(0.0).top(14).left(24).right(24))
        .into(),
    )
}

/// "Keep which copy?", over everything while a conflict is being decided;
/// "GitHub's copy can't be read" when that is what stopped the sync.
pub fn conflict_overlay<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let tokens = app.tokens();
    let dialog = app.conflict_open.then(|| match &app.sync {
        SyncState::Unreadable { kept, reason } => unreadable_dialog(app, *kept, reason, &tokens),
        _ => conflict_dialog(app, &tokens),
    });
    ui::modal(
        base,
        dialog,
        Message::Sync(SyncMessage::CloseConflict),
        &tokens,
    )
}

fn conflict_dialog<'a>(app: &App, tokens: &Tokens) -> Element<'a, Message> {
    let (local, remote) = match &app.sync {
        SyncState::Conflict {
            local_changes,
            remote_commit,
        } => (*local_changes, remote_commit.as_str()),
        _ => (0, ""),
    };
    let sides = row![
        text(format!(
            "This computer: {local} {}",
            if local == 1 { "change" } else { "changes" }
        ))
        .size(13)
        .color(tokens.sub)
        .width(Fill),
        text(format!("GitHub: last commit {remote}"))
            .size(13)
            .color(tokens.sub),
    ];
    let warning = row![
        ui::dot(tokens.danger, 9.0, 4.5),
        text("Whichever you keep, the other copy's changes are gone. Earlier versions stay in the repo's history.")
            .size(13)
            .color(tokens.text)
            .width(Fill),
    ]
    .spacing(10)
    .align_y(Center);
    let body = column![
        text("The library changed on GitHub and on this computer since they last matched.")
            .size(14),
        sides,
        warning,
    ]
    .spacing(12);
    let footer = row![
        ui::labeled_button(
            "Decide later",
            ui::ButtonKind::Text,
            *tokens,
            Some(Message::Sync(SyncMessage::CloseConflict))
        ),
        space::horizontal(),
        ui::labeled_button(
            "Keep GitHub's",
            ui::ButtonKind::Destructive,
            *tokens,
            Some(Message::Sync(SyncMessage::KeepTheirs))
        ),
        ui::labeled_button(
            "Keep mine",
            ui::ButtonKind::Neutral,
            *tokens,
            Some(Message::Sync(SyncMessage::KeepMine))
        ),
    ]
    .spacing(8)
    .align_y(Center);
    ui::dialog(
        "Keep which copy?",
        body,
        footer,
        520,
        tokens,
        Message::Sync(SyncMessage::CloseConflict),
    )
}

/// GitHub's copy is no library any more: the reason, and the ways out.
/// Keep mine is the neutral fill as in the conflict dialog; Check again
/// is for after a fix on GitHub.
fn unreadable_dialog<'a>(
    app: &App,
    kept: u32,
    reason: &str,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let (repo, file) = app
        .source
        .as_ref()
        .map_or(("the repository", "library.json"), |s| {
            (s.repo.as_str(), s.file.as_str())
        });
    let sides = row![
        text(format!(
            "This computer: {kept} {}",
            if kept == 1 { "change" } else { "changes" }
        ))
        .size(13)
        .color(tokens.sub)
        .width(Fill),
        text(format!("GitHub: {file} on {repo}"))
            .size(13)
            .color(tokens.sub),
    ];
    let warning = row![
        ui::dot(tokens.danger, 9.0, 4.5),
        text("Keep mine replaces the file on GitHub with this computer's copy; the edits made there are gone. Earlier versions stay in the repo's history.")
            .size(13)
            .color(tokens.text)
            .width(Fill),
    ]
    .spacing(10)
    .align_y(Center);
    let body = column![
        text("The file on GitHub is no longer a valid library, so it can't replace this computer's copy.")
            .size(14),
        ui::mono(reason.to_owned(), 12.5, tokens.sub),
        sides,
        warning,
    ]
    .spacing(12);
    let footer = row![
        ui::labeled_button(
            "Decide later",
            ui::ButtonKind::Text,
            *tokens,
            Some(Message::Sync(SyncMessage::CloseConflict))
        ),
        space::horizontal(),
        ui::labeled_button(
            "Check again",
            ui::ButtonKind::Outline,
            *tokens,
            Some(Message::Sync(SyncMessage::Retry))
        ),
        ui::labeled_button(
            "Keep mine",
            ui::ButtonKind::Neutral,
            *tokens,
            Some(Message::Sync(SyncMessage::KeepMine))
        ),
    ]
    .spacing(8)
    .align_y(Center);
    ui::dialog(
        "GitHub's copy can't be read",
        body,
        footer,
        520,
        tokens,
        Message::Sync(SyncMessage::CloseConflict),
    )
}

/// The sync state and the Settings gear, pinned to the bottom. The gear
/// is marked while Settings is open.
fn sync_row<'a>(app: &App, tokens: &Tokens) -> Element<'a, Message> {
    let (dot, label, sub, action) = sync_parts(app, tokens);
    let quiet = matches!(
        app.sync,
        SyncState::Synced | SyncState::ExpiresIn(_) | SyncState::Saving { .. }
    ) || app.kept_locally;
    let color = if quiet { tokens.sub } else { tokens.text };
    // The state, the action and the gear on one line; the line under it
    // runs the sidebar's full width, as in the mockup, so a long sub never
    // runs into the action.
    let mut line = row![
        ui::dot(dot, 8.0, 4.0),
        container(
            text(label)
                .size(12)
                .color(color)
                .wrapping(text::Wrapping::None)
        )
        .width(Fill)
        .clip(true),
    ]
    .spacing(8)
    .align_y(Center)
    .height(30);
    if let Some((action, message)) = action {
        let ink = tokens.ink;
        line = line.push(
            button(
                text(action)
                    .size(12)
                    .font(font::SEMIBOLD)
                    .wrapping(text::Wrapping::None),
            )
            .padding(0)
            .on_press(message)
            .style(move |_, _| button::Style {
                text_color: ink,
                ..button::Style::default()
            })
            .focusable(4.0, tokens),
        );
    }
    let on_settings = app.screen == Screen::Settings;
    line = line.push(
        button(container(ui::icon(Icon::Settings, 18.0, tokens.sub)).center(Fill))
            .width(26)
            .height(26)
            .padding(0)
            .on_press(Message::Show(Screen::Settings))
            .style(ui::row_style(tokens, on_settings, 8.0))
            .focusable(8.0, tokens),
    );
    let mut content = column![line];
    if !sub.is_empty() {
        content = content.push(
            container(
                text(sub)
                    .size(11)
                    .color(tokens.sub)
                    .wrapping(text::Wrapping::None),
            )
            .padding(Padding::new(0.0).left(16).bottom(4))
            .width(Fill)
            .clip(true),
        );
    }
    container(content)
        .padding(Padding::new(0.0).left(6).right(2).top(2))
        .into()
}
