//! The New and Edit plan dialog with its linked hashtags view, and the
//! phone's D4 (remove from a plan) and D5 (delete a plan).

use std::collections::HashMap;

use iced::widget::{Column, Row, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Message, PlanForm, TagView};
use crate::app::{App, Message as AppMessage};
use crate::model::{Plan, TagCategory};
use crate::plans::{self as rules};
use crate::theme::{Tokens, font};
use crate::ui::{self, ButtonKind, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Plans(message)
}

pub fn view<'a>(app: &'a App, form: &'a PlanForm) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    if let Some(tags) = &form.tag_view {
        return tag_view(app, form, tags, &tokens);
    }
    let title = if form.editing.is_some() {
        "Edit plan"
    } else {
        "New plan"
    };
    let mut body = Column::new().spacing(18);

    let name = ui::labeled(
        "Name",
        ui::text_field(
            "e.g. Halloween, Road trip Texas",
            &form.name,
            |n| msg(Message::Name(n)),
            font::REGULAR,
            &tokens,
        ),
        &tokens,
    );
    let mut year = column![
        label("Year", true, &tokens),
        ui::text_field(
            "2027",
            &form.year,
            |y| msg(Message::Year(y)),
            font::MONO,
            &tokens
        ),
    ]
    .spacing(6)
    .width(120);
    if !form.year_is_valid() {
        year = year.push(text("A four-digit year.").size(12.5).color(tokens.danger));
    }
    body = body.push(row![container(name).width(Fill), year].spacing(12));

    if form.editing.is_some() {
        let help = if form.started {
            "Active. Stays active after its end date until every entry is done or removed."
        } else {
            "Not started. Turns on by itself when the plan's first date comes."
        };
        body = body.push(ui::setting_switch(
            "Started",
            help,
            form.started,
            &tokens,
            msg(Message::Started(!form.started)),
        ));
    }

    let mut chips = Row::new().spacing(6).align_y(Center);
    for tag in &form.tags {
        chips = chips.push(ui::removable_chip(
            format!("#{tag}"),
            None,
            &tokens,
            msg(Message::TagToggle(tag.clone())),
        ));
    }
    let open = if form.tags.is_empty() {
        "Choose from your tags ›"
    } else {
        "Edit tags ›"
    };
    chips = chips.push(ui::add_chip(open, &tokens, msg(Message::OpenTags)));
    body = body.push(
        column![
            label("Linked hashtags", true, &tokens),
            chips.wrap().vertical_spacing(6),
            help_text(
                "Adds an \"All\" view with everything in the library that has any of these tags. \
                 Leave empty for a free-form plan.",
                &tokens
            ),
        ]
        .spacing(8),
    );

    if let Some(season) = rules::season_for_tags(&form.tags) {
        let help = if form.seasonal {
            season.help()
        } else {
            "This plan looks like any other plan."
        };
        body = body.push(ui::setting_switch(
            "Seasonal theme",
            help,
            form.seasonal,
            &tokens,
            msg(Message::Seasonal(!form.seasonal)),
        ));
    }

    if form.editing.is_none() {
        body = body.push(start_with(app, form, &tokens));
    }

    let timeline_help = if form.timeline {
        "Entries sit on a timeline, each with a day or a period."
    } else {
        "A plain checklist without dates."
    };
    body = body.push(ui::setting_switch(
        "Timeline",
        timeline_help,
        form.timeline,
        &tokens,
        msg(Message::Timeline(!form.timeline)),
    ));

    let mut footer = Row::new().spacing(10).align_y(Center);
    if form.editing.is_some() {
        footer = footer.push(ui::labeled_button(
            "Delete plan",
            ButtonKind::Destructive,
            tokens,
            Some(msg(Message::DeletePlan)),
        ));
    }
    let save = if form.editing.is_some() {
        "Save"
    } else {
        "Create"
    };
    footer = footer
        .push(space::horizontal())
        .push(ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::Close)),
        ))
        .push(ui::labeled_button(
            save,
            ButtonKind::Primary,
            tokens,
            form.can_save().then(|| msg(Message::Save)),
        ));
    ui::dialog(
        title,
        ui::scroll(body).height(iced::Shrink),
        footer,
        560,
        &tokens,
        msg(Message::Close),
    )
}

/// A field label, with "(optional)" in gray.
fn label<'a>(name: &'a str, optional: bool, tokens: &Tokens) -> Element<'a, AppMessage> {
    let mut label = Row::new()
        .spacing(4)
        .push(text(name).size(13).font(font::SEMIBOLD).color(tokens.text));
    if optional {
        label = label.push(text("(optional)").size(13).color(tokens.sub));
    }
    label.into()
}

fn help_text<'a>(value: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(value).size(12.5).color(tokens.sub).into()
}

/// Start with Empty or a copy of a plan, with the plans to pick from.
fn start_with<'a>(app: &'a App, form: &'a PlanForm, tokens: &Tokens) -> Element<'a, AppMessage> {
    let modes = [
        ("Empty", !form.copy, msg(Message::StartWith { copy: false })),
        (
            "Copy a plan",
            form.copy,
            msg(Message::StartWith { copy: true }),
        ),
    ];
    let mut column = Column::new()
        .spacing(8)
        .push(text("Start with").size(13).font(font::SEMIBOLD))
        .push(container(ui::segmented(modes, 32.0, tokens)).width(230));
    if form.copy {
        let sources =
            Column::with_children(app.repo.library().plans.iter().map(|plan| {
                source_row(plan, form.source.as_deref() == Some(&plan.id), app, tokens)
            }));
        let line = tokens.line;
        column = column.push(container(sources).padding(4).style(move |_| {
            container::Style::default().border(Border::default().rounded(10).width(1).color(line))
        }));
    }
    let help = if form.copy {
        "Entries you had not finished in that plan are copied in."
    } else {
        "You add entries yourself afterwards."
    };
    column.push(help_text(help, tokens)).into()
}

/// A plan to copy: a radio, its title and its state.
fn source_row<'a>(plan: &'a Plan, on: bool, app: &App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let accent = tokens.accent;
    let ring = if on { tokens.accent } else { tokens.sub };
    let radio = container(
        container(space())
            .width(8)
            .height(8)
            .style(move |_| container::Style {
                background: on.then_some(Background::Color(accent)),
                border: Border::default().rounded(4),
                ..container::Style::default()
            }),
    )
    .center(16)
    .style(move |_| {
        container::Style::default().border(Border::default().rounded(8).width(2).color(ring))
    });
    button(
        row![
            radio,
            text(plan.title()).size(14).width(Fill),
            text(rules::plan_state(plan, app.today).label())
                .size(12.5)
                .color(tokens.sub),
        ]
        .spacing(10)
        .align_y(Center)
        .height(Fill),
    )
    .width(Fill)
    .height(40)
    .padding([0, 10])
    .on_press(msg(Message::Source(plan.id.clone())))
    .style(ui::row_style(tokens, on, 7.0))
    .into()
}

/// The linked hashtags, drilled into from the form: a search, the groups
/// (Season open first), Clear, a summary and Done.
fn tag_view<'a>(
    app: &'a App,
    form: &'a PlanForm,
    view: &'a TagView,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let query = view.query.trim().trim_start_matches('#').to_lowercase();
    let searching = !query.is_empty();
    let counts: HashMap<&str, usize> = library.tag_counts().into_iter().collect();

    let mut list = Column::new().spacing(2);
    let mut any = false;
    for category in TagCategory::ALL {
        let mut names: Vec<&str> = library
            .tags
            .get(&category)
            .into_iter()
            .flatten()
            .map(String::as_str)
            .filter(|n| n.contains(&query))
            .collect();
        if names.is_empty() {
            continue;
        }
        any = true;
        names.sort_by_key(|n| n.to_lowercase());
        let selected = names
            .iter()
            .filter(|n| form.tags.iter().any(|t| t == *n))
            .count();
        let open =
            searching || ((category == TagCategory::Season) != view.toggled.contains(&category));
        let chips = names
            .iter()
            .map(|&name| {
                ui::filter_chip(
                    format!("#{name}"),
                    None,
                    Some(counts.get(name).copied().unwrap_or(0)),
                    form.tags.iter().any(|t| t == name),
                    tokens,
                    msg(Message::TagToggle(name.to_owned())),
                )
            })
            .collect();
        list = list.push(ui::tag_group(
            category.name(),
            names.len(),
            selected,
            open,
            msg(Message::TagToggleGroup(category)),
            chips,
            tokens,
        ));
    }
    if !any {
        list = list.push(text("No tags match.").size(13.5).color(tokens.sub));
    }
    let search = ui::search_field(
        "Search tags",
        &view.query,
        |q| msg(Message::TagQuery(q)),
        None,
        None,
        tokens,
    );
    let back = {
        let tokens = *tokens;
        button(container(ui::icon(Icon::ChevronLeft, 18.0, tokens.text)).center(Fill))
            .width(32)
            .height(32)
            .padding(0)
            .on_press(msg(Message::CloseTags))
            .style(ui::row_style(&tokens, false, 8.0))
    };
    let form_title = if form.editing.is_some() {
        "Edit plan"
    } else {
        "New plan"
    };
    let heading = row![
        back,
        text("Linked hashtags").size(19).font(font::EXTRABOLD),
        text(form_title).size(13).color(tokens.sub),
        space::horizontal(),
        ui::icon_button(Icon::Close, 16.0, tokens, msg(Message::Close)),
    ]
    .spacing(8)
    .align_y(Center);
    let body = column![search, ui::scroll(list).height(400)].spacing(12);

    let summary = if form.tags.is_empty() {
        "No tags selected".to_owned()
    } else {
        let names: Vec<String> = form.tags.iter().map(|t| format!("#{t}")).collect();
        format!("{} selected: {}", form.tags.len(), names.join(", "))
    };
    let footer = row![
        ui::labeled_button(
            "Clear",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::TagClear))
        ),
        text(summary).size(13).color(tokens.sub).width(Fill),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::CloseTags))
        ),
    ]
    .spacing(10)
    .align_y(Center);
    ui::panel(heading, body, footer, 560, tokens)
}

/// D5: "Delete Halloween 2025?"
pub fn delete<'a>(app: &'a App, id: &str) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let Some(plan) = app.repo.library().plan(id) else {
        return space().into();
    };
    let note = match plan.items.len() {
        0 => "It has no entries yet.".to_owned(),
        1 => "Its entry stays in your library.".to_owned(),
        n => format!("Its {n} entries stay in your library."),
    };
    let body = column![
        text("The plan and its checkmarks are removed.").size(14),
        text(note).size(13).color(tokens.sub),
    ]
    .spacing(10);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::Close))
        ),
        ui::labeled_button(
            "Delete plan",
            ButtonKind::Destructive,
            tokens,
            Some(msg(Message::ConfirmDeletePlan))
        ),
    ]
    .spacing(8);
    ui::dialog(
        format!("Delete {}?", plan.title()),
        body,
        footer,
        460,
        &tokens,
        msg(Message::Close),
    )
}

/// D4: "Remove Dracula?" from this plan, or delete it everywhere.
pub fn remove<'a>(app: &'a App, plan: &str, entry: &str) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let (Some(plan), Some(entry)) = (library.plan(plan), library.entry(entry)) else {
        return space().into();
    };
    let body = text(format!(
        "It is in the plan {} and in your library.",
        plan.title()
    ))
    .size(14);
    let wide =
        |label: &'a str, kind, message| ui::wide_button(label, kind, tokens, Some(msg(message)));
    let footer = column![
        wide(
            "Remove from this plan",
            ButtonKind::Neutral,
            Message::RemoveFromPlan
        ),
        wide(
            "Delete everywhere",
            ButtonKind::Destructive,
            Message::DeleteEverywhere
        ),
        wide("Cancel", ButtonKind::Text, Message::Close),
    ]
    .spacing(8)
    .width(Fill)
    .padding(Padding::new(0.0).top(4));
    ui::dialog(
        format!("Remove {}?", entry.title),
        body,
        footer,
        420,
        &tokens,
        msg(Message::Close),
    )
}
