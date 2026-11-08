//! The list itself: the entries in the chosen view, grouped by type when
//! everything is shown, or an empty state.

use iced::widget::{Column, button, column, container, grid, row, scrollable, space, stack, text};
use iced::{Background, Border, Element, Fill, FillPortion, Padding, Shadow, Vector};

use super::{Listing, Message, View, rail};
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, Row, Sort};
use crate::model::{Entry, MediaType};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, EntryLook, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// Grid cells: at most this wide, 2:3 covers with the caption under them.
/// Albums get square cells of the same width (`ALBUM_CARD_HEIGHT`).
const CARD_MAX_WIDTH: f32 = 190.0;
const CARD_WIDTH: f32 = 173.0;
const CARD_HEIGHT: f32 = CARD_WIDTH * 1.5 + 28.0;
const ALBUM_CARD_HEIGHT: f32 = CARD_WIDTH + 28.0;
const CARD_SPACING: f32 = 20.0;

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

    // One type is one block, or in sections with the section rail once
    // the list is long; All or several types get one group per type.
    let (list, right) = match sectioned(app, listing, view, &tokens) {
        Some(sectioned) => sectioned,
        None => (grouped(app, listing, view, &tokens), SIDE),
    };
    if view == View::Text {
        // The table header stays above the scrolling rows, as wide as
        // they are.
        let creator = browse::creator_label(state.single());
        let header = ui::entry_table_header(
            ["Title", creator, "Released", "Status", "Rating", "Tags"],
            &tokens,
        );
        column![
            container(header).padding(Padding::new(0.0).left(SIDE).right(right)),
            list
        ]
        .into()
    } else {
        list
    }
}

/// The list's padding left and right of its rows; the right is wider
/// beside the section rail.
const SIDE: f32 = rail::LEFT;

/// A long list of one type in the phone's sections, which follow the sort
/// (`browse::sections`), with the section rail and the room the list
/// leaves for it at its right. The text and list views get a header per
/// section; the grid has the rail alone, its cards in one block.
fn sectioned<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    view: View,
    tokens: &Tokens,
) -> Option<(Element<'a, AppMessage>, f32)> {
    let state = &app.library;
    state.single()?;
    let sections = browse::row_sections(state.sort, &listing.rows, app.today);
    if sections.len() < 2 {
        return None;
    }
    let names = sections.iter().map(|s| s.short.clone()).collect();
    let on_rail = rail::Sections::new(names, state.sort == Sort::TitleAz);
    let room = on_rail.room();
    if view == View::Grid {
        let rows = rail::GridRows::new(
            state.list_width - SIDE - room,
            CARD_MAX_WIDTH,
            CARD_SPACING,
            CARD_WIDTH / CARD_HEIGHT,
        );
        if !state.is_tall(rows.height(listing.rows.len())) {
            return None;
        }
        let counts: Vec<usize> = sections.iter().map(|s| s.rows.len()).collect();
        let on_rail = on_rail.in_grid(state, &counts, rows);
        let cards = block(app, &listing.rows, view, tokens);
        let list = scrolling([cards], vec![on_rail.view(state, tokens)], room);
        return Some((list, room));
    }
    if !state.is_long(listing.shown.len(), view) {
        return None;
    }
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
            section_header(&section.label, entries, tokens),
            block(app, &section.rows, view, tokens)
        ]
        .into()
    });
    let on_rail = on_rail.in_list(state);
    let list = scrolling(children, vec![on_rail.view(state, tokens)], room);
    Some((list, room))
}

/// One type in one block, or All and several types grouped by type, in
/// type order, each in the current sort.
fn grouped<'a>(
    app: &'a App,
    listing: &Listing<'a>,
    view: View,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
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
            group = group.push(group_header(*media, rows.len(), tokens));
        }
        group.push(block(app, rows, view, tokens)).into()
    });
    // The type-colored rail replaces the scrollbar.
    let mut overlays = Vec::new();
    if groups.len() > 1 {
        let counted: Vec<(Option<MediaType>, Vec<&Entry>)> = groups
            .iter()
            .map(|(media, rows)| (*media, rows.iter().map(Row::lead).collect()))
            .collect();
        overlays.extend(pinned_header(state, &counted, tokens));
        overlays.push(rail(state, &counted, tokens));
    }
    scrolling(children, overlays, SIDE)
}

/// A scrolling list of `children`, measured by `Groups` for the rails and
/// the pinned header, with `overlays` laid over it and `right` px kept
/// free at its right. The scrollbar gives way to a rail when there is
/// one.
pub(super) fn scrolling<'a>(
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
    let scroll = ui::scroll(container(list).padding(Padding::new(SIDE).top(4).right(right)))
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

/// A long list in sections with the section rail: `children` are the
/// sections, each with its header, and `names` their short labels;
/// `alphabet` when they are letters, so the rail lists the whole
/// alphabet.
pub(super) fn sectioned_list<'a>(
    app: &'a App,
    children: impl IntoIterator<Item = Element<'a, AppMessage>>,
    names: Vec<String>,
    alphabet: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = &app.library;
    let on_rail = rail::Sections::new(names, alphabet).in_list(state);
    let room = on_rail.room();
    scrolling(children, vec![on_rail.view(state, tokens)], room)
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

/// A type group's heading: a bar and the type in its color, large, and
/// the count.
fn group_header<'a>(media: MediaType, count: usize, tokens: &Tokens) -> Element<'a, AppMessage> {
    let color = tokens.type_color(media);
    bar_header(media.label(), color, color, count, tokens)
}

/// A large group heading: a 4 px bar in `bar`, the label at 24 px
/// extrabold in `color`, and the count. The type groups of a grouped
/// list, and the Coming soon page's months (the bar in the accent, the
/// label in the text color).
pub(super) fn bar_header<'a>(
    label: &str,
    bar: iced::Color,
    color: iced::Color,
    count: usize,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    container(
        row![
            container(space())
                .width(4)
                .height(22)
                .style(move |_| container::Style::default()
                    .background(bar)
                    .border(Border::default().rounded(2))),
            text(label.to_owned())
                .size(theme::size::GROUP)
                .font(font::EXTRABOLD)
                .line_height(1.0)
                .color(color),
            ui::mono(count.to_string(), 13.0, tokens.sub),
        ]
        .spacing(10)
        .align_y(iced::Center),
    )
    .padding(Padding::new(0.0).top(18).bottom(12))
    .into()
}

/// The group being scrolled through, by its top.
pub(super) fn current_group(state: &super::State) -> Option<usize> {
    let offset = state.scroll.offset;
    state
        .group_tops
        .iter()
        .rposition(|&top| top <= offset + 6.0)
}

/// The current group's header, pinned to the top once its own header has
/// scrolled away, with a soft shadow under it. Its background hides
/// what scrolls under it, on Glass too.
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
    Some(pinned_bar(header, tokens))
}

/// `header` pinned at the top of the list, over what scrolls under it,
/// with a soft shadow; the type groups' and the Coming soon page's months.
pub(super) fn pinned_bar<'a>(
    header: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let (background, shadow) = (tokens.pinned, tokens.shadow);
    let bar = container(header)
        .width(Fill)
        .padding([0, 24])
        .style(move |_| container::Style {
            background: Some(Background::Color(background)),
            shadow: Shadow {
                color: shadow.scale_alpha(0.6),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 8.0,
            },
            ..container::Style::default()
        });
    column![bar, space::vertical()].into()
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
    // Each row is found by its entry's id, so opening or closing the
    // details pane can keep it in view (`reveal`).
    let looks = rows.iter().map(|row| {
        let (look, on_press) = look(app, row, view, tokens);
        (row_id(&row.lead().id), look, on_press)
    });
    let found = |id: String, row: Element<'a, AppMessage>| container(row).id(id).into();
    // A block is one type: albums get square cells.
    let square = rows
        .first()
        .is_some_and(|row| ui::cover::is_square(row.lead().media));
    let card_height = if square {
        ALBUM_CARD_HEIGHT
    } else {
        CARD_HEIGHT
    };
    match view {
        View::Grid => grid(
            looks.map(|(id, look, on_press)| found(id, ui::entry_card(look, tokens, on_press))),
        )
        .fluid(CARD_MAX_WIDTH)
        .spacing(CARD_SPACING)
        .height(grid::aspect_ratio(CARD_WIDTH, card_height))
        .into(),
        View::List => Column::with_children(
            looks.map(|(id, look, on_press)| found(id, ui::entry_list_row(look, tokens, on_press))),
        )
        .spacing(2)
        .into(),
        View::Text => Column::with_children(looks.map(|(id, look, on_press)| {
            found(
                id,
                column![
                    ui::entry_table_row(look, tokens, on_press),
                    ui::rule(tokens.line.scale_alpha(0.5)),
                ]
                .into(),
            )
        }))
        .into(),
    }
}

/// The widget id of an entry's row or card in the list.
pub(super) fn row_id(entry: &str) -> String {
    format!("library-row:{entry}")
}

/// What the views show of a row, and what a click does: an entry is
/// selected; a series opens in the details pane. A show's creator gets its season
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
                type_color: tokens.type_color(entry.media),
                cover: theme::cover_color(&entry.id),
                image: app.artwork.cover(entry),
                status: entry.status,
                status_label: browse::status_label(Some(entry.media), entry.status).to_owned(),
                rating: entry.rating.map_or("—".into(), |r| format!("{r}/10")),
                tags: tags.join("  "),
                selected: is_selected,
                dimmed: view == View::Grid && state.pane_open() && !is_selected,
                stack: None,
                new_season: entry.has_new_season(today),
                square: ui::cover::is_square(entry.media),
            };
            (look, msg(Message::Select(entry.id.clone())))
        }
        Row::Series(group) => {
            // Its row stays marked while the series is in the pane, or a
            // part picked from it.
            let shows = |media: MediaType, name: &str| {
                media == group.media && crate::model::Series::same(name, &group.name)
            };
            let series_on = state.open_series().is_some_and(|s| shows(s.media, &s.name))
                || state.back_series().is_some_and(|b| shows(b.media, &b.name));
            let look = EntryLook {
                title: group.title(),
                creator: group.subtitle(),
                year: group.year_span().unwrap_or_default(),
                released: group.year_span().unwrap_or_default(),
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
                selected: series_on,
                dimmed: view == View::Grid && state.pane_open() && !series_on,
                stack: Some(group.parts.len()),
                new_season: false,
                square: ui::cover::is_square(group.media),
            };
            let open = super::OpenSeries {
                media: group.media,
                name: group.name.clone(),
                highlight: None,
            };
            (look, msg(Message::OpenSeries(open)))
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

    /// The movies in the grid in title order, long enough for the section
    /// rail, the list measured as on the window's first redraw.
    fn long_grid() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app.library.force_long = true;
        for message in [
            Message::ShowType(Some(MediaType::Movies)),
            Message::SetView(View::Grid),
            Message::Sort(Sort::TitleAz),
        ] {
            let _ = app.update(msg(message));
        }
        let messages: Vec<_> = {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            let redraw = iced::window::Event::RedrawRequested(std::time::Instant::now());
            let _ = ui.simulate([iced::Event::Window(redraw)]);
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
        app
    }

    #[test]
    fn the_music_lists_measure_themselves_too() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Music))));
        let messages: Vec<_> = {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            let redraw = iced::window::Event::RedrawRequested(std::time::Instant::now());
            let _ = ui.simulate([iced::Event::Window(redraw)]);
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
        // Music may be the first list shown: without its own measure no
        // artist list would ever count as long.
        assert!(app.library.viewport > 0.0 && app.library.list_width > 0.0);
    }

    #[test]
    fn the_grids_rail_finds_the_rows_the_cards_are_in() {
        let app = long_grid();
        let listing = app.library.listing(app.repo.library(), app.today);
        let names = vec!["A".to_owned(), "Z".to_owned()];
        let room = rail::Sections::new(names, true).room();
        let rows = rail::GridRows::new(
            app.library.list_width - SIDE - room,
            CARD_MAX_WIDTH,
            CARD_SPACING,
            CARD_WIDTH / CARD_HEIGHT,
        );
        assert!(rows.columns > 1 && listing.rows.len() > rows.columns);
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        let mut card = |index: usize| {
            let id = row_id(&listing.rows[index].lead().id);
            ui.find(iced::advanced::widget::Id::from(id))
                .expect("the card")
                .bounds()
        };
        let first = card(0);
        assert!((first.height - rows.cell).abs() < 0.01);
        for index in 0..listing.rows.len() {
            let top = card(index).y - first.y;
            assert!((top - rows.top(index)).abs() < 0.01, "card {index}");
        }
    }

    #[test]
    fn a_press_on_the_rail_jumps_and_holds_until_the_release() {
        use iced::mouse::{Button, Event as Mouse};

        let app = long_grid();
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        let mut label = |text: &str| ui.find(text).expect("on the rail").bounds().center();
        let (d, p, b) = (label("D"), label("P"), label("B"));
        ui.point_at(d);
        let _ = ui.simulate([iced::Event::Mouse(Mouse::ButtonPressed(Button::Left))]);
        // Held, the pointer wanders off the rail: over B, which no movie
        // starts with, within D, and level with P.
        for position in [b, d, iced::Point::new(p.x - 300.0, p.y)] {
            ui.point_at(position);
            let _ = ui.simulate([iced::Event::Mouse(Mouse::CursorMoved { position })]);
        }
        let _ = ui.simulate([iced::Event::Mouse(Mouse::ButtonReleased(Button::Left))]);
        // Let go, the pointer over a label does nothing.
        ui.point_at(d);
        let _ = ui.simulate([iced::Event::Mouse(Mouse::CursorMoved { position: d })]);
        let sent: Vec<String> = ui
            .into_messages()
            .filter_map(|message| match message {
                AppMessage::Library(Message::JumpToSection {
                    label,
                    offset,
                    held,
                }) => Some(format!("{label} {held} {}", offset > 0.0)),
                AppMessage::Library(Message::RailReleased) => Some("released".into()),
                _ => None,
            })
            .collect();
        // D is in the first row, P in the second.
        assert_eq!(sent, ["D true false", "P true true", "released"]);
    }

    /// The heights of the texts reading `0` in the laid-out window.
    struct Texts(&'static str, Vec<f32>);

    impl iced::advanced::widget::Operation for Texts {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation),
        ) {
            operate(self);
        }

        fn text(
            &mut self,
            _id: Option<&iced::advanced::widget::Id>,
            bounds: iced::Rectangle,
            text: &str,
        ) {
            if text == self.0 {
                self.1.push(bounds.height);
            }
        }
    }

    #[test]
    fn the_bubble_names_the_section_while_the_button_is_held() {
        use iced::advanced::renderer::Headless;
        use iced_test::runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced_test::futures::futures::executor::block_on(
            <iced::Renderer as Headless>::new(iced::Font::default(), 14.into(), None),
        )
        .expect("a headless renderer");
        let mut app = long_grid();
        let mut texts = |app: &App| {
            let size = crate::app::WINDOW_SIZE;
            let mut ui = UserInterface::build(app.view(), size, Cache::default(), &mut renderer);
            let mut texts = Texts("P", Vec::new());
            ui.operate(&renderer, &mut texts);
            texts.1
        };
        let jump = |held| {
            msg(Message::JumpToSection {
                label: "P".into(),
                offset: 0.0,
                held,
            })
        };
        // Enter on the label: the rail's own P, small.
        let _ = app.update(jump(false));
        let plain = texts(&app);
        assert!(matches!(plain[..], [label] if label < 20.0));
        // Held: the bubble's P as well, large.
        let _ = app.update(jump(true));
        let held = texts(&app);
        assert!(matches!(held[..], [label, bubble] if label < 20.0 && bubble >= 40.0));
        let _ = app.update(msg(Message::RailReleased));
        assert_eq!(texts(&app), plain, "gone at the release");
    }

    /// How far the list is scrolled, read off the laid-out window.
    struct ListOffset(Option<f32>);

    impl iced::advanced::widget::Operation for ListOffset {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::advanced::widget::Id>,
            _bounds: iced::Rectangle,
            _content_bounds: iced::Rectangle,
            translation: Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&iced::advanced::widget::Id::new(super::super::LIST_ID)) {
                self.0 = Some(translation.y);
            }
        }
    }

    #[test]
    fn opening_and_closing_details_never_moves_the_list() {
        use iced::advanced::renderer::Headless;
        use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, scroll_to};
        use iced_test::runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced_test::futures::futures::executor::block_on(
            <iced::Renderer as Headless>::new(iced::Font::default(), 14.into(), None),
        )
        .expect("a headless renderer");
        let size = crate::app::WINDOW_SIZE;
        let views = [View::Grid, View::List, View::Text];
        for (view, pinned) in views.into_iter().flat_map(|v| [(v, true), (v, false)]) {
            let mut app = grouped_app();
            let _ = app.update(msg(Message::SetView(view)));
            if !pinned {
                let _ = app.update(msg(Message::TogglePanel));
            }
            let mut ui = UserInterface::build(app.view(), size, Cache::default(), &mut renderer);
            ui.operate(
                &renderer,
                &mut scroll_to(
                    super::super::LIST_ID.into(),
                    AbsoluteOffset {
                        x: None,
                        y: Some(300.0),
                    },
                ),
            );
            let mut cache = ui.into_cache();
            let id = app.library.listing(app.repo.library(), app.today).shown[3]
                .id
                .clone();
            // Open, then close by clicking the same entry again.
            for _ in 0..2 {
                let _ = app.update(msg(Message::Select(id.clone())));
                let mut ui = UserInterface::build(app.view(), size, cache, &mut renderer);
                let mut offset = ListOffset(None);
                ui.operate(&renderer, &mut offset);
                assert_eq!(offset.0, Some(300.0), "{view:?}, pinned {pinned}");
                cache = ui.into_cache();
            }
        }
    }

    /// A Shift held when a dialog opens (Ctrl+Shift+click opens Edit) and
    /// released inside it: the list under the dialog must still hear the
    /// release, or its scrollable keeps a phantom Shift and the wheel
    /// scrolls sideways, which a vertical list drops (2026-11-07).
    #[test]
    fn a_shift_released_inside_a_dialog_still_lets_the_wheel_scroll() {
        use iced::advanced::clipboard::Null;
        use iced::advanced::renderer::Headless;
        use iced::keyboard::{self, Modifiers};
        use iced::mouse::{self, ScrollDelta};
        use iced_test::runtime::user_interface::{Cache, UserInterface};

        let mut renderer = iced_test::futures::futures::executor::block_on(
            <iced::Renderer as Headless>::new(iced::Font::default(), 14.into(), None),
        )
        .expect("a headless renderer");
        let size = crate::app::WINDOW_SIZE;
        let mut app = long_grid();
        let id = app.library.listing(app.repo.library(), app.today).shown[0]
            .id
            .clone();
        let position = iced::Point::new(size.width / 2.0, size.height / 2.0);
        let cursor = mouse::Cursor::Available(position);
        let wheel = iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
        });
        let modifiers = |m| iced::Event::Keyboard(keyboard::Event::ModifiersChanged(m));
        // One frame: the view built on the cache, the events, the list's
        // scroll offsets it reported.
        let mut frame = |app: &App, cache: Cache, events: &[iced::Event]| {
            let mut ui = UserInterface::build(app.view(), size, cache, &mut renderer);
            let mut messages = Vec::new();
            let _ = ui.update(events, cursor, &mut renderer, &mut Null, &mut messages);
            let offsets: Vec<f32> = messages
                .into_iter()
                .filter_map(|message| match message {
                    AppMessage::Library(Message::Scrolled(scroll)) => Some(scroll.offset),
                    _ => None,
                })
                .collect();
            (ui.into_cache(), offsets)
        };
        let moved = iced::Event::Mouse(mouse::Event::CursorMoved { position });
        let (cache, offsets) = frame(&app, Cache::default(), &[moved, wheel.clone()]);
        let first = offsets[0];
        assert!(first > 0.0, "the wheel scrolls the list: {offsets:?}");
        let (cache, _) = frame(
            &app,
            cache,
            &[modifiers(Modifiers::CTRL | Modifiers::SHIFT)],
        );
        let _ = app.update(AppMessage::Entry(crate::screens::entry::Message::Edit(id)));
        assert!(app.dialog.is_some());
        let (cache, _) = frame(&app, cache, &[modifiers(Modifiers::empty())]);
        let _ = app.update(AppMessage::Entry(crate::screens::entry::Message::Close));
        assert!(app.dialog.is_none());
        let (_, offsets) = frame(&app, cache, &[wheel]);
        assert!(
            offsets.first().is_some_and(|&offset| offset > first),
            "the wheel still scrolls after the dialog: {offsets:?}"
        );
    }
}
