//! The library in a file on this computer: `library.json` under
//! `storage::data_dir()`, written in the canonical form after every
//! change that alters it, so changes survive a restart (phase 2).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::model::{Library, codec};
use crate::storage;

use super::{LibraryRepository, Operation, RepoError};

#[derive(Debug)]
pub struct LocalLibraryRepository {
    path: PathBuf,
    library: Library,
    /// The text last read or written, to skip a write that changes
    /// nothing. `None` while the file does not hold the library in memory
    /// (a write failed), so the next change writes it whole.
    written: Option<String>,
}

/// Why the library at a path could not be opened.
#[derive(Debug)]
pub enum OpenError {
    /// The file or its folder could not be read or written.
    Io(io::Error),
    /// The seed for a new library does not decode.
    Seed(serde_json::Error),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Io(error) => write!(f, "{error}"),
            OpenError::Seed(error) => write!(f, "the seed library does not decode: {error}"),
        }
    }
}

impl From<io::Error> for OpenError {
    fn from(error: io::Error) -> Self {
        OpenError::Io(error)
    }
}

impl LocalLibraryRepository {
    /// Opens the library at `path`. With no file yet it starts from `seed`
    /// (the bundled fixture until phase 3 fetches GitHub's copy) and
    /// writes it. A file that cannot be read is never overwritten: it is
    /// renamed aside with an `.unreadable` suffix and the library starts
    /// from `seed` again.
    pub fn open(path: PathBuf, seed: &str) -> Result<Self, OpenError> {
        if let Some(text) = storage::read_if_exists(&path)? {
            match codec::decode(&text) {
                Ok(library) => {
                    return Ok(LocalLibraryRepository {
                        path,
                        library,
                        written: Some(text),
                    });
                }
                Err(error) => {
                    let aside = aside_path(&path);
                    fs::rename(&path, &aside)?;
                    let message = format!(
                        "{} could not be read ({error}); moved to {}",
                        path.display(),
                        aside.display()
                    );
                    eprintln!("libri: {message}");
                    crate::trace::line(message);
                }
            }
        }
        let library = codec::decode(seed).map_err(OpenError::Seed)?;
        let mut repo = LocalLibraryRepository {
            path,
            library,
            written: None,
        };
        repo.write()?;
        Ok(repo)
    }

    /// Where the library is kept.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the library in memory, unless the file already holds it.
    fn write(&mut self) -> io::Result<()> {
        let text = codec::encode(&self.library);
        if self.written.as_deref() == Some(text.as_str()) {
            return Ok(());
        }
        storage::write_atomically(&self.path, &text)?;
        self.written = Some(text);
        Ok(())
    }
}

/// `library.json.20261027-213005.unreadable` next to `library.json`.
fn aside_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(format!(".{}.unreadable", storage::file_stamp()));
    path.with_file_name(name)
}

impl LibraryRepository for LocalLibraryRepository {
    fn library(&self) -> &Library {
        &self.library
    }

    /// Applies the change and writes the file. The change stays in memory
    /// when the write fails; the failure is logged, and the next change
    /// writes the file whole.
    fn apply(&mut self, operation: Operation) -> Result<&Library, RepoError> {
        let mut next = self.library.clone();
        operation.apply_to(&mut next)?;
        self.library = next;
        if let Err(error) = self.write() {
            let message = format!("could not write {}: {error}", self.path.display());
            eprintln!("libri: {message}");
            crate::trace::line(message);
            self.written = None;
        }
        Ok(&self.library)
    }

    fn delete(&mut self) -> io::Result<()> {
        self.library = Library::default();
        self.written = None;
        storage::remove_if_exists(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::EntryStatus;
    use crate::storage::TempDir;

    const MUMMY: &str = "movies/the-mummy-1999";

    fn open(dir: &TempDir) -> LocalLibraryRepository {
        LocalLibraryRepository::open(dir.path("library.json"), fixtures::LIBRARY_JSON).unwrap()
    }

    #[test]
    fn a_new_library_starts_from_the_seed_and_is_written_as_the_seed_is() {
        let dir = TempDir::new("local");
        let repo = open(&dir);
        assert_eq!(
            repo.library(),
            &codec::decode(fixtures::LIBRARY_JSON).unwrap()
        );
        assert_eq!(
            fs::read_to_string(dir.path("library.json")).unwrap(),
            fixtures::LIBRARY_JSON,
            "the fixture is canonical, so the file is the fixture's bytes"
        );
    }

    #[test]
    fn a_change_is_written_and_read_back_on_the_next_open() {
        let dir = TempDir::new("local");
        let mut repo = open(&dir);
        repo.apply(Operation::SetStatus {
            id: MUMMY.into(),
            status: EntryStatus::Want,
        })
        .unwrap();
        repo.apply(Operation::SetRating {
            id: MUMMY.into(),
            rating: Some(7),
        })
        .unwrap();
        let text = fs::read_to_string(dir.path("library.json")).unwrap();
        assert_eq!(text, codec::encode(repo.library()), "the canonical form");
        let again = open(&dir);
        let mummy = again.library().entry(MUMMY).unwrap();
        assert_eq!((mummy.status, mummy.rating), (EntryStatus::Want, Some(7)));
        assert!(!dir.path("library.json.tmp").exists());
    }

    #[test]
    fn a_change_that_cannot_apply_writes_nothing() {
        let dir = TempDir::new("local");
        let mut repo = open(&dir);
        let before = fs::metadata(dir.path("library.json"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            repo.apply(Operation::SetRating {
                id: MUMMY.into(),
                rating: Some(11),
            }),
            Err(RepoError::InvalidRating(11))
        );
        assert_eq!(
            repo.apply(Operation::SetStatus {
                id: "movies/nope".into(),
                status: EntryStatus::Want,
            }),
            Err(RepoError::MissingEntry("movies/nope".into()))
        );
        let after = fs::metadata(dir.path("library.json"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn an_unreadable_file_is_moved_aside_and_the_seed_takes_its_place() {
        let dir = TempDir::new("local");
        fs::write(dir.path("library.json"), "{ not json").unwrap();
        let repo = open(&dir);
        assert_eq!(repo.library().entries.len(), {
            codec::decode(fixtures::LIBRARY_JSON).unwrap().entries.len()
        });
        let names: Vec<String> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"library.json".to_owned()));
        let aside = names
            .iter()
            .find(|n| n.starts_with("library.json.") && n.ends_with(".unreadable"))
            .expect("the unreadable file is kept aside");
        assert_eq!(fs::read_to_string(dir.path(aside)).unwrap(), "{ not json");
    }

    #[test]
    fn delete_removes_the_file_and_empties_the_library() {
        let dir = TempDir::new("local");
        let mut repo = open(&dir);
        repo.delete().unwrap();
        assert!(!dir.path("library.json").exists());
        assert_eq!(repo.library(), &Library::default());
        repo.delete().unwrap();
        // A change after the delete starts the file again, from the
        // empty library.
        let plan = crate::model::Plan {
            id: "reading-2027".into(),
            name: "Reading".into(),
            year: Some(2027),
            ..codec::decode(fixtures::LIBRARY_JSON).unwrap().plans[0].clone()
        };
        repo.apply(Operation::AddPlan(plan)).unwrap();
        let again = open(&dir);
        assert!(again.library().entries.is_empty());
        assert_eq!(again.library().plans.len(), 1);
    }
}
