//! The tag picker dialog (from the Filter popover's "All tags…") and the
//! All tags popover (from the sidebar), which also pins tags.

use std::collections::HashMap;
use std::rc::Rc;

use iced::widget::{Column, Row, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Listing, MAX_PINS, Message};
use crate::app::{App, Message as AppMessage};
use crate::model::{Library, TagCategory, sort_tags};
use crate::tags::{self, PickerState};
use crate::theme::{Tokens, font};
use crate::ui::{self, FocusExt, Icon, Picker};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// The library's tags by category, in category order, A to Z within each,
/// with how many entries use them. Only tags matching `query`, when set; a
/// leading "#" is ignored. A tag in several categories is under each.
fn groups<'a>(library: &'a Library, query: &str) -> Vec<(TagCategory, Vec<(&'a str, usize)>)> {
    let query = query.trim().trim_start_matches('#').to_lowercase();
    let counts: HashMap<&str, usize> = library.tag_counts().into_iter().collect();
    library
        .tags
        .iter()
        .filter_map(|(&category, names)| {
            let tags: Vec<(&str, usize)> = sort_tags(
                names
                    .iter()
                    .map(String::as_str)
                    .filter(|n| n.contains(&query)),
            )
            .into_iter()
            .map(|name| (name, counts.get(name).copied().unwrap_or(0)))
            .collect();
            (!tags.is_empty()).then_some((category, tags))
        })
        .collect()
}

/// The long-group threshold: the phone's, or three with `LIBRI_LONG_LISTS`
/// so the short fixture shows a long group.
pub fn long_group(app: &App) -> usize {
    if app.library.force_long {
        3
    } else {
        tags::LONG_GROUP
    }
}

/// The tag picker: a search across every group, groups that fold (those
/// with a picked tag start open), a long group as a row, then Clear tags,
/// a summary and Done.
pub fn picker<'a>(
    app: &'a App,
    state: &'a PickerState,
    listing: &Listing<'a>,
) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let filter = &app.library.filter;
    let types = &app.library.types;

    let picker = Picker {
        state,
        groups: library
            .tags
            .iter()
            .map(|(&c, names)| (c, names.clone()))
            .collect(),
        counts: library.tag_counts().into_iter().collect(),
        categories_of: Box::new(|tag| library.categories_of(tag)),
        is_on: Box::new(|tag, category| filter.has_tag(tag, category)),
        // Game genres only with Games among the types, Actors and Streaming
        // with Movies or Shows; everything with no type chosen.
        visible: Box::new(|category| {
            types.is_empty() || types.iter().any(|&media| category.offered_for(media))
        }),
        default_open: Box::new(|_, selected| selected > 0),
        long_group: long_group(app),
        with_menu: false,
        with_add_to: false,
        height: 440.0,
        map: Rc::new(|message| msg(Message::Picker(message))),
    };

    let chips = filter.tag_chips().len();
    let summary = if chips == 0 {
        "Shows entries that have all selected tags".to_owned()
    } else {
        format!("{chips} selected · {} shown", listing.shown.len())
    };
    let done = if chips == 0 {
        "Done".to_owned()
    } else {
        format!("Show {}", listing.shown.len())
    };
    let header = picker.header(
        "Filter by tags",
        None,
        None,
        msg(Message::ClosePicker),
        &tokens,
    );
    let body = picker.body("Search tags", None, &tokens);
    let footer = ui::tag_picker::footer(
        "Clear tags",
        Some(msg(Message::ClearTags)),
        summary,
        done,
        msg(Message::ClosePicker),
        &tokens,
    );
    ui::panel(header, body, footer, 560, &tokens)
}

/// The sidebar's All tags popover: every tag by category with its count and
/// a pin; a click on a tag filters the library by it alone (as the category
/// it is listed under, for a tag in several).
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
            let key = library.is_multi_category(tag).then_some(category);
            list = list.push(tag_row(tag, key, count, pinned, full, &tokens));
        }
    }

    let footer = row![
        text(format!("{} of {MAX_PINS} pinned", state.pins.len()))
            .size(12.5)
            .color(tokens.sub)
            .width(Fill),
        ui::labeled_button(
            "Unpin all",
            ui::ButtonKind::Outline,
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
    key: Option<TagCategory>,
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
    .on_press(msg(Message::PickFromAllTags(tag.to_owned(), key)))
    .style(ui::row_style(tokens, false, 7.0))
    .focusable(7.0, tokens);

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
        })
        .focusable(7.0, &tokens);
    Row::new()
        .push(pick)
        .push(pin)
        .spacing(4)
        .align_y(Center)
        .into()
}
