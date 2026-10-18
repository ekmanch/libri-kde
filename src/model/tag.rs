use serde::{Deserialize, Serialize};

use super::MediaType;

/// Tag categories, in display order, which is also their order in
/// `library.json`. The JSON keys of the `tags` map are the display names.
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
    #[serde(rename = "Animation style")]
    AnimationStyle,
    #[serde(rename = "Other")]
    Other,
}

impl TagCategory {
    pub const ALL: [TagCategory; 11] = [
        TagCategory::Genres,
        TagCategory::GameGenres,
        TagCategory::MusicGenres,
        TagCategory::Mood,
        TagCategory::Season,
        TagCategory::Authors,
        TagCategory::Directors,
        TagCategory::Actors,
        TagCategory::Studios,
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
            TagCategory::Authors => "Authors",
            TagCategory::Directors => "Directors",
            TagCategory::Actors => "Actors",
            TagCategory::Studios => "Studios",
            TagCategory::AnimationStyle => "Animation style",
            TagCategory::Other => "Other",
        }
    }

    /// Whether the tag picker lists this category for an entry of `media`
    /// before anything is typed. A search finds every tag.
    pub fn offered_for(self, media: MediaType) -> bool {
        match self {
            TagCategory::GameGenres => media == MediaType::Games,
            TagCategory::MusicGenres => media == MediaType::Music,
            TagCategory::Actors => matches!(media, MediaType::Movies | MediaType::Shows),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_declaration_order_and_names_match_the_json_keys() {
        assert!(TagCategory::ALL.is_sorted());
        for category in TagCategory::ALL {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(json, format!("\"{}\"", category.name()));
        }
    }
}
