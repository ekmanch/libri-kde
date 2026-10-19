//! The details pane beside the list, shown while an entry is selected:
//! cover, type, title, creator, released, status, rating, tags, note,
//! about, plans, then Edit and Delete.

use iced::widget::{Column, button, column, container, row, text};
use iced::{Background, Border, Color, Element, Fill, Padding};

use super::Message;
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Entry, EntryStatus};
use crate::screens::Screen;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// The pane is this wide.
pub const WIDTH: f32 = 352.0;

pub fn view<'a>(app: &'a App, entry: &'a Entry) -> Element<'a, AppMessage> {
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
    if let Some(released) = entry.released {
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

    let mut tags = iced::widget::Row::new().spacing(6);
    for tag in &entry.tags {
        tags = tags.push(ui::tag_chip(format!("#{tag}"), &tokens));
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
        });

    let library = app.repo.library();
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
                .on_press(AppMessage::Show(Screen::Plans))
                .style(move |_, _| button::Style {
                    text_color: ink,
                    ..button::Style::default()
                }),
        );
    }
    in_plans = in_plans.push(ui::add_chip(
        "+ Add to plan",
        &tokens,
        msg(Message::AddToPlan(id())),
    ));

    let mut sections = Column::new()
        .spacing(20)
        .push(header)
        .push(section("Status", status, &tokens))
        .push(section("Rating", rating, &tokens))
        .push(section("Tags", tags.into(), &tokens))
        .push(section("Note", note.into(), &tokens));
    if !entry.description.is_empty() {
        let about = text(&entry.description)
            .size(14)
            .color(tokens.sub)
            .line_height(1.55);
        sections = sections.push(section("About", about.into(), &tokens));
    }
    sections = sections.push(section("In plans", in_plans.into(), &tokens));

    let actions = row![
        ui::wide_button(
            "Edit",
            ButtonKind::Neutral,
            tokens,
            Some(msg(Message::Edit(id())))
        ),
        ui::labeled_button(
            "Delete",
            ButtonKind::Destructive,
            tokens,
            Some(msg(Message::Delete(id())))
        ),
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

fn section<'a>(
    label: &str,
    body: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![ui::section_header(label, tokens), body]
        .spacing(8)
        .into()
}
