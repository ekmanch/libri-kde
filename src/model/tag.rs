use serde::{Deserialize, Serialize};

use super::MediaType;

/// Tag categories, in display order, which is also their order in
/// `library.json`. The JSON keys of the `tags` map are the display names.
/// The list is the phone's (its DESIGN.md section 7, settled with it on
/// 2026-10-24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TagCategory {
    /// Genres for movies, shows, books and anime.
    #[serde(rename = "Genres")]
    Genres,
    #[serde(rename = "Game genres")]
    GameGenres,
    #[serde(rename = "Music genres")]
    MusicGenres,
    #[serde(rename = "Mood")]
    Mood,
    #[serde(rename = "Season")]
    Season,
    /// The language the work is in (#japanese, #swedish), one per tag.
    #[serde(rename = "Languages")]
    Languages,
    /// A creator tag's group is the person's or studio's main role, not the
    /// entry's type: #stephen-king is an author even on a movie, and
    /// Directors also holds showrunners and anime directors.
    #[serde(rename = "Authors")]
    Authors,
    #[serde(rename = "Directors")]
    Directors,
    /// The cast of movies and shows.
    #[serde(rename = "Actors")]
    Actors,
    #[serde(rename = "Studios")]
    Studios,
    /// Where a movie or show first came out (#netflix, #hbo), whether the
    /// platform made it or bought it. Movies and shows only.
    #[serde(rename = "Streaming")]
    Streaming,
    #[serde(rename = "Animation style")]
    AnimationStyle,
    #[serde(rename = "Other")]
    Other,
}

/// The two kinds of tag that may be in more than one category of their
/// kind: people (Authors, Directors, Actors) and genres (Genres, Game
/// genres, Music genres).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagKind {
    People,
    Genres,
}

impl TagCategory {
    pub const ALL: [TagCategory; 13] = [
        TagCategory::Genres,
        TagCategory::GameGenres,
        TagCategory::MusicGenres,
        TagCategory::Mood,
        TagCategory::Season,
        TagCategory::Languages,
        TagCategory::Authors,
        TagCategory::Directors,
        TagCategory::Actors,
        TagCategory::Studios,
        TagCategory::Streaming,
        TagCategory::AnimationStyle,
        TagCategory::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TagCategory::Genres => "Genres",
            TagCategory::GameGenres => "Game genres",
            TagCategory::MusicGenres => "Music genres",
            TagCategory::Mood => "Mood",
            TagCategory::Season => "Season",
            TagCategory::Languages => "Languages",
            TagCategory::Authors => "Authors",
            TagCategory::Directors => "Directors",
            TagCategory::Actors => "Actors",
            TagCategory::Studios => "Studios",
            TagCategory::Streaming => "Streaming",
            TagCategory::AnimationStyle => "Animation style",
            TagCategory::Other => "Other",
        }
    }

    /// The category as a role after a tag in more than one category:
    /// "#clint-eastwood · Actor", "#sports · Game genre".
    pub fn role_label(self) -> &'static str {
        match self {
            TagCategory::Genres => "Genre",
            TagCategory::GameGenres => "Game genre",
            TagCategory::MusicGenres => "Music genre",
            TagCategory::Mood => "Mood",
            TagCategory::Season => "Season",
            TagCategory::Languages => "Language",
            TagCategory::Authors => "Author",
            TagCategory::Directors => "Director",
            TagCategory::Actors => "Actor",
            TagCategory::Studios => "Studio",
            TagCategory::Streaming => "Streaming",
            TagCategory::AnimationStyle => "Animation style",
            TagCategory::Other => "Other",
        }
    }

    /// The role in lower case with its article: "an actor", "a game genre",
    /// for the usage line of the categories dialog.
    pub fn role_with_article(self) -> String {
        let role = self.role_label().to_lowercase();
        let article = if role.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {role}")
    }

    /// The category a new tag starts on: game genres for a game, music
    /// genres for music, genres otherwise.
    pub fn default_for(media: MediaType) -> TagCategory {
        match media {
            MediaType::Games => TagCategory::GameGenres,
            MediaType::Music => TagCategory::MusicGenres,
            _ => TagCategory::Genres,
        }
    }

    /// Whether the tag picker lists this category for an entry of `media`
    /// before anything is typed: game genres only for games, music genres
    /// only for music, actors and streaming only for movies and shows. A
    /// search finds every tag.
    pub fn offered_for(self, media: MediaType) -> bool {
        match self {
            TagCategory::GameGenres => media == MediaType::Games,
            TagCategory::MusicGenres => media == MediaType::Music,
            TagCategory::Actors => matches!(media, MediaType::Movies | MediaType::Shows),
            _ => self.used_for(media),
        }
    }

    /// Whether an entry of `media` can have tags of this category at all:
    /// Streaming is for movies and shows only, so the new-tag dialog and
    /// the Filter popover leave it out for the other types. Every other
    /// category is for every type.
    pub fn used_for(self, media: MediaType) -> bool {
        self != TagCategory::Streaming || matches!(media, MediaType::Movies | MediaType::Shows)
    }

    /// The kind of tag the category holds, when a tag of that kind can sit
    /// in more than one category. None for the rest, whose tags have one
    /// category each.
    pub fn kind(self) -> Option<TagKind> {
        match self {
            TagCategory::Authors | TagCategory::Directors | TagCategory::Actors => {
                Some(TagKind::People)
            }
            TagCategory::Genres | TagCategory::GameGenres | TagCategory::MusicGenres => {
                Some(TagKind::Genres)
            }
            _ => None,
        }
    }

    /// Whether a tag of this category can also be in `other`: both hold
    /// people, or both hold genres. A category never shares with itself.
    pub fn shares_kind_with(self, other: TagCategory) -> bool {
        other != self && self.kind().is_some() && self.kind() == other.kind()
    }

    /// The categories a tag of this category can be in, this one included:
    /// the three people groups, the three genre groups, or just this one.
    pub fn family(self) -> Vec<TagCategory> {
        TagCategory::ALL
            .into_iter()
            .filter(|&c| c == self || self.shares_kind_with(c))
            .collect()
    }

    /// The categories whose tags have exactly one category (Mood, Season,
    /// Languages, Studios, Streaming, Animation style, Other): a tag in one
    /// of them can be moved to another, and keeps every entry it is on.
    pub fn single_category_groups() -> Vec<TagCategory> {
        TagCategory::ALL
            .into_iter()
            .filter(|c| c.kind().is_none())
            .collect()
    }
}

/// "#clint-eastwood · Director, Actor": a tag with the roles it has here.
/// With no `categories` (a tag in one category) just "#tag".
pub fn tag_label(tag: &str, categories: &[TagCategory]) -> String {
    let roles: Vec<&str> = categories.iter().map(|c| c.role_label()).collect();
    if roles.is_empty() {
        format!("#{tag}")
    } else {
        format!("#{tag} · {}", roles.join(", "))
    }
}

/// The key tags sort by within a group: the tag itself, a leading "the"
/// ignored, so people sort by first name.
pub fn tag_sort_key(tag: &str) -> &str {
    tag.strip_prefix("the-").unwrap_or(tag)
}

/// `tags` A to Z by [`tag_sort_key`], ties by the whole tag.
pub fn sort_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut list: Vec<&str> = tags.into_iter().collect();
    list.sort_by(|a, b| tag_sort_key(a).cmp(tag_sort_key(b)).then(a.cmp(b)));
    list
}

/// "Stephen King" for #stephen-king: the words capitalized, for the rows of
/// a long group.
pub fn tag_display_name(tag: &str) -> String {
    tag.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_declaration_order_and_names_match_the_json_keys() {
        assert!(TagCategory::ALL.is_sorted());
        assert_eq!(TagCategory::ALL.len(), 13);
        for category in TagCategory::ALL {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(json, format!("\"{}\"", category.name()));
        }
    }

    #[test]
    fn the_two_new_categories_sit_where_the_phone_puts_them() {
        let names: Vec<&str> = TagCategory::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names[4..7], ["Season", "Languages", "Authors"]);
        assert_eq!(names[9..12], ["Studios", "Streaming", "Animation style"]);
    }

    #[test]
    fn people_and_genres_share_a_kind_and_the_rest_stand_alone() {
        assert_eq!(
            TagCategory::Directors.family(),
            [
                TagCategory::Authors,
                TagCategory::Directors,
                TagCategory::Actors
            ]
        );
        assert_eq!(
            TagCategory::MusicGenres.family(),
            [
                TagCategory::Genres,
                TagCategory::GameGenres,
                TagCategory::MusicGenres
            ]
        );
        assert_eq!(TagCategory::Mood.family(), [TagCategory::Mood]);
        assert!(!TagCategory::Genres.shares_kind_with(TagCategory::Actors));
        assert!(!TagCategory::Mood.shares_kind_with(TagCategory::Mood));
        assert_eq!(TagCategory::single_category_groups().len(), 7);
    }

    #[test]
    fn streaming_is_for_movies_and_shows_only() {
        assert!(TagCategory::Streaming.used_for(MediaType::Movies));
        assert!(TagCategory::Streaming.offered_for(MediaType::Shows));
        assert!(!TagCategory::Streaming.used_for(MediaType::Anime));
        assert!(!TagCategory::Streaming.offered_for(MediaType::Books));
        assert!(TagCategory::Languages.used_for(MediaType::Games));
        assert!(TagCategory::Actors.used_for(MediaType::Books));
        assert!(!TagCategory::Actors.offered_for(MediaType::Books));
    }

    #[test]
    fn labels_and_sorting_follow_the_phone() {
        assert_eq!(
            tag_label(
                "clint-eastwood",
                &[TagCategory::Directors, TagCategory::Actors]
            ),
            "#clint-eastwood · Director, Actor"
        );
        assert_eq!(tag_label("horror", &[]), "#horror");
        assert_eq!(TagCategory::Actors.role_with_article(), "an actor");
        assert_eq!(TagCategory::GameGenres.role_with_article(), "a game genre");
        assert_eq!(
            sort_tags(["the-cramps", "stephen-king", "bram-stoker"]),
            ["bram-stoker", "the-cramps", "stephen-king"]
        );
        assert_eq!(tag_display_name("ursula-k-le-guin"), "Ursula K Le Guin");
    }
}
