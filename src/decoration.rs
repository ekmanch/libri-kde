//! What KWin draws for Libri's window: the title bar in Libri's colors,
//! and the blur behind the window with the Glass theme.
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
//!
//! Glass paints see-through surfaces, and asks KWin through the
//! `org_kde_kwin_blur` protocol to blur what is behind the whole window,
//! as Plasma's own panels do; the blur effect (or Better Blur) does the
//! rest, with the strength, noise and tint the owner set for it. Any other
//! theme takes the request back.

use std::fmt;
use std::sync::Arc;

use iced::window::{self, Window};
use iced::{Color, Task};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};
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
        // The chrome's shade at the chrome's strength: Klassy takes the
        // alpha (unless its own opacity override is on), Breeze does not.
        (Theme::Glass, _) => scheme!("glass"),
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

/// Libri's hold on what KWin draws for its window: the palette object KWin
/// keeps for the title bar and the scheme named last, and the blur request
/// while Glass is on.
#[derive(Clone)]
pub struct Decoration {
    connection: Connection,
    /// The queue the objects belong to, kept for as long as they are.
    _queue: Arc<EventQueue<Registry>>,
    handle: QueueHandle<Registry>,
    surface: WlSurface,
    palette: OrgKdeKwinServerDecorationPalette,
    current: &'static str,
    /// `None` without KWin's blur effect, or any other compositor.
    blur_manager: Option<OrgKdeKwinBlurManager>,
    blur: Option<OrgKdeKwinBlur>,
}

impl Decoration {
    /// Takes hold of the main window's decoration once the window is open:
    /// `None` anywhere but Plasma Wayland. Boxed, since the Wayland objects
    /// would otherwise make it the app's largest message by far.
    pub fn take() -> Task<Option<Box<Decoration>>> {
        window::oldest()
            .and_then(|id| window::run(id, |window| Decoration::attach(window).map(Box::new)))
    }

    fn attach(window: &dyn Window) -> Option<Decoration> {
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
        let blur_manager = globals.bind(&handle, 1..=1, ()).ok();
        Some(Decoration {
            connection,
            _queue: Arc::new(queue),
            handle,
            surface,
            palette,
            current: "",
            blur_manager,
            blur: None,
        })
    }

    /// Names the scheme for `theme` and `flat` to KWin, and asks for the
    /// blur behind the window or takes the request back, when either
    /// changed.
    pub fn set(&mut self, theme: Theme, flat: bool) {
        let path = scheme_path(theme, flat);
        let mut changed = false;
        if path != self.current {
            self.current = path;
            self.palette.set_palette(path.to_owned());
            changed = true;
        }
        if let Some(manager) = &self.blur_manager
            && theme.is_glass() != self.blur.is_some()
        {
            match self.blur.take() {
                Some(blur) => {
                    manager.unset(&self.surface);
                    blur.release();
                }
                None => {
                    // No region: the whole surface. KWin applies it with
                    // the surface's next commit, the redraw of the theme
                    // change.
                    let blur = manager.create(&self.surface, &self.handle, ());
                    blur.set_region(None);
                    blur.commit();
                    self.blur = Some(blur);
                }
            }
            changed = true;
        }
        if changed {
            let _ = self.connection.flush();
        }
    }

    /// Whether KWin is blurring what is behind the window.
    pub fn blurred(&self) -> bool {
        self.blur.is_some()
    }
}

impl fmt::Debug for Decoration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Decoration")
            .field("current", &self.current)
            .field("blurred", &self.blurred())
            .finish()
    }
}

/// The state behind the objects' queue. No event ever arrives: the
/// registry's globals are read once, and the palette and the blur have
/// none.
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
delegate_noop!(Registry: ignore OrgKdeKwinBlurManager);
delegate_noop!(Registry: ignore OrgKdeKwinBlur);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::theme::PEACH;

    /// A scheme entry: the color, and the alpha where there is one.
    type Entry = ((u8, u8, u8), Option<u8>);

    /// The `key=r,g,b` or `key=r,g,b,a` entries of a scheme file, by
    /// `group.key`.
    fn entries(path: &str) -> HashMap<String, Entry> {
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

    fn parse_rgb(value: &str) -> Option<Entry> {
        let mut parts = value.split(',').map(|part| part.trim().parse::<u8>().ok());
        let rgb = (parts.next()??, parts.next()??, parts.next()??);
        Some((rgb, parts.next().flatten()))
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
                // Glass carries the chrome's strength as the bar's alpha.
                let alpha = theme.is_glass().then_some(140);
                let expect = |key: &str, color: Color| {
                    let (got, got_alpha) = entries[key];
                    assert_eq!(
                        (got, got_alpha),
                        (
                            rgb(color),
                            if key.ends_with("Background") || key.ends_with("BackgroundNormal") {
                                alpha
                            } else {
                                None
                            }
                        ),
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
        assert_eq!(
            scheme_path(Theme::Glass, false),
            scheme_path(Theme::Glass, true)
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
