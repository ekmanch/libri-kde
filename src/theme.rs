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

/// An accent color: the value the dark themes draw, and the stronger
/// shade the light theme draws in its place. The stored setting is the
/// accent's name, the same on every theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accent {
    pub name: &'static str,
    pub fill: Color,
    pub light: Color,
}

impl Accent {
    /// The color `theme` draws for this accent: the light value on Light,
    /// and with Catppuccin Mocha the default Peach becomes Catppuccin's own
    /// peach. The accent picker shows this, so its swatches follow the
    /// theme.
    pub fn shown(self, theme: Theme) -> Color {
        match theme {
            Theme::Light => self.light,
            Theme::Mocha if self == PEACH => hex(0xFAB387),
            _ => self.fill,
        }
    }
}

const fn accent(name: &'static str, fill: u32, light: u32) -> Accent {
    Accent {
        name,
        fill: hex(fill),
        light: hex(light),
    }
}

pub const VIVID: [Accent; 6] = [
    accent("Peach", 0xF8B289, 0xFF8A4C),
    accent("Violet", 0xA78BFA, 0x8B5CF6),
    accent("Hot pink", 0xFF7AB6, 0xF74F9E),
    accent("Amber", 0xF2B544, 0xFFB300),
    accent("Green", 0x5EE08A, 0x34C759),
    accent("Blue", 0x5AA9FF, 0x0A84FF),
];

pub const CATPPUCCIN: [Accent; 10] = [
    // The light values are Catppuccin Latte's.
    accent("Beige", 0xF9E2AF, 0xDF8E1D),
    accent("Rosewater", 0xF5E0DC, 0xDC8A78),
    accent("Flamingo", 0xF2CDCD, 0xDD7878),
    accent("Pink", 0xF5C2E7, 0xEA76CB),
    accent("Red", 0xF38BA8, 0xD20F39),
    accent("Mauve", 0xCBA6F7, 0x8839EF),
    accent("Lavender", 0xB4BEFE, 0x7287FD),
    accent("Blue", 0x89B4FA, 0x1E66F5),
    accent("Sky", 0x89DCEB, 0x04A5E5),
    accent("Green", 0xA6E3A1, 0x40A02B),
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

/// The dark pill on a cover of the owner's own ("Yours") and the chip
/// across a cover's foot ("Not 2:3 · Adjust"), the same on every theme
/// since they sit on an image: near-black, mostly opaque, with the
/// viewer's light text.
pub const COVER_BADGE: Color = Color::from_rgba8(0x13, 0x14, 0x16, 0.88);
/// The dim over the part of a picture that Fit your cover cuts away, and
/// the frame around the part it keeps.
pub const FIT_DIM: Color = Color::from_rgba8(0x08, 0x09, 0x0A, 0.6);
pub const FIT_FRAME: Color = hex(0xEEF0F2);

/// The cover viewer, dark on every theme as in the mockup: its text and
/// the second line.
pub const VIEWER_TEXT: Color = hex(0xEEF0F2);
pub const VIEWER_SUB: Color = hex(0xA3A8AE);
/// Its round buttons (×, ‹ and ›) and their hover: a gray that hides
/// most of what is behind, where the mockup has white veils, which let
/// a colorful cover behind the backdrop through at full strength.
pub const VIEWER_ROUND: Color = Color::from_rgba8(0x3A, 0x3D, 0x42, 0.65);
pub const VIEWER_ROUND_HOVER: Color = Color::from_rgba8(0x56, 0x5A, 0x60, 0.65);
/// Change cover…, opaque where the mockup has white veils, since the
/// library's covers show through the backdrop behind it: its fill (the
/// solid surface Glass gives its dialogs), hover and outline.
pub const VIEWER_BUTTON: Color = hex(0x26292D);
pub const VIEWER_BUTTON_HOVER: Color = hex(0x32353A);
pub const VIEWER_BUTTON_LINE: Color = hex(0x4D5053);

/// A mid gray, about what a blurred wallpaper averages: behind Glass in
/// the Settings theme card, and what the contrast check lays under it.
pub const WALLPAPER: Color = hex(0x808080);

/// How much of the screen the cover viewer's backdrop lets through: a
/// tenth, where the mockup has 18 %, at which a list's text behind the
/// title under the cover ran through it.
const VIEWER_SHOWS: f32 = 0.10;

/// The cover viewer's backdrop over the dialog scrim `under`, so the two
/// together are a near-black that hides all but `VIEWER_SHOWS` of the
/// screen.
pub fn viewer_scrim(under: Color) -> Color {
    let alpha = 1.0 - VIEWER_SHOWS / (1.0 - under.a).max(VIEWER_SHOWS);
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
    /// The frame of a group of setting rows and the lines between them
    /// (`ui::setting_rows`): the divider on Light, and on the dark themes a
    /// step closer to the background, since lines read heavier on dark.
    pub group_line: Color,
    pub text: Color,
    /// Secondary text and section headers.
    pub sub: Color,
    /// The gray of a selection: segmented controls, the view switch, a
    /// picked status in the details. Never the accent.
    pub selection: Color,
    /// A hovered row, and a selected row of a list.
    pub hover: Color,
    /// Progress and slider tracks.
    pub track: Color,
    /// Popovers and menus.
    pub popover: Color,
    /// The picked answer of a pick-one dialog (the music overlap
    /// question): a step lighter than the popover, solid on Glass too.
    pub picked: Color,
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
    /// Behind the grouped list's pinned type header: the content pane,
    /// and on Glass nearly solid, so covers never show through it.
    pub pinned: Color,
    /// The unpinned details panel, a drawer over the list: the content
    /// pane, and on Glass its base #151515 fully opaque, with a shadow to
    /// its left.
    pub drawer: Color,
    pub drawer_shadow: Color,
    /// The accent as this theme draws it. On Light it is for thin fills
    /// alone: progress bars, toggles and focus outlines.
    pub accent: Color,
    /// Links and icons that stand out from the text ("See all", "Clear
    /// all", a lone check mark): the accent on the dark themes, a dark gray
    /// on Light, where no text is colored.
    pub ink: Color,
    /// The "#" of a tag in the sidebar: the accent on the dark themes, a
    /// mid gray on Light.
    pub mark: Color,
    /// The fill of a main button (Add, Save), the Filter button's count,
    /// the index rail's current label and the picked rating: the accent
    /// on the dark themes, a pale tint of it on Light.
    pub tint: Color,
    /// The fill of what is picked among its like: the sidebar's row, the
    /// Following window's kind, an active filter chip. The selection gray
    /// on the dark themes, a pale tint of the accent on Light. The picked
    /// status chip is `status_fill`.
    pub selection_tint: Color,
    /// Text on an accent fill or on a tint.
    pub on_accent: Color,
    /// The green of a connected state, and its faint card fill (Light
    /// alone fills the Connected row; see `connected_frame`).
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
        let accent = accent.shown(theme);
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
                0xF4F5F7, 0xFFFFFF, 0xEEF0F2, 0xFFFFFF, 0xE1E4E8, 0x17181B, 0x5B616B,
            ]),
            (Theme::Light, true) => opaque([
                0xFFFFFF, 0xFFFFFF, 0xEEF0F2, 0xFFFFFF, 0xE1E4E8, 0x17181B, 0x5B616B,
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
        // Popovers and menus stay solid on Glass, so they read over the
        // glass they open on.
        let popover = pick(hex(0xFFFFFF), if glass { hex(0x26292D) } else { raised });
        let selection = pick(hex(0xECEEF0), veil(0.13, mix_white(content, 0.14)));
        Tokens {
            theme,
            light,
            chrome,
            content,
            raised,
            field,
            line,
            group_line: pick(line, veil(0.08, mix_white(content, 0.07))),
            text,
            sub,
            selection,
            hover: pick(hex(0xF4F5F7), veil(0.07, mix_white(content, 0.07))),
            track: pick(hex(0xE3E5E9), veil(0.14, mix_white(content, 0.12))),
            popover,
            picked: pick(hex(0xF4F5F7), mix_white(popover, 0.06)),
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
            pinned: if glass {
                Color {
                    a: 0.94,
                    ..hex(0x151515)
                }
            } else {
                content
            },
            // KWin blurs what is behind the window, never Libri's own
            // list, so a see-through drawer would show the covers under it.
            drawer: if glass { hex(0x151515) } else { content },
            drawer_shadow: pick(
                Color::from_rgba8(20, 22, 28, 0.16),
                Color::from_rgba8(0, 0, 0, 0.55),
            ),
            accent,
            ink: accent,
            mark: accent,
            tint: accent,
            selection_tint: selection,
            on_accent: pick(hex(0x17181B), hex(0x131416)),
        }
        .with_accent(accent)
    }

    /// These tokens with another accent: a seasonal plan draws in its
    /// season's color. On Light the colors are fills and the text is
    /// neutral (docs/DESIGN.md section 8), so only the fills follow the
    /// accent there: the links and icons are a dark gray, the "#" marks a
    /// mid gray, and buttons and picked items take a pale tint of it.
    pub fn with_accent(self, accent: Color) -> Tokens {
        let (ink, mark, tint, selection_tint) = if self.light {
            (
                hex(0x2F343B),
                hex(0x9AA0A8),
                mix_white(accent, 0.78),
                mix_white(accent, 0.82),
            )
        } else {
            (accent, accent, accent, self.selection)
        };
        Tokens {
            accent,
            ink,
            mark,
            tint,
            selection_tint,
            ..self
        }
    }

    /// A main button's fill under the pointer: a little of what is behind
    /// it on the dark themes, a stronger tint on Light, where a paler one
    /// would fade into the page.
    pub fn tint_hover(&self) -> Color {
        if self.light {
            mix_white(self.accent, 0.70)
        } else {
            self.tint.scale_alpha(0.85)
        }
    }

    /// The picked status chip's fill (All, Want, Done): Light's pale tint
    /// of the accent, and on the dark themes the solid accent, as the Add
    /// button. Never a low-opacity tint there: it turns an orange accent
    /// brown on dark (canvas, 2026-11-21).
    pub fn status_fill(&self) -> Color {
        if self.light {
            self.selection_tint
        } else {
            self.tint
        }
    }

    /// The text on the picked status chip.
    pub fn on_status(&self) -> Color {
        if self.light {
            self.text
        } else {
            self.on_accent
        }
    }

    /// The edge of a ticked check box, whose fill is `tint` with the
    /// `on_accent` check: the accent on the dark themes, and on Light a
    /// stronger tint than the fill (45 % of the accent), so the box
    /// matches a main button and still has an edge on white.
    pub fn tick_line(&self) -> Color {
        if self.light {
            mix_white(self.accent, 0.55)
        } else {
            self.accent
        }
    }

    /// The frame of Settings' Connected row on the dark themes, where it
    /// has no fill and its title is in the green: the green at 70 %.
    /// None on Light, which keeps the pale green fill.
    pub fn connected_frame(&self) -> Option<Color> {
        (!self.light).then(|| self.success.scale_alpha(0.7))
    }

    /// A type's color, for dots, bars and marks: the Libri set, the
    /// Catppuccin set with Catppuccin Mocha, and a brighter set with
    /// Light, where it is never the color of text (`type_text`).
    pub fn type_color(&self, media: MediaType) -> Color {
        let (libri, catppuccin, light) = match media {
            MediaType::Movies => (0xF2B544, 0xF9E2AF, 0xFFC400),
            MediaType::Shows => (0x7FB7FF, 0x89B4FA, 0x0A84FF),
            MediaType::Books => (0x8FD694, 0xA6E3A1, 0x34C759),
            MediaType::Games => (0xFF8F7A, 0xFAB387, 0xFF5A4F),
            MediaType::Anime => (0xFF8CC1, 0xF5C2E7, 0xF74F9E),
            MediaType::Music => (0xC4A7FF, 0xCBA6F7, 0xAF52DE),
        };
        hex(match self.theme {
            Theme::Graphite | Theme::Charcoal | Theme::Glass => libri,
            Theme::Mocha => catppuccin,
            Theme::Light => light,
        })
    }

    /// The color of text that names a type: the page heading, "Movie"
    /// above a title, a group's heading. The type's color on the dark
    /// themes, the normal text color on Light.
    pub fn type_text(&self, media: MediaType) -> Color {
        if self.light {
            self.text
        } else {
            self.type_color(media)
        }
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

/// The hairline just inside a cover's edge on Light, so a white cover
/// does not dissolve into the page; nothing on the dark themes. Asked of
/// the iced theme (`Tokens::iced_theme`), which every widget is handed as
/// it is drawn, so a cover needs no tokens to draw it.
pub fn cover_edge(theme: &iced::Theme) -> Option<Color> {
    (!theme.extended_palette().is_dark).then_some(Color::from_rgba8(20, 22, 28, 0.10))
}

/// The color of the soft shadow under a cover on Light (canvas "Light
/// theme", 2026-11-18); `ui::cover` holds its two sizes.
pub const COVER_SHADOW: Color = Color::from_rgba8(20, 22, 28, 0.13);

/// Whether covers cast that shadow: on Light only. Asked of the iced
/// theme, as `cover_edge` is.
pub fn lifts_covers(theme: &iced::Theme) -> bool {
    !theme.extended_palette().is_dark
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
    /// The type headers of a grouped list.
    pub const GROUP: f32 = 24.0;
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

    /// The viewer's backdrop and the dialog scrim under it together let
    /// a tenth of the screen through, on every theme.
    #[test]
    fn the_cover_viewers_backdrop_hides_nine_tenths() {
        for theme in Theme::ALL {
            let under = Tokens::styled(theme, PEACH, false).scrim;
            let shows = (1.0 - under.a) * (1.0 - viewer_scrim(under).a);
            assert!((shows - VIEWER_SHOWS).abs() < 1e-4, "{theme:?}: {shows}");
        }
    }

    /// The viewer's round buttons stand out from the backdrop over a
    /// black screen and over a white one, and the screen's own contrast
    /// behind them drops to a third or less.
    #[test]
    fn the_cover_viewers_round_buttons_read_over_any_screen() {
        let backdrop = |screen: Color| {
            let under = Tokens::styled(Theme::Graphite, PEACH, false).scrim;
            over(viewer_scrim(under), over(under, screen))
        };
        let (dark, bright) = (backdrop(Color::BLACK), backdrop(Color::WHITE));
        for fill in [VIEWER_ROUND, VIEWER_ROUND_HOVER] {
            assert!(fill.a < 1.0 && fill.a >= 0.6);
            for behind in [dark, bright] {
                let disc = over(fill, behind);
                assert!(disc.r - behind.r > 0.05, "{disc:?} on {behind:?}");
                assert!(contrast(VIEWER_TEXT, disc) >= 4.5);
            }
        }
        let spread = over(VIEWER_ROUND, bright).r - over(VIEWER_ROUND, dark).r;
        assert!(spread <= (bright.r - dark.r) * 0.4);
    }

    /// Change cover… sits on a see-through backdrop with the library's
    /// covers behind it, so nothing of it lets them through, and its
    /// label reads on it.
    #[test]
    fn the_cover_viewers_button_is_opaque() {
        for color in [VIEWER_BUTTON, VIEWER_BUTTON_HOVER, VIEWER_BUTTON_LINE] {
            assert_eq!(color.a, 1.0);
        }
        assert!(contrast(VIEWER_TEXT, VIEWER_BUTTON) >= 4.5);
        assert!(contrast(VIEWER_TEXT, VIEWER_BUTTON_HOVER) >= 4.5);
    }

    /// The contrast check of phase 1g (docs/DESIGN.md section 10): body
    /// and secondary text reach AA (4.5), the inks and the text that names
    /// a type, used for labels and headings in SemiBold or larger, reach
    /// 3, on the content and the chrome of every theme, two-tone and flat,
    /// and the text on a main button, on a picked chip and on a selected
    /// sidebar row, and the check in a ticked box, reach 4.5.
    /// Light's own type colors and accents are fills, never text, so they
    /// are not checked. Glass is see-through, so its surfaces are checked
    /// over a mid gray, about what a blurred wallpaper averages; over white
    /// the body text would fall to 3.5 on the chrome, the price of glass.
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
                    let bg = over(bg, WALLPAPER);
                    check(format!("{name} text on {surface}"), tokens.text, bg, 4.5);
                    check(format!("{name} sub on {surface}"), tokens.sub, bg, 4.5);
                    for media in MediaType::ALL {
                        let what = format!("{name} {} on {surface}", media.label());
                        check(what, tokens.type_text(media), bg, 3.0);
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
                        format!("{name} text on {} button", accent.name),
                        t.on_accent,
                        t.tint,
                        4.5,
                    );
                    check(
                        format!("{name} text on {} chip", accent.name),
                        t.on_status(),
                        over(t.status_fill(), over(t.content, WALLPAPER)),
                        4.5,
                    );
                    check(
                        format!("{name} text on {} sidebar row", accent.name),
                        t.text,
                        over(t.selection_tint, over(t.chrome, WALLPAPER)),
                        4.5,
                    );
                    check(
                        format!("{name} check in {} box", accent.name),
                        t.on_accent,
                        t.tint,
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
        assert_eq!(Tokens::new(Theme::Light, PEACH).selection, hex(0xECEEF0));
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
        assert_eq!(glass.group_line, white(0.08));
        assert_eq!(card_background(&glass), white(0.04));
        assert_eq!(glass.popover, hex(0x26292D));
        assert_eq!(glass.pinned, base(0.94));
        assert_eq!(glass.drawer, hex(0x151515), "opaque over the list");
        assert_eq!(Tokens::new(Theme::Mocha, PEACH).pinned, hex(0x1E1E2E));
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

    /// The dark themes draw everything in the accent itself; they did
    /// not change with Light's rework.
    #[test]
    fn the_dark_themes_draw_the_accent_itself() {
        for theme in [Theme::Graphite, Theme::Charcoal, Theme::Glass] {
            let t = Tokens::new(theme, PEACH);
            assert_eq!([t.accent, t.ink, t.mark, t.tint], [PEACH.fill; 4]);
            assert_eq!(t.selection_tint, t.selection);
            // The picked status chip is the Add button's colors, and a
            // ticked box is the accent with its own edge (2026-11-21).
            assert_eq!([t.status_fill(), t.tick_line()], [PEACH.fill; 2]);
            assert_eq!(t.on_status(), t.on_accent);
            assert_eq!(t.status_fill().a, 1.0);
            // Connected: a green frame at 70 %, no fill.
            assert_eq!(
                t.connected_frame(),
                Some(Color::from_rgba8(0xA6, 0xE3, 0xA1, 0.7))
            );
            assert_eq!(cover_edge(&t.iced_theme()), None);
            assert_eq!(t.type_text(MediaType::Books), hex(0x8FD694));
        }
    }

    /// Light (2026-11-18): colors are fills and text is neutral. Each
    /// accent has its own value there, links and icons are a dark gray,
    /// the "#" marks a mid gray, and buttons and picked items take a pale
    /// tint of the accent, as the mockup mixes them (`mixWhite` at 0.78
    /// and 0.82).
    #[test]
    fn light_draws_colors_as_fills_and_text_neutral() {
        let light = Tokens::new(Theme::Light, PEACH);
        assert_eq!(light.accent, hex(0xFF8A4C));
        assert_eq!(light.ink, hex(0x2F343B));
        assert_eq!(light.mark, hex(0x9AA0A8));
        assert_eq!(light.tint.into_rgba8(), [0xFF, 0xE5, 0xD8, 0xFF]);
        assert_eq!(light.selection_tint.into_rgba8(), [0xFF, 0xEA, 0xDF, 0xFF]);
        assert_eq!(light.on_accent, light.text);
        // The picked status chip keeps the tint; a ticked box is the main
        // button's tint with a stronger one as its edge (`mixWhite` at
        // 0.55); Connected keeps its fill.
        assert_eq!(light.status_fill(), light.selection_tint);
        assert_eq!(light.on_status(), light.text);
        assert_eq!(light.tick_line(), mix_white(hex(0xFF8A4C), 0.55));
        assert_eq!(light.connected_frame(), None);
        assert_eq!(
            [light.line, light.raised, light.selection, light.hover],
            [hex(0xE1E4E8), hex(0xEEF0F2), hex(0xECEEF0), hex(0xF4F5F7)]
        );
        assert_eq!(
            cover_edge(&light.iced_theme()),
            Some(Color::from_rgba8(20, 22, 28, 0.10))
        );
        for theme in [Theme::Mocha, Theme::Glass] {
            assert_eq!(cover_edge(&Tokens::new(theme, PEACH).iced_theme()), None);
        }
        for media in MediaType::ALL {
            assert_eq!(light.type_text(media), light.text);
        }
        assert_eq!(light.type_color(MediaType::Games), hex(0xFF5A4F));
        let latte = Tokens::new(Theme::Light, CATPPUCCIN[8]);
        assert_eq!((latte.accent, latte.ink), (hex(0x04A5E5), hex(0x2F343B)));
    }

    /// A seasonal plan's color replaces the accent: everything on the
    /// dark themes, the fills and the tints alone on Light.
    #[test]
    fn a_seasons_color_takes_the_accents_place() {
        let dark = Tokens::new(Theme::Graphite, PEACH).with_accent(HALLOWEEN);
        assert_eq!([dark.accent, dark.ink, dark.tint], [HALLOWEEN; 3]);
        let light = Tokens::new(Theme::Light, PEACH).with_accent(HALLOWEEN);
        assert_eq!(light.accent, HALLOWEEN);
        assert_eq!(light.ink, hex(0x2F343B));
        assert_eq!(light.tint, mix_white(HALLOWEEN, 0.78));
        assert_eq!(light.selection_tint, mix_white(HALLOWEEN, 0.82));
    }

    #[test]
    fn catppuccin_mocha_brings_its_own_type_colors() {
        let movies = |theme| Tokens::new(theme, PEACH).type_color(MediaType::Movies);
        assert_eq!(movies(Theme::Graphite), hex(0xF2B544));
        assert_eq!(movies(Theme::Charcoal), hex(0xF2B544));
        assert_eq!(movies(Theme::Glass), hex(0xF2B544));
        assert_eq!(movies(Theme::Mocha), hex(0xF9E2AF));
        assert_eq!(movies(Theme::Light), hex(0xFFC400));
    }

    #[test]
    fn catppuccin_mocha_turns_the_default_peach_into_catppuccin_peach() {
        let mocha = Tokens::new(Theme::Mocha, PEACH);
        assert_eq!((mocha.accent, mocha.ink), (hex(0xFAB387), hex(0xFAB387)));
        assert_eq!(PEACH.shown(Theme::Light), hex(0xFF8A4C));
        assert_eq!(Tokens::new(Theme::Graphite, PEACH).accent, PEACH.fill);
        assert_eq!(Tokens::new(Theme::Mocha, VIVID[1]).accent, VIVID[1].fill);
    }

    #[test]
    fn cover_colors_match_the_mockups_cover_of() {
        // coverOf("m1") in the canvas: (109*3 + 49*4) % 8 = 3.
        assert_eq!(cover_color("m1"), hex(0x6B4B8A));
    }
}
