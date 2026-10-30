//! Operations as JSON, for the offline queue (`pending.json`). The `op`
//! key names the operation with the phone's name where the phone has the
//! same operation (`operation_json.dart`); the rest is the operation's
//! own fields, with the keys in alphabetical order as serde_json writes
//! a map. The queue is read by this computer alone, so an operation
//! only the desktop has (`setPlanItemDone`) and a field only it needs
//! (`today` on the season operations) are written as they are.

use serde_json::{Map, Value, json};

use crate::model::{Date, Entry, EntryStatus, Plan, PlanItem, Season, TagCategory, TagPicks};

use super::Operation;

/// The operation as a JSON object: `op` and the fields.
pub fn encode(operation: &Operation) -> Value {
    let (name, fields) = match operation {
        Operation::AddEntry(entry) => ("addEntry", json!({ "entry": entry })),
        Operation::UpdateEntry(entry) => ("updateEntry", json!({ "entry": entry })),
        Operation::DeleteEntry { id } => ("deleteEntry", json!({ "entryId": id })),
        Operation::SetStatus { id, status } => {
            ("setStatus", json!({ "entryId": id, "status": status }))
        }
        Operation::SetRating { id, rating } => {
            ("setRating", json!({ "entryId": id, "rating": rating }))
        }
        Operation::SetTags {
            id,
            tags,
            tag_categories,
        } => (
            "setTags",
            json!({ "entryId": id, "tags": tags, "tagCategories": tag_categories }),
        ),
        Operation::SetNote { id, note } => ("setNote", json!({ "entryId": id, "note": note })),
        Operation::SetPlanItemDone { plan, entry, done } => (
            "setPlanItemDone",
            json!({ "planId": plan, "entryId": entry, "done": done }),
        ),
        Operation::AddPlan(plan) => ("addPlan", json!({ "plan": plan })),
        Operation::UpdatePlan(plan) => ("updatePlan", json!({ "plan": plan })),
        Operation::DeletePlan { id } => ("deletePlan", json!({ "planId": id })),
        Operation::CreateTag { name, categories } => {
            ("addTag", json!({ "name": name, "categories": categories }))
        }
        Operation::SetTagCategories { name, categories } => (
            "setTagCategories",
            json!({ "name": name, "categories": categories }),
        ),
        Operation::SetPlanItem { plan, item } => {
            ("setPlanItem", json!({ "planId": plan, "item": item }))
        }
        Operation::RemovePlanItem { plan, entry } => (
            "removePlanItem",
            json!({ "planId": plan, "entryId": entry }),
        ),
        Operation::SetTrackRating {
            album,
            title,
            rating: Some(rating),
        } => (
            "rateTrack",
            json!({ "entryId": album, "track": { "title": title, "rating": rating } }),
        ),
        Operation::SetTrackRating {
            album,
            title,
            rating: None,
        } => (
            "clearTrackRating",
            json!({ "entryId": album, "trackId": null, "title": title }),
        ),
        Operation::SetSeasonStatus {
            id,
            season,
            status,
            today,
        } => (
            "setSeasonStatus",
            json!({ "entryId": id, "season": season, "status": status, "today": today }),
        ),
        Operation::MarkSeasonsDone { id, before, today } => (
            "markSeasonsDone",
            json!({ "entryId": id, "before": before, "today": today }),
        ),
        Operation::SetSeasons { id, seasons, today } => (
            "setSeasons",
            json!({ "entryId": id, "seasons": seasons, "today": today }),
        ),
        Operation::RemoveSeason { id, season, today } => (
            "removeSeason",
            json!({ "entryId": id, "season": season, "today": today }),
        ),
        Operation::KeepSeason { id, season } => {
            ("keepSeason", json!({ "entryId": id, "season": season }))
        }
        Operation::SetSeriesCover { id } => ("setSeriesCover", json!({ "entryId": id })),
    };
    let mut object = Map::new();
    object.insert("op".into(), Value::String(name.into()));
    if let Value::Object(fields) = fields {
        object.extend(fields);
    }
    Value::Object(object)
}

/// The inverse of [`encode`]. An unknown `op` or a missing field is an
/// error that names it.
pub fn decode(value: &Value) -> Result<Operation, String> {
    let op = value["op"].as_str().ok_or_else(|| "no \"op\"".to_owned())?;
    let string = |key: &str| -> Result<String, String> {
        value[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("{op}: no \"{key}\""))
    };
    let number = |key: &str| -> Result<u32, String> {
        value[key]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| format!("{op}: no \"{key}\""))
    };
    fn field<T: serde::de::DeserializeOwned>(
        op: &str,
        value: &Value,
        key: &str,
    ) -> Result<T, String> {
        serde_json::from_value(value[key].clone()).map_err(|e| format!("{op}: \"{key}\": {e}"))
    }
    // The day the change was made; a queue from before the field, or a
    // hand edit, takes today.
    let today = || {
        value["today"]
            .as_str()
            .and_then(Date::parse)
            .unwrap_or_else(Date::today)
    };
    Ok(match op {
        "addEntry" => Operation::AddEntry(field::<Entry>(op, value, "entry")?),
        "updateEntry" => Operation::UpdateEntry(field::<Entry>(op, value, "entry")?),
        "deleteEntry" => Operation::DeleteEntry {
            id: string("entryId")?,
        },
        "setStatus" => Operation::SetStatus {
            id: string("entryId")?,
            status: field::<EntryStatus>(op, value, "status")?,
        },
        "setRating" => Operation::SetRating {
            id: string("entryId")?,
            rating: field::<Option<u8>>(op, value, "rating")?,
        },
        "setTags" => Operation::SetTags {
            id: string("entryId")?,
            tags: field::<Vec<String>>(op, value, "tags")?,
            tag_categories: field::<Option<TagPicks>>(op, value, "tagCategories")?,
        },
        "setNote" => Operation::SetNote {
            id: string("entryId")?,
            note: string("note")?,
        },
        "setPlanItemDone" => Operation::SetPlanItemDone {
            plan: string("planId")?,
            entry: string("entryId")?,
            done: value["done"].as_bool().unwrap_or(false),
        },
        "addPlan" => Operation::AddPlan(field::<Plan>(op, value, "plan")?),
        "updatePlan" => Operation::UpdatePlan(field::<Plan>(op, value, "plan")?),
        "deletePlan" => Operation::DeletePlan {
            id: string("planId")?,
        },
        "addTag" => Operation::CreateTag {
            name: string("name")?,
            categories: field::<Vec<TagCategory>>(op, value, "categories")?,
        },
        "setTagCategories" => Operation::SetTagCategories {
            name: string("name")?,
            categories: field::<Vec<TagCategory>>(op, value, "categories")?,
        },
        "setPlanItem" => Operation::SetPlanItem {
            plan: string("planId")?,
            item: field::<PlanItem>(op, value, "item")?,
        },
        "removePlanItem" => Operation::RemovePlanItem {
            plan: string("planId")?,
            entry: string("entryId")?,
        },
        "rateTrack" => Operation::SetTrackRating {
            album: string("entryId")?,
            title: value["track"]["title"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{op}: no track title"))?,
            rating: Some(
                value["track"]["rating"]
                    .as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or_else(|| format!("{op}: no track rating"))?,
            ),
        },
        "clearTrackRating" => Operation::SetTrackRating {
            album: string("entryId")?,
            title: string("title")?,
            rating: None,
        },
        "setSeasonStatus" => Operation::SetSeasonStatus {
            id: string("entryId")?,
            season: number("season")?,
            status: field::<Option<EntryStatus>>(op, value, "status")?,
            today: today(),
        },
        "markSeasonsDone" => Operation::MarkSeasonsDone {
            id: string("entryId")?,
            before: number("before")?,
            today: today(),
        },
        "setSeasons" => Operation::SetSeasons {
            id: string("entryId")?,
            seasons: field::<Vec<Season>>(op, value, "seasons")?,
            today: today(),
        },
        "removeSeason" => Operation::RemoveSeason {
            id: string("entryId")?,
            season: number("season")?,
            today: today(),
        },
        "keepSeason" => Operation::KeepSeason {
            id: string("entryId")?,
            season: number("season")?,
        },
        "setSeriesCover" => Operation::SetSeriesCover {
            id: string("entryId")?,
        },
        other => return Err(format!("unknown operation \"{other}\"")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{EntryStatus, codec};

    #[test]
    fn every_operation_round_trips_through_json() {
        let library = codec::decode(fixtures::LIBRARY_JSON).unwrap();
        let entry = library.entries[0].clone();
        let plan = library.plans[0].clone();
        let item = plan.items[0].clone();
        let today = Date::parse("2026-10-19").unwrap();
        let season = Season {
            season: 2,
            released: None,
            status: Some(EntryStatus::Done),
            fresh: None,
            manual: None,
        };
        let id = || "movies/the-mummy-1999".to_owned();
        let operations = vec![
            Operation::AddEntry(entry.clone()),
            Operation::UpdateEntry(entry),
            Operation::DeleteEntry { id: id() },
            Operation::SetStatus {
                id: id(),
                status: EntryStatus::InProgress,
            },
            Operation::SetRating {
                id: id(),
                rating: Some(7),
            },
            Operation::SetRating {
                id: id(),
                rating: None,
            },
            Operation::SetTags {
                id: id(),
                tags: vec!["horror".into(), "clint-eastwood".into()],
                tag_categories: Some(TagPicks(vec![(
                    "clint-eastwood".into(),
                    vec![TagCategory::Actors],
                )])),
            },
            Operation::SetTags {
                id: id(),
                tags: vec!["horror".into()],
                tag_categories: None,
            },
            Operation::SetNote {
                id: id(),
                note: "Rewatch in October.".into(),
            },
            Operation::SetPlanItemDone {
                plan: plan.id.clone(),
                entry: id(),
                done: true,
            },
            Operation::AddPlan(plan.clone()),
            Operation::UpdatePlan(plan.clone()),
            Operation::DeletePlan {
                id: plan.id.clone(),
            },
            Operation::CreateTag {
                name: "slow-burn".into(),
                categories: vec![TagCategory::Mood, TagCategory::Other],
            },
            Operation::SetTagCategories {
                name: "slow-burn".into(),
                categories: vec![TagCategory::Mood],
            },
            Operation::SetPlanItem {
                plan: plan.id.clone(),
                item,
            },
            Operation::RemovePlanItem {
                plan: plan.id.clone(),
                entry: id(),
            },
            Operation::SetTrackRating {
                album: "music/album".into(),
                title: "Enter Sandman".into(),
                rating: Some(9),
            },
            Operation::SetTrackRating {
                album: "music/album".into(),
                title: "Enter Sandman".into(),
                rating: None,
            },
            Operation::SetSeasonStatus {
                id: id(),
                season: 2,
                status: Some(EntryStatus::Want),
                today,
            },
            Operation::SetSeasonStatus {
                id: id(),
                season: 2,
                status: None,
                today,
            },
            Operation::MarkSeasonsDone {
                id: id(),
                before: 3,
                today,
            },
            Operation::SetSeasons {
                id: id(),
                seasons: vec![season],
                today,
            },
            Operation::RemoveSeason {
                id: id(),
                season: 2,
                today,
            },
            Operation::KeepSeason {
                id: id(),
                season: 2,
            },
            Operation::SetSeriesCover { id: id() },
        ];
        for operation in operations {
            let value = encode(&operation);
            assert_eq!(decode(&value).unwrap(), operation, "{value}");
        }
    }

    #[test]
    fn the_names_and_fields_are_the_phones() {
        let value = encode(&Operation::SetStatus {
            id: "movies/x".into(),
            status: EntryStatus::InProgress,
        });
        assert_eq!(value["op"], "setStatus");
        assert_eq!(value["entryId"], "movies/x");
        assert_eq!(value["status"], "in_progress");
        let value = encode(&Operation::CreateTag {
            name: "x".into(),
            categories: vec![TagCategory::Genres],
        });
        assert_eq!(value["op"], "addTag");
        assert_eq!(value["categories"][0], "Genres");
        assert!(decode(&json!({ "op": "setKind" })).is_err());
        assert!(decode(&json!({ "op": "setStatus", "entryId": "x" })).is_err());
        assert!(decode(&json!({ "x": 1 })).is_err());
    }
}
