//! A cover or an artist photo wherever one shows: the fetched image once
//! it is on this computer, else the placeholder, a colored box (or disc)
//! with a title or initials on it. Covers keep their slots: 2:3 posters
//! for movies, shows, books, anime and games, squares for albums and
//! songs, discs for artists; an image of another shape is cropped to fit.

use iced::widget::image::{self, Image};
use iced::widget::{Space, container, text};
use iced::{Border, Color, ContentFit, Element, Length};

use crate::theme::font;

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
        .clip(true)
        .style(move |_| container::background(tint).border(Border::default().rounded(radius)))
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
/// them; `width` sets the tile, the cover 2:3 under it.
pub struct Choice<'a> {
    pub handle: Option<image::Handle>,
    pub tint: Color,
    pub title: &'a str,
    pub sub: String,
    pub note: &'a str,
    pub on: bool,
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
    let (accent, text_color) = (tokens.accent, tokens.text);
    let label_size = if width >= 150.0 { 15.0 } else { 13.0 };
    // The ring sits around the cover, 3 px in the accent on the one in
    // use; the cover would draw over a border of its own container.
    let inner = width - 2.0 * RING;
    let face = container(titled_cover(
        choice.handle,
        choice.tint,
        choice.title,
        inner,
        inner * 1.5,
        label_size,
    ))
    .padding(RING)
    .style(move |_| {
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
