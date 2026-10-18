//! All reading and changing of the library goes through
//! [`LibraryRepository`]. Screens never know which implementation they
//! have: the fixture in phase 1, a local file in phase 2, GitHub in phase 3.

mod fixture;

use std::fmt;

pub use fixture::FixtureLibraryRepository;

use crate::model::{Entry, EntryStatus, Library};

pub trait LibraryRepository {
    /// The current library.
    fn library(&self) -> &Library;

    /// Applies one change and returns the resulting library. A change that
    /// cannot apply leaves the library as it was.
    fn apply(&mut self, operation: Operation) -> Result<&Library, RepoError>;
}

/// The starter set of changes. Each is small and named so phase 2 can give
/// it a readable summary and re-apply it after a sync conflict. None of
/// them reorders existing entries or plans.
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    AddEntry(Entry),
    /// Replaces the entry with the same id, in place.
    UpdateEntry(Entry),
    /// Removes the entry and its items in every plan.
    DeleteEntry {
        id: String,
    },
    SetStatus {
        id: String,
        status: EntryStatus,
    },
    SetRating {
        id: String,
        rating: Option<u8>,
    },
    SetPlanItemDone {
        plan: String,
        entry: String,
        done: bool,
    },
}

impl Operation {
    pub fn apply_to(self, library: &mut Library) -> Result<(), RepoError> {
        match self {
            Operation::AddEntry(entry) => {
                if library.entry(&entry.id).is_some() {
                    return Err(RepoError::DuplicateEntry(entry.id));
                }
                library.entries.push(entry);
            }
            Operation::UpdateEntry(entry) => {
                let id = entry.id.clone();
                *entry_mut(library, &id)? = entry;
            }
            Operation::DeleteEntry { id } => {
                entry_mut(library, &id)?;
                library.entries.retain(|entry| entry.id != id);
                for plan in &mut library.plans {
                    plan.items.retain(|item| item.entry != id);
                }
            }
            Operation::SetStatus { id, status } => entry_mut(library, &id)?.status = status,
            Operation::SetRating { id, rating } => {
                if rating.is_some_and(|r| !(1..=10).contains(&r)) {
                    return Err(RepoError::InvalidRating(rating.unwrap_or_default()));
                }
                entry_mut(library, &id)?.rating = rating;
            }
            Operation::SetPlanItemDone { plan, entry, done } => {
                let found = library.plans.iter_mut().find(|p| p.id == plan);
                let plan = found.ok_or(RepoError::MissingPlan(plan))?;
                let found = plan.items.iter_mut().find(|item| item.entry == entry);
                found.ok_or(RepoError::MissingEntry(entry))?.done = done;
            }
        }
        Ok(())
    }
}

fn entry_mut<'a>(library: &'a mut Library, id: &str) -> Result<&'a mut Entry, RepoError> {
    let found = library.entries.iter_mut().find(|entry| entry.id == id);
    found.ok_or_else(|| RepoError::MissingEntry(id.to_owned()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoError {
    MissingEntry(String),
    MissingPlan(String),
    DuplicateEntry(String),
    InvalidRating(u8),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::MissingEntry(id) => write!(f, "no entry {id}"),
            RepoError::MissingPlan(id) => write!(f, "no plan {id}"),
            RepoError::DuplicateEntry(id) => write!(f, "an entry {id} already exists"),
            RepoError::InvalidRating(rating) => write!(f, "rating {rating} is not 1 to 10"),
        }
    }
}

impl std::error::Error for RepoError {}
