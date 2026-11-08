//! The fake lookup: a small fixed catalog the tests and the mock runs
//! search, with track lists, series parts and episodes for the fixture
//! library. It answers at once and never touches the network, so the app
//! asks it on the spot (`Service::is_instant`).

use super::{
    ArtistHit, CreatorMatch, Episode, FollowKind, GamePlatform, Hits, MATCHING_ARTISTS, Match,
    MusicList, MusicPage, MusicRequest, MusicScope, Reply, SeriesPart, SeriesParts, Source,
    TrackInfo,
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
        MediaType::Shows,
        "Severance",
        "Dan Erickson",
        "2022-02-18",
        "Mark leads a team whose memories are split between their work and their outside lives.",
        &["scifi", "dark", "mystery"],
        "fake:show:severance",
    ),
    other(
        MediaType::Movies,
        "The Lord of the Rings: The Hunt for Gollum",
        "Andy Serkis",
        "2027-12-17",
        "Andy Serkis directs and stars in a return to Middle-earth, following Gollum in the years before the Fellowship.",
        &["fantasy", "epic"],
        "fake:movie:hunt-for-gollum",
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
                // Still to come, for Discover (the phone's fake has it too).
                SeriesPart {
                    number: Some(4),
                    title: "The Lord of the Rings: The Hunt for Gollum".into(),
                    year: Some(2027),
                    released: Released::parse("2027-12-17"),
                    source: Some("fake:movie:hunt-for-gollum".into()),
                    creator: Some("Andy Serkis".into()),
                    cover: None,
                    extra: false,
                },
            ],
        }),
        (MediaType::Books, "berserk") => {
            const YEARS: [u16; 43] = [
                1990, 1991, 1991, 1992, 1993, 1993, 1994, 1994, 1995, 1995, 1996, 1996, 1997, 1997,
                1998, 1998, 1999, 1999, 2000, 2000, 2001, 2001, 2002, 2002, 2003, 2003, 2004, 2005,
                2005, 2005, 2006, 2007, 2008, 2009, 2010, 2011, 2013, 2016, 2017, 2018, 2021, 2021,
                2025,
            ];
            let mut parts: Vec<SeriesPart> = YEARS
                .iter()
                .enumerate()
                .map(|(i, &year)| {
                    let n = i as u32 + 1;
                    part(n, &format!("Berserk, Vol. {n}"), year, None)
                })
                .collect();
            // Volume 44, announced for a month, for Discover.
            parts.push(SeriesPart {
                number: Some(44),
                title: "Berserk, Vol. 44".into(),
                year: Some(2027),
                released: Released::parse("2027-03"),
                source: None,
                creator: Some("Kentaro Miura, Kouji Mori".into()),
                cover: None,
                extra: false,
            });
            Some(SeriesParts {
                name: "Berserk".into(),
                parts,
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

/// What the creators of the fixture library have on the way, for
/// Discover: the type, the creator as the library spells them, the
/// title, the release ("" for none yet), the description and the id.
const CREATOR_RELEASES: &[(MediaType, &str, &str, &str, &str, &str)] = &[
    (
        MediaType::Books,
        "Stephen King",
        "Stephen King: new novel",
        "2027-05",
        "Announced for spring 2027; title and details to follow.",
        "fake:book:king-2027",
    ),
    (
        MediaType::Books,
        "Patrick Rothfuss",
        "The Doors of Stone",
        "",
        "The third book of The Kingkiller Chronicle. No date yet.",
        "fake:book:doors-of-stone",
    ),
    (
        MediaType::Music,
        "Radiohead",
        "Radiohead: new album",
        "2027-02",
        "The band's first record since A Moon Shaped Pool.",
        "fake:album:radiohead-2027",
    ),
    (
        MediaType::Music,
        "Alice in Chains",
        "Alice in Chains: new album",
        "",
        "The band has confirmed a new record is in the works. No date yet.",
        "fake:album:alice-in-chains",
    ),
];

/// The coming releases of the creator called `name` among `media`.
pub fn creator_releases(name: &str, media: MediaType) -> Vec<Match> {
    CREATOR_RELEASES
        .iter()
        .filter(|(m, creator, ..)| *m == media && creator.eq_ignore_ascii_case(name.trim()))
        .map(
            |(media, creator, title, released, description, source)| Match {
                released: Released::parse(released),
                creator: Some((*creator).to_owned()),
                description: (*description).to_owned(),
                kind: (*media == MediaType::Music).then_some(EntryKind::Album),
                ..Match::new(*media, *title, *source)
            },
        )
        .collect()
}

/// The seasons of a fixture show as the fake lists them: Severance with
/// its third season announced, for Discover.
pub fn seasons(source: &str) -> Option<super::ShowSeasons> {
    let season = |n: u32, released: &str| Season {
        season: n,
        released: Released::parse(released),
        status: None,
        fresh: None,
        manual: None,
    };
    match source {
        "fake:show:severance" => Some(super::ShowSeasons {
            seasons: vec![
                season(1, "2022-02-18"),
                season(2, "2025-01-17"),
                season(3, "2027"),
            ],
            canceled: false,
            kind: None,
        }),
        _ => None,
    }
}

/// The catalog's match with `source`.
pub fn by_source(source: &str) -> Option<Match> {
    CATALOG
        .iter()
        .find(|m| m.source == source)
        .map(Row::to_match)
}

/// The music the fake pages through (the canvas's "ghost" boards): the
/// artists with their fans, then each album with its artist, date, id and
/// songs, an artist's newest first.
const ARTISTS: &[(&str, u32)] = &[
    ("Ghost", 615_851),
    ("Ghostface Killah", 402_118),
    ("Ghost Town", 52_340),
    ("Massive Attack", 1_204_377),
];

type AlbumRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const ALBUMS: &[AlbumRow] = &[
    (
        "Ghost",
        "Skeletá",
        "2025-04-25",
        "fake:album:skeleta",
        &[
            "Peacefield",
            "Lachryma",
            "Satanized",
            "Guiding Lights",
            "De Profundis Borealis",
        ],
    ),
    (
        "Ghost",
        "Impera",
        "2022-03-11",
        "fake:album:impera",
        &[
            "Kaisarion",
            "Spillways",
            "Call Me Little Sunshine",
            "Hunter's Moon",
            "Watcher in the Sky",
        ],
    ),
    (
        "Ghost",
        "Seven Inches of Satanic Panic",
        "2019-09-13",
        "fake:album:seven-inches",
        &["Mary on a Cross", "Kiss the Go-Goat"],
    ),
    (
        "Ghost",
        "Prequelle",
        "2018-06-01",
        "fake:album:prequelle",
        &[
            "Rats",
            "Faith",
            "Dance Macabre",
            "Pro Memoria",
            "See the Light",
        ],
    ),
    (
        "Ghost",
        "Meliora",
        "2015-08-21",
        "fake:album:meliora",
        &[
            "Spirit",
            "From the Pinnacle to the Pit",
            "Cirice",
            "He Is",
            "Mummy Dust",
        ],
    ),
    (
        "Ghost",
        "Infestissumam",
        "2013-04-10",
        "fake:album:infestissumam",
        &[
            "Per Aspera ad Inferi",
            "Secular Haze",
            "Year Zero",
            "Ghuleh / Zombie Queen",
        ],
    ),
    (
        "Ghost",
        "Opus Eponymous",
        "2010-10-18",
        "fake:album:opus-eponymous",
        &[
            "Deus Culpa",
            "Con Clavi Con Dio",
            "Ritual",
            "Elizabeth",
            "Stand by Him",
        ],
    ),
    (
        "Ghostface Killah",
        "Fishscale",
        "2006-03-28",
        "fake:album:fishscale",
        &["Shakey Dog", "Back Like That"],
    ),
    (
        "Ghostface Killah",
        "Supreme Clientele",
        "2000-02-08",
        "fake:album:supreme-clientele",
        &["Nutmeg", "Cherchez LaGhost"],
    ),
    (
        "Ghostface Killah",
        "Ironman",
        "1996-10-29",
        "fake:album:ironman",
        &["Daytona 500", "All That I Got Is You"],
    ),
    (
        "Ghost Town",
        "Evolution",
        "2015-03-10",
        "fake:album:evolution",
        &["Bad Dreams"],
    ),
    (
        "Ghost Town",
        "The After Party",
        "2013-09-17",
        "fake:album:the-after-party",
        &["Universe Is Laughing"],
    ),
    (
        "Ghost Town",
        "Party in the Graveyard",
        "2011-11-01",
        "fake:album:party-in-the-graveyard",
        &["You're So Creepy"],
    ),
    (
        "Coldplay",
        "Ghost Stories",
        "2014-05-16",
        "fake:album:ghost-stories",
        &["Magic", "A Sky Full of Stars"],
    ),
    (
        "Nine Inch Nails",
        "Ghosts I–IV",
        "2008-03-02",
        "fake:album:ghosts-i-iv",
        &["1 Ghosts I", "2 Ghosts I"],
    ),
    (
        "The Police",
        "Ghost in the Machine",
        "1981-10-02",
        "fake:album:ghost-in-the-machine",
        &[
            "Spirits in the Material World",
            "Every Little Thing She Does Is Magic",
        ],
    ),
    (
        "Massive Attack",
        "Mezzanine",
        "1998-04-20",
        "deezer:album:302127",
        &["Angel", "Teardrop", "Inertia Creeps"],
    ),
];

fn album_match(row: &AlbumRow) -> Match {
    let (artist, title, released, source, _) = *row;
    by_source(source).unwrap_or_else(|| Match {
        creator: Some(artist.to_owned()),
        released: Released::parse(released),
        kind: Some(EntryKind::Album),
        description: format!("An album by {artist}."),
        cover_url: Some(format!("fake://cover/{source}")),
        ..Match::new(MediaType::Music, title, source)
    })
}

fn song_matches(row: &AlbumRow) -> impl Iterator<Item = Match> + '_ {
    let (artist, album, released, source, songs) = *row;
    songs.iter().map(move |song| Match {
        creator: Some(artist.to_owned()),
        released: Released::parse(released),
        kind: Some(EntryKind::Song),
        album: Some(album.to_owned()),
        album_source: Some(source.to_owned()),
        cover_url: Some(format!("fake://cover/{source}")),
        ..Match::new(
            MediaType::Music,
            *song,
            format!("fake:track:{}", text::slugify(&format!("{artist} {song}"))),
        )
    })
}

/// A music page of the fake: a search finds albums and songs whose title
/// or artist holds the query, and artists whose name does; an artist's
/// list is theirs, newest first.
pub fn music_page(request: &MusicRequest, limit: usize) -> MusicPage {
    let holds = |text: &str, query: &str| text.to_lowercase().contains(query);
    let (rows, artists): (Vec<Match>, Vec<ArtistHit>) = match &request.list {
        MusicList::Search(query) => {
            let query = query.trim().to_lowercase();
            let albums = ALBUMS
                .iter()
                .filter(|row| holds(row.0, &query) || holds(row.1, &query));
            let rows = match request.scope {
                MusicScope::Albums => albums.map(album_match).collect(),
                MusicScope::Songs => ALBUMS
                    .iter()
                    .flat_map(song_matches)
                    .filter(|m| {
                        holds(&m.title, &query)
                            || m.creator.as_deref().is_some_and(|c| holds(c, &query))
                    })
                    .collect(),
            };
            let mut artists: Vec<&(&str, u32)> = ARTISTS
                .iter()
                .filter(|(name, _)| holds(name, &query))
                .collect();
            artists.sort_by_key(|(_, fans)| std::cmp::Reverse(*fans));
            let artists = artists
                .into_iter()
                .take(MATCHING_ARTISTS)
                .map(|(name, _)| ArtistHit {
                    source: format!("fake:artist:{}", text::slugify(name)),
                    name: (*name).to_owned(),
                    picture_url: None,
                    albums: Some(ALBUMS.iter().filter(|row| row.0 == *name).count() as u32),
                })
                .collect();
            (rows, artists)
        }
        MusicList::Artist { name, .. } => {
            let albums = ALBUMS.iter().filter(|row| row.0 == name);
            let rows = match request.scope {
                MusicScope::Albums => albums.map(album_match).collect(),
                MusicScope::Songs => albums.flat_map(song_matches).collect(),
            };
            (rows, Vec::new())
        }
    };
    let end = (request.cursor + limit).min(rows.len());
    MusicPage {
        matches: rows.get(request.cursor..end).unwrap_or_default().to_vec(),
        total: Some(rows.len()),
        next: (end < rows.len()).then_some(end),
        artists: if request.cursor == 0 {
            artists
        } else {
            Vec::new()
        },
    }
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

    fn details(&self, found: &Match) -> Reply<Match> {
        Ok(by_source(&found.source).unwrap_or_else(|| found.clone()))
    }

    fn seasons(&self, source: &str) -> Reply<Option<super::ShowSeasons>> {
        Ok(seasons(source))
    }

    fn series_parts(&self, _source: &str, name: Option<&str>) -> Reply<Option<SeriesParts>> {
        let Some(name) = name else {
            return Ok(None);
        };
        Ok(series_parts(MediaType::Movies, name).or_else(|| series_parts(MediaType::Books, name)))
    }

    fn creator_kinds(&self) -> &'static [FollowKind] {
        &[FollowKind::Authors, FollowKind::Artists]
    }

    fn find_creator(&self, kind: FollowKind, name: &str) -> Reply<Option<CreatorMatch>> {
        Ok(Some(CreatorMatch {
            provider_id: format!("fake:{}:{name}", kind.key()),
            inactive: None,
        }))
    }

    fn creator_releases(
        &self,
        provider_id: &str,
        kind: FollowKind,
        _today: crate::model::Date,
    ) -> Reply<Vec<Match>> {
        let name = provider_id.rsplit(':').next().unwrap_or_default();
        let media = if kind == FollowKind::Artists {
            MediaType::Music
        } else {
            MediaType::Books
        };
        Ok(creator_releases(name, media))
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

    fn music_page(&self, request: &MusicRequest, limit: usize) -> Reply<MusicPage> {
        Ok(music_page(request, limit))
    }

    fn album_released(&self, album_source: &str) -> Reply<Option<Released>> {
        Ok(ALBUMS
            .iter()
            .find(|row| row.3 == album_source)
            .and_then(|row| Released::parse(row.2)))
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
        assert_eq!(berserk.parts.len(), 44);
        assert_eq!(berserk.parts[42].year, Some(2025));
        assert_eq!(berserk.parts[43].released, Released::parse("2027-03"));
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
