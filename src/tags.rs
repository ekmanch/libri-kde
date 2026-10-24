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

/// What the picker shows: the search, the groups folded or unfolded by
/// hand, the long group drilled into, and the right-click menu (the entry
/// form alone).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PickerState {
    pub query: String,
    /// Groups folded or unfolded by hand, against their default.
    pub toggled: BTreeSet<TagCategory>,
    pub drill: Option<Drill>,
    /// The tag and the group whose chip was right-clicked.
    pub menu: Option<(String, TagCategory)>,
}

/// A long group opened in full: its category and the search typed there.
#[derive(Debug, Clone, PartialEq)]
pub struct Drill {
    pub category: TagCategory,
    pub query: String,
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
    /// Handles the messages that only change the picker's own state.
    /// `Toggle`, `Categories`, `AddTo` and `Jump` are the caller's; the
    /// first three close the menu here.
    pub fn update(&mut self, message: &PickerMessage) {
        match message {
            PickerMessage::Query(query) => self.query.clone_from(query),
            PickerMessage::ToggleGroup(category) => {
                if !self.toggled.remove(category) {
                    self.toggled.insert(*category);
                }
            }
            PickerMessage::Drill(category) => {
                self.drill = Some(Drill {
                    category: *category,
                    query: String::new(),
                });
            }
            PickerMessage::Back => self.drill = None,
            PickerMessage::DrillQuery(query) => {
                if let Some(drill) = &mut self.drill {
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

    /// Opens `category` (and leaves the others as they are), after a tag
    /// was created or recategorized into it.
    pub fn open(&mut self, category: TagCategory, default_open: bool) {
        if default_open {
            self.toggled.remove(&category);
        } else {
            self.toggled.insert(category);
        }
    }

    /// Esc inside the picker: the menu, then the long group. False when
    /// neither was open.
    pub fn dismiss_inner(&mut self) -> bool {
        if self.menu.take().is_some() {
            return true;
        }
        self.drill.take().is_some()
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
        let mut state = PickerState::default();
        state.update(&PickerMessage::ToggleGroup(TagCategory::Mood));
        assert!(state.toggled.contains(&TagCategory::Mood));
        state.update(&PickerMessage::ToggleGroup(TagCategory::Mood));
        assert!(state.toggled.is_empty());
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
}
