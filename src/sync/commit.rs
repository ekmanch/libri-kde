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
//!
//! An artist's photo has a fixed name, so two devices can write the same
//! path. Such a file is written or deleted only over a blob this
//! computer saw there (`Change::Put`, `Change::Remove`); over anything
//! else the change is dropped and reported, and GitHub's file stands.
//! `artists/_missing.json` is never sent whole: the queued edits are
//! merged per artist into the repo's copy as it is when the commit is
//! made (`covers::artists::merged_missing`), and an artist who got a
//! file or left the library leaves the record there. Its `picked` list
//! goes the same way, per slug; a pick that is dropped takes its edit to
//! the list with it, and a photo fetched again at a sharper size
//! (`Change::Sharpen`) is dropped when the list, as the commit finds it,
//! has the artist. `settings.json`
//! beside the library goes the same way: its queued edits are made on
//! the repo's copy as it is then (`shared::merged`), so a change made on
//! the phone meanwhile is kept.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::connection::Target;
use crate::covers::artists::{self, MissingEdit};
use crate::covers::{self, Change, backfill};
use crate::github::{Client, Failure, FailureKind, PutResult, Reply, TreeEdit, TreeFile, blob_sha};
use crate::shared;

/// What the commit does to one cover file, by its path in the repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The file from this computer's `covers/`, at this path under it.
    Upload(String),
    /// A blob the repo already has.
    Blob(String),
    Delete,
}

/// A change to a fixed-name file that was not made, since GitHub holds
/// a file this computer never saw there: the path under `covers/`, and
/// whether the owner is told (a picture they picked, or Use automatic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub path: String,
    pub picked: bool,
}

/// What a commit comes to: the edits by path in the repo, the changes
/// dropped, and the edits to `_missing.json` still to merge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub edits: Vec<(String, Edit)>,
    pub dropped: Vec<Dropped>,
    pub missing: Vec<MissingEdit>,
    /// The edits to `settings.json` beside the library, merged like the
    /// missing record's.
    pub settings: Vec<shared::Edit>,
    /// The paths in the repo, among `edits`, of the photos fetched again
    /// at a sharper size: each still has to pass `keep_picked`.
    pub sharper: Vec<String>,
}

/// A commit made, or found to be made already, and what it dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub result: PutResult,
    pub dropped: Vec<Dropped>,
}

/// The edits `changes` come to against `tree`, the blobs of the repo by
/// path: the paths in the repo (under `root`, the `covers` folder) with
/// what happens to each, in path order. Deletes of files that are not
/// there and blobs already in place are left out.
pub fn plan(tree: &[TreeFile], root: &str, changes: &[Change]) -> Vec<(String, Edit)> {
    plan_guarded(tree, root, changes, &|_| None).edits
}

/// `plan` with the fixed-name files: the last `Put` or `Remove` of a
/// path is made when the path holds one of the blobs it has `seen`
/// (`None` being no file), or holds this computer's file already
/// (`local_sha`, the blob of the copy here: the commit landed and its
/// answer was lost); else it is dropped.
pub fn plan_guarded(
    tree: &[TreeFile],
    root: &str,
    changes: &[Change],
    local_sha: &dyn Fn(&str) -> Option<String>,
) -> Plan {
    let there: HashMap<&str, &str> = tree
        .iter()
        .map(|file| (file.path.as_str(), file.sha.as_str()))
        .collect();
    let full = |path: &str| format!("{root}/{path}");
    // What each touched path ends as: none is gone.
    let mut ends: HashMap<String, Option<Edit>> = HashMap::new();
    // The last guarded change of each path.
    let mut guards: HashMap<String, &Change> = HashMap::new();
    let mut missing = Vec::new();
    let mut settings = Vec::new();
    for change in changes {
        match change {
            Change::Put { path, .. } => {
                ends.insert(full(path), Some(Edit::Upload(path.clone())));
                guards.insert(full(path), change);
            }
            Change::Remove { path, .. } => {
                ends.insert(full(path), None);
                guards.insert(full(path), change);
            }
            Change::Sharpen { path, .. } => {
                ends.insert(full(path), Some(Edit::Upload(path.clone())));
                guards.insert(full(path), change);
            }
            Change::Missing(edit) => missing.push(edit.clone()),
            Change::Settings(edit) => settings.push(edit.clone()),
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
    let mut dropped = Vec::new();
    let mut sharper = Vec::new();
    for (at, change) in guards {
        let current = there.get(at.as_str()).map(|sha| (*sha).to_owned());
        let (path, made, picked) = match change {
            Change::Put { path, seen, picked } => (
                path,
                seen.contains(&current) || (current.is_some() && local_sha(path) == current),
                *picked,
            ),
            Change::Remove { path, seen } => {
                (path, current.is_none() || seen.contains(&current), true)
            }
            // Only over a photo that is there: a sharper copy never
            // brings back a file that was removed.
            Change::Sharpen { path, seen } => (
                path,
                current.is_some() && (seen.contains(&current) || local_sha(path) == current),
                false,
            ),
            _ => continue,
        };
        if made {
            if matches!(change, Change::Sharpen { .. }) {
                sharper.push(at);
            }
        } else {
            ends.remove(&at);
            dropped.push(Dropped {
                path: path.clone(),
                picked,
            });
        }
    }
    dropped.sort_by(|a, b| a.path.cmp(&b.path));
    sharper.sort();
    // A pick or a Use automatic that was not made says nothing about
    // GitHub's file: its edit to the `picked` list goes with it.
    let dropped_slugs: Vec<&str> = dropped
        .iter()
        .filter_map(|file| artists::slug_of_path(&file.path))
        .collect();
    missing.retain(|edit| match edit {
        MissingEdit::Pick { slug } | MissingEdit::Unpick { slug } => {
            !dropped_slugs.contains(&slug.as_str())
        }
        _ => true,
    });
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
    Plan {
        edits,
        dropped,
        missing,
        settings,
        sharper,
    }
}

/// Takes the sharper photos of the artists on `picked` out of the plan:
/// `picked` is the record's list as this commit leaves it, or `None` when
/// the record could not be read, which replaces nobody's photo. Their
/// uploads go to `dropped`, so GitHub's file stands and is fetched again.
pub fn keep_picked(plan: &mut Plan, root: &str, picked: Option<&BTreeSet<String>>) {
    let kept: Vec<String> = std::mem::take(&mut plan.sharper)
        .into_iter()
        .filter(|at| {
            let path = at
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or(at);
            let on_list = match (picked, artists::slug_of_path(path)) {
                (Some(picked), Some(slug)) => picked.contains(slug),
                _ => true,
            };
            if on_list {
                plan.edits.retain(|(edit_at, _)| edit_at != at);
                plan.dropped.push(Dropped {
                    path: path.to_owned(),
                    picked: false,
                });
            }
            !on_list
        })
        .collect();
    plan.sharper = kept;
    plan.dropped.sort_by(|a, b| a.path.cmp(&b.path));
}

/// `_missing.json` as the commit writes it, or `None` to leave it:
/// `theirs` (the repo's copy) with `missing` merged in, without the
/// artists that have a file once `edits` are made and without those the
/// library no longer has (`in_library`, by slug).
pub fn missing_text(
    tree: &[TreeFile],
    root: &str,
    edits: &[(String, Edit)],
    missing: &[MissingEdit],
    theirs: Option<&str>,
    in_library: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let has_file = |slug: &str| {
        let path = format!("{root}/{}/{slug}.jpg", artists::FOLDER);
        match edits.iter().find(|(at, _)| *at == path) {
            Some((_, edit)) => *edit != Edit::Delete,
            None => tree.iter().any(|file| file.path == path),
        }
    };
    match artists::merged_missing(theirs, missing, has_file, in_library) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("libri: sync: _missing.json on GitHub is left as it is: {error}");
            None
        }
    }
}

/// Whether `edits` put an artist's photo in the repo. The missing record
/// is then read again, with or without edits of its own: a photo the
/// owner picked for an artist Apple had none for takes the artist off it.
pub fn adds_photo(root: &str, edits: &[(String, Edit)]) -> bool {
    edits.iter().any(|(path, edit)| {
        *edit != Edit::Delete
            && path
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
                .is_some_and(|rest| artists::slug_of_path(rest).is_some())
    })
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
) -> Reply<Committed> {
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
    let local_sha = |path: &str| {
        let bytes = std::fs::read(covers_dir?.join(path)).ok()?;
        Some(blob_sha(&bytes))
    };
    let mut plan = plan_guarded(&tree, &root, changes, &local_sha);
    let at = format!("{root}/{}", artists::missing_path());
    let listed = tree.iter().any(|file| file.path == at);
    if !plan.missing.is_empty() || (listed && adds_photo(&root, &plan.edits)) {
        let theirs = match listed {
            true => client.file_raw(&at, &target.branch, None)?,
            false => None,
        };
        let theirs = theirs.as_ref().map(|raw| raw.text().unwrap_or("not UTF-8"));
        // The photos the owner chose, on any device, are never replaced
        // by a sharper copy.
        if !plan.sharper.is_empty() {
            let picked = artists::picked_after(theirs, &plan.missing);
            keep_picked(&mut plan, &root, picked.as_ref().ok());
        }
        // The library as this commit leaves it: the one it carries, else
        // the branch's. One that cannot be read drops nobody.
        let slugs = match library {
            Some(bytes) => std::str::from_utf8(bytes).ok().and_then(backfill::slugs_in),
            None => client
                .file_raw(&target.file, &target.branch, None)?
                .and_then(|raw| raw.text().ok().and_then(backfill::slugs_in)),
        };
        let in_library = |slug: &str| slugs.as_ref().is_none_or(|slugs| slugs.contains(slug));
        if let Some(text) = missing_text(
            &tree,
            &root,
            &plan.edits,
            &plan.missing,
            theirs,
            &in_library,
        ) {
            edits.push((at, TreeEdit::Text(text)));
        }
    }
    if !plan.settings.is_empty() {
        let at = shared::path_beside(&target.file);
        let theirs = match tree.iter().any(|file| file.path == at) {
            true => client.file_raw(&at, &target.branch, None)?,
            false => None,
        };
        let theirs = theirs.as_ref().map(|raw| raw.text().unwrap_or("not UTF-8"));
        match shared::merged(theirs, &plan.settings) {
            Ok(Some(text)) => edits.push((at, TreeEdit::Text(text))),
            Ok(None) => {}
            // Another version's file, or one edited by hand into
            // something else: it stands, and the changes stay on this
            // computer.
            Err(error) => {
                eprintln!("libri: sync: settings.json on GitHub is left as it is: {error}")
            }
        }
    }
    let dropped = plan.dropped;
    for (path, edit) in plan.edits {
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
        return Ok(Committed {
            result: PutResult {
                blob_sha: blob,
                commit_sha: head.commit,
            },
            dropped,
        });
    }
    let tree = client.create_tree(&head.tree, &edits)?;
    let commit = client.create_commit(message, &tree, &head.commit)?;
    client.move_branch(&target.branch, &commit)?;
    Ok(Committed {
        result: PutResult {
            blob_sha: blob,
            commit_sha: commit,
        },
        dropped,
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

    fn put(path: &str, seen: &[Option<&str>], picked: bool) -> Change {
        Change::Put {
            path: path.into(),
            seen: seen.iter().map(|sha| sha.map(str::to_owned)).collect(),
            picked,
        }
    }

    fn remove(path: &str, seen: &[Option<&str>]) -> Change {
        Change::Remove {
            path: path.into(),
            seen: seen.iter().map(|sha| sha.map(str::to_owned)).collect(),
        }
    }

    const ADELE: &str = "covers/artists/adele.jpg";

    #[test]
    fn a_photo_is_created_only_where_there_is_none() {
        let fetched = [put("artists/adele.jpg", &[None], false)];
        let made = plan_guarded(&[], "covers", &fetched, &|_| None);
        assert_eq!(
            made.edits,
            [(ADELE.to_owned(), Edit::Upload("artists/adele.jpg".into()))]
        );
        assert!(made.dropped.is_empty());
        // Another device put one there first: GitHub's stands.
        let theirs = tree(&[(ADELE, "theirs")]);
        let refused = plan_guarded(&theirs, "covers", &fetched, &|_| Some("mine".into()));
        assert!(refused.edits.is_empty(), "{refused:?}");
        assert_eq!(
            refused.dropped,
            [Dropped {
                path: "artists/adele.jpg".into(),
                picked: false
            }]
        );
        // The commit landed and its answer was lost: the file there is
        // this computer's, so nothing is dropped.
        let landed = plan_guarded(&theirs, "covers", &fetched, &|_| Some("theirs".into()));
        assert!(landed.dropped.is_empty(), "{landed:?}");
    }

    #[test]
    fn a_photo_is_replaced_only_over_the_one_seen() {
        let there = tree(&[(ADELE, "old")]);
        let pick = [put("artists/adele.jpg", &[Some("old")], true)];
        let made = plan_guarded(&there, "covers", &pick, &|_| Some("new".into()));
        assert_eq!(made.edits.len(), 1);
        assert!(made.dropped.is_empty());
        // Replaced on another device before this one synced.
        let moved_on = tree(&[(ADELE, "other")]);
        let refused = plan_guarded(&moved_on, "covers", &pick, &|_| Some("new".into()));
        assert!(refused.edits.is_empty());
        assert_eq!(
            refused.dropped,
            [Dropped {
                path: "artists/adele.jpg".into(),
                picked: true
            }]
        );
        // Two picks in a row: the last one counts, and it has seen the
        // first, which landed meanwhile.
        let first_landed = tree(&[(ADELE, "first")]);
        let picks = [
            put("artists/adele.jpg", &[None], true),
            put("artists/adele.jpg", &[None, Some("first")], true),
        ];
        let made = plan_guarded(&first_landed, "covers", &picks, &|_| Some("second".into()));
        assert_eq!(made.edits.len(), 1);
        assert!(made.dropped.is_empty());
    }

    #[test]
    fn use_automatic_deletes_only_the_photo_seen() {
        let there = tree(&[(ADELE, "old")]);
        let gone = [remove("artists/adele.jpg", &[Some("old")])];
        let made = plan_guarded(&there, "covers", &gone, &|_| None);
        assert_eq!(made.edits, [(ADELE.to_owned(), Edit::Delete)]);
        // Gone already: nothing to do and nothing dropped.
        let made = plan_guarded(&[], "covers", &gone, &|_| None);
        assert_eq!(made, Plan::default());
        // Replaced elsewhere meanwhile: it stays.
        let made = plan_guarded(&tree(&[(ADELE, "other")]), "covers", &gone, &|_| None);
        assert!(made.edits.is_empty());
        assert_eq!(made.dropped.len(), 1);
        // A pick taken back before it was pushed.
        let undone = [
            put("artists/adele.jpg", &[None], true),
            remove("artists/adele.jpg", &[None]),
        ];
        assert_eq!(
            plan_guarded(&[], "covers", &undone, &|_| None),
            Plan::default()
        );
    }

    fn sharpen(path: &str, seen: &[Option<&str>]) -> Change {
        Change::Sharpen {
            path: path.into(),
            seen: seen.iter().map(|sha| sha.map(str::to_owned)).collect(),
        }
    }

    #[test]
    fn a_sharper_photo_never_replaces_a_picked_one() {
        let there = tree(&[
            ("library.json", "lib"),
            ("covers/artists/adele.jpg", "a1"),
            ("covers/artists/burial.jpg", "b1"),
            ("covers/artists/dido.jpg", "new"),
        ]);
        let changes = [
            sharpen("artists/adele.jpg", &[Some("a1")]),
            sharpen("artists/burial.jpg", &[Some("b1")]),
            // Removed meanwhile, and replaced meanwhile: neither is made.
            sharpen("artists/can.jpg", &[Some("c1")]),
            sharpen("artists/dido.jpg", &[Some("old")]),
        ];
        let plan = plan_guarded(&there, "covers", &changes, &|_| Some("mine".into()));
        assert_eq!(
            plan.sharper,
            ["covers/artists/adele.jpg", "covers/artists/burial.jpg"]
        );
        let paths =
            |plan: &Plan| -> Vec<String> { plan.dropped.iter().map(|d| d.path.clone()).collect() };
        assert_eq!(paths(&plan), ["artists/can.jpg", "artists/dido.jpg"]);
        assert!(plan.dropped.iter().all(|dropped| !dropped.picked));

        // The phone put Burial on the list after this computer looked.
        let theirs =
            "{\n  \"version\": 1,\n  \"picked\": [\n    \"burial\"\n  ],\n  \"artists\": {}\n}\n";
        let mut kept = plan.clone();
        let picked = artists::picked_after(Some(theirs), &kept.missing).unwrap();
        keep_picked(&mut kept, "covers", Some(&picked));
        assert_eq!(
            kept.edits,
            [(
                "covers/artists/adele.jpg".to_owned(),
                Edit::Upload("artists/adele.jpg".into())
            )]
        );
        assert_eq!(
            paths(&kept),
            ["artists/burial.jpg", "artists/can.jpg", "artists/dido.jpg"]
        );
        // A pick queued here counts before it is on GitHub.
        let queued = [MissingEdit::Pick {
            slug: "adele".into(),
        }];
        let picked = artists::picked_after(Some(theirs), &queued).unwrap();
        let mut none = plan.clone();
        keep_picked(&mut none, "covers", Some(&picked));
        assert!(none.edits.is_empty(), "{none:?}");
        // A record that cannot be read replaces nobody's photo.
        assert!(artists::picked_after(Some("not json"), &[]).is_err());
        let mut unread = plan;
        keep_picked(&mut unread, "covers", None);
        assert!(unread.edits.is_empty() && unread.sharper.is_empty());
    }

    #[test]
    fn a_dropped_pick_is_not_put_on_the_picked_list() {
        let pick = |slug: &str| {
            Change::Missing(MissingEdit::Pick {
                slug: slug.to_owned(),
            })
        };
        let changes = [
            put("artists/adele.jpg", &[None], true),
            pick("adele"),
            put("artists/burial.jpg", &[None], true),
            pick("burial"),
        ];
        // Another device put a photo of Adele there first.
        let there = tree(&[(ADELE, "theirs")]);
        let plan = plan_guarded(&there, "covers", &changes, &|_| Some("mine".into()));
        assert_eq!(plan.dropped.len(), 1);
        assert_eq!(
            plan.missing,
            [MissingEdit::Pick {
                slug: "burial".into()
            }]
        );
    }

    #[test]
    fn a_picked_photo_takes_the_artist_off_the_missing_record() {
        let theirs = "{\n  \"version\": 1,\n  \"artists\": {\n    \"can\": {\"name\": \"Can\", \"tried\": \"2026-11-14\", \"reason\": \"no-photo\"},\n    \"neu\": {\"name\": \"Neu!\", \"tried\": \"2026-11-14\", \"reason\": \"no-match\"}\n  }\n}\n";
        // A pick carries no edit of the record, only the file.
        let made = plan_guarded(
            &[],
            "covers",
            &[put("artists/can.jpg", &[None], true)],
            &|_| None,
        );
        assert!(made.missing.is_empty());
        assert!(adds_photo("covers", &made.edits));
        let all = |_: &str| true;
        let text = missing_text(
            &[],
            "covers",
            &made.edits,
            &made.missing,
            Some(theirs),
            &all,
        )
        .expect("the record changes");
        let merged = artists::Missing::decode(&text).unwrap();
        let slugs: Vec<&str> = merged.artists.keys().map(String::as_str).collect();
        assert_eq!(slugs, ["neu"]);
        // A title's cover, or a photo taken away, does not read the record.
        let cover = (
            "covers/music/low.jpg".to_owned(),
            Edit::Upload("music/low.jpg".into()),
        );
        let gone = ("covers/artists/can.jpg".to_owned(), Edit::Delete);
        assert!(!adds_photo("covers", &[cover, gone]));
    }

    #[test]
    fn the_missing_record_is_merged_with_the_repos_copy() {
        let tried = |slug: &str, name: &str| MissingEdit::Set {
            slug: slug.into(),
            tried: artists::Tried {
                name: name.into(),
                tried: crate::model::Date::parse("2026-11-15").unwrap(),
                reason: artists::Reason::NoMatch,
            },
        };
        let changes = [
            put("artists/adele.jpg", &[None], false),
            Change::Missing(tried("adele", "Adele")),
            Change::Missing(tried("burial", "Burial")),
        ];
        let theirs = "{\n  \"version\": 1,\n  \"artists\": {\n    \"can\": {\"name\": \"Can\", \"tried\": \"2026-11-14\", \"reason\": \"no-photo\"}\n  }\n}\n";
        let made = plan_guarded(&[], "covers", &changes, &|_| None);
        let all = |_: &str| true;
        let text = missing_text(
            &[],
            "covers",
            &made.edits,
            &made.missing,
            Some(theirs),
            &all,
        )
        .expect("the record changes");
        let merged = artists::Missing::decode(&text).unwrap();
        // The other device's artist is kept, this one's added, and the
        // artist whose photo goes up in this commit is not missing.
        let slugs: Vec<&str> = merged.artists.keys().map(String::as_str).collect();
        assert_eq!(slugs, ["burial", "can"]);
        // An artist no entry names any more leaves the record, whoever
        // recorded them.
        let left = |slug: &str| slug != "can";
        let text = missing_text(
            &[],
            "covers",
            &made.edits,
            &made.missing,
            Some(theirs),
            &left,
        )
        .expect("the record changes");
        let merged = artists::Missing::decode(&text).unwrap();
        let slugs: Vec<&str> = merged.artists.keys().map(String::as_str).collect();
        assert_eq!(slugs, ["burial"]);
        // A copy that cannot be read is left alone.
        assert_eq!(
            missing_text(&[], "covers", &made.edits, &made.missing, Some("{}"), &all),
            None
        );
    }

    #[test]
    fn settings_edits_are_set_aside_for_the_merge_and_touch_no_cover() {
        let hide = shared::Edit::Hide("tmdb:movie:1".into());
        let changes = [
            upload("movies/a.jpg"),
            Change::Settings(shared::Edit::Timeline(true)),
            Change::Settings(hide.clone()),
        ];
        let plan = plan_guarded(&tree(&[]), "covers", &changes, &|_| None);
        assert_eq!(plan.settings, [shared::Edit::Timeline(true), hide]);
        assert_eq!(plan.edits.len(), 1, "the upload alone");
        // The file is written from the repo's copy as it is then.
        let theirs = "{\n  \"version\": 1,\n  \"seasonal_theme_default\": false\n}\n";
        let text = shared::merged(Some(theirs), &plan.settings)
            .unwrap()
            .unwrap();
        let settings = shared::decode(&text).unwrap();
        assert!(settings.timeline && !settings.seasonal);
        assert!(settings.not_interested.contains("tmdb:movie:1"));
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
