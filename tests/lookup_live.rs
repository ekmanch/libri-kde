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
