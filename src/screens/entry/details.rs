//! The details pane beside the list, shown while an entry is selected:
//! cover, type, title, creator, released, status, rating, tags, note,
//! about, plans, then Edit and Delete. Inside a plan it also shows the
//! entry's place in it (when, and the plan's note), and Delete asks
//! whether to remove it from the plan only.

use iced::widget::{Column, button, column, container, row, text};
use iced::{Background, Border, Color, Element, Fill, Padding};

use super::Message;
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::lookup;
use crate::model::{Entry, EntryStatus, MediaType, Plan};
use crate::music;
use crate::screens::plans;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Align, ButtonKind, FocusExt, Icon, Popover};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// The pane is this wide.
pub const WIDTH: f32 = 352.0;

pub fn view<'a>(app: &'a App, entry: &'a Entry, plan: Option<&'a Plan>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let id = || entry.id.clone();
    let color = tokens.type_color(entry.media);

    let tint = theme::cover_color(&entry.id);
    let cover = container(
        text(&entry.title)
            .size(14)
            .font(font::EXTRABOLD)
            .color(Color::WHITE),
    )
    .width(104)
    .height(156)
    .padding(10)
    .align_y(iced::alignment::Vertical::Bottom)
    .style(move |_| container::background(tint).border(Border::default().rounded(8)));
    let mut facts = Column::new()
        .spacing(6)
        .padding(Padding::new(0.0).top(2))
        .push(ui::mono(
            browse::kind_label(entry).to_uppercase(),
            11.0,
            color,
        ))
        .push(text(&entry.title).size(24).font(font::EXTRABOLD));
    if let Some(creator) = &entry.creator {
        facts = facts.push(text(creator).size(14).color(tokens.sub));
    }
    if let Some(album) = &entry.album {
        facts = facts.push(text(album).size(14).font(font::ITALIC).color(tokens.sub));
    }
    if let Some(released) = entry.first_released() {
        facts = facts.push(ui::mono(released.to_string(), 12.5, tokens.sub));
    }
    let header = row![cover, facts.width(Fill)].spacing(16);

    let statuses = EntryStatus::options_for(entry.media).iter().map(|&status| {
        let label = browse::status_label(Some(entry.media), status);
        (
            label,
            entry.status == status,
            msg(Message::SetStatus(id(), status)),
        )
    });
    let status = ui::segmented(statuses, 32.0, &tokens);

    let rating = ui::rating(
        entry.rating,
        app.library.details_rating_open,
        msg(Message::ToggleDetailsRating),
        move |rating| msg(Message::SetRating(entry.id.clone(), rating)),
        &tokens,
    );

    // A tag in several categories shows its roles here, merged.
    let library = app.repo.library();
    let mut tags = iced::widget::Row::new().spacing(6);
    for tag in &entry.tags {
        let roles = library.roles_on(entry, tag);
        let detail = (!roles.is_empty()).then(|| {
            let roles: Vec<&str> = roles.iter().map(|c| c.role_label()).collect();
            roles.join(", ")
        });
        tags = tags.push(ui::tag_chip(format!("#{tag}"), detail, &tokens));
    }
    let tags = tags
        .push(ui::add_chip("+ Tag", &tokens, msg(Message::EditTags(id()))))
        .wrap()
        .vertical_spacing(6);

    // The note is edited in the form; the box opens it.
    let (note, note_color) = if entry.note.is_empty() {
        ("Add a personal note…", tokens.sub)
    } else {
        (entry.note.as_str(), tokens.text)
    };
    let note = button(text(note).size(14).font(font::ITALIC).color(note_color))
        .width(Fill)
        .padding([9, 12])
        .on_press(msg(Message::Edit(id())))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(tokens.field)),
            border: Border::default().rounded(8).width(1).color(tokens.line),
            ..button::Style::default()
        })
        .focusable(8.0, &tokens);

    let plans: Vec<_> = library
        .plans
        .iter()
        .filter(|p| p.item_for(&entry.id).is_some())
        .collect();
    let mut in_plans = Column::new().spacing(6);
    if plans.is_empty() {
        in_plans = in_plans.push(text("Not in any plan").size(14).color(tokens.sub));
    }
    for plan in plans {
        let ink = tokens.ink;
        in_plans = in_plans.push(
            button(text(format!("{} →", plan.title())).size(14))
                .padding(0)
                .on_press(AppMessage::Plans(plans::Message::ShowPlan(plan.id.clone())))
                .style(move |_, _| button::Style {
                    text_color: ink,
                    ..button::Style::default()
                })
                .focusable(4.0, &tokens),
        );
    }
    in_plans = in_plans.push(ui::add_chip(
        "+ Add to plan",
        &tokens,
        msg(Message::AddToPlan(id())),
    ));

    let mut sections = Column::new().spacing(20).push(header);
    if let Some(plan) = plan {
        sections = sections.push(section(
            "This plan",
            in_plan(app, plan, entry, &tokens),
            &tokens,
        ));
    }
    sections = sections
        .push(section("Status", status, &tokens))
        .push(section("Rating", rating, &tokens));
    if entry.media == MediaType::Music && !music::is_song(entry) {
        sections = sections.push(tracks(app, entry, &tokens));
    }
    sections = sections
        .push(section("Tags", tags.into(), &tokens))
        .push(section("Note", note, &tokens));
    if !entry.description.is_empty() {
        let about = text(&entry.description)
            .size(14)
            .color(tokens.sub)
            .line_height(1.55);
        sections = sections.push(section("About", about.into(), &tokens));
    }
    sections = sections.push(section("In plans", in_plans.into(), &tokens));

    // Inside a plan, Delete asks: this plan only, or everywhere (D4).
    let delete = match plan {
        Some(_) => AppMessage::Plans(plans::Message::RemoveItem(id())),
        None => msg(Message::Delete(id())),
    };
    let actions = row![
        ui::wide_button(
            "Edit",
            ButtonKind::Neutral,
            tokens,
            Some(msg(Message::Edit(id())))
        ),
        ui::labeled_button("Delete", ButtonKind::Destructive, tokens, Some(delete)),
    ]
    .spacing(8);

    let content_bg = tokens.content;
    container(
        column![
            ui::scroll(container(sections).padding(Padding::new(22.0).bottom(8))).height(Fill),
            container(actions).padding(Padding::new(22.0).top(6).bottom(18)),
        ]
        .height(Fill),
    )
    .width(WIDTH)
    .height(Fill)
    .style(move |_| container::background(content_bg))
    .into()
}

/// The entry in the open plan: its When (a day or a period, on a
/// timeline plan) and the plan's own note for it.
fn in_plan<'a>(
    app: &'a App,
    plan: &'a Plan,
    entry: &'a Entry,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let Some(item) = plan.item_for(&entry.id) else {
        return text("Not in this plan").size(14).color(tokens.sub).into();
    };
    let mut column = Column::new().spacing(10);
    if plan.timeline {
        let open = app.plans.when_open;
        let (label, color) = match item.when {
            Some(when) => (crate::plans::format_when(when, plan.year), tokens.text),
            None => ("Whenever".to_owned(), tokens.sub),
        };
        let tokens_copy = *tokens;
        let field = button(
            row![
                ui::icon(Icon::Calendar, 15.0, tokens.sub),
                text(label).size(14).color(color).width(Fill),
            ]
            .spacing(8)
            .align_y(iced::Center)
            .height(Fill),
        )
        .width(Fill)
        .height(36)
        .padding([0, 10])
        .on_press(AppMessage::Plans(plans::Message::ToggleWhen))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(tokens_copy.field)),
            border: Border::default()
                .rounded(8)
                .width(1)
                .color(tokens_copy.line),
            ..button::Style::default()
        })
        .focusable(8.0, tokens);
        let calendar = open.then(|| {
            let picked = ui::calendar::calendar(
                plans::shown_month(app),
                item.when,
                app.today,
                tokens.accent,
                tokens.on_accent,
                |day| AppMessage::Plans(plans::Message::PickDay(day)),
                |by| AppMessage::Plans(plans::Message::Month(by)),
                tokens,
            );
            let hint = if app.plans.anchor.is_some() {
                "Click a later day for a period."
            } else {
                "Click a day, then a later one for a period."
            };
            let footer = row![
                text(hint).size(12).color(tokens_copy.sub).width(Fill),
                ui::labeled_button(
                    "Clear",
                    ButtonKind::Text,
                    tokens_copy,
                    item.when
                        .is_some()
                        .then_some(AppMessage::Plans(plans::Message::ClearWhen))
                ),
            ]
            .spacing(8)
            .align_y(iced::Center);
            container(column![picked, footer].spacing(6).width(252))
                .padding(12)
                .style(move |_| ui::panel_style(&tokens_copy, 12.0))
                .into()
        });
        let when = Popover::new(field, calendar)
            .align(Align::Start)
            .on_dismiss(AppMessage::Plans(plans::Message::ToggleWhen));
        column = column.push(labeled("When", when.into(), tokens));
    }
    let note = ui::text_field(
        "A note for this plan",
        &item.note,
        |note| AppMessage::Plans(plans::Message::ItemNote(note)),
        font::REGULAR,
        tokens,
    );
    column
        .push(labeled("Plan note", note.into(), tokens))
        .into()
}

/// A small label over a control.
fn labeled<'a>(
    label: &'a str,
    control: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![text(label).size(12.5).color(tokens.sub), control]
        .spacing(5)
        .into()
}

/// An album's tracks: the track list with a rating per track, rated
/// tracks the list lacks after it. Only rated tracks are stored.
fn tracks<'a>(app: &'a App, album: &'a Entry, tokens: &Tokens) -> Element<'a, AppMessage> {
    let rated = album.tracks.as_deref().unwrap_or_default();
    let rating_of = |title: &str| {
        rated
            .iter()
            .find(|t| t.title.eq_ignore_ascii_case(title))
            .map(|t| t.rating)
    };
    let listed = lookup::track_list(music::artist_of(album), &album.title).unwrap_or_default();
    let extra = rated
        .iter()
        .filter(|t| !listed.iter().any(|l| l.eq_ignore_ascii_case(&t.title)))
        .map(|t| t.title.as_str());
    let open = app.library.track_rating_open.as_deref();

    let mut rows = Column::new();
    for (index, title) in listed.iter().copied().chain(extra).enumerate() {
        let number = if index < listed.len() {
            (index + 1).to_string()
        } else {
            String::new()
        };
        let picker = ui::track_rating(
            rating_of(title),
            open == Some(title),
            msg(Message::ToggleTrackRating(title.to_owned())),
            move |rating| {
                msg(Message::RateTrack {
                    album: album.id.clone(),
                    title: title.to_owned(),
                    rating,
                })
            },
            tokens,
        );
        rows = rows.push(
            row![
                container(ui::mono(number, 12.0, tokens.sub)).width(22),
                container(text(title).size(14).wrapping(text::Wrapping::None))
                    .width(Fill)
                    .clip(true),
                picker,
            ]
            .spacing(8)
            .align_y(iced::Center)
            .height(36),
        );
    }
    let note = if listed.is_empty() {
        Some("No track list was found for this album.")
    } else if rated.is_empty() {
        Some("No tracks rated yet.")
    } else {
        None
    };
    let mut body = Column::new().spacing(6);
    if let Some(note) = note {
        body = body.push(text(note).size(13).color(tokens.sub));
    }
    body = body.push(rows);
    let header = row![
        ui::section_header("Tracks", tokens),
        iced::widget::space::horizontal(),
        text(format!("{} rated", rated.len()))
            .size(12.5)
            .color(tokens.sub),
    ];
    column![header, body].spacing(8).into()
}

fn section<'a>(
    label: &str,
    body: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![ui::section_header(label, tokens), body]
        .spacing(8)
        .into()
}
