//! Design tokens from the desktop mockup (`design/canvas.html`): the five
//! themes, the type colors, the accent palette and the fonts. Never
//! hard-code a color or font in a screen; add it here instead.

use iced::font::{Style, Weight};
use iced::{Color, Font, theme::Palette};

use crate::model::MediaType;

const fn hex(rgb: u32) -> Color {
    Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// White at `alpha`: the veils Glass lays on its see-through surfaces.
const fn white(alpha: f32) -> Color {
    Color::from_rgba(1.0, 1.0, 1.0, alpha)
}

/// The five themes. Settings also offers System, which follows Plasma and
/// resolves to Graphite or Light (docs/DESIGN.md 4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Theme {
    #[default]
    Graphite,
    Charcoal,
    /// Catppuccin Mocha, with the Catppuccin type colors.
    Mocha,
    /// Dark and see-through: flat Graphite's shade at a fixed strength,
    /// with KWin's blur behind the window (docs/DESIGN.md section 8).
    Glass,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 5] = [
        Theme::Graphite,
        Theme::Charcoal,
        Theme::Mocha,
        Theme::Glass,
        Theme::Light,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Graphite => "Graphite",
            Theme::Charcoal => "Charcoal",
            Theme::Mocha => "Catppuccin Mocha",
            Theme::Glass => "Glass",
            Theme::Light => "Light",
        }
    }

    pub fn is_light(self) -> bool {
        self == Theme::Light
    }

    /// Whether the surfaces are see-through and the window asks KWin for
    /// blur behind it.
    pub fn is_glass(self) -> bool {
        self == Theme::Glass
    }
}

/// An accent color: the fill used on both themes, and the darker ink used
/// for accent text, icons and outlines on the light theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accent {
    pub name: &'static str,
    pub fill: Color,
    pub ink: Color,
}

impl Accent {
    /// The accent as `theme` shows it: with Catppuccin Mocha the default
    /// Peach becomes Catppuccin's own peach.
    pub fn for_theme(self, theme: Theme) -> Accent {
        if theme == Theme::Mocha && self == PEACH {
            Accent {
                fill: hex(0xFAB387),
                ink: hex(0xFAB387),
                ..self
            }
        } else {
            self
        }
    }
}

const fn accent(name: &'static str, fill: u32, ink: u32) -> Accent {
    Accent {
        name,
        fill: hex(fill),
        ink: hex(ink),
    }
}

pub const VIVID: [Accent; 6] = [
    accent("Peach", 0xF8B289, 0xC2521B),
    accent("Violet", 0xA78BFA, 0x7C5CE0),
    accent("Hot pink", 0xFF7AB6, 0xD6337F),
    accent("Amber", 0xF2B544, 0xA36706),
    accent("Green", 0x5EE08A, 0x15803D),
    accent("Blue", 0x5AA9FF, 0x1D6FE0),
];

pub const CATPPUCCIN: [Accent; 10] = [
    accent("Beige", 0xF9E2AF, 0xA66706),
    accent("Rosewater", 0xF5E0DC, 0xAD604F),
    accent("Flamingo", 0xF2CDCD, 0xB85759),
    accent("Pink", 0xF5C2E7, 0xBA4A9E),
    accent("Red", 0xF38BA8, 0xD20F39),
    accent("Mauve", 0xCBA6F7, 0x8839EF),
    accent("Lavender", 0xB4BEFE, 0x596ADD),
    accent("Blue", 0x89B4FA, 0x1E66F5),
    accent("Sky", 0x89DCEB, 0x017DAF),
    accent("Green", 0xA6E3A1, 0x238700),
];

/// The default accent.
pub const PEACH: Accent = VIVID[0];

/// Seasonal accents, used only inside a plan with a seasonal theme.
pub const HALLOWEEN: Color = hex(0xF08A3C);
pub const CHRISTMAS: Color = hex(0x35A56A);

/// Placeholder cover colors, until covers are fetched in phase 4.
const COVERS: [u32; 8] = [
    0x4B5F8A, 0x8A6E3F, 0x3F6B8A, 0x6B4B8A, 0x8A553F, 0x3F8A6E, 0x8A3F5F, 0x5F6B3F,
];

/// An entry's placeholder cover color, picked from its id as the mockup
/// does (`coverOf`), so the same entry always gets the same color.
pub fn cover_color(id: &str) -> Color {
    let sum: usize = id
        .encode_utf16()
        .enumerate()
        .map(|(i, c)| usize::from(c) * (i + 3))
        .sum();
    hex(COVERS[sum % COVERS.len()])
}

/// The status badge on a grid cover: its dark disc and the green mark.
pub const BADGE: Color = hex(0x2A2D31);
pub const BADGE_MARK: Color = hex(0x5EE08A);
/// The DNF badge's mark.
pub const BADGE_DNF: Color = hex(0xA3A8AE);

/// The cover viewer, dark on every theme as in the mockup: its text,
/// the second line, the round close button's fill and Change cover…'s
/// outline and fill.
pub const VIEWER_TEXT: Color = hex(0xEEF0F2);
pub const VIEWER_SUB: Color = hex(0xA3A8AE);
pub const VIEWER_CLOSE: Color = white(0.08);
pub const VIEWER_LINE: Color = white(0.18);
pub const VIEWER_FILL: Color = white(0.06);

/// The cover viewer's backdrop over the dialog scrim `under`, so the two
/// together are the mockup's near-black at 82 %.
pub fn viewer_scrim(under: Color) -> Color {
    let alpha = 1.0 - 0.18 / (1.0 - under.a).max(0.18);
    Color::from_rgba8(8, 9, 10, alpha.clamp(0.0, 1.0))
}

/// The sync row's dot: in sync, and failing (the token expired).
pub const SYNC_OK: Color = hex(0xA6E3A1);
pub const SYNC_ERROR: Color = hex(0xF38BA8);

/// Every color a screen draws with, resolved for one theme and accent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub theme: Theme,
    pub light: bool,
    /// The window frame and sidebar.
    pub chrome: Color,
    /// The content pane.
    pub content: Color,
    /// Neutral buttons, chips and cards.
    pub raised: Color,
    /// Text fields.
    pub field: Color,
    pub line: Color,
    pub text: Color,
    /// Secondary text and section headers.
    pub sub: Color,
    /// The selected row or sidebar item. Never the accent.
    pub selection: Color,
    /// A hovered row.
    pub hover: Color,
    /// Progress and slider tracks.
    pub track: Color,
    /// Popovers and menus.
    pub popover: Color,
    /// The Add and Edit dialog's surface, and the lookup pane beside it.
    /// Opaque on every theme: a dialog never shows the screen through it,
    /// so on Glass they are Graphite's content and chrome.
    pub dialog: Color,
    pub dialog_chrome: Color,
    /// Destructive text: Delete, Disconnect.
    pub danger: Color,
    /// Behind a modal dialog.
    pub scrim: Color,
    /// Under popovers and dialogs.
    pub shadow: Color,
    /// The outline that marks the view on the grouped list's type rail.
    pub rail_thumb: Color,
    pub accent: Color,
    /// Accent text, icons and outlines.
    pub ink: Color,
    /// Text on an accent fill.
    pub on_accent: Color,
    /// The green of a connected state, and its faint card fill.
    pub success: Color,
    pub success_fill: Color,
    /// The amber of a warning (the token expiry dialog), and its disc.
    pub warning: Color,
    pub warning_fill: Color,
    /// The faint red card fill behind an error state.
    pub danger_fill: Color,
}

impl Tokens {
    /// The two-tone tokens: a darker chrome and a lighter content pane.
    pub fn new(theme: Theme, accent: Accent) -> Tokens {
        Tokens::styled(theme, accent, false)
    }

    /// The tokens for a theme and accent; `flat` uses one shade for the
    /// chrome and the content (docs/DESIGN.md section 8).
    pub fn styled(theme: Theme, accent: Accent, flat: bool) -> Tokens {
        let accent = accent.for_theme(theme);
        let opaque = |colors: [u32; 7]| colors.map(hex);
        let [chrome, content, raised, field, line, text, sub] = match (theme, flat) {
            (Theme::Graphite, false) => opaque([
                0x131416, 0x1D1F22, 0x2A2D31, 0x1D1F22, 0x3A3E44, 0xEEF0F2, 0xA3A8AE,
            ]),
            (Theme::Graphite, true) => opaque([
                0x151515, 0x151515, 0x252525, 0x151515, 0x2A2A2A, 0xEEF0F2, 0xA3A8AE,
            ]),
            (Theme::Charcoal, false) => opaque([
                0x151515, 0x1C1C1C, 0x272727, 0x202020, 0x2E2E2E, 0xEDEDED, 0xA3A3A3,
            ]),
            (Theme::Charcoal, true) => opaque([
                0x151515, 0x151515, 0x242424, 0x202020, 0x2E2E2E, 0xEDEDED, 0xA3A3A3,
            ]),
            (Theme::Mocha, false) => opaque([
                0x181825, 0x1E1E2E, 0x2B2C3E, 0x232334, 0x313244, 0xCDD6F4, 0xA6ADC8,
            ]),
            (Theme::Mocha, true) => opaque([
                0x181825, 0x181825, 0x2A2B3C, 0x232334, 0x313244, 0xCDD6F4, 0xA6ADC8,
            ]),
            // One base, flat Graphite's #151515, at two strengths: the chrome
            // lets 45 % of the blurred desktop through, the content 28 %;
            // flat gives the content the chrome's glass, as flat does in
            // every theme. The grays are white veils, so they read the same
            // over any wallpaper. Fixed, no slider (decided 2026-10-25).
            (Theme::Glass, flat) => {
                let base = |alpha| Color {
                    a: alpha,
                    ..hex(0x151515)
                };
                [
                    base(0.55),
                    base(if flat { 0.55 } else { 0.72 }),
                    white(0.08),
                    white(0.05),
                    white(0.14),
                    hex(0xEEF0F2),
                    hex(0xB8BDC3),
                ]
            }
            (Theme::Light, false) => opaque([
                0xF4F5F7, 0xFFFFFF, 0xE9EBEE, 0xFFFFFF, 0xD5D9DE, 0x17181B, 0x5B616B,
            ]),
            (Theme::Light, true) => opaque([
                0xFFFFFF, 0xFFFFFF, 0xE9EBEE, 0xFFFFFF, 0xD5D9DE, 0x17181B, 0x5B616B,
            ]),
        };
        let light = theme.is_light();
        let pick = |on_light: Color, on_dark: Color| if light { on_light } else { on_dark };
        // On Glass a shade mixed from the see-through content would be
        // opaque, so the fills are white veils at the mockup's strengths.
        let glass = theme.is_glass();
        let veil = |alpha: f32, mixed: Color| if glass { white(alpha) } else { mixed };
        let faint = |color: Color| color.scale_alpha(0.08);
        let success = pick(hex(0x40A02B), hex(0xA6E3A1));
        let warning = pick(hex(0xA36706), hex(0xF2B544));
        let danger = pick(hex(0xD7263D), hex(0xF0525F));
        Tokens {
            theme,
            light,
            chrome,
            content,
            raised,
            field,
            line,
            text,
            sub,
            selection: pick(hex(0xE9EBEE), veil(0.13, mix_white(content, 0.14))),
            hover: pick(hex(0xF1F2F4), veil(0.07, mix_white(content, 0.07))),
            track: pick(hex(0xE3E5E9), veil(0.14, mix_white(content, 0.12))),
            // Popovers and menus stay solid on Glass, so they read over the
            // glass they open on.
            popover: pick(hex(0xFFFFFF), if glass { hex(0x26292D) } else { raised }),
            dialog: if glass {
                hex(if flat { 0x151515 } else { 0x1D1F22 })
            } else {
                content
            },
            dialog_chrome: if glass {
                hex(if flat { 0x151515 } else { 0x131416 })
            } else {
                chrome
            },
            danger,
            success,
            success_fill: faint(success),
            warning,
            warning_fill: pick(hex(0xFDEBD9), hex(0xF2B544).scale_alpha(0.16)),
            danger_fill: faint(danger),
            scrim: pick(
                Color::from_rgba8(20, 22, 28, 0.32),
                Color::from_rgba8(8, 9, 10, 0.62),
            ),
            shadow: pick(
                Color::from_rgba8(20, 22, 28, 0.18),
                Color::from_rgba8(0, 0, 0, 0.6),
            ),
            rail_thumb: pick(
                Color::from_rgba8(20, 22, 28, 0.75),
                Color::from_rgba(1.0, 1.0, 1.0, 0.9),
            ),
            accent: accent.fill,
            ink: pick(accent.ink, accent.fill),
            on_accent: pick(hex(0x17181B), hex(0x131416)),
        }
    }

    /// A type's color, for section headings, dots and labels: the Libri
    /// set, the Catppuccin set with Catppuccin Mocha, and the light inks
    /// (4.5:1 on white, except Movies, kept gold on purpose) with Light.
    pub fn type_color(&self, media: MediaType) -> Color {
        let (libri, catppuccin, light) = match media {
            MediaType::Movies => (0xF2B544, 0xF9E2AF, 0xA87C00),
            MediaType::Shows => (0x7FB7FF, 0x89B4FA, 0x0074DC),
            MediaType::Books => (0x8FD694, 0xA6E3A1, 0x01872C),
            MediaType::Games => (0xFF8F7A, 0xFAB387, 0xD63A1F),
            MediaType::Anime => (0xFF8CC1, 0xF5C2E7, 0xC8157E),
            MediaType::Music => (0xC4A7FF, 0xCBA6F7, 0x7C3AED),
        };
        hex(match self.theme {
            Theme::Graphite | Theme::Charcoal | Theme::Glass => libri,
            Theme::Mocha => catppuccin,
            Theme::Light => light,
        })
    }

    /// The iced theme behind the custom styles: the window background and
    /// the defaults for widgets that are not styled by hand yet. Glass
    /// clears the window to nothing: each screen paints its own surfaces at
    /// their see-through strengths (the shell lays the chrome under the
    /// sidebar alone, not under the content), and KWin blurs the desktop
    /// behind them (`decoration`).
    pub fn iced_theme(&self) -> iced::Theme {
        iced::Theme::custom(
            "Libri",
            Palette {
                background: if self.theme.is_glass() {
                    Color::TRANSPARENT
                } else {
                    self.chrome
                },
                text: self.text,
                primary: self.accent,
                success: hex(0x5EE08A),
                warning: hex(0xF2B544),
                danger: self.danger,
            },
        )
    }
}

/// Mixes `color` toward white by `amount` (0 to 1), as the mockup's
/// `mixWhite` does. The alpha is kept, so a see-through surface stays one.
pub fn mix_white(color: Color, amount: f32) -> Color {
    let mix = |c: f32| ((c * 255.0) + (255.0 - c * 255.0) * amount).round() / 255.0;
    Color::from_rgba(mix(color.r), mix(color.g), mix(color.b), color.a)
}

/// The surface of plan cards and card-style sections: white on Light, a
/// faint white veil on Glass, a touch lighter than the content otherwise.
pub fn card_background(tokens: &Tokens) -> Color {
    if tokens.light {
        Color::WHITE
    } else if tokens.theme.is_glass() {
        white(0.04)
    } else {
        mix_white(tokens.content, 0.035)
    }
}

/// Figtree only. Headings are ExtraBold, body Regular, labels and buttons
/// SemiBold.
pub mod font {
    use super::*;

    pub const REGULAR: Font = Font::with_name("Figtree");
    pub const ITALIC: Font = Font {
        style: Style::Italic,
        ..REGULAR
    };
    pub const MEDIUM: Font = Font {
        weight: Weight::Medium,
        ..REGULAR
    };
    pub const SEMIBOLD: Font = Font {
        weight: Weight::Semibold,
        ..REGULAR
    };
    pub const BOLD: Font = Font {
        weight: Weight::Bold,
        ..REGULAR
    };
    pub const EXTRABOLD: Font = Font {
        weight: Weight::ExtraBold,
        ..REGULAR
    };
    /// Years, counts and dates.
    pub const MONO: Font = Font::with_name("JetBrains Mono");
    pub const MONO_MEDIUM: Font = Font {
        weight: Weight::Medium,
        ..MONO
    };

    /// The font files, loaded at startup.
    pub const FILES: [&[u8]; 8] = [
        include_bytes!("../assets/fonts/figtree/Figtree-Regular.ttf"),
        include_bytes!("../assets/fonts/figtree/Figtree-Italic.ttf"),
        include_bytes!("../assets/fonts/figtree/Figtree-Medium.ttf"),
        include_bytes!("../assets/fonts/figtree/Figtree-SemiBold.ttf"),
        include_bytes!("../assets/fonts/figtree/Figtree-Bold.ttf"),
        include_bytes!("../assets/fonts/figtree/Figtree-ExtraBold.ttf"),
        include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"),
        include_bytes!("../assets/fonts/jetbrains-mono/JetBrainsMono-Medium.ttf"),
    ];
}

/// Text sizes in logical pixels.
pub mod size {
    /// Screen titles and dialog titles.
    pub const HEADING: f32 = 26.0;
    pub const TITLE: f32 = 18.0;
    pub const BODY: f32 = 14.0;
    pub const SMALL: f32 = 13.0;
    /// Section headers in small gray caps.
    pub const SECTION: f32 = 12.5;
    pub const MONO: f32 = 12.0;
}

/// WCAG 2 contrast between two colors, 1 to 21.
pub fn contrast(a: Color, b: Color) -> f32 {
    fn luminance(color: Color) -> f32 {
        let channel = |c: f32| {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    }
    let (light, dark) = {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b), a.min(b))
    };
    (light + 0.05) / (dark + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `color` laid over `backdrop`: what a see-through surface shows.
    fn over(color: Color, backdrop: Color) -> Color {
        let lay = |c: f32, b: f32| c * color.a + b * (1.0 - color.a);
        Color::from_rgb(
            lay(color.r, backdrop.r),
            lay(color.g, backdrop.g),
            lay(color.b, backdrop.b),
        )
    }

    /// The contrast check of phase 1g (docs/DESIGN.md section 10): body
    /// and secondary text reach AA (4.5), the accent inks and the type
    /// colors, used for labels and headings in SemiBold or larger, reach
    /// 3, on the content and the chrome of every theme, two-tone and flat.
    /// Glass is see-through, so its surfaces are checked over a mid gray,
    /// about what a blurred wallpaper averages; over white the body text
    /// would fall to 3.5 on the chrome, the price of glass.
    #[test]
    fn inks_and_type_colors_read_on_every_theme() {
        let mut table = String::new();
        let mut failures = Vec::new();
        let mut check = |what: String, fg: Color, bg: Color, least: f32| {
            let ratio = contrast(fg, bg);
            table.push_str(&format!("{what}: {ratio:.2}\n"));
            if ratio < least {
                failures.push(format!("{what} is {ratio:.2}, under {least}"));
            }
        };
        for theme in Theme::ALL {
            for flat in [false, true] {
                let tokens = Tokens::styled(theme, PEACH, flat);
                let name = format!("{}{}", theme.label(), if flat { " flat" } else { "" });
                for (surface, bg) in [("content", tokens.content), ("chrome", tokens.chrome)] {
                    let bg = over(bg, hex(0x808080));
                    check(format!("{name} text on {surface}"), tokens.text, bg, 4.5);
                    check(format!("{name} sub on {surface}"), tokens.sub, bg, 4.5);
                    for media in MediaType::ALL {
                        let what = format!("{name} {} on {surface}", media.label());
                        check(what, tokens.type_color(media), bg, 3.0);
                    }
                    for accent in VIVID.iter().chain(&CATPPUCCIN) {
                        let ink = Tokens::styled(theme, *accent, flat).ink;
                        check(
                            format!("{name} {} ink on {surface}", accent.name),
                            ink,
                            bg,
                            3.0,
                        );
                    }
                }
                for accent in VIVID.iter().chain(&CATPPUCCIN) {
                    let t = Tokens::styled(theme, *accent, flat);
                    check(
                        format!("{name} text on {} fill", accent.name),
                        t.on_accent,
                        t.accent,
                        4.5,
                    );
                }
            }
        }
        println!("{table}");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Every dialog surface is opaque, the popovers too: a see-through
    /// dialog would show the screen it stands on, and a picker over the
    /// form would show the form.
    #[test]
    fn dialogs_and_popovers_are_opaque_on_every_theme() {
        for theme in Theme::ALL {
            for flat in [false, true] {
                let tokens = Tokens::styled(theme, PEACH, flat);
                for (name, color) in [
                    ("dialog", tokens.dialog),
                    ("dialog chrome", tokens.dialog_chrome),
                    ("popover", tokens.popover),
                ] {
                    assert_eq!(color.a, 1.0, "{} {name} (flat: {flat})", theme.label());
                }
            }
        }
    }

    #[test]
    fn selection_gray_matches_the_mockup() {
        // mixWhite("#1D1F22", 0.14) in the mockup.
        let graphite = Tokens::new(Theme::Graphite, PEACH);
        assert_eq!(graphite.selection.into_rgba8(), [0x3D, 0x3E, 0x41, 0xFF]);
        assert_eq!(Tokens::new(Theme::Light, PEACH).selection, hex(0xE9EBEE));
    }

    #[test]
    fn flat_uses_one_shade_for_chrome_and_content() {
        let flat = Tokens::styled(Theme::Graphite, PEACH, true);
        assert_eq!(flat.chrome, flat.content);
        assert_eq!(flat.chrome, hex(0x151515));
        assert_eq!(flat.raised, hex(0x252525));
        // mixWhite("#151515", 0.14).
        assert_eq!(flat.selection.into_rgba8(), [0x36, 0x36, 0x36, 0xFF]);
        let light = Tokens::styled(Theme::Light, PEACH, true);
        assert_eq!(light.chrome, Color::WHITE);
        assert_ne!(Tokens::new(Theme::Light, PEACH).chrome, Color::WHITE);
    }

    #[test]
    fn glass_is_one_shade_at_two_strengths_with_white_veils() {
        let glass = Tokens::new(Theme::Glass, PEACH);
        let base = |alpha| Color {
            a: alpha,
            ..hex(0x151515)
        };
        assert_eq!(glass.chrome, base(0.55));
        assert_eq!(glass.content, base(0.72));
        assert_eq!(
            Tokens::styled(Theme::Glass, PEACH, true).content,
            base(0.55)
        );
        assert_eq!(glass.raised, white(0.08));
        assert_eq!(glass.selection, white(0.13));
        assert_eq!(glass.hover, white(0.07));
        assert_eq!(card_background(&glass), white(0.04));
        assert_eq!(glass.popover, hex(0x26292D));
        assert_eq!(glass.type_color(MediaType::Movies), hex(0xF2B544));
        assert!(!glass.light);
        // The window clears to nothing; the shell paints the surfaces.
        assert_eq!(glass.iced_theme().palette().background, Color::TRANSPARENT);
        assert_eq!(
            Tokens::new(Theme::Graphite, PEACH)
                .iced_theme()
                .palette()
                .background,
            hex(0x131416)
        );
        // A mix keeps the surface see-through.
        assert_eq!(mix_white(glass.content, 0.1).a, 0.72);
    }

    #[test]
    fn the_ink_is_the_fill_on_dark_and_the_darker_ink_on_light() {
        assert_eq!(Tokens::new(Theme::Graphite, PEACH).ink, PEACH.fill);
        assert_eq!(Tokens::new(Theme::Light, PEACH).ink, PEACH.ink);
    }

    #[test]
    fn catppuccin_mocha_brings_its_own_type_colors() {
        let movies = |theme| Tokens::new(theme, PEACH).type_color(MediaType::Movies);
        assert_eq!(movies(Theme::Graphite), hex(0xF2B544));
        assert_eq!(movies(Theme::Charcoal), hex(0xF2B544));
        assert_eq!(movies(Theme::Glass), hex(0xF2B544));
        assert_eq!(movies(Theme::Mocha), hex(0xF9E2AF));
        assert_eq!(movies(Theme::Light), hex(0xA87C00));
    }

    #[test]
    fn catppuccin_mocha_turns_the_default_peach_into_catppuccin_peach() {
        let mocha = Tokens::new(Theme::Mocha, PEACH);
        assert_eq!((mocha.accent, mocha.ink), (hex(0xFAB387), hex(0xFAB387)));
        assert_eq!(Tokens::new(Theme::Graphite, PEACH).accent, PEACH.fill);
        assert_eq!(Tokens::new(Theme::Mocha, VIVID[1]).accent, VIVID[1].fill);
    }

    #[test]
    fn cover_colors_match_the_mockups_cover_of() {
        // coverOf("m1") in the canvas: (109*3 + 49*4) % 8 = 3.
        assert_eq!(cover_color("m1"), hex(0x6B4B8A));
    }
}
