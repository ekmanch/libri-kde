//! The one place `library.json` text is produced.
//!
//! The output is canonical and byte for byte what the phone app's
//! `library_codec.dart` writes, so a save from either app produces a
//! minimal git diff: two-space indent, `"key": value`, one trailing
//! newline, keys in model declaration order, tag categories in
//! [`TagCategory`](super::TagCategory) order, entries and plans in the order
//! they are stored, non-ASCII text written as is. Decoding then encoding a
//! canonical file gives back the same bytes.

use serde::Serialize;
use serde_json::ser::{PrettyFormatter, Serializer};

use super::Library;

/// An empty library in the canonical form: what the setup creates, and
/// what a real run starts from until GitHub's copy arrives.
pub const EMPTY: &str =
    "{\n  \"version\": 2,\n  \"tags\": {},\n  \"entries\": [],\n  \"plans\": []\n}\n";

/// Decodes `library.json` text. Tag categories come back in category order
/// whatever order the file had them in, so the next save moves them.
pub fn decode(text: &str) -> Result<Library, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn encode(library: &Library) -> String {
    let mut bytes = Vec::new();
    let mut serializer =
        Serializer::with_formatter(&mut bytes, PrettyFormatter::with_indent(b"  "));
    library
        .serialize(&mut serializer)
        .expect("a library always serializes");
    let mut text = String::from_utf8(bytes).expect("serde_json writes UTF-8");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::{EntryKind, TagCategory};

    #[test]
    fn the_fixture_round_trips_to_the_same_bytes() {
        let library = decode(fixtures::LIBRARY_JSON).unwrap();
        assert_eq!(
            encode(&library).as_bytes(),
            fixtures::LIBRARY_JSON.as_bytes()
        );
    }

    #[test]
    fn the_fixture_file_on_disk_round_trips_to_the_same_bytes() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fixtures/library.json");
        let original = std::fs::read(path).unwrap();
        let library = decode(std::str::from_utf8(&original).unwrap()).unwrap();
        assert_eq!(encode(&library).into_bytes(), original);
    }

    #[test]
    fn the_empty_library_is_canonical() {
        assert_eq!(encode(&Library::default()), EMPTY);
    }

    #[test]
    fn tag_categories_are_written_in_category_order() {
        let text = r#"{"version": 2, "tags": {"Other": ["favorite"], "Genres": ["horror"]}, "entries": [], "plans": []}"#;
        let encoded = encode(&decode(text).unwrap());
        assert_eq!(
            encoded,
            "{\n  \"version\": 2,\n  \"tags\": {\n    \"Genres\": [\n      \"horror\"\n    ],\n    \
             \"Other\": [\n      \"favorite\"\n    ]\n  },\n  \"entries\": [],\n  \"plans\": []\n}\n"
        );
    }

    #[test]
    fn common_fields_are_always_written_and_type_specific_ones_only_when_set() {
        let text = r#"{"version": 2, "entries": [{"id": "movies/x-2000", "type": "movies",
            "title": "Shōgun \"quoted\"", "status": "want", "added": "2026-10-01"}]}"#;
        let encoded = encode(&decode(text).unwrap());
        assert!(encoded.contains("\"title\": \"Shōgun \\\"quoted\\\"\",\n"));
        for key in ["released", "creator", "rating", "source"] {
            assert!(encoded.contains(&format!("\"{key}\": null")), "{key}");
        }
        for key in [
            "cover_from",
            "platform",
            "formats",
            "kind",
            "album",
            "tracks",
            "seasons",
        ] {
            assert!(!encoded.contains(&format!("\"{key}\"")), "{key}");
        }
        assert!(encoded.contains("\"tags\": {},\n"));
        assert!(encoded.contains("\"plans\": []\n"));
    }

    #[test]
    fn cover_from_is_written_right_after_source() {
        let text = r#"{"version": 2, "entries": [{"id": "books/x-1975", "type": "books",
            "title": "X", "status": "want", "added": "2026-10-01", "source": "openlibrary:OL1W",
            "cover_from": "itunes:2019", "formats": ["physical"]}]}"#;
        let encoded = encode(&decode(text).unwrap());
        let source_at = encoded.find("\"source\":").unwrap();
        let cover_at = encoded.find("\"cover_from\": \"itunes:2019\"").unwrap();
        let formats_at = encoded.find("\"formats\":").unwrap();
        assert!(source_at < cover_at && cover_at < formats_at);
    }

    #[test]
    fn tag_picks_keep_their_key_order_and_are_written_right_after_tags() {
        let text = r#"{"version": 2, "entries": [{"id": "movies/x-2004", "type": "movies",
            "title": "X", "status": "want", "tags": ["drama", "clint-eastwood", "morgan-freeman"],
            "tag_categories": {"morgan-freeman": ["Actors"], "clint-eastwood": ["Directors", "Actors"]},
            "added": "2026-10-01"}]}"#;
        let library = decode(text).unwrap();
        let entry = &library.entries[0];
        let picks = entry.tag_categories.as_ref().unwrap();
        assert_eq!(picks.0[0].0, "morgan-freeman");
        assert_eq!(
            picks.get("clint-eastwood"),
            Some(&[TagCategory::Directors, TagCategory::Actors][..])
        );
        let encoded = encode(&library);
        let tags_at = encoded.find("\"tags\": [").unwrap();
        let picks_at = encoded.find("\"tag_categories\": {").unwrap();
        let note_at = encoded.find("\"note\":").unwrap();
        assert!(tags_at < picks_at && picks_at < note_at);
        assert!(
            encoded.find("\"morgan-freeman\": [").unwrap()
                < encoded.find("\"clint-eastwood\": [").unwrap(),
            "the file's order is kept"
        );
    }

    #[test]
    fn the_fixture_has_the_phones_two_category_tag_and_the_new_fields() {
        let library = decode(fixtures::LIBRARY_JSON).unwrap();
        assert_eq!(
            library.categories_of("clint-eastwood"),
            [TagCategory::Directors, TagCategory::Actors]
        );
        let baby = library.entry("movies/million-dollar-baby-2004").unwrap();
        assert_eq!(
            library.categories_on(baby, "clint-eastwood"),
            [TagCategory::Directors, TagCategory::Actors]
        );
        let ugly = library
            .entry("movies/the-good-the-bad-and-the-ugly-1966")
            .unwrap();
        assert_eq!(
            library.categories_on(ugly, "clint-eastwood"),
            [TagCategory::Actors]
        );
        assert!(library.tags.contains_key(&TagCategory::Languages));
        assert!(library.tags.contains_key(&TagCategory::Streaming));
        let berserk = library.entry("books/berserk-vol-1-1990").unwrap();
        assert_eq!(berserk.pages, Some(224));
        assert_eq!(berserk.series.as_ref().map(|s| s.number), Some(Some(1)));
        let frieren = library
            .entries
            .iter()
            .find(|e| e.kind == Some(EntryKind::Series))
            .expect("an anime series");
        assert!(frieren.uses_seasons());
    }
}
