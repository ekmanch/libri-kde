//! A cover or an artist photo wherever one shows: the fetched image once
//! it is on this computer, else the placeholder, a colored box (or disc)
//! with a title or initials on it. Covers keep their slots: 2:3 posters
//! for movies, shows, books, anime and games, squares for albums and
//! songs, discs for artists; an image of another shape is cropped to fit.

use iced::widget::image::{self, Image};
use iced::widget::{Space, column, container, text};
use iced::{Border, Color, ContentFit, Element, Fill, FillPortion, Length, Shadow, Vector};

use crate::model::MediaType;
use crate::theme::font;

/// The soft shadow under a cover or an artist's photo on Light (canvas
/// "Light theme", 2026-11-18): the grid, the shelves, Coming soon, the
/// details panel and the edit sheet. The blur and the color are the
/// canvas's, the offset the phone's (its `docs/DESIGN.md`, 6 where the
/// canvas draws 4). The dark themes draw none.
pub const SHADOW: Shadow = Shadow {
    color: crate::theme::COVER_SHADOW,
    offset: Vector::new(0.0, 6.0),
    blur_radius: 14.0,
};

/// The shadow under a small cover in a list or a table row, one up to
/// `THUMB_WIDTH` wide.
pub const THUMB_SHADOW: Shadow = Shadow {
    color: crate::theme::COVER_SHADOW,
    offset: Vector::new(0.0, 2.0),
    blur_radius: 6.0,
};

/// The widest cover that still takes the small shadow.
const THUMB_WIDTH: f32 = 56.0;

/// The shadow for a cover `width` wide; a cover that fills its slot is a
/// tile, never a thumbnail.
fn shadow_for(width: Length) -> Shadow {
    match width {
        Length::Fixed(width) if width <= THUMB_WIDTH => THUMB_SHADOW,
        _ => SHADOW,
    }
}

/// A box that draws `shadow` around `radius` corners on Light and nothing
/// else: iced draws a quad's shadow outside its shape only, so the box
/// can stay empty and nothing shows through or beside the cover on it.
fn lifted(shadow: Option<Shadow>, radius: f32) -> impl Fn(&iced::Theme) -> container::Style {
    move |theme| match shadow {
        Some(shadow) if crate::theme::lifts_covers(theme) => container::Style::default()
            .border(Border::default().rounded(radius))
            .shadow(shadow),
        _ => container::Style::default(),
    }
}

/// Whether the type's covers are square: music's albums (and the songs,
/// which show their album's), as on the phone. Artists are discs.
pub fn is_square(media: MediaType) -> bool {
    media == MediaType::Music
}

/// A cover's height for its width: the width for a square, 1.5 times it
/// for a poster.
pub fn height_for(width: f32, square: bool) -> f32 {
    if square { width } else { width * 1.5 }
}

/// A cover that fills a 2:3 slot, or for a square cover the bottom two
/// thirds of it, so the captions under a grid of squares in 2:3 slots
/// stay where a poster's would (Coming soon's grid with Music only); a
/// row mixing types frames its squares instead (`poster_frame`).
pub fn in_poster_slot<'a, Message: 'a>(
    cover: impl Into<Element<'a, Message>>,
    square: bool,
) -> Element<'a, Message> {
    if !square {
        return cover.into();
    }
    column![
        Space::new().width(Fill).height(FillPortion(1)),
        container(cover).width(Fill).height(FillPortion(2)),
    ]
    .into()
}

/// How bright a poster frame's backdrop is: its blurred copy drawn this
/// faded over black.
const BACKDROP_BRIGHTNESS: f32 = 0.55;

/// A square cover in a 2:3 tile among posters (option C of the album
/// sketch, owner 2026-11-08): the square centered on `backdrop`, its
/// blurred copy (`blur`), darkened and filling the tile, rounded by
/// `radius`; until the copy is made, the image itself. The tile fills
/// the 2:3 slot it is given, `square` its middle two thirds. An album
/// without its image is a plain 2:3 placeholder there instead. The tile
/// is the cover here: with `lift` it casts the covers' shadow on Light,
/// and the square on it is a `flat_cover`.
pub fn poster_frame<'a, Message: 'a>(
    square: impl Into<Element<'a, Message>>,
    backdrop: image::Handle,
    radius: f32,
    lift: bool,
) -> Element<'a, Message> {
    let dark = Color::BLACK;
    let base = container(
        Image::new(backdrop)
            .width(Fill)
            .height(Fill)
            .content_fit(ContentFit::Cover)
            .border_radius(radius)
            .opacity(BACKDROP_BRIGHTNESS),
    )
    .width(Fill)
    .height(Fill)
    .clip(true)
    .style(move |theme| {
        let shadow = if lift && crate::theme::lifts_covers(theme) {
            SHADOW
        } else {
            Shadow::default()
        };
        container::background(dark)
            .border(Border::default().rounded(radius))
            .shadow(shadow)
    });
    // The square is the tile's width, two thirds of its height, with a
    // sixth above and below.
    let centered = column![
        Space::new().width(Fill).height(FillPortion(1)),
        container(square).width(Fill).height(FillPortion(4)),
        Space::new().width(Fill).height(FillPortion(1)),
    ];
    iced::widget::stack![base, centered].into()
}

/// A placeholder's label: the words and their size, drawn bottom-left in
/// white; a disc centers them.
#[derive(Debug, Clone)]
pub struct Label {
    pub words: String,
    pub size: f32,
    pub padding: f32,
    pub centered: bool,
}

/// The image at `width` by `height`, cropped to fill and rounded by
/// `radius`; the tinted placeholder with `label` until then. On Light a
/// hairline lies over its edge (`theme::cover_edge`) and a soft shadow
/// under it: `THUMB_SHADOW` for a small cover in a row, else `SHADOW`.
pub fn cover<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: impl Into<Length>,
    height: impl Into<Length>,
    radius: f32,
    label: Option<Label>,
) -> Element<'a, Message> {
    let width = width.into();
    build(
        handle,
        tint,
        width,
        height.into(),
        radius,
        label,
        Some(shadow_for(width)),
    )
}

/// A cover without the shadow: the cover viewer's, a choice in a cover
/// picker, a selected tile under its accent ring, the square on a poster
/// frame.
pub fn flat_cover<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: impl Into<Length>,
    height: impl Into<Length>,
    radius: f32,
    label: Option<Label>,
) -> Element<'a, Message> {
    build(
        handle,
        tint,
        width.into(),
        height.into(),
        radius,
        label,
        None,
    )
}

fn build<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: Length,
    height: Length,
    radius: f32,
    label: Option<Label>,
    shadow: Option<Shadow>,
) -> Element<'a, Message> {
    // A layer over the cover, not the cover's own border: iced draws a
    // container's border under its content, where the image hides it.
    let edge = container(Space::new())
        .width(Fill)
        .height(Fill)
        .style(move |theme| match crate::theme::cover_edge(theme) {
            Some(color) => container::Style::default()
                .border(Border::default().rounded(radius).width(1).color(color)),
            None => container::Style::default(),
        });
    iced::widget::stack![
        face(handle, tint, width, height, radius, label, shadow),
        edge
    ]
    .into()
}

fn face<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: Length,
    height: Length,
    radius: f32,
    label: Option<Label>,
    shadow: Option<Shadow>,
) -> Element<'a, Message> {
    match handle {
        Some(handle) => container(
            Image::new(handle)
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Cover)
                .border_radius(radius),
        )
        .width(width)
        .height(height)
        // Nothing behind the image: a tinted box under it showed as a
        // hairline of the type's color at the edge wherever the image
        // landed a fraction of a pixel off it (after a slide, say). The
        // shadow is drawn around the box, never inside it.
        .clip(true)
        .style(lifted(shadow, radius))
        .into(),
        None => {
            let (padding, align, centered) = match &label {
                Some(label) if label.centered => (0.0, iced::alignment::Vertical::Center, true),
                Some(label) => (label.padding, iced::alignment::Vertical::Bottom, false),
                None => (0.0, iced::alignment::Vertical::Bottom, false),
            };
            let face: Element<'a, Message> = match label {
                Some(label) => text(label.words)
                    .size(label.size)
                    .font(font::EXTRABOLD)
                    .color(Color::WHITE)
                    .into(),
                None => Space::new().into(),
            };
            // The mockup's placeholder: the title's color with a fade to
            // dark over its lower half, which keeps the white words
            // readable, and a highlight from the top left over that.
            let base = container(Space::new())
                .width(width)
                .height(height)
                .style(move |_| container::Style {
                    background: Some(fade(tint)),
                    border: Border::default().rounded(radius),
                    ..container::Style::default()
                });
            let highlight = Image::new(highlight())
                .width(Fill)
                .height(Fill)
                .content_fit(ContentFit::Fill)
                .border_radius(radius);
            let mut words = container(face)
                .width(Fill)
                .height(Fill)
                .padding(padding)
                .align_y(align)
                .clip(true);
            if centered {
                words = words.align_x(iced::alignment::Horizontal::Center);
            }
            // The shadow from a box of its own around the placeholder:
            // iced draws none for a quad filled with a gradient.
            container(iced::widget::stack![base, highlight, words])
                .width(width)
                .height(height)
                .style(lifted(shadow, radius))
                .into()
        }
    }
}

/// How dark a placeholder is at its bottom edge, and where the fade to
/// it starts: the mockup's `linear-gradient(180deg, rgba(0,0,0,0) 45%,
/// rgba(0,0,0,0.42) 100%)`.
const FADE: f32 = 0.42;
const FADE_FROM: f32 = 0.45;

/// A placeholder's fill: `tint`, darkening over its lower half. The
/// black is mixed into the tint here, so one gradient is the color and
/// the fade.
fn fade(tint: Color) -> iced::Background {
    let dark = Color {
        r: tint.r * (1.0 - FADE),
        g: tint.g * (1.0 - FADE),
        b: tint.b * (1.0 - FADE),
        a: tint.a,
    };
    iced::gradient::Linear::new(iced::Radians::PI)
        .add_stop(0.0, tint)
        .add_stop(FADE_FROM, tint)
        .add_stop(1.0, dark)
        .into()
}

/// The highlight's strength at its center, and how far along the
/// ellipse's radius it reaches.
const HIGHLIGHT: f32 = 0.16;
const HIGHLIGHT_REACH: f32 = 0.6;

/// The highlight's strength at a point of the cover, both coordinates 0
/// to 1: the mockup's `radial-gradient(130% 70% at 20% 0%,
/// rgba(255,255,255,0.16), rgba(255,255,255,0) 60%)`, an ellipse 130 %
/// of the width by 70 % of the height around a point on the top edge, a
/// fifth of the way in.
fn highlight_at(x: f32, y: f32) -> f32 {
    let distance = (((x - 0.2) / 1.3).powi(2) + (y / 0.7).powi(2)).sqrt();
    HIGHLIGHT * (1.0 - distance / HIGHLIGHT_REACH).max(0.0)
}

/// The highlight as a picture, stretched over a placeholder of any
/// shape: iced 0.14 draws linear gradients only, and the highlight is
/// radial. Its measures are all shares of the cover's own, so one small
/// picture of white at the strengths of `highlight_at` is the gradient
/// for every size; made once.
fn highlight() -> image::Handle {
    static HANDLE: std::sync::OnceLock<image::Handle> = std::sync::OnceLock::new();
    const SIDE: u32 = 96;
    HANDLE
        .get_or_init(|| {
            let at = |i: u32| (i as f32 + 0.5) / SIDE as f32;
            let pixels: Vec<u8> = (0..SIDE * SIDE)
                .flat_map(|i| {
                    let alpha = highlight_at(at(i % SIDE), at(i / SIDE));
                    [255, 255, 255, (alpha * 255.0).round() as u8]
                })
                .collect();
            image::Handle::from_rgba(SIDE, SIDE, pixels)
        })
        .clone()
}

/// A plain tinted box without words, or the image.
pub fn plain_cover<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: f32,
    height: f32,
    radius: f32,
) -> Element<'a, Message> {
    cover(handle, tint, width, height, radius, None)
}

/// A small plain cover in a row, `width` wide: a 2:3 poster, or for
/// `media` with square covers (albums) a square, so the row is only as
/// tall as its cover and the gaps between rows stay the same.
pub fn thumb<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: f32,
    media: MediaType,
    radius: f32,
) -> Element<'a, Message> {
    plain_cover(
        handle,
        tint,
        width,
        height_for(width, is_square(media)),
        radius,
    )
}

/// A cover with the title on its placeholder, as the inspector, the form
/// and the shelves show it.
pub fn titled_cover<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    title: &str,
    width: f32,
    height: f32,
    size: f32,
) -> Element<'a, Message> {
    cover(
        handle,
        tint,
        width,
        height,
        8.0,
        Some(title_label(title, width, size)),
    )
}

/// A titled cover without the shadow (`flat_cover`).
pub fn flat_titled_cover<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    title: &str,
    width: f32,
    height: f32,
    size: f32,
) -> Element<'a, Message> {
    flat_cover(
        handle,
        tint,
        width,
        height,
        8.0,
        Some(title_label(title, width, size)),
    )
}

fn title_label(title: &str, width: f32, size: f32) -> Label {
    Label {
        words: title.to_owned(),
        size,
        padding: if width >= 150.0 { 14.0 } else { 10.0 },
        centered: false,
    }
}

/// One choice in Pick a cover: the cover ringed in the accent when it is
/// the one in use, the title, the year and edition, and a note ("The
/// match's cover"). The form's dialog and the cover viewer both list
/// them; `width` sets the tile, the cover 2:3 under it, or square.
pub struct Choice<'a> {
    pub handle: Option<image::Handle>,
    pub tint: Color,
    pub title: &'a str,
    pub sub: String,
    pub note: &'a str,
    pub on: bool,
    pub square: bool,
    /// A dark pill in the cover's top left corner: "Yours" on one of the
    /// owner's own files.
    pub badge: Option<&'a str>,
}

/// The accent ring around the chosen cover.
const RING: f32 = 3.0;

pub fn choice_tile<'a, Message: Clone + 'a>(
    choice: Choice<'_>,
    width: f32,
    on_press: Message,
    tokens: &crate::theme::Tokens,
) -> Element<'a, Message> {
    use super::FocusExt;
    use iced::widget::{button, column};

    let on = choice.on;
    let square = choice.square;
    let (accent, text_color) = (tokens.accent, tokens.text);
    let label_size = if width >= 150.0 { 15.0 } else { 13.0 };
    // The ring sits around the cover, 3 px in the accent on the one in
    // use; the cover would draw over a border of its own container.
    let inner = width - 2.0 * RING;
    let cover = flat_titled_cover(
        choice.handle,
        choice.tint,
        choice.title,
        inner,
        height_for(inner, square),
        label_size,
    );
    let cover: Element<'a, Message> = match choice.badge {
        Some(badge) => {
            iced::widget::stack![cover, container(pill(badge.to_owned(), None)).padding(6),].into()
        }
        None => cover,
    };
    let face = container(cover).padding(RING).style(move |_| {
        container::Style::default().border(
            Border::default()
                .rounded(8.0 + RING)
                .width(if on { RING } else { 0.0 })
                .color(accent),
        )
    });
    button(
        column![
            face,
            container(
                text(choice.title.to_owned())
                    .size(13)
                    .font(font::SEMIBOLD)
                    .wrapping(text::Wrapping::None)
            )
            .width(width)
            .clip(true),
            text(choice.sub).size(12).color(tokens.sub),
            text(choice.note.to_owned()).size(12).color(tokens.sub),
        ]
        .spacing(3)
        .width(width),
    )
    .padding(0)
    .on_press(on_press)
    .style(move |_, _| button::Style {
        text_color,
        ..button::Style::default()
    })
    .focusable(8.0, tokens)
}

/// The dark pill a cover of the owner's own carries: "Yours".
pub fn badge<'a, Message: 'a>(words: &str) -> Element<'a, Message> {
    pill(words.to_owned(), None)
}

/// The dark pill on a cover: its words, and after them words in `accent`
/// (the action of a chip).
fn pill<'a, Message: 'a>(words: String, action: Option<(&'a str, Color)>) -> Element<'a, Message> {
    use crate::theme::{COVER_BADGE, VIEWER_TEXT};
    let mut line = iced::widget::Row::new().push(
        text(words)
            .size(11)
            .font(font::BOLD)
            .color(VIEWER_TEXT)
            .wrapping(text::Wrapping::None),
    );
    if let Some((action, accent)) = action {
        line = line.push(
            text(action)
                .size(11)
                .font(font::BOLD)
                .color(accent)
                .wrapping(text::Wrapping::None),
        );
    }
    container(line)
        .padding([3, 7])
        .style(|_| container::background(COVER_BADGE).border(Border::default().rounded(6)))
        .into()
}

/// A choice tile with a chip across its cover's foot: "Not 2:3 · " and
/// the action after it ("Adjust") in the accent, a button of its own
/// over the tile's.
pub fn with_chip<'a, Message: Clone + 'a>(
    tile: Element<'a, Message>,
    width: f32,
    square: bool,
    words: String,
    action: &'a str,
    on_press: Message,
    tokens: &crate::theme::Tokens,
) -> Element<'a, Message> {
    use super::FocusExt;
    use iced::widget::button;

    let cover_height = height_for(width - 2.0 * RING, square) + 2.0 * RING;
    let chip = button(pill(words, Some((action, tokens.accent))))
        .padding(0)
        .on_press(on_press)
        .style(|_, _| button::Style::default())
        .focusable(6.0, tokens);
    iced::widget::stack![
        tile,
        container(chip)
            .width(width)
            .height(cover_height)
            .padding(RING + 6.0)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Bottom),
    ]
    .into()
}

/// The Upload tile that leads the owner's covers in Pick a cover: a
/// plus, "Upload" and how ("Browse or Ctrl+V"), in the shape of a cover;
/// "Reading…" while a picked file is being read.
pub fn upload_tile<'a, Message: Clone + 'a>(
    width: f32,
    square: bool,
    busy: bool,
    on_press: Message,
    tokens: &crate::theme::Tokens,
) -> Element<'a, Message> {
    use super::{FocusExt, Icon, icon};
    use iced::widget::button;

    let tokens = *tokens;
    let inner = width - 2.0 * RING;
    let words = column![
        icon(Icon::Plus, 22.0, tokens.text),
        text(if busy { "Reading…" } else { "Upload" })
            .size(13)
            .font(font::SEMIBOLD)
            .color(tokens.text),
        text("Browse or Ctrl+V")
            .size(11)
            .color(tokens.sub)
            .align_x(iced::alignment::Horizontal::Center),
    ]
    .spacing(6)
    .align_x(iced::alignment::Horizontal::Center);
    button(
        container(words)
            .width(inner)
            .height(height_for(inner, square))
            .padding(8)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding(RING)
    .on_press_maybe((!busy).then_some(on_press))
    .style(move |_, status| button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                tokens.selection
            } else {
                tokens.hover
            }
            .into(),
        ),
        border: Border::default().rounded(8.0 + RING),
        ..button::Style::default()
    })
    .focusable(8.0 + RING, &tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The placeholder's highlight is the mockup's radial gradient: 16 %
    /// white at the top edge a fifth of the way in, gone at 60 % of an
    /// ellipse 130 % of the width by 70 % of the height.
    #[test]
    fn the_highlight_is_the_mockups_radial_gradient() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(highlight_at(0.2, 0.0), 0.16));
        // Halfway to its end: straight down, 0.3 × 0.7 of the height.
        assert!(close(highlight_at(0.2, 0.21), 0.08));
        // Its end: 0.6 × 0.7 down, and 0.6 × 1.3 to the right.
        assert!(close(highlight_at(0.2, 0.42), 0.0));
        assert!(close(highlight_at(0.98, 0.0), 0.0));
        assert_eq!(highlight_at(0.5, 0.9), 0.0);
        // The top right corner is still inside it, faintly.
        assert!(highlight_at(1.0, 0.0) == 0.0 && highlight_at(0.9, 0.0) > 0.0);
    }

    /// The fade keeps the title's color down to 45 % and ends 42 % toward
    /// black.
    #[test]
    fn the_fade_darkens_the_lower_half() {
        let tint = Color::from_rgb8(0x4B, 0x5F, 0x8A);
        let iced::Background::Gradient(iced::Gradient::Linear(linear)) = fade(tint) else {
            panic!("a linear gradient");
        };
        let stops: Vec<_> = linear.stops.iter().flatten().collect();
        assert_eq!(stops.len(), 3);
        assert_eq!((stops[0].offset, stops[0].color), (0.0, tint));
        assert_eq!((stops[1].offset, stops[1].color), (0.45, tint));
        assert_eq!(stops[2].offset, 1.0);
        assert_eq!(stops[2].color.into_rgba8(), [0x2C, 0x37, 0x50, 0xFF]);
    }

    /// Light's two shadows (canvas `.cv` and `.cvs`, the tile's offset
    /// from the phone): the small one for a cover up to 56 px wide, the
    /// other for every wider cover and for a tile that fills its slot.
    #[test]
    fn a_cover_takes_the_shadow_of_its_width() {
        let color = Color::from_rgba8(20, 22, 28, 0.13);
        assert_eq!(
            (SHADOW.color, SHADOW.offset, SHADOW.blur_radius),
            (color, Vector::new(0.0, 6.0), 14.0)
        );
        assert_eq!(
            (
                THUMB_SHADOW.color,
                THUMB_SHADOW.offset,
                THUMB_SHADOW.blur_radius
            ),
            (color, Vector::new(0.0, 2.0), 6.0)
        );
        for width in [26.0, 44.0, 56.0] {
            assert_eq!(shadow_for(Length::Fixed(width)), THUMB_SHADOW);
        }
        for width in [Length::Fixed(57.0), Length::Fixed(150.0), Fill] {
            assert_eq!(shadow_for(width), SHADOW);
        }
    }

    /// Only Light lifts its covers, and a flat cover casts nothing there
    /// either.
    #[test]
    fn only_light_draws_the_shadow() {
        use crate::theme::{PEACH, Theme, Tokens};
        let light = Tokens::new(Theme::Light, PEACH).iced_theme();
        assert_eq!(lifted(Some(SHADOW), 8.0)(&light).shadow, SHADOW);
        assert_eq!(lifted(None, 8.0)(&light).shadow, Shadow::default());
        for theme in [Theme::Graphite, Theme::Charcoal, Theme::Mocha, Theme::Glass] {
            let dark = Tokens::new(theme, PEACH).iced_theme();
            assert_eq!(lifted(Some(SHADOW), 8.0)(&dark).shadow, Shadow::default());
        }
    }
}
