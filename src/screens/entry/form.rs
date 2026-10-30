//! The Add and Edit dialog: the type, the online lookup on the left (Add
//! only), the form on the right, and the footer with "Also add to plan".
//! The tag picker and the new-tag confirmation open over it.

use std::fmt;
use std::rc::Rc;

use iced::widget::{
    Column, Row, button, column, container, pick_list, row, space, text, text_editor,
};
use iced::{Background, Border, Center, Color, Element, Fill, Padding, Shadow, Vector};

use super::{CategoriesDialog, Form, Message, NewTag, Replace};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::lookup;
use crate::model::{BookFormat, EntryKind, EntryStatus, MediaType, TagCategory, text as rules};
use crate::screens::entry::SeasonDraft;
use crate::tags::PickerState;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon, Picker};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

pub fn view<'a>(app: &'a App, form: &'a Form) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let adding = form.editing.is_none();

    let mut body = Row::new().height(Fill);
    if adding || form.again {
        body = body.push(lookup(app, form, &tokens));
    }
    body = body.push(
        ui::scroll(fields(app, form, &tokens))
            .height(Fill)
            .width(Fill),
    );

    let panel = column![
        header(form, &tokens),
        ui::rule(tokens.line),
        body,
        ui::rule(tokens.line),
        footer(app, form, &tokens),
    ];
    let panel: Element<'a, AppMessage> = container(panel)
        .width(1080)
        .max_height(800)
        .height(Fill)
        .clip(true)
        .style(move |_| container::Style {
            text_color: Some(tokens.text),
            background: Some(Background::Color(tokens.dialog)),
            border: Border::default().rounded(14).width(1).color(tokens.line),
            shadow: Shadow {
                color: tokens.shadow,
                offset: Vector::new(0.0, 12.0),
                blur_radius: 32.0,
            },
            ..container::Style::default()
        })
        .into();
    // Keep the dialog inside a small window.
    let panel: Element<'_, AppMessage> = container(panel).padding([30, 40]).center(Fill).into();

    // Every layer goes through `ui::modal` whether or not its dialog is
    // open, so the tree keeps its shape and the form keeps its state.
    let with_cover = ui::modal(
        panel,
        form.cover_dialog.then(|| cover_dialog(form, &tokens)),
        msg(Message::CloseCover),
        &tokens,
    );
    let with_picker = ui::modal(
        with_cover,
        form.picker
            .as_ref()
            .map(|picker| tag_picker(app, form, picker, &tokens)),
        msg(Message::ClosePicker),
        &tokens,
    );
    let with_new_tag = ui::modal(
        with_picker,
        form.new_tag
            .as_ref()
            .map(|new_tag| new_tag_dialog(new_tag, form.media, &tokens)),
        msg(Message::CancelNewTag),
        &tokens,
    );
    let with_categories = ui::modal(
        with_new_tag,
        form.categories
            .as_ref()
            .map(|dialog| categories_dialog(app, form, dialog, &tokens)),
        msg(Message::CancelCategories),
        &tokens,
    );
    ui::modal(
        with_categories,
        form.replace
            .as_ref()
            .map(|replace| replace_dialog(replace, &tokens)),
        msg(Message::ReplaceCancel),
        &tokens,
    )
}

/// The phone's D12 and D13: keep both, replace, or go back to the form.
fn replace_dialog<'a>(replace: &Replace, tokens: &Tokens) -> Element<'a, AppMessage> {
    let (title, body, note, replace_label) = match replace {
        Replace::Songs { album, songs } => {
            let n = songs.len();
            let note = if n == 1 {
                "The song entry is removed. Its rating moves to the album's track list.".to_owned()
            } else {
                format!(
                    "The {} song entries are removed. Their ratings move to the album's track list.",
                    count_word(n)
                )
            };
            (
                format!(
                    "You have {n} {} from {album} saved",
                    if n == 1 { "song" } else { "songs" }
                ),
                songs.join(" · "),
                note,
                "Replace songs with album",
            )
        }
        Replace::Album {
            album,
            song,
            adding,
        } => (
            format!("{album} is already in your library"),
            format!(
                "You are {} the song {song} from this album.",
                if *adding { "adding" } else { "saving" }
            ),
            "Replacing deletes the album entry, with its rating and note.".to_owned(),
            "Replace album with this song",
        ),
    };
    let body = column![text(body).size(14), text(note).size(13).color(tokens.sub)].spacing(10);
    let actions = column![
        ui::wide_button(
            "Keep both",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::ReplaceKeepBoth))
        ),
        ui::wide_button(
            replace_label,
            ButtonKind::Destructive,
            *tokens,
            Some(msg(Message::ReplaceConfirm))
        ),
        ui::wide_button(
            "Cancel",
            ButtonKind::Text,
            *tokens,
            Some(msg(Message::ReplaceCancel))
        ),
    ]
    .spacing(8);
    ui::dialog(
        title,
        body,
        actions,
        440,
        tokens,
        msg(Message::ReplaceCancel),
    )
}

/// "two", "three": small counts in words, as the phone writes them.
fn count_word(n: usize) -> String {
    const WORDS: [&str; 10] = [
        "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    WORDS
        .get(n)
        .map_or_else(|| n.to_string(), |w| (*w).to_owned())
}

/// "Add to library", the type, and Close.
fn header<'a>(form: &Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let title = if form.editing.is_some() {
        "Edit entry"
    } else {
        "Add to library"
    };
    let types = Row::with_children(MediaType::ALL.map(|media| {
        let label = match media {
            MediaType::Movies => "Movie",
            MediaType::Shows => "Show",
            MediaType::Books => "Book",
            MediaType::Games => "Game",
            MediaType::Music => "Music",
            MediaType::Anime => "Anime",
        };
        ui::filter_chip(
            label.to_owned(),
            Some(tokens.type_color(media)),
            None,
            form.media == media,
            tokens,
            msg(Message::Type(media)),
        )
    }))
    .spacing(6);
    container(
        row![
            text(title).size(21).font(font::EXTRABOLD),
            container(types).padding(Padding::new(0.0).left(12)),
            space::horizontal(),
            ui::icon_button(Icon::Close, 18.0, tokens, msg(Message::Close)),
        ]
        .spacing(14)
        .align_y(Center),
    )
    .height(60)
    .padding(Padding::new(0.0).left(24).right(20))
    .align_y(Center)
    .into()
}

/// The lookup: a search, a game's platform filter, its matches (the
/// arrow keys move, Enter picks) in whatever state the lookup is in, and
/// "Fill in the details myself", or Cancel for "Look up again".
fn lookup<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = app.lookup;
    let again = form.again;
    let matches = form.matches();
    let query = form.query.trim();
    let noun = form.media.label().to_lowercase();
    let platform = form.platform_filter.as_deref();

    let head = if again {
        "Pick a match. It replaces the creator, year, description and pages; your title, note, \
         status and tags stay."
            .to_owned()
    } else if query.is_empty() {
        "Type a title to look it up online.".to_owned()
    } else if state == lookup::LookupState::Loading {
        format!("Looking up \u{201c}{query}\u{201d}\u{2026}")
    } else if state.shows_rows() && !matches.is_empty() {
        let n = matches.len();
        let found = if n == 1 {
            "1 match".to_owned()
        } else {
            format!("{n} matches")
        };
        match platform {
            Some(platform) => format!("On {platform} · {found} · ↑ ↓ to move, Enter to pick"),
            None => format!("{found} · ↑ ↓ to move, Enter to pick"),
        }
    } else {
        String::new()
    };

    // The rows, a message, or the loading placeholders.
    let mut list = Column::new().spacing(4);
    let message: Option<(String, bool)> = match state {
        _ if query.is_empty() => None,
        lookup::LookupState::Loading => None,
        lookup::LookupState::Offline => Some((
            "Could not reach the lookup. Check your connection and try again.".into(),
            true,
        )),
        lookup::LookupState::NotSetUp => Some((
            format!(
                "Lookups for {noun} are not set up on this build (no API key). You can still fill \
                 in the details yourself."
            ),
            false,
        )),
        lookup::LookupState::PartialNone => Some((
            "Some lookups could not be reached, so a match may be missing. Try again in a \
             moment."
                .into(),
            true,
        )),
        _ if matches.is_empty() => Some((
            match platform {
                Some(platform) => format!("No games called that on {platform}."),
                None => format!("No matches among {noun}."),
            },
            false,
        )),
        _ => None,
    };
    if state == lookup::LookupState::Loading && !query.is_empty() {
        for _ in 0..3 {
            list = list.push(placeholder_row(tokens));
        }
    } else if let Some((message, retry)) = message {
        let mut block = Column::new()
            .spacing(10)
            .push(text(message).size(13.5).color(tokens.sub));
        if retry {
            block = block.push(ui::labeled_button(
                "Try again",
                ButtonKind::Outline,
                *tokens,
                Some(msg(Message::RetryLookup)),
            ));
        }
        list = list.push(container(block).padding([8, 4]));
    } else {
        for (index, found) in matches.iter().enumerate() {
            list = list.push(match_row(form, index, found, platform, tokens));
        }
        if state == lookup::LookupState::Partial && !matches.is_empty() {
            list = list.push(
                container(
                    text("Some lookups could not be reached.")
                        .size(12.5)
                        .color(tokens.sub),
                )
                .padding([6, 8]),
            );
        }
    }

    let foot = row![
        text("Not listed?").size(12.5).color(tokens.sub),
        if again {
            ui::labeled_button(
                "Cancel",
                ButtonKind::Text,
                *tokens,
                Some(msg(Message::CancelLookAgain)),
            )
        } else {
            ui::labeled_button(
                "Fill in the details myself",
                ButtonKind::Text,
                *tokens,
                Some(msg(Message::Manual)),
            )
        },
    ]
    .spacing(6)
    .align_y(Center);

    let mut pane = Column::new().spacing(12).push(ui::search_field(
        "Search online",
        &form.query,
        |query| msg(Message::Query(query)),
        None,
        Some(LOOKUP_ID),
        tokens,
    ));
    if form.media == MediaType::Games {
        pane = pane.push(platform_filter(app, form, tokens));
    }
    pane = pane
        .push(text(head).size(12).color(tokens.sub))
        .push(ui::scroll(list).height(Fill))
        .push(container(foot).center_x(Fill));
    let chrome = tokens.dialog_chrome;
    container(pane)
        .width(380)
        .height(Fill)
        .padding(18)
        .style(move |_| container::background(chrome))
        .into()
}

/// One match: a cover, the title, and the creator, platforms (on a game
/// lookup narrowed to one) and year.
fn match_row<'a>(
    form: &Form,
    index: usize,
    found: &'static lookup::Match,
    platform: Option<&str>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let picked = form.picked == Some(index);
    let year = found.released.get(..4).unwrap_or_default();
    let tint = theme::cover_color(found.source);
    let cover = container(space())
        .width(44)
        .height(62)
        .style(move |_| container::background(tint).border(Border::default().rounded(5)));
    let mut parts = vec![found.creator.to_owned()];
    if let Some(platform) = platform {
        let others = found.platforms.len().saturating_sub(1);
        parts.push(match others {
            0 => platform.to_owned(),
            1 => format!("{platform} and 1 more"),
            n => format!("{platform} and {n} more"),
        });
    }
    parts.push(year.to_owned());
    if let Some(edition) = found.edition {
        parts.push(edition.to_owned());
    }
    let words = column![
        text(found.title).size(14.5).font(font::SEMIBOLD),
        text(parts.join(" · ")).size(12.5).color(tokens.sub),
    ]
    .spacing(2);
    button(row![cover, words].spacing(12).align_y(Center))
        .width(Fill)
        .padding(8)
        .on_press(msg(Message::Pick(index)))
        .style(ui::row_style(tokens, picked, 10.0))
        .focusable(10.0, tokens)
}

/// A gray row while the lookup is on its way.
fn placeholder_row<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    let raised = tokens.raised;
    let block = |w: f32, h: f32| {
        container(space())
            .width(w)
            .height(h)
            .style(move |_| container::background(raised).border(Border::default().rounded(5)))
    };
    container(
        row![
            block(44.0, 62.0),
            column![block(180.0, 14.0), block(120.0, 12.0)].spacing(8)
        ]
        .spacing(12)
        .align_y(Center),
    )
    .padding(8)
    .into()
}

/// The "Platform · Any" chip under the search, with a cross to clear a
/// picked platform, and the picker it opens: a search, Any, Your
/// platforms and All platforms.
fn platform_filter<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let picked = form.platform_filter.as_deref();
    let chip = ui::filter_chip(
        format!("Platform · {}", picked.unwrap_or("Any")),
        None,
        None,
        picked.is_some(),
        tokens,
        msg(Message::TogglePlatformPicker),
    );
    let panel = form
        .platform_open
        .then(|| platform_picker(app, form, tokens));
    let anchor = ui::Popover::new(chip, panel)
        .align(ui::Align::Start)
        .on_dismiss(msg(Message::TogglePlatformPicker));
    let mut line = row![anchor].spacing(6).align_y(Center);
    if picked.is_some() {
        line = line.push(ui::icon_button(
            Icon::Close,
            12.0,
            tokens,
            msg(Message::PickPlatform(None)),
        ));
    }
    line.into()
}

fn platform_picker<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let query = form.platform_query.trim().to_lowercase();
    let searching = !query.is_empty();
    let picked = form.platform_filter.as_deref();
    let found = lookup::platforms_matching(&query);
    let yours = Form::your_platforms(library, app.settings.platform.as_deref());
    let others_of = |name: &str| {
        lookup::PLATFORMS
            .iter()
            .find(|(n, _)| *n == name)
            .map_or("", |(_, others)| others)
            .to_owned()
    };
    let row_of = |name: String, others: String, on: bool, pick: Option<String>| {
        let mut words = Column::new().spacing(1).push(text(name).size(14));
        if !others.is_empty() {
            words = words.push(text(others).size(12).color(tokens.sub));
        }
        let mark: Element<'a, AppMessage> = if on {
            ui::icon(Icon::Check, 14.0, tokens.ink)
        } else {
            space().width(14).into()
        };
        button(row![words.width(Fill), mark].spacing(10).align_y(Center))
            .width(Fill)
            .padding([7, 10])
            .on_press(msg(Message::PickPlatform(pick)))
            .style(ui::row_style(tokens, false, 8.0))
            .focusable(8.0, tokens)
    };
    let mut list = Column::new().spacing(2);
    let mut any = false;
    let section = |head: &str| -> Element<'a, AppMessage> {
        container(ui::section_header(head, tokens))
            .padding([8, 10])
            .into()
    };
    if !searching {
        list = list.push(section("Any"));
        list = list.push(row_of(
            "Any platform".into(),
            String::new(),
            picked.is_none(),
            None,
        ));
        any = true;
    }
    let matching = |name: &String| {
        !searching
            || format!("{name} {}", others_of(name))
                .to_lowercase()
                .contains(&query)
    };
    let yours: Vec<&String> = yours.iter().filter(|n| matching(n)).collect();
    // While searching, every hit sits under one "Matches" heading.
    if searching && (!yours.is_empty() || !found.is_empty()) {
        list = list.push(section("Matches"));
    }
    if !yours.is_empty() {
        if !searching {
            list = list.push(section("Your platforms"));
        }
        for name in yours {
            list = list.push(row_of(
                name.clone(),
                others_of(name),
                picked == Some(name.as_str()),
                Some(name.clone()),
            ));
        }
        any = true;
    }
    if !found.is_empty() {
        if !searching {
            list = list.push(section("All platforms"));
        }
        for (name, others) in found {
            list = list.push(row_of(
                name.to_owned(),
                others.to_owned(),
                picked == Some(name),
                Some(name.to_owned()),
            ));
        }
        any = true;
    }
    if !any {
        list = list.push(
            container(
                text("No platform called that.")
                    .size(13.5)
                    .color(tokens.sub),
            )
            .padding(10),
        );
    }
    let search = ui::search_field(
        "Search, e.g. SNES or Genesis",
        &form.platform_query,
        |q| msg(Message::PlatformQuery(q)),
        None,
        None,
        tokens,
    );
    let tokens_copy = *tokens;
    container(column![search, ui::scroll(list).height(320)].spacing(8))
        .width(340)
        .padding(10)
        .style(move |_| ui::panel_style(&tokens_copy, 12.0))
        .into()
}

/// The lookup field's id, focused when the dialog opens.
pub const LOOKUP_ID: &str = "entry-lookup";

/// The cover and the fields, in two columns where they pair up.
fn fields<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let blank = form.title.trim().is_empty() || form.source.is_none();
    // The cover is the match's, or another match's after Pick a cover.
    let tint = match (&form.cover_from, &form.source) {
        (Some(id), _) => theme::cover_color(id),
        (None, Some(source)) => theme::cover_color(source),
        (None, None) => theme::cover_color(&format!("{}/{}", form.media.key(), form.title)),
    };
    let cover_title = if form.title.is_empty() {
        "No title yet"
    } else {
        form.title.as_str()
    };
    let cover_face = container(
        text(cover_title)
            .size(18)
            .font(font::EXTRABOLD)
            .color(Color::WHITE),
    )
    .width(150)
    .height(225)
    .padding(14)
    .align_y(iced::alignment::Vertical::Bottom)
    .style(move |_| container::background(tint).border(Border::default().rounded(8)));
    // A click on the cover opens Pick a cover, once there is a match.
    let cover_button = button(cover_face)
        .padding(0)
        .on_press_maybe((!blank).then(|| msg(Message::OpenCover)))
        .style(|_, _| button::Style::default())
        .focusable(8.0, tokens);
    let note = if blank {
        "No cover yet"
    } else if form.cover_from.is_some() {
        "From another match"
    } else {
        "From the match · click to change"
    };
    let mut cover = column![
        cover_button,
        container(text(note).size(12).color(tokens.sub)).center_x(150),
    ]
    .spacing(10);
    if form.cover_from.is_some() {
        cover = cover.push(
            container(ui::labeled_button(
                "Use the match's cover",
                ButtonKind::Text,
                *tokens,
                Some(msg(Message::PickCover(None))),
            ))
            .center_x(150),
        );
    }

    let creator_label = browse::creator_label(Some(form.media));
    let mut released = column![ui::text_field(
        "2026-12-25",
        &form.released,
        |r| msg(Message::Released(r)),
        font::MONO,
        tokens
    )]
    .spacing(6);
    if !form.released_is_valid() {
        released = released.push(
            text("Use a year, a month or a day: 2026, 2026-12 or 2026-12-25.")
                .size(12.5)
                .color(tokens.danger),
        );
    } else if form.ebook_note && form.editing.is_none() {
        released = released.push(
            text("Year from the ebook edition. Check if it's a reprint.")
                .size(12.5)
                .color(tokens.sub),
        );
    }

    let creator = ui::labeled(
        creator_label,
        ui::text_field(
            creator_label,
            &form.creator,
            |c| msg(Message::Creator(c)),
            font::REGULAR,
            tokens,
        ),
        tokens,
    );
    let mut grid = Column::new().spacing(16).width(Fill);
    if form.editing.is_some() && form.fetched {
        let tokens_copy = *tokens;
        grid = grid.push(
            container(
                text("Details fetched online. You can change anything.")
                    .size(13)
                    .color(tokens.sub),
            )
            .width(Fill)
            .padding([8, 12])
            .style(move |_| {
                container::background(tokens_copy.raised).border(Border::default().rounded(8))
            }),
        );
    }
    // In Edit the Title label carries "Look up again".
    let mut title_label = row![
        text("Title")
            .size(12.5)
            .font(font::SEMIBOLD)
            .color(tokens.sub)
    ]
    .spacing(8)
    .align_y(Center);
    if form.editing.is_some() && !form.again {
        let ink = tokens.ink;
        title_label = title_label.push(
            button(
                row![
                    ui::icon(Icon::Search, 12.0, ink),
                    text("Look up again").size(12.5).font(font::SEMIBOLD),
                ]
                .spacing(4)
                .align_y(Center),
            )
            .padding(0)
            .on_press(msg(Message::LookAgain))
            .style(move |_, _| button::Style {
                text_color: ink,
                ..button::Style::default()
            })
            .focusable(4.0, tokens),
        );
    }
    grid = grid.push(
        column![
            title_label,
            ui::text_field(
                "Title",
                &form.title,
                |t| msg(Message::Title(t)),
                font::REGULAR,
                tokens,
            )
        ]
        .spacing(6),
    );
    // A show has no Released field: its seasons carry the dates.
    if form.uses_seasons() {
        grid = grid.push(creator);
    } else {
        grid = grid.push(pair(creator, ui::labeled("Released", released, tokens)));
    }
    if form.media == MediaType::Anime {
        grid = grid.push(anime_kind(form, tokens));
    }
    if form.media == MediaType::Books {
        let pages = ui::labeled(
            "Pages",
            ui::text_field(
                "Optional",
                &form.pages,
                |p| msg(Message::Pages(p)),
                font::MONO,
                tokens,
            ),
            tokens,
        );
        grid = grid.push(pair(pages, space().into()));
    }
    if form.can_have_series() {
        grid = grid.push(pair(
            ui::labeled(
                "Series",
                ui::text_field(
                    "Optional",
                    &form.series_name,
                    |s| msg(Message::SeriesName(s)),
                    font::REGULAR,
                    tokens,
                ),
                tokens,
            ),
            ui::labeled(
                "Number",
                ui::text_field(
                    "#",
                    &form.series_number,
                    |n| msg(Message::SeriesNumber(n)),
                    font::MONO,
                    tokens,
                ),
                tokens,
            ),
        ));
    }
    if let Some(extra) = type_fields(form, library, tokens) {
        grid = grid.push(extra);
    }
    if form.uses_seasons() {
        grid = grid.push(season_editor(app, form, tokens));
    }

    // A show with several seasons has its status set by them.
    let status: Element<'a, AppMessage> = if form.uses_seasons() && form.seasons.len() > 1 {
        let label = browse::status_label(Some(form.media), form.status_from_seasons(app.today));
        container(
            row![
                text(label).size(14).font(font::SEMIBOLD),
                text("· Set by the seasons").size(14).color(tokens.sub),
            ]
            .spacing(5)
            .align_y(Center),
        )
        .height(34)
        .align_y(Center)
        .into()
    } else {
        let statuses = EntryStatus::options_for(form.media).iter().map(|&status| {
            let label = browse::status_label(Some(form.media), status);
            (label, form.status == status, msg(Message::Status(status)))
        });
        ui::segmented(statuses, 34.0, tokens)
    };
    let rating = ui::rating(
        form.rating,
        form.rating_open,
        msg(Message::ToggleRating),
        |r| msg(Message::Rate(r)),
        tokens,
    );
    grid = grid
        .push(pair(
            ui::labeled("Status", status, tokens),
            ui::labeled("Rating", rating, tokens),
        ))
        .push(ui::labeled("Tags", tags(form, library, tokens), tokens));

    let editor = text_editor(&form.description)
        .on_action(|action| msg(Message::Description(action)))
        .height(84)
        .padding([9, 12])
        .size(14)
        .style({
            let tokens = *tokens;
            move |_, status| text_editor::Style {
                background: Background::Color(tokens.field),
                border: Border::default().rounded(8).width(1).color(match status {
                    text_editor::Status::Focused { .. } => tokens.ink,
                    _ => tokens.line,
                }),
                placeholder: tokens.sub,
                value: tokens.text,
                selection: tokens.selection,
            }
        });
    grid = grid
        .push(ui::labeled("Description", editor, tokens))
        .push(ui::labeled(
            "Note",
            ui::text_field(
                "Your own note (optional)",
                &form.note,
                |n| msg(Message::Note(n)),
                font::ITALIC,
                tokens,
            ),
            tokens,
        ));

    container(row![cover, grid].spacing(24))
        .padding([22, 26])
        .into()
}

/// Pick a cover: the match's own cover first, then the other matches for
/// the title, each with its year and edition. A click picks at once and
/// rings the cover; Done closes. Only the cover changes.
fn cover_dialog<'a>(form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let choices = form.cover_choices();
    let own = form.source.as_deref();
    let tiles = choices.into_iter().map(|found| {
        let is_own = Some(found.source) == own;
        let on = match &form.cover_from {
            Some(id) => id == found.source,
            None => is_own,
        };
        let tint = theme::cover_color(found.source);
        let (accent, line) = (tokens.accent, tokens.line);
        let cover = container(
            text(found.title)
                .size(13)
                .font(font::EXTRABOLD)
                .color(Color::WHITE),
        )
        .width(104)
        .height(156)
        .padding(10)
        .align_y(iced::alignment::Vertical::Bottom)
        .style(move |_| {
            container::background(tint).border(
                Border::default()
                    .rounded(8)
                    .width(if on { 3.0 } else { 1.0 })
                    .color(if on { accent } else { line }),
            )
        });
        let year = found.released.get(..4).unwrap_or_default();
        let sub = match found.edition {
            Some(edition) => format!("{year} · {edition}"),
            None => year.to_owned(),
        };
        let note = if is_own { "The match's cover" } else { "" };
        button(
            column![
                cover,
                container(
                    text(found.title)
                        .size(13)
                        .font(font::SEMIBOLD)
                        .wrapping(text::Wrapping::None)
                )
                .width(104)
                .clip(true),
                text(sub).size(12).color(tokens.sub),
                text(note).size(12).color(tokens.sub),
            ]
            .spacing(3)
            .width(104),
        )
        .padding(0)
        .on_press(msg(Message::PickCover(
            (!is_own).then(|| found.source.to_owned()),
        )))
        .style({
            let text_color = tokens.text;
            move |_, _| button::Style {
                text_color,
                ..button::Style::default()
            }
        })
        .focusable(8.0, tokens)
    });
    let body = column![
        text(format!(
            "Other matches for \u{201c}{}\u{201d}. Only the cover changes; title, year, description \
             and series stay.",
            form.title.trim()
        ))
        .size(14)
        .color(tokens.sub),
        Row::with_children(tiles).spacing(16).wrap().vertical_spacing(16),
    ]
    .spacing(16);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::CloseCover))
        ),
    ];
    ui::dialog(
        "Pick a cover",
        body,
        footer,
        560,
        tokens,
        msg(Message::CloseCover),
    )
}

/// An anime's "Series or movie" control, with what each means. Unpicked
/// for an anime from before kinds, until the owner says.
fn anime_kind<'a>(form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let kinds =
        [("Series", EntryKind::Series), ("Movie", EntryKind::Movie)].map(|(label, kind)| {
            (
                label,
                form.anime_kind == Some(kind),
                msg(Message::AnimeKind(kind)),
            )
        });
    let help = match form.anime_kind {
        Some(EntryKind::Series) => "Works like a show: seasons, episodes, status from the seasons.",
        Some(_) => "Works like a movie: one released date, can be part of a series.",
        None => "Not decided yet: pick one.",
    };
    ui::labeled(
        "Series or movie",
        column![
            container(ui::segmented(kinds, 34.0, tokens)).width(200),
            text(help).size(12.5).color(tokens.sub),
        ]
        .spacing(6),
        tokens,
    )
}

/// The season editor: one line per season (the name, the date, the
/// status pill and a remove cross), "+ Add season", and the note that a
/// date changed here is the owner's.
fn season_editor<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let today = app.today;
    let several = form.seasons.len() > 1;
    let mut lines = Column::new().spacing(8);
    for season in &form.seasons {
        lines = lines.push(season_line(form, season, several, today, tokens));
    }
    let add = ui::add_chip("+ Add season", tokens, msg(Message::AddSeason));
    ui::labeled(
        "Seasons",
        column![
            lines,
            add,
            text("A date you change here is yours: the daily check never overwrites it.")
                .size(12.5)
                .color(tokens.sub),
        ]
        .spacing(10),
        tokens,
    )
}

fn season_line<'a>(
    form: &'a Form,
    season: &'a SeasonDraft,
    several: bool,
    today: crate::model::Date,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let number = season.number;
    let date = ui::text_field(
        "Not set",
        &season.released,
        move |d| msg(Message::SeasonDate(number, d)),
        font::MONO,
        tokens,
    )
    .width(170);
    let mut line = row![
        container(
            text(format!("Season {number}"))
                .size(14)
                .font(font::SEMIBOLD)
        )
        .width(90),
        date,
    ]
    .spacing(12)
    .align_y(Center);
    if !season.is_valid() {
        line = line.push(
            text("Use 2026, 2026-12 or 2026-12-25.")
                .size(12.5)
                .color(tokens.danger),
        );
    }
    if several {
        let earlier = form.seasons.iter().any(|s| {
            s.number < number
                && s.released().is_none_or(|r| r.is_released_on(today))
                && s.status != Some(EntryStatus::Done)
        });
        let catch_up = earlier.then(|| {
            let done = browse::status_label(Some(form.media), EntryStatus::Done);
            let label = if number == 2 {
                format!("Mark season 1 {done}")
            } else {
                format!("Mark seasons 1–{} {done}", number - 1)
            };
            (label, msg(Message::SeasonCatchUp(number)))
        });
        let open = form.season_menu == Some(number);
        line = line.push(ui::season_pill(
            form.media,
            season.status,
            open,
            msg(Message::FormSeasonMenu((!open).then_some(number))),
            move |status| msg(Message::SeasonStatus(number, status)),
            catch_up,
            tokens,
        ));
    }
    line = line.push(space::horizontal()).push(ui::icon_button(
        Icon::Close,
        12.0,
        tokens,
        msg(Message::RemoveSeasonDraft(number)),
    ));
    line.into()
}

/// Two fields side by side, sharing the width.
fn pair<'a>(
    left: Element<'a, AppMessage>,
    right: Element<'a, AppMessage>,
) -> Element<'a, AppMessage> {
    row![container(left).width(Fill), container(right).width(Fill)]
        .spacing(18)
        .into()
}

/// The fields only some types have: platforms for games, formats for
/// books, album or song (and the album) for music.
fn type_fields<'a>(
    form: &'a Form,
    library: &crate::model::Library,
    tokens: &Tokens,
) -> Option<Element<'a, AppMessage>> {
    match form.media {
        MediaType::Games => {
            let mut chips =
                Row::with_children(form.platform_options(library).into_iter().map(|platform| {
                    let on = form.platforms_on.contains(&platform);
                    ui::filter_chip(
                        platform.clone(),
                        None,
                        None,
                        on,
                        tokens,
                        msg(Message::TogglePlatform(platform)),
                    )
                }))
                .spacing(6);
            chips = match &form.other_platform {
                Some(name) => chips.push(
                    container(
                        ui::text_field(
                            "Platform",
                            name,
                            |n| msg(Message::OtherPlatform(n)),
                            font::REGULAR,
                            tokens,
                        )
                        .on_submit(msg(Message::AddOtherPlatform))
                        .size(13)
                        .padding([5, 10]),
                    )
                    .width(160),
                ),
                None => chips.push(ui::add_chip(
                    "+ Other",
                    tokens,
                    msg(Message::OpenOtherPlatform),
                )),
            };
            Some(ui::labeled(
                "Platform",
                chips.wrap().vertical_spacing(6),
                tokens,
            ))
        }
        MediaType::Books => {
            let chips = row![
                ui::filter_chip(
                    "Audiobook".into(),
                    None,
                    None,
                    form.formats.contains(&BookFormat::Audiobook),
                    tokens,
                    msg(Message::ToggleFormat(BookFormat::Audiobook))
                ),
                ui::filter_chip(
                    "Physical".into(),
                    None,
                    None,
                    form.formats.contains(&BookFormat::Physical),
                    tokens,
                    msg(Message::ToggleFormat(BookFormat::Physical))
                ),
            ]
            .spacing(6);
            Some(ui::labeled("Formats", chips, tokens))
        }
        MediaType::Music => {
            let kinds = [("Album", EntryKind::Album), ("Song", EntryKind::Song)]
                .map(|(label, kind)| (label, form.kind == kind, msg(Message::Kind(kind))));
            let kind = ui::labeled(
                "Album or song",
                container(ui::segmented(kinds, 34.0, tokens)).width(200),
                tokens,
            );
            let album = (form.kind == EntryKind::Song).then(|| {
                ui::labeled(
                    "Album (optional)",
                    ui::text_field(
                        "Album",
                        &form.album,
                        |a| msg(Message::Album(a)),
                        font::REGULAR,
                        tokens,
                    ),
                    tokens,
                )
            });
            Some(match album {
                Some(album) => pair(kind, album),
                None => kind,
            })
        }
        _ => None,
    }
}

/// The picked tags, the button that opens the picker, and the lookup's
/// suggestions not taken yet ("new" when the library has no such tag).
fn tags<'a>(
    form: &'a Form,
    library: &crate::model::Library,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    // A tag in several categories is one chip per role it has here.
    let mut chosen = Row::new().spacing(6).align_y(Center);
    for tag in &form.tags {
        let roles = form.roles_on(tag, library);
        if roles.is_empty() {
            chosen = chosen.push(ui::removable_chip(
                format!("#{tag}"),
                None,
                None,
                tokens,
                msg(Message::RemoveTag(tag.clone(), None)),
            ));
        }
        for role in roles {
            chosen = chosen.push(ui::removable_chip(
                format!("#{tag}"),
                Some(role.role_label().to_owned()),
                None,
                tokens,
                msg(Message::RemoveTag(tag.clone(), Some(role))),
            ));
        }
    }
    let more = if form.tags.is_empty() {
        "Add tags ›"
    } else {
        "Add more tags ›"
    };
    chosen = chosen.push(ui::add_chip(more, tokens, msg(Message::OpenPicker)));
    let mut column = Column::new()
        .spacing(8)
        .push(chosen.wrap().vertical_spacing(6));
    if !form.suggested.is_empty() {
        let mut suggested = Row::new()
            .spacing(6)
            .align_y(Center)
            .push(text("Suggested").size(12).color(tokens.sub));
        for suggestion in &form.suggested {
            let tag = &suggestion.tag;
            let new = !library.all_tags().any(|t| t == tag);
            // "new", "new · Actor", or "· Actor" for a person the library
            // has under another people category.
            let mut notes = Vec::new();
            if new {
                notes.push("new".to_owned());
            }
            if let Some(category) = suggestion.category {
                notes.push(category.role_label().to_owned());
            }
            let label = if notes.is_empty() {
                format!("+ #{tag}")
            } else {
                format!("+ #{tag}  {}", notes.join(" · "))
            };
            suggested = suggested.push(ui::add_chip(
                label,
                tokens,
                msg(Message::AddSuggested(tag.clone())),
            ));
        }
        column = column.push(suggested.wrap().vertical_spacing(6));
    }
    column.into()
}

/// A plan in "Also add to plan".
#[derive(Debug, Clone, PartialEq)]
struct PlanChoice {
    id: Option<String>,
    title: String,
}

impl fmt::Display for PlanChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.title)
    }
}

/// What the form does, "Also add to plan", Cancel and the main action.
fn footer<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let hint = match (&form.editing, &form.source) {
        (Some(_), _) => "Every field can be edited.",
        (None, Some(_)) => "Fetched online. Every field can be edited.",
        (None, None) => "Fill in what you know; only the title is needed.",
    };
    let library = app.repo.library();
    let none = PlanChoice {
        id: None,
        title: "None".into(),
    };
    let choices: Vec<PlanChoice> = std::iter::once(none.clone())
        .chain(library.plans.iter().map(|plan| PlanChoice {
            id: Some(plan.id.clone()),
            title: plan.title(),
        }))
        .collect();
    let selected = choices
        .iter()
        .find(|c| c.id == form.also_plan)
        .cloned()
        .unwrap_or(none);
    let plans = pick_list(choices, Some(selected), |choice: PlanChoice| {
        msg(Message::AlsoPlan(choice.id))
    })
    .text_size(13.5)
    .padding([7, 10])
    .style({
        let tokens = *tokens;
        move |_, _| pick_list::Style {
            text_color: tokens.text,
            placeholder_color: tokens.sub,
            handle_color: tokens.sub,
            background: Background::Color(tokens.field),
            border: Border::default().rounded(8).width(1).color(tokens.line),
        }
    })
    .menu_style({
        let tokens = *tokens;
        move |_| iced::overlay::menu::Style {
            background: Background::Color(tokens.popover),
            border: Border::default().rounded(8).width(1).color(tokens.line),
            text_color: tokens.text,
            selected_text_color: tokens.text,
            selected_background: Background::Color(tokens.selection),
            shadow: Shadow::default(),
        }
    });
    let save = if form.editing.is_some() {
        "Save"
    } else {
        "Add to library"
    };
    container(
        row![
            text(hint).size(12.5).color(tokens.sub),
            space::horizontal(),
            text("Also add to plan").size(13).color(tokens.sub),
            plans,
            hinted_button(
                "Cancel",
                "Esc",
                ButtonKind::Outline,
                tokens,
                Some(msg(Message::Close))
            ),
            hinted_button(
                save,
                "Ctrl ↵",
                ButtonKind::Primary,
                tokens,
                form.can_save().then(|| msg(Message::Save))
            ),
        ]
        .spacing(10)
        .align_y(Center),
    )
    .height(62)
    .padding(Padding::new(0.0).left(24).right(20))
    .align_y(Center)
    .into()
}

/// A button with its keyboard shortcut after the label, in mono.
fn hinted_button<'a>(
    label: &'a str,
    shortcut: &'a str,
    kind: ButtonKind,
    tokens: &Tokens,
    on_press: Option<AppMessage>,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let (fill, fg, border) = match kind {
        ButtonKind::Primary => (Some(tokens.accent), tokens.on_accent, None),
        _ => (None, tokens.text, Some(tokens.line)),
    };
    let disabled = on_press.is_none();
    button(
        row![
            text(label).size(14).font(font::SEMIBOLD),
            ui::mono(shortcut, 11.0, fg.scale_alpha(0.7))
        ]
        .spacing(8)
        .align_y(Center)
        .height(Fill),
    )
    .height(36)
    .padding([0, 16])
    .on_press_maybe(on_press)
    .style(move |_, _| button::Style {
        background: fill.map(|c| Background::Color(if disabled { c.scale_alpha(0.5) } else { c })),
        text_color: if disabled { fg.scale_alpha(0.5) } else { fg },
        border: Border::default()
            .rounded(8)
            .width(if border.is_some() { 1.0 } else { 0.0 })
            .color(border.unwrap_or(Color::TRANSPARENT)),
        ..button::Style::default()
    })
    .focusable(8.0, &tokens)
}

/// The form's tag picker: the shared picker with a search that can also
/// create a tag (with a "Did you mean" for near misses), the right-click
/// menu and "+ Add to Directors", then Clear and Done.
fn tag_picker<'a>(
    app: &'a App,
    form: &'a Form,
    state: &'a PickerState,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let query = rules::normalize_tag(&state.query);
    let searching = !query.is_empty();
    let groups = form.picker_groups(library);
    let holds_a_tag: Vec<TagCategory> = groups
        .iter()
        .filter(|(_, names)| names.iter().any(|n| form.tags.contains(n)))
        .map(|(c, _)| *c)
        .collect();
    let media = form.media;

    let picker = Picker {
        state,
        groups,
        counts: library.tag_counts().into_iter().collect(),
        categories_of: Box::new(move |tag| form.categories_of(tag, library)),
        is_on: Box::new(move |tag, category| form.has_tag_as(tag, category, library)),
        // Other types' genres, Actors and Streaming stay out of the way
        // unless a group holds a tag on this entry, or you search.
        visible: Box::new(move |category| {
            category.offered_for(media) || holds_a_tag.contains(&category)
        }),
        default_open: Box::new(|_, selected| selected > 0),
        long_group: crate::screens::library::long_group(app),
        with_menu: true,
        with_add_to: true,
        height: 340.0,
        map: Rc::new(|message| msg(Message::Picker(message))),
    };

    let exists =
        library.all_tags().any(|t| t == query) || form.new_tags.iter().any(|(t, _)| *t == query);
    let create = (searching && !exists).then(|| create_box(app, form, &query, tokens));
    let header = picker.header("Tags", None, None, msg(Message::ClosePicker), tokens);
    let body = picker.body("Search or create a tag", create, tokens);

    let summary = if form.tags.is_empty() {
        "No tags selected".to_owned()
    } else {
        let names: Vec<String> = form
            .tags
            .iter()
            .map(|t| crate::model::tag_label(t, &form.roles_on(t, library)))
            .collect();
        format!("{} selected: {}", form.tags.len(), names.join(", "))
    };
    let footer = ui::tag_picker::footer(
        "Clear",
        Some(msg(Message::PickerClear)),
        summary,
        "Done".to_owned(),
        msg(Message::ClosePicker),
        tokens,
    );
    ui::panel(header, body, footer, 560, tokens)
}

/// Shown while the search names a tag the library does not have: a
/// "Did you mean" for a near miss, and the button that creates it.
fn create_box<'a>(
    app: &'a App,
    form: &Form,
    query: &str,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let known = library
        .all_tags()
        .chain(form.new_tags.iter().map(|(t, _)| t.as_str()));
    let similar = rules::find_similar_tag(query, known).map(str::to_owned);
    let mut content = Column::new().spacing(8);
    let hint = match &similar {
        Some(existing) => {
            content = content.push(
                row![
                    text("Did you mean").size(13),
                    ui::filter_chip(
                        format!("#{existing}"),
                        None,
                        None,
                        false,
                        tokens,
                        msg(Message::Picker(crate::tags::PickerMessage::Toggle(
                            existing.clone(),
                            None
                        )))
                    ),
                    text("?").size(13),
                ]
                .spacing(6)
                .align_y(Center),
            );
            "Or create a new tag anyway.".to_owned()
        }
        None => format!("No tag called #{query} yet."),
    };
    content = content.push(
        row![
            text(hint).size(13).color(tokens.sub).width(Fill),
            ui::labeled_button(
                format!("Create #{query}"),
                ButtonKind::Outline,
                *tokens,
                Some(msg(Message::CreateTag))
            ),
        ]
        .spacing(10)
        .align_y(Center),
    );
    let line = tokens.line;
    container(content)
        .padding([10, 12])
        .width(Fill)
        .style(move |_| {
            container::Style::default().border(Border::default().rounded(10).width(1).color(line))
        })
        .into()
}

/// Creating a tag: confirm it and pick its categories, one or several of
/// one kind. Streaming is left out for every type but movies and shows.
fn new_tag_dialog<'a>(
    new_tag: &'a NewTag,
    media: MediaType,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let categories = Row::with_children(
        TagCategory::ALL
            .into_iter()
            .filter(|c| c.used_for(media))
            .map(|category| {
                ui::filter_chip(
                    category.name().to_owned(),
                    None,
                    None,
                    new_tag.categories.contains(&category),
                    tokens,
                    msg(Message::NewTagCategory(category)),
                )
            }),
    )
    .spacing(6)
    .wrap()
    .vertical_spacing(6);
    let sentence = if new_tag.categories.is_empty() {
        "Pick at least one category.".to_owned()
    } else {
        let names: Vec<&str> = new_tag.categories.iter().map(|c| c.name()).collect();
        format!(
            "It will be added to your tag list under {}.",
            join_names(&names)
        )
    };
    let body = column![
        row![
            text(format!("#{}", new_tag.name)).size(14).font(font::BOLD),
            text(" is not in your tag list yet.")
                .size(14)
                .color(tokens.sub),
        ],
        column![ui::section_header("Categories", tokens), categories].spacing(8),
        text(sentence).size(13).color(tokens.sub),
        text(
            "People can be in more than one people category (Authors, Directors, Actors), \
             genres in more than one genre category. Picking a category of another kind \
             starts over with it."
        )
        .size(12.5)
        .color(tokens.sub),
    ]
    .spacing(14);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::CancelNewTag))
        ),
        ui::labeled_button(
            "Create tag",
            ButtonKind::Primary,
            *tokens,
            (!new_tag.categories.is_empty()).then(|| msg(Message::ConfirmNewTag))
        ),
    ]
    .spacing(8);
    ui::dialog(
        "Create a new tag?",
        body,
        footer,
        500,
        tokens,
        msg(Message::CancelNewTag),
    )
}

/// "Genres", "Genres and Music genres", "Authors, Directors and Actors".
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A tag's categories (right-click › Categories…): the categories of its
/// kind as chips, at least one picked, with where the tag is used; or, for
/// a tag of a kind with one category each, the group to move it to.
fn categories_dialog<'a>(
    app: &'a App,
    form: &'a Form,
    dialog: &'a CategoriesDialog,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let family = if dialog.single {
        TagCategory::single_category_groups()
    } else {
        dialog.draft.first().map(|c| c.family()).unwrap_or_default()
    };
    let chips = Row::with_children(family.into_iter().map(|category| {
        ui::filter_chip(
            category.name().to_owned(),
            None,
            None,
            dialog.draft.contains(&category),
            tokens,
            msg(Message::CategoryDraft(category)),
        )
    }))
    .spacing(6)
    .wrap()
    .vertical_spacing(6);
    let (head, hint, save) = if dialog.single {
        (
            "Group",
            "Moves the tag to another group. It stays on all its entries.",
            "Move",
        )
    } else if dialog.draft.first().and_then(|c| c.kind()) == Some(crate::model::TagKind::People) {
        (
            "Categories",
            "A person can be in Authors, Directors and Actors. At least one stays; unticking \
             one takes that role off every entry.",
            "Save",
        )
    } else {
        (
            "Categories",
            "A genre can be in Genres, Game genres and Music genres. At least one stays.",
            "Save",
        )
    };
    let body = column![
        text(form.usage(&dialog.tag, library))
            .size(13)
            .color(tokens.sub),
        column![ui::section_header(head, tokens), chips].spacing(8),
        text(hint).size(12.5).color(tokens.sub),
    ]
    .spacing(14);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::CancelCategories))
        ),
        ui::labeled_button(
            save,
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::SaveCategories))
        ),
    ]
    .spacing(8);
    ui::dialog(
        format!("#{}", dialog.tag),
        body,
        footer,
        500,
        tokens,
        msg(Message::CancelCategories),
    )
}
