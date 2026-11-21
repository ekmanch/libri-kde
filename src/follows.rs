//! What Discover follows (the phone's DESIGN.md 10c, `follows.dart`):
//! every follow is derived from the library, never typed. Shows and anime
//! series linked to a lookup and not dropped, series with a part Done or
//! In progress, the authors, directors, studios and actors that are tags
//! of those categories, and the artists of albums and songs.
//! `settings.json` only holds the exceptions (`shared::Settings`): what
//! is unticked although it is on by default, and what is ticked although
//! it is off (actors, and creators no longer active).
//!
//! The daily check (`discover`) asks about what this module derives, and
//! the Following window, the Settings row, the Coming soon button and the
//! right-click menus read the same list, so they never disagree.

use std::collections::{BTreeMap, HashMap};

use crate::discover::CreatorStatus;
use crate::lookup::{self, FollowKind};
use crate::model::{
    Entry, EntryKind, EntryStatus, Library, MediaType, Series, TagCategory, tag_display_name,
};
use crate::shared::Settings;

/// The seven kinds of follow, in the phone's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Shows,
    Series,
    Authors,
    Artists,
    Directors,
    Studios,
    Actors,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Shows,
        Kind::Series,
        Kind::Authors,
        Kind::Artists,
        Kind::Directors,
        Kind::Studios,
        Kind::Actors,
    ];

    /// The key in `settings.json`, and the first part of a follow's key.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Shows => "shows",
            Kind::Series => "series",
            Kind::Authors => "authors",
            Kind::Artists => "artists",
            Kind::Directors => "directors",
            Kind::Studios => "studios",
            Kind::Actors => "actors",
        }
    }

    pub fn from_key(key: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// The kind's name in the Following window.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Shows => "Shows and anime",
            Kind::Series => "Series and collections",
            Kind::Authors => "Authors",
            Kind::Artists => "Artists",
            Kind::Directors => "Directors",
            Kind::Studios => "Studios",
            Kind::Actors => "Actors",
        }
    }

    /// What the check finds for it, under the name.
    pub fn what(self) -> &'static str {
        match self {
            Kind::Shows => "New seasons",
            Kind::Series => "Next parts",
            Kind::Authors => "New books",
            Kind::Artists => "New albums",
            Kind::Directors | Kind::Actors => "New movies and shows",
            Kind::Studios => "New movies and games",
        }
    }

    /// "Followed for new books", as the phone words each kind.
    pub fn group(self) -> &'static str {
        match self {
            Kind::Shows => "Followed for new seasons",
            Kind::Series => "Followed for new parts",
            Kind::Authors => "Followed for new books",
            Kind::Artists => "Followed for new albums",
            Kind::Directors | Kind::Actors => "Followed for new movies and shows",
            Kind::Studios => "Followed for new movies and games",
        }
    }

    /// One follow of the kind, as a menu names it: "Author".
    pub fn role(self) -> &'static str {
        match self {
            Kind::Shows => "Show",
            Kind::Series => "Series",
            Kind::Authors => "Author",
            Kind::Artists => "Artist",
            Kind::Directors => "Director",
            Kind::Studios => "Studio",
            Kind::Actors => "Actor",
        }
    }

    /// "Search authors", "Search shows and anime".
    pub fn search_hint(self) -> String {
        format!("Search {}", self.label().to_lowercase())
    }

    /// Why the kind's list is empty, as the phone says it.
    pub fn empty_text(self) -> String {
        match self {
            Kind::Shows => "No shows or anime with seasons from a lookup yet. They are \
                            followed as you add them."
                .to_owned(),
            Kind::Series => "No series with a part done or in progress yet. A series is \
                             followed once you have read, watched or played part of it."
                .to_owned(),
            Kind::Artists => {
                "No artists yet. Add an album or a song and its artist is followed.".to_owned()
            }
            _ => format!(
                "No {} tagged yet. Tag an entry with one and they are followed.",
                self.label().to_lowercase()
            ),
        }
    }

    /// 'No actors match "x".'
    pub fn no_match_text(self, query: &str) -> String {
        format!(
            "No {} match \"{}\".",
            self.label().to_lowercase(),
            query.trim()
        )
    }

    /// Whether a follow of the kind is on until it is unticked. Actors are
    /// off until ticked: a library tags far more of them than anyone wants
    /// to follow.
    pub fn default_on(self) -> bool {
        self != Kind::Actors
    }

    /// The kind as the lookups know it; shows and series are not creators.
    pub fn creator(self) -> Option<FollowKind> {
        match self {
            Kind::Shows | Kind::Series => None,
            Kind::Authors => Some(FollowKind::Authors),
            Kind::Artists => Some(FollowKind::Artists),
            Kind::Directors => Some(FollowKind::Directors),
            Kind::Studios => Some(FollowKind::Studios),
            Kind::Actors => Some(FollowKind::Actors),
        }
    }
}

impl From<FollowKind> for Kind {
    fn from(kind: FollowKind) -> Kind {
        match kind {
            FollowKind::Authors => Kind::Authors,
            FollowKind::Artists => Kind::Artists,
            FollowKind::Directors => Kind::Directors,
            FollowKind::Studios => Kind::Studios,
            FollowKind::Actors => Kind::Actors,
        }
    }
}

/// The key a follow goes by in the check's record and on what it found:
/// "authors/stephen-king", "shows/shows/severance-2022".
pub fn key_of(kind: Kind, id: &str) -> String {
    format!("{}/{id}", kind.key())
}

/// A follow's key taken apart again.
pub fn parse_key(key: &str) -> Option<(Kind, &str)> {
    let (kind, id) = key.split_once('/')?;
    Some((Kind::from_key(kind)?, id))
}

/// The id an artist is followed by: the name in lower case, so names that
/// differ only in case are one artist.
pub fn artist_id(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Whether a show is asked for new seasons: linked to a lookup that lists
/// seasons, and not dropped. `instant` is the fake, which knows the
/// fixture's shows without ids.
fn is_asked(entry: &Entry, instant: bool) -> bool {
    let linked = instant
        || entry
            .source
            .as_deref()
            .is_some_and(|s| s.starts_with("tmdb:tv:") || s.starts_with("anilist:"));
    entry.uses_seasons() && linked && entry.status != EntryStatus::Dnf && !entry.seasonal_only
}

/// The shows and anime series followed for new seasons, before the
/// settings.
pub fn shows(library: &Library, instant: bool) -> Vec<&Entry> {
    library
        .entries
        .iter()
        .filter(|entry| is_asked(entry, instant))
        .collect()
}

/// A series of the library with its parts.
pub struct SeriesGroup<'a> {
    pub media: MediaType,
    pub name: String,
    /// "<type>/<the name as a title key>": the id in `settings.json`.
    pub key: String,
    pub parts: Vec<&'a Entry>,
}

/// The series followed for next parts, before the settings: those with a
/// part Done or In progress (Want alone is not following).
pub fn series(library: &Library) -> Vec<SeriesGroup<'_>> {
    let mut groups: Vec<SeriesGroup<'_>> = Vec::new();
    for entry in &library.entries {
        let Some(series) = &entry.series else {
            continue;
        };
        if !entry.can_have_series() || entry.seasonal_only {
            continue;
        }
        match groups
            .iter_mut()
            .find(|group| group.media == entry.media && Series::same(&group.name, &series.name))
        {
            Some(group) => group.parts.push(entry),
            None => groups.push(SeriesGroup {
                media: entry.media,
                name: series.name.clone(),
                key: format!(
                    "{}/{}",
                    entry.media.key(),
                    lookup::text::title_key(&series.name).replace(' ', "")
                ),
                parts: vec![entry],
            }),
        }
    }
    groups.retain(|group| {
        group
            .parts
            .iter()
            .any(|p| matches!(p.status, EntryStatus::Done | EntryStatus::InProgress))
    });
    groups
}

/// A creator the library has work by.
pub struct Creator<'a> {
    pub kind: FollowKind,
    /// The tag, or an artist's name in lower case.
    pub id: String,
    pub name: String,
    pub entries: Vec<&'a Entry>,
}

impl Creator<'_> {
    pub fn key(&self) -> String {
        key_of(self.kind.into(), &self.id)
    }
}

/// The creators of `kind` in the library, before the settings: the tags
/// of its category with an entry not dropped; artists from the creator
/// field of albums and songs. In a mock run (`instant`) authors come from
/// the creator field too, since the fixture has no author tags.
pub fn creators(library: &Library, kind: FollowKind, instant: bool) -> Vec<Creator<'_>> {
    let by_field = kind == FollowKind::Authors && instant;
    if let Some(category) = kind.category().filter(|_| !by_field) {
        return tagged(library, kind, category);
    }
    let media = if by_field {
        MediaType::Books
    } else {
        MediaType::Music
    };
    let mut named: BTreeMap<String, Creator<'_>> = BTreeMap::new();
    for entry in library.entries_of(media) {
        let field = entry.creator.as_deref().unwrap_or_default().trim();
        let names = if by_field {
            lookup::text::split_creators(field)
        } else {
            vec![field.to_owned()]
        };
        for name in names {
            let id = if by_field {
                crate::model::text::slugify(&name)
            } else {
                artist_id(&name)
            };
            if id.is_empty() || lookup::NO_PHOTO.contains(&id.as_str()) {
                continue;
            }
            named
                .entry(id.clone())
                .or_insert_with(|| Creator {
                    kind,
                    id,
                    name,
                    entries: Vec::new(),
                })
                .entries
                .push(entry);
        }
    }
    named.into_values().collect()
}

/// The tags of `category`, each with the entries that carry it in that
/// role, gathered in one pass over the entries.
fn tagged(library: &Library, kind: FollowKind, category: TagCategory) -> Vec<Creator<'_>> {
    let Some(tags) = library.tags.get(&category) else {
        return Vec::new();
    };
    let mut at: HashMap<&str, usize> = HashMap::with_capacity(tags.len());
    let mut found: Vec<Vec<&Entry>> = vec![Vec::new(); tags.len()];
    for (index, tag) in tags.iter().enumerate() {
        at.insert(tag.as_str(), index);
    }
    for entry in &library.entries {
        for tag in &entry.tags {
            if let Some(&index) = at.get(tag.as_str())
                && entry.has_tag_as(tag, category)
            {
                found[index].push(entry);
            }
        }
    }
    tags.iter()
        .zip(found)
        .filter(|(_, entries)| {
            !entries.is_empty() && !entries.iter().all(|e| e.status == EntryStatus::Dnf)
        })
        .map(|(tag, entries)| Creator {
            kind,
            id: tag.clone(),
            name: creator_name(tag, &entries),
            entries,
        })
        .collect()
}

/// A creator tag as the name it stands for: the name in a creator field of
/// one of its entries ("J. K. Rowling" for #j-k-rowling), else the tag in
/// words.
fn creator_name(tag: &str, entries: &[&Entry]) -> String {
    entries
        .iter()
        .filter_map(|entry| entry.creator.as_deref())
        .flat_map(lookup::text::split_creators)
        .find(|name| crate::model::text::slugify(name) == tag)
        .unwrap_or_else(|| tag_display_name(tag))
}

/// "2 movies", "1 anime".
fn count(media: MediaType, n: usize) -> String {
    let (one, many) = match media {
        MediaType::Movies => ("movie", "movies"),
        MediaType::Shows => ("show", "shows"),
        MediaType::Books => ("book", "books"),
        MediaType::Games => ("game", "games"),
        MediaType::Music => ("album", "albums"),
        MediaType::Anime => ("anime", "anime"),
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "2 movies, 1 show", in type order; an artist's "1 album, 1 song".
fn counts(kind: FollowKind, entries: &[&Entry]) -> String {
    let mut parts = Vec::new();
    if kind == FollowKind::Artists {
        let songs = entries
            .iter()
            .filter(|e| e.kind == Some(EntryKind::Song))
            .count();
        let albums = entries.len() - songs;
        if albums > 0 {
            parts.push(count(MediaType::Music, albums));
        }
        if songs > 0 {
            parts.push(format!(
                "{songs} {}",
                if songs == 1 { "song" } else { "songs" }
            ));
        }
        return parts.join(", ");
    }
    for media in MediaType::ALL {
        let n = entries.iter().filter(|e| e.media == media).count();
        if n > 0 {
            parts.push(count(media, n));
        }
    }
    parts.join(", ")
}

/// One thing followed, as the Following window lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    pub kind: Kind,
    pub id: String,
    pub name: String,
    /// What the library has of theirs: "3 books", "Show · 2 seasons".
    pub detail: String,
    /// Why a creator is no longer active ("Died 2021"), from the check.
    pub inactive: Option<String>,
    /// Whether the follow is on, after the settings.
    pub followed: bool,
}

impl Follow {
    pub fn is_active(&self) -> bool {
        self.inactive.is_none()
    }

    /// On until unticked: the kind's default, and the creator active.
    pub fn default_on(&self) -> bool {
        self.kind.default_on() && self.is_active()
    }

    /// The row's second line: what the library has, or why the creator is
    /// no longer active and what that leaves checked.
    pub fn second_line(&self) -> String {
        match &self.inactive {
            None => self.detail.clone(),
            Some(why) if self.kind == Kind::Authors => format!("{why} · Series still checked"),
            Some(why) => format!("{why} · Not checked"),
        }
    }

    fn sort_key(&self) -> String {
        let key = self.name.trim().to_lowercase();
        match key.strip_prefix("the ") {
            Some(rest) => rest.trim_start().to_owned(),
            None => key,
        }
    }
}

/// The follows of every kind, as the library, the settings and the
/// check's creator records give them. Each kind's list is A to Z, a
/// leading "The" set aside.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Follows {
    by_kind: BTreeMap<Kind, Vec<Follow>>,
}

impl Follows {
    pub fn derive(
        library: &Library,
        settings: &Settings,
        statuses: &BTreeMap<String, CreatorStatus>,
        instant: bool,
    ) -> Follows {
        let mut by_kind = BTreeMap::new();
        for kind in Kind::ALL {
            let mut list: Vec<Follow> = match kind.creator() {
                None if kind == Kind::Shows => shows(library, instant)
                    .into_iter()
                    .map(|show| {
                        let seasons = show.season_list().len();
                        let word = if show.media == MediaType::Anime {
                            "Anime"
                        } else {
                            "Show"
                        };
                        let detail = match seasons {
                            0 => format!("{word} · No seasons yet"),
                            1 => format!("{word} · 1 season"),
                            n => format!("{word} · {n} seasons"),
                        };
                        (show.id.clone(), show.title.clone(), detail)
                    })
                    .map(|(id, name, detail)| raw(kind, id, name, detail))
                    .collect(),
                None => series(library)
                    .into_iter()
                    .map(|group| {
                        let detail = format!(
                            "{} · {}",
                            group.media.label(),
                            count(group.media, group.parts.len())
                        );
                        raw(kind, group.key, Series::display_name(&group.name), detail)
                    })
                    .collect(),
                Some(creator) => creators(library, creator, instant)
                    .into_iter()
                    .map(|c| {
                        let detail = counts(creator, &c.entries);
                        raw(kind, c.id, c.name, detail)
                    })
                    .collect(),
            };
            for follow in &mut list {
                follow.inactive = statuses
                    .get(&key_of(kind, &follow.id))
                    .and_then(|status| status.inactive.clone());
                follow.followed = settings.is_followed(kind, &follow.id, follow.default_on());
            }
            list.sort_by_cached_key(|follow| (follow.sort_key(), follow.id.clone()));
            by_kind.insert(kind, list);
        }
        Follows { by_kind }
    }

    pub fn of(&self, kind: Kind) -> &[Follow] {
        self.by_kind.get(&kind).map_or(&[], Vec::as_slice)
    }

    pub fn find(&self, kind: Kind, id: &str) -> Option<&Follow> {
        self.of(kind).iter().find(|follow| follow.id == id)
    }

    /// How many of `kind` are followed.
    pub fn followed(&self, kind: Kind) -> usize {
        self.of(kind).iter().filter(|f| f.followed).count()
    }

    /// How many are followed in all: "Following 423".
    pub fn total(&self) -> usize {
        Kind::ALL.into_iter().map(|kind| self.followed(kind)).sum()
    }

    /// The count beside a kind: "119" when every active one is followed,
    /// "10 of 25" otherwise. One no longer active counts only while ticked.
    pub fn count_label(&self, kind: Kind) -> String {
        let all = self.of(kind);
        let counted = all.iter().filter(|f| f.is_active() || f.followed).count();
        let followed = self.followed(kind);
        if followed == counted {
            followed.to_string()
        } else {
            format!("{followed} of {counted}")
        }
    }

    /// Whether Unfollow all is offered rather than Follow all: while more
    /// than half of the active ones are followed.
    pub fn most_followed(&self, kind: Kind) -> bool {
        let active: Vec<&Follow> = self.of(kind).iter().filter(|f| f.is_active()).collect();
        active.iter().filter(|f| f.followed).count() * 2 > active.len()
    }

    /// The ids Follow all or Unfollow all changes: the active ones that
    /// are not `on` yet. Those no longer active are left alone.
    pub fn to_set(&self, kind: Kind, on: bool) -> Vec<String> {
        self.of(kind)
            .iter()
            .filter(|f| f.is_active() && f.followed != on)
            .map(|f| f.id.clone())
            .collect()
    }
}

fn raw(kind: Kind, id: String, name: String, detail: String) -> Follow {
    Follow {
        kind,
        id,
        name,
        detail,
        inactive: None,
        followed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::Date;
    use crate::shared::Edit;

    fn library() -> Library {
        crate::model::codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    fn derive(settings: &Settings, statuses: &BTreeMap<String, CreatorStatus>) -> Follows {
        Follows::derive(&library(), settings, statuses, true)
    }

    fn died(year: &str) -> CreatorStatus {
        CreatorStatus {
            provider_id: None,
            inactive: Some(format!("Died {year}")),
            checked_on: Date::parse("2026-11-01").unwrap(),
        }
    }

    #[test]
    fn every_kind_is_derived_from_the_library_in_name_order() {
        let follows = derive(&Settings::default(), &BTreeMap::new());
        for kind in Kind::ALL {
            let names: Vec<String> = follows.of(kind).iter().map(Follow::sort_key).collect();
            let mut sorted = names.clone();
            sorted.sort();
            assert_eq!(names, sorted, "{kind:?} is A to Z");
        }
        let show = follows
            .find(Kind::Shows, "shows/severance-2022")
            .expect("a show with seasons is followed");
        assert!(show.followed);
        assert!(show.detail.starts_with("Show · "), "{}", show.detail);
        assert!(!follows.of(Kind::Artists).is_empty());
        assert!(
            follows.of(Kind::Artists).iter().all(|a| a.followed),
            "artists are on until unticked"
        );
        assert!(
            follows.of(Kind::Actors).iter().all(|a| !a.followed),
            "actors are off until ticked"
        );
    }

    #[test]
    fn a_leading_the_is_set_aside_in_the_order() {
        let follow = |name: &str| raw(Kind::Artists, artist_id(name), name.into(), String::new());
        let mut list = [follow("The Beatles"), follow("Adele"), follow("Camel")];
        list.sort_by_cached_key(Follow::sort_key);
        let names: Vec<&str> = list.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Adele", "The Beatles", "Camel"]);
    }

    #[test]
    fn the_settings_hold_the_exceptions_and_the_counts_follow() {
        let all = derive(&Settings::default(), &BTreeMap::new());
        let artists = all.of(Kind::Artists);
        let n = artists.len();
        assert_eq!(all.count_label(Kind::Artists), n.to_string());
        assert!(all.most_followed(Kind::Artists));
        let first = artists[0].id.clone();
        let mut settings = Settings::default();
        settings.apply(&Edit::Follow {
            kind: Kind::Artists,
            ids: vec![first.clone()],
            on: false,
            default_on: true,
        });
        let follows = derive(&settings, &BTreeMap::new());
        assert!(!follows.find(Kind::Artists, &first).unwrap().followed);
        assert_eq!(
            follows.count_label(Kind::Artists),
            format!("{} of {n}", n - 1)
        );
        assert_eq!(follows.total(), all.total() - 1);
        assert_eq!(follows.to_set(Kind::Artists, true), [first]);
    }

    #[test]
    fn one_no_longer_active_is_off_until_ticked_and_left_alone_by_follow_all() {
        let all = derive(&Settings::default(), &BTreeMap::new());
        let artist = all.of(Kind::Artists)[0].clone();
        let key = key_of(Kind::Artists, &artist.id);
        let statuses = BTreeMap::from([(key, died("1996"))]);
        let follows = derive(&Settings::default(), &statuses);
        let gone = follows.find(Kind::Artists, &artist.id).unwrap();
        assert!(!gone.followed && !gone.is_active());
        assert_eq!(gone.second_line(), "Died 1996 · Not checked");
        // Counted only while ticked, so the label is still a plain number.
        let n = follows.of(Kind::Artists).len() - 1;
        assert_eq!(follows.count_label(Kind::Artists), n.to_string());
        assert!(
            !follows.to_set(Kind::Artists, false).contains(&artist.id),
            "Unfollow all leaves it alone"
        );
        // Ticked anyway: an exception the other way.
        let mut settings = Settings::default();
        settings.apply(&Edit::Follow {
            kind: Kind::Artists,
            ids: vec![artist.id.clone()],
            on: true,
            default_on: false,
        });
        let follows = derive(&settings, &statuses);
        assert!(follows.find(Kind::Artists, &artist.id).unwrap().followed);
        assert_eq!(
            follows.count_label(Kind::Artists),
            (n + 1).to_string(),
            "ticked, it counts"
        );
    }

    #[test]
    fn an_author_no_longer_active_keeps_their_series() {
        let mut follow = raw(Kind::Authors, "x".into(), "X".into(), "3 books".into());
        assert_eq!(follow.second_line(), "3 books");
        follow.inactive = Some("Died 2021".into());
        assert_eq!(follow.second_line(), "Died 2021 · Series still checked");
    }

    #[test]
    fn the_keys_come_apart_again() {
        let key = key_of(Kind::Shows, "shows/severance-2022");
        assert_eq!(parse_key(&key), Some((Kind::Shows, "shows/severance-2022")));
        assert_eq!(
            parse_key("series/movies/halo"),
            Some((Kind::Series, "movies/halo"))
        );
        assert_eq!(parse_key("nothing"), None);
    }
}
