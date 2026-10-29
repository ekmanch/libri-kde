//! What Libri remembers about the copy of the library file on GitHub,
//! kept in `sync.json` next to the library, as on the phone
//! (`sync_meta.dart`): the base of every three-way comparison.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::clock::Timestamp;
use crate::storage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    /// Which file this is the base of: "owner/repo:path@branch". A base
    /// only applies to the same file; one written for another file (a
    /// library kept on this computer, then connected elsewhere) is
    /// ignored.
    pub target: String,
    /// The ETag of the last download, for the next conditional request.
    /// For the raw format it is the quoted blob sha, so a save sets it
    /// too.
    pub etag: Option<String>,
    /// The blob sha of the file on GitHub, as last seen or written. The
    /// Contents API wants it as the `sha` of the file being replaced.
    pub remote_sha: String,
    /// The blob sha of the remote file re-encoded in the canonical form.
    /// Equal to `remote_sha` once Libri has saved; different for a file
    /// formatted by hand. The local copy with no pending operations
    /// encodes to this sha.
    pub canonical_sha: String,
    /// The commit of the last save, when known.
    pub commit_sha: Option<String>,
    /// When the local copy last matched GitHub (a download or a
    /// successful save).
    pub last_synced: Option<Timestamp>,
    /// When GitHub was last asked whether the file changed.
    pub last_checked: Option<Timestamp>,
}

impl Base {
    pub fn to_json(&self) -> Value {
        json!({
            "target": self.target,
            "etag": self.etag,
            "remoteSha": self.remote_sha,
            "canonicalSha": self.canonical_sha,
            "commitSha": self.commit_sha,
            "lastSynced": self.last_synced.map(Timestamp::to_rfc3339),
            "lastChecked": self.last_checked.map(Timestamp::to_rfc3339),
        })
    }

    /// `None` when the value is not a base.
    pub fn from_json(json: &Value) -> Option<Base> {
        let string = |key: &str| json[key].as_str().map(str::to_owned);
        let when = |key: &str| json[key].as_str().and_then(Timestamp::parse);
        Some(Base {
            target: string("target").unwrap_or_default(),
            etag: string("etag"),
            remote_sha: string("remoteSha")?,
            canonical_sha: string("canonicalSha")?,
            commit_sha: string("commitSha"),
            last_synced: when("lastSynced"),
            last_checked: when("lastChecked"),
        })
    }
}

/// `sync.json` on this computer.
#[derive(Debug, Clone, Default)]
pub struct BaseFile {
    /// `None` keeps nothing (the tests).
    path: Option<PathBuf>,
}

impl BaseFile {
    pub fn at(path: PathBuf) -> BaseFile {
        BaseFile { path: Some(path) }
    }

    /// The stored base for `target`, or `None` when there is none, it is
    /// unreadable (which costs one full download) or it is about another
    /// file. A base from before the target field is taken as this one's.
    pub fn load(&self, target: &str) -> Option<Base> {
        let path = self.path.as_ref()?;
        let text = storage::read_if_exists(path).ok()??;
        let json: Value = serde_json::from_str(&text).ok()?;
        let base = Base::from_json(&json)?;
        if !base.target.is_empty() && base.target != target {
            crate::trace::line(format_args!(
                "the sync base is for {}, not {target}: no base",
                base.target
            ));
            return None;
        }
        Some(base)
    }

    pub fn save(&self, base: &Base) {
        let Some(path) = &self.path else {
            return;
        };
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&base.to_json()).expect("json serializes")
        );
        if let Err(error) = storage::write_atomically(path, &text) {
            let message = format!("could not write {}: {error}", path.display());
            eprintln!("libri: {message}");
            crate::trace::line(message);
        }
    }

    pub fn delete(&self) {
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
    fn the_base_round_trips_and_only_applies_to_its_target() {
        let dir = TempDir::new("base");
        let file = BaseFile::at(dir.path("sync.json"));
        assert_eq!(file.load("a/b:library.json@main"), None);
        let base = Base {
            target: "a/b:library.json@main".into(),
            etag: Some("\"abc\"".into()),
            remote_sha: "abc".into(),
            canonical_sha: "def".into(),
            commit_sha: None,
            last_synced: Some(Timestamp(1_700_000_000)),
            last_checked: None,
        };
        file.save(&base);
        assert_eq!(file.load("a/b:library.json@main"), Some(base));
        assert_eq!(file.load("a/b:other.json@main"), None);
        let text = std::fs::read_to_string(dir.path("sync.json")).unwrap();
        assert!(text.contains("\"remoteSha\": \"abc\""));
        assert!(text.contains("\"lastSynced\": \"2023-11-14T22:13:20Z\""));
        file.delete();
        assert_eq!(file.load("a/b:library.json@main"), None);
    }

    #[test]
    fn a_base_without_a_target_is_taken_as_this_ones() {
        let json = json!({ "remoteSha": "a", "canonicalSha": "a" });
        let base = Base::from_json(&json).unwrap();
        assert_eq!(base.target, "");
        assert!(Base::from_json(&json!({ "etag": "x" })).is_none());
    }
}
