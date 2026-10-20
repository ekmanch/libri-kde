//! Renders every screen headless, in Graphite and Light at the default
//! window size, to `screenshots/` (git-ignored), to compare with the
//! mockup. Skipped by a plain `cargo test`; run it with:
//!
//! ```fish
//! cargo test --test screenshots -- --ignored
//! ```

use std::path::PathBuf;

use libri::app::{self, App, Message};
use libri::browse::Sort;
use libri::model::{Date, MediaType};
use libri::screens::library::{self, View};
use libri::screens::setup::{self, Step};
use libri::screens::{Screen, plans};
use libri::settings::{SyncState, ThemeChoice};
use libri::theme::Theme;

/// A capture: its file name and how it differs from the screen's start.
type Variant = (&'static str, fn(&mut App));

fn nothing(_: &mut App) {}

fn setup_at(step: Step) -> impl Fn(&mut App) {
    move |app| app.setup = setup::State::at(step, app.source.as_ref())
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
        (Screen::Plans, ("plans", nothing)),
        (Screen::Settings, ("settings", nothing)),
        (
            Screen::Settings,
            ("settings-flat", |app| app.settings.flat = true),
        ),
    ];

    for (screen, (name, change)) in variants {
        for theme in [Theme::Graphite, Theme::Light] {
            let mut app = App::new(screen);
            app.settings.theme = ThemeChoice::Fixed(theme);
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
            let snapshot = ui.snapshot(&app.tokens().iced_theme()).unwrap();

            // `matches_image` writes the file when it is missing and adds the
            // renderer to the name ("library-light-wgpu.png").
            let name = format!("{name}-{}", theme.label().to_lowercase());
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
