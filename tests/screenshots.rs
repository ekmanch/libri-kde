//! Renders every screen headless, in Graphite and Light at the default
//! window size, to `screenshots/` (git-ignored), to compare with the
//! mockup. Skipped by a plain `cargo test`; run it with:
//!
//! ```fish
//! cargo test --test screenshots -- --ignored
//! ```

use std::path::PathBuf;

use libri::app::{self, App};
use libri::model::Date;
use libri::screens::Screen;
use libri::screens::setup::{self, Step};
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
        (Screen::Plans, ("plans", nothing)),
        (Screen::Settings, ("settings", nothing)),
        (
            Screen::Settings,
            ("settings-flat", |app| app.settings.flat = true),
        ),
        (Screen::TokenSheet, ("tokens", nothing)),
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
