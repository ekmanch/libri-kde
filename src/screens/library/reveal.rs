//! The operation behind `reveal`: finds the list and an entry's row in
//! it, and scrolls the list just enough to show the row.

use iced::advanced::widget::Id;
use iced::advanced::widget::operation::{self, Operation, Outcome, Scrollable};
use iced::widget::scrollable::AbsoluteOffset;
use iced::{Rectangle, Vector};

/// Room kept above a revealed row, for the pinned type header.
const TOP: f32 = 64.0;
/// Room kept below it.
const BOTTOM: f32 = 16.0;

pub struct Reveal {
    list: Id,
    row: Id,
    /// The list's viewport height, its content's top and how far it is
    /// scrolled.
    view: Option<(f32, f32, f32)>,
    /// The row's top and bottom, in the window.
    found: Option<(f32, f32)>,
}

impl Reveal {
    pub fn new(list: &'static str, row: &str) -> Reveal {
        Reveal {
            list: Id::new(list),
            row: Id::from(row.to_owned()),
            view: None,
            found: None,
        }
    }
}

impl<T: Send + 'static> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.list) {
            self.view = Some((bounds.height, content_bounds.y, translation.y));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.row) {
            self.found = Some((bounds.y, bounds.y + bounds.height));
        }
    }

    fn finish(&self) -> Outcome<T> {
        let (Some((height, content, offset)), Some((top, bottom))) = (self.view, self.found) else {
            return Outcome::None;
        };
        match offset_to_show(top - content, bottom - content, offset, height) {
            Some(y) => Outcome::Chain(Box::new(operation::scrollable::scroll_to(
                self.list.clone(),
                AbsoluteOffset {
                    x: None,
                    y: Some(y),
                },
            ))),
            None => Outcome::None,
        }
    }
}

/// The offset that shows a row from `top` to `bottom` (in the content) in
/// a viewport `height` tall scrolled to `offset`, or `None` when it shows
/// already. A row taller than the room shows from its top.
pub fn offset_to_show(top: f32, bottom: f32, offset: f32, height: f32) -> Option<f32> {
    let room = height - TOP - BOTTOM;
    if top < offset + TOP || bottom - top > room {
        let y = (top - TOP).max(0.0);
        (y != offset).then_some(y)
    } else if bottom > offset + height - BOTTOM {
        Some(bottom - height + BOTTOM)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_in_view_stays_where_it_is() {
        assert_eq!(offset_to_show(500.0, 600.0, 300.0, 800.0), None);
    }

    #[test]
    fn a_row_below_the_view_comes_up_to_its_bottom() {
        assert_eq!(
            offset_to_show(1200.0, 1300.0, 300.0, 800.0),
            Some(1300.0 - 800.0 + BOTTOM)
        );
    }

    #[test]
    fn a_row_above_the_view_comes_down_clear_of_the_pinned_header() {
        assert_eq!(
            offset_to_show(100.0, 200.0, 300.0, 800.0),
            Some(100.0 - TOP)
        );
        assert_eq!(offset_to_show(10.0, 100.0, 300.0, 800.0), Some(0.0));
    }
}
