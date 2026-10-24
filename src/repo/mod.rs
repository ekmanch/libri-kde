//! All reading and changing of the library goes through
//! [`LibraryRepository`]. Screens never know which implementation they
//! have: the fixture in phase 1, a local file in phase 2, GitHub in phase 3.

mod fixture;

use std::fmt;

pub use fixture::FixtureLibraryRepository;

use crate::model::{
    Date, Entry, EntryStatus, Library, MediaType, Plan, PlanItem, Season, TagCategory, Track,
};

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
    AddPlan(Plan),
    /// Replaces the plan with the same id, in place.
    UpdatePlan(Plan),
    /// Removes the plan; its entries stay in the library.
    DeletePlan {
        id: String,
    },
    /// Files a new tag under one or more categories of one kind. Only the
    /// entry form creates tags, right before the entry that uses it is
    /// saved.
    CreateTag {
        name: String,
        categories: Vec<TagCategory>,
    },
    /// Changes the categories a tag is in. A category taken away takes the
    /// role off every entry, and an entry left with the tag in no category
    /// loses the tag; a move between one-category groups keeps every entry.
    /// A tag back to one category has its picks removed everywhere.
    SetTagCategories {
        name: String,
        categories: Vec<TagCategory>,
    },
    /// Adds the item to the plan, or replaces the entry's item there.
    SetPlanItem {
        plan: String,
        item: PlanItem,
    },
    RemovePlanItem {
        plan: String,
        entry: String,
    },
    /// Rates a track on an album, or clears it: only rated tracks are
    /// stored. A track is matched by title, case ignored.
    SetTrackRating {
        album: String,
        title: String,
        rating: Option<u8>,
    },
    /// Sets one season's status (`None` clears it); the show's status
    /// follows. A season given a status is no longer "new".
    SetSeasonStatus {
        id: String,
        season: u32,
        status: Option<EntryStatus>,
        today: Date,
    },
    /// Catching up: marks every season before `before` Done.
    MarkSeasonsDone {
        id: String,
        before: u32,
        today: Date,
    },
    /// Replaces a show's seasons (the form, or a plan's untick putting
    /// them back); the show's status follows.
    SetSeasons {
        id: String,
        seasons: Vec<Season>,
        today: Date,
    },
    /// Removes a season the lookup dropped.
    RemoveSeason {
        id: String,
        season: u32,
        today: Date,
    },
    /// Keeps a season the lookup dropped: marks it manual, so it is never
    /// asked about again.
    KeepSeason {
        id: String,
        season: u32,
    },
    /// The part whose cover stands for its series in the library list:
    /// `cover` set on `id`, cleared on the series' other parts.
    SetSeriesCover {
        id: String,
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
                library.remove_unused_tags();
            }
            Operation::DeleteEntry { id } => {
                entry_mut(library, &id)?;
                library.entries.retain(|entry| entry.id != id);
                for plan in &mut library.plans {
                    plan.items.retain(|item| item.entry != id);
                }
                library.remove_unused_tags();
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
                // Ticking an item also turns Started on, as on the phone.
                if done {
                    plan.started = true;
                }
            }
            Operation::AddPlan(plan) => {
                if library.plan(&plan.id).is_some() {
                    return Err(RepoError::DuplicatePlan(plan.id));
                }
                library.plans.push(plan);
            }
            Operation::UpdatePlan(plan) => {
                let id = plan.id.clone();
                *plan_mut(library, &id)? = plan;
                library.remove_unused_tags();
            }
            Operation::DeletePlan { id } => {
                plan_mut(library, &id)?;
                library.plans.retain(|plan| plan.id != id);
                library.remove_unused_tags();
            }
            Operation::CreateTag { name, categories } => {
                if library.all_tags().any(|t| t == name) {
                    return Err(RepoError::DuplicateTag(name));
                }
                if categories.is_empty() {
                    return Err(RepoError::NoCategory(name));
                }
                for category in in_order(&categories) {
                    library.tags.entry(category).or_default().push(name.clone());
                }
            }
            Operation::SetTagCategories { name, categories } => {
                let before = library.categories_of(&name);
                let after = in_order(&categories);
                if before.is_empty() {
                    return Err(RepoError::MissingTag(name));
                }
                if after.is_empty() {
                    return Err(RepoError::NoCategory(name));
                }
                if before == after {
                    return Ok(());
                }
                for category in TagCategory::ALL {
                    let listed = library
                        .tags
                        .get(&category)
                        .is_some_and(|names| names.contains(&name));
                    if after.contains(&category) && !listed {
                        library.tags.entry(category).or_default().push(name.clone());
                    } else if !after.contains(&category) && listed {
                        let names = library.tags.get_mut(&category).expect("listed");
                        names.retain(|n| *n != name);
                        if names.is_empty() {
                            library.tags.remove(&category);
                        }
                    }
                }
                for entry in &mut library.entries {
                    retag(entry, &name, &before, &after);
                }
                library.remove_unused_tags();
            }
            Operation::SetPlanItem { plan, item } => {
                let plan = plan_mut(library, &plan)?;
                match plan.items.iter_mut().find(|i| i.entry == item.entry) {
                    Some(existing) => *existing = item,
                    None => plan.items.push(item),
                }
            }
            Operation::SetTrackRating {
                album,
                title,
                rating,
            } => {
                if rating.is_some_and(|r| !(1..=10).contains(&r)) {
                    return Err(RepoError::InvalidRating(rating.unwrap_or_default()));
                }
                let album = entry_mut(library, &album)?;
                let tracks = album.tracks.get_or_insert_with(Vec::new);
                let found = tracks
                    .iter()
                    .position(|t| t.title.eq_ignore_ascii_case(&title));
                match (found, rating) {
                    (Some(index), Some(rating)) => tracks[index].rating = rating,
                    (Some(index), None) => {
                        tracks.remove(index);
                    }
                    (None, Some(rating)) => tracks.push(Track {
                        title,
                        id: None,
                        rating,
                    }),
                    (None, None) => {}
                }
            }
            Operation::SetSeasonStatus {
                id,
                season,
                status,
                today,
            } => {
                let show = entry_mut(library, &id)?;
                let found = show
                    .seasons
                    .as_mut()
                    .and_then(|seasons| seasons.iter_mut().find(|s| s.season == season))
                    .ok_or(RepoError::MissingSeason(season))?;
                found.status = status;
                if status.is_some() {
                    found.fresh = None;
                }
                show.refresh_status(today);
            }
            Operation::MarkSeasonsDone { id, before, today } => {
                let show = entry_mut(library, &id)?;
                for season in show.seasons.iter_mut().flatten() {
                    if season.season < before {
                        season.status = Some(EntryStatus::Done);
                        season.fresh = None;
                    }
                }
                show.refresh_status(today);
            }
            Operation::SetSeasons { id, seasons, today } => {
                let show = entry_mut(library, &id)?;
                let mut seasons = seasons;
                seasons.sort_by_key(|s| s.season);
                show.seasons = (!seasons.is_empty()).then_some(seasons);
                show.refresh_status(today);
            }
            Operation::RemoveSeason { id, season, today } => {
                let show = entry_mut(library, &id)?;
                let seasons = show.seasons.get_or_insert_with(Vec::new);
                let before = seasons.len();
                seasons.retain(|s| s.season != season);
                if seasons.len() == before {
                    return Err(RepoError::MissingSeason(season));
                }
                if seasons.is_empty() {
                    show.seasons = None;
                }
                show.refresh_status(today);
            }
            Operation::KeepSeason { id, season } => {
                let show = entry_mut(library, &id)?;
                let found = show
                    .seasons
                    .as_mut()
                    .and_then(|seasons| seasons.iter_mut().find(|s| s.season == season))
                    .ok_or(RepoError::MissingSeason(season))?;
                found.manual = Some(true);
            }
            Operation::SetSeriesCover { id } => {
                let part = entry_mut(library, &id)?;
                let Some(series) = &part.series else {
                    return Err(RepoError::NotInSeries(id));
                };
                let (media, name): (MediaType, String) = (part.media, series.name.clone());
                for entry in &mut library.entries {
                    if entry.media != media {
                        continue;
                    }
                    if let Some(series) = &mut entry.series
                        && crate::model::Series::same(&series.name, &name)
                    {
                        series.cover = (entry.id == id).then_some(true);
                    }
                }
            }
            Operation::RemovePlanItem { plan, entry } => {
                let plan = plan_mut(library, &plan)?;
                let before = plan.items.len();
                plan.items.retain(|item| item.entry != entry);
                if plan.items.len() == before {
                    return Err(RepoError::MissingEntry(entry));
                }
            }
        }
        Ok(())
    }
}

/// `categories` without repeats, in category order.
fn in_order(categories: &[TagCategory]) -> Vec<TagCategory> {
    TagCategory::ALL
        .into_iter()
        .filter(|c| categories.contains(c))
        .collect()
}

/// An entry's share of a tag's category change, as the phone's
/// `SetTagCategories` does it.
fn retag(entry: &mut Entry, name: &str, before: &[TagCategory], after: &[TagCategory]) {
    if !entry.tags.iter().any(|t| t == name) {
        return;
    }
    if before.len() == 1 && after.len() == 1 {
        // A move: the tag is on the entry as its one category, whatever
        // it was.
        entry.set_tag_pick(name, Vec::new());
        return;
    }
    let had = entry.categories_of(name, before);
    let kept: Vec<TagCategory> = after.iter().copied().filter(|c| had.contains(c)).collect();
    if kept.is_empty() {
        entry.tags.retain(|t| t != name);
        entry.set_tag_pick(name, Vec::new());
    } else if after.len() == 1 {
        entry.set_tag_pick(name, Vec::new());
    } else {
        entry.set_tag_pick(name, kept);
    }
}

fn plan_mut<'a>(library: &'a mut Library, id: &str) -> Result<&'a mut Plan, RepoError> {
    let found = library.plans.iter_mut().find(|plan| plan.id == id);
    found.ok_or_else(|| RepoError::MissingPlan(id.to_owned()))
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
    DuplicateTag(String),
    MissingTag(String),
    NoCategory(String),
    DuplicatePlan(String),
    InvalidRating(u8),
    MissingSeason(u32),
    NotInSeries(String),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::MissingEntry(id) => write!(f, "no entry {id}"),
            RepoError::MissingPlan(id) => write!(f, "no plan {id}"),
            RepoError::DuplicateEntry(id) => write!(f, "an entry {id} already exists"),
            RepoError::DuplicateTag(name) => write!(f, "a tag #{name} already exists"),
            RepoError::MissingTag(name) => write!(f, "no tag #{name}"),
            RepoError::NoCategory(name) => write!(f, "#{name} needs at least one category"),
            RepoError::DuplicatePlan(id) => write!(f, "a plan {id} already exists"),
            RepoError::InvalidRating(rating) => write!(f, "rating {rating} is not 1 to 10"),
            RepoError::MissingSeason(n) => write!(f, "no season {n}"),
            RepoError::NotInSeries(id) => write!(f, "{id} is in no series"),
        }
    }
}

impl std::error::Error for RepoError {}
