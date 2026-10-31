//! The list itself: the entries in the chosen view, grouped by type when
//! everything is shown, or an empty state.

use iced::widget::{Column, button, column, container, grid, row, scrollable, space, stack, text};
use iced::{Background, Border, Element, Fill, FillPortion, Padding, Shadow, Vector};

use super::{Listing, Message, View};
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, Row};
use crate::model::{Entry, MediaType};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, EntryLook, FocusExt, Icon};

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
    if state.single() == Some(MediaType::Music) {
        return super::music::body(app, listing, view, &tokens);
    }

    // One type is one block, or the phone's long-list sections with a jump
    // rail; All or several types get one group per type, in type order,
    // each in the current sort.
    let sections = (state.single().is_some() && state.is_long(listing.shown.len(), view))
        .then(|| browse::row_sections(state.sort, &listing.rows, app.today))
        .filter(|sections| sections.len() >= 2);
    let list: Element<'a, AppMessage> = if let Some(sections) = sections {
        let labels: Vec<String> = sections.iter().map(|s| s.short.clone()).collect();
        let children = sections.iter().map(|section| {
            // The count line counts entries, not rows.
            let entries: usize = section
                .rows
                .iter()
                .map(|row| match row {
                    Row::Entry(_) => 1,
                    Row::Series(group) => group.parts.len(),
                })
                .sum();
            column![
                section_header(&section.label, entries, &tokens),
                block(app, &section.rows, view, &tokens)
            ]
            .into()
        });
        sectioned_list(app, children, &labels, &tokens)
    } else {
        let groups: Vec<(Option<MediaType>, Vec<Row<'a>>)> = match state.single() {
            Some(_) => vec![(None, listing.rows.clone())],
            None => MediaType::ALL
                .into_iter()
                .map(|media| {
                    let rows = listing
                        .rows
                        .iter()
                        .filter(|row| row.lead().media == media)
                        .cloned()
                        .collect();
                    (Some(media), rows)
                })
                .filter(|(_, rows): &(_, Vec<Row<'a>>)| !rows.is_empty())
                .collect(),
        };
        let children = groups.iter().map(|(media, rows)| {
            let mut group = Column::new();
            if let Some(media) = media {
                group = group.push(group_header(*media, rows.len(), &tokens));
            }
            group.push(block(app, rows, view, &tokens)).into()
        });
        // The type-colored rail replaces the scrollbar.
        let mut overlays = Vec::new();
        if groups.len() > 1 {
            let counted: Vec<(Option<MediaType>, Vec<&Entry>)> = groups
                .iter()
                .map(|(media, rows)| (*media, rows.iter().map(Row::lead).collect()))
                .collect();
            overlays.extend(pinned_header(state, &counted, &tokens));
            overlays.push(rail(state, &counted, &tokens));
        }
        scrolling(children, overlays, 24.0)
    };
    if view == View::Text {
        // The table header stays above the scrolling rows.
        let creator = browse::creator_label(state.single());
        let header = ui::entry_table_header(
            ["Title", creator, "Released", "Status", "Rating", "Tags"],
            &tokens,
        );
        column![container(header).padding([0, 24]), list].into()
    } else {
        list
    }
}

/// A scrolling list of `children`, measured by `Groups` for the rails and
/// the pinned header, with `overlays` laid over it. The scrollbar gives
/// way to a rail when there is one.
fn scrolling<'a>(
    children: impl IntoIterator<Item = Element<'a, AppMessage>>,
    overlays: Vec<Element<'a, AppMessage>>,
    right: f32,
) -> Element<'a, AppMessage> {
    let list = ui::Groups::new(children, |measure| msg(Message::GroupsMeasured(measure)));
    let scrollbar = if overlays.is_empty() {
        ui::scrollbar()
    } else {
        scrollable::Scrollbar::hidden()
    };
    let scroll = ui::scroll(container(list).padding(Padding::new(24.0).top(4).right(right)))
        .direction(scrollable::Direction::Vertical(scrollbar))
        .id(super::LIST_ID)
        .on_scroll(|viewport| {
            msg(Message::Scrolled(super::Scroll {
                offset: viewport.absolute_offset().y,
                viewport: viewport.bounds().height,
                content: viewport.content_bounds().height,
            }))
        })
        .height(Fill);
    let mut layers = stack![scroll];
    for overlay in overlays {
        layers = layers.push(overlay);
    }
    layers.into()
}

/// A long list in sections: `children` are the sections, each with its
/// header, and `labels` the rail's short labels, one per section.
pub(super) fn sectioned_list<'a>(
    app: &'a App,
    children: impl IntoIterator<Item = Element<'a, AppMessage>>,
    labels: &[String],
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    // The rail's labels need room beside the rows.
    scrolling(
        children,
        vec![section_rail(&app.library, labels, tokens)],
        64.0,
    )
}

/// A long-list section's heading: the decade, letter or rating in gray
/// caps, and the count.
pub(super) fn section_header<'a>(
    label: &str,
    count: usize,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    container(
        row![
            text(label.to_uppercase())
                .size(theme::size::SECTION)
                .font(font::BOLD)
                .color(tokens.sub),
            ui::mono(count.to_string(), 12.0, tokens.sub),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
    .padding(Padding::new(0.0).top(14).bottom(8))
    .into()
}

/// The jump rail of a long list: one short label per section down the
/// right edge, the one being scrolled through marked; a click jumps there.
fn section_rail<'a>(
    state: &super::State,
    labels: &[String],
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let current = current_group(state);
    let items = labels.iter().enumerate().map(|(index, label)| {
        let on = current == Some(index);
        let (color, weight) = if on {
            (tokens.text, font::BOLD)
        } else {
            (tokens.sub, font::SEMIBOLD)
        };
        button(text(label.clone()).size(10.5).font(weight).color(color))
            .padding([1, 5])
            .on_press(msg(Message::JumpToGroup(index)))
            .style(ui::row_style(tokens, on, 5.0))
            .focusable(5.0, tokens)
    });
    container(
        Column::with_children(items)
            .spacing(1)
            .align_x(iced::Center),
    )
    .width(Fill)
    .height(Fill)
    .align_right(Fill)
    .align_y(iced::Center)
    .padding(Padding::new(0.0).right(6))
    .into()
}

/// A type group's heading: the type in its color, caps, and the count.
fn group_header<'a>(media: MediaType, count: usize, tokens: &Tokens) -> Element<'a, AppMessage> {
    container(
        row![
            text(media.label().to_uppercase())
                .size(theme::size::SECTION)
                .font(font::BOLD)
                .color(tokens.type_color(media)),
            ui::mono(count.to_string(), 12.0, tokens.sub),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
    .padding(Padding::new(0.0).top(14).bottom(10))
    .into()
}

/// The group being scrolled through, by its top.
fn current_group(state: &super::State) -> Option<usize> {
    let offset = state.scroll.offset;
    state
        .group_tops
        .iter()
        .rposition(|&top| top <= offset + 6.0)
}

/// The current group's header, pinned to the top once its own header has
/// scrolled away, with a soft shadow under it.
fn pinned_header<'a>(
    state: &super::State,
    groups: &[(Option<MediaType>, Vec<&Entry>)],
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    let index = current_group(state)?;
    if state.scroll.offset <= state.group_tops[index] + 0.5 {
        return None;
    }
    let (media, entries) = groups.get(index)?;
    let header = group_header((*media)?, entries.len(), tokens);
    let (content, shadow) = (tokens.content, tokens.shadow);
    let bar = container(header)
        .width(Fill)
        .padding([0, 24])
        .style(move |_| container::Style {
            background: Some(Background::Color(content)),
            shadow: Shadow {
                color: shadow.scale_alpha(0.6),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 8.0,
            },
            ..container::Style::default()
        });
    Some(column![bar, space::vertical()].into())
}

/// The type-colored scrollbar: one segment per group, sized by its count,
/// that jumps there on a click, and an outlined thumb for the view.
fn rail<'a>(
    state: &super::State,
    groups: &[(Option<MediaType>, Vec<&Entry>)],
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let segments =
        Column::with_children(groups.iter().enumerate().map(|(index, (media, entries))| {
            let color = media.map_or(tokens.sub, |m| tokens.type_color(m));
            let portion = u16::try_from(entries.len()).unwrap_or(u16::MAX).max(1);
            button(space())
                .width(5)
                .height(FillPortion(portion))
                .padding(0)
                .on_press(msg(Message::JumpToGroup(index)))
                .style(move |_, _| button::Style {
                    background: Some(Background::Color(color)),
                    border: Border::default().rounded(2),
                    ..button::Style::default()
                })
                .into()
        }))
        .spacing(2)
        .height(Fill);

    // The thumb, as parts of a thousand of the rail.
    let scroll = state.scroll;
    let (top, size) = if scroll.content > scroll.viewport && scroll.content > 0.0 {
        let top = (scroll.offset / scroll.content * 1000.0)
            .round()
            .clamp(0.0, 1000.0) as u16;
        let size = (scroll.viewport / scroll.content * 1000.0)
            .round()
            .clamp(1.0, 1000.0) as u16;
        (top, size.min(1000 - top))
    } else {
        (0, 1000)
    };
    let edge = tokens.rail_thumb;
    let thumb = column![
        space().height(FillPortion(top)),
        container(space())
            .width(11)
            .height(FillPortion(size.max(1)))
            .style(move |_| container::Style::default()
                .border(Border::default().rounded(5).width(1.5).color(edge))),
        space().height(FillPortion(1000 - top - size)),
    ]
    .height(Fill);
    let rail = stack![container(segments).center_x(11), thumb]
        .width(11)
        .height(Fill);
    container(rail)
        .width(Fill)
        .height(Fill)
        .align_right(Fill)
        .padding(Padding::new(0.0).top(44).right(5).bottom(10))
        .into()
}

/// `block` for plain entries (the music lists).
pub(super) fn block_entries<'a>(
    app: &'a App,
    entries: &[&'a Entry],
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let rows: Vec<Row<'a>> = entries.iter().map(|&e| Row::Entry(e)).collect();
    block(app, &rows, view, tokens)
}

pub(super) fn block<'a>(
    app: &'a App,
    rows: &[Row<'a>],
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let looks = rows.iter().map(|row| look(app, row, view, tokens));
    match view {
        View::Grid => grid(looks.map(|(look, on_press)| ui::entry_card(look, tokens, on_press)))
            .fluid(CARD_MAX_WIDTH)
            .spacing(20)
            .height(grid::aspect_ratio(CARD_WIDTH, CARD_HEIGHT))
            .into(),
        View::List => Column::with_children(
            looks.map(|(look, on_press)| ui::entry_list_row(look, tokens, on_press)),
        )
        .spacing(2)
        .into(),
        View::Text => Column::with_children(looks.map(|(look, on_press)| {
            column![
                ui::entry_table_row(look, tokens, on_press),
                ui::rule(tokens.line.scale_alpha(0.5)),
            ]
            .into()
        }))
        .into(),
    }
}

/// What the views show of a row, and what a click does: an entry is
/// selected; a series opens its page. A show's creator gets its season
/// count, and its year is the years it ran.
fn look<'a>(app: &'a App, row: &Row<'a>, view: View, tokens: &Tokens) -> (EntryLook, AppMessage) {
    let state = &app.library;
    let selected = state.selected.as_deref();
    let today = app.today;
    match row {
        Row::Entry(entry) => {
            let is_selected = selected == Some(entry.id.as_str());
            let tags: Vec<String> = entry.tags.iter().map(|t| format!("#{t}")).collect();
            let seasons = entry.season_list().len();
            let creator = entry.creator.as_deref().unwrap_or("");
            let creator = if seasons > 1 {
                format!("{creator} · {}", entry.season_count_label())
            } else {
                creator.to_owned()
            };
            let years = entry.years(today).unwrap_or_default();
            let released = if seasons > 0 {
                years.clone()
            } else {
                entry.released.map(|r| r.to_string()).unwrap_or_default()
            };
            let look = EntryLook {
                title: entry.title.clone(),
                creator,
                year: years,
                released,
                kind: browse::kind_label(entry),
                type_color: tokens.type_color(entry.media),
                cover: theme::cover_color(&entry.id),
                image: app.artwork.cover(entry),
                status: entry.status,
                status_label: browse::status_label(Some(entry.media), entry.status).to_owned(),
                rating: entry.rating.map_or("—".into(), |r| format!("{r}/10")),
                tags: tags.join("  "),
                selected: is_selected,
                dimmed: view == View::Grid && selected.is_some() && !is_selected,
                stack: None,
                new_season: entry.has_new_season(today),
            };
            (look, msg(Message::Select(entry.id.clone())))
        }
        Row::Series(group) => {
            let look = EntryLook {
                title: group.title(),
                creator: group.subtitle(),
                year: group.year_span().unwrap_or_default(),
                released: group.year_span().unwrap_or_default(),
                kind: "Series",
                type_color: tokens.type_color(group.media),
                cover: theme::cover_color(&group.cover_entry().id),
                image: app.artwork.cover(group.cover_entry()),
                status: if group.all_done() {
                    crate::model::EntryStatus::Done
                } else {
                    crate::model::EntryStatus::Want
                },
                status_label: group.done_label(),
                rating: String::new(),
                tags: String::new(),
                selected: false,
                dimmed: view == View::Grid && selected.is_some(),
                stack: Some(group.parts.len()),
                new_season: false,
            };
            let page = super::Page::Series {
                media: group.media,
                name: group.name.clone(),
                highlight: None,
            };
            (look, msg(Message::OpenPage(page)))
        }
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
        match state.single() {
            Some(media) => (
                Icon::Document,
                format!("No {} yet", media.label().to_lowercase()),
                format!("{} you add will show up here.", media.label()),
                Some(("Add", msg(Message::Add))),
            ),
            None if !state.types.is_empty() => (
                Icon::Document,
                format!(
                    "No {} yet",
                    state.title().to_lowercase().replace(" + ", " or ")
                ),
                "Entries you add will show up here.".into(),
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
    } else if state.filter_count() == 0 && listing.base.is_empty() {
        // Only seasonal-only entries in this type.
        (
            Icon::Filter,
            "Nothing to show".into(),
            "Everything here is seasonal-only. Turn on \"Show seasonal-only entries\" in Filter to see it.".into(),
            None,
        )
    } else if state.filter_count() == 0 {
        let status = state
            .status
            .map_or("", |s| browse::status_label(state.single(), s));
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
            state.filter.no_matches_text(&state.types, state.status),
            Some(("Clear filters", msg(Message::ClearAll))),
        )
    };
    Some(ui::empty_state(which, title, body, action, tokens))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Date;
    use crate::screens::Screen;

    fn grouped_app() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        for message in [
            Message::ShowType(Some(MediaType::Movies)),
            Message::ToggleType(MediaType::Games),
        ] {
            let _ = app.update(msg(message));
        }
        app
    }

    #[test]
    fn the_grouped_list_reports_where_each_group_starts() {
        let app = grouped_app();
        let messages: Vec<_> = {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            // The list reports on the first event, as on the window's first
            // redraw.
            let redraw = iced::window::Event::RedrawRequested(std::time::Instant::now());
            let _ = ui.simulate([iced::Event::Window(redraw)]);
            ui.into_messages().collect()
        };
        let measure = messages.into_iter().find_map(|m| match m {
            AppMessage::Library(Message::GroupsMeasured(measure)) => Some(measure),
            _ => None,
        });
        let measure = measure.expect("the list measures its groups");
        assert_eq!(measure.tops.len(), 2);
        assert!(measure.tops[0] == 0.0 && measure.tops[1] > 0.0);
        assert!(measure.viewport > 0.0, "and how tall its visible part is");
    }

    #[test]
    fn the_current_group_is_the_last_one_scrolled_to() {
        let mut app = grouped_app();
        app.library.group_tops = vec![0.0, 600.0, 1200.0];
        app.library.scroll.offset = 0.0;
        assert_eq!(current_group(&app.library), Some(0));
        app.library.scroll.offset = 597.0;
        assert_eq!(
            current_group(&app.library),
            Some(1),
            "a few px early, as the mockup"
        );
        app.library.scroll.offset = 1500.0;
        assert_eq!(current_group(&app.library), Some(2));
    }
}
