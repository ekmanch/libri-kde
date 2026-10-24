//! Online lookup, faked for phase 1: a small fixed catalog the Add entry
//! dialog searches. Phase 4 asks the real providers instead.

use crate::model::{Date, Entry, EntryKind, MediaType, Season, Series};

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

/// One part of a series as the lookup lists it: TMDB's collection, IGDB's
/// collection or Wikidata's series.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesPart {
    pub number: Option<u32>,
    pub title: String,
    pub year: Option<u16>,
    /// The lookup id, when the source has one.
    pub source: Option<String>,
}

/// The parts of a series of `media`, in series order, when the lookup
/// knows it. Phase 4 fetches and keeps them; until then two series are
/// fixed here.
pub fn series_parts(media: MediaType, name: &str) -> Option<Vec<SeriesPart>> {
    let part = |number: u32, title: &str, year: u16, source: Option<&str>| SeriesPart {
        number: Some(number),
        title: title.to_owned(),
        year: Some(year),
        source: source.map(str::to_owned),
    };
    match (media, Series::display_name(name).to_lowercase().as_str()) {
        (MediaType::Movies, "the lord of the rings") => Some(vec![
            part(
                1,
                "The Lord of the Rings: The Fellowship of the Ring",
                2001,
                Some("tmdb:movie:120"),
            ),
            part(
                2,
                "The Lord of the Rings: The Two Towers",
                2002,
                Some("tmdb:movie:121"),
            ),
            part(
                3,
                "The Lord of the Rings: The Return of the King",
                2003,
                Some("tmdb:movie:122"),
            ),
        ]),
        (MediaType::Books, "berserk") => {
            const YEARS: [u16; 43] = [
                1990, 1991, 1991, 1992, 1993, 1993, 1994, 1994, 1995, 1995, 1996, 1996, 1997, 1997,
                1998, 1998, 1999, 1999, 2000, 2000, 2001, 2001, 2002, 2002, 2003, 2003, 2004, 2005,
                2005, 2005, 2006, 2007, 2008, 2009, 2010, 2011, 2013, 2016, 2017, 2018, 2021, 2021,
                2025,
            ];
            Some(
                YEARS
                    .iter()
                    .enumerate()
                    .map(|(i, &year)| {
                        let n = i as u32 + 1;
                        part(n, &format!("Berserk, Vol. {n}"), year, None)
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// One episode of a season, as TMDB lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub number: u32,
    /// None until TMDB has a title (its placeholder "Episode 5" counts as
    /// none).
    pub title: Option<String>,
    pub aired: Option<Date>,
    /// In minutes.
    pub runtime: Option<u32>,
}

/// The episodes of `season`, made up for the fixture: a weekly season
/// when the season's date is a full day and it is not the first, every
/// episode on the season's day otherwise. Phase 4 fetches them.
pub fn episodes(entry: &Entry, season: &Season) -> Vec<Episode> {
    const FILLER: [&str; 10] = [
        "Pilot",
        "Arrival",
        "The Long Night",
        "Small Hours",
        "Undertow",
        "Homecoming",
        "Static",
        "Open House",
        "Fault Lines",
        "The Last One",
    ];
    let full_date = season
        .released
        .is_some_and(|r| r.month.is_some() && r.day.is_some());
    let (count, weekly, runtime) = match (entry.media, season.season) {
        (MediaType::Anime, _) => (10, full_date, 24),
        (_, n) => (8, full_date && n > 1, 50),
    };
    let start = season.released.map(|r| r.available_from());
    (0..count)
        .map(|i| {
            let aired = start.map(|day| {
                if weekly {
                    day.add_days(i64::from(i) * 7)
                } else {
                    day
                }
            });
            Episode {
                number: i + 1,
                title: Some(FILLER[i as usize % FILLER.len()].to_owned()),
                aired,
                runtime: Some(runtime),
            }
        })
        .collect()
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
    fn the_fixture_series_and_episodes_are_listed() {
        let berserk = series_parts(MediaType::Books, "Berserk").unwrap();
        assert_eq!(berserk.len(), 43);
        assert_eq!(berserk[42].year, Some(2025));
        assert!(series_parts(MediaType::Movies, "The Lord of the Rings Collection").is_some());
        assert!(series_parts(MediaType::Games, "Berserk").is_none());
    }

    #[test]
    fn every_match_has_a_valid_release_date() {
        for m in CATALOG {
            assert!(Released::parse(m.released).is_some(), "{}", m.title);
        }
    }
}
