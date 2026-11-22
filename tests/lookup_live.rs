//! The real providers, asked once each: AniList, Open Library with Apple
//! Books, Deezer and Wikidata always; TMDB and IGDB with their keys
//! (`secrets.json`). Skipped by a plain `cargo test`; run it by hand with:
//!
//! ```fish
//! cargo test --test lookup_live -- --ignored --nocapture
//! ```

use libri::lookup::{FollowKind, Keys, Service};
use libri::model::{Date, MediaType};

#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn the_real_providers_answer_a_search_and_a_pick() {
    let keys = Keys::load();
    let service = Service::real(&keys, None);
    let cases = [
        (MediaType::Anime, "mushishi"),
        (MediaType::Books, "dracula"),
        (MediaType::Music, "mezzanine massive attack"),
        (MediaType::Movies, "the shining"),
        (MediaType::Shows, "adolescence"),
        (MediaType::Games, "street fighter ii"),
    ];
    for (media, query) in cases {
        if service.sources_for(media).is_empty() {
            println!("{media:?}: not set up, skipped");
            continue;
        }
        let result = service.search(query, media, None);
        println!(
            "{media:?} \"{query}\": {} matches, state {:?}, failures {:?}",
            result.matches.len(),
            result.state(),
            result.failures
        );
        assert!(
            !result.matches.is_empty(),
            "{media:?}: no matches for {query}: {:?}",
            result.failures
        );
        let first = &result.matches[0];
        let source = service
            .source_of(&first.source)
            .expect("a source for the id");
        let full = source
            .details(first)
            .unwrap_or_else(|e| panic!("{media:?} details: {e}"));
        println!(
            "  {} ({}) by {:?}, {} tags, cover {:?}, series {:?}, seasons {}",
            full.title,
            full.year_text(),
            full.creator,
            full.tags.len(),
            full.cache_cover_url().map(|u| &u[..u.len().min(60)]),
            full.series.as_ref().map(|s| &s.name),
            full.seasons.len()
        );
        assert!(full.cache_cover_url().is_some(), "{media:?}: no cover");
        let cover = source
            .cover_url(&full.source, media)
            .unwrap_or_else(|e| panic!("{media:?} cover: {e}"));
        assert!(cover.is_some(), "{media:?}: cover_url gave nothing");
    }
    if service.sources_for(MediaType::Games).is_empty() {
        return;
    }
    let platforms = service.platforms().expect("the platform list");
    println!(
        "{} platforms, e.g. {:?}",
        platforms.len(),
        platforms.iter().find(|p| p.name == "Super Nintendo")
    );
    assert!(platforms.len() > 100);
    // A game is dated per platform by its full release: Hades left early
    // access on the PC in 2020, two years after it started.
    let hades = service.search("hades", MediaType::Games, None);
    let hit = hades
        .matches
        .iter()
        .find(|m| m.source == "igdb:113112")
        .expect("Hades among the matches");
    println!("  Hades per platform: {:?}", hit.platform_releases);
    assert_eq!(
        hit.released_on(["PC"]).map(|r| r.to_string()).as_deref(),
        Some("2020-09-17")
    );
    // The game by its id, for the daily check, and a studio's games with
    // a release on any platform still to come.
    let igdb = service.source_of("igdb:113112").expect("IGDB");
    let game = igdb.game("igdb:113112").expect("an answer").expect("Hades");
    assert_eq!(game.released_for(Some("PC")), hit.released_on(["PC"]));
    // Rockstar North, by its id.
    let today = Date::parse("2026-11-15").expect("a date");
    let coming = igdb
        .creator_releases("igdb:company:365", FollowKind::Studios, today)
        .expect("the studio's games");
    for game in &coming {
        println!(
            "  {}: first {:?}, on PC {:?}",
            game.title,
            game.released.map(|r| r.to_string()),
            game.released_for(Some("PC")).map(|r| r.to_string())
        );
    }
}

#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn episodes_series_and_tracks_come_from_the_real_providers() {
    let keys = Keys::load();
    let service = Service::real(&keys, None);
    if let Some(tmdb) = service.source_of("tmdb:tv:66732")
        && tmdb.is_configured()
    {
        let episodes = tmdb.season_episodes("tmdb:tv:66732", 1).unwrap().unwrap();
        println!(
            "Stranger Things S1: {} episodes, first {:?}",
            episodes.len(),
            episodes.first()
        );
        assert_eq!(episodes.len(), 8);
        let parts = tmdb.series_parts("tmdb:movie:121", None).unwrap().unwrap();
        println!("{}: {} parts", parts.name, parts.parts.len());
        assert_eq!(parts.parts.len(), 3);
    }
    let deezer = service.music().expect("Deezer");
    let tracks = deezer.album_tracks("deezer:album:302127").unwrap();
    println!("Mezzanine: {} tracks", tracks.len());
    assert!(tracks.len() >= 11);
    let photo = deezer.artist_photo_url("Massive Attack").unwrap();
    println!("photo {photo:?}");
    assert!(photo.is_some());
    let books = service.source_of("openlibrary:OL17405W").expect("books");
    let series = books.series_parts("openlibrary:OL27448W", None).unwrap();
    println!(
        "Fellowship series: {:?}",
        series.as_ref().map(|s| (&s.name, s.parts.len()))
    );
    let anime = service.source_of("anilist:154587").expect("AniList");
    let seasons = anime.seasons("anilist:154587").unwrap().unwrap();
    println!(
        "Frieren: {} seasons, kind {:?}",
        seasons.seasons.len(),
        seasons.kind
    );
    assert!(!seasons.seasons.is_empty());
}

#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn wikidata_knows_a_books_series() {
    let wikidata = libri::lookup::wikidata::Wikidata::new();
    let found = wikidata.series_of("The Fellowship of the Ring", Some("J. R. R. Tolkien"));
    println!(
        "{:?}",
        found
            .as_ref()
            .map(|f| f.as_ref().map(|(p, n)| (&p.name, p.parts.len(), n)))
    );
    let (parts, number) = found.unwrap().expect("a series");
    assert_eq!(parts.name, "The Lord of the Rings");
    assert_eq!(number, Some(1));
}

#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn a_cover_is_resolved_and_its_image_fetched() {
    use libri::lookup::store::ImageStore;
    let service = Service::real(&Keys::load(), None);
    let library = libri::model::codec::decode(libri::fixtures::LIBRARY_JSON).unwrap();
    let thing = library
        .entry("movies/the-thing-1982")
        .expect("the fixture's Thing");
    let (url, matched) = libri::artwork::resolve_cover(&service, thing).unwrap();
    println!("The Thing: {url:?} matched to {matched:?}");
    assert!(
        matched
            .as_deref()
            .unwrap_or_default()
            .starts_with("tmdb:movie:")
    );
    let url = url.expect("a cover");
    let dir = std::env::temp_dir().join(format!("libri-live-{}", std::process::id()));
    let store = ImageStore::at(Some(dir.clone()));
    let path = store.fetch(&url).expect("the download");
    let size = std::fs::metadata(&path).unwrap().len();
    println!("{} bytes at {}", size, path.display());
    assert!(size > 5_000);
    assert!(store.cached(&url).is_some());
    let photo = libri::artwork::resolve_artist(&service, "Alice in Chains").unwrap();
    println!("Alice in Chains: {photo:?}");
    assert!(photo.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

/// Frieren as the phone's standing check leaves it: an anime whose
/// `cover_from` is its TMDB show. The desktop shows that poster, the
/// AniList cover once it is picked under the entry's own id, and offers
/// a larger copy of each in the viewer.
#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn an_animes_tmdb_cover_from_is_shown() {
    use libri::model::MediaType;
    let service = Service::real(&Keys::load(), None);
    let library = libri::model::codec::decode(libri::fixtures::LIBRARY_JSON).unwrap();
    let mut frieren = library
        .entry("anime/frieren-beyond-journeys-end-2023")
        .expect("the fixture's Frieren")
        .clone();
    frieren.source = Some("anilist:154587".into());
    frieren.tmdb = Some("tmdb:tv:209867".into());
    frieren.cover_from = frieren.tmdb.clone();
    let (poster, _) = libri::artwork::resolve_cover(&service, &frieren).unwrap();
    println!("Frieren, cover_from its TMDB show: {poster:?}");
    let poster = poster.expect("TMDB's poster");
    assert!(poster.starts_with("https://image.tmdb.org/"), "{poster}");
    frieren.cover_from = frieren.source.clone();
    let (own, _) = libri::artwork::resolve_cover(&service, &frieren).unwrap();
    println!("Frieren, cover_from its own AniList id: {own:?}");
    let own = own.expect("AniList's cover");
    assert!(own.contains("anilist"), "{own}");
    for url in [&poster, &own] {
        let width = libri::lookup::largest_cover_width(url, false);
        println!("  largest width of {url}: {width:?}");
    }
    assert_eq!(frieren.media, MediaType::Anime);
}

/// The whole daily check on the real library and the real `settings.json`,
/// writing only to `LIBRI_LIVE_DIR`:
///
/// ```fish
/// LIBRI_LIVE_SETTINGS=../library/settings.json LIBRI_LIVE_DIR=/tmp/live \
///     cargo test --test lookup_live -- --ignored --nocapture the_daily_check
/// ```
#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn the_daily_check_runs_on_the_real_library() {
    use libri::discover;
    use libri::model::{Date, codec};

    let dir = std::path::PathBuf::from(std::env::var("LIBRI_LIVE_DIR").expect("LIBRI_LIVE_DIR"));
    let settings = std::env::var("LIBRI_LIVE_SETTINGS").expect("LIBRI_LIVE_SETTINGS");
    let settings = libri::shared::decode(&std::fs::read_to_string(settings).unwrap()).unwrap();
    let path = libri::storage::data_dir().unwrap().join("library.json");
    let library = codec::decode(&std::fs::read_to_string(path).unwrap()).unwrap();
    let service = Service::real(&Keys::load(), Some(&dir));
    let file = dir.join("discover.json");
    // Goes on from what an earlier run in that folder left, as a retry does.
    let previous = discover::State::at(Some(&dir)).record().clone();
    let start = std::time::Instant::now();
    let outcome = discover::run(
        &service,
        &library,
        Date::today(),
        &settings,
        &previous,
        Some(&file),
    );
    println!("took {:?}", start.elapsed());
    println!(
        "complete: {}, stopped: {:?}",
        outcome.complete, outcome.stopped
    );
    println!("asked: {}", outcome.sources.join(", "));
    println!("{} items", outcome.items.len());
    for item in &outcome.items {
        println!(
            "  {:?} {} [{}] {}",
            item.released.map(|r| r.to_string()),
            item.full_title(),
            item.provider,
            item.why
        );
    }
}

#[test]
#[ignore = "asks Deezer; run with --ignored"]
fn deezer_pages_a_search_and_an_artists_albums_and_songs() {
    use libri::lookup::{MusicList, MusicRequest, MusicScope};
    let service = Service::real(&Keys::default(), None);
    let search = |scope, cursor| MusicRequest {
        list: MusicList::Search("ghost".into()),
        scope,
        cursor,
    };
    let first = service.music_page(&search(MusicScope::Albums, 0)).unwrap();
    let names: Vec<&str> = first.artists.iter().map(|a| a.name.as_str()).collect();
    println!("albums: {:?} total, artists {names:?}", first.total);
    assert!(!first.matches.is_empty() && first.next == Some(25));
    assert!(names.contains(&"Ghost"), "Ghost among {names:?}");
    let next = service
        .music_page(&search(MusicScope::Albums, first.next.unwrap()))
        .unwrap();
    assert!(next.artists.is_empty());
    assert_ne!(next.matches[0].source, first.matches[0].source);
    let year = service
        .album_released(&first.matches[0].source)
        .unwrap()
        .map(|r| r.year);
    println!("first album {} from {year:?}", first.matches[0].title);
    assert!(year.is_some());

    let ghost = first.artists.iter().find(|a| a.name == "Ghost").unwrap();
    let artist = |scope| MusicRequest {
        list: MusicList::Artist {
            source: ghost.source.clone(),
            name: ghost.name.clone(),
        },
        scope,
        cursor: 0,
    };
    let albums = service.music_page(&artist(MusicScope::Albums)).unwrap();
    let years: Vec<u16> = albums.matches.iter().filter_map(|m| m.year()).collect();
    println!(
        "Ghost: {:?} albums, {:?}",
        albums.total,
        &years[..years.len().min(8)]
    );
    assert!(years.windows(2).all(|w| w[0] >= w[1]), "newest first");
    let songs = service.music_page(&artist(MusicScope::Songs)).unwrap();
    println!(
        "Ghost's songs: {} on the first page, next {:?}, first {:?}",
        songs.matches.len(),
        songs.next,
        songs.matches.first().map(|m| (&m.title, &m.album))
    );
    assert!(songs.matches.len() >= 25);
}

/// Deezer's search lists a recording once, so "Picture You" comes from
/// the 2014 single alone; the 2015 album's copy is added from its tracks.
#[test]
#[ignore = "asks Deezer; run with --ignored"]
fn deezer_search_shows_a_song_on_its_single_and_on_its_album() {
    use libri::lookup::{MusicList, MusicRequest, MusicScope};
    let service = Service::real(&Keys::default(), None);
    let page = service
        .music_page(&MusicRequest {
            list: MusicList::Search("the amazing".into()),
            scope: MusicScope::Songs,
            cursor: 0,
        })
        .unwrap();
    let copies: Vec<&str> = page
        .matches
        .iter()
        .filter(|m| m.title == "Picture You" && m.creator.as_deref() == Some("The Amazing"))
        .filter_map(|m| m.album_source.as_deref())
        .collect();
    println!("Picture You on {copies:?}");
    assert!(copies.contains(&"deezer:album:679137691"), "the single");
    assert!(copies.contains(&"deezer:album:9682786"), "the album");
}

#[test]
#[ignore = "asks the real providers; run with --ignored"]
fn wikidata_knows_who_died() {
    use libri::lookup::FollowKind;
    let wikidata = libri::lookup::wikidata::Wikidata::new();
    let names = ["Eva Cassidy".to_owned(), "Massive Attack".to_owned()];
    let found = wikidata.inactive_of(FollowKind::Artists, &names);
    println!("{found:?}");
    let found = found.expect("an answer");
    assert_eq!(
        found.get("Eva Cassidy").map(String::as_str),
        Some("Died 1996")
    );
    assert!(!found.contains_key("Massive Attack"));
}

/// Apple Music for an artist's photo: one search, the namesakes' album
/// lists, one artist page and one image (DESIGN.md "Artist photos").
#[test]
#[ignore = "asks Apple and reads one artist page; run with --ignored"]
fn apple_finds_an_artists_photo_and_tells_a_namesake_apart() {
    use libri::covers::artists::Reason;
    use libri::lookup::apple_artists::{AppleArtists, Found};
    let apple = AppleArtists::new();
    let namesakes = apple.namesakes("Massive Attack").expect("Apple answers");
    println!("Massive Attack: {} namesake(s)", namesakes.len());
    for namesake in &namesakes {
        println!("  {} {} {}", namesake.id, namesake.name, namesake.page);
    }
    assert!(!namesakes.is_empty());
    match apple.find("Massive Attack", &["Mezzanine".to_owned()], &[], 640) {
        Ok(Found::Photo(bytes)) => {
            let size = libri::lookup::store::image_size(&bytes);
            println!("photo: {} bytes, {size:?}", bytes.len());
            assert_eq!(size, Some((640, 640)));
        }
        other => panic!("no photo of Massive Attack: {other:?}"),
    }
    // An album nobody of the name has: no match, and no page is read.
    let none = apple.find(
        "Massive Attack",
        &["Not An Album Of Theirs".to_owned()],
        &[],
        640,
    );
    // The same photo once more at the sharper size, from the match and
    // the address kept in memory: only the image is asked for.
    match apple.sharper("Massive Attack", &["Mezzanine".to_owned()], &[], 1200) {
        Ok(Some(bytes)) => {
            let size = libri::lookup::store::image_size(&bytes);
            println!("sharper photo: {} bytes, {size:?}", bytes.len());
            assert!(size.is_some_and(|(width, _)| width > 640), "{size:?}");
        }
        other => panic!("no sharper photo of Massive Attack: {other:?}"),
    }
    assert_eq!(none.ok(), Some(Found::Missing(Reason::NoMatch)));
    // No album saved, only a song: the song search tells who it is.
    let rows = apple
        .song_rows("Massive Attack", "Teardrop")
        .expect("Apple answers");
    println!("Teardrop: {} track(s)", rows.len());
    let ids: Vec<&str> = namesakes.iter().map(|n| n.id.as_str()).collect();
    assert!(
        rows.iter().any(|(id, track)| ids.contains(&id.as_str())
            && libri::covers::artists::same_title(track, "Teardrop")),
        "{rows:?}"
    );
}

/// The owner's case of 2026-11-22: Deezer's songs have no "Oats In the
/// Water", and the extended search finds it on Burgh Island (2012); its
/// cover, its date and its album's track list then come from Apple by
/// the ids alone.
#[test]
#[ignore = "asks Deezer and Apple; run with --ignored"]
fn extended_search_finds_a_song_deezer_does_not_show() {
    use libri::lookup::{MusicList, MusicRequest, MusicScope};
    let service = Service::real(&Keys::default(), None);
    let words = "oats in the water ben howard";
    let page = service
        .music_page(&MusicRequest {
            list: MusicList::Search(words.into()),
            scope: MusicScope::Songs,
            cursor: 0,
        })
        .unwrap();
    println!("Deezer: {} songs", page.matches.len());
    let found = service.extended_music(words, MusicScope::Songs).unwrap();
    for hit in &found {
        println!(
            "{} · {} · {} · {} ({})",
            hit.title,
            hit.creator.as_deref().unwrap_or("?"),
            hit.album.as_deref().unwrap_or("?"),
            hit.year_text(),
            hit.source
        );
    }
    let song = found
        .iter()
        .find(|m| m.album.as_deref() == Some("Burgh Island"))
        .expect("the song on Burgh Island");
    assert_eq!(song.title.to_lowercase(), "oats in the water");
    assert_eq!(song.creator.as_deref(), Some("Ben Howard"));
    assert_eq!(song.year(), Some(2012));
    assert!(song.source.starts_with("itunes:track:"));
    assert!(
        !page
            .matches
            .iter()
            .any(|m| m.title == song.title && m.creator == song.creator && m.album == song.album),
        "Deezer does not show it"
    );
    // Asked once: the second ask is the remembered answer.
    assert_eq!(
        service.cached_extended(words, MusicScope::Songs),
        Some(found.clone())
    );

    let apple = service.source_of(&song.source).expect("Apple Music");
    assert_eq!(apple.name(), "Apple Music");
    let cover = apple.cover_url(&song.source, MediaType::Music).unwrap();
    println!("cover: {cover:?}");
    assert!(cover.is_some_and(|url| url.contains("mzstatic.com")));
    let album = song.album_source.as_deref().unwrap();
    assert_eq!(
        service.album_released(album).unwrap().map(|r| r.year),
        Some(2012)
    );
    let tracks = apple.album_tracks(album).unwrap();
    println!(
        "tracks: {:?}",
        tracks.iter().map(|t| &t.title).collect::<Vec<_>>()
    );
    assert!(
        tracks
            .iter()
            .any(|t| t.title.to_lowercase() == "oats in the water")
    );
}
