//! The section rail of a list of one type, in every view: where the
//! list's sections start, which one it is scrolled to, and the rail that
//! shows and jumps to them (`ui::SectionRail`). The text and list views
//! have a header per section, measured by `ui::Groups`; the grid has
//! none, so its sections are found from the rows its cards fall into.

use iced::Element;

use super::{Message, State};
use crate::app::Message as AppMessage;
use crate::browse;
use crate::browse::RailMark;
use crate::theme::Tokens;
use crate::ui::{self, RailLabel, SectionRail};

/// The list's padding left of its rows (`entries::scrolling`).
pub(super) const LEFT: f32 = 24.0;

/// The rows of a fluid grid of cards, by iced's own arithmetic
/// (`grid(..).fluid(max_width).spacing(spacing)` with an aspect ratio).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GridRows {
    pub columns: usize,
    /// A card's height.
    pub cell: f32,
    /// From one row's top to the next.
    pub pitch: f32,
}

impl GridRows {
    /// The rows of a grid `width` px wide whose cells are at most
    /// `max_width` wide and `ratio` (width over height) in shape.
    pub fn new(width: f32, max_width: f32, spacing: f32, ratio: f32) -> GridRows {
        let width = width.max(1.0);
        let columns = (((width + spacing) / (max_width + spacing)).ceil() as usize).max(1);
        let cell_width = (width - spacing * (columns - 1) as f32) / columns as f32;
        let cell = cell_width / ratio;
        GridRows {
            columns,
            cell,
            pitch: cell + spacing,
        }
    }

    /// The top of the row that holds card `index`, from the grid's top.
    pub fn top(&self, index: usize) -> f32 {
        (index / self.columns) as f32 * self.pitch
    }

    /// The height of a grid of `count` cards.
    pub fn height(&self, count: usize) -> f32 {
        let rows = count.div_ceil(self.columns) as f32;
        (rows * self.pitch - (self.pitch - self.cell)).max(0.0)
    }

    /// The first card of the first row still in view with the list
    /// scrolled to `offset`: a row is gone once its cards have left.
    pub fn first_shown(&self, offset: f32) -> usize {
        let row = if offset < self.cell {
            0
        } else {
            ((offset - self.cell) / self.pitch).floor() as usize + 1
        };
        row * self.columns
    }
}

/// A list's sections as the rail needs them.
pub(super) struct Sections {
    /// Each section's label, in the list's order.
    names: Vec<String>,
    /// The rail's labels, and the section each stands for.
    marks: Vec<RailMark>,
    /// The rail shows the whole alphabet, spread over its height.
    spread: bool,
    /// The rail is at its widest whatever its labels.
    wide: bool,
    /// Where each section starts, as the scroll offset that shows it.
    tops: Vec<f32>,
    /// The section at the top of the view.
    shown: Option<usize>,
}

impl Sections {
    /// The sections with the labels `names` on a rail with room for all
    /// of them; `alphabet` for the orders by title and by artist, where
    /// the rail lists "#" and A to Z.
    pub fn new(names: Vec<String>, alphabet: bool) -> Sections {
        Sections::fitted(names, alphabet, usize::MAX)
    }

    /// [`Sections::new`] on a rail with room for `fits` labels
    /// (`State::rail_fits`): without room for the alphabet it lists the
    /// sections alone, and without room for those it is thinned.
    pub fn fitted(names: Vec<String>, alphabet: bool, fits: usize) -> Sections {
        let marks = browse::rail_labels(&names, alphabet, fits);
        let whole = names.len().max(browse::RAIL_ALPHABET.len());
        Sections {
            spread: alphabet && marks.len() >= whole,
            names,
            marks,
            wide: false,
            tops: Vec::new(),
            shown: None,
        }
    }

    /// The rail at its widest whatever the labels: the Coming soon page,
    /// whose months come and go with the slices.
    pub fn wide(mut self) -> Sections {
        self.wide = true;
        self
    }

    fn labels(&self) -> Vec<RailLabel> {
        self.marks
            .iter()
            .map(|mark| RailLabel {
                text: mark.text.clone(),
                present: mark.section.is_some(),
            })
            .collect()
    }

    /// The room the list leaves at its right for the rail.
    pub fn room(&self) -> f32 {
        ui::section_rail::room(&self.labels(), self.wide)
    }

    /// The rail shows fewer labels than the list has sections.
    fn thinned(&self) -> bool {
        self.marks.len() < self.names.len()
    }

    /// The label that stands for `section`: its own, or on a thinned rail
    /// the "•" between the two labels it lies between.
    fn mark_of(&self, section: usize) -> Option<usize> {
        let kept = |mark: &RailMark| !mark.skipped && mark.section.is_some();
        let own = self
            .marks
            .iter()
            .position(|mark| kept(mark) && mark.section == Some(section));
        if own.is_some() || !self.thinned() {
            return own;
        }
        let after = self
            .marks
            .iter()
            .position(|mark| kept(mark) && mark.section > Some(section));
        Some(after.map_or(0, |index| index.saturating_sub(1)))
    }

    /// In a list with a header per section, each a child of the list's
    /// `ui::Groups`: a section starts at its header, and the one shown is
    /// the last whose header reached the top (a few px early, as the type
    /// groups).
    pub fn in_list(mut self, state: &State) -> Sections {
        self.tops = state
            .group_tops
            .iter()
            .take(self.names.len())
            .copied()
            .collect();
        let offset = state.scroll.offset;
        self.shown = self.tops.iter().rposition(|&top| top <= offset + 6.0);
        self
    }

    /// In a grid of cards in `rows`, with `counts` cards per section: a
    /// section starts at the row its first card is in, and the one shown
    /// is that of the first card in the top row.
    pub fn in_grid(mut self, state: &State, counts: &[usize], rows: GridRows) -> Sections {
        let mut first = 0;
        let firsts: Vec<usize> = counts
            .iter()
            .map(|count| {
                let at = first;
                first += count;
                at
            })
            .collect();
        self.tops = firsts.iter().map(|&index| rows.top(index)).collect();
        let card = rows.first_shown(state.scroll.offset);
        self.shown = firsts.iter().rposition(|&index| index <= card);
        self
    }

    /// The section the rail marks: the one being dragged to; the one last
    /// jumped to while the list rests where that jump left it (a section
    /// that starts mid-row, or too near the end to reach the top, would
    /// otherwise mark another); else the one at the top of the view.
    fn current(&self, state: &State) -> Option<usize> {
        let picked = state
            .rail_pick
            .as_ref()
            .and_then(|pick| self.names.iter().position(|name| name == pick));
        if let Some(section) = picked
            && (state.rail_drag
                || self
                    .tops
                    .get(section)
                    .is_some_and(|&top| state.rests_at(top)))
        {
            return Some(section);
        }
        self.shown
    }

    /// The rail, as a layer over the list.
    pub fn view<'a>(self, state: &State, tokens: &Tokens) -> Element<'a, AppMessage> {
        let current = self.current(state);
        let thinned = self.thinned();
        // The bubble names the section itself, level with its label; on a
        // thinned rail as far down the labels as the section is down the
        // list.
        let bubble = current.filter(|_| state.rail_drag).and_then(|section| {
            let at = if thinned {
                let along = (section as f32 + 0.5) / self.names.len() as f32;
                along * self.marks.len() as f32 - 0.5
            } else {
                self.mark_of(section)? as f32
            };
            Some((self.names[section].clone(), at))
        });
        let current = current.and_then(|section| self.mark_of(section));
        let labels = self.labels();
        let count = self.names.len();
        let Sections {
            names,
            marks,
            spread,
            wide,
            tops,
            ..
        } = self;
        let jump = move |section: usize, held: bool| {
            AppMessage::Library(Message::JumpToSection {
                label: names[section].clone(),
                offset: tops.get(section).copied().unwrap_or_default(),
                held,
            })
        };
        // Held, the rail stops at every section: its labels, or thinned
        // all the sections between them too.
        let stops = if thinned {
            (0..count)
                .map(|section| Some(jump(section, true)))
                .collect()
        } else {
            marks
                .iter()
                .map(|mark| mark.section.map(|section| jump(section, true)))
                .collect()
        };
        SectionRail {
            labels,
            current,
            bubble,
            spread,
            wide,
            on_jump: Box::new(move |label| jump(marks[label].section.unwrap_or_default(), false)),
            stops,
            on_release: AppMessage::Library(Message::RailReleased),
        }
        .view(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::library::Scroll;

    fn names(labels: &[&str]) -> Vec<String> {
        labels.iter().map(|&label| label.to_owned()).collect()
    }

    /// Seven cards to a row, 300 px from one row to the next.
    fn rows() -> GridRows {
        GridRows {
            columns: 7,
            cell: 280.0,
            pitch: 300.0,
        }
    }

    fn scrolled(offset: f32) -> State {
        State {
            scroll: Scroll {
                offset,
                viewport: 800.0,
                content: 3000.0,
            },
            ..State::default()
        }
    }

    #[test]
    fn the_rows_are_the_grids_own() {
        // 1000 px: five cells of at most 190 with 20 between them.
        let rows = GridRows::new(1000.0, 190.0, 20.0, 0.5);
        assert_eq!(rows.columns, 5);
        assert_eq!(rows.cell, 368.0, "a 184 px cell, twice as tall");
        assert_eq!(rows.pitch, 388.0);
        assert_eq!(rows.top(4), 0.0);
        assert_eq!(rows.top(5), 388.0);
        assert_eq!(rows.height(5), 368.0);
        assert_eq!(rows.height(6), 756.0);
        assert_eq!(rows.height(0), 0.0);
        // Not measured yet: one column, never none.
        assert_eq!(GridRows::new(0.0, 190.0, 20.0, 0.5).columns, 1);
    }

    #[test]
    fn a_grid_row_is_shown_until_its_cards_have_left() {
        let rows = rows();
        assert_eq!(rows.first_shown(0.0), 0);
        assert_eq!(rows.first_shown(279.0), 0, "a sliver of the first row");
        assert_eq!(rows.first_shown(280.0), 7);
        assert_eq!(rows.first_shown(300.0), 7, "scrolled to the second row");
        assert_eq!(rows.first_shown(600.0), 14);
    }

    #[test]
    fn a_grids_sections_start_at_the_row_of_their_first_card() {
        // A has ten cards, B three, C twenty: B starts in the second row,
        // C in the same one.
        let counts = [10, 3, 20];
        let sections =
            Sections::new(names(&["A", "B", "C"]), false).in_grid(&scrolled(0.0), &counts, rows());
        assert_eq!(sections.tops, [0.0, 300.0, 300.0]);
        assert_eq!(sections.shown, Some(0));
        // Scrolled to the second row, whose first card is an A still.
        let state = scrolled(300.0);
        let sections =
            Sections::new(names(&["A", "B", "C"]), false).in_grid(&state, &counts, rows());
        assert_eq!(sections.shown, Some(0));
        assert_eq!(sections.current(&state), Some(0));
        // The third row starts with a C.
        let state = scrolled(600.0);
        let sections =
            Sections::new(names(&["A", "B", "C"]), false).in_grid(&state, &counts, rows());
        assert_eq!(sections.current(&state), Some(2));
    }

    #[test]
    fn a_jump_marks_its_section_until_the_list_moves_on() {
        let counts = [10, 3, 20];
        let place = |state: &State| {
            Sections::new(names(&["A", "B", "C"]), false).in_grid(state, &counts, rows())
        };
        // B starts mid-row: by the scroll alone the rail would mark A.
        let mut state = scrolled(0.0);
        state.update(Message::JumpToSection {
            label: "B".into(),
            offset: 300.0,
            held: false,
        });
        assert_eq!(state.scroll.offset, 300.0, "before the list says so");
        assert_eq!(place(&state).current(&state), Some(1));
        // The wheel moves the list: the scroll decides again.
        state.update(Message::Scrolled(Scroll {
            offset: 340.0,
            ..state.scroll
        }));
        assert_eq!(place(&state).current(&state), Some(0));
        // A section too near the end to reach the top is marked at the
        // end of the list.
        let mut state = scrolled(0.0);
        state.update(Message::JumpToSection {
            label: "C".into(),
            offset: 2900.0,
            held: false,
        });
        assert_eq!(state.scroll.offset, 2200.0, "as far as the list goes");
        let mut sections = place(&state);
        sections.tops[2] = 2900.0;
        assert_eq!(sections.current(&state), Some(2));
        // Another sort has no such section.
        state.group_tops = vec![0.0, 2200.0];
        let other = Sections::new(names(&["2020s", "2010s"]), false).in_list(&state);
        assert_eq!(other.current(&state), Some(1));
    }

    #[test]
    fn the_bubble_shows_only_while_the_button_is_held() {
        let mut state = scrolled(0.0);
        let jump = |held| Message::JumpToSection {
            label: "B".into(),
            offset: 300.0,
            held,
        };
        // Enter on a label, or a wheel: no bubble.
        state.update(jump(false));
        assert!(!state.rail_drag);
        state.update(Message::Scrolled(Scroll {
            offset: 500.0,
            ..state.scroll
        }));
        assert!(!state.rail_drag);
        // Held: the bubble, and the dragged section marked wherever the
        // list is, until the release.
        state.update(jump(true));
        assert!(state.rail_drag);
        state.update(Message::Scrolled(Scroll {
            offset: 120.0,
            ..state.scroll
        }));
        let sections =
            Sections::new(names(&["A", "B", "C"]), false).in_grid(&state, &[10, 3, 20], rows());
        assert_eq!(sections.current(&state), Some(1));
        state.update(Message::RailReleased);
        assert!(!state.rail_drag);
        // A release that never came (the rail went away under the
        // pointer) ends with whatever the library does next.
        state.update(jump(true));
        state.update(Message::ClosePopup);
        assert!(!state.rail_drag);
    }

    #[test]
    fn a_lists_sections_start_at_their_headers() {
        let mut state = scrolled(597.0);
        state.group_tops = vec![0.0, 600.0, 1200.0];
        let sections = Sections::new(names(&["A", "B", "C"]), true).in_list(&state);
        assert_eq!(sections.tops, [0.0, 600.0, 1200.0]);
        assert_eq!(sections.shown, Some(1), "a few px early");
        // The alphabet's other letters are on the rail, with nothing to
        // jump to.
        assert_eq!(sections.marks.len(), 27);
        assert!(sections.spread);
        assert_eq!(sections.marks[1].section, Some(0), "A, after #");
        assert_eq!(sections.marks[4].section, None, "D");
    }

    #[test]
    fn a_thinned_rail_marks_the_dot_a_skipped_section_is_under() {
        let years: Vec<String> = (0..12).map(|i| (2026 - i).to_string()).collect();
        // Room for five: 2026 • 2020 • 2015.
        let sections = Sections::fitted(years.clone(), false, 5);
        assert!(sections.thinned() && !sections.spread);
        assert_eq!(sections.marks.len(), 5);
        assert_eq!(sections.mark_of(0), Some(0));
        assert_eq!(sections.mark_of(2), Some(1), "2024, under the first dot");
        assert_eq!(sections.mark_of(6), Some(2), "2020, on the rail");
        assert_eq!(sections.mark_of(7), Some(3));
        assert_eq!(sections.mark_of(11), Some(4));
        // The rail is narrow or wide by the labels it shows.
        assert_eq!(sections.room(), 66.0, "four characters");
        let letters = Sections::fitted(names(&["A", "B", "C"]), false, 30);
        assert_eq!(letters.room(), 44.0);
        assert_eq!(Sections::new(names(&["5–9", "4"]), false).room(), 44.0);
        assert_eq!(
            Sections::new(names(&["Nov", "Dec"]), false).wide().room(),
            66.0
        );
        // An alphabet with no room: the letters the list has, from the
        // top.
        let short = Sections::fitted(names(&["A", "B", "C"]), true, 20);
        assert_eq!(short.marks.len(), 3);
        assert!(!short.spread && !short.thinned());
    }
}
