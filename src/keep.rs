//! The pictures kept decoded. iced's renderer frees an image the moment
//! another arrives while it was not drawn, so a list that comes back into
//! view (the tab beside the picked one, the rows a screen further down)
//! had to read and decode its covers again, and they popped in after the
//! list did. An `Allocation` of an image keeps it in the renderer's
//! memory and draws it on the very next frame; this holds one for every
//! picture the screens say should be ready (`screens::kept`): the covers
//! on screen, a screen either side of them, and what the tabs or slices
//! beside the picked one show where they were left.
//!
//! Only what is near the screen is held, never the library: a decoded
//! cover takes about a megabyte of the graphics card's memory.

use std::collections::{HashMap, HashSet};

use iced::Task;
use iced::advanced::image::Id;
use iced::widget::image::{self, Allocation, Handle};

/// What a batch of allocations came to: each picture's id and its
/// allocation, or none when the file could not be read.
pub type Batch = Vec<(Id, Option<Allocation>)>;

#[derive(Debug, Default)]
pub struct Kept {
    /// The pictures to keep now.
    wanted: HashSet<Id>,
    held: HashMap<Id, Allocation>,
    /// Asked of the renderer, not answered yet.
    asked: HashSet<Id>,
    /// Files the renderer could not read; not asked for again.
    failed: HashSet<Id>,
    /// The pictures of the lists beside the one on show, as last worked
    /// out: they change with the list, not with its scrolling.
    pub beside: Vec<Handle>,
}

impl Kept {
    /// Keeps `handles` and lets go of every other picture. Returns the
    /// ones to ask the renderer for: not held, not on their way.
    pub fn set(&mut self, handles: impl IntoIterator<Item = Handle>) -> Vec<Handle> {
        let mut ask = Vec::new();
        self.wanted.clear();
        for handle in handles {
            let id = handle.id();
            if self.wanted.insert(id)
                && !self.held.contains_key(&id)
                && !self.failed.contains(&id)
                && self.asked.insert(id)
            {
                ask.push(handle);
            }
        }
        let wanted = &self.wanted;
        self.held.retain(|id, _| wanted.contains(id));
        ask
    }

    /// The renderer's answer: what is still wanted is held, the rest is
    /// let go at once.
    pub fn arrived(&mut self, batch: Batch) {
        for (id, allocation) in batch {
            self.asked.remove(&id);
            match allocation {
                Some(allocation) if self.wanted.contains(&id) => {
                    self.held.insert(id, allocation);
                }
                Some(_) => {}
                None => {
                    self.failed.insert(id);
                }
            }
        }
    }

    /// How many pictures are held.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

/// Asks the renderer to load `handles` and hand over their allocations,
/// all in one answer: a message each would rebuild the window once per
/// picture.
pub fn allocate(handles: Vec<Handle>) -> Task<Batch> {
    if handles.is_empty() {
        return Task::none();
    }
    Task::batch(handles.into_iter().map(|handle| {
        let id = handle.id();
        image::allocate(handle).map(move |result| (id, result.ok()))
    }))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(name: &str) -> Handle {
        Handle::from_path(format!("/nowhere/{name}.jpg"))
    }

    #[test]
    fn a_picture_is_asked_for_once_and_let_go_when_no_longer_wanted() {
        let mut kept = Kept::default();
        let (a, b, c) = (handle("a"), handle("b"), handle("c"));
        let ask = kept.set([a.clone(), b.clone(), a.clone()]);
        assert_eq!(ask.len(), 2, "each once");
        assert!(kept.set([a.clone(), b.clone()]).is_empty(), "on their way");
        assert!(kept.is_empty());

        // `b` could not be read: it is not asked for again. Nothing can
        // make an `Allocation` here but the renderer, so the held side is
        // checked through what is asked for.
        kept.arrived(vec![(b.id(), None)]);
        let ask = kept.set([a.clone(), b.clone(), c.clone()]);
        assert_eq!(ask.len(), 1, "only the new one");
        assert_eq!(ask[0].id(), c.id());

        // No longer wanted: an answer that comes late asks again when the
        // picture is wanted once more.
        kept.set([c.clone()]);
        kept.arrived(vec![(a.id(), None), (c.id(), None)]);
        assert!(kept.set([a, c]).is_empty(), "both failed");
        assert_eq!(kept.len(), 0);
    }
}
