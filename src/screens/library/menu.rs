//! The right-click menu on a title, a series or an artist (canvas
//! revision of 2026-11-17), in every view of the lists and on Home. It
//! opens at the pointer without selecting or opening what is under it,
//! and every item runs an action the app already has: the menu is one
//! more way to reach them. The Menu key or Shift+F10 opens it for the
//! selected title.

use iced::{Element, Point, Task};

use super::{Message, MusicTab, OpenSeries};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Entry, EntryStatus, MediaType, Series};
use crate::music;
use crate::screens::{entry, plans};
use crate::ui::context::{self, Layer, RowLook};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// What a menu is open on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Entry(String),
    Series { media: MediaType, name: String },
    Artist(String),
}

/// The rows that open a submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sub {
    Status,
    Plan,
}

/// The open menu: what it is for, where the pointer was (in the window)
/// and the submenu open beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub target: Target,
    pub at: Point,
    pub sub: Option<Sub>,
}

/// Where the Menu key opens the menu when the selected title's row is
/// not on screen (Home, or a list scrolled away from it).
pub const KEY_POINT: Point = Point::new(320.0, 220.0);
/// How far into the selected title's row the Menu key opens the menu.
pub const KEY_INSET: f32 = 28.0;

/// An item of the menu, for the target it is open on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// "Open details", "Open series", an artist's "Show albums".
    Open,
    Edit,
    ViewCover,
    ChangeCover,
    Status(EntryStatus),
    /// "Rate…": the details with the rating picker open.
    Rate,
    /// A plan of the Add to plan submenu, by its id.
    TogglePlan(String),
    NewPlan,
    GoToArtist,
    GoToAlbum,
    /// An artist's "Show songs".
    ShowSongs,
    /// "Copy title" or "Copy name".
    Copy,
    Delete,
}

/// A row of the menu, or the line between two groups.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Separator,
    Row { look: RowLook, does: Does },
}

/// What a row does: an action, or a submenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    Run(Action),
    Opens(Sub),
}

/// A row of a submenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubItem {
    Separator,
    Row {
        label: String,
        on: bool,
        action: Action,
    },
}

fn row(label: &str, hint: impl Into<String>, action: Action) -> Item {
    Item::Row {
        look: RowLook {
            label: label.to_owned(),
            hint: hint.into(),
            ..RowLook::default()
        },
        does: Does::Run(action),
    }
}

fn opens(label: &str, hint: impl Into<String>, sub: Sub) -> Item {
    Item::Row {
        look: RowLook {
            label: label.to_owned(),
            hint: hint.into(),
            arrow: true,
            ..RowLook::default()
        },
        does: Does::Opens(sub),
    }
}

/// Whether the entry's status is its own to set: a show with several
/// seasons has it from them.
fn sets_status(entry: &Entry) -> bool {
    entry.season_list().len() <= 1
}

/// The menu's header and items for `target`, or `None` when what it was
/// opened on is gone from the library. The shortcuts that already do the
/// same show at the right of their item, and Status, Rate and Add to plan
/// show what is set now.
pub fn items(app: &App, target: &Target) -> Option<(String, Vec<Item>)> {
    let library = app.repo.library();
    match target {
        Target::Entry(id) => {
            let entry = library.entry(id)?;
            let mut items = vec![
                row("Open details", "Click", Action::Open),
                row("Edit…", "Ctrl+Shift+click", Action::Edit),
                Item::Separator,
                row("View cover", "Ctrl+click", Action::ViewCover),
                row("Change cover…", "", Action::ChangeCover),
                Item::Separator,
            ];
            if sets_status(entry) {
                let status = browse::status_label(Some(entry.media), entry.status);
                items.push(opens("Status", status, Sub::Status));
            }
            let rating = entry
                .rating
                .map_or("Not rated".to_owned(), |r| format!("{r}/10"));
            items.push(row("Rate…", rating, Action::Rate));
            let in_plans: Vec<_> = library
                .plans
                .iter()
                .filter(|plan| plan.item_for(id).is_some())
                .collect();
            let plans = match in_plans.as_slice() {
                [] => String::new(),
                [only] => only.title(),
                several => format!("{} plans", several.len()),
            };
            items.push(opens("Add to plan", plans, Sub::Plan));
            if entry.media == MediaType::Music {
                let artist = music::artist_of(entry);
                let album = music::is_song(entry)
                    .then(|| music::album_of_song(&library.entries, entry))
                    .flatten();
                if artist != music::VARIOUS_ARTISTS || album.is_some() {
                    items.push(Item::Separator);
                }
                if artist != music::VARIOUS_ARTISTS {
                    items.push(row("Go to artist", artist, Action::GoToArtist));
                }
                if let Some(album) = album {
                    items.push(row("Go to album", album.title.as_str(), Action::GoToAlbum));
                }
            }
            items.push(Item::Separator);
            items.push(row("Copy title", "", Action::Copy));
            items.push(Item::Separator);
            items.push(Item::Row {
                look: RowLook {
                    label: "Delete…".into(),
                    danger: true,
                    ..RowLook::default()
                },
                does: Does::Run(Action::Delete),
            });
            Some((entry.title.clone(), items))
        }
        Target::Series { media, name } => {
            let parts = crate::series::entries_in(library, *media, name);
            if parts.is_empty() {
                return None;
            }
            let items = vec![
                row("Open series", "Click", Action::Open),
                Item::Separator,
                row("View cover", "Ctrl+click", Action::ViewCover),
                Item::Separator,
                row("Copy name", "", Action::Copy),
            ];
            Some((name.clone(), items))
        }
        Target::Artist(name) => {
            let (albums, songs) = saved_by(app, name);
            if albums + songs == 0 {
                return None;
            }
            let count = |n: usize| if n == 0 { String::new() } else { n.to_string() };
            let mut items = vec![row("Show albums", count(albums), Action::Open)];
            if songs > 0 {
                items.push(row("Show songs", count(songs), Action::ShowSongs));
            }
            items.extend([
                Item::Separator,
                row("View cover", "Ctrl+click", Action::ViewCover),
                row("Change cover…", "", Action::ChangeCover),
                Item::Separator,
                row("Copy name", "", Action::Copy),
            ]);
            Some((name.clone(), items))
        }
    }
}

/// How many albums and songs of the artist are saved.
fn saved_by(app: &App, name: &str) -> (usize, usize) {
    let mine = app
        .repo
        .library()
        .entries_of(MediaType::Music)
        .filter(|e| music::artist_of(e).eq_ignore_ascii_case(name));
    let (songs, albums): (Vec<_>, Vec<_>) = mine.partition(|e| music::is_song(e));
    (albums.len(), songs.len())
}

/// The rows of a submenu: the type's statuses with the current one
/// checked, or a row per plan with a check on those the title is in,
/// then "New plan…".
pub fn sub_items(app: &App, target: &Target, sub: Sub) -> Vec<SubItem> {
    let library = app.repo.library();
    let Target::Entry(id) = target else {
        return Vec::new();
    };
    let Some(entry) = library.entry(id) else {
        return Vec::new();
    };
    match sub {
        Sub::Status => EntryStatus::options_for(entry.media)
            .iter()
            .map(|&status| SubItem::Row {
                label: browse::status_label(Some(entry.media), status).to_owned(),
                on: entry.status == status,
                action: Action::Status(status),
            })
            .collect(),
        Sub::Plan => {
            let mut rows: Vec<SubItem> = library
                .plans
                .iter()
                .map(|plan| SubItem::Row {
                    label: plan.title(),
                    on: plan.item_for(id).is_some(),
                    action: Action::TogglePlan(plan.id.clone()),
                })
                .collect();
            if !rows.is_empty() {
                rows.push(SubItem::Separator);
            }
            rows.push(SubItem::Row {
                label: "New plan…".into(),
                on: false,
                action: Action::NewPlan,
            });
            rows
        }
    }
}

/// `content` with the right-click menu for `target`: the press opens it
/// where the layer said the pointer was.
pub fn area<'a>(
    target: Target,
    content: impl Into<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    iced::widget::mouse_area(content)
        .on_right_press(msg(Message::OpenMenu(target)))
        .into()
}

/// The layer over the library that draws the open menu, and reports
/// where the right button goes down while none is open.
pub fn layer<'a>(app: &'a App) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let closed = || {
        Layer::new(None, Point::ORIGIN)
            .on_point(|point| msg(Message::Pointer(point)))
            .into()
    };
    let Some(menu) = &app.library.menu else {
        return closed();
    };
    let Some((head, items)) = items(app, &menu.target) else {
        return closed();
    };
    // How far down the menu the open submenu's row starts.
    let mut top = context::PADDING + context::HEADER;
    let mut sub_row = None;
    let rows: Vec<Element<'a, AppMessage>> = items
        .into_iter()
        .map(|item| match item {
            Item::Separator => {
                top += context::SEPARATOR;
                context::separator(&tokens)
            }
            Item::Row { mut look, does } => {
                let message = match does {
                    Does::Run(action) => Message::Menu(action),
                    Does::Opens(sub) => {
                        if menu.sub == Some(sub) {
                            look.active = true;
                            sub_row = Some(top);
                        }
                        Message::MenuSub(sub)
                    }
                };
                top += context::ROW;
                context::menu_row(look, &tokens, msg(message))
            }
        })
        .collect();
    let panel = context::menu(Some(head), rows, context::WIDTH, &tokens);
    let mut layer = Layer::new(Some(panel), menu.at).on_dismiss(msg(Message::CloseMenu));
    if let (Some(sub), Some(row)) = (menu.sub, sub_row) {
        let rows = sub_items(app, &menu.target, sub)
            .into_iter()
            .map(|item| match item {
                SubItem::Separator => context::separator(&tokens),
                SubItem::Row { label, on, action } => {
                    context::check_row(label, on, &tokens, msg(Message::Menu(action)))
                }
            });
        let panel = context::menu(None, rows, context::SUB_WIDTH, &tokens);
        layer = layer.sub(panel, row);
    }
    layer.into()
}

/// Runs an item of the open menu, which closes: each one sends what a
/// click or a button elsewhere already sends.
pub fn run(app: &mut App, action: Action) -> Task<AppMessage> {
    let Some(menu) = app.library.menu.take() else {
        return Task::none();
    };
    match menu.target {
        Target::Entry(id) => run_on_entry(app, id, action),
        Target::Series { media, name } => run_on_series(app, media, name, action),
        Target::Artist(name) => run_on_artist(app, name, action),
    }
}

/// Selects `id` unless it is selected: a click on the selected title
/// would close its details.
fn select(app: &mut App, id: &str) -> Task<AppMessage> {
    if app.library.selected.as_deref() == Some(id) {
        return Task::none();
    }
    app.act(msg(Message::Select(id.to_owned())))
}

/// Shows Music's own list, with the search emptied so the artist or the
/// album gone to is not narrowed away.
fn show_music(app: &mut App) -> Task<AppMessage> {
    let mut steps = Vec::new();
    if app.library.single() != Some(MediaType::Music) || app.library.is_overview() {
        steps.push(app.act(msg(Message::ShowType(Some(MediaType::Music)))));
    }
    if !app.library.query.is_empty() {
        steps.push(app.act(msg(Message::Search(String::new()))));
    }
    Task::batch(steps)
}

fn run_on_entry(app: &mut App, id: String, action: Action) -> Task<AppMessage> {
    let Some(entry) = app.repo.library().entry(&id) else {
        return Task::none();
    };
    let message = |message: entry::Message| AppMessage::Entry(message);
    match action {
        Action::Open => select(app, &id),
        Action::Edit => app.act(message(entry::Message::Edit(id))),
        Action::ViewCover => app.act(message(entry::Message::ViewCover(id))),
        Action::ChangeCover => {
            let view = app.act(message(entry::Message::ViewCover(id)));
            let change = app.act(message(entry::Message::ChangeCover));
            Task::batch([view, change])
        }
        // With one season the status is that season's, as in details.
        Action::Status(status) if sets_status(entry) => match entry.season_list() {
            [only] => {
                let season = only.season;
                app.act(message(entry::Message::SetSeasonStatus {
                    id,
                    season,
                    status: Some(status),
                }))
            }
            _ => app.act(message(entry::Message::SetStatus(id, status))),
        },
        Action::Rate => {
            let selected = select(app, &id);
            let open = if app.library.details_rating_open {
                Task::none()
            } else {
                app.act(message(entry::Message::ToggleDetailsRating))
            };
            Task::batch([selected, open])
        }
        Action::TogglePlan(plan) => {
            entry::toggle_plan(app, id, plan);
            Task::none()
        }
        Action::NewPlan => app.act(AppMessage::Plans(plans::Message::NewPlan)),
        Action::GoToArtist => {
            let artist = music::artist_of(entry).to_owned();
            let music = show_music(app);
            let open = app.act(msg(Message::OpenArtist(artist)));
            Task::batch([music, open])
        }
        Action::GoToAlbum => {
            let entries = &app.repo.library().entries;
            let Some(album) = music::album_of_song(entries, entry).map(|album| album.id.clone())
            else {
                return Task::none();
            };
            let mut steps = vec![show_music(app)];
            if app.library.artist.is_some() {
                steps.push(app.act(msg(Message::CloseArtist)));
            }
            if app.library.music_tab != MusicTab::Albums {
                steps.push(app.act(msg(Message::MusicTab(MusicTab::Albums))));
            }
            steps.push(select(app, &album));
            Task::batch(steps).chain(super::reveal(&app.library, &album))
        }
        Action::Copy => iced::clipboard::write(entry.title.clone()),
        Action::Delete => app.act(message(entry::Message::Delete(id))),
        Action::Status(_) | Action::ShowSongs => Task::none(),
    }
}

fn run_on_series(
    app: &mut App,
    media: MediaType,
    name: String,
    action: Action,
) -> Task<AppMessage> {
    match action {
        Action::Open => {
            // A click on the row of the open series would close it.
            let state = &app.library;
            let shows = |m: MediaType, n: &str| m == media && Series::same(n, &name);
            let open = state.open_series().is_some_and(|s| shows(s.media, &s.name))
                || state.back_series().is_some_and(|b| shows(b.media, &b.name));
            if open {
                return Task::none();
            }
            app.act(msg(Message::OpenSeries(OpenSeries {
                media,
                name,
                highlight: None,
            })))
        }
        Action::ViewCover => {
            let parts = crate::series::entries_in(app.repo.library(), media, &name);
            match crate::series::cover_entry(&parts).map(|cover| cover.id.clone()) {
                Some(id) => app.act(AppMessage::Entry(entry::Message::ViewCover(id))),
                None => Task::none(),
            }
        }
        Action::Copy => iced::clipboard::write(name),
        _ => Task::none(),
    }
}

fn run_on_artist(app: &mut App, name: String, action: Action) -> Task<AppMessage> {
    let artist =
        |message: entry::artist::Message| AppMessage::Entry(entry::Message::Artist(message));
    match action {
        Action::Open => app.act(msg(Message::OpenArtist(name))),
        // The artist's page lists the albums and then the songs: it opens
        // with the songs in view.
        Action::ShowSongs => {
            let open = app.act(msg(Message::OpenArtist(name)));
            open.chain(super::reveal_songs())
        }
        Action::ViewCover => app.act(artist(entry::artist::Message::View(name))),
        Action::ChangeCover => {
            let view = app.act(artist(entry::artist::Message::View(name)));
            let change = app.act(artist(entry::artist::Message::Change));
            Task::batch([view, change])
        }
        Action::Copy => iced::clipboard::write(name),
        _ => Task::none(),
    }
}

/// The Menu key or Shift+F10: the menu for the selected title, at its
/// row or card when the list shows it.
pub fn open_for_selected(app: &App) -> Task<AppMessage> {
    let Some(id) = app.library.selected.clone() else {
        return Task::none();
    };
    let locate = super::reveal::Locate::new(super::LIST_ID, &super::entries::row_id(&id));
    iced::advanced::widget::operate(locate).map(move |at: Option<Point>| {
        let at = at.map(|corner| Point::new(corner.x + KEY_INSET, corner.y + KEY_INSET));
        msg(Message::OpenMenuAt(Target::Entry(id.clone()), at))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;

    const MOVIE: &str = "movies/the-mummy-1999";
    const SONG: &str = "music/bad-moon-rising-creedence-clearwater-revival";

    fn app() -> App {
        App::new(Screen::Library)
    }

    fn send(app: &mut App, message: Message) {
        let _ = app.update(msg(message));
    }

    fn labels(app: &App, target: &Target) -> Vec<String> {
        let (_, items) = items(app, target).expect("the target is in the library");
        items
            .into_iter()
            .map(|item| match item {
                Item::Separator => "-".to_owned(),
                Item::Row { look, .. } => look.label,
            })
            .collect()
    }

    fn hint(app: &App, target: &Target, label: &str) -> String {
        let (_, items) = items(app, target).unwrap();
        items
            .into_iter()
            .find_map(|item| match item {
                Item::Row { look, .. } if look.label == label => Some(look.hint),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {label}"))
    }

    #[test]
    fn a_title_lists_its_actions_with_their_shortcuts_and_values() {
        let app = app();
        let target = Target::Entry(MOVIE.into());
        assert_eq!(
            labels(&app, &target),
            [
                "Open details",
                "Edit…",
                "-",
                "View cover",
                "Change cover…",
                "-",
                "Status",
                "Rate…",
                "Add to plan",
                "-",
                "Copy title",
                "-",
                "Delete…"
            ]
        );
        assert_eq!(hint(&app, &target, "Open details"), "Click");
        assert_eq!(hint(&app, &target, "Edit…"), "Ctrl+Shift+click");
        assert_eq!(hint(&app, &target, "View cover"), "Ctrl+click");
        let entry = app.repo.library().entry(MOVIE).unwrap();
        let status = browse::status_label(Some(MediaType::Movies), entry.status);
        assert_eq!(hint(&app, &target, "Status"), status);
        let rating = entry
            .rating
            .map_or("Not rated".into(), |r| format!("{r}/10"));
        assert_eq!(hint(&app, &target, "Rate…"), rating);
    }

    #[test]
    fn a_show_with_several_seasons_has_no_status_item() {
        let app = app();
        let several = labels(&app, &Target::Entry("shows/severance-2022".into()));
        assert!(!several.contains(&"Status".to_owned()));
        let one = labels(&app, &Target::Entry("shows/the-bear-2022".into()));
        assert!(one.contains(&"Status".to_owned()));
    }

    #[test]
    fn music_adds_its_artist_and_a_song_its_album() {
        let mut app = app();
        let song = labels(&app, &Target::Entry(SONG.into()));
        assert!(song.contains(&"Go to artist".to_owned()));
        assert!(!song.contains(&"Go to album".to_owned()), "not saved");
        // With the song's album in the library, the song leads to it.
        let mut entry = app.repo.library().entry(SONG).unwrap().clone();
        entry.album = Some("Dirt".into());
        entry.creator = Some("Alice in Chains".into());
        app.apply(crate::repo::Operation::UpdateEntry(entry));
        let target = Target::Entry(SONG.into());
        let song = labels(&app, &target);
        let at = song.iter().position(|l| l == "Go to artist").unwrap();
        assert_eq!(
            &song[at - 1..at + 3],
            ["-", "Go to artist", "Go to album", "-"]
        );
        assert_eq!(hint(&app, &target, "Go to album"), "Dirt");
        // Various Artists is nobody's page.
        let various = "music/now-thats-what-i-call-christmas-various-artists";
        let album = labels(&app, &Target::Entry(various.into()));
        assert!(!album.contains(&"Go to artist".to_owned()));
        assert!(
            !album.join(" ").contains("- -"),
            "no empty group: {album:?}"
        );
    }

    #[test]
    fn a_series_and_an_artist_have_their_own_items() {
        let app = app();
        let series = Target::Series {
            media: MediaType::Books,
            name: "Berserk".into(),
        };
        assert_eq!(
            labels(&app, &series),
            ["Open series", "-", "View cover", "-", "Copy name"]
        );
        // Songs alone, or albums alone: Show songs only with songs.
        let albums = labels(&app, &Target::Artist("Metallica".into()));
        assert_eq!(
            albums,
            [
                "Show albums",
                "-",
                "View cover",
                "Change cover…",
                "-",
                "Copy name"
            ]
        );
        let songs = labels(&app, &Target::Artist("The Pogues".into()));
        assert_eq!(&songs[..2], ["Show albums", "Show songs"]);
        assert_eq!(
            hint(&app, &Target::Artist("The Pogues".into()), "Show songs"),
            "1"
        );
    }

    #[test]
    fn the_menu_opens_at_the_pointer_without_selecting_and_esc_closes_it() {
        let mut app = app();
        send(&mut app, Message::ShowType(Some(MediaType::Movies)));
        send(&mut app, Message::Toggle(super::super::Popup::Sort));
        send(&mut app, Message::Pointer(Point::new(548.0, 236.0)));
        send(&mut app, Message::OpenMenu(Target::Entry(MOVIE.into())));
        let menu = app.library.menu.clone().expect("the menu is open");
        assert_eq!(menu.at, Point::new(548.0, 236.0));
        assert_eq!(app.library.selected, None, "nothing is selected");
        assert_eq!(app.library.popup, None, "the Sort menu closed");
        send(&mut app, Message::MenuSub(Sub::Status));
        assert_eq!(app.library.menu.as_ref().unwrap().sub, Some(Sub::Status));
        send(&mut app, Message::MenuSub(Sub::Plan));
        assert_eq!(app.library.menu.as_ref().unwrap().sub, Some(Sub::Plan));
        send(&mut app, Message::MenuSub(Sub::Plan));
        assert_eq!(app.library.menu.as_ref().unwrap().sub, None);
        let _ = app.update(AppMessage::Key(crate::app::KeyPress::Escape));
        assert_eq!(app.library.menu, None);
        // Not under a dialog.
        let _ = app.update(AppMessage::Entry(entry::Message::Delete(MOVIE.into())));
        send(&mut app, Message::OpenMenu(Target::Entry(MOVIE.into())));
        assert_eq!(app.library.menu, None);
    }

    fn open(app: &mut App, target: Target) {
        send(app, Message::OpenMenu(target));
        assert!(app.library.menu.is_some());
    }

    #[test]
    fn the_items_run_what_a_click_or_a_button_already_does() {
        let mut app = app();
        send(&mut app, Message::ShowType(Some(MediaType::Movies)));
        let movie = || Target::Entry(MOVIE.into());

        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::Open));
        assert_eq!(app.library.selected.as_deref(), Some(MOVIE));
        assert_eq!(app.library.menu, None, "an item closes the menu");
        // Open details on the open title keeps it open.
        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::Open));
        assert_eq!(app.library.selected.as_deref(), Some(MOVIE));

        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::Rate));
        assert!(app.library.details_rating_open);

        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::Status(EntryStatus::Dnf)));
        let status = |app: &App, id: &str| app.repo.library().entry(id).unwrap().status;
        assert_eq!(status(&app, MOVIE), EntryStatus::Dnf);

        let in_plan = |app: &App| {
            let plan = app.repo.library().plan("christmas-2026").unwrap();
            plan.item_for(MOVIE).is_some()
        };
        let was = in_plan(&app);
        open(&mut app, movie());
        send(
            &mut app,
            Message::Menu(Action::TogglePlan("christmas-2026".into())),
        );
        assert_eq!(in_plan(&app), !was);

        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::Edit));
        assert!(matches!(app.dialog, Some(entry::Dialog::Form(_))));
        app.dialog = None;

        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::ChangeCover));
        assert!(matches!(&app.dialog, Some(entry::Dialog::Cover(view)) if view.changing));
        app.dialog = None;

        open(&mut app, movie());
        send(&mut app, Message::Menu(Action::Delete));
        assert!(matches!(app.dialog, Some(entry::Dialog::Delete { .. })));
        assert!(app.repo.library().entry(MOVIE).is_some(), "it asks first");
    }

    #[test]
    fn one_season_takes_the_status_and_several_seasons_refuse_it() {
        let mut app = app();
        let bear = "shows/the-bear-2022";
        open(&mut app, Target::Entry(bear.into()));
        send(&mut app, Message::Menu(Action::Status(EntryStatus::Dnf)));
        let entry = app.repo.library().entry(bear).unwrap();
        assert_eq!(entry.season_list()[0].status, Some(EntryStatus::Dnf));

        let severance = "shows/severance-2022";
        let was = app.repo.library().entry(severance).unwrap().clone();
        open(&mut app, Target::Entry(severance.into()));
        send(&mut app, Message::Menu(Action::Status(EntryStatus::Dnf)));
        assert_eq!(app.repo.library().entry(severance), Some(&was));
    }

    #[test]
    fn go_to_artist_opens_their_page_from_anywhere_with_the_search_emptied() {
        let mut app = app();
        send(&mut app, Message::Search("moon".into()));
        open(&mut app, Target::Entry(SONG.into()));
        send(&mut app, Message::Menu(Action::GoToArtist));
        assert_eq!(app.library.single(), Some(MediaType::Music));
        assert_eq!(
            app.library.artist.as_deref(),
            Some("Creedence Clearwater Revival")
        );
        assert_eq!(app.library.query, "");
    }

    #[test]
    fn a_series_and_an_artist_open_as_a_click_opens_them() {
        let mut app = app();
        let series = Target::Series {
            media: MediaType::Books,
            name: "Berserk".into(),
        };
        open(&mut app, series.clone());
        send(&mut app, Message::Menu(Action::Open));
        let shown = |app: &App| app.library.open_series().map(|s| s.name.clone());
        assert_eq!(shown(&app).as_deref(), Some("Berserk"));
        // Again on the open series: it stays open.
        open(&mut app, series.clone());
        send(&mut app, Message::Menu(Action::Open));
        assert_eq!(shown(&app).as_deref(), Some("Berserk"));
        open(&mut app, series);
        send(&mut app, Message::Menu(Action::ViewCover));
        assert!(matches!(app.dialog, Some(entry::Dialog::Cover(_))));
        app.dialog = None;

        send(&mut app, Message::ShowType(Some(MediaType::Music)));
        open(&mut app, Target::Artist("The Pogues".into()));
        send(&mut app, Message::Menu(Action::ShowSongs));
        assert_eq!(app.library.artist.as_deref(), Some("The Pogues"));
        send(&mut app, Message::CloseArtist);
        open(&mut app, Target::Artist("The Pogues".into()));
        send(&mut app, Message::Menu(Action::ViewCover));
        assert!(matches!(app.dialog, Some(entry::Dialog::Artist(_))));
    }

    /// One round of the window: the view, `button` pressed and let go at
    /// `at`, and the messages that came of it.
    fn press(app: &App, at: Point, button: iced::mouse::Button) -> Vec<AppMessage> {
        use iced::mouse::Event::{ButtonPressed, ButtonReleased};
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        ui.point_at(at);
        let _ = ui.simulate([
            iced::Event::Mouse(ButtonPressed(button)),
            iced::Event::Mouse(ButtonReleased(button)),
        ]);
        // The list measures itself on any event; that is not the press's.
        let measured = |message: &AppMessage| {
            matches!(message, AppMessage::Library(Message::GroupsMeasured(_)))
        };
        ui.into_messages().filter(|m| !measured(m)).collect()
    }

    /// The middle of the first text that reads `words`.
    fn middle_of(app: &App, words: &str) -> Point {
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        let found = ui.find(words).unwrap_or_else(|_| panic!("no {words}"));
        found.visible_bounds().expect("it is on screen").center()
    }

    #[test]
    fn a_right_click_opens_the_menu_at_the_pointer_and_a_click_outside_only_closes_it() {
        let mut app = app();
        send(&mut app, Message::ShowType(Some(MediaType::Movies)));
        send(&mut app, Message::SetView(super::super::View::Text));
        send(&mut app, Message::TogglePanel);
        let at = middle_of(&app, "The Mummy");
        let messages = press(&app, at, iced::mouse::Button::Right);
        assert!(
            matches!(
                messages.as_slice(),
                [
                    AppMessage::Library(Message::Pointer(point)),
                    AppMessage::Library(Message::OpenMenu(Target::Entry(id)))
                ] if *point == at && id == MOVIE
            ),
            "{messages:?}"
        );
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.library.menu.as_ref().map(|menu| menu.at), Some(at));
        assert_eq!(app.library.selected, None);
        // The menu shows, with the shortcuts at the right of their items.
        for words in ["Open details", "Ctrl+Shift+click", "Add to plan", "Delete…"] {
            let _ = middle_of(&app, words);
        }
        // A click on another row while it is open closes it and selects
        // nothing; so does a right-click.
        let other = Point::new(at.x - 40.0, at.y + 3.0 * 43.0);
        for button in [iced::mouse::Button::Left, iced::mouse::Button::Right] {
            let messages = press(&app, other, button);
            assert!(
                matches!(
                    messages.as_slice(),
                    [AppMessage::Library(Message::CloseMenu)]
                ),
                "{messages:?}"
            );
        }
        // An item sends its action.
        let delete = middle_of(&app, "Delete…");
        let messages = press(&app, delete, iced::mouse::Button::Left);
        assert!(
            matches!(
                messages.as_slice(),
                [AppMessage::Library(Message::Menu(Action::Delete))]
            ),
            "{messages:?}"
        );
        // And a submenu opens beside its row.
        send(&mut app, Message::MenuSub(Sub::Plan));
        let plan = middle_of(&app, "New plan…");
        assert!(plan.x > at.x + context::WIDTH - 8.0, "at the menu's right");
    }

    #[test]
    fn the_menu_key_opens_the_selected_titles_menu() {
        let mut app = app();
        send(&mut app, Message::ShowType(Some(MediaType::Movies)));
        let key = iced::keyboard::Key::Named(iced::keyboard::key::Named::ContextMenu);
        let press = |app: &mut App, key: &iced::keyboard::Key, modifiers| {
            let _ = app.update(AppMessage::KeyPressed(key.clone(), modifiers, None));
        };
        press(&mut app, &key, iced::keyboard::Modifiers::empty());
        assert_eq!(app.library.menu, None, "nothing is selected");
        send(&mut app, Message::Select(MOVIE.into()));
        // The row's place comes back from the list; here it is given.
        send(
            &mut app,
            Message::OpenMenuAt(Target::Entry(MOVIE.into()), None),
        );
        assert_eq!(app.library.menu.as_ref().unwrap().at, KEY_POINT);
        // The arrow keys leave the selection alone under the menu.
        let _ = app.update(AppMessage::Key(crate::app::KeyPress::Down));
        assert_eq!(app.library.selected.as_deref(), Some(MOVIE));
    }
}
