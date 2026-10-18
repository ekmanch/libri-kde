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
        for key in ["platform", "formats", "kind", "album", "tracks", "seasons"] {
            assert!(!encoded.contains(&format!("\"{key}\"")), "{key}");
        }
        assert!(encoded.contains("\"tags\": {},\n"));
        assert!(encoded.contains("\"plans\": []\n"));
    }
}
