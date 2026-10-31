//! The fake lookup: a small fixed catalog the tests and the mock runs
//! search, with track lists, series parts and episodes for the fixture
//! library. It answers at once and never touches the network, so the app
//! asks it on the spot (`Service::is_instant`).

use super::{
    Episode, GamePlatform, Hits, Match, Reply, SeriesPart, SeriesParts, Source, TrackInfo,
};
use crate::model::{Entry, EntryKind, MediaType, Released, Season, Series, text};

/// One match of the catalog, as a constant.
struct Row {
    media: MediaType,
    title: &'static str,
    creator: &'static str,
    released: &'static str,
    description: &'static str,
    /// Tag suggestions, already in the library's spelling.
    tags: &'static [&'static str],
    platforms: &'static [&'static str],
    kind: Option<EntryKind>,
    source: &'static str,
    edition: Option<&'static str>,
    ebook_dated: bool,
    cast: &'static [&'static str],
    /// A language code ("ko"), when the lookup says and it is not English.
    language: Option<&'static str>,
    /// A show's first network as TMDB names it ("Netflix").
    network: Option<&'static str>,
}

impl Row {
    fn to_match(&self) -> Match {
        Match {
            released: Released::parse(self.released),
            ebook_dated: self.ebook_dated,
            creator: Some(self.creator.to_owned()),
            description: self.description.to_owned(),
            platforms: self.platforms.iter().map(|p| (*p).to_owned()).collect(),
            cast: self.cast.iter().map(|c| (*c).to_owned()).collect(),
            tags: self.tags.iter().map(|t| (*t).to_owned()).collect(),
            kind: self.kind,
            language: self.language.map(str::to_owned),
            network: self.network.map(str::to_owned),
            edition: self.edition.map(str::to_owned),
            // A made-up image per match, so the cover picker has choices;
            // nothing is ever fetched for it.
            cover_url: Some(format!("fake://cover/{}", self.source)),
            ..Match::new(self.media, self.title, self.source)
        }
    }
}

/// The game platforms the fake knows, A to Z: the name and its other
/// names, which a search matches too.
const PLATFORMS: &[(&str, &str)] = &[
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

/// The fake's platforms as the picker lists them.
pub fn platforms() -> Vec<GamePlatform> {
    PLATFORMS
        .iter()
        .enumerate()
        .map(|(i, (name, others))| GamePlatform {
            id: i as i64 + 1,
            name: (*name).to_owned(),
            aka: others
                .split(" · ")
                .filter(|o| !o.is_empty())
                .map(str::to_owned)
                .collect(),
        })
        .collect()
}

const fn game(
    title: &'static str,
    creator: &'static str,
    released: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
    platforms: &'static [&'static str],
    source: &'static str,
) -> Row {
    Row {
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
) -> Row {
    Row {
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

const CATALOG: &[Row] = &[
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
    Row {
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
    Row {
        cast: &["Song Kang-ho", "Lee Sun-kyun", "Cho Yeo-jeong"],
        language: Some("ko"),
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
    Row {
        cast: &["Stephen Graham", "Owen Cooper", "Ashley Walters"],
        network: Some("Netflix"),
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
    Row {
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
    Row {
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
    Row {
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
    Row {
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
    Row {
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

/// The album's track list, when the fake has one, with made-up ids.
pub fn track_list(artist: &str, album: &str) -> Option<Vec<TrackInfo>> {
    TRACK_LISTS
        .iter()
        .find(|(a, t, _)| a.eq_ignore_ascii_case(artist) && t.eq_ignore_ascii_case(album))
        .map(|(_, _, tracks)| {
            tracks
                .iter()
                .enumerate()
                .map(|(i, title)| TrackInfo {
                    id: format!("fake:track:{}:{}", text::slugify(album), i + 1),
                    title: (*title).to_owned(),
                })
                .collect()
        })
}

/// The parts of a series of `media`, in series order, when the fake knows
/// it: The Lord of the Rings and Berserk.
pub fn series_parts(media: MediaType, name: &str) -> Option<SeriesParts> {
    let part = |number: u32, title: &str, year: u16, source: Option<&str>| SeriesPart {
        number: Some(number),
        title: title.to_owned(),
        year: Some(year),
        released: Some(Released {
            year,
            month: None,
            day: None,
        }),
        source: source.map(str::to_owned),
        creator: None,
        cover: None,
        extra: false,
    };
    match (media, Series::display_name(name).to_lowercase().as_str()) {
        (MediaType::Movies, "the lord of the rings") => Some(SeriesParts {
            name: "The Lord of the Rings".into(),
            parts: vec![
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
            ],
        }),
        (MediaType::Books, "berserk") => {
            const YEARS: [u16; 43] = [
                1990, 1991, 1991, 1992, 1993, 1993, 1994, 1994, 1995, 1995, 1996, 1996, 1997, 1997,
                1998, 1998, 1999, 1999, 2000, 2000, 2001, 2001, 2002, 2002, 2003, 2003, 2004, 2005,
                2005, 2005, 2006, 2007, 2008, 2009, 2010, 2011, 2013, 2016, 2017, 2018, 2021, 2021,
                2025,
            ];
            Some(SeriesParts {
                name: "Berserk".into(),
                parts: YEARS
                    .iter()
                    .enumerate()
                    .map(|(i, &year)| {
                        let n = i as u32 + 1;
                        part(n, &format!("Berserk, Vol. {n}"), year, None)
                    })
                    .collect(),
            })
        }
        _ => None,
    }
}

/// The episodes of `season`, made up for the fixture: a weekly season
/// when the season's date is a full day and it is not the first, every
/// episode on the season's day otherwise.
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
/// ignored, narrowed to `platform` for games. Nothing for an empty query.
pub fn search(media: MediaType, query: &str, platform: Option<&str>) -> Vec<Match> {
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
        .map(Row::to_match)
        .collect()
}

/// The catalog's match with `source`.
pub fn by_source(source: &str) -> Option<Match> {
    CATALOG
        .iter()
        .find(|m| m.source == source)
        .map(Row::to_match)
}

/// The fake as a source.
pub struct Fake;

impl Source for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    fn name(&self) -> &'static str {
        "Fake"
    }

    fn types(&self) -> &'static [MediaType] {
        &MediaType::ALL
    }

    fn issued(&self, _source: &str) -> bool {
        true
    }

    fn search(&self, query: &str, media: MediaType, limit: usize) -> Reply<Hits> {
        let mut hits = search(media, query, None);
        hits.truncate(limit);
        Ok(Hits::of(hits))
    }

    fn platforms(&self) -> Reply<Vec<GamePlatform>> {
        Ok(platforms())
    }

    fn search_on_platform(
        &self,
        query: &str,
        platform: &GamePlatform,
        limit: usize,
    ) -> Reply<Hits> {
        let mut hits = search(MediaType::Games, query, Some(&platform.name));
        for hit in &mut hits {
            hit.searched_platform = Some(platform.name.clone());
        }
        hits.truncate(limit);
        Ok(Hits::of(hits))
    }

    fn match_title(
        &self,
        title: &str,
        media: MediaType,
        year: Option<u16>,
        _creator: Option<&str>,
    ) -> Reply<Option<Match>> {
        let hits = search(media, title, None);
        Ok(
            super::text::pick_by_title_and_year(&hits, title, year, |m| &m.title, Match::year)
                .map(|i| hits[i].clone()),
        )
    }

    fn series_parts(&self, _source: &str, name: Option<&str>) -> Reply<Option<SeriesParts>> {
        let Some(name) = name else {
            return Ok(None);
        };
        Ok(series_parts(MediaType::Movies, name).or_else(|| series_parts(MediaType::Books, name)))
    }

    fn cover_url(&self, _source: &str, _media: MediaType) -> Reply<Option<String>> {
        Ok(None)
    }

    fn cover_choices(&self, title: &str, media: MediaType, _creator: Option<&str>) -> Reply<Hits> {
        Ok(Hits::of(search(media, title, None)))
    }

    fn is_music(&self) -> bool {
        true
    }

    fn album_tracks(&self, _album_source: &str) -> Reply<Vec<TrackInfo>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_titles_of_the_type() {
        let games = search(MediaType::Games, "castlevania", None);
        assert_eq!(games.len(), 4);
        assert!(search(MediaType::Movies, "castlevania", None).is_empty());
        assert!(search(MediaType::Games, "  ", None).is_empty());
        assert_eq!(by_source("tmdb:movie:694").unwrap().title, "The Shining");
    }

    #[test]
    fn the_fixture_series_and_track_lists_are_listed() {
        let berserk = series_parts(MediaType::Books, "Berserk").unwrap();
        assert_eq!(berserk.parts.len(), 43);
        assert_eq!(berserk.parts[42].year, Some(2025));
        assert!(series_parts(MediaType::Movies, "The Lord of the Rings Collection").is_some());
        assert!(series_parts(MediaType::Games, "Berserk").is_none());
        assert_eq!(track_list("Alice in Chains", "Dirt").unwrap().len(), 13);
        assert!(track_list("Nobody", "Nothing").is_none());
    }

    #[test]
    fn a_platform_narrows_the_games() {
        assert_eq!(search(MediaType::Games, "street fighter", None).len(), 3);
        assert_eq!(
            search(MediaType::Games, "street fighter", Some("Mega Drive")).len(),
            2
        );
        assert!(search(MediaType::Games, "street fighter", Some("Nintendo 64")).is_empty());
        let platforms = platforms();
        assert_eq!(platforms.iter().filter(|p| p.matches("snes")).count(), 1);
        assert_eq!(
            platforms
                .iter()
                .find(|p| p.matches("genesis"))
                .unwrap()
                .name,
            "Mega Drive"
        );
    }

    #[test]
    fn every_match_has_a_valid_release_date_and_a_language_code() {
        for row in CATALOG {
            assert!(Released::parse(row.released).is_some(), "{}", row.title);
            if let Some(code) = row.language {
                assert!(
                    super::super::text::language_tag(code).is_some(),
                    "{}",
                    row.title
                );
            }
        }
    }
}
