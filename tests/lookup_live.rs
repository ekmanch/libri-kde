//! The real providers, asked once each: AniList, Open Library with Apple
//! Books, Deezer and Wikidata always; TMDB and IGDB with their keys
//! (`secrets.json`). Skipped by a plain `cargo test`; run it by hand with:
//!
//! ```fish
//! cargo test --test lookup_live -- --ignored --nocapture
//! ```

use libri::lookup::{Keys, Service};
use libri::model::MediaType;

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
    use libri::discover::{self, Settings};
    use libri::model::{Date, codec};

    let dir = std::path::PathBuf::from(std::env::var("LIBRI_LIVE_DIR").expect("LIBRI_LIVE_DIR"));
    let settings = std::env::var("LIBRI_LIVE_SETTINGS").expect("LIBRI_LIVE_SETTINGS");
    let settings = Settings::parse(&std::fs::read_to_string(settings).unwrap());
    let path = libri::storage::data_dir().unwrap().join("library.json");
    let library = codec::decode(&std::fs::read_to_string(path).unwrap()).unwrap();
    let service = Service::real(&Keys::load(), Some(&dir));
    let file = dir.join("discover.json");
    // Goes on from what an earlier run in that folder left, as a retry does.
    let previous = discover::State::at(Some(&dir), None).record;
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
