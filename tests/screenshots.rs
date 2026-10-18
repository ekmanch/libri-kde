//! Renders every screen headless, in Graphite and Light at the default
//! window size, to `screenshots/` (git-ignored), to compare with the
//! mockup. Skipped by a plain `cargo test`; run it with:
//!
//! ```fish
//! cargo test --test screenshots -- --ignored
//! ```

use std::path::PathBuf;

use libri::app::{self, App};
use libri::screens::Screen;
use libri::theme::Theme;

#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn capture_every_screen() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    std::fs::create_dir_all(&folder).unwrap();

    for screen in Screen::ALL {
        for theme in [Theme::Graphite, Theme::Light] {
            let mut app = App::new(screen);
            app.theme = theme;
            let mut ui =
                iced_test::Simulator::with_size(app::settings(), app::WINDOW_SIZE, app.view());
            let snapshot = ui.snapshot(&app.tokens().iced_theme()).unwrap();

            // `matches_image` writes the file when it is missing and adds the
            // renderer to the name ("library-light-wgpu.png").
            let name = format!("{}-{}", screen.name(), theme.label().to_lowercase());
            let path = folder.join(format!("{name}.png"));
            let written = folder.join(format!("{name}-wgpu.png"));
            let _ = std::fs::remove_file(&written);
            snapshot.matches_image(&path).unwrap();
            assert!(written.exists() || path.exists(), "no capture for {name}");
        }
    }
}
