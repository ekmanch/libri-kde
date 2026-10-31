//! The API keys the lookups need (`secrets.json`, git-ignored; the phone's
//! `secrets.example.json` lists the names). A missing key leaves that
//! provider "not set up": nothing is asked and nothing crashes.

use std::path::{Path, PathBuf};

/// `TMDB_READ_TOKEN`, `IGDB_CLIENT_ID`, `IGDB_CLIENT_SECRET` and the
/// optional `GOOGLE_BOOKS_KEY`. AniList, Open Library, Apple Books,
/// Deezer and Wikidata need none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    pub tmdb_read_token: String,
    pub igdb_client_id: String,
    pub igdb_client_secret: String,
    pub google_books_key: String,
}

impl Keys {
    /// The keys for this run: each from its environment variable when set,
    /// else from the first `secrets.json` found, under the config folder
    /// (`~/.config/libri/`) or beside `Cargo.toml` (a development run).
    pub fn load() -> Keys {
        let mut keys = Keys::default();
        for path in Keys::files() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                keys = Keys::from_json(&text);
                break;
            }
        }
        let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        if let Some(token) = env("TMDB_READ_TOKEN") {
            keys.tmdb_read_token = token;
        }
        if let Some(id) = env("IGDB_CLIENT_ID") {
            keys.igdb_client_id = id;
        }
        if let Some(secret) = env("IGDB_CLIENT_SECRET") {
            keys.igdb_client_secret = secret;
        }
        if let Some(key) = env("GOOGLE_BOOKS_KEY") {
            keys.google_books_key = key;
        }
        keys
    }

    /// Where `secrets.json` is looked for, in order.
    pub fn files() -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Some(dir) = crate::storage::config_dir() {
            files.push(dir.join("secrets.json"));
        }
        files.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("secrets.json"));
        files
    }

    /// The phone's `secrets.json`: a flat object of the four names.
    pub fn from_json(text: &str) -> Keys {
        let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        let field = |name: &str| {
            json.get(name)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        Keys {
            tmdb_read_token: field("TMDB_READ_TOKEN"),
            igdb_client_id: field("IGDB_CLIENT_ID"),
            igdb_client_secret: field("IGDB_CLIENT_SECRET"),
            google_books_key: field("GOOGLE_BOOKS_KEY"),
        }
    }

    /// Whether any provider that needs a key has one.
    pub fn any(&self) -> bool {
        !self.tmdb_read_token.is_empty()
            || (!self.igdb_client_id.is_empty() && !self.igdb_client_secret.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phones_secrets_file_reads_and_a_missing_key_is_empty() {
        let keys = Keys::from_json(r#"{"TMDB_READ_TOKEN": " t ", "IGDB_CLIENT_ID": "id"}"#);
        assert_eq!(keys.tmdb_read_token, "t");
        assert_eq!(keys.igdb_client_id, "id");
        assert_eq!(keys.igdb_client_secret, "");
        assert!(keys.any());
        assert!(!Keys::from_json("not json").any());
        assert!(!Keys::files().is_empty());
    }
}
