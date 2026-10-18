use crate::fixtures;
use crate::model::{Library, codec};

use super::{LibraryRepository, Operation, RepoError};

/// Loads the bundled fixture and keeps every change in memory only.
/// Nothing is ever written anywhere.
#[derive(Debug, Clone)]
pub struct FixtureLibraryRepository {
    library: Library,
}

impl FixtureLibraryRepository {
    pub fn load() -> Result<Self, serde_json::Error> {
        Ok(FixtureLibraryRepository {
            library: codec::decode(fixtures::LIBRARY_JSON)?,
        })
    }
}

impl LibraryRepository for FixtureLibraryRepository {
    fn library(&self) -> &Library {
        &self.library
    }

    fn apply(&mut self, operation: Operation) -> Result<&Library, RepoError> {
        // Work on a copy so a failed change leaves nothing half done.
        let mut next = self.library.clone();
        operation.apply_to(&mut next)?;
        self.library = next;
        Ok(&self.library)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntryStatus;

    const MUMMY: &str = "movies/the-mummy-1999";

    fn repo() -> FixtureLibraryRepository {
        FixtureLibraryRepository::load().unwrap()
    }

    #[test]
    fn set_status_and_rating_change_only_that_entry() {
        let mut repo = repo();
        let before = repo.library().clone();
        repo.apply(Operation::SetStatus {
            id: MUMMY.into(),
            status: EntryStatus::Want,
        })
        .unwrap();
        let library = repo
            .apply(Operation::SetRating {
                id: MUMMY.into(),
                rating: None,
            })
            .unwrap();
        let mummy = library.entry(MUMMY).unwrap();
        assert_eq!((mummy.status, mummy.rating), (EntryStatus::Want, None));
        let others = |l: &Library| {
            l.entries
                .iter()
                .filter(|e| e.id != MUMMY)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(others(repo.library()), others(&before));
    }

    #[test]
    fn delete_entry_removes_it_from_every_plan() {
        let mut repo = repo();
        let id = "movies/the-thing-1982";
        assert!(
            repo.library()
                .plans
                .iter()
                .any(|p| p.item_for(id).is_some())
        );
        let library = repo
            .apply(Operation::DeleteEntry { id: id.into() })
            .unwrap();
        assert!(library.entry(id).is_none());
        assert!(library.plans.iter().all(|p| p.item_for(id).is_none()));
    }

    #[test]
    fn a_failed_change_leaves_the_library_as_it_was() {
        let mut repo = repo();
        let before = repo.library().clone();
        let duplicate = before.entry(MUMMY).unwrap().clone();
        assert_eq!(
            repo.apply(Operation::AddEntry(duplicate)),
            Err(RepoError::DuplicateEntry(MUMMY.into()))
        );
        assert_eq!(
            repo.apply(Operation::SetRating {
                id: MUMMY.into(),
                rating: Some(11)
            }),
            Err(RepoError::InvalidRating(11))
        );
        assert_eq!(
            repo.apply(Operation::SetPlanItemDone {
                plan: "nope".into(),
                entry: MUMMY.into(),
                done: true
            }),
            Err(RepoError::MissingPlan("nope".into()))
        );
        assert_eq!(repo.library(), &before);
    }

    #[test]
    fn set_plan_item_done_marks_the_item() {
        let mut repo = repo();
        let entry = "books/dracula-1897";
        let library = repo
            .apply(Operation::SetPlanItemDone {
                plan: "halloween-2025".into(),
                entry: entry.into(),
                done: false,
            })
            .unwrap();
        assert!(
            !library
                .plan("halloween-2025")
                .unwrap()
                .item_for(entry)
                .unwrap()
                .done
        );
    }
}
