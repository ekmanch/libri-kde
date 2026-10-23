//! KWin's title bar in Libri's colors.
//!
//! The title bar is KDE's own (docs/DESIGN.md section 3), so Libri cannot
//! paint it. On Plasma Wayland KWin takes a color scheme per window through
//! the `org_kde_kwin_server_decoration_palette` protocol, the way Konsole
//! tints its title bar per profile. Breeze, Darkly and Klassy follow it; an
//! Aurorae theme paints its own fixed colors and ignores it. Libri ships one
//! scheme file per theme and style under `assets/color-schemes/`, with the
//! chrome as the bar and the secondary text as the title, and names the one
//! for the current theme once the window is open and whenever the theme
//! changes. Anywhere else nothing happens.
//!
//! Each file also carries the chrome and the text as the window colors.
//! Klassy keeps the colors it derived until the window's application
//! palette changes, and the title bar entries are not part of that palette,
//! so without the window colors every scheme after the first was ignored.

use std::fmt;
use std::sync::Arc;

use iced::window::{self, Window};
use iced::{Color, Task};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_plasma::server_decoration_palette::client::{
    org_kde_kwin_server_decoration_palette::OrgKdeKwinServerDecorationPalette,
    org_kde_kwin_server_decoration_palette_manager::OrgKdeKwinServerDecorationPaletteManager,
};

use crate::theme::{Theme, Tokens};

/// The scheme file for a theme and style: an absolute path into the
/// checkout, which is where `cargo run` finds it (TODO.md, phase 5, has
/// the installed copy).
pub fn scheme_path(theme: Theme, flat: bool) -> &'static str {
    macro_rules! scheme {
        ($name:literal) => {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/color-schemes/",
                $name,
                ".colors"
            )
        };
    }
    match (theme, flat) {
        (Theme::Graphite, false) => scheme!("graphite"),
        (Theme::Graphite, true) => scheme!("graphite-flat"),
        (Theme::Charcoal, _) => scheme!("charcoal"),
        (Theme::Mocha, _) => scheme!("mocha"),
        (Theme::Light, false) => scheme!("light"),
        (Theme::Light, true) => scheme!("light-flat"),
    }
}

/// The title of a window behind another: halfway from the secondary text
/// to the chrome.
pub fn inactive_title(tokens: &Tokens) -> Color {
    let (a, b) = (tokens.sub, tokens.chrome);
    Color::from_rgb((a.r + b.r) / 2.0, (a.g + b.g) / 2.0, (a.b + b.b) / 2.0)
}

/// Libri's hold on its window's title bar: the palette object KWin keeps
/// for the window, and the scheme named last.
#[derive(Clone)]
pub struct TitleBar {
    connection: Connection,
    /// The queue the palette objects belong to, kept for as long as they are.
    _queue: Arc<EventQueue<Registry>>,
    palette: OrgKdeKwinServerDecorationPalette,
    current: &'static str,
}

impl TitleBar {
    /// Takes hold of the main window's title bar once the window is open:
    /// `None` anywhere but Plasma Wayland.
    pub fn take() -> Task<Option<TitleBar>> {
        window::oldest().and_then(|id| window::run(id, TitleBar::attach))
    }

    fn attach(window: &dyn Window) -> Option<TitleBar> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Wayland(surface) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: winit owns the connection and the surface for as long as
        // the window lives, and the closure runs on the event loop's thread
        // while the window is open. The backend is told the display is
        // foreign, so it never closes it.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let id =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
                .ok()?;
        let surface = WlSurface::from_id(&connection, id).ok()?;
        let (globals, queue) = registry_queue_init::<Registry>(&connection).ok()?;
        let handle = queue.handle();
        let manager: OrgKdeKwinServerDecorationPaletteManager =
            globals.bind(&handle, 1..=1, ()).ok()?;
        let palette = manager.create(&surface, &handle, ());
        Some(TitleBar {
            connection,
            _queue: Arc::new(queue),
            palette,
            current: "",
        })
    }

    /// Names the scheme for `theme` and `flat` to KWin, when it changed.
    pub fn set(&mut self, theme: Theme, flat: bool) {
        let path = scheme_path(theme, flat);
        if path == self.current {
            return;
        }
        self.current = path;
        self.palette.set_palette(path.to_owned());
        let _ = self.connection.flush();
    }
}

impl fmt::Debug for TitleBar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TitleBar")
            .field("current", &self.current)
            .finish()
    }
}

/// The state behind the palette objects' queue. No event ever arrives:
/// the registry's globals are read once, and the palette has none.
struct Registry;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Registry {
    fn event(
        _: &mut Registry,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Registry>,
    ) {
    }
}

delegate_noop!(Registry: ignore OrgKdeKwinServerDecorationPaletteManager);
delegate_noop!(Registry: ignore OrgKdeKwinServerDecorationPalette);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::theme::PEACH;

    /// The `key=r,g,b` entries of a scheme file, by `group.key`.
    fn entries(path: &str) -> HashMap<String, (u8, u8, u8)> {
        let text = std::fs::read_to_string(path).expect("the scheme file is in the checkout");
        let mut group = String::new();
        let mut entries = HashMap::new();
        for line in text.lines() {
            if let Some(name) = line.strip_prefix('[') {
                group = name.trim_end_matches(']').replace("][", ":");
            } else if let Some((key, value)) = line.split_once('=')
                && let Some(rgb) = parse_rgb(value)
            {
                entries.insert(format!("{group}.{key}"), rgb);
            }
        }
        entries
    }

    fn parse_rgb(value: &str) -> Option<(u8, u8, u8)> {
        let mut parts = value.split(',').map(|part| part.trim().parse::<u8>().ok());
        Some((parts.next()??, parts.next()??, parts.next()??))
    }

    fn rgb(color: Color) -> (u8, u8, u8) {
        let channel = |value: f32| (value * 255.0).round() as u8;
        (channel(color.r), channel(color.g), channel(color.b))
    }

    #[test]
    fn every_scheme_paints_the_bar_in_the_chrome_and_the_title_in_the_secondary_text() {
        for theme in Theme::ALL {
            for flat in [false, true] {
                let tokens = Tokens::styled(theme, PEACH, flat);
                let entries = entries(scheme_path(theme, flat));
                let expect = |key: &str, color: Color| {
                    assert_eq!(
                        entries.get(key).copied(),
                        Some(rgb(color)),
                        "{theme:?} flat={flat} {key}"
                    );
                };
                let inactive = inactive_title(&tokens);
                expect("Colors:Window.BackgroundNormal", tokens.chrome);
                expect("Colors:Window.ForegroundNormal", tokens.text);
                expect("WM.activeBackground", tokens.chrome);
                expect("WM.activeForeground", tokens.sub);
                expect("WM.inactiveBackground", tokens.chrome);
                expect("WM.inactiveForeground", inactive);
                expect("Colors:Header.BackgroundNormal", tokens.chrome);
                expect("Colors:Header.ForegroundNormal", tokens.sub);
                expect("Colors:Header:Inactive.BackgroundNormal", tokens.chrome);
                expect("Colors:Header:Inactive.ForegroundNormal", inactive);
            }
        }
    }

    #[test]
    fn themes_whose_chrome_does_not_change_share_one_scheme() {
        assert_eq!(
            scheme_path(Theme::Charcoal, false),
            scheme_path(Theme::Charcoal, true)
        );
        assert_eq!(
            scheme_path(Theme::Mocha, false),
            scheme_path(Theme::Mocha, true)
        );
        assert_ne!(
            scheme_path(Theme::Graphite, false),
            scheme_path(Theme::Graphite, true)
        );
        assert_ne!(
            scheme_path(Theme::Light, false),
            scheme_path(Theme::Light, true)
        );
    }
}
