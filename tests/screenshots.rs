//! Renders every screen headless, in Graphite, Light and Glass at the
//! default window size, to `screenshots/` (git-ignored), to compare with
//! the mockup. Glass comes out see-through, on nothing; the desktop
//! behind it is KWin's. Skipped by a plain `cargo test`; run it with:
//!
//! ```fish
//! cargo test --test screenshots -- --ignored
//! ```
//!
//! `LIBRI_SHOT=settings` renders only the captures whose name contains
//! it, `LIBRI_SHOT_THEME=charcoal,mocha` other themes than the three, and
//! `LIBRI_SHOT_FLAT=1` the flat style.

use std::path::PathBuf;

use libri::app::{self, App, Message};
use libri::browse::Sort;
use libri::lookup::LookupState;
use libri::model::{Date, EntryStatus, MediaType, Released, Season, TagCategory};
use libri::repo::Operation;
use libri::screens::library::{self, Page, SeasonMenu, View};
use libri::screens::setup::{self, Step, Which};
use libri::screens::{Screen, entry, plans};
use libri::settings::{Missing, SyncState, ThemeChoice};
use libri::tags::PickerMessage;
use libri::theme::Theme;

/// A capture: its file name and how it differs from the screen's start.
type Variant = (&'static str, fn(&mut App));

fn nothing(_: &mut App) {}

fn setup_at(step: Step) -> impl Fn(&mut App) {
    move |app| app.setup = setup::State::at(step, app.source.as_ref())
}

/// The movie list with Million Dollar Baby, whose #clint-eastwood is a
/// director and an actor.
fn movies(app: &mut App) {
    let _ = app.update(Message::Library(library::Message::ShowType(Some(
        MediaType::Movies,
    ))));
    let _ = app.update(Message::Library(library::Message::SetView(View::List)));
}

const BABY: &str = "movies/million-dollar-baby-2004";

/// Edit Million Dollar Baby with the form's tag picker open.
fn edit_baby_tags(app: &mut App) {
    movies(app);
    let _ = app.update(Message::Entry(entry::Message::Edit(BABY.into())));
    let _ = app.update(Message::Entry(entry::Message::OpenPicker));
}

/// Million Dollar Baby selected with the details pane's "+ Tag" picker
/// open.
fn details_baby_tags(app: &mut App) {
    movies(app);
    select(app, BABY);
    let _ = app.update(Message::Entry(entry::Message::EditTags(BABY.into())));
}

fn picker(app: &mut App, message: PickerMessage) {
    let _ = app.update(Message::Entry(entry::Message::Picker(message)));
}

const FRIEREN: &str = "anime/frieren-beyond-journeys-end-2023";
const STRANGER: &str = "shows/stranger-things-2016";
const BERSERK_43: &str = "books/berserk-vol-43-2025";
const FELLOWSHIP: &str = "movies/the-lord-of-the-rings-the-fellowship-of-the-ring-2001";

fn show_type(app: &mut App, media: MediaType) {
    let _ = app.update(Message::Library(library::Message::ShowType(Some(media))));
    let _ = app.update(Message::Library(library::Message::SetView(View::List)));
}

fn select(app: &mut App, id: &str) {
    let _ = app.update(Message::Library(library::Message::Select(id.into())));
}

fn open_page(app: &mut App, page: Page) {
    let _ = app.update(Message::Library(library::Message::OpenPage(page)));
}

/// The Add dialog on `media` with `query` typed and its first match picked.
fn add_from(app: &mut App, media: MediaType, query: &str) {
    show_type(app, media);
    let _ = app.update(Message::Entry(entry::Message::Add));
    let _ = app.update(Message::Entry(entry::Message::Type(media)));
    let _ = app.update(Message::Entry(entry::Message::Query(query.into())));
    let _ = app.update(Message::Entry(entry::Message::Pick(0)));
}

/// The Add dialog with a lookup state faked and nothing picked.
fn add_in_state(app: &mut App, state: LookupState) {
    show_type(app, MediaType::Games);
    app.lookup = state;
    let _ = app.update(Message::Entry(entry::Message::Add));
    let _ = app.update(Message::Entry(entry::Message::Type(MediaType::Games)));
    let _ = app.update(Message::Entry(entry::Message::Query("castlevania".into())));
}

fn season(n: u32, released: &str, status: Option<EntryStatus>) -> Season {
    Season {
        season: n,
        released: Released::parse(released),
        status,
        fresh: None,
        manual: None,
    }
}

/// Replaces a show's seasons, for states the fixture lacks.
fn set_seasons(app: &mut App, id: &str, seasons: Vec<Season>) {
    app.apply(Operation::SetSeasons {
        id: id.into(),
        seasons,
        today: app.today,
    });
}

/// Stranger Things with 29 seasons, the latest airing, like the mockup's
/// South Park.
fn many_seasons(app: &mut App) {
    let mut seasons: Vec<Season> = (1..=28)
        .map(|n| season(n, &format!("{}-10-07", 1997 + n), Some(EntryStatus::Done)))
        .collect();
    seasons.push(season(29, "2026-10-07", Some(EntryStatus::InProgress)));
    set_seasons(app, STRANGER, seasons);
}

/// Stranger Things with a second season announced for 2027.
fn announced(app: &mut App, status: Option<EntryStatus>) {
    set_seasons(
        app,
        STRANGER,
        vec![
            season(1, "2016-07-15", Some(EntryStatus::Done)),
            season(2, "2027", status),
        ],
    );
}

#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn capture_every_screen() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    std::fs::create_dir_all(&folder).unwrap();

    let variants: Vec<(Screen, Variant)> = vec![
        (Screen::Splash, ("splash", nothing)),
        (
            Screen::Splash,
            ("splash-first-run", |app| app.source = None),
        ),
        (Screen::Setup, ("setup", nothing)),
        (
            Screen::Setup,
            ("setup-connect", |app| setup_at(Step::Connect)(app)),
        ),
        (
            Screen::Setup,
            ("setup-branch", |app| setup_at(Step::Branch)(app)),
        ),
        (
            Screen::Setup,
            ("setup-file", |app| setup_at(Step::File)(app)),
        ),
        (
            Screen::Setup,
            ("setup-create", |app| setup_at(Step::Create)(app)),
        ),
        // Phase 3: the wizard waiting on GitHub, and a refused token.
        (
            Screen::Setup,
            ("setup-connect-checking", |app| {
                setup_at(Step::Connect)(app);
                app.setup.busy = Some(setup::Busy::Connecting);
            }),
        ),
        (
            Screen::Setup,
            ("setup-connect-refused", |app| {
                setup_at(Step::Connect)(app);
                app.setup.error = Some("GitHub refused the token. Check it and try again.".into());
            }),
        ),
        (
            Screen::Setup,
            ("setup-create-creating", |app| {
                setup_at(Step::Create)(app);
                app.setup.busy = Some(setup::Busy::Creating);
            }),
        ),
        (
            Screen::Setup,
            ("setup-existing", |app| {
                setup_at(Step::Existing)(app);
                app.setup.choice = setup::Choice::Existing;
            }),
        ),
        (
            Screen::Setup,
            ("setup-done", |app| setup_at(Step::Done)(app)),
        ),
        (
            Screen::Setup,
            ("setup-which-library", |app| {
                app.kept_locally = true;
                setup_at(Step::Existing)(app);
                app.setup.choice = setup::Choice::Existing;
                app.setup.which = Some(Which::Here);
            }),
        ),
        (Screen::Library, ("library", nothing)),
        (
            Screen::Library,
            ("library-token-warning", |app| {
                app.sync = SyncState::ExpiresIn(6);
                app.token_warning = true;
            }),
        ),
        (
            Screen::Library,
            ("library-token-expired", |app| {
                app.sync = SyncState::TokenExpired;
            }),
        ),
        // Phase 1k: the sync states, the conflict, local mode, Disconnect
        // and About.
        (
            Screen::Library,
            ("library-sync-saving", |app| {
                app.sync = SyncState::Saving { changes: 2 }
            }),
        ),
        (
            Screen::Library,
            ("library-sync-offline", |app| {
                app.sync = SyncState::Offline { kept: 3 }
            }),
        ),
        (
            Screen::Library,
            ("library-sync-failed", |app| {
                app.sync = SyncState::Failed { kept: 3 }
            }),
        ),
        (
            Screen::Library,
            ("library-sync-not-found", |app| {
                app.sync = SyncState::NotFound(Missing::File);
            }),
        ),
        (
            Screen::Library,
            ("library-sync-conflict", |app| {
                app.sync = SyncState::from_env("conflict").unwrap();
            }),
        ),
        (
            Screen::Library,
            ("library-conflict-dialog", |app| {
                app.sync = SyncState::from_env("conflict").unwrap();
                app.conflict_open = true;
            }),
        ),
        (
            Screen::Plans,
            ("plans-sync-conflict", |app| {
                app.sync = SyncState::from_env("conflict").unwrap();
            }),
        ),
        (
            Screen::Library,
            ("library-local", |app| {
                app.source = None;
                app.kept_locally = true;
            }),
        ),
        (
            Screen::Settings,
            ("settings-source-conflict", |app| {
                app.sync = SyncState::from_env("conflict").unwrap();
            }),
        ),
        (
            Screen::Settings,
            ("settings-source-not-found", |app| {
                app.sync = SyncState::NotFound(Missing::File);
            }),
        ),
        (
            Screen::Settings,
            ("settings-source-local", |app| {
                app.source = None;
                app.kept_locally = true;
            }),
        ),
        (
            Screen::Settings,
            ("settings-disconnect-confirm", |app| {
                let _ = app.update(Message::Settings(
                    libri::screens::settings::Message::Disconnect,
                ));
            }),
        ),
        (
            Screen::Settings,
            ("settings-disconnect-keep-or-delete", |app| {
                let _ = app.update(Message::Settings(
                    libri::screens::settings::Message::Disconnect,
                ));
                let _ = app.update(Message::Settings(
                    libri::screens::settings::Message::ConfirmDisconnect,
                ));
            }),
        ),
        (
            Screen::Settings,
            ("settings-licenses", |app| {
                let _ = app.update(Message::Settings(
                    libri::screens::settings::Message::OpenLicenses,
                ));
            }),
        ),
        (
            Screen::Library,
            ("library-long-text", |app| {
                app.library.force_long = true;
                let _ = app.update(Message::Library(library::Message::ShowType(Some(
                    MediaType::Movies,
                ))));
                let _ = app.update(Message::Library(library::Message::SetView(View::Text)));
                let _ = app.update(Message::Library(library::Message::Sort(Sort::TitleAz)));
            }),
        ),
        (
            Screen::Library,
            ("library-long-list", |app| {
                app.library.force_long = true;
                let _ = app.update(Message::Library(library::Message::ShowType(Some(
                    MediaType::Games,
                ))));
                let _ = app.update(Message::Library(library::Message::SetView(View::List)));
                let _ = app.update(Message::Library(library::Message::Sort(Sort::YearNewest)));
            }),
        ),
        (
            Screen::Library,
            ("music-artists-long", |app| {
                app.library.force_long = true;
                let _ = app.update(Message::Library(library::Message::ShowType(Some(
                    MediaType::Music,
                ))));
                let _ = app.update(Message::Library(library::Message::SetView(View::Text)));
            }),
        ),
        // Phase 1h: tags in several categories, the long groups and the
        // dialogs around them.
        (
            Screen::Library,
            ("library-tag-roles", |app| {
                movies(app);
                let _ = app.update(Message::Library(library::Message::Select(
                    "movies/million-dollar-baby-2004".into(),
                )));
            }),
        ),
        (
            Screen::Library,
            ("library-filter-director", |app| {
                movies(app);
                let _ = app.update(Message::Library(library::Message::ToggleTag(
                    "clint-eastwood".into(),
                    Some(TagCategory::Directors),
                )));
            }),
        ),
        (
            Screen::Library,
            ("tags-filter-picker", |app| {
                let _ = app.update(Message::Library(library::Message::OpenPicker));
                let _ = app.update(Message::Library(library::Message::Picker(
                    PickerMessage::Toggle("clint-eastwood".into(), Some(TagCategory::Directors)),
                )));
            }),
        ),
        (
            Screen::Library,
            ("tags-filter-picker-long", |app| {
                app.library.force_long = true;
                let _ = app.update(Message::Library(library::Message::OpenPicker));
            }),
        ),
        (
            Screen::Library,
            ("tags-filter-picker-drill", |app| {
                app.library.force_long = true;
                let _ = app.update(Message::Library(library::Message::OpenPicker));
                let _ = app.update(Message::Library(library::Message::Picker(
                    PickerMessage::Drill(TagCategory::Genres),
                )));
            }),
        ),
        (Screen::Library, ("edit-tag-picker", edit_baby_tags)),
        (Screen::Library, ("details-tag-picker", details_baby_tags)),
        (
            Screen::Library,
            ("details-new-tag", |app| {
                details_baby_tags(app);
                picker(app, PickerMessage::Query("gothic".into()));
                let _ = app.update(Message::Entry(entry::Message::CreateTag));
            }),
        ),
        (
            Screen::Library,
            ("details-note-draft", |app| {
                movies(app);
                select(app, BABY);
                let _ = app.update(Message::Entry(entry::Message::NoteDraft {
                    id: BABY.into(),
                    note: "Rewatch with the commentary track".into(),
                }));
            }),
        ),
        (
            Screen::Library,
            ("edit-tag-menu", |app| {
                edit_baby_tags(app);
                picker(
                    app,
                    PickerMessage::Menu(Some(("clint-eastwood".into(), TagCategory::Directors))),
                );
            }),
        ),
        (
            Screen::Library,
            ("edit-tag-categories", |app| {
                edit_baby_tags(app);
                picker(app, PickerMessage::Categories("clint-eastwood".into()));
            }),
        ),
        (
            Screen::Library,
            ("edit-tag-add-to", |app| {
                edit_baby_tags(app);
                picker(app, PickerMessage::Query("freeman".into()));
            }),
        ),
        (
            Screen::Library,
            ("edit-tag-move", |app| {
                edit_baby_tags(app);
                picker(app, PickerMessage::Categories("classic".into()));
                let _ = app.update(Message::Entry(entry::Message::CategoryDraft(
                    TagCategory::Languages,
                )));
            }),
        ),
        (
            Screen::Library,
            ("edit-new-tag-two-categories", |app| {
                edit_baby_tags(app);
                picker(app, PickerMessage::Query("gothic".into()));
                let _ = app.update(Message::Entry(entry::Message::CreateTag));
                let _ = app.update(Message::Entry(entry::Message::NewTagCategory(
                    TagCategory::MusicGenres,
                )));
            }),
        ),
        (
            Screen::Library,
            ("edit-show-tag-picker", |app| {
                let _ = app.update(Message::Library(library::Message::ShowType(Some(
                    MediaType::Shows,
                ))));
                let _ = app.update(Message::Entry(entry::Message::EditTags(
                    "shows/stranger-things-2016".into(),
                )));
            }),
        ),
        // Phase 1i: seasons, series and the pages in the content pane.
        (
            Screen::Library,
            ("shows-list", |app| show_type(app, MediaType::Shows)),
        ),
        (
            Screen::Library,
            ("anime-seasons", |app| {
                show_type(app, MediaType::Anime);
                select(app, FRIEREN);
            }),
        ),
        (
            Screen::Library,
            ("anime-season-menu", |app| {
                show_type(app, MediaType::Anime);
                select(app, FRIEREN);
                let _ = app.update(Message::Library(library::Message::SeasonMenu(Some(
                    SeasonMenu {
                        id: FRIEREN.into(),
                        season: 2,
                        in_page: false,
                    },
                ))));
            }),
        ),
        (
            Screen::Library,
            ("show-many-seasons", |app| {
                many_seasons(app);
                show_type(app, MediaType::Shows);
                select(app, STRANGER);
            }),
        ),
        (
            Screen::Library,
            ("page-all-seasons", |app| {
                many_seasons(app);
                show_type(app, MediaType::Shows);
                select(app, STRANGER);
                open_page(
                    app,
                    Page::Seasons {
                        id: STRANGER.into(),
                    },
                );
            }),
        ),
        (
            Screen::Library,
            ("page-season-weekly", |app| {
                set_seasons(
                    app,
                    FRIEREN,
                    vec![
                        season(1, "2023-09-29", Some(EntryStatus::Done)),
                        season(2, "2026-09-24", Some(EntryStatus::InProgress)),
                    ],
                );
                show_type(app, MediaType::Anime);
                select(app, FRIEREN);
                open_page(
                    app,
                    Page::Season {
                        id: FRIEREN.into(),
                        number: 2,
                    },
                );
            }),
        ),
        (
            Screen::Library,
            ("page-season-all-out", |app| {
                show_type(app, MediaType::Anime);
                select(app, FRIEREN);
                open_page(
                    app,
                    Page::Season {
                        id: FRIEREN.into(),
                        number: 1,
                    },
                );
            }),
        ),
        (
            Screen::Library,
            ("show-one-season", |app| {
                show_type(app, MediaType::Shows);
                select(app, STRANGER);
            }),
        ),
        (
            Screen::Library,
            ("show-season-announced", |app| {
                announced(app, None);
                show_type(app, MediaType::Shows);
                select(app, STRANGER);
            }),
        ),
        (
            Screen::Library,
            ("season-dropped", |app| {
                announced(app, Some(EntryStatus::Want));
                show_type(app, MediaType::Shows);
                select(app, STRANGER);
                app.dialog = Some(entry::Dialog::SeasonDropped {
                    id: STRANGER.into(),
                    season: 2,
                });
            }),
        ),
        (
            Screen::Library,
            ("movies-series-row", |app| {
                app.library.force_long = true;
                show_type(app, MediaType::Movies);
                let _ = app.update(Message::Library(library::Message::Sort(Sort::YearOldest)));
            }),
        ),
        (
            Screen::Library,
            ("movies-series-grid", |app| {
                app.library.force_long = true;
                show_type(app, MediaType::Movies);
                let _ = app.update(Message::Library(library::Message::SetView(View::Grid)));
            }),
        ),
        (
            Screen::Library,
            ("movie-in-series", |app| {
                show_type(app, MediaType::Movies);
                select(app, FELLOWSHIP);
            }),
        ),
        (
            Screen::Library,
            ("page-series", |app| {
                show_type(app, MediaType::Movies);
                open_page(
                    app,
                    Page::Series {
                        media: MediaType::Movies,
                        name: "The Lord of the Rings".into(),
                        highlight: None,
                    },
                );
            }),
        ),
        (
            Screen::Library,
            ("series-cover", |app| {
                show_type(app, MediaType::Movies);
                open_page(
                    app,
                    Page::Series {
                        media: MediaType::Movies,
                        name: "The Lord of the Rings".into(),
                        highlight: None,
                    },
                );
                let _ = app.update(Message::Library(library::Message::OpenSeriesCover(
                    MediaType::Movies,
                    "The Lord of the Rings".into(),
                )));
            }),
        ),
        (
            Screen::Library,
            ("book-long-series", |app| {
                show_type(app, MediaType::Books);
                select(app, BERSERK_43);
            }),
        ),
        (
            Screen::Library,
            ("page-all-volumes", |app| {
                show_type(app, MediaType::Books);
                select(app, BERSERK_43);
                open_page(
                    app,
                    Page::Series {
                        media: MediaType::Books,
                        name: "Berserk".into(),
                        highlight: Some(BERSERK_43.into()),
                    },
                );
            }),
        ),
        (
            Screen::Library,
            ("edit-book-series", |app| {
                show_type(app, MediaType::Books);
                let _ = app.update(Message::Entry(entry::Message::Edit(BERSERK_43.into())));
            }),
        ),
        (
            Screen::Library,
            ("edit-anime-seasons", |app| {
                show_type(app, MediaType::Anime);
                let _ = app.update(Message::Entry(entry::Message::Edit(FRIEREN.into())));
            }),
        ),
        (
            Screen::Library,
            ("edit-anime-season-menu", |app| {
                show_type(app, MediaType::Anime);
                let _ = app.update(Message::Entry(entry::Message::Edit(FRIEREN.into())));
                let _ = app.update(Message::Entry(entry::Message::FormSeasonMenu(Some(2))));
            }),
        ),
        // Phase 1j: the Add and Edit dialog's lookup pane, cover and platform.
        (
            Screen::Library,
            ("add-game-from-match", |app| {
                add_from(app, MediaType::Games, "castlevania")
            }),
        ),
        (
            Screen::Library,
            ("add-game-on-platform", |app| {
                show_type(app, MediaType::Games);
                let _ = app.update(Message::Entry(entry::Message::Add));
                let _ = app.update(Message::Entry(entry::Message::Type(MediaType::Games)));
                let _ = app.update(Message::Entry(entry::Message::PickPlatform(Some(
                    "Super Nintendo".into(),
                ))));
                let _ = app.update(Message::Entry(entry::Message::Query(
                    "street fighter".into(),
                )));
                let _ = app.update(Message::Entry(entry::Message::Pick(0)));
            }),
        ),
        (
            Screen::Library,
            ("add-platform-picker", |app| {
                show_type(app, MediaType::Games);
                let _ = app.update(Message::Entry(entry::Message::Add));
                let _ = app.update(Message::Entry(entry::Message::Type(MediaType::Games)));
                let _ = app.update(Message::Entry(entry::Message::Query(
                    "street fighter".into(),
                )));
                let _ = app.update(Message::Entry(entry::Message::TogglePlatformPicker));
            }),
        ),
        (
            Screen::Library,
            ("add-platform-picker-searching", |app| {
                show_type(app, MediaType::Games);
                let _ = app.update(Message::Entry(entry::Message::Add));
                let _ = app.update(Message::Entry(entry::Message::Type(MediaType::Games)));
                let _ = app.update(Message::Entry(entry::Message::Query(
                    "street fighter".into(),
                )));
                let _ = app.update(Message::Entry(entry::Message::TogglePlatformPicker));
                let _ = app.update(Message::Entry(entry::Message::PlatformQuery("snes".into())));
            }),
        ),
        (
            Screen::Library,
            ("add-movie-actors", |app| {
                add_from(app, MediaType::Movies, "weapons")
            }),
        ),
        (
            Screen::Library,
            ("add-show-streaming", |app| {
                add_from(app, MediaType::Shows, "adolescence")
            }),
        ),
        (
            Screen::Library,
            ("add-book-ebook-year", |app| {
                add_from(app, MediaType::Books, "a christmas carol")
            }),
        ),
        (
            Screen::Library,
            ("add-looking-up", |app| {
                add_in_state(app, LookupState::Loading)
            }),
        ),
        (
            Screen::Library,
            ("add-offline", |app| add_in_state(app, LookupState::Offline)),
        ),
        (
            Screen::Library,
            ("add-not-set-up", |app| {
                add_in_state(app, LookupState::NotSetUp)
            }),
        ),
        (
            Screen::Library,
            ("add-partial", |app| add_in_state(app, LookupState::Partial)),
        ),
        (
            Screen::Library,
            ("add-partial-none", |app| {
                add_in_state(app, LookupState::PartialNone)
            }),
        ),
        (
            Screen::Library,
            ("add-no-matches", |app| {
                show_type(app, MediaType::Games);
                let _ = app.update(Message::Entry(entry::Message::Add));
                let _ = app.update(Message::Entry(entry::Message::Type(MediaType::Games)));
                let _ = app.update(Message::Entry(entry::Message::Query("zork".into())));
            }),
        ),
        (
            Screen::Library,
            ("edit-look-up-again", |app| {
                show_type(app, MediaType::Books);
                let _ = app.update(Message::Entry(entry::Message::Edit(
                    "books/dracula-1897".into(),
                )));
                let _ = app.update(Message::Entry(entry::Message::LookAgain));
                let _ = app.update(Message::Entry(entry::Message::Pick(0)));
            }),
        ),
        (
            Screen::Library,
            ("edit-cover-from-other", |app| {
                show_type(app, MediaType::Books);
                let _ = app.update(Message::Entry(entry::Message::Edit(
                    "books/dracula-1897".into(),
                )));
                let _ = app.update(Message::Entry(entry::Message::LookAgain));
                let _ = app.update(Message::Entry(entry::Message::Pick(0)));
                let _ = app.update(Message::Entry(entry::Message::CancelLookAgain));
                let _ = app.update(Message::Entry(entry::Message::PickCover(Some(
                    "itunes:498187584".into(),
                ))));
            }),
        ),
        (
            Screen::Library,
            ("edit-pick-cover", |app| {
                show_type(app, MediaType::Books);
                let _ = app.update(Message::Entry(entry::Message::Edit(
                    "books/dracula-1897".into(),
                )));
                let _ = app.update(Message::Entry(entry::Message::LookAgain));
                let _ = app.update(Message::Entry(entry::Message::Pick(0)));
                let _ = app.update(Message::Entry(entry::Message::CancelLookAgain));
                let _ = app.update(Message::Entry(entry::Message::OpenCover));
            }),
        ),
        (Screen::Plans, ("plans", nothing)),
        (Screen::Settings, ("settings", nothing)),
        (
            Screen::Settings,
            ("settings-flat", |app| app.settings.flat = true),
        ),
    ];

    // `LIBRI_SHOT=settings` renders only the variants whose name contains it.
    let only = std::env::var("LIBRI_SHOT").ok();
    let themes = match std::env::var("LIBRI_SHOT_THEME") {
        Ok(names) => names
            .split(',')
            .map(|name| {
                // "mocha" names Catppuccin Mocha.
                let name = name.trim().to_lowercase();
                Theme::ALL
                    .into_iter()
                    .find(|theme| {
                        theme
                            .label()
                            .to_lowercase()
                            .split_whitespace()
                            .any(|word| word == name)
                    })
                    .unwrap_or_else(|| panic!("no theme {name}"))
            })
            .collect(),
        Err(_) => vec![Theme::Graphite, Theme::Light, Theme::Glass],
    };
    let flat = std::env::var_os("LIBRI_SHOT_FLAT").is_some();
    for (screen, (name, change)) in variants {
        if only
            .as_ref()
            .is_some_and(|only| !name.contains(only.as_str()))
        {
            continue;
        }
        for &theme in &themes {
            let mut app = App::new(screen);
            app.settings.theme = ThemeChoice::Fixed(theme);
            app.settings.flat = flat;
            // The fixture's dates are set around this day.
            app.today = Date::parse("2026-10-19").unwrap();
            change(&mut app);
            // Settings is taller than the window; capture all of it.
            let size = if screen == Screen::Settings {
                iced::Size::new(app::WINDOW_SIZE.width, 2500.0)
            } else {
                app::WINDOW_SIZE
            };
            let mut ui = iced_test::Simulator::with_size(app::settings(), size, app.view());
            let iced_theme = app.tokens().iced_theme();
            if screen == Screen::Settings {
                // iced decodes PNGs (the About logos) on a worker thread and
                // asks for a redraw when they are ready, which the headless
                // shell ignores: render once, wait, then take the capture.
                let _ = ui.snapshot(&iced_theme).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let snapshot = ui.snapshot(&iced_theme).unwrap();

            // `matches_image` writes the file when it is missing and adds the
            // renderer to the name ("library-light-wgpu.png").
            let style = if flat { "-flat" } else { "" };
            let theme_name = theme.label().to_lowercase().replace(' ', "-");
            let name = format!("{name}-{theme_name}{style}");
            let path = folder.join(format!("{name}.png"));
            let written = folder.join(format!("{name}-wgpu.png"));
            let _ = std::fs::remove_file(&written);
            snapshot.matches_image(&path).unwrap();
            assert!(written.exists() || path.exists(), "no capture for {name}");
        }
    }
}

/// The window sizes of phase 1g: the minimum, the common screens, and
/// 1080p and 1440p at KDE's 125 % and 150 % scaling (their logical sizes).
/// Captures the library with the details pane, the overview and a plan
/// page in Graphite, to `screenshots/sizes/`.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn capture_window_sizes() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("screenshots/sizes");
    std::fs::create_dir_all(&folder).unwrap();

    let sizes = [
        ("min", app::MIN_WINDOW_SIZE),
        ("1280x720", iced::Size::new(1280.0, 720.0)),
        ("1366x768", iced::Size::new(1366.0, 768.0)),
        ("1536x864", iced::Size::new(1536.0, 864.0)),
        ("1706x960", iced::Size::new(1706.0, 960.0)),
        ("1920x1080", iced::Size::new(1920.0, 1080.0)),
        ("2048x1152", iced::Size::new(2048.0, 1152.0)),
        ("2560x1440", iced::Size::new(2560.0, 1440.0)),
    ];
    let pages: Vec<Variant> = vec![
        ("overview", nothing),
        ("library", |app| {
            let _ = app.update(Message::Library(library::Message::ShowType(Some(
                MediaType::Movies,
            ))));
            let _ = app.update(Message::Library(library::Message::SetView(View::Grid)));
            let _ = app.update(Message::Library(library::Message::Select(
                "movies/die-hard-1988".into(),
            )));
        }),
        ("plan", |app| {
            let _ = app.update(Message::Plans(plans::Message::ShowPlan(
                "halloween-2026".into(),
            )));
            let _ = app.update(Message::Plans(plans::Message::Select(
                "movies/die-hard-1988".into(),
            )));
        }),
    ];
    for (size_name, size) in sizes {
        for (page, change) in &pages {
            let mut app = App::new(Screen::Library);
            app.settings.theme = ThemeChoice::Fixed(Theme::Graphite);
            app.today = Date::parse("2026-10-19").unwrap();
            change(&mut app);
            app.window = size;
            let mut ui = iced_test::Simulator::with_size(app::settings(), size, app.view());
            let snapshot = ui.snapshot(&app.tokens().iced_theme()).unwrap();
            let name = format!("{size_name}-{page}");
            let path = folder.join(format!("{name}.png"));
            let _ = std::fs::remove_file(folder.join(format!("{name}-wgpu.png")));
            snapshot.matches_image(&path).unwrap();
        }
    }
}
