//! The toolbar (title, search, Filter, Sort, view, Add) and the chips row
//! under it.

use iced::widget::{Column, Row, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Listing, Message, MusicTab, Popup, SEARCH_ID, State, TOP_TAGS, View};
use crate::app::{App, Message as AppMessage};
use crate::browse::{self, Decade};
use crate::model::{Entry, MediaType};
use crate::screens::shell;
use crate::theme::{Tokens, font};
use crate::ui::{self, Align, ButtonKind, FocusExt, Icon, Popover};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

pub fn toolbar<'a>(app: &'a App, listing: &Listing<'a>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let title_color = state
        .single()
        .map_or(tokens.text, |media| tokens.type_color(media));
    let placeholder = match state.single() {
        Some(media) => search_hint(media),
        None => "Search everything",
    };
    // The toolbar spans the content's full width whatever the details
    // panel does; on a small window it gives way: a shorter search, then
    // icons and short labels alone. The panel button takes 48 px.
    let toggle = super::panel::toggle(app);
    let room = if toggle.is_some() { 48.0 } else { 0.0 };
    let density = Density::for_width(shell::content_width(app, 0.0) - room);
    let search = container(ui::search_field(
        placeholder,
        &state.query,
        |query| msg(Message::Search(query)),
        Some("Ctrl K"),
        Some(msg(Message::ClearSearch)),
        Some(SEARCH_ID),
        &tokens,
    ))
    .width(match density {
        Density::Full => 280,
        Density::Compact => 180,
        Density::Tight => 150,
    });

    let filter_count = state.filter_count();
    let filter_label: Element<'a, AppMessage> = match density {
        Density::Tight => space().width(0).into(),
        _ => text("Filter").into(),
    };
    let filter_button = ui::menu_button(
        Some(Icon::Filter),
        filter_label,
        (filter_count > 0).then_some(filter_count),
        state.popup == Some(Popup::Filter) || filter_count > 0,
        &tokens,
        msg(Message::Toggle(Popup::Filter)),
    );
    let filter_popover = (state.popup == Some(Popup::Filter)).then(|| filter_panel(app, listing));
    let filter = Popover::new(filter_button, filter_popover)
        .align(Align::End)
        .on_dismiss(msg(Message::ClosePopup));

    let current_sort = state.current_sort();
    let sort_label = match density {
        Density::Full => {
            row![text("Sort").color(tokens.sub), text(current_sort.short())].spacing(6)
        }
        Density::Compact => row![text(current_sort.short())],
        Density::Tight => row![text("Sort")],
    };
    let sort_button = ui::menu_button(
        None,
        sort_label,
        None,
        state.popup == Some(Popup::Sort),
        &tokens,
        msg(Message::Toggle(Popup::Sort)),
    );
    let sort_menu = (state.popup == Some(Popup::Sort)).then(|| {
        let items = state
            .sort_choices()
            .iter()
            .map(|&sort| (sort.label(), sort == current_sort, msg(Message::Sort(sort))));
        ui::radio_menu("Sort by", items, &tokens)
    });
    let sort = Popover::new(sort_button, sort_menu)
        .align(Align::End)
        .on_dismiss(msg(Message::ClosePopup));

    // The overview counts the whole library, an artist's page the artist's
    // entries, a list what it shows: on Music's tabs the artists, the
    // albums or the songs.
    let count = if state.is_overview() {
        app.repo.library().count(None)
    } else if let Some(artist) = &state.artist {
        let of_artist = |e: &&&Entry| crate::music::artist_of(e).eq_ignore_ascii_case(artist);
        listing.shown.iter().filter(of_artist).count()
    } else {
        state.count_shown(&listing.shown, &app.artwork)
    };

    let current = state.view_mode();
    let views = ui::icon_switch(
        [
            (Icon::ViewGrid, View::Grid),
            (Icon::ViewList, View::List),
            (Icon::ViewTable, View::Text),
        ]
        .map(|(which, view)| (which, view == current, msg(Message::SetView(view)))),
        &tokens,
    );

    // On an artist's page, a back arrow to Music comes first.
    let back = state
        .artist
        .is_some()
        .then(|| ui::icon_button(Icon::ChevronLeft, 18.0, &tokens, msg(Message::CloseArtist)));
    let bar = row![
        row(back),
        text(state.title())
            .size(26)
            .font(font::EXTRABOLD)
            .color(title_color),
        ui::mono(count.to_string(), 13.0, tokens.sub),
        space::horizontal(),
        search,
        filter,
        sort,
        views,
        row(toggle),
        ui::add_button(
            if density == Density::Tight { "" } else { "Add" },
            &tokens,
            msg(Message::Add)
        ),
    ]
    .spacing(10)
    .align_y(Center);
    container(bar)
        .height(64)
        .padding(Padding::new(0.0).left(24).right(20))
        .align_y(Center)
        .into()
}

/// How much room the toolbar has: everything, short labels, or icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Full,
    Compact,
    Tight,
}

impl Density {
    /// The toolbar needs about 1000 px for everything and 820 for the
    /// compact form.
    pub fn for_width(width: f32) -> Density {
        if width >= 1000.0 {
            Density::Full
        } else if width >= 820.0 {
            Density::Compact
        } else {
            Density::Tight
        }
    }
}

fn search_hint(media: MediaType) -> &'static str {
    match media {
        MediaType::Movies => "Search movies",
        MediaType::Shows => "Search shows",
        MediaType::Books => "Search books",
        MediaType::Games => "Search games",
        MediaType::Music => "Search music",
        MediaType::Anime => "Search anime",
    }
}

/// The Filter popover: types, decades, tags, a rating range, the
/// seasonal-only switch, then Reset and the live count.
fn filter_panel<'a>(app: &'a App, listing: &Listing<'a>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let library = app.repo.library();
    let filter = &state.filter;

    let types = Row::with_children(MediaType::ALL.map(|media| {
        ui::filter_chip(
            media.label().to_owned(),
            Some(tokens.type_color(media)),
            None,
            state.types.contains(&media),
            &tokens,
            msg(Message::ToggleType(media)),
        )
    }))
    .spacing(6)
    .wrap()
    .vertical_spacing(6);

    let in_type = library
        .entries
        .iter()
        .filter(|e| state.types.is_empty() || state.types.contains(&e.media));
    let decades = Row::with_children(
        Decade::options_for(in_type.map(|e| e.year()))
            .into_iter()
            .map(|decade| {
                let on = filter.decades.contains(&decade);
                ui::filter_chip(
                    decade.label(),
                    None,
                    None,
                    on,
                    &tokens,
                    msg(Message::ToggleDecade(decade)),
                )
            }),
    )
    .spacing(6)
    .wrap()
    .vertical_spacing(6);

    let counts = library.tag_counts();
    let mut quick: Vec<&str> = if state.pins.is_empty() {
        counts.iter().take(TOP_TAGS).map(|(tag, _)| *tag).collect()
    } else {
        state.pins.iter().map(String::as_str).collect()
    };
    for tag in &filter.tags {
        if !quick.contains(&tag.as_str()) {
            quick.push(tag);
        }
    }
    let tags = Row::with_children(quick.into_iter().map(|tag| {
        let on = filter.has_tag(tag, None);
        ui::filter_chip(
            format!("#{tag}"),
            None,
            None,
            on,
            &tokens,
            msg(Message::ToggleTag(tag.to_owned(), None)),
        )
    }))
    .push(ui::add_chip("All tags…", &tokens, msg(Message::OpenPicker)))
    .spacing(6)
    .wrap()
    .vertical_spacing(6);

    let hint = filter
        .rating_label()
        .unwrap_or_else(|| "Click one, or two for a range".into());
    let ratings = Row::with_children((1..=10u8).map(|n| {
        let on = filter
            .rating
            .is_some_and(|(low, high)| (low..=high).contains(&n));
        rating_cell(n, on, &tokens)
    }))
    .spacing(4);

    let result = text(format!(
        "{} shown",
        shown_noun(state, state.count_shown(&listing.shown, &app.artwork))
    ))
    .size(13)
    .color(tokens.sub)
    .width(Fill)
    .align_x(iced::alignment::Horizontal::Right);
    let footer = row![
        ui::labeled_button(
            "Reset",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::ResetFilter))
        ),
        result,
    ]
    .align_y(Center);

    let panel = Column::new()
        .push(group("Type", types.into(), &tokens))
        .push(group("Decade", decades.into(), &tokens))
        .push(group("Tags", tags.into(), &tokens))
        .push(
            column![
                row![
                    ui::section_header("Rating", &tokens),
                    text(hint).size(12.5).color(tokens.sub)
                ]
                .spacing(8)
                .align_y(Center),
                ratings,
            ]
            .spacing(8),
        )
        .push(ui::switch(
            "Show seasonal-only entries",
            filter.include_seasonal,
            &tokens,
            msg(Message::ToggleSeasonal),
        ))
        .push(footer)
        .spacing(16);
    container(panel)
        .width(340)
        .padding(14)
        .style(move |_| ui::panel_style(&tokens, 12.0))
        .into()
}

/// What the list on screen counts: Music's tabs their artists, albums or
/// songs, every other list its entries.
fn shown_noun(state: &State, n: usize) -> String {
    if !super::music_tabs_shown(state) {
        return count_noun(state.single(), n);
    }
    let (one, many) = match state.music_tab {
        MusicTab::Artists => ("artist", "artists"),
        MusicTab::Albums => ("album", "albums"),
        MusicTab::Songs => ("song", "songs"),
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "1 movie", "9 movies", "3 entries".
fn count_noun(media: Option<MediaType>, n: usize) -> String {
    let (one, many) = match media {
        Some(MediaType::Movies) => ("movie", "movies"),
        Some(MediaType::Shows) => ("show", "shows"),
        Some(MediaType::Books) => ("book", "books"),
        Some(MediaType::Games) => ("game", "games"),
        Some(MediaType::Music) => ("album or song", "albums and songs"),
        Some(MediaType::Anime) => ("anime", "anime"),
        None => ("entry", "entries"),
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

fn group<'a>(
    label: &str,
    body: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![ui::section_header(label, tokens), body]
        .spacing(8)
        .into()
}

/// One of the Filter popover's ten rating buttons.
fn rating_cell<'a>(n: u8, on: bool, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(
        container(text(n.to_string()).size(13.5).font(font::SEMIBOLD))
            .center_x(Fill)
            .center_y(Fill),
    )
    .width(Fill)
    .height(32)
    .padding(0)
    .on_press(msg(Message::PickRating(n)))
    .style(move |_, status| button::Style {
        background: if on {
            Some(Background::Color(tokens.selection))
        } else {
            matches!(status, button::Status::Hovered).then_some(Background::Color(tokens.hover))
        },
        text_color: tokens.text,
        border: Border::default().rounded(7).width(1).color(if on {
            tokens.selection
        } else {
            tokens.line
        }),
        ..button::Style::default()
    })
    .focusable(7.0, &tokens)
}

/// The tags the search names, as chips to narrow by (canvas revision of
/// 2026-11-17, as on the phone): the word "Tags", up to
/// `browse::SUGGESTED_TAGS` chips with how many entries of the types on
/// show carry each (on an artist's page, of the artist's), then
/// "+N more", which opens Filter by tags on the same text. A click adds
/// the tag to the filter; the search and the other suggestions stay.
fn suggested_tags<'a>(app: &'a App, tokens: &Tokens) -> Vec<Element<'a, AppMessage>> {
    let state = &app.library;
    // What the search looks through: the types on show, seasonal-only
    // entries too.
    let searched = app.repo.library().entries.iter().filter(|e| {
        (state.types.is_empty() || state.types.contains(&e.media))
            && state
                .artist
                .as_ref()
                .is_none_or(|artist| crate::music::artist_of(e).eq_ignore_ascii_case(artist))
    });
    let found = browse::tag_suggestions(searched, &state.query, &state.filter.tags);
    if found.is_empty() {
        return Vec::new();
    }
    let label = text("TAGS")
        .size(11)
        .font(font::SEMIBOLD)
        .color(tokens.sub)
        .height(28)
        .align_y(Center);
    let mut chips: Vec<Element<'a, AppMessage>> = vec![
        container(label)
            .padding(Padding::new(0.0).left(10).right(2))
            .into(),
    ];
    chips.extend(
        found
            .iter()
            .take(browse::SUGGESTED_TAGS)
            .map(|&(tag, count)| {
                let chip = ui::suggested_chip(
                    format!("#{tag}"),
                    count,
                    tokens,
                    msg(Message::ToggleTag(tag.to_owned(), None)),
                );
                ui::tooltip(chip, format!("Show only #{tag}"), tokens)
            }),
    );
    if found.len() > browse::SUGGESTED_TAGS {
        let ink = tokens.ink;
        chips.push(
            button(
                text(format!("+{} more", found.len() - browse::SUGGESTED_TAGS))
                    .size(13)
                    .font(font::SEMIBOLD)
                    .height(Fill)
                    .align_y(Center),
            )
            .height(28)
            .padding([0, 4])
            .on_press(msg(Message::MoreTags))
            .style(move |_, _| button::Style {
                text_color: ink,
                ..button::Style::default()
            })
            .focusable(6.0, tokens),
        );
    }
    chips
}

/// The status chips with counts, then the tags the search suggests, then
/// the active filters as removable chips (several types, the overview's
/// two, tags, decades, seasonal, rating), then "Clear all".
pub fn chips<'a>(app: &'a App, listing: &Listing<'a>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let filter = &state.filter;
    let mut chips = Row::new().spacing(6).align_y(Center);

    // Music's tabs come first, outside an artist's page.
    if super::music_tabs_shown(state) {
        let tabs = [
            ("Artists", MusicTab::Artists),
            ("Albums", MusicTab::Albums),
            ("Songs", MusicTab::Songs),
        ]
        .map(|(label, tab)| (label, state.music_tab == tab, msg(Message::MusicTab(tab))));
        chips = chips.push(
            container(ui::segmented(tabs, 28.0, &tokens))
                .width(270)
                .padding(Padding::new(0.0).right(10)),
        );
    }

    // On an artist's page the counts are the artist's.
    let base: Vec<&Entry> = listing
        .base
        .iter()
        .copied()
        .filter(|e| {
            state
                .artist
                .as_ref()
                .is_none_or(|artist| crate::music::artist_of(e).eq_ignore_ascii_case(artist))
        })
        .collect();
    // Music's tabs count what they list: an artist under each status
    // they have an album or a song in.
    let all = ui::status_chip(
        "All",
        state.count_shown(&base, &app.artwork),
        state.status.is_none(),
        &tokens,
        msg(Message::Status(None)),
    );
    chips = chips.push(all);
    for status in browse::status_options(&state.types) {
        let with_status: Vec<&Entry> = base
            .iter()
            .copied()
            .filter(|e| e.status == status)
            .collect();
        let count = state.count_shown(&with_status, &app.artwork);
        let label = browse::status_label(state.single(), status);
        let on = state.status == Some(status);
        chips = chips.push(ui::status_chip(
            label,
            count,
            on,
            &tokens,
            msg(Message::Status(Some(status))),
        ));
    }

    chips = chips.extend(suggested_tags(app, &tokens));

    let mut active = Vec::new();
    // Each removable chip keeps 6 px more room than the status chips.
    let mut add = |label: String, detail: Option<String>, dot, message| {
        let chip = ui::removable_chip(label, detail, dot, &tokens, msg(message));
        active.push(container(chip).padding(Padding::new(0.0).left(6)).into());
    };
    if state.types.len() > 1 {
        for &media in &state.types {
            let dot = Some(tokens.type_color(media));
            add(
                media.label().to_owned(),
                None,
                dot,
                Message::ToggleType(media),
            );
        }
    }
    if filter.not_released {
        add(
            "Not released yet".into(),
            None,
            None,
            Message::ClearNotReleased,
        );
    }
    if filter.recent {
        add(
            "Added in the past month".into(),
            None,
            None,
            Message::ClearRecent,
        );
    }
    for (tag, category) in filter.tag_chips() {
        let role = category.map(|c| c.role_label().to_owned());
        add(
            format!("#{tag}"),
            role,
            None,
            Message::ToggleTag(tag, category),
        );
    }
    for &decade in &filter.decades {
        add(decade.label(), None, None, Message::ToggleDecade(decade));
    }
    if filter.include_seasonal {
        add(
            "Incl. seasonal-only".into(),
            None,
            None,
            Message::ToggleSeasonal,
        );
    }
    if let Some(label) = filter.rating_label() {
        add(label, None, None, Message::ClearRating);
    }
    chips = chips.extend(active);

    // With nothing shown, the empty state's button clears the filters.
    let filtering = state.filter_count() > 0 || state.status.is_some();
    if filtering && !listing.shown.is_empty() {
        let ink = tokens.ink;
        chips = chips.push(
            button(
                text("Clear all")
                    .size(13)
                    .font(font::SEMIBOLD)
                    .height(Fill)
                    .align_y(Center),
            )
            .height(28)
            .padding(Padding::new(0.0).left(10).right(4))
            .on_press(msg(Message::ClearAll))
            .style(move |_, _| button::Style {
                text_color: ink,
                ..button::Style::default()
            })
            .focusable(6.0, &tokens),
        );
    }
    container(chips.wrap().vertical_spacing(6))
        .padding(Padding::new(0.0).left(24).right(24).bottom(12))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window as it draws, to look for words in.
    fn window(app: &App) -> iced_test::Simulator<'_, AppMessage> {
        iced_test::Simulator::with_size(crate::app::settings(), crate::app::WINDOW_SIZE, app.view())
    }

    #[test]
    fn a_search_offers_the_tags_it_names_until_they_are_picked() {
        let mut app = App::new(crate::screens::Screen::Library);
        let send = |app: &mut App, message| {
            let _ = app.update(msg(message));
        };
        send(&mut app, Message::ShowType(Some(MediaType::Music)));
        send(&mut app, Message::MusicTab(MusicTab::Albums));
        // One letter offers nothing.
        send(&mut app, Message::Search("r".into()));
        assert!(window(&app).find("#electronic").is_err());
        send(&mut app, Message::Search("ro".into()));
        {
            let mut ui = window(&app);
            for words in ["#rock", "#electronic"] {
                assert!(ui.find(words).is_ok(), "{words}");
            }
            assert!(ui.find("+1 more").is_err());
        }
        // A click picks the tag as a filter; the text and the other
        // suggestion stay, and the picked one is offered no more.
        let messages: Vec<AppMessage> = {
            let mut ui = window(&app);
            let _ = ui.click("#electronic");
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.library.filter.tags, ["electronic"]);
        assert_eq!(app.library.query, "ro");
        assert_eq!(app.library.filter_count(), 1);
        let found = browse::tag_suggestions(
            app.repo.library().entries_of(MediaType::Music),
            &app.library.query,
            &app.library.filter.tags,
        );
        assert_eq!(found, [("rock", 6)]);
        assert!(window(&app).find("#rock").is_ok());
        // On Artists too.
        send(&mut app, Message::MusicTab(MusicTab::Artists));
        assert!(window(&app).find("#rock").is_ok());
    }

    #[test]
    fn more_than_five_suggestions_fold_into_a_link() {
        let mut app = App::new(crate::screens::Screen::Library);
        let _ = app.update(msg(Message::Search("an".into())));
        let found = browse::tag_suggestions(&app.repo.library().entries, "an", &[]);
        assert!(found.len() > browse::SUGGESTED_TAGS, "{found:?}");
        let more = format!("+{} more", found.len() - browse::SUGGESTED_TAGS);
        let messages: Vec<AppMessage> = {
            let mut ui = window(&app);
            let _ = ui.click(more.as_str());
            ui.into_messages().collect()
        };
        assert!(
            matches!(
                messages.as_slice(),
                [AppMessage::Library(Message::MoreTags)]
            ),
            "{messages:?}"
        );
    }

    #[test]
    fn counts_read_as_words() {
        assert_eq!(count_noun(Some(MediaType::Movies), 1), "1 movie");
        assert_eq!(count_noun(None, 3), "3 entries");
        assert_eq!(count_noun(Some(MediaType::Music), 2), "2 albums and songs");

        let mut state = State::default();
        state.update(Message::ShowType(Some(MediaType::Music)));
        assert_eq!(shown_noun(&state, 1), "1 artist");
        state.update(Message::MusicTab(MusicTab::Songs));
        assert_eq!(shown_noun(&state, 4), "4 songs");
        state.update(Message::OpenArtist("Metallica".into()));
        assert_eq!(shown_noun(&state, 4), "4 albums and songs");
    }
}
