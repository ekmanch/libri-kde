//! The tag picker's state and messages, shared by the Filter popover's
//! "All tags…", the entry form's Tags and the plan dialog's linked
//! hashtags. The widget is `ui::tag_picker`; each caller keeps a
//! [`PickerState`] and maps [`PickerMessage`] into its own messages.

use std::collections::BTreeSet;

use iced::Task;
use iced::widget::{operation, scrollable};

use crate::model::TagCategory;

/// A group with more tags than this is one row that opens the full list.
pub const LONG_GROUP: usize = 24;

/// The long group's list, for the letter rail's jumps.
pub const DRILL_ID: &str = "tag-picker-drill";

/// What the picker shows: the search, the groups unfolded, the long group
/// drilled into, and the right-click menu (the entry form alone).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PickerState {
    pub query: String,
    /// The groups unfolded: those holding a picked tag when the picker
    /// opened (the plan dialog's Season), then as the headers are clicked.
    /// A tick never folds or unfolds a group.
    pub open: BTreeSet<TagCategory>,
    pub drill: Option<Drill>,
    /// The tag and the group whose chip was right-clicked.
    pub menu: Option<(String, TagCategory)>,
}

/// A long group opened in full: its category and the search typed there.
#[derive(Debug, Clone, PartialEq)]
pub struct Drill {
    pub category: TagCategory,
    pub query: String,
    /// The search was just emptied by a key. The field reports its text
    /// before the key reaches the app, so the Backspace that emptied the
    /// search must not also go back.
    pub emptied: bool,
}

/// The groups that start open: those holding a picked tag, as `is_on`
/// says it (asked with the group for a tag in several categories, with
/// none for a tag in one, like the picker's chips).
pub fn groups_with_picks<'a>(
    groups: impl IntoIterator<Item = (TagCategory, &'a [String])>,
    multi: impl Fn(&str) -> bool,
    is_on: impl Fn(&str, Option<TagCategory>) -> bool,
) -> BTreeSet<TagCategory> {
    groups
        .into_iter()
        .filter(|(category, tags)| {
            tags.iter()
                .any(|tag| is_on(tag, multi(tag).then_some(*category)))
        })
        .map(|(category, _)| category)
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerMessage {
    Query(String),
    ToggleGroup(TagCategory),
    /// A tag, as a category for a tag in several (`None` for one in one).
    Toggle(String, Option<TagCategory>),
    /// Opens a long group in full.
    Drill(TagCategory),
    /// Back from the long group to the list.
    Back,
    DrillQuery(String),
    /// The letter rail: scrolls the long group's list to this offset.
    Jump(f32),
    /// The right-click menu on a chip: open on a tag and group, or closed.
    Menu(Option<(String, TagCategory)>),
    /// The menu's "Categories…" or "Move to another group…".
    Categories(String),
    /// "+ Add to Directors": a tag of the kind found under another category.
    AddTo(String, TagCategory),
}

impl PickerState {
    /// A picker with `open` unfolded.
    pub fn opening(open: impl IntoIterator<Item = TagCategory>) -> Self {
        Self {
            open: open.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Handles the messages that only change the picker's own state.
    /// `Toggle`, `Categories`, `AddTo` and `Jump` are the caller's; the
    /// first three close the menu here.
    pub fn update(&mut self, message: &PickerMessage) {
        match message {
            PickerMessage::Query(query) => self.query.clone_from(query),
            PickerMessage::ToggleGroup(category) => {
                if !self.open.remove(category) {
                    self.open.insert(*category);
                }
            }
            PickerMessage::Drill(category) => {
                self.drill = Some(Drill {
                    category: *category,
                    query: String::new(),
                    emptied: false,
                });
            }
            PickerMessage::Back => self.drill = None,
            PickerMessage::DrillQuery(query) => {
                if let Some(drill) = &mut self.drill {
                    drill.emptied = !drill.query.is_empty() && query.is_empty();
                    drill.query.clone_from(query);
                }
            }
            PickerMessage::Menu(menu) => self.menu.clone_from(menu),
            PickerMessage::Toggle(..) | PickerMessage::Categories(_) | PickerMessage::AddTo(..) => {
                self.menu = None;
            }
            PickerMessage::Jump(_) => {}
        }
    }

    /// Unfolds `category` (and leaves the others as they are), after a tag
    /// was created or recategorized into it.
    pub fn open(&mut self, category: TagCategory) {
        self.open.insert(category);
    }

    /// Esc inside the picker: the menu, then the long group. False when
    /// neither was open.
    pub fn dismiss_inner(&mut self) -> bool {
        if self.menu.take().is_some() {
            return true;
        }
        self.back()
    }

    /// Back (the mouse button or Alt+Left): out of the long group. False
    /// when none was open.
    pub fn back(&mut self) -> bool {
        self.drill.take().is_some()
    }

    /// Backspace: out of the long group when its search is empty, so the
    /// key still deletes text while there is some. The Backspace that
    /// emptied the search stays in the group. False when it stayed.
    pub fn back_on_backspace(&mut self) -> bool {
        let Some(drill) = &mut self.drill else {
            return false;
        };
        let emptied = std::mem::take(&mut drill.emptied);
        if drill.query.is_empty() && !emptied && self.menu.is_none() {
            self.drill = None;
            return true;
        }
        false
    }
}

/// Scrolls the long group's list to `offset`.
pub fn jump<M: Send + 'static>(offset: f32) -> Task<M> {
    operation::scroll_to(
        DRILL_ID,
        scrollable::AbsoluteOffset {
            x: None,
            y: Some(offset),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_follows_its_own_messages_and_closes_the_menu_on_a_pick() {
        let mut state = PickerState::opening([TagCategory::Genres]);
        assert!(state.open.contains(&TagCategory::Genres));
        state.update(&PickerMessage::ToggleGroup(TagCategory::Mood));
        assert!(state.open.contains(&TagCategory::Mood));
        state.update(&PickerMessage::Toggle("cozy".into(), None));
        assert!(
            state.open.contains(&TagCategory::Mood),
            "a tick never folds"
        );
        state.update(&PickerMessage::ToggleGroup(TagCategory::Mood));
        assert!(!state.open.contains(&TagCategory::Mood));
        state.update(&PickerMessage::Drill(TagCategory::Authors));
        state.update(&PickerMessage::DrillQuery("king".into()));
        assert_eq!(state.drill.as_ref().unwrap().query, "king");
        state.update(&PickerMessage::Menu(Some((
            "horror".into(),
            TagCategory::Genres,
        ))));
        state.update(&PickerMessage::Toggle("horror".into(), None));
        assert!(state.menu.is_none());
        assert!(state.dismiss_inner(), "the long group closes");
        assert!(!state.dismiss_inner());
    }

    /// Backspace leaves the long group only with its search empty, and
    /// not with the press that emptied it; Back always does.
    #[test]
    fn backspace_and_back_leave_the_long_group() {
        let mut state = PickerState::default();
        assert!(!state.back_on_backspace());
        state.update(&PickerMessage::Drill(TagCategory::Authors));
        state.update(&PickerMessage::DrillQuery("k".into()));
        assert!(!state.back_on_backspace(), "deletes text instead");
        assert!(state.drill.is_some());
        state.update(&PickerMessage::DrillQuery(String::new()));
        assert!(!state.back_on_backspace(), "this press emptied the search");
        assert!(state.drill.is_some());
        // The field reports an empty search again on the next press.
        state.update(&PickerMessage::DrillQuery(String::new()));
        assert!(state.back_on_backspace());
        assert!(state.drill.is_none());

        state.update(&PickerMessage::Drill(TagCategory::Authors));
        assert!(state.back());
        assert!(!state.back());
    }

    #[test]
    fn the_groups_holding_a_pick_start_open() {
        let genres = vec!["horror".to_owned(), "drama".to_owned()];
        let people = vec!["clint-eastwood".to_owned()];
        let groups = [
            (TagCategory::Genres, genres.as_slice()),
            (TagCategory::Directors, people.as_slice()),
            (TagCategory::Actors, people.as_slice()),
        ];
        let open = groups_with_picks(
            groups,
            |tag| tag == "clint-eastwood",
            |tag, category| tag == "drama" || category == Some(TagCategory::Actors),
        );
        assert_eq!(
            open.into_iter().collect::<Vec<_>>(),
            [TagCategory::Genres, TagCategory::Actors]
        );
    }
}
