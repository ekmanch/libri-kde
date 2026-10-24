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
    use crate::model::{EntryStatus, PlanItem, TagCategory};

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

    #[test]
    fn plan_items_are_added_replaced_and_removed() {
        let mut repo = repo();
        let item = PlanItem {
            entry: MUMMY.into(),
            done: false,
            repeat: true,
            note: String::new(),
            when: None,
        };
        let plan = || "christmas-2026".to_owned();
        repo.apply(Operation::SetPlanItem {
            plan: plan(),
            item: item.clone(),
        })
        .unwrap();
        let library = repo
            .apply(Operation::SetPlanItem {
                plan: plan(),
                item: PlanItem {
                    note: "Again".into(),
                    ..item
                },
            })
            .unwrap();
        let christmas = library.plan("christmas-2026").unwrap();
        assert_eq!(
            christmas.items.iter().filter(|i| i.entry == MUMMY).count(),
            1
        );
        assert_eq!(christmas.item_for(MUMMY).unwrap().note, "Again");
        let library = repo
            .apply(Operation::RemovePlanItem {
                plan: plan(),
                entry: MUMMY.into(),
            })
            .unwrap();
        assert!(
            library
                .plan("christmas-2026")
                .unwrap()
                .item_for(MUMMY)
                .is_none()
        );
    }

    #[test]
    fn a_new_tag_is_filed_once() {
        let mut repo = repo();
        let create = |name: &str| Operation::CreateTag {
            name: name.into(),
            categories: vec![TagCategory::Mood],
        };
        let library = repo.apply(create("eerie")).unwrap();
        assert_eq!(library.category_of("eerie"), Some(TagCategory::Mood));
        assert_eq!(
            repo.apply(create("horror")),
            Err(RepoError::DuplicateTag("horror".into()))
        );
    }

    #[test]
    fn track_ratings_are_added_changed_and_cleared() {
        let mut repo = repo();
        let dirt = "music/dirt-alice-in-chains";
        let rate = |title: &str, rating| Operation::SetTrackRating {
            album: dirt.into(),
            title: title.into(),
            rating,
        };
        repo.apply(rate("Would?", Some(10))).unwrap();
        repo.apply(rate("rooster", Some(8))).unwrap();
        let library = repo.apply(rate("Them Bones", None)).unwrap();
        let tracks = library.entry(dirt).unwrap().tracks.clone().unwrap();
        let titles: Vec<_> = tracks
            .iter()
            .map(|t| (t.title.as_str(), t.rating))
            .collect();
        assert_eq!(
            titles,
            [("Rooster", 8), ("Down in a Hole", 9), ("Would?", 10)]
        );
    }

    #[test]
    fn plans_are_added_updated_and_deleted_and_a_tick_starts_one() {
        let mut repo = repo();
        let mut plan = repo.library().plan("christmas-2026").unwrap().clone();
        assert!(!plan.started);
        let library = repo
            .apply(Operation::SetPlanItemDone {
                plan: plan.id.clone(),
                entry: plan.items[0].entry.clone(),
                done: true,
            })
            .unwrap();
        assert!(library.plan("christmas-2026").unwrap().started);

        plan.id = "christmas-2027".into();
        plan.year = Some(2027);
        repo.apply(Operation::AddPlan(plan.clone())).unwrap();
        assert_eq!(
            repo.apply(Operation::AddPlan(plan.clone())),
            Err(RepoError::DuplicatePlan(plan.id.clone()))
        );
        plan.name = "Xmas".into();
        let library = repo.apply(Operation::UpdatePlan(plan.clone())).unwrap();
        assert_eq!(library.plan("christmas-2027").unwrap().name, "Xmas");
        let library = repo.apply(Operation::DeletePlan { id: plan.id }).unwrap();
        assert!(library.plan("christmas-2027").is_none());
        assert!(
            library.entry("movies/die-hard-1988").is_some(),
            "entries stay"
        );
    }

    #[test]
    fn a_tag_can_be_created_in_two_categories_of_one_kind() {
        let mut repo = repo();
        let library = repo
            .apply(Operation::CreateTag {
                name: "gothic".into(),
                categories: vec![TagCategory::MusicGenres, TagCategory::Genres],
            })
            .unwrap();
        assert_eq!(
            library.categories_of("gothic"),
            [TagCategory::Genres, TagCategory::MusicGenres],
            "in category order"
        );
        assert_eq!(
            repo.apply(Operation::CreateTag {
                name: "empty".into(),
                categories: Vec::new(),
            }),
            Err(RepoError::NoCategory("empty".into()))
        );
    }

    #[test]
    fn taking_a_role_away_drops_the_tag_where_it_had_only_that_role() {
        let mut repo = repo();
        let library = repo
            .apply(Operation::SetTagCategories {
                name: "clint-eastwood".into(),
                categories: vec![TagCategory::Directors],
            })
            .unwrap();
        assert_eq!(
            library.categories_of("clint-eastwood"),
            [TagCategory::Directors]
        );
        let baby = library.entry("movies/million-dollar-baby-2004").unwrap();
        assert!(baby.tags.iter().any(|t| t == "clint-eastwood"));
        assert!(baby.tag_categories.is_none(), "one category: no pick");
        let ugly = library
            .entry("movies/the-good-the-bad-and-the-ugly-1966")
            .unwrap();
        assert!(
            !ugly.tags.iter().any(|t| t == "clint-eastwood"),
            "he was only an actor there"
        );
        assert!(ugly.tag_categories.is_none());
    }

    #[test]
    fn adding_a_role_keeps_every_entry_as_it_was() {
        let mut repo = repo();
        let library = repo
            .apply(Operation::SetTagCategories {
                name: "hilary-swank".into(),
                categories: vec![TagCategory::Actors, TagCategory::Directors],
            })
            .unwrap();
        let baby = library.entry("movies/million-dollar-baby-2004").unwrap();
        assert_eq!(
            library.categories_on(baby, "hilary-swank"),
            [TagCategory::Actors],
            "the new role is not ticked anywhere"
        );
    }

    #[test]
    fn moving_a_one_category_tag_keeps_every_entry() {
        let mut repo = repo();
        let before = repo.library().tag_counts();
        let classic = before.iter().find(|(t, _)| *t == "classic").unwrap().1;
        let library = repo
            .apply(Operation::SetTagCategories {
                name: "classic".into(),
                categories: vec![TagCategory::Languages],
            })
            .unwrap();
        assert_eq!(library.categories_of("classic"), [TagCategory::Languages]);
        let after = library.tag_counts();
        assert_eq!(
            after.iter().find(|(t, _)| *t == "classic").unwrap().1,
            classic
        );
        assert_eq!(
            repo.apply(Operation::SetTagCategories {
                name: "nobody".into(),
                categories: vec![TagCategory::Other],
            }),
            Err(RepoError::MissingTag("nobody".into()))
        );
    }
}
