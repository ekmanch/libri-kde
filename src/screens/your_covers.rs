//! Settings › Library source › Your covers: the owner's own covers in
//! the library repo (`covers`), looked after in one place.
//!
//! Check for online covers asks the lookup about every title that shows
//! one of the owner's files (but the ones marked Keep mine) and lists
//! the hits in one dialog: a choice per title (use the online cover and
//! delete the file, use it and keep the file, keep the owner's), Set all
//! for the lot, and Apply, which says how many files it deletes. Unused
//! covers lists the files no title shows and whose name matches no
//! title, Delete or Keep per file. The third row only says what an
//! upload does.

use iced::widget::{Column, Row, column, container, row, space, text};
use iced::{Border, Center, Element, Fill, Task};

use super::entry::pick::{self, OnlineChoice};
use crate::app::{App, Message as AppMessage};
use crate::artwork::{self, Wants};
use crate::browse;
use crate::covers::{self, Change};
use crate::model::Entry;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind};

/// The dialogs' widths.
const FOUND_WIDTH: f32 = 900.0;
const UNUSED_WIDTH: f32 = 640.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The check is running.
    pub checking: bool,
    /// The last check found nothing: said under the row until the page
    /// is left.
    pub none_found: bool,
    /// The dialog of the check's hits.
    pub found: Option<Vec<Found>>,
    /// How many titles the check asked about, for the dialog's words.
    pub checked: usize,
    /// The dialog of unused files: each path, and whether it is deleted.
    pub unused: Option<Vec<(String, bool)>>,
}

/// A title that shows one of the owner's files and has an online cover
/// by now.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub id: String,
    pub url: String,
    pub choice: OnlineChoice,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Check,
    /// The check's hits: entry ids with the cover the lookup has.
    Checked(Vec<(String, String)>),
    Choice(usize, OnlineChoice),
    SetAll(OnlineChoice),
    Apply,
    CloseFound,
    Review,
    /// Delete (true) or Keep one of the unused files.
    Unused(usize, bool),
    DeleteUnused,
    CloseUnused,
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Settings(super::settings::Message::Covers(message))
}

/// The entries the check asks about: the ones that show one of the
/// owner's files and are not marked Keep mine.
fn asked(app: &App) -> Vec<Entry> {
    covers::entries_using(app.repo.library())
        .filter(|entry| entry.cover_keep_mine != Some(true))
        .cloned()
        .collect()
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Check => {
            if app.settings_page.covers.checking {
                return Task::none();
            }
            let entries = asked(app);
            let page = &mut app.settings_page.covers;
            page.none_found = false;
            page.checked = entries.len();
            if entries.is_empty() {
                page.none_found = true;
                return Task::none();
            }
            let service = app.lookups.clone();
            let ask = move || -> Vec<(String, String)> {
                entries
                    .into_iter()
                    .filter_map(|entry| {
                        let plain = Entry {
                            cover_from: None,
                            ..entry
                        };
                        let (url, _) = artwork::resolve_cover(&service, &plain).ok()?;
                        Some((plain.id, url?))
                    })
                    .collect()
            };
            if app.lookups.is_instant() {
                return update(app, Message::Checked(ask()));
            }
            page.checking = true;
            // The files' list with it, so the dialog shows what is there
            // now.
            let listing = app.list_covers();
            let check = Task::perform(crate::app::blocking(move || Ok(ask())), |result| {
                msg(Message::Checked(result.unwrap_or_default()))
            });
            return Task::batch([listing, check]);
        }
        Message::Checked(hits) => {
            let library = app.repo.library();
            // An entry changed meanwhile is left out.
            let found: Vec<Found> = hits
                .into_iter()
                .filter(|(id, _)| {
                    library
                        .entry(id)
                        .is_some_and(|entry| covers::is_repo(entry.cover_from.as_deref()))
                })
                .map(|(id, url)| Found {
                    id,
                    url,
                    choice: OnlineChoice::default(),
                })
                .collect();
            let page = &mut app.settings_page.covers;
            page.checking = false;
            page.none_found = found.is_empty();
            page.found = (!found.is_empty()).then_some(found);
        }
        Message::Choice(index, choice) => {
            if let Some(found) = app
                .settings_page
                .covers
                .found
                .as_mut()
                .and_then(|found| found.get_mut(index))
            {
                found.choice = choice;
            }
        }
        Message::SetAll(choice) => {
            for found in app.settings_page.covers.found.iter_mut().flatten() {
                found.choice = choice;
            }
        }
        Message::Apply => {
            let found = app.settings_page.covers.found.take().unwrap_or_default();
            for found in found {
                pick::answer_online(app, &found.id, found.choice);
            }
        }
        Message::CloseFound => app.settings_page.covers.found = None,
        Message::Review => {
            let unused: Vec<(String, bool)> = app
                .covers
                .unused(app.repo.library())
                .into_iter()
                .map(|file| (file.path.clone(), true))
                .collect();
            app.settings_page.covers.unused = (!unused.is_empty()).then_some(unused);
        }
        Message::Unused(index, delete) => {
            if let Some(file) = app
                .settings_page
                .covers
                .unused
                .as_mut()
                .and_then(|files| files.get_mut(index))
            {
                file.1 = delete;
            }
        }
        Message::DeleteUnused => {
            let files = app.settings_page.covers.unused.take().unwrap_or_default();
            // Only what is still unused goes: a title may have picked a
            // file while the dialog was open.
            let still: Vec<String> = app
                .covers
                .unused(app.repo.library())
                .into_iter()
                .map(|file| file.path.clone())
                .collect();
            let deletes: Vec<Change> = files
                .into_iter()
                .filter(|(path, delete)| *delete && still.contains(path))
                .map(|(path, _)| Change::Delete { path })
                .collect();
            let n = deletes.len();
            let summary = format!("Delete {n} unused cover{}", if n == 1 { "" } else { "s" });
            app.change_cover_files(deletes, Some(summary));
        }
        Message::CloseUnused => app.settings_page.covers.unused = None,
    }
    Task::none()
}

/// Esc: closes the open dialog. False when none was.
pub fn dismiss(state: &mut State) -> bool {
    state.found.take().is_some() || state.unused.take().is_some()
}

/// A row of the group: its name and a line under it, and a button.
fn group_row<'a>(
    name: &'a str,
    detail: String,
    action: Option<Element<'a, AppMessage>>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let raised = tokens.raised;
    let mut line = Row::new().spacing(12).align_y(Center).push(
        column![
            text(name).size(14).font(font::SEMIBOLD),
            text(detail).size(12.5).line_height(1.45).color(tokens.sub),
        ]
        .spacing(1)
        .width(Fill),
    );
    if let Some(action) = action {
        line = line.push(action);
    }
    container(line)
        .padding([12, 14])
        .style(move |_| container::background(raised).border(Border::default().rounded(10)))
        .into()
}

/// The three rows under Library source.
pub fn group<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let page = &app.settings_page.covers;
    let library = app.repo.library();
    let using = covers::entries_using(library).count();
    let check_detail = if page.checking {
        "Checking…".to_owned()
    } else if using == 0 {
        "No title uses a cover of your own yet.".to_owned()
    } else if page.none_found {
        format!(
            "{}. No online cover was found for {}.",
            titles_use(using),
            if using == 1 { "it" } else { "them" }
        )
    } else {
        format!(
            "{}. Looks for an online cover for {}.",
            titles_use(using),
            if using == 1 { "it" } else { "each of them" }
        )
    };
    let check = ui::sized_button(
        "Check now",
        ButtonKind::Outline,
        32.0,
        *tokens,
        (using > 0 && !page.checking).then(|| msg(Message::Check)),
    );
    let unused = app.covers.unused(library);
    let unused_detail = match unused.len() {
        0 => "No file is unused.".to_owned(),
        n => format!(
            "{n} file{} that no title uses or matches · {}",
            if n == 1 { "" } else { "s" },
            covers::size_label(unused.iter().map(|file| file.size).sum())
        ),
    };
    let review = ui::sized_button(
        "Review",
        ButtonKind::Outline,
        32.0,
        *tokens,
        (!unused.is_empty()).then(|| msg(Message::Review)),
    );
    column![
        group_row("Check for online covers", check_detail, Some(check), tokens),
        group_row("Unused covers", unused_detail, Some(review), tokens),
        group_row(
            "Uploads",
            "Scaled to 1000 px on the long side and saved as JPEG; files you commit yourself are \
             left as they are."
                .to_owned(),
            None,
            tokens,
        ),
    ]
    .spacing(8)
    .into()
}

/// "12 titles use your own covers", "1 title uses your own cover".
fn titles_use(n: usize) -> String {
    if n == 1 {
        "1 title uses your own cover".to_owned()
    } else {
        format!("{n} titles use your own covers")
    }
}

/// The open dialog and what closes it, if one is open.
pub fn dialog<'a>(app: &'a App, tokens: &Tokens) -> Option<(Element<'a, AppMessage>, AppMessage)> {
    let page = &app.settings_page.covers;
    if let Some(found) = &page.found {
        return Some((found_dialog(app, found, tokens), msg(Message::CloseFound)));
    }
    page.unused
        .as_ref()
        .map(|files| (unused_dialog(app, files, tokens), msg(Message::CloseUnused)))
}

/// Three joined choices, at a fixed width.
fn choices<'a>(
    picked: Option<OnlineChoice>,
    to: impl Fn(OnlineChoice) -> Message,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    container(ui::segmented(
        OnlineChoice::ALL.map(|choice| (choice.short(), picked == Some(choice), msg(to(choice)))),
        30.0,
        tokens,
    ))
    .width(410)
    .into()
}

fn found_dialog<'a>(app: &'a App, found: &'a [Found], tokens: &Tokens) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let checked = app.settings_page.covers.checked;
    let n = found.len();
    let header = row![
        column![
            text(format!(
                "{n} online cover{} found",
                if n == 1 { "" } else { "s" }
            ))
            .size(20)
            .font(font::EXTRABOLD),
            text(format!(
                "Checked the {checked} title{} that use{} your own cover{}. Pick per title, then Apply.",
                if checked == 1 { "" } else { "s" },
                if checked == 1 { "s" } else { "" },
                if checked == 1 { "" } else { "s" },
            ))
            .size(13.5)
            .color(tokens.sub),
        ]
        .spacing(4)
        .width(Fill),
        text("Set all:").size(12.5).color(tokens.sub),
        choices(None, Message::SetAll, tokens),
    ]
    .spacing(12)
    .align_y(Center);

    let line = tokens.line;
    let rows = found.iter().enumerate().filter_map(|(index, found)| {
        let entry = library.entry(&found.id)?;
        let square = ui::cover::is_square(entry.media);
        let (w, h) = if square { (44.0, 44.0) } else { (36.0, 54.0) };
        let mine = entry.cover_from.clone().unwrap_or_default();
        let file = covers::repo_path(&mine)
            .map(covers::name_of)
            .unwrap_or_default();
        let mut sub = browse::kind_label(entry).to_owned();
        if square {
            if let Some(artist) = entry.creator.as_deref().filter(|c| !c.is_empty()) {
                sub = format!("{sub} · {artist}");
            }
        } else if let Some(year) = entry.year() {
            sub = format!("{sub} · {year}");
        }
        let words = column![
            text(entry.title.clone()).size(14.5).font(font::SEMIBOLD),
            row![
                text(format!("{sub} ·")).size(12.5).color(tokens.sub),
                ui::mono(file.to_owned(), 12.0, tokens.sub),
            ]
            .spacing(5)
            .align_y(Center),
        ]
        .spacing(1)
        .width(Fill);
        let item = row![
            ui::plain_cover(
                app.artwork.image(&mine),
                theme::cover_color(&mine),
                w,
                h,
                4.0
            ),
            text("→").size(14).color(tokens.sub),
            ui::plain_cover(
                app.artwork.image(&found.url),
                theme::cover_color(&entry.id),
                w,
                h,
                4.0
            ),
            words,
            choices(
                Some(found.choice),
                move |choice| Message::Choice(index, choice),
                tokens
            ),
        ]
        .spacing(12)
        .align_y(Center);
        Some(
            column![
                container(item).padding([10, 0]),
                container(space())
                    .width(Fill)
                    .height(1)
                    .style(move |_| container::background(line)),
            ]
            .into(),
        )
    });
    let list = Column::with_children(rows.collect::<Vec<Element<'a, AppMessage>>>());
    let max_height = (app.window.height - 260.0).max(200.0);

    let deletes = found
        .iter()
        .filter(|found| found.choice == OnlineChoice::UseAndDelete)
        .count();
    let apply = match deletes {
        0 => "Apply".to_owned(),
        1 => "Apply · deletes 1 file".to_owned(),
        n => format!("Apply · deletes {n} files"),
    };
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Later",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::CloseFound))
        ),
        ui::labeled_button(
            apply,
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::Apply))
        ),
    ]
    .spacing(10);
    let tokens = *tokens;
    container(
        column![
            header,
            container(ui::scroll(list)).max_height(max_height),
            footer
        ]
        .spacing(12),
    )
    .width(FOUND_WIDTH)
    .padding(24)
    .style(move |_| ui::panel_style(&tokens, 14.0))
    .into()
}

fn unused_dialog<'a>(
    app: &'a App,
    files: &'a [(String, bool)],
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let line = tokens.line;
    let rows = files.iter().enumerate().map(|(index, (path, delete))| {
        let url = covers::cover_from(path);
        let (folder, name) = covers::split(path);
        let square = folder == covers::folder(crate::model::MediaType::Music);
        let (w, h) = if square { (40.0, 40.0) } else { (34.0, 51.0) };
        let (label, _) = covers::label_and_extension(name);
        // A file the app named was uploaded for a title that is gone.
        let why = if covers::is_upload_label(label) {
            "Its title was deleted"
        } else {
            "Doesn't match any title"
        };
        let keep = container(ui::segmented(
            [
                ("Delete", *delete, msg(Message::Unused(index, true))),
                ("Keep", !*delete, msg(Message::Unused(index, false))),
            ],
            30.0,
            tokens,
        ))
        .width(150);
        let item = row![
            ui::plain_cover(app.artwork.image(&url), theme::cover_color(&url), w, h, 4.0),
            column![
                ui::mono(path.clone(), 12.5, tokens.text),
                text(why).size(12.5).color(tokens.sub),
            ]
            .spacing(2)
            .width(Fill),
            keep,
        ]
        .spacing(12)
        .align_y(Center);
        column![
            container(item).padding([10, 0]),
            container(space())
                .width(Fill)
                .height(1)
                .style(move |_| container::background(line)),
        ]
        .into()
    });
    let list = Column::with_children(rows.collect::<Vec<Element<'a, AppMessage>>>());
    let max_height = (app.window.height - 260.0).max(200.0);
    let n = files.iter().filter(|(_, delete)| *delete).count();
    let delete = match n {
        0 => "Delete".to_owned(),
        1 => "Delete 1 file".to_owned(),
        n => format!("Delete {n} files"),
    };
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::CloseUnused))
        ),
        // Neutral with red text: never a red fill.
        ui::labeled_button(
            delete,
            ButtonKind::NeutralDestructive,
            *tokens,
            (n > 0).then(|| msg(Message::DeleteUnused)),
        ),
    ]
    .spacing(10);
    let tokens = *tokens;
    container(
        column![
            text("Unused covers").size(20).font(font::EXTRABOLD),
            text("No title uses these as its cover, and none of their names match a title.")
                .size(13.5)
                .color(tokens.sub),
            container(ui::scroll(list)).max_height(max_height),
            footer,
        ]
        .spacing(12),
    )
    .width(UNUSED_WIDTH)
    .padding(24)
    .style(move |_| ui::panel_style(&tokens, 14.0))
    .into()
}

/// What the open dialog shows: each title's own cover and the online
/// one, or the unused files.
pub fn wanted(app: &App, wants: &mut Wants) {
    let page = &app.settings_page.covers;
    for found in page.found.iter().flatten() {
        wants.covers.push(found.id.clone());
        wants.images.push(found.url.clone());
    }
    for (path, _) in page.unused.iter().flatten() {
        wants.images.push(covers::cover_from(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::covers::Covers;
    use crate::repo::Operation;
    use crate::screens::Screen;

    const BABY: &str = "movies/million-dollar-baby-2004";
    const DRACULA: &str = "books/dracula-1897";
    const STEELBOOK: &str = "Movies/million-dollar-baby-2004.steelbook.jpg";
    const DELUXE: &str = "Music/dirt-alice-in-chains.deluxe.jpg";
    const DRACULA_OWN: &str = "Books/dracula-1897.first-edition.jpg";

    /// The demo's files and one for Dracula, which the fake's lookup has
    /// a cover for; it has none for Million Dollar Baby.
    fn app() -> App {
        let mut app = App::new(Screen::Settings);
        let mut files = covers::demo();
        files.push(covers::File {
            path: DRACULA_OWN.into(),
            size: 200_000,
            sha: "d".repeat(40),
        });
        app.covers = Covers::with_files(files);
        app
    }

    fn show_own(app: &mut App, id: &str, path: &str, keep: bool) {
        let entry = app.repo.library().entry(id).unwrap();
        let changed = Entry {
            cover_from: Some(covers::cover_from(path)),
            cover_keep_mine: keep.then_some(true),
            ..entry.clone()
        };
        app.apply(Operation::UpdateEntry(changed));
    }

    fn send(app: &mut App, message: Message) {
        let _ = update(app, message);
    }

    #[test]
    fn check_now_lists_the_hits_and_apply_follows_each_choice() {
        let mut app = app();
        show_own(&mut app, BABY, STEELBOOK, false);
        show_own(&mut app, DRACULA, DRACULA_OWN, false);
        send(&mut app, Message::Check);
        let page = &app.settings_page.covers;
        assert_eq!(page.checked, 2);
        let found = page
            .found
            .clone()
            .expect("the fake has a cover for Dracula");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, DRACULA);
        assert_eq!(found[0].choice, OnlineChoice::UseAndDelete);

        // Set all, then the row's own choice.
        send(&mut app, Message::SetAll(OnlineChoice::KeepMine));
        assert_eq!(
            app.settings_page.covers.found.as_ref().unwrap()[0].choice,
            OnlineChoice::KeepMine
        );
        send(&mut app, Message::Apply);
        assert!(app.settings_page.covers.found.is_none());
        let dracula = app.repo.library().entry(DRACULA).unwrap();
        assert!(dracula.cover_from.is_some());
        assert_eq!(dracula.cover_keep_mine, Some(true));

        // Keep mine is never asked about again.
        send(&mut app, Message::Check);
        assert_eq!(app.settings_page.covers.checked, 1);
        assert!(app.settings_page.covers.found.is_none());
        assert!(app.settings_page.covers.none_found);

        // Use online and delete mine: the file goes with the pick.
        let mut app = self::app();
        show_own(&mut app, DRACULA, DRACULA_OWN, false);
        send(&mut app, Message::Check);
        send(&mut app, Message::Apply);
        assert!(
            app.repo
                .library()
                .entry(DRACULA)
                .unwrap()
                .cover_from
                .is_none()
        );
        assert!(app.covers.get(DRACULA_OWN).is_none(), "deleted with it");
    }

    #[test]
    fn unused_covers_are_reviewed_and_deleted_in_one_commit() {
        let mut app = app();
        show_own(&mut app, BABY, STEELBOOK, true);
        send(&mut app, Message::Review);
        let files = app.settings_page.covers.unused.clone().unwrap();
        // The demo's seven files named after no title; the two named
        // after Million Dollar Baby and Dirt are not unused.
        assert_eq!(files.len(), 7);
        assert!(files.iter().all(|(_, delete)| *delete));
        assert!(
            !files
                .iter()
                .any(|(path, _)| [STEELBOOK, DELUXE, DRACULA_OWN].contains(&path.as_str()))
        );
        let kept = files[0].0.clone();
        send(&mut app, Message::Unused(0, false));
        send(&mut app, Message::DeleteUnused);
        assert!(app.settings_page.covers.unused.is_none());
        let left = app.covers.unused(app.repo.library());
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].path, kept);
        assert!(app.covers.get(STEELBOOK).is_some());
    }

    #[test]
    fn the_group_and_its_dialogs_draw() {
        let mut app = app();
        show_own(&mut app, DRACULA, DRACULA_OWN, false);
        let shows = |app: &App, words: &[&str]| {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            for words in words {
                assert!(ui.find(*words).is_ok(), "{words}");
            }
        };
        send(&mut app, Message::Check);
        shows(
            &app,
            &[
                "1 online cover found",
                "Set all:",
                "Dracula",
                "Apply · deletes 1 file",
            ],
        );
        send(&mut app, Message::CloseFound);
        send(&mut app, Message::Review);
        shows(
            &app,
            &[
                "Unused covers",
                "Movies/old-poster.png",
                "Doesn't match any title",
                "Delete 7 files",
            ],
        );
        assert!(dismiss(&mut app.settings_page.covers));
        assert!(app.settings_page.covers.unused.is_none());
    }
}
