//! The library connection on this computer: `connection.json` next to
//! the library, as on the phone (`connection_store.dart` and
//! `LibraryConnection`): the repository, the file and the branch, whether
//! the library was kept here after a Disconnect, and what GitHub last
//! said about the token's expiry. The token itself is in KDE Wallet
//! (`wallet`), never here.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::model::Date;
use crate::storage;

/// Where the library file is: a repo, a path and a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// "owner/repo".
    pub repository: String,
    /// The path of the file inside the repo.
    pub file: String,
    pub branch: String,
}

impl Target {
    /// "owner/repo:path@branch", the key a sync base is written for.
    pub fn key(&self) -> String {
        format!("{}:{}@{}", self.repository, self.file, self.branch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub target: Target,
    /// Disconnected, but the library stayed on this computer ("Keep it
    /// on this computer"): Libri shows and edits `library.json` without
    /// a sync until a repo is connected again.
    pub local: bool,
    /// When the token stops working, from GitHub's expiration header on
    /// the last successful answer; `None` until one said, or for a token
    /// that never expires.
    pub token_expires: Option<Timestamp>,
    /// The day the expiry warning was last shown, so it shows once a day.
    pub token_warned_on: Option<Date>,
}

impl Connection {
    pub fn new(target: Target) -> Connection {
        Connection {
            target,
            local: false,
            token_expires: None,
            token_warned_on: None,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut json = json!({
            "repository": self.target.repository,
            "file": self.target.file,
            "branch": self.target.branch,
        });
        if self.local {
            json["local"] = Value::Bool(true);
        }
        if let Some(expires) = self.token_expires {
            json["tokenExpires"] = Value::String(expires.to_rfc3339());
        }
        if let Some(day) = self.token_warned_on {
            json["tokenWarnedOn"] = Value::String(day.to_string());
        }
        json
    }

    pub fn from_json(json: &Value) -> Option<Connection> {
        let string = |key: &str| json[key].as_str().map(str::to_owned);
        Some(Connection {
            target: Target {
                repository: string("repository")?,
                file: string("file").unwrap_or_else(|| "library.json".into()),
                branch: string("branch").unwrap_or_else(|| "main".into()),
            },
            local: json["local"].as_bool().unwrap_or(false),
            token_expires: json["tokenExpires"].as_str().and_then(Timestamp::parse),
            token_warned_on: json["tokenWarnedOn"].as_str().and_then(Date::parse),
        })
    }
}

/// `connection.json` on this computer.
#[derive(Debug, Clone, Default)]
pub struct ConnectionFile {
    /// `None` keeps nothing (the tests).
    path: Option<PathBuf>,
}

impl ConnectionFile {
    pub fn at(path: PathBuf) -> ConnectionFile {
        ConnectionFile { path: Some(path) }
    }

    /// The stored connection, or `None` when there is none or it cannot
    /// be read (then Libri starts over with the setup).
    pub fn load(&self) -> Option<Connection> {
        let path = self.path.as_ref()?;
        let text = match storage::read_if_exists(path) {
            Ok(text) => text?,
            Err(error) => {
                eprintln!("libri: could not read {}: {error}", path.display());
                return None;
            }
        };
        let json: Value = match serde_json::from_str(&text) {
            Ok(json) => json,
            Err(error) => {
                eprintln!("libri: {} does not parse: {error}", path.display());
                return None;
            }
        };
        Connection::from_json(&json)
    }

    pub fn save(&self, connection: &Connection) {
        let Some(path) = &self.path else {
            return;
        };
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&connection.to_json()).expect("json serializes")
        );
        if let Err(error) = storage::write_atomically(path, &text) {
            let message = format!("could not write {}: {error}", path.display());
            eprintln!("libri: {message}");
            crate::trace::line(message);
        }
    }

    pub fn clear(&self) {
        if let Some(path) = &self.path
            && let Err(error) = storage::remove_if_exists(path)
        {
            eprintln!("libri: could not remove {}: {error}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TempDir;

    #[test]
    fn the_connection_round_trips_with_only_the_set_fields_written() {
        let dir = TempDir::new("connection");
        let file = ConnectionFile::at(dir.path("connection.json"));
        assert_eq!(file.load(), None);
        let mut connection = Connection::new(Target {
            repository: "ekmanch/library".into(),
            file: "library.json".into(),
            branch: "main".into(),
        });
        file.save(&connection);
        let text = std::fs::read_to_string(dir.path("connection.json")).unwrap();
        assert!(!text.contains("local") && !text.contains("token"));
        assert_eq!(file.load(), Some(connection.clone()));

        connection.local = true;
        connection.token_expires = Some(Timestamp(1_700_000_000));
        connection.token_warned_on = Date::parse("2026-10-29");
        file.save(&connection);
        let text = std::fs::read_to_string(dir.path("connection.json")).unwrap();
        assert!(text.contains("\"local\": true"));
        assert!(text.contains("\"tokenWarnedOn\": \"2026-10-29\""));
        assert_eq!(file.load(), Some(connection.clone()));
        assert_eq!(connection.target.key(), "ekmanch/library:library.json@main");
        file.clear();
        assert_eq!(file.load(), None);
    }

    #[test]
    fn the_phones_defaults_fill_a_short_file() {
        let json = json!({ "repository": "a/b" });
        let connection = Connection::from_json(&json).unwrap();
        assert_eq!(connection.target.file, "library.json");
        assert_eq!(connection.target.branch, "main");
        assert!(Connection::from_json(&json!({ "file": "x" })).is_none());
    }
}
