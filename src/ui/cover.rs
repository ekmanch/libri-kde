//! A cover or an artist photo wherever one shows: the fetched image once
//! it is on this computer, else the placeholder, a colored box (or disc)
//! with a title or initials on it. Covers keep their slots: 2:3 posters
//! for movies, shows, books, anime and games, squares for albums and
//! songs, discs for artists; an image of another shape is cropped to fit.

use iced::widget::image::{self, Image};
use iced::widget::{Space, column, container, text};
use iced::{Border, Color, ContentFit, Element, Fill, FillPortion, Length};

use crate::model::MediaType;
use crate::theme::font;

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
/// without its image is a plain 2:3 placeholder there instead.
pub fn poster_frame<'a, Message: 'a>(
    square: impl Into<Element<'a, Message>>,
    backdrop: image::Handle,
    radius: f32,
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
    .style(move |_| container::background(dark).border(Border::default().rounded(radius)));
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
/// `radius`; the tinted placeholder with `label` until then.
pub fn cover<'a, Message: 'a>(
    handle: Option<image::Handle>,
    tint: Color,
    width: impl Into<Length>,
    height: impl Into<Length>,
    radius: f32,
    label: Option<Label>,
) -> Element<'a, Message> {
    let (width, height) = (width.into(), height.into());
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
        // landed a fraction of a pixel off it (after a slide, say).
        .clip(true)
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
            let mut placeholder = container(face)
                .width(width)
                .height(height)
                .padding(padding)
                .align_y(align)
                .clip(true)
                .style(move |_| {
                    container::background(tint).border(Border::default().rounded(radius))
                });
            if centered {
                placeholder = placeholder.align_x(iced::alignment::Horizontal::Center);
            }
            placeholder.into()
        }
    }
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
        Some(Label {
            words: title.to_owned(),
            size,
            padding: if width >= 150.0 { 14.0 } else { 10.0 },
            centered: false,
        }),
    )
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
    let cover = titled_cover(
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
