//! Plan rules, as on the phone (plan_progress.dart, plan_builders.dart,
//! plan_season.dart, plan_timeline.dart): where a plan stands, its
//! season, ids and copies, and the timeline's buckets.

use crate::model::{Date, Entry, EntryStatus, Library, MediaType, Plan, PlanItem, PlanWhen, text};

/// Where a plan stands on the plans overview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlanState {
    Active,
    NotStarted,
    Finished,
}

impl PlanState {
    pub fn label(self) -> &'static str {
        match self {
            PlanState::Active => "Active",
            PlanState::NotStarted => "Not started",
            PlanState::Finished => "Finished",
        }
    }
}

/// Finished: at least one entry and every entry done (an empty plan is
/// never finished), and the plan's last date, if it has one, has passed,
/// so a plan with days left stays active for more entries. Active:
/// Started is on, or the plan's first date has come. Otherwise not
/// started.
pub fn plan_state(plan: &Plan, today: Date) -> PlanState {
    let all_done = !plan.items.is_empty() && plan.items.iter().all(|i| i.done);
    if all_done && plan.last_date().is_none_or(|d| d < today) {
        PlanState::Finished
    } else if plan.started || plan.first_date().is_some_and(|d| d <= today) {
        PlanState::Active
    } else {
        PlanState::NotStarted
    }
}

/// The two seasons with artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Season {
    Halloween,
    Christmas,
}

impl Season {
    pub fn name(self) -> &'static str {
        match self {
            Season::Halloween => "Halloween",
            Season::Christmas => "Christmas",
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Season::Halloween => "halloween",
            Season::Christmas => "christmas",
        }
    }

    /// The help line under the Seasonal theme switch.
    pub fn help(self) -> &'static str {
        match self {
            Season::Halloween => "Halloween artwork and an orange accent inside this plan.",
            Season::Christmas => "Christmas artwork and a green accent inside this plan.",
        }
    }
}

/// The season a set of linked tags implies, if any.
pub fn season_for_tags<'a>(tags: impl IntoIterator<Item = &'a String>) -> Option<Season> {
    tags.into_iter()
        .find_map(|tag| match tag.to_lowercase().as_str() {
            "halloween" => Some(Season::Halloween),
            "christmas" | "xmas" => Some(Season::Christmas),
            _ => None,
        })
}

/// The season a plan shows: only with its theme switched on.
pub fn season_of(plan: &Plan) -> Option<Season> {
    if plan.seasonal_theme {
        season_for_tags(&plan.tags)
    } else {
        None
    }
}

/// "halloween-2026", "summer-vacation": unique among `existing`.
pub fn plan_id<'a>(
    name: &str,
    year: Option<i32>,
    existing: impl IntoIterator<Item = &'a str> + Clone,
) -> String {
    let words = match year {
        Some(year) => format!("{name} {year}"),
        None => name.to_owned(),
    };
    let slug = text::slugify(&words);
    let base = if slug.is_empty() {
        "plan".to_owned()
    } else {
        slug
    };
    let taken = |id: &str| existing.clone().into_iter().any(|e| e == id);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("an unused id")
}

/// A new plan from `source`: its tags and switches, and the items not
/// finished there, without their dates or checkmarks.
pub fn copy_plan(source: &Plan, id: String, name: String, year: Option<i32>) -> Plan {
    Plan {
        id,
        name,
        year,
        tags: source.tags.clone(),
        timeline: source.timeline,
        seasonal_theme: source.seasonal_theme,
        started: false,
        items: source
            .items
            .iter()
            .filter(|i| !i.done)
            .map(|i| PlanItem {
                done: false,
                when: None,
                ..i.clone()
            })
            .collect(),
    }
}

/// The plans with the same name as `plan`, newest year first, `plan`
/// included: the year chip's choices.
pub fn plan_years<'a>(library: &'a Library, plan: &Plan) -> Vec<&'a Plan> {
    let mut same: Vec<&Plan> = library
        .plans
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(&plan.name))
        .collect();
    same.sort_by_key(|p| std::cmp::Reverse(p.year.unwrap_or(0)));
    same
}

/// The year a copy of `plan` gets: the year after it, or after this year.
pub fn next_year(plan: &Plan, today: Date) -> i32 {
    plan.year.unwrap_or(i32::from(today.year)) + 1
}

pub fn done_count(plan: &Plan) -> usize {
    plan.items.iter().filter(|i| i.done).count()
}

/// "2 of 6 done".
pub fn progress_label(plan: &Plan) -> String {
    format!("{} of {} done", done_count(plan), plan.items.len())
}

/// "Empty, linked to #christmas", "Linked to #halloween", or "6 entries".
pub fn subtitle(plan: &Plan) -> String {
    let tags: Vec<String> = plan.tags.iter().map(|t| format!("#{t}")).collect();
    let tags = tags.join(", ");
    match (plan.items.len(), tags.is_empty()) {
        (0, true) => "Empty".into(),
        (0, false) => format!("Empty, linked to {tags}"),
        (_, false) => format!("Linked to {tags}"),
        (1, true) => "1 entry".into(),
        (n, true) => format!("{n} entries"),
    }
}

/// The span of the items' dates, from the first day to the last.
pub fn date_span(plan: &Plan) -> Option<PlanWhen> {
    let from = plan
        .items
        .iter()
        .filter_map(|i| i.when.map(|w| w.from))
        .min()?;
    let to = plan
        .items
        .iter()
        .filter_map(|i| i.when.map(|w| w.to))
        .max()?;
    Some(PlanWhen { from, to })
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// "Oct 17".
pub fn month_day(date: Date) -> String {
    format!("{} {}", MONTHS[usize::from(date.month) - 1], date.day)
}

/// "Sat Oct 17".
pub fn weekday_day(date: Date) -> String {
    format!("{} {}", WEEKDAYS[date.weekday()], month_day(date))
}

/// "Oct 19–31", "Oct 28 – Nov 2", or "Sat Oct 17" for one day; the year is
/// added only when it is not `year`.
pub fn format_when(when: PlanWhen, year: Option<i32>) -> String {
    let with_year = |date: Date, text: String| match year {
        Some(year) if i32::from(date.year) != year => format!("{text} {}", date.year),
        _ => text,
    };
    let (from, to) = (when.from, when.to);
    if when.is_single_day() {
        with_year(from, weekday_day(from))
    } else if from.month == to.month && from.year == to.year {
        with_year(from, format!("{}–{}", month_day(from), to.day))
    } else {
        format!(
            "{} – {}",
            with_year(from, month_day(from)),
            with_year(to, month_day(to))
        )
    }
}

/// "Rewatch", "Reread", "Replay", "Relisten": a plan row for an entry
/// that was already done when it was added.
pub fn repeat_label(media: MediaType) -> &'static str {
    match media {
        MediaType::Movies | MediaType::Shows | MediaType::Anime => "Rewatch",
        MediaType::Books => "Reread",
        MediaType::Games => "Replay",
        MediaType::Music => "Relisten",
    }
}

/// Where a timeline group sits against today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineState {
    Past,
    Current,
    Future,
    Undated,
}

/// Items that share a "When" bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineGroup<'a> {
    /// "Earlier", "Last week", "This week", "Sat Oct 17", "Later", "Whenever".
    pub title: String,
    /// "Oct 19–31", "One day", "No date".
    pub dates: String,
    pub state: TimelineState,
    pub items: Vec<&'a PlanItem>,
}

/// The timeline: Earlier, Last week, This week, then each future single
/// day and Later by date, then Whenever. Inside a bucket items follow the
/// type order, then their date, then the plan's order.
pub fn timeline<'a>(plan: &'a Plan, library: &Library, today: Date) -> Vec<TimelineGroup<'a>> {
    let week_start = today.add_days(-(today.weekday() as i64));
    let week_end = week_start.add_days(6);
    let last_week_start = week_start.add_days(-7);

    let (mut earlier, mut last_week, mut this_week, mut later, mut undated) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut days: Vec<(Date, Vec<&PlanItem>)> = Vec::new();
    for item in &plan.items {
        let Some(when) = item.when else {
            undated.push(item);
            continue;
        };
        if when.to < last_week_start {
            earlier.push(item);
        } else if when.to < week_start {
            last_week.push(item);
        } else if when.from <= week_end {
            this_week.push(item);
        } else if when.is_single_day() {
            match days.iter_mut().find(|(day, _)| *day == when.from) {
                Some((_, items)) => items.push(item),
                None => days.push((when.from, vec![item])),
            }
        } else {
            later.push(item);
        }
    }

    let rank = |item: &PlanItem| {
        library
            .entry(&item.entry)
            .map(|e| {
                MediaType::ALL
                    .iter()
                    .position(|&m| m == e.media)
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    };
    let position = |item: &PlanItem| {
        plan.items
            .iter()
            .position(|i| std::ptr::eq(i, item))
            .unwrap_or(0)
    };
    let ordered = |mut items: Vec<&'a PlanItem>| {
        items.sort_by_key(|i| (rank(i), i.when.map(|w| w.from), position(i)));
        items
    };
    let span = |items: &[&PlanItem]| {
        let from = items
            .iter()
            .filter_map(|i| i.when.map(|w| w.from))
            .min()
            .expect("dated");
        let to = items
            .iter()
            .filter_map(|i| i.when.map(|w| w.to))
            .max()
            .expect("dated");
        format_when(PlanWhen { from, to }, plan.year)
    };
    let group = |title: &str, dates: String, state, items| TimelineGroup {
        title: title.to_owned(),
        dates,
        state,
        items,
    };

    let mut groups = Vec::new();
    if !earlier.is_empty() {
        groups.push(group(
            "Earlier",
            span(&earlier),
            TimelineState::Past,
            ordered(earlier),
        ));
    }
    if !last_week.is_empty() {
        groups.push(group(
            "Last week",
            span(&last_week),
            TimelineState::Past,
            ordered(last_week),
        ));
    }
    if !this_week.is_empty() {
        let dates = format_when(
            PlanWhen {
                from: week_start,
                to: week_end,
            },
            plan.year,
        );
        groups.push(group(
            "This week",
            dates,
            TimelineState::Current,
            ordered(this_week),
        ));
    }
    let mut future: Vec<(Date, u8, TimelineGroup)> = days
        .into_iter()
        .map(|(day, items)| {
            (
                day,
                0,
                group(
                    &weekday_day(day),
                    "One day".into(),
                    TimelineState::Future,
                    ordered(items),
                ),
            )
        })
        .collect();
    if !later.is_empty() {
        let start = later
            .iter()
            .filter_map(|i| i.when.map(|w| w.from))
            .min()
            .expect("dated");
        future.push((
            start,
            1,
            group("Later", span(&later), TimelineState::Future, ordered(later)),
        ));
    }
    future.sort_by_key(|(day, order, _)| (*day, *order));
    groups.extend(future.into_iter().map(|(_, _, g)| g));
    if !undated.is_empty() {
        groups.push(group(
            "Whenever",
            "No date".into(),
            TimelineState::Undated,
            ordered(undated),
        ));
    }
    groups
}

/// The entries the "Add to this plan" pane offers: those with any of the
/// plan's tags, or with no tags wanted titles, not in the plan yet.
pub fn pool<'a>(plan: &Plan, library: &'a Library) -> Vec<&'a Entry> {
    library
        .entries
        .iter()
        .filter(|e| plan.item_for(&e.id).is_none())
        .filter(|e| {
            if plan.tags.is_empty() {
                e.status == EntryStatus::Want
            } else {
                e.tags.iter().any(|t| plan.tags.contains(t))
            }
        })
        .collect()
}

/// Everything tagged with any of the plan's tags: the "All Halloween" view.
pub fn collection<'a>(plan: &Plan, library: &'a Library) -> Vec<&'a Entry> {
    library
        .entries
        .iter()
        .filter(|e| e.tags.iter().any(|t| plan.tags.contains(t)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::codec;

    fn library() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn plan_states_follow_the_phone() {
        let library = library();
        let today = date("2026-10-19");
        let state = |id| plan_state(library.plan(id).unwrap(), today);
        assert_eq!(state("halloween-2026"), PlanState::Active);
        assert_eq!(state("christmas-2026"), PlanState::NotStarted);
        assert_eq!(state("halloween-2025"), PlanState::Finished);
    }

    #[test]
    fn a_plan_all_done_stays_active_until_its_last_date_passes() {
        let mut plan = library().plan("halloween-2026").unwrap().clone();
        for item in &mut plan.items {
            item.done = true;
        }
        let last = plan.last_date().unwrap();
        assert_eq!(last, date("2026-10-31"));
        assert_eq!(plan_state(&plan, date("2026-10-23")), PlanState::Active);
        assert_eq!(plan_state(&plan, last), PlanState::Active);
        assert_eq!(plan_state(&plan, date("2026-11-01")), PlanState::Finished);
        for item in &mut plan.items {
            item.when = None;
        }
        assert_eq!(plan_state(&plan, date("2026-10-23")), PlanState::Finished);
    }

    #[test]
    fn a_plan_is_active_once_its_first_date_comes() {
        let mut plan = library().plan("christmas-2026").unwrap().clone();
        plan.items[0].when = Some(PlanWhen {
            from: date("2026-12-01"),
            to: date("2026-12-01"),
        });
        assert_eq!(plan_state(&plan, date("2026-11-30")), PlanState::NotStarted);
        assert_eq!(plan_state(&plan, date("2026-12-01")), PlanState::Active);
    }

    #[test]
    fn seasons_need_the_tag_and_the_theme() {
        let library = library();
        let mut plan = library.plan("halloween-2026").unwrap().clone();
        assert_eq!(season_of(&plan), Some(Season::Halloween));
        plan.seasonal_theme = false;
        assert_eq!(season_of(&plan), None);
    }

    #[test]
    fn plan_ids_and_copies() {
        let library = library();
        let ids = library.plans.iter().map(|p| p.id.as_str());
        assert_eq!(
            plan_id("Halloween", Some(2027), ids.clone()),
            "halloween-2027"
        );
        assert_eq!(
            plan_id("Halloween", Some(2026), ids.clone()),
            "halloween-2026-2"
        );
        assert_eq!(plan_id("", None, ids), "plan");

        let source = library.plan("halloween-2026").unwrap();
        let copy = copy_plan(
            source,
            "halloween-2027".into(),
            "Halloween".into(),
            Some(2027),
        );
        assert_eq!(
            copy.items.len(),
            source.items.iter().filter(|i| !i.done).count()
        );
        assert!(copy.items.iter().all(|i| !i.done && i.when.is_none()));
        assert_eq!(next_year(source, date("2026-10-19")), 2027);
    }

    #[test]
    fn the_year_chip_lists_plans_of_the_same_name_newest_first() {
        let library = library();
        let years: Vec<_> = plan_years(&library, library.plan("halloween-2025").unwrap())
            .iter()
            .map(|p| p.year)
            .collect();
        assert_eq!(years, [Some(2026), Some(2025)]);
    }

    #[test]
    fn whens_read_short() {
        let when = |a, b| PlanWhen {
            from: date(a),
            to: date(b),
        };
        assert_eq!(
            format_when(when("2026-10-19", "2026-10-31"), Some(2026)),
            "Oct 19–31"
        );
        assert_eq!(
            format_when(when("2026-10-28", "2026-11-02"), Some(2026)),
            "Oct 28 – Nov 2"
        );
        assert_eq!(
            format_when(when("2026-10-17", "2026-10-17"), Some(2026)),
            "Sat Oct 17"
        );
        assert_eq!(
            format_when(when("2027-01-02", "2027-01-02"), Some(2026)),
            "Sat Jan 2 2027"
        );
    }

    #[test]
    fn the_timeline_buckets_by_time() {
        let library = library();
        let plan = library.plan("halloween-2026").unwrap();
        let groups = timeline(plan, &library, date("2026-10-19"));
        let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
        // Oct 1 and Oct 3 are earlier, Oct 10–17 last week, Oct 19–31 and
        // Oct 24–26 start this week, and three items have no date.
        assert_eq!(titles, ["Earlier", "Last week", "This week", "Whenever"]);
        assert_eq!(
            (groups[2].dates.as_str(), groups[2].items.len()),
            ("Oct 19–25", 2)
        );
        assert_eq!(groups.last().unwrap().items.len(), 3);
    }

    #[test]
    fn the_pool_offers_tagged_entries_not_in_the_plan() {
        let library = library();
        let plan = library.plan("halloween-2026").unwrap();
        let pool = pool(plan, &library);
        assert!(!pool.is_empty());
        assert!(
            pool.iter()
                .all(|e| plan.item_for(&e.id).is_none() && e.tags.iter().any(|t| t == "halloween"))
        );
        assert_eq!(subtitle(plan), "Linked to #halloween");
    }
}
