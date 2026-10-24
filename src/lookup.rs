//! Online lookup, faked for phase 1: a small fixed catalog the Add entry
//! dialog searches. Phase 4 asks the real providers instead.

use crate::model::{EntryKind, MediaType};

/// One match a lookup returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub media: MediaType,
    pub title: &'static str,
    pub creator: &'static str,
    pub released: &'static str,
    pub description: &'static str,
    /// Tag suggestions, already in the library's spelling.
    pub tags: &'static [&'static str],
    pub platforms: &'static [&'static str],
    pub kind: Option<EntryKind>,
    /// The lookup id, "igdb:1234".
    pub source: &'static str,
}

const fn game(
    title: &'static str,
    creator: &'static str,
    released: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
    platforms: &'static [&'static str],
    source: &'static str,
) -> Match {
    Match {
        media: MediaType::Games,
        title,
        creator,
        released,
        description,
        tags,
        platforms,
        kind: None,
        source,
    }
}

const fn other(
    media: MediaType,
    title: &'static str,
    creator: &'static str,
    released: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
    source: &'static str,
) -> Match {
    Match {
        media,
        title,
        creator,
        released,
        description,
        tags,
        platforms: &[],
        kind: if matches!(media, MediaType::Music) {
            Some(EntryKind::Album)
        } else {
            None
        },
        source,
    }
}

pub const CATALOG: &[Match] = &[
    game(
        "Castlevania: Belmont's Curse",
        "Evil Empire",
        "2026-10-15",
        "A new Castlevania from the team behind Dead Cells, with Motion Twin advising.",
        &["metroidvania", "action", "platformer", "soulslike"],
        &["PC", "PlayStation 5", "Xbox Series X|S", "Switch"],
        "igdb:305611",
    ),
    game(
        "Castlevania: Symphony of the Night",
        "Konami",
        "1997-03-20",
        "Alucard explores Dracula's castle in the game that shaped the metroidvania.",
        &["metroidvania", "classic", "dark"],
        &["PlayStation", "Xbox 360"],
        "igdb:1128",
    ),
    game(
        "Castlevania: Lords of Shadow",
        "MercurySteam",
        "2010-10-05",
        "Gabriel Belmont fights the Lords of Shadow to bring back his murdered wife.",
        &["action", "dark"],
        &["PlayStation 3", "Xbox 360", "PC"],
        "igdb:2012",
    ),
    game(
        "Castlevania Advance Collection",
        "Konami",
        "2021-09-23",
        "Four Castlevania games from the Game Boy Advance and Super Famicom.",
        &["metroidvania", "classic"],
        &["PC", "Switch", "PlayStation 4", "Xbox One"],
        "igdb:152237",
    ),
    game(
        "Hollow Knight",
        "Team Cherry",
        "2017-02-24",
        "A knight descends into the ruined insect kingdom of Hallownest.",
        &["metroidvania", "dark", "soulslike"],
        &["PC", "Switch"],
        "igdb:14593",
    ),
    other(
        MediaType::Movies,
        "Dune",
        "Denis Villeneuve",
        "2021-10-22",
        "Paul Atreides travels to the desert planet Arrakis, where his family is betrayed.",
        &["scifi", "epic"],
        "tmdb:movie:438631",
    ),
    other(
        MediaType::Movies,
        "Dune: Part Three",
        "Denis Villeneuve",
        "2026-12-18",
        "The final chapter of Villeneuve's Dune adaptation.",
        &["scifi", "epic"],
        "tmdb:movie:1170608",
    ),
    other(
        MediaType::Movies,
        "The Shining",
        "Stanley Kubrick",
        "1980-05-23",
        "A writer takes a winter caretaker job at an isolated mountain hotel.",
        &["horror", "classic", "psychological"],
        "tmdb:movie:694",
    ),
    other(
        MediaType::Shows,
        "The Fall of the House of Usher",
        "Mike Flanagan",
        "2023-10-12",
        "The ruthless heirs of a pharmaceutical dynasty start dying one by one.",
        &["horror", "dark", "gothic"],
        "tmdb:tv:157065",
    ),
    other(
        MediaType::Books,
        "The Shining",
        "Stephen King",
        "1977-01-28",
        "Jack Torrance takes his family to the Overlook Hotel for the winter.",
        &["horror", "classic"],
        "openlibrary:OL81631W",
    ),
    other(
        MediaType::Books,
        "Piranesi",
        "Susanna Clarke",
        "2020-09-15",
        "A man lives alone in a house of endless halls and tides.",
        &["fantasy", "mystery"],
        "openlibrary:OL20811929W",
    ),
    other(
        MediaType::Music,
        "Mezzanine",
        "Massive Attack",
        "1998-04-20",
        "The third album by Massive Attack.",
        &["electronic", "trip-hop", "dark"],
        "deezer:album:302127",
    ),
    other(
        MediaType::Anime,
        "Mushishi",
        "Hiroshi Nagahama",
        "2005-10-23",
        "Ginko wanders the countryside studying mushi, the most basic forms of life.",
        &["fantasy", "cozy", "melancholic"],
        "anilist:457",
    ),
];

/// Track lists for albums the fixture has, by artist and album title.
/// Phase 4 fetches them from the album's lookup instead.
const TRACK_LISTS: &[(&str, &str, &[&str])] = &[
    (
        "Alice in Chains",
        "Dirt",
        &[
            "Them Bones",
            "Dam That River",
            "Rain When I Die",
            "Down in a Hole",
            "Sickman",
            "Rooster",
            "Junkhead",
            "Dirt",
            "God Smack",
            "Iron Gland",
            "Hate to Feel",
            "Angry Chair",
            "Would?",
        ],
    ),
    (
        "Metallica",
        "Master of Puppets",
        &[
            "Battery",
            "Master of Puppets",
            "The Thing That Should Not Be",
            "Welcome Home (Sanitarium)",
            "Disposable Heroes",
            "Leper Messiah",
            "Orion",
            "Damage, Inc.",
        ],
    ),
    (
        "Pink Floyd",
        "The Dark Side of the Moon",
        &[
            "Speak to Me",
            "Breathe",
            "On the Run",
            "Time",
            "The Great Gig in the Sky",
            "Money",
            "Us and Them",
            "Any Colour You Like",
            "Brain Damage",
            "Eclipse",
        ],
    ),
    (
        "Radiohead",
        "In Rainbows",
        &[
            "15 Step",
            "Bodysnatchers",
            "Nude",
            "Weird Fishes/Arpeggi",
            "All I Need",
            "Faust Arp",
            "Reckoner",
            "House of Cards",
            "Jigsaw Falling into Place",
            "Videotape",
        ],
    ),
    (
        "The Beatles",
        "Abbey Road",
        &[
            "Come Together",
            "Something",
            "Maxwell's Silver Hammer",
            "Oh! Darling",
            "Octopus's Garden",
            "I Want You (She's So Heavy)",
            "Here Comes the Sun",
            "Because",
            "You Never Give Me Your Money",
            "Sun King",
            "Mean Mr. Mustard",
            "Polythene Pam",
            "She Came In Through the Bathroom Window",
            "Golden Slumbers",
            "Carry That Weight",
            "The End",
            "Her Majesty",
        ],
    ),
];

/// The album's track list, when the lookup has one.
pub fn track_list(artist: &str, album: &str) -> Option<&'static [&'static str]> {
    TRACK_LISTS
        .iter()
        .find(|(a, t, _)| a.eq_ignore_ascii_case(artist) && t.eq_ignore_ascii_case(album))
        .map(|(_, _, tracks)| *tracks)
}

/// The matches for `query` among `media`: titles that contain it, case
/// ignored. Nothing for an empty query.
pub fn search(media: MediaType, query: &str) -> Vec<&'static Match> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    CATALOG
        .iter()
        .filter(|m| m.media == media && m.title.to_lowercase().contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Released;

    #[test]
    fn search_finds_titles_of_the_type() {
        let games = search(MediaType::Games, "castlevania");
        assert_eq!(games.len(), 4);
        assert!(search(MediaType::Movies, "castlevania").is_empty());
        assert!(search(MediaType::Games, "  ").is_empty());
    }

    #[test]
    fn every_match_has_a_valid_release_date() {
        for m in CATALOG {
            assert!(Released::parse(m.released).is_some(), "{}", m.title);
        }
    }
}
