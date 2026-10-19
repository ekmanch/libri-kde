//! The Add and Edit dialog: the type, the online lookup on the left (Add
//! only), the form on the right, and the footer with "Also add to plan".
//! The tag picker and the new-tag confirmation open over it.

use std::collections::HashMap;
use std::fmt;

use iced::widget::{
    Column, Row, button, column, container, pick_list, row, space, text, text_editor,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length, Padding, Shadow, Vector};

use super::{Form, Message, NewTag, Replace, TagPicker};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{BookFormat, EntryStatus, MediaType, MusicKind, TagCategory, text as rules};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

pub fn view<'a>(app: &'a App, form: &'a Form) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let adding = form.editing.is_none();

    let mut body = Row::new().height(Fill);
    if adding {
        body = body.push(lookup(form, &tokens));
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
            background: Some(Background::Color(tokens.content)),
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
    let panel = container(panel).padding([30, 40]).center(Fill).into();

    let with_picker = match &form.picker {
        Some(picker) => ui::modal(
            panel,
            Some(tag_picker(app, form, picker, &tokens)),
            msg(Message::ClosePicker),
            &tokens,
        ),
        None => panel,
    };
    let with_new_tag = match &form.new_tag {
        Some(new_tag) => ui::modal(
            with_picker,
            Some(new_tag_dialog(new_tag, &tokens)),
            msg(Message::CancelNewTag),
            &tokens,
        ),
        None => with_picker,
    };
    match &form.replace {
        Some(replace) => ui::modal(
            with_new_tag,
            Some(replace_dialog(replace, &tokens)),
            msg(Message::ReplaceCancel),
            &tokens,
        ),
        None => with_new_tag,
    }
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

/// The lookup: a search, its matches (the arrow keys move, Enter picks),
/// and "Enter manually instead".
fn lookup<'a>(form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let matches = form.matches();
    let query = form.query.trim();
    let hint = if query.is_empty() {
        "Type a title to look it up online.".to_owned()
    } else if matches.is_empty() {
        format!("No matches for \"{query}\".")
    } else {
        let n = matches.len();
        let found = if n == 1 {
            "1 match".to_owned()
        } else {
            format!("{n} matches")
        };
        format!("{found} found online · ↑ ↓ to move, Enter to pick")
    };
    let rows = Column::with_children(matches.iter().enumerate().map(|(index, found)| {
        let picked = form.picked == Some(index);
        let year = found.released.get(..4).unwrap_or_default();
        let tint = theme::cover_color(found.source);
        let cover = container(space())
            .width(44)
            .height(62)
            .style(move |_| container::background(tint).border(Border::default().rounded(5)));
        let words = column![
            text(found.title).size(14.5).font(font::SEMIBOLD),
            text(format!("{} · {year}", found.creator))
                .size(12.5)
                .color(tokens.sub),
        ]
        .spacing(2);
        button(row![cover, words].spacing(12).align_y(Center))
            .width(Fill)
            .padding(8)
            .on_press(msg(Message::Pick(index)))
            .style(ui::row_style(tokens, picked, 10.0))
            .into()
    }))
    .spacing(4);
    let manual = ui::add_chip("Enter manually instead", tokens, msg(Message::Manual));
    let chrome = tokens.chrome;
    container(
        column![
            ui::search_field(
                "Search online",
                &form.query,
                |query| msg(Message::Query(query)),
                None,
                Some(LOOKUP_ID),
                tokens
            ),
            text(hint).size(12).color(tokens.sub),
            ui::scroll(rows).height(Fill),
            container(manual).center_x(Fill),
        ]
        .spacing(12),
    )
    .width(380)
    .height(Fill)
    .padding(18)
    .style(move |_| container::background(chrome))
    .into()
}

/// The lookup field's id, focused when the dialog opens.
pub const LOOKUP_ID: &str = "entry-lookup";

/// The cover and the fields, in two columns where they pair up.
fn fields<'a>(app: &'a App, form: &'a Form, tokens: &Tokens) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let tint = theme::cover_color(&format!("{}/{}", form.media.key(), form.title));
    let cover_title = if form.title.is_empty() {
        "No title yet"
    } else {
        form.title.as_str()
    };
    let cover = column![
        container(
            text(cover_title)
                .size(18)
                .font(font::EXTRABOLD)
                .color(Color::WHITE)
        )
        .width(150)
        .height(225)
        .padding(14)
        .align_y(iced::alignment::Vertical::Bottom)
        .style(move |_| container::background(tint).border(Border::default().rounded(8))),
        container(
            text(if form.source.is_some() {
                "Cover fetched online"
            } else {
                "No cover yet"
            })
            .size(12)
            .color(tokens.sub)
        )
        .center_x(150),
    ]
    .spacing(10);

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
    }

    let mut grid = Column::new()
        .spacing(16)
        .width(Fill)
        .push(ui::labeled(
            "Title",
            ui::text_field(
                "Title",
                &form.title,
                |t| msg(Message::Title(t)),
                font::REGULAR,
                tokens,
            ),
            tokens,
        ))
        .push(pair(
            ui::labeled(
                creator_label,
                ui::text_field(
                    creator_label,
                    &form.creator,
                    |c| msg(Message::Creator(c)),
                    font::REGULAR,
                    tokens,
                ),
                tokens,
            ),
            ui::labeled("Released", released, tokens),
        ));
    if let Some(extra) = type_fields(form, library, tokens) {
        grid = grid.push(extra);
    }

    let statuses = EntryStatus::options_for(form.media).iter().map(|&status| {
        let label = browse::status_label(Some(form.media), status);
        (label, form.status == status, msg(Message::Status(status)))
    });
    let rating = ui::rating(
        form.rating,
        form.rating_open,
        msg(Message::ToggleRating),
        |r| msg(Message::Rate(r)),
        tokens,
    );
    grid = grid
        .push(pair(
            ui::labeled("Status", ui::segmented(statuses, 34.0, tokens), tokens),
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
            let kinds = [("Album", MusicKind::Album), ("Song", MusicKind::Song)]
                .map(|(label, kind)| (label, form.kind == kind, msg(Message::Kind(kind))));
            let kind = ui::labeled(
                "Album or song",
                container(ui::segmented(kinds, 34.0, tokens)).width(200),
                tokens,
            );
            let album = (form.kind == MusicKind::Song).then(|| {
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
    let mut chosen = Row::new().spacing(6).align_y(Center);
    for tag in &form.tags {
        chosen = chosen.push(ui::removable_chip(
            format!("#{tag}"),
            None,
            tokens,
            msg(Message::RemoveTag(tag.clone())),
        ));
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
        for tag in &form.suggested {
            let new = !library.all_tags().any(|t| t == tag);
            let label = if new {
                format!("+ #{tag}  new")
            } else {
                format!("+ #{tag}")
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
    .into()
}

/// The form's tag picker: a search that can also create a tag (with a
/// "Did you mean" for near misses), groups that fold, Clear and Done.
fn tag_picker<'a>(
    app: &'a App,
    form: &'a Form,
    picker: &'a TagPicker,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let query = rules::normalize_tag(&picker.query);
    let searching = !query.is_empty();
    let counts: HashMap<&str, usize> = library.tag_counts().into_iter().collect();

    // The library's groups, plus the tags created in this form.
    let mut groups: Vec<(TagCategory, Vec<&str>)> = Vec::new();
    for category in TagCategory::ALL {
        let mut names: Vec<&str> = library
            .tags
            .get(&category)
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        names.extend(
            form.new_tags
                .iter()
                .filter(|(_, c)| *c == category)
                .map(|(n, _)| n.as_str()),
        );
        names.retain(|n| n.contains(&query));
        names.sort_by_key(|n| n.to_lowercase());
        let offered = searching
            || category.offered_for(form.media)
            || names.iter().any(|n| form.tags.iter().any(|t| t == n));
        if !names.is_empty() && offered {
            groups.push((category, names));
        }
    }

    let mut list = Column::new().spacing(2);
    for (category, names) in &groups {
        let selected = names
            .iter()
            .filter(|n| form.tags.iter().any(|t| t == *n))
            .count();
        // Groups holding a tag on this entry start open; a search opens all.
        let open = searching || ((selected > 0) != picker.toggled.contains(category));
        let chips = names
            .iter()
            .map(|&name| {
                let on = form.tags.iter().any(|t| t == name);
                let count = counts.get(name).copied().unwrap_or(0);
                ui::filter_chip(
                    format!("#{name}"),
                    None,
                    Some(count),
                    on,
                    tokens,
                    msg(Message::PickerToggleTag(name.to_owned())),
                )
            })
            .collect();
        list = list.push(ui::tag_group(
            category.name(),
            names.len(),
            selected,
            open,
            msg(Message::PickerToggleGroup(*category)),
            chips,
            tokens,
        ));
    }

    let mut body = Column::new().spacing(12).push(ui::search_field(
        "Search or create a tag",
        &picker.query,
        |q| msg(Message::PickerQuery(q)),
        None,
        None,
        tokens,
    ));
    let exists =
        library.all_tags().any(|t| t == query) || form.new_tags.iter().any(|(t, _)| *t == query);
    if searching && !exists {
        body = body.push(create_box(app, form, &query, tokens));
    } else if groups.is_empty() {
        body = body.push(text("No tags match.").size(13.5).color(tokens.sub));
    }
    body = body.push(ui::scroll(list).height(Length::Fixed(340.0)));

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
            Some(msg(Message::PickerClear))
        ),
        text(summary).size(13).color(tokens.sub).width(Fill),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::ClosePicker))
        ),
    ]
    .spacing(10)
    .align_y(Center);
    ui::dialog("Tags", body, footer, 560, tokens, msg(Message::ClosePicker))
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
                        msg(Message::PickerToggleTag(existing.clone()))
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

/// Creating a tag: confirm it and pick its category.
fn new_tag_dialog<'a>(new_tag: &'a NewTag, tokens: &Tokens) -> Element<'a, AppMessage> {
    let categories = Row::with_children(TagCategory::ALL.map(|category| {
        ui::filter_chip(
            category.name().to_owned(),
            None,
            None,
            new_tag.category == category,
            tokens,
            msg(Message::NewTagCategory(category)),
        )
    }))
    .spacing(6)
    .wrap()
    .vertical_spacing(6);
    let body = column![
        row![
            text(format!("#{}", new_tag.name)).size(14).font(font::BOLD),
            text(" is not in your tag list yet.")
                .size(14)
                .color(tokens.sub),
        ],
        column![ui::section_header("Category", tokens), categories].spacing(8),
        text(format!(
            "It will be added to your tag list under {}.",
            new_tag.category.name()
        ))
        .size(13)
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
            Some(msg(Message::ConfirmNewTag))
        ),
    ]
    .spacing(8);
    ui::dialog(
        "Create a new tag?",
        body,
        footer,
        480,
        tokens,
        msg(Message::CancelNewTag),
    )
}
