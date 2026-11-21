//! A picture that fills the place it is given, as the covers and the
//! artists' photos do: iced's `Image` with a size of its own. `Image`
//! asks the renderer for the picture's dimensions when it is laid out,
//! and asking loads the file, so a list of hundreds of covers decoded
//! every one of them on its first layout, on screen or far below, each
//! one a relayout of the window as it arrived and each freed again since
//! it was never drawn. Here the layout never looks at the file: the size
//! comes from the lengths alone, and the picture is asked for where it is
//! drawn, which a list only does for what is in view.

use iced::advanced::image::{self, FilterMethod};
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{mouse, renderer};
use iced::border::Radius;
use iced::widget::image::Handle;
use iced::{ContentFit, Element, Length, Point, Radians, Rectangle, Size};

pub struct Picture {
    handle: Handle,
    width: Length,
    height: Length,
    fit: ContentFit,
    radius: Radius,
    opacity: f32,
}

/// `handle` filling its place, cropped to cover it.
pub fn picture(handle: Handle) -> Picture {
    Picture {
        handle,
        width: Length::Fill,
        height: Length::Fill,
        fit: ContentFit::Cover,
        radius: Radius::default(),
        opacity: 1.0,
    }
}

impl Picture {
    /// How the picture is fitted to its place; `Cover` unless said.
    pub fn content_fit(mut self, fit: ContentFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn border_radius(mut self, radius: impl Into<Radius>) -> Self {
        self.radius = radius.into();
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

/// Where a picture of `size` is drawn to fit `bounds`: centered, at the
/// size `fit` gives it (past the bounds for `Cover`, which the clip cuts).
fn placed(size: Size<u32>, bounds: Rectangle, fit: ContentFit) -> Rectangle {
    let size = Size::new(size.width as f32, size.height as f32);
    let fitted = fit.fit(size, bounds.size());
    Rectangle::new(
        Point::new(
            bounds.center_x() - fitted.width / 2.0,
            bounds.center_y() - fitted.height / 2.0,
        ),
        fitted,
    )
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Picture
where
    Renderer: image::Renderer<Handle = Handle>,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(self.width, self.height, Size::ZERO))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        // Not loaded yet: the renderer loads it and asks for a redraw.
        let Some(size) = renderer.measure_image(&self.handle) else {
            return;
        };
        renderer.draw_image(
            image::Image {
                handle: self.handle.clone(),
                filter_method: FilterMethod::default(),
                rotation: Radians(0.0),
                border_radius: self.radius,
                opacity: self.opacity,
                snap: true,
            },
            placed(size, bounds, self.fit),
            bounds,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<Picture> for Element<'a, Message, Theme, Renderer>
where
    Renderer: image::Renderer<Handle = Handle>,
{
    fn from(picture: Picture) -> Self {
        Element::new(picture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_covers_its_place_from_the_middle() {
        let bounds = Rectangle::new(Point::new(10.0, 20.0), Size::new(100.0, 100.0));
        // A poster in a square: as wide as the square, cut above and below.
        assert_eq!(
            placed(Size::new(200, 300), bounds, ContentFit::Cover),
            Rectangle::new(Point::new(10.0, -5.0), Size::new(100.0, 150.0))
        );
        // A wide picture: as tall, cut left and right.
        assert_eq!(
            placed(Size::new(400, 200), bounds, ContentFit::Cover),
            Rectangle::new(Point::new(-40.0, 20.0), Size::new(200.0, 100.0))
        );
        // Stretched to the place.
        assert_eq!(
            placed(Size::new(400, 200), bounds, ContentFit::Fill),
            bounds
        );
    }
}
