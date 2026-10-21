//! The places visited, for the mouse's Back and Forward buttons and
//! Alt+Left and Alt+Right: the screen, the library's view and the plan
//! page, as a browser remembers pages.

use crate::screens::{Screen, library, plans};

/// How far back the history remembers.
const CAPACITY: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Back,
    Forward,
}

/// Where the app is: the screen, what the library shows and which plan
/// page is open. Selection, popovers and scroll positions are not part of
/// a place.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub screen: Screen,
    pub library: library::Place,
    pub plans: plans::Page,
}

/// The places left behind, and those left by going back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    back: Vec<Place>,
    forward: Vec<Place>,
}

impl History {
    /// Records `from` as the place just left for a new one. A new place
    /// drops the places ahead, as in a browser.
    pub fn visit(&mut self, from: Place) {
        self.forward.clear();
        if self.back.last() == Some(&from) {
            return;
        }
        if self.back.len() == CAPACITY {
            self.back.remove(0);
        }
        self.back.push(from);
    }

    /// The place to go to from `current`, which moves to the other side;
    /// `None` at either end.
    pub fn go(&mut self, direction: Direction, current: Place) -> Option<Place> {
        let (from, to) = match direction {
            Direction::Back => (&mut self.forward, &mut self.back),
            Direction::Forward => (&mut self.back, &mut self.forward),
        };
        let place = to.pop()?;
        from.push(current);
        Some(place)
    }

    pub fn can_go(&self, direction: Direction) -> bool {
        match direction {
            Direction::Back => !self.back.is_empty(),
            Direction::Forward => !self.forward.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(screen: Screen) -> Place {
        Place {
            screen,
            library: library::Place::default(),
            plans: plans::Page::default(),
        }
    }

    #[test]
    fn back_returns_to_the_place_left_and_forward_comes_back() {
        let mut history = History::default();
        history.visit(at(Screen::Library));
        history.visit(at(Screen::Plans));
        assert_eq!(
            history.go(Direction::Back, at(Screen::Settings)),
            Some(at(Screen::Plans))
        );
        assert_eq!(
            history.go(Direction::Back, at(Screen::Plans)),
            Some(at(Screen::Library))
        );
        assert_eq!(history.go(Direction::Back, at(Screen::Library)), None);
        assert_eq!(
            history.go(Direction::Forward, at(Screen::Library)),
            Some(at(Screen::Plans))
        );
        assert_eq!(
            history.go(Direction::Forward, at(Screen::Plans)),
            Some(at(Screen::Settings))
        );
        assert_eq!(history.go(Direction::Forward, at(Screen::Settings)), None);
    }

    #[test]
    fn a_new_place_drops_the_places_ahead() {
        let mut history = History::default();
        history.visit(at(Screen::Library));
        let _ = history.go(Direction::Back, at(Screen::Plans));
        assert!(history.can_go(Direction::Forward));
        history.visit(at(Screen::Library));
        assert!(!history.can_go(Direction::Forward));
        assert!(history.can_go(Direction::Back));
    }

    #[test]
    fn the_same_place_twice_in_a_row_is_kept_once() {
        let mut history = History::default();
        history.visit(at(Screen::Library));
        history.visit(at(Screen::Library));
        assert_eq!(
            history.go(Direction::Back, at(Screen::Plans)),
            Some(at(Screen::Library))
        );
        assert!(!history.can_go(Direction::Back));
    }

    #[test]
    fn the_oldest_place_falls_off_at_capacity() {
        let mut history = History::default();
        history.visit(at(Screen::Settings));
        for _ in 0..CAPACITY {
            history.visit(at(Screen::Library));
            history.visit(at(Screen::Plans));
        }
        let mut last = None;
        while let Some(place) = history.go(Direction::Back, at(Screen::Library)) {
            last = Some(place);
        }
        assert_ne!(last, Some(at(Screen::Settings)));
    }
}
