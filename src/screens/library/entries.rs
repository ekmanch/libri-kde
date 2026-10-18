//! The list itself: the entries in the chosen view, grouped by type when
//! everything is shown, or an empty state.

use iced::widget::{Column, column, container, grid, row, text};
use iced::{Element, Fill, Padding};

use super::{Listing, Message, View};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Entry, MediaType, MusicKind};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, EntryLook, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// Grid cells: at most this wide, 2:3 covers with the caption under them.
const CARD_MAX_WIDTH: f32 = 190.0;
const CARD_WIDTH: f32 = 173.0;
const CARD_HEIGHT: f32 = CARD_WIDTH * 1.5 + 28.0;

pub fn body<'a>(app: &'a App, listing: &Listing<'a>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    if let Some(empty) = empty_state(app, listing, &tokens) {
        return empty;
    }
    let state = &app.library;
    let view = state.view_mode();
    let mut content = Column::new().spacing(if view == View::Text { 0 } else { 2 });
    if view == View::Text {
        let creator = browse::creator_label(state.media);
        content = content.push(ui::entry_table_header(
            ["Title", creator, "Released", "Status", "Rating", "Tags"],
            &tokens,
        ));
    }
    match state.media {
        Some(_) => content = content.push(block(app, &listing.shown, view, &tokens)),
        // Everything: one section per type, in type order.
        None => {
            for media in MediaType::ALL {
                let entries: Vec<&Entry> = listing
                    .shown
                    .iter()
                    .copied()
                    .filter(|e| e.media == media)
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                content = content
                    .push(section_header(media, entries.len(), &tokens))
                    .push(block(app, &entries, view, &tokens));
            }
        }
    }
    ui::scroll(container(content).padding(Padding::new(24.0).top(4)))
        .height(Fill)
        .into()
}

/// A type's heading in the Everything list: the type in its color, caps,
/// and the count.
fn section_header<'a>(media: MediaType, count: usize, tokens: &Tokens) -> Element<'a, AppMessage> {
    container(
        row![
            text(media.label().to_uppercase())
                .size(theme::size::SECTION)
                .font(font::BOLD)
                .color(tokens.type_color(media)),
            ui::mono(count.to_string(), 11.5, tokens.sub),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
    .padding(Padding::new(0.0).top(18).bottom(10))
    .into()
}

fn block<'a>(
    app: &'a App,
    entries: &[&'a Entry],
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    let looks = entries.iter().map(|entry| {
        (
            look(entry, state.selected.as_deref(), view, tokens),
            entry.id.clone(),
        )
    });
    match view {
        View::Grid => {
            grid(looks.map(|(look, id)| ui::entry_card(look, tokens, msg(Message::Select(id)))))
                .fluid(CARD_MAX_WIDTH)
                .spacing(20)
                .height(grid::aspect_ratio(CARD_WIDTH, CARD_HEIGHT))
                .into()
        }
        View::List => Column::with_children(
            looks.map(|(look, id)| ui::entry_list_row(look, tokens, msg(Message::Select(id)))),
        )
        .spacing(2)
        .into(),
        View::Text => Column::with_children(looks.map(|(look, id)| {
            column![
                ui::entry_table_row(look, tokens, msg(Message::Select(id))),
                ui::rule(tokens.line.scale_alpha(0.5)),
            ]
            .into()
        }))
        .into(),
    }
}

/// What the views show of an entry.
fn look<'a>(
    entry: &'a Entry,
    selected: Option<&str>,
    view: View,
    tokens: &Tokens,
) -> EntryLook<'a> {
    let is_selected = selected == Some(entry.id.as_str());
    let kind = match entry.media {
        MediaType::Movies => "Movie",
        MediaType::Shows => "Show",
        MediaType::Books => "Book",
        MediaType::Games => "Game",
        MediaType::Music if entry.kind == Some(MusicKind::Song) => "Song",
        MediaType::Music => "Album",
        MediaType::Anime => "Anime",
    };
    let tags: Vec<String> = entry.tags.iter().map(|t| format!("#{t}")).collect();
    EntryLook {
        title: &entry.title,
        creator: entry.creator.as_deref().unwrap_or(""),
        year: entry.year().map(|y| y.to_string()).unwrap_or_default(),
        released: entry.released.map(|r| r.to_string()).unwrap_or_default(),
        kind,
        type_color: tokens.type_color(entry.media),
        cover: theme::cover_color(&entry.id),
        status: entry.status,
        status_label: browse::status_label(Some(entry.media), entry.status),
        rating: entry.rating.map_or("—".into(), |r| format!("{r}/10")),
        tags: tags.join("  "),
        selected: is_selected,
        dimmed: view == View::Grid && selected.is_some() && !is_selected,
    }
}

/// Nothing to show: an empty type, a search with no results, filters that
/// match nothing, or a status nothing has yet.
fn empty_state<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    if !listing.shown.is_empty() {
        return None;
    }
    let state = &app.library;
    let query = state.query.trim();
    let (which, title, body, action) = if listing.in_type == 0 {
        match state.media {
            Some(media) => (
                Icon::Document,
                format!("No {} yet", media.label().to_lowercase()),
                format!("{} you add will show up here.", media.label()),
                Some(("Add", msg(Message::Add))),
            ),
            None => (
                Icon::Document,
                "Your library is empty".into(),
                "Entries you add will show up here.".into(),
                Some(("Add", msg(Message::Add))),
            ),
        }
    } else if !query.is_empty() {
        (
            Icon::Search,
            format!("No results for \"{query}\""),
            "Check the spelling, or add it to your library as a new entry.".into(),
            Some(("Clear search", msg(Message::Search(String::new())))),
        )
    } else if state.filter.is_empty() && listing.base.is_empty() {
        // Only seasonal-only entries in this type.
        (
            Icon::Filter,
            "Nothing to show".into(),
            "Everything here is seasonal-only. Turn on \"Show seasonal-only entries\" in Filter to see it.".into(),
            None,
        )
    } else if state.filter.is_empty() {
        let status = state
            .status
            .map_or("", |s| browse::status_label(state.media, s));
        (
            Icon::Filter,
            format!("Nothing marked {} yet", status.to_lowercase()),
            String::new(),
            None,
        )
    } else {
        (
            Icon::Filter,
            "Nothing matches these filters".into(),
            state.filter.no_matches_text(state.media, state.status),
            Some(("Clear filters", msg(Message::ClearAll))),
        )
    };
    Some(ui::empty_state(which, title, body, action, tokens))
}
