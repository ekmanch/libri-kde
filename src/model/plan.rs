use serde::{Deserialize, Serialize};

use super::Date;

/// A named list of entries, for example "Halloween 2026". An entry matches
/// the plan's tags if it has any of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub timeline: bool,
    #[serde(default)]
    pub seasonal_theme: bool,
    /// Set by the Started switch, and by the app when the plan's first date
    /// arrives. Never cleared on its own.
    #[serde(default)]
    pub started: bool,
    #[serde(default)]
    pub items: Vec<PlanItem>,
}

impl Plan {
    /// The name with the year, as the UI shows it: "Halloween 2026".
    pub fn title(&self) -> String {
        match self.year {
            Some(year) => format!("{} {year}", self.name),
            None => self.name.clone(),
        }
    }

    pub fn item_for(&self, entry_id: &str) -> Option<&PlanItem> {
        self.items.iter().find(|item| item.entry == entry_id)
    }

    /// The earliest "When" among the items.
    pub fn first_date(&self) -> Option<Date> {
        self.items
            .iter()
            .filter_map(|item| item.when.map(|when| when.from))
            .min()
    }

    /// The last day of the latest "When" among the items.
    pub fn last_date(&self) -> Option<Date> {
        self.items
            .iter()
            .filter_map(|item| item.when.map(|when| when.to))
            .max()
    }
}

/// One entry inside a plan. `entry` is the entry id. `repeat` is set when
/// the entry was already done when it was added: a rewatch or re-read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanItem {
    pub entry: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub repeat: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub when: Option<PlanWhen>,
}

/// When a plan item is scheduled: a single day (`from` equals `to`) or a
/// period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanWhen {
    pub from: Date,
    pub to: Date,
}

impl PlanWhen {
    pub fn is_single_day(&self) -> bool {
        self.from == self.to
    }
}
