//! The `library.json` data model, version 2, shared with the phone app
//! (`ekmanch/library-app`, docs/DESIGN.md section 8).

pub mod codec;
mod date;
mod entry;
mod plan;
mod tag;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub use date::{Date, Released};
pub use entry::{BookFormat, Entry, EntryStatus, MediaType, MusicKind, Season, Track};
pub use plan::{Plan, PlanItem, PlanWhen};
pub use tag::TagCategory;

/// The whole `library.json` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    #[serde(default = "Library::version")]
    pub version: u32,
    /// Ordered by category, which is the order the file is written in.
    #[serde(default)]
    pub tags: BTreeMap<TagCategory, Vec<String>>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub plans: Vec<Plan>,
}

impl Default for Library {
    fn default() -> Self {
        Library {
            version: Library::version(),
            tags: BTreeMap::new(),
            entries: Vec::new(),
            plans: Vec::new(),
        }
    }
}

impl Library {
    fn version() -> u32 {
        2
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn plan(&self, id: &str) -> Option<&Plan> {
        self.plans.iter().find(|plan| plan.id == id)
    }

    pub fn entries_of(&self, media: MediaType) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(move |entry| entry.media == media)
    }

    /// Entries of `media` (every type when `None`), seasonal-only ones
    /// left out as the main lists do.
    pub fn count(&self, media: Option<MediaType>) -> usize {
        self.entries
            .iter()
            .filter(|e| !e.seasonal_only && media.is_none_or(|m| e.media == m))
            .count()
    }

    /// Every tag on an entry with the number of entries that have it, most
    /// used first, ties A to Z.
    pub fn tag_counts(&self) -> Vec<(&str, usize)> {
        let mut counts = BTreeMap::<&str, usize>::new();
        for tag in self.entries.iter().flat_map(|entry| &entry.tags) {
            *counts.entry(tag).or_default() += 1;
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        counts
    }

    /// Tags used by at least one entry or one plan.
    pub fn used_tags(&self) -> BTreeSet<&str> {
        let entry_tags = self.entries.iter().flat_map(|entry| &entry.tags);
        let plan_tags = self.plans.iter().flat_map(|plan| &plan.tags);
        entry_tags.chain(plan_tags).map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn the_fixture_decodes_into_the_model() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        assert_eq!(library.version, 2);
        for media in MediaType::ALL {
            assert!(library.entries_of(media).next().is_some(), "{media:?}");
        }
        let mummy = library.entry("movies/the-mummy-1999").unwrap();
        assert_eq!(mummy.rating, Some(9));
        assert_eq!(mummy.released, Released::parse("1999"));
        assert!(library.entries.iter().any(|e| e.tracks.is_some()));
        assert!(library.entries.iter().any(|e| e.seasons.is_some()));
        assert!(library.entries.iter().any(|e| e.formats.is_some()));
        assert_eq!(
            library.plan("halloween-2025").unwrap().title(),
            "Halloween 2025"
        );
    }

    #[test]
    fn counts_leave_out_seasonal_only_entries() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        assert_eq!(library.count(Some(MediaType::Music)), 12);
        assert_eq!(library.count(None), 52);
    }

    #[test]
    fn tag_counts_are_most_used_first_then_a_to_z() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        let counts = library.tag_counts();
        assert_eq!(counts.len(), 39);
        assert_eq!(
            &counts[..3],
            &[("halloween", 16), ("classic", 12), ("favorite", 12)]
        );
    }

    #[test]
    fn every_plan_item_points_at_an_entry() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        for plan in &library.plans {
            for item in &plan.items {
                assert!(library.entry(&item.entry).is_some(), "{}", item.entry);
            }
        }
    }
}
