//! One commit that carries the library file and the cover files that
//! changed with it, through GitHub's Git Data API: the blobs of the
//! uploads, a tree made from the branch's own with the edits on top, a
//! commit on the branch's head, and the branch moved forward to it. The
//! Contents API cannot do this; it commits one file at a time.
//!
//! The commit is planned from the tree as it is now, so it holds only
//! what still differs: an upload that is there already, a delete of a
//! file that is gone and a move that has happened are left out, and a
//! commit with nothing left is not made. That makes sending the same
//! changes twice harmless, which a lost answer needs. The library file
//! must still be the blob the sync last saw, or the commit is refused as
//! a conflict, like a save with a stale sha; so is a branch that moved
//! while the commit was made, since the branch is never forced.

use std::collections::HashMap;
use std::path::Path;

use crate::connection::Target;
use crate::covers::{self, Change};
use crate::github::{Client, Failure, FailureKind, PutResult, Reply, TreeEdit, TreeFile, blob_sha};

/// What the commit does to one cover file, by its path in the repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The file from this computer's `covers/`, at this path under it.
    Upload(String),
    /// A blob the repo already has.
    Blob(String),
    Delete,
}

/// The edits `changes` come to against `tree`, the blobs of the repo by
/// path: the paths in the repo (under `root`, the `covers` folder) with
/// what happens to each, in path order. Deletes of files that are not
/// there and blobs already in place are left out.
pub fn plan(tree: &[TreeFile], root: &str, changes: &[Change]) -> Vec<(String, Edit)> {
    let there: HashMap<&str, &str> = tree
        .iter()
        .map(|file| (file.path.as_str(), file.sha.as_str()))
        .collect();
    let full = |path: &str| format!("{root}/{path}");
    // What each touched path ends as: none is gone.
    let mut ends: HashMap<String, Option<Edit>> = HashMap::new();
    for change in changes {
        match change {
            Change::Upload { path } => {
                ends.insert(full(path), Some(Edit::Upload(path.clone())));
            }
            Change::Delete { path } => {
                ends.insert(full(path), None);
            }
            Change::Move { from, to } => {
                let (old, new) = (full(from), full(to));
                let moved = match ends.get(&old) {
                    // An upload not yet pushed: this computer's copy has
                    // moved with it.
                    Some(Some(Edit::Upload(_))) => Some(Edit::Upload(to.clone())),
                    Some(Some(edit)) => Some(edit.clone()),
                    Some(None) => None,
                    None => there
                        .get(old.as_str())
                        .map(|sha| Edit::Blob((*sha).to_owned())),
                };
                // Nothing at the old path: the move has happened, or the
                // file is gone.
                if let Some(edit) = moved {
                    ends.insert(new, Some(edit));
                    ends.insert(old, None);
                }
            }
        }
    }
    let mut edits: Vec<(String, Edit)> = ends
        .into_iter()
        .filter_map(|(path, end)| match end {
            None => there
                .contains_key(path.as_str())
                .then_some((path, Edit::Delete)),
            Some(Edit::Blob(sha)) if there.get(path.as_str()) == Some(&sha.as_str()) => None,
            Some(edit) => Some((path, edit)),
        })
        .collect();
    edits.sort_by(|a, b| a.0.cmp(&b.0));
    edits
}

/// Makes the commit on `target`'s branch: `library` as the library file
/// when it changed, and `changes` to the cover files, whose uploads are
/// read from `covers_dir`. `expected` is the blob the library file must
/// still be. Gives the library file's blob and the commit; with nothing
/// to change no commit is made and the head is given.
pub fn run(
    client: &Client,
    target: &Target,
    covers_dir: Option<&Path>,
    library: Option<&[u8]>,
    message: &str,
    expected: Option<&str>,
    changes: &[Change],
) -> Reply<PutResult> {
    let head = client.head(&target.branch)?;
    let tree = client.tree(&head.tree)?;
    let current = tree
        .iter()
        .find(|file| file.path == target.file)
        .map(|file| file.sha.clone())
        .ok_or_else(|| Failure {
            kind: FailureKind::NotFound,
            status: None,
            message: "the library file is not on the branch".into(),
        })?;
    if expected.is_some_and(|expected| expected != current) {
        return Err(Failure::new(
            FailureKind::Conflict,
            "the library file changed on GitHub",
        ));
    }
    let root = covers::root(&target.file);
    let mut edits: Vec<(String, TreeEdit)> = Vec::new();
    for (path, edit) in plan(&tree, &root, changes) {
        match edit {
            Edit::Delete => edits.push((path, TreeEdit::Delete)),
            Edit::Blob(sha) => edits.push((path, TreeEdit::Blob(sha))),
            Edit::Upload(local) => {
                let Some(bytes) = covers_dir.and_then(|dir| std::fs::read(dir.join(&local)).ok())
                else {
                    // The copy is gone from this computer: there is
                    // nothing to send, and the rest still goes.
                    eprintln!("libri: sync: the cover {local} is not on this computer; skipped");
                    continue;
                };
                let sha = blob_sha(&bytes);
                let listed = tree.iter().find(|file| file.path == path);
                if listed.is_some_and(|file| file.sha == sha) {
                    continue;
                }
                let sha = client.create_blob(&bytes)?;
                edits.push((path, TreeEdit::Blob(sha)));
            }
        }
    }
    let mut blob = current;
    if let Some(library) = library {
        let sha = blob_sha(library);
        if sha != blob {
            let text = std::str::from_utf8(library)
                .map_err(|e| Failure::new(FailureKind::Other, e.to_string()))?;
            edits.push((target.file.clone(), TreeEdit::Text(text.to_owned())));
            blob = sha;
        }
    }
    if edits.is_empty() {
        return Ok(PutResult {
            blob_sha: blob,
            commit_sha: head.commit,
        });
    }
    let tree = client.create_tree(&head.tree, &edits)?;
    let commit = client.create_commit(message, &tree, &head.commit)?;
    client.move_branch(&target.branch, &commit)?;
    Ok(PutResult {
        blob_sha: blob,
        commit_sha: commit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> Vec<TreeFile> {
        files
            .iter()
            .map(|(path, sha)| TreeFile {
                path: (*path).into(),
                size: 10,
                sha: (*sha).into(),
            })
            .collect()
    }

    fn upload(path: &str) -> Change {
        Change::Upload { path: path.into() }
    }

    fn delete(path: &str) -> Change {
        Change::Delete { path: path.into() }
    }

    fn moved(from: &str, to: &str) -> Change {
        Change::Move {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn uploads_deletes_and_moves_become_edits_under_the_covers_folder() {
        let tree = tree(&[
            ("library.json", "lib"),
            ("covers/movies/old.png", "old"),
            ("covers/movies/a.jpg", "aaa"),
        ]);
        let edits = plan(
            &tree,
            "covers",
            &[
                upload("movies/new.1234567.jpg"),
                delete("movies/old.png"),
                moved("movies/a.jpg", "shows/a.jpg"),
            ],
        );
        assert_eq!(
            edits,
            [
                ("covers/movies/a.jpg".to_owned(), Edit::Delete),
                (
                    "covers/movies/new.1234567.jpg".to_owned(),
                    Edit::Upload("movies/new.1234567.jpg".into())
                ),
                ("covers/movies/old.png".to_owned(), Edit::Delete),
                ("covers/shows/a.jpg".to_owned(), Edit::Blob("aaa".into())),
            ]
        );
        // Beside a library file in a folder.
        let edits = plan(&[], "data/covers", &[upload("books/b.jpg")]);
        assert_eq!(edits[0].0, "data/covers/books/b.jpg");
    }

    #[test]
    fn what_is_done_already_is_left_out() {
        // The same changes sent again after they landed: nothing left
        // but the upload, which the bytes then tell is there.
        let tree = tree(&[("covers/shows/a.jpg", "aaa")]);
        let edits = plan(
            &tree,
            "covers",
            &[
                delete("movies/old.png"),
                moved("movies/a.jpg", "shows/a.jpg"),
            ],
        );
        assert!(edits.is_empty(), "{edits:?}");
    }

    #[test]
    fn later_changes_win_over_earlier_ones_on_a_path() {
        // Uploaded, then replaced by a fitted copy before the push.
        let edits = plan(
            &[],
            "covers",
            &[
                upload("movies/a.1111111.jpg"),
                delete("movies/a.1111111.jpg"),
                upload("movies/a.2222222.jpg"),
            ],
        );
        assert_eq!(
            edits,
            [(
                "covers/movies/a.2222222.jpg".to_owned(),
                Edit::Upload("movies/a.2222222.jpg".into())
            )]
        );
        // An upload moved before it was pushed goes up at its new path.
        let edits = plan(
            &[],
            "covers",
            &[upload("movies/a.jpg"), moved("movies/a.jpg", "shows/a.jpg")],
        );
        assert_eq!(
            edits,
            [(
                "covers/shows/a.jpg".to_owned(),
                Edit::Upload("shows/a.jpg".into())
            )]
        );
    }
}
