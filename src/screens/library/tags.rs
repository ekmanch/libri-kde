//! The tag picker dialog (from the Filter popover's "All tags…") and the
//! All tags popover (from the sidebar), which also pins tags.

use std::collections::HashMap;

use iced::widget::{Column, Row, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Listing, MAX_PINS, Message, TagPicker};
use crate::app::{App, Message as AppMessage};
use crate::model::{Library, TagCategory};
use crate::theme::{Tokens, font};
use crate::ui::{self, ButtonKind, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// The library's tags by category, in category order, A to Z within each,
/// with how many entries use them. Only tags matching `query`, when set; a
/// leading "#" is ignored.
fn groups<'a>(library: &'a Library, query: &str) -> Vec<(TagCategory, Vec<(&'a str, usize)>)> {
    let query = query.trim().trim_start_matches('#').to_lowercase();
    let counts: HashMap<&str, usize> = library.tag_counts().into_iter().collect();
    library
        .tags
        .iter()
        .filter_map(|(&category, names)| {
            let mut tags: Vec<(&str, usize)> = names
                .iter()
                .filter(|name| name.contains(&query))
                .map(|name| {
                    (
                        name.as_str(),
                        counts.get(name.as_str()).copied().unwrap_or(0),
                    )
                })
                .collect();
            tags.sort_by_key(|(name, _)| name.to_lowercase());
            (!tags.is_empty()).then_some((category, tags))
        })
        .collect()
}

/// The tag picker: a search across every group, collapsible groups (Genres
/// and groups with a picked tag start open; a search opens them all), then
/// Clear tags, a summary and Done.
pub fn picker<'a>(
    app: &'a App,
    picker: &'a TagPicker,
    listing: &Listing<'a>,
) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let picked = &app.library.filter.tags;
    let searching = !picker.query.trim().is_empty();

    let mut list = Column::new().spacing(2);
    let found = groups(app.repo.library(), &picker.query);
    if found.is_empty() {
        list = list.push(text("No tags match.").size(13.5).color(tokens.sub));
    }
    for (category, tags) in found {
        let selected = tags
            .iter()
            .filter(|(tag, _)| picked.iter().any(|p| p == tag))
            .count();
        let default_open = category == TagCategory::Genres || selected > 0;
        let open = searching || default_open != picker.toggled.contains(&category);
        let chips = tags
            .iter()
            .map(|&(tag, count)| {
                let on = picked.iter().any(|p| p == tag);
                ui::filter_chip(
                    format!("#{tag}"),
                    Some(count),
                    on,
                    &tokens,
                    msg(Message::ToggleTag(tag.to_owned())),
                )
            })
            .collect();
        list = list.push(ui::tag_group(
            category.name(),
            tags.len(),
            selected,
            open,
            msg(Message::TogglePickerGroup(category)),
            chips,
            &tokens,
        ));
    }

    let search = ui::search_field(
        "Search tags",
        &picker.query,
        |query| msg(Message::PickerSearch(query)),
        None,
        None,
        &tokens,
    );
    let body = column![search, ui::scroll(list).height(440)].spacing(12);

    let summary = if picked.is_empty() {
        "Shows entries with every selected tag".to_owned()
    } else {
        format!("{} selected · {} shown", picked.len(), listing.shown.len())
    };
    let done = if picked.is_empty() {
        "Done".to_owned()
    } else {
        format!("Show {}", listing.shown.len())
    };
    let footer = row![
        ui::labeled_button(
            "Clear tags",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::ClearTags))
        ),
        text(summary).size(13).color(tokens.sub).width(Fill),
        owned_button(done, &tokens, msg(Message::ClosePicker)),
    ]
    .spacing(10)
    .align_y(Center);
    ui::dialog(
        "Filter by tags",
        body,
        footer,
        560,
        &tokens,
        msg(Message::ClosePicker),
    )
}

/// A primary button whose label is built at runtime.
fn owned_button<'a>(
    label: String,
    tokens: &Tokens,
    on_press: AppMessage,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(
        text(label)
            .size(13.5)
            .font(font::SEMIBOLD)
            .height(Fill)
            .align_y(Center),
    )
    .height(34)
    .padding([0, 16])
    .on_press(on_press)
    .style(move |_, _| button::Style {
        background: Some(Background::Color(tokens.accent)),
        text_color: tokens.on_accent,
        border: Border::default().rounded(8),
        ..button::Style::default()
    })
    .into()
}

/// The sidebar's All tags popover: every tag by category with its count and
/// a pin; a click on a tag filters the library by it alone.
pub fn all_tags_popover<'a>(app: &'a App) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let library = app.repo.library();
    let total = library.tag_counts().len();

    let header = row![
        text("All tags").size(17).font(font::EXTRABOLD),
        ui::mono(total.to_string(), 12.0, tokens.sub),
        space::horizontal(),
        ui::icon_button(Icon::Close, 16.0, &tokens, msg(Message::ClosePopup)),
    ]
    .spacing(8)
    .align_y(Center)
    .padding([0, 4]);
    let search = ui::search_field(
        "Filter tags",
        &state.all_tags_query,
        |query| msg(Message::AllTagsSearch(query)),
        None,
        None,
        &tokens,
    );

    let full = state.pins.len() >= MAX_PINS;
    let mut list = Column::new();
    let found = groups(library, &state.all_tags_query);
    if found.is_empty() {
        list = list
            .push(container(text("No tags match.").size(13.5).color(tokens.sub)).padding([18, 8]));
    }
    for (category, tags) in found {
        list = list.push(
            container(
                row![
                    ui::section_header(category.name(), &tokens),
                    ui::mono(tags.len().to_string(), 11.0, tokens.sub)
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(Padding::new(0.0).top(16).bottom(6).left(8).right(8)),
        );
        for (tag, count) in tags {
            let pinned = state.pins.iter().any(|p| p == tag);
            list = list.push(tag_row(tag, count, pinned, full, &tokens));
        }
    }

    let footer = row![
        text(format!("{} of {MAX_PINS} pinned", state.pins.len()))
            .size(12.5)
            .color(tokens.sub)
            .width(Fill),
        ui::labeled_button(
            "Unpin all",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::UnpinAll))
        ),
    ]
    .align_y(Center)
    .padding(Padding::new(0.0).top(6).left(4).right(4));

    let panel = column![header, search, ui::scroll(list).height(Fill), footer].spacing(10);
    container(panel)
        .width(320)
        .height(560)
        .padding(Padding::new(12.0).top(14).bottom(10))
        .style(move |_| ui::panel_style(&tokens, 12.0))
        .into()
}

/// A tag with its count, and the pin button beside it.
fn tag_row<'a>(
    tag: &'a str,
    count: usize,
    pinned: bool,
    full: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let pick = button(
        row![
            text(format!("#{tag}")).size(14).width(Fill),
            ui::mono(count.to_string(), 12.0, tokens.sub)
        ]
        .spacing(10)
        .align_y(Center)
        .height(Fill),
    )
    .width(Fill)
    .height(34)
    .padding([0, 8])
    .on_press(msg(Message::PickFromAllTags(tag.to_owned())))
    .style(ui::row_style(tokens, false, 7.0));

    let tokens = *tokens;
    // Eight pinned: the other pins fade and do nothing.
    let disabled = full && !pinned;
    let color = match (pinned, disabled) {
        (true, _) => tokens.ink,
        (false, true) => tokens.sub.scale_alpha(0.35),
        (false, false) => tokens.sub,
    };
    let pin = button(container(ui::icon(Icon::Pin, 15.0, color)).center(Fill))
        .width(30)
        .height(30)
        .padding(0)
        .on_press_maybe((!disabled).then(|| msg(Message::TogglePin(tag.to_owned()))))
        .style(move |_, status| button::Style {
            background: if pinned {
                Some(Background::Color(tokens.selection))
            } else {
                matches!(status, button::Status::Hovered).then_some(Background::Color(tokens.hover))
            },
            border: Border::default().rounded(7),
            ..button::Style::default()
        });
    Row::new()
        .push(pick)
        .push(pin)
        .spacing(4)
        .align_y(Center)
        .into()
}
