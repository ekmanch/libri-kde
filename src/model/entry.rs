use serde::{Deserialize, Serialize};

use super::{Date, Released};

/// The six kinds of entry in the library. JSON uses the lowercase names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movies,
    Shows,
    Books,
    Games,
    Music,
    Anime,
}

impl MediaType {
    pub const ALL: [MediaType; 6] = [
        MediaType::Movies,
        MediaType::Shows,
        MediaType::Books,
        MediaType::Games,
        MediaType::Music,
        MediaType::Anime,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MediaType::Movies => "Movies",
            MediaType::Shows => "Shows",
            MediaType::Books => "Books",
            MediaType::Games => "Games",
            MediaType::Music => "Music",
            MediaType::Anime => "Anime",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    Want,
    InProgress,
    Done,
    Dnf,
}

impl EntryStatus {
    /// The statuses an entry of `media` can have. A movie or an album is
    /// finished in one sitting, so neither is ever "in progress", and music
    /// has no DNF.
    pub fn options_for(media: MediaType) -> &'static [EntryStatus] {
        match media {
            MediaType::Movies => &[EntryStatus::Want, EntryStatus::Done, EntryStatus::Dnf],
            MediaType::Music => &[EntryStatus::Want, EntryStatus::Done],
            _ => &[
                EntryStatus::Want,
                EntryStatus::InProgress,
                EntryStatus::Done,
                EntryStatus::Dnf,
            ],
        }
    }
}

/// Formats a book is owned in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    Audiobook,
    Physical,
}

/// Whether a music entry is an album or a single song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MusicKind {
    Album,
    Song,
}

/// One title in the library, of any type.
///
/// The field order here is the key order in `library.json`. Common fields
/// are always written, even when null, so an edit changes as few lines as
/// possible. Type-specific fields are omitted when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(rename = "type")]
    pub media: MediaType,
    pub title: String,
    #[serde(default)]
    pub released: Option<Released>,
    #[serde(default)]
    pub creator: Option<String>,
    pub status: EntryStatus,
    /// 1 to 10.
    #[serde(default)]
    pub rating: Option<u8>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub description: String,
    /// Hidden from the main lists unless "Include seasonal-only" is on.
    #[serde(default)]
    pub seasonal_only: bool,
    pub added: Date,
    /// The lookup id the entry was found by, "tmdb:movie:694", or null.
    #[serde(default)]
    pub source: Option<String>,
    // Games.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    // Books.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<Vec<BookFormat>>,
    // Music.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MusicKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Vec<Track>>,
    // Shows: in season number order, omitted when the show has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seasons: Option<Vec<Season>>,
}

impl Entry {
    /// The release year. A show with seasons counts from its first season,
    /// so sorting and the decade filter place it where it began.
    pub fn year(&self) -> Option<u16> {
        self.seasons
            .as_deref()
            .and_then(<[Season]>::first)
            .and_then(|season| season.released)
            .or(self.released)
            .map(|released| released.year)
    }

    /// False only when a release date is known and still in the future.
    pub fn is_released_on(&self, today: Date) -> bool {
        self.released.is_none_or(|r| r.is_released_on(today))
    }
}

/// A rated track on an album entry. Only rated tracks are stored; `id` is
/// the lookup id, when known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub rating: u8,
}

/// One season of a show. `status` is null when not set and never changes
/// the show's own status. `fresh` (`"new"` in the file) marks a season the
/// daily check added; it is written only when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Season {
    pub season: u32,
    #[serde(default)]
    pub released: Option<Released>,
    #[serde(default)]
    pub status: Option<EntryStatus>,
    #[serde(rename = "new", default, skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
}

impl Season {
    /// Shown with a "New" label, and the show with a "New season" badge.
    pub fn is_new(&self) -> bool {
        self.fresh == Some(true) && self.status.is_none()
    }
}
