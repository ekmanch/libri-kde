//! Online lookup, faked for phase 1: a small fixed catalog the Add entry
//! dialog searches. Phase 4 asks the real providers instead.

use crate::model::{Date, Entry, EntryKind, MediaType, Season, Series, text};

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
    /// A book's edition, for the cover picker: "Hardcover", "Ebook".
    pub edition: Option<&'static str>,
    /// A book dated only by its ebook edition, so the year may be a
    /// reprint's.
    pub ebook_dated: bool,
    /// The top billed cast of a movie or show, for actor tag suggestions.
    pub cast: &'static [&'static str],
    /// The work's language as a tag name ("japanese"), when the lookup
    /// says and it is not English.
    pub language: Option<&'static str>,
    /// A show's first network or a movie's platform as a tag name
    /// ("netflix"), for the Streaming suggestion.
    pub network: Option<&'static str>,
}

/// The game platforms the lookup knows, A to Z: the name and its other
/// names (the abbreviation, alternative and own names), which a search
/// matches too.
pub const PLATFORMS: &[(&str, &str)] = &[
    ("Arcade", ""),
    ("Game Boy", "GB"),
    ("Game Boy Advance", "GBA"),
    ("Mega Drive", "Genesis"),
    ("Nintendo 64", "N64"),
    ("Nintendo Switch", "Switch"),
    ("PC", "Windows · Steam"),
    ("PlayStation", "PS1 · PSX"),
    ("PlayStation 2", "PS2"),
    ("PlayStation 5", "PS5"),
    (
        "Super Nintendo",
        "SNES · Super Famicom · Super Nintendo Entertainment System",
    ),
    ("Xbox Series X|S", "Xbox"),
];

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
        edition: None,
        ebook_dated: false,
        cast: &[],
        language: None,
        network: None,
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
        edition: None,
        ebook_dated: false,
        cast: &[],
        language: None,
        network: None,
    }
}

pub const CATALOG: &[Match] = &[
    game(
        "Street Fighter II: The World Warrior",
        "Capcom",
        "1992-06-10",
        "Eight world warriors fight their way to M. Bison. The arcade hit comes home.",
        &["fighting", "arcade"],
        &["Super Nintendo", "Arcade", "Game Boy", "Mega Drive", "PC"],
        "igdb:1098",
    ),
    game(
        "Street Fighter II Turbo",
        "Capcom",
        "1993-07-11",
        "The faster Street Fighter II, with the four bosses playable.",
        &["fighting"],
        &["Super Nintendo"],
        "igdb:3185",
    ),
    game(
        "Super Street Fighter II",
        "Capcom",
        "1994-06-25",
        "Four new world warriors join the fight.",
        &["fighting", "arcade"],
        &["Super Nintendo", "Mega Drive", "Arcade"],
        "igdb:3186",
    ),
    Match {
        cast: &["Julia Garner", "Josh Brolin", "Alden Ehrenreich"],
        ..other(
            MediaType::Movies,
            "Weapons",
            "Zach Cregger",
            "2025-08-08",
            "When all but one child from the same class vanish on the same night, a town tries to find out why.",
            &["horror", "mystery"],
            "tmdb:movie:1078605",
        )
    },
    other(
        MediaType::Movies,
        "Weapons of Mass Distraction",
        "Stephen Surjik",
        "1997-05-18",
        "Two media moguls fight over a football team.",
        &["comedy"],
        "tmdb:movie:74436",
    ),
    Match {
        cast: &["Song Kang-ho", "Lee Sun-kyun", "Cho Yeo-jeong"],
        language: Some("korean"),
        ..other(
            MediaType::Movies,
            "Parasite",
            "Bong Joon-ho",
            "2019-05-30",
            "A poor family schemes its way into the household of a wealthy one.",
            &["thriller", "dark"],
            "tmdb:movie:496243",
        )
    },
    Match {
        cast: &["Stephen Graham", "Owen Cooper", "Ashley Walters"],
        network: Some("netflix"),
        ..other(
            MediaType::Shows,
            "Adolescence",
            "Jack Thorne, Stephen Graham",
            "2025-03-13",
            "A 13-year-old is arrested for the murder of a classmate, and his family tries to understand why.",
            &["drama", "crime"],
            "tmdb:tv:249042",
        )
    },
    other(
        MediaType::Shows,
        "Adolescence of Utena",
        "Kunihiko Ikuhara",
        "1999-08-14",
        "Utena's story retold as a film.",
        &["fantasy"],
        "tmdb:tv:40891",
    ),
    Match {
        edition: Some("Ebook"),
        ebook_dated: true,
        ..other(
            MediaType::Books,
            "A Christmas Carol",
            "Charles Dickens",
            "2012-10-10",
            "A miser is visited by three spirits on Christmas Eve.",
            &["christmas", "classic", "ghosts"],
            "itunes:570254606",
        )
    },
    other(
        MediaType::Books,
        "A Christmas Carol and Other Christmas Writings",
        "Charles Dickens",
        "2003-11-04",
        "The Carol with Dickens's other Christmas stories.",
        &["christmas", "classic"],
        "openlibrary:OL8193416W",
    ),
    Match {
        edition: Some("Ebook"),
        ebook_dated: true,
        ..other(
            MediaType::Books,
            "A Christmas Carol (Illustrated)",
            "Charles Dickens",
            "2017-12-01",
            "The Carol with the original illustrations.",
            &["christmas", "classic"],
            "itunes:1319049962",
        )
    },
    Match {
        edition: Some("Hardcover"),
        ..other(
            MediaType::Books,
            "Dracula",
            "Bram Stoker",
            "1897-05-26",
            "Jonathan Harker travels to Transylvania to meet a client who is not what he seems.",
            &["horror", "classic", "gothic"],
            "openlibrary:OL17405W",
        )
    },
    Match {
        edition: Some("Paperback"),
        ..other(
            MediaType::Books,
            "Dracula",
            "Bram Stoker",
            "2003-02-25",
            "The Penguin Classics edition.",
            &["horror", "classic"],
            "openlibrary:OL24241M",
        )
    },
    Match {
        edition: Some("Ebook"),
        ebook_dated: true,
        ..other(
            MediaType::Books,
            "Dracula",
            "Bram Stoker",
            "2012-03-13",
            "The ebook edition.",
            &["horror", "classic"],
            "itunes:498187584",
        )
    },
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

/// What the lookup is doing, for the Add dialog's left pane. Phase 4 sets
/// it from the real providers; until then `LIBRI_LOOKUP` fakes it:
/// "loading", "offline", "not-set-up", "partial", "partial-none".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LookupState {
    #[default]
    Results,
    Loading,
    Offline,
    NotSetUp,
    /// Some sources failed; the rows are what the others found.
    Partial,
    /// Some sources failed and the others found nothing.
    PartialNone,
}

impl LookupState {
    pub fn from_env(value: &str) -> Option<LookupState> {
        Some(match value {
            "results" => LookupState::Results,
            "loading" => LookupState::Loading,
            "offline" => LookupState::Offline,
            "not-set-up" => LookupState::NotSetUp,
            "partial" => LookupState::Partial,
            "partial-none" => LookupState::PartialNone,
            _ => return None,
        })
    }

    /// Whether the pane shows rows in this state.
    pub fn shows_rows(self) -> bool {
        matches!(self, LookupState::Results | LookupState::Partial)
    }
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
    search_on(media, query, None)
}

/// [`search`] narrowed to the games that came out on `platform`.
pub fn search_on(media: MediaType, query: &str, platform: Option<&str>) -> Vec<&'static Match> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    CATALOG
        .iter()
        .filter(|m| m.media == media && m.title.to_lowercase().contains(&query))
        .filter(|m| {
            platform.is_none_or(|platform| {
                m.platforms
                    .iter()
                    .any(|p| text::normalize_platform(p).eq_ignore_ascii_case(platform))
            })
        })
        .collect()
}

/// The other matches for an entry's title, whose covers Pick a cover
/// offers: one of each lookup id, the entry's own match left out.
pub fn cover_choices(media: MediaType, title: &str, own: Option<&str>) -> Vec<&'static Match> {
    search(media, title)
        .into_iter()
        .filter(|m| own != Some(m.source))
        .collect()
}

/// The platforms whose name or other names contain `query`, case aside.
pub fn platforms_matching(query: &str) -> Vec<(&'static str, &'static str)> {
    let query = query.trim().to_lowercase();
    PLATFORMS
        .iter()
        .copied()
        .filter(|(name, others)| {
            query.is_empty() || format!("{name} {others}").to_lowercase().contains(&query)
        })
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
    fn a_platform_narrows_the_games_and_the_cover_choices_leave_out_the_own_match() {
        assert_eq!(search(MediaType::Games, "street fighter").len(), 3);
        assert_eq!(
            search_on(MediaType::Games, "street fighter", Some("Mega Drive")).len(),
            2
        );
        assert!(search_on(MediaType::Games, "street fighter", Some("Nintendo 64")).is_empty());
        let choices = cover_choices(MediaType::Books, "Dracula", Some("openlibrary:OL17405W"));
        assert_eq!(choices.len(), 2);
        assert!(choices.iter().all(|m| m.edition.is_some()));
        assert_eq!(platforms_matching("snes").len(), 1);
        assert_eq!(platforms_matching("genesis")[0].0, "Mega Drive");
    }

    #[test]
    fn every_match_has_a_valid_release_date() {
        for m in CATALOG {
            assert!(Released::parse(m.released).is_some(), "{}", m.title);
        }
    }
}
