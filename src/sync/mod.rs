//! GitHub sync (phase 3): the library on GitHub with a copy on this
//! computer, following the phone's rules (its DESIGN.md sections 9 and
//! 10b, `github_library_repository.dart`).
//!
//! Reads: the local file shows at once; a conditional raw request asks
//! GitHub whether the file changed, on start and whenever the app asks.
//! Writes: an operation goes into `pending.json`, then into the local
//! file; after a short pause every pending operation is sent as one
//! commit whose message is the operation's summary ("Mark The Mummy as
//! done"). Offline, the queue waits for the next trigger.
//!
//! Every sync is a three-way comparison against the base in `sync.json`,
//! the blob of the file as it was when both sides last matched: this
//! computer's copy against the base's canonical form, GitHub's blob
//! against the base's blob. Neither changed: nothing to do. Only this
//! computer: push. Only GitHub: pull. Both, or a save refused for a
//! stale sha: a conflict, and nothing moves until the user picks a side.
//!
//! The [`Engine`] here is the logic alone and never touches the network:
//! it hands out one [`Request`] at a time and takes the [`Answer`] back,
//! so the app runs each request as an iced Task against the real
//! [`github::Client`](crate::github::Client) and the tests script the
//! answers.

pub mod base;
pub mod pending;

use std::path::Path;

use crate::clock::Timestamp;
use crate::connection::Target;
use crate::github::{CommitInfo, Failure, FailureKind, PutResult, RawFile, RepoInfo, blob_sha};
use crate::model::{Library, codec};
use crate::repo::Operation;
use crate::settings::Missing;

pub use base::{Base, BaseFile};
pub use pending::{InFlight, Pending, PendingQueue};

/// How long edits are gathered before they go out as one commit.
pub const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(2500);
/// How often GitHub is asked for changes while Libri runs, and how old
/// a check may be when the window comes back before it is repeated.
pub const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// What a round does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Settle a save whose answer was lost, compare both sides to the
    /// base, then pull, push, or stop at a conflict.
    Sync,
    /// This computer's copy replaces GitHub's, whatever is there now.
    KeepMine,
    /// GitHub's copy replaces this computer's and the queue is dropped.
    KeepTheirs,
}

/// One GitHub call the engine wants made, against the connection's file
/// and branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The file in the raw format; conditional with an ETag.
    Raw { etag: Option<String> },
    /// A save through the Contents API.
    Put {
        bytes: Vec<u8>,
        message: String,
        sha: Option<String>,
    },
    /// The newest commits that touched the file.
    Commits { limit: usize },
    /// The repository itself, to tell what a 404 was about.
    Repo,
}

type Reply<T> = Result<T, Failure>;

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Raw(Reply<Option<RawFile>>),
    Put(Reply<PutResult>),
    Commits(Reply<Vec<CommitInfo>>),
    Repo(Reply<RepoInfo>),
}

impl Answer {
    fn failure(&self) -> Option<&Failure> {
        match self {
            Answer::Raw(Err(f))
            | Answer::Put(Err(f))
            | Answer::Commits(Err(f))
            | Answer::Repo(Err(f)) => Some(f),
            _ => None,
        }
    }
}

/// What the sync is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Nothing in progress and the last contact with GitHub went fine.
    #[default]
    Idle,
    /// Asking GitHub whether the file changed.
    Checking,
    /// A commit is on its way.
    Saving,
    /// The last call found no connection; the queue waits.
    Offline,
    /// The last save or check failed for another reason (rate limit, a
    /// server error); Retry sends it again.
    Failed,
    /// GitHub refused the token.
    TokenProblem,
    /// The file changed on GitHub and on this computer since they last
    /// matched. Nothing moves until a side is picked.
    Conflict,
    /// GitHub's copy is no library any more (a hand edit broke it), so it
    /// cannot replace this computer's. Nothing moves until it is fixed
    /// there or "Keep mine" replaces it; the checks go on, so a fixed
    /// file is picked up again. The reason is `Engine::remote_error`.
    Unreadable,
    /// GitHub answered 404: the file, its branch or the whole repo is
    /// gone, or the token no longer sees the repo. Nothing syncs until the
    /// connection is fixed; the checks go on, so a restored file or
    /// branch is picked up again.
    NotFound(Missing),
}

/// The sync state the sidebar row, the banner and Settings show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    pub phase: Phase,
    /// Operations not yet saved to GitHub.
    pub pending: usize,
    /// When this computer's copy last matched GitHub.
    pub last_synced: Option<Timestamp>,
    /// When GitHub last answered a check.
    pub last_checked: Option<Timestamp>,
    /// The date of GitHub's last commit on the file, looked up when a
    /// conflict is found ("GitHub: last commit Oct 13, 20:12").
    pub remote_changed_at: Option<Timestamp>,
}

/// What a step of the engine asks for: the next request, if any, and a
/// library that replaces this computer's copy, if GitHub's was taken.
#[derive(Debug, Default)]
pub struct Outcome {
    pub request: Option<Request>,
    pub library: Option<Library>,
}

impl Outcome {
    fn request(request: Request) -> Outcome {
        Outcome {
            request: Some(request),
            library: None,
        }
    }
}

/// Where a round is, between a request and its answer.
#[derive(Debug, Clone)]
enum Step {
    /// A save whose answer was lost: the file as it is now tells whether
    /// it landed.
    Settle,
    /// The file is neither the save nor the base: the commit list tells.
    SettleByCommits {
        in_flight: InFlight,
    },
    /// The conditional read of a check.
    Check,
    /// The last commit's date, for the conflict dialog.
    LastCommit,
    /// GitHub's copy, to replace this computer's: with the queue applied
    /// on top, or dropped whole ("Keep GitHub's").
    TakeRemote {
        reapply: bool,
    },
    /// A save on its way.
    Put {
        ids: Vec<u64>,
        keep_mine: bool,
    },
    /// "Keep mine": the file as it is now, whose sha the save replaces.
    KeepMineFetch,
    /// A 404: the repository, then the file's commits, tell what is gone.
    ReportRepo,
    ReportCommits,
}

#[derive(Debug)]
pub struct Engine {
    target: Target,
    pending: PendingQueue,
    base_file: BaseFile,
    base: Option<Base>,
    phase: Phase,
    remote_changed_at: Option<Timestamp>,
    /// Why GitHub's copy cannot be read, while the phase is Unreadable.
    remote_error: Option<String>,
    step: Option<Step>,
    /// The round running, and whether it pushes after its check: a save
    /// refused for a stale sha is checked again, and the queue goes out
    /// on the next round instead.
    kind: Kind,
    push: bool,
    /// A round asked for while another ran; it starts when that ends.
    queued: Option<Kind>,
    /// A fixed "now" for the tests.
    clock: Option<Timestamp>,
}

impl Engine {
    /// The engine for `target`, with its queue and base from `dir`
    /// (`pending.json` and `sync.json`), or in memory alone without one.
    pub fn open(dir: Option<&Path>, target: Target) -> Engine {
        let (pending, base_file) = match dir {
            Some(dir) => (
                PendingQueue::open(dir.join("pending.json")),
                BaseFile::at(dir.join("sync.json")),
            ),
            None => (PendingQueue::in_memory(), BaseFile::default()),
        };
        let base = base_file.load(&target.key());
        Engine {
            target,
            pending,
            base_file,
            base,
            phase: Phase::Idle,
            remote_changed_at: None,
            remote_error: None,
            step: None,
            kind: Kind::Sync,
            push: true,
            queued: None,
            clock: None,
        }
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    /// Whether a base is known, so the local file is a copy of GitHub's.
    pub fn has_base(&self) -> bool {
        self.base.is_some()
    }

    /// Takes a base written by the setup (a file just created or
    /// replaced), so the next check is conditional.
    pub fn set_base(&mut self, base: Base) {
        self.save_base(base);
    }

    pub fn set_clock(&mut self, now: Timestamp) {
        self.clock = Some(now);
    }

    fn now(&self) -> Timestamp {
        self.clock.unwrap_or_else(Timestamp::now)
    }

    pub fn status(&self) -> Status {
        Status {
            phase: self.phase,
            pending: self.pending.len(),
            last_synced: self.base.as_ref().and_then(|b| b.last_synced),
            last_checked: self.base.as_ref().and_then(|b| b.last_checked),
            remote_changed_at: self.remote_changed_at,
        }
    }

    /// Why GitHub's copy cannot be read, while the phase is Unreadable:
    /// serde's message with the line and column.
    pub fn remote_error(&self) -> Option<&str> {
        self.remote_error.as_deref()
    }

    /// Whether a request is out.
    pub fn running(&self) -> bool {
        self.step.is_some()
    }

    pub fn pending(&self) -> &PendingQueue {
        &self.pending
    }

    /// Queues a change before the local file applies it: a crash between
    /// the two never leaves a change in the file that GitHub will not
    /// get.
    pub fn queue(&mut self, operation: Operation, summary: String) {
        self.pending.add(operation, summary);
    }

    /// A reconnect to the same file: the round out is forgotten (its
    /// answer is dropped by the app) and the phase starts over, while the
    /// queue, a save in flight and the base stay for the next round to
    /// settle and compare.
    pub fn restart(&mut self) {
        self.step = None;
        self.queued = None;
        self.phase = Phase::Idle;
        self.remote_changed_at = None;
        self.remote_error = None;
        self.push = true;
    }

    /// Disconnect: the queue goes; the base too unless the library is
    /// kept on this computer, for the comparison on a reconnect.
    pub fn discard(&mut self, keep_library: bool) {
        self.pending.delete();
        if !keep_library {
            self.base_file.delete();
            self.base = None;
        }
        self.step = None;
        self.queued = None;
    }

    /// Starts a round, or queues it behind the one running.
    pub fn begin(&mut self, kind: Kind) -> Outcome {
        if self.step.is_some() {
            // The later ask wins; a Sync behind a resolution is moot.
            if self.queued != Some(Kind::KeepMine) && self.queued != Some(Kind::KeepTheirs) {
                self.queued = Some(kind);
            }
            return Outcome::default();
        }
        self.kind = kind;
        self.push = true;
        match kind {
            Kind::KeepTheirs => {
                self.phase = Phase::Checking;
                self.step = Some(Step::TakeRemote { reapply: false });
                Outcome::request(Request::Raw { etag: None })
            }
            Kind::KeepMine => {
                self.phase = Phase::Saving;
                self.step = Some(Step::KeepMineFetch);
                Outcome::request(Request::Raw { etag: None })
            }
            Kind::Sync => {
                self.phase = Phase::Checking;
                if self.pending.in_flight().is_some() {
                    self.step = Some(Step::Settle);
                    Outcome::request(Request::Raw { etag: None })
                } else {
                    self.start_check()
                }
            }
        }
    }

    /// Asks GitHub whether the file changed: conditionally against the
    /// base, or, with no base (a lost `sync.json`, or one for another
    /// file), GitHub's copy whole with whatever is queued applied on top.
    fn start_check(&mut self) -> Outcome {
        match &self.base {
            None => {
                self.step = Some(Step::TakeRemote { reapply: true });
                Outcome::request(Request::Raw { etag: None })
            }
            Some(base) => {
                self.step = Some(Step::Check);
                Outcome::request(Request::Raw {
                    etag: base.etag.clone(),
                })
            }
        }
    }

    /// Takes the answer to the request out, with this computer's library
    /// as it is now, and says what comes next.
    pub fn answer(&mut self, answer: Answer, library: &Library) -> Outcome {
        let Some(step) = self.step.take() else {
            return Outcome::default();
        };
        if let Some(failure) = answer.failure() {
            let failure = failure.clone();
            return match step {
                Step::LastCommit => {
                    log(format_args!("could not read the last commit: {failure}"));
                    self.finish(Phase::Conflict)
                }
                Step::Put {
                    ids: _,
                    keep_mine: false,
                } if failure.kind == FailureKind::Conflict => {
                    // A rejected save never landed. The file changed under
                    // it: both sides changed, unless the check finds this
                    // computer's copy on GitHub.
                    self.pending.clear_in_flight();
                    log("the file changed under the save");
                    self.phase = Phase::Checking;
                    self.push = false;
                    self.start_check()
                }
                Step::ReportRepo if failure.kind == FailureKind::NotFound => {
                    self.finish(Phase::NotFound(Missing::Repo))
                }
                Step::ReportCommits if failure.kind == FailureKind::NotFound => {
                    self.finish(Phase::NotFound(Missing::Branch))
                }
                Step::ReportRepo | Step::ReportCommits => {
                    // Offline or refused while asking: that state, then.
                    self.finish(phase_for(&failure))
                }
                _ => self.report(failure),
            };
        }
        match (step, answer) {
            (Step::Settle, Answer::Raw(Ok(raw))) => self.settle(raw),
            (Step::SettleByCommits { in_flight }, Answer::Commits(Ok(commits))) => {
                self.settle_by_commits(&in_flight, &commits)
            }
            (Step::Check, Answer::Raw(Ok(raw))) => self.checked(raw, library),
            (Step::LastCommit, Answer::Commits(Ok(commits))) => {
                self.remote_changed_at = commits.first().map(|c| c.date);
                self.finish(Phase::Conflict)
            }
            (Step::TakeRemote { reapply }, Answer::Raw(Ok(Some(raw)))) => {
                match self.decode_remote(&raw) {
                    Some(remote) => self.take_remote(&raw, remote, reapply),
                    None => self.finish(Phase::Unreadable),
                }
            }
            (Step::TakeRemote { .. }, Answer::Raw(Ok(None))) => {
                log("an unconditional request answered 304");
                self.finish(Phase::Failed)
            }
            (Step::Put { ids, .. }, Answer::Put(Ok(result))) => {
                self.saved(&result);
                self.pending.remove(&ids);
                self.finish(Phase::Idle)
            }
            (Step::KeepMineFetch, Answer::Raw(Ok(Some(current)))) => {
                self.keep_mine(&current, library)
            }
            (Step::KeepMineFetch, Answer::Raw(Ok(None))) => {
                log("an unconditional request answered 304");
                self.finish(Phase::Failed)
            }
            (Step::ReportRepo, Answer::Repo(Ok(_))) => {
                self.step = Some(Step::ReportCommits);
                Outcome::request(Request::Commits { limit: 1 })
            }
            (Step::ReportCommits, Answer::Commits(Ok(_))) => {
                self.finish(Phase::NotFound(Missing::File))
            }
            (step, answer) => {
                log(format_args!("unexpected answer {answer:?} at {step:?}"));
                self.finish(Phase::Failed)
            }
        }
    }

    /// A save was started and its answer never came. Find out whether it
    /// landed before anything is sent again, so no change is saved twice.
    fn settle(&mut self, raw: Option<RawFile>) -> Outcome {
        let Some(in_flight) = self.pending.in_flight().cloned() else {
            return self.start_check();
        };
        let Some(raw) = raw else {
            return self.start_check();
        };
        let now = self.now();
        if raw.blob_sha == in_flight.blob_sha {
            log(format_args!(
                "the save \"{}\" landed; dropping its operations",
                first_line(&in_flight.message)
            ));
            self.save_base(Base {
                target: self.target.key(),
                etag: raw.etag,
                remote_sha: raw.blob_sha.clone(),
                canonical_sha: raw.blob_sha,
                commit_sha: None,
                last_synced: Some(now),
                last_checked: Some(now),
            });
            self.pending.remove(&in_flight.ids);
            return self.start_check();
        }
        if self.base.as_ref().map(|b| &b.remote_sha) == Some(&raw.blob_sha) {
            // The file is as it was before the save: it did not land.
            self.pending.clear_in_flight();
            if let Some(mut base) = self.base.clone() {
                base.etag = raw.etag;
                base.last_checked = Some(now);
                self.save_base(base);
            }
            return self.start_check();
        }
        // Something else was saved since. The commit list tells whether
        // ours is among it.
        self.step = Some(Step::SettleByCommits { in_flight });
        Outcome::request(Request::Commits { limit: 5 })
    }

    fn settle_by_commits(&mut self, in_flight: &InFlight, commits: &[CommitInfo]) -> Outcome {
        let since = Timestamp(in_flight.started_at.0 - 5 * 60);
        let landed = commits
            .iter()
            .any(|c| c.message == in_flight.message && c.date > since);
        if landed {
            log(format_args!(
                "the save \"{}\" landed under later changes",
                first_line(&in_flight.message)
            ));
            self.pending.remove(&in_flight.ids);
            // The base moves to the save: the later changes are GitHub's
            // alone, and the check that follows pulls them.
            let last_synced = self.base.as_ref().and_then(|b| b.last_synced);
            self.save_base(Base {
                target: self.target.key(),
                etag: None,
                remote_sha: in_flight.blob_sha.clone(),
                canonical_sha: in_flight.blob_sha.clone(),
                commit_sha: None,
                last_synced,
                last_checked: Some(self.now()),
            });
        } else {
            self.pending.clear_in_flight();
        }
        self.start_check()
    }

    /// Compares both sides to the base. Pulls when only GitHub changed,
    /// pushes when only this computer did, and stops at a conflict.
    fn checked(&mut self, raw: Option<RawFile>, library: &Library) -> Outcome {
        let Some(mut base) = self.base.clone() else {
            return self.start_check();
        };
        let now = self.now();
        let remote_changed = raw.as_ref().is_some_and(|r| r.blob_sha != base.remote_sha);
        let local_changed = sha_of(library) != base.canonical_sha;
        if !remote_changed {
            if let Some(raw) = &raw {
                base.etag = raw.etag.clone();
            }
            base.last_checked = Some(now);
            if !local_changed {
                base.last_synced = Some(now);
            }
            self.save_base(base);
            return if self.push {
                self.push(library)
            } else {
                self.finish(Phase::Idle)
            };
        }
        let raw = raw.expect("a changed file came with its content");
        // A copy that cannot be read is told before a conflict is: there
        // is no "Keep GitHub's" to offer then.
        let Some(remote) = self.decode_remote(&raw) else {
            return self.finish(Phase::Unreadable);
        };
        if !local_changed {
            return self.take_remote(&raw, remote, true);
        }
        log(
            "the file changed on GitHub and on this computer since they last matched; waiting for a choice",
        );
        base.last_checked = Some(now);
        self.save_base(base);
        // When GitHub's copy last changed, for the conflict dialog. Not
        // worth failing over.
        self.step = Some(Step::LastCommit);
        Outcome::request(Request::Commits { limit: 1 })
    }

    /// GitHub's copy decoded, or `None` with the reason kept for the
    /// Unreadable phase the caller ends in: a file that is no library (a
    /// hand edit that broke it) can replace nothing, so the sync stops
    /// until it is fixed there or "Keep mine" replaces it.
    fn decode_remote(&mut self, raw: &RawFile) -> Option<Library> {
        match raw
            .text()
            .map_err(|e| e.to_string())
            .and_then(|text| codec::decode(text).map_err(|e| e.to_string()))
        {
            Ok(remote) => Some(remote),
            Err(error) => {
                log(format_args!("the file on GitHub cannot be read: {error}"));
                self.remote_error = Some(error);
                None
            }
        }
    }

    /// Takes GitHub's copy (`raw`, decoded as `remote`) as this
    /// computer's and the new base. With `reapply` the queued operations
    /// go on top of it, dropping any that no longer applies; without it
    /// the queue is dropped whole.
    fn take_remote(&mut self, raw: &RawFile, remote: Library, reapply: bool) -> Outcome {
        let mut library = remote.clone();
        let mut dropped = Vec::new();
        for pending in self.pending.operations() {
            if !reapply {
                dropped.push(pending.id);
                continue;
            }
            if let Err(error) = pending.operation.clone().apply_to(&mut library) {
                log(format_args!(
                    "dropping \"{}\", it no longer applies: {error}",
                    pending.summary
                ));
                dropped.push(pending.id);
            }
        }
        if !dropped.is_empty() {
            self.pending.remove(&dropped);
        }
        let now = self.now();
        let last_synced = if self.pending.is_empty() {
            Some(now)
        } else {
            self.base.as_ref().and_then(|b| b.last_synced)
        };
        self.save_base(Base {
            target: self.target.key(),
            etag: raw.etag.clone(),
            remote_sha: raw.blob_sha.clone(),
            canonical_sha: sha_of(&remote),
            commit_sha: None,
            last_synced,
            last_checked: Some(now),
        });
        let mut outcome = if self.kind == Kind::Sync && self.push {
            self.push(&library)
        } else {
            self.finish(Phase::Idle)
        };
        outcome.library = Some(library);
        outcome
    }

    /// Sends this computer's copy as one commit when it differs from the
    /// base: the queued operations with their summaries, or, with nothing
    /// queued (edits made while the library was kept on this computer),
    /// a plain message.
    fn push(&mut self, library: &Library) -> Outcome {
        let Some(base) = self.base.clone() else {
            return self.finish(Phase::Idle);
        };
        let bytes = codec::encode(library).into_bytes();
        let sha = blob_sha(&bytes);
        let ids = self.pending.ids();
        if sha == base.canonical_sha {
            // Nothing to send: the changes cancel out, or are there
            // already.
            if !ids.is_empty() {
                self.pending.remove(&ids);
            }
            return self.finish(Phase::Idle);
        }
        let message = if self.pending.is_empty() {
            "Save the changes made on this computer".to_owned()
        } else {
            commit_message(self.pending.operations())
        };
        self.pending.mark_in_flight(InFlight {
            ids: ids.clone(),
            blob_sha: sha,
            message: message.clone(),
            started_at: self.now(),
        });
        self.phase = Phase::Saving;
        self.step = Some(Step::Put {
            ids,
            keep_mine: false,
        });
        Outcome::request(Request::Put {
            bytes,
            message,
            sha: Some(base.remote_sha),
        })
    }

    /// "Keep mine": this computer's copy over the file as it is now.
    fn keep_mine(&mut self, current: &RawFile, library: &Library) -> Outcome {
        let bytes = codec::encode(library).into_bytes();
        let ids = self.pending.ids();
        let mut message = "Keep this computer's copy of the library".to_owned();
        if !self.pending.is_empty() {
            message.push_str("\n\n");
            message.push_str(&summaries(self.pending.operations()));
        }
        self.pending.mark_in_flight(InFlight {
            ids: ids.clone(),
            blob_sha: blob_sha(&bytes),
            message: message.clone(),
            started_at: self.now(),
        });
        self.step = Some(Step::Put {
            ids,
            keep_mine: true,
        });
        Outcome::request(Request::Put {
            bytes,
            message,
            sha: Some(current.blob_sha.clone()),
        })
    }

    /// The base after a successful save: the file is what this computer
    /// wrote. The raw ETag is the quoted blob sha, so the next check can
    /// be conditional without a download first.
    fn saved(&mut self, result: &PutResult) {
        let now = self.now();
        self.save_base(Base {
            target: self.target.key(),
            etag: Some(format!("\"{}\"", result.blob_sha)),
            remote_sha: result.blob_sha.clone(),
            canonical_sha: result.blob_sha.clone(),
            commit_sha: Some(result.commit_sha.clone()),
            last_synced: Some(now),
            last_checked: Some(now),
        });
    }

    /// A failed call ends the round in its state. A 404 gets two more
    /// calls to tell what is gone: the repo (or a token that no longer
    /// sees it), the branch, or the file. That is not a failed save:
    /// Retry cannot help, only the connection can be fixed.
    fn report(&mut self, failure: Failure) -> Outcome {
        log(format_args!("sync failed: {failure}"));
        if failure.kind == FailureKind::NotFound {
            self.step = Some(Step::ReportRepo);
            return Outcome::request(Request::Repo);
        }
        self.finish(phase_for(&failure))
    }

    /// Ends the round in `phase`, and starts the round queued behind it.
    fn finish(&mut self, phase: Phase) -> Outcome {
        self.step = None;
        self.phase = phase;
        if phase != Phase::Conflict {
            self.remote_changed_at = None;
        }
        if phase != Phase::Unreadable {
            self.remote_error = None;
        }
        match self.queued.take() {
            Some(kind) => self.begin(kind),
            None => Outcome::default(),
        }
    }

    fn save_base(&mut self, mut base: Base) {
        base.target = self.target.key();
        self.base_file.save(&base);
        self.base = Some(base);
    }
}

fn phase_for(failure: &Failure) -> Phase {
    match failure.kind {
        FailureKind::Unauthorized | FailureKind::Forbidden => Phase::TokenProblem,
        FailureKind::Offline => Phase::Offline,
        _ => Phase::Failed,
    }
}

/// The blob sha of a library in the canonical form.
fn sha_of(library: &Library) -> String {
    blob_sha(codec::encode(library).as_bytes())
}

/// One operation: its summary. Several: the first and how many more,
/// with every summary listed in the body.
pub fn commit_message(operations: &[Pending]) -> String {
    let first = operations
        .first()
        .map(|o| o.summary.as_str())
        .unwrap_or_default();
    if operations.len() <= 1 {
        return first.to_owned();
    }
    let others = operations.len() - 1;
    format!(
        "{first} and {others} other change{}\n\n{}",
        if others == 1 { "" } else { "s" },
        summaries(operations)
    )
}

/// "- Mark A as done\n- Rate B 7 of 10".
fn summaries(operations: &[Pending]) -> String {
    operations
        .iter()
        .map(|o| format!("- {}", o.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

fn log(what: impl std::fmt::Display) {
    crate::trace::line(format_args!("sync: {what}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model::EntryStatus;

    const MUMMY: &str = "movies/the-mummy-1999";
    const DIE_HARD: &str = "movies/die-hard-1988";

    fn target() -> Target {
        Target {
            repository: "ekmanch/libri-scratch".into(),
            file: "library.json".into(),
            branch: "main".into(),
        }
    }

    fn fixture() -> Library {
        codec::decode(fixtures::LIBRARY_JSON).unwrap()
    }

    /// The file as GitHub serves it: canonical bytes, the quoted sha as
    /// the ETag.
    fn raw(library: &Library) -> RawFile {
        let bytes = codec::encode(library).into_bytes();
        let sha = blob_sha(&bytes);
        RawFile::new(bytes, Some(format!("\"{sha}\"")))
    }

    fn status(id: &str, status: EntryStatus) -> Operation {
        Operation::SetStatus {
            id: id.into(),
            status,
        }
    }

    /// A status the entry does not have, so the change is a change.
    fn flip(library: &Library, id: &str) -> Operation {
        let current = library.entry(id).unwrap().status;
        status(
            id,
            if current == EntryStatus::Want {
                EntryStatus::Done
            } else {
                EntryStatus::Want
            },
        )
    }

    fn with(library: &Library, operation: Operation) -> Library {
        let mut next = library.clone();
        operation.apply_to(&mut next).unwrap();
        next
    }

    /// An engine whose base is the fixture, as after a first download.
    fn synced() -> (Engine, Library) {
        let mut engine = Engine::open(None, target());
        engine.set_clock(Timestamp(1_000));
        let library = fixture();
        let outcome = engine.begin(Kind::Sync);
        assert_eq!(outcome.request, Some(Request::Raw { etag: None }));
        let outcome = engine.answer(Answer::Raw(Ok(Some(raw(&library)))), &Library::default());
        assert_eq!(outcome.library, Some(library.clone()));
        assert_eq!(outcome.request, None);
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.status().last_synced, Some(Timestamp(1_000)));
        (engine, library)
    }

    fn put_ok(sha: &str) -> Answer {
        Answer::Put(Ok(PutResult {
            blob_sha: sha.into(),
            commit_sha: "c1".into(),
        }))
    }

    fn failure(kind: FailureKind) -> Failure {
        Failure::new(kind, "")
    }

    #[test]
    fn without_a_base_the_first_check_takes_githubs_copy() {
        let (engine, _) = synced();
        assert!(engine.has_base());
        assert_eq!(engine.status().pending, 0);
    }

    #[test]
    fn a_check_with_nothing_changed_is_one_conditional_request() {
        let (mut engine, library) = synced();
        engine.set_clock(Timestamp(2_000));
        let outcome = engine.begin(Kind::Sync);
        let etag = Some(format!("\"{}\"", raw(&library).blob_sha));
        assert_eq!(outcome.request, Some(Request::Raw { etag }));
        let outcome = engine.answer(Answer::Raw(Ok(None)), &library);
        assert_eq!(outcome.request, None);
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.status().last_synced, Some(Timestamp(2_000)));
        assert_eq!(engine.status().last_checked, Some(Timestamp(2_000)));
    }

    #[test]
    fn a_local_change_is_pushed_with_its_summary_and_the_bases_sha() {
        let (mut engine, library) = synced();
        let base_sha = raw(&library).blob_sha;
        engine.queue(
            status(MUMMY, EntryStatus::Want),
            "Mark The Mummy as want".into(),
        );
        let edited = with(&library, status(MUMMY, EntryStatus::Want));
        assert_eq!(engine.status().pending, 1);

        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(None)), &edited);
        let Some(Request::Put {
            bytes,
            message,
            sha,
        }) = outcome.request
        else {
            panic!("a save was expected");
        };
        assert_eq!(bytes, codec::encode(&edited).into_bytes());
        assert_eq!(message, "Mark The Mummy as want");
        assert_eq!(sha, Some(base_sha));
        assert_eq!(engine.status().phase, Phase::Saving);
        assert!(engine.pending().in_flight().is_some());

        let outcome = engine.answer(put_ok("new-sha"), &edited);
        assert_eq!(outcome.request, None);
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.status().pending, 0);
        assert!(engine.pending().in_flight().is_none());
        // The next check is conditional on the saved sha.
        let outcome = engine.begin(Kind::Sync);
        assert_eq!(
            outcome.request,
            Some(Request::Raw {
                etag: Some("\"new-sha\"".into())
            })
        );
    }

    #[test]
    fn several_changes_make_one_commit_with_every_summary_in_the_body() {
        let (mut engine, library) = synced();
        engine.queue(
            status(MUMMY, EntryStatus::Want),
            "Mark The Mummy as want".into(),
        );
        engine.queue(
            status(DIE_HARD, EntryStatus::Done),
            "Mark Die Hard as done".into(),
        );
        engine.queue(
            Operation::SetRating {
                id: MUMMY.into(),
                rating: Some(7),
            },
            "Rate The Mummy 7 of 10".into(),
        );
        let edited = with(
            &with(
                &with(&library, status(MUMMY, EntryStatus::Want)),
                status(DIE_HARD, EntryStatus::Done),
            ),
            Operation::SetRating {
                id: MUMMY.into(),
                rating: Some(7),
            },
        );
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(None)), &edited);
        let Some(Request::Put { message, .. }) = outcome.request else {
            panic!("a save was expected");
        };
        assert_eq!(
            message,
            "Mark The Mummy as want and 2 other changes\n\n- Mark The Mummy as want\n- Mark Die Hard as done\n- Rate The Mummy 7 of 10"
        );
        assert_eq!(
            commit_message(&engine.pending().operations()[..2]),
            "Mark The Mummy as want and 1 other change\n\n- Mark The Mummy as want\n- Mark Die Hard as done"
        );
    }

    #[test]
    fn a_change_on_github_alone_is_pulled() {
        let (mut engine, library) = synced();
        let remote = with(&library, flip(&library, DIE_HARD));
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(Some(raw(&remote)))), &library);
        assert_eq!(outcome.library, Some(remote.clone()));
        assert_eq!(outcome.request, None, "nothing to push back");
        assert_eq!(engine.status().phase, Phase::Idle);
        // A change queued here and another on GitHub.
        let mine = flip(&remote, MUMMY);
        engine.queue(mine.clone(), "Mark The Mummy".into());
        let local = with(&remote, mine);
        let remote2 = with(&remote, flip(&remote, DIE_HARD));
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(Some(raw(&remote2)))), &local);
        // Both changed: that is a conflict, not a merge.
        assert_eq!(outcome.request, Some(Request::Commits { limit: 1 }));
        assert_eq!(outcome.library, None);
    }

    #[test]
    fn both_changed_is_a_conflict_until_a_side_is_picked() {
        let (mut engine, library) = synced();
        let mine = flip(&library, MUMMY);
        engine.queue(mine.clone(), "Mark The Mummy as want".into());
        let local = with(&library, mine.clone());
        let remote = with(&library, flip(&library, DIE_HARD));
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(Some(raw(&remote)))), &local);
        assert_eq!(outcome.request, Some(Request::Commits { limit: 1 }));
        let commit = CommitInfo {
            sha: "r1".into(),
            message: "Mark Die Hard as done".into(),
            date: Timestamp(900),
        };
        let outcome = engine.answer(Answer::Commits(Ok(vec![commit])), &local);
        assert_eq!(outcome.request, None);
        assert_eq!(engine.status().phase, Phase::Conflict);
        assert_eq!(engine.status().remote_changed_at, Some(Timestamp(900)));
        assert_eq!(engine.status().pending, 1);

        // Keep GitHub's: its copy replaces this computer's, the queue goes.
        let mut theirs = Engine::open(None, target());
        std::mem::swap(&mut theirs, &mut engine);
        let outcome = theirs.begin(Kind::KeepTheirs);
        assert_eq!(outcome.request, Some(Request::Raw { etag: None }));
        let outcome = theirs.answer(Answer::Raw(Ok(Some(raw(&remote)))), &local);
        assert_eq!(outcome.library, Some(remote.clone()));
        assert_eq!(outcome.request, None);
        assert_eq!(theirs.status().phase, Phase::Idle);
        assert_eq!(theirs.status().pending, 0);
        assert_eq!(theirs.status().remote_changed_at, None);

        // Keep mine: this computer's copy over whatever is there now.
        let (mut engine, library) = synced();
        engine.queue(mine, "Mark The Mummy as want".into());
        let outcome = engine.begin(Kind::KeepMine);
        assert_eq!(outcome.request, Some(Request::Raw { etag: None }));
        assert_eq!(engine.status().phase, Phase::Saving);
        let current = raw(&remote);
        let outcome = engine.answer(Answer::Raw(Ok(Some(current.clone()))), &local);
        let Some(Request::Put {
            bytes,
            message,
            sha,
        }) = outcome.request
        else {
            panic!("a save was expected");
        };
        assert_eq!(bytes, codec::encode(&local).into_bytes());
        assert_eq!(
            message,
            "Keep this computer's copy of the library\n\n- Mark The Mummy as want"
        );
        assert_eq!(sha, Some(current.blob_sha));
        let _ = engine.answer(put_ok("mine"), &local);
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.status().pending, 0);
        let _ = library;
    }

    #[test]
    fn a_file_on_github_that_is_no_library_stops_with_the_reason() {
        // A hand edit on GitHub put a number where a string goes. The
        // check finds a changed file that cannot be read: not a conflict,
        // since there is no "Keep GitHub's" to offer, and not a failed
        // save.
        let (mut engine, library) = synced();
        let mine = flip(&library, MUMMY);
        engine.queue(mine.clone(), "Mark The Mummy as want".into());
        let local = with(&library, mine);
        let broken = RawFile::new(
            br#"{"entries": [{"id": "movies/x", "released": 2015}]}"#.to_vec(),
            Some("\"broken\"".into()),
        );
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(Some(broken.clone()))), &local);
        assert_eq!(outcome.request, None);
        assert_eq!(outcome.library, None);
        assert_eq!(engine.status().phase, Phase::Unreadable);
        assert_eq!(engine.status().pending, 1);
        let reason = engine.remote_error().unwrap().to_owned();
        assert!(reason.contains("2015"), "{reason}");
        assert!(reason.contains("line 1"), "{reason}");

        // Keep GitHub's cannot help either: the same state again.
        let _ = engine.begin(Kind::KeepTheirs);
        let outcome = engine.answer(Answer::Raw(Ok(Some(broken.clone()))), &local);
        assert_eq!(outcome.library, None);
        assert_eq!(engine.status().phase, Phase::Unreadable);
        assert_eq!(engine.status().pending, 1);

        // Keep mine replaces the broken file and clears the reason.
        let outcome = engine.begin(Kind::KeepMine);
        assert_eq!(outcome.request, Some(Request::Raw { etag: None }));
        let outcome = engine.answer(Answer::Raw(Ok(Some(broken.clone()))), &local);
        let Some(Request::Put { sha, .. }) = outcome.request else {
            panic!("a save was expected");
        };
        assert_eq!(sha, Some(broken.blob_sha.clone()));
        let _ = engine.answer(put_ok("mine"), &local);
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.status().pending, 0);
        assert_eq!(engine.remote_error(), None);

        // Fixed on GitHub instead: the next check reads it again.
        let (mut engine, library) = synced();
        let _ = engine.begin(Kind::Sync);
        let _ = engine.answer(Answer::Raw(Ok(Some(broken))), &library);
        assert_eq!(engine.status().phase, Phase::Unreadable);
        let fixed = with(&library, flip(&library, DIE_HARD));
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(Some(raw(&fixed)))), &library);
        assert_eq!(outcome.library, Some(fixed));
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.remote_error(), None);
    }

    #[test]
    fn a_save_refused_for_a_stale_sha_is_checked_again_without_a_push() {
        let (mut engine, library) = synced();
        engine.queue(
            status(MUMMY, EntryStatus::Want),
            "Mark The Mummy as want".into(),
        );
        let local = with(&library, status(MUMMY, EntryStatus::Want));
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(None)), &local);
        assert!(matches!(outcome.request, Some(Request::Put { .. })));
        let outcome = engine.answer(Answer::Put(Err(failure(FailureKind::Conflict))), &local);
        assert!(matches!(
            outcome.request,
            Some(Request::Raw { etag: Some(_) })
        ));
        assert!(engine.pending().in_flight().is_none());
        // A spurious refusal: the file is as it was. The queue waits for
        // the next round.
        let outcome = engine.answer(Answer::Raw(Ok(None)), &local);
        assert_eq!(outcome.request, None);
        assert_eq!(engine.status().phase, Phase::Idle);
        assert_eq!(engine.status().pending, 1);
        // The next round sends it.
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(None)), &local);
        assert!(matches!(outcome.request, Some(Request::Put { .. })));
    }

    #[test]
    fn a_save_whose_answer_was_lost_is_settled_first() {
        // Landed: the file on GitHub is the save.
        let (mut engine, library) = synced();
        let mine = flip(&library, MUMMY);
        engine.queue(mine.clone(), "Mark The Mummy as want".into());
        let local = with(&library, mine.clone());
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(None)), &local);
        let Some(Request::Put { bytes, .. }) = outcome.request else {
            panic!()
        };
        let sent = blob_sha(&bytes);
        // The answer is lost: the engine is opened again with the queue.
        let in_flight = engine.pending().in_flight().cloned().unwrap();
        let mut again = Engine::open(None, target());
        again.set_clock(Timestamp(3_000));
        again
            .pending
            .add(mine.clone(), "Mark The Mummy as want".into());
        again.pending.mark_in_flight(in_flight.clone());
        again.set_base(engine.base.clone().unwrap());
        let outcome = again.begin(Kind::Sync);
        assert_eq!(outcome.request, Some(Request::Raw { etag: None }));
        let outcome = again.answer(Answer::Raw(Ok(Some(raw(&local)))), &local);
        assert_eq!(again.status().pending, 0, "the save landed");
        assert!(matches!(
            outcome.request,
            Some(Request::Raw { etag: Some(_) })
        ));
        let outcome = again.answer(Answer::Raw(Ok(None)), &local);
        assert_eq!(outcome.request, None);
        assert_eq!(again.status().phase, Phase::Idle);
        assert_eq!(again.base.as_ref().unwrap().remote_sha, sent);

        // Not landed: the file is the base. The save is sent again.
        let mut again = Engine::open(None, target());
        again
            .pending
            .add(mine.clone(), "Mark The Mummy as want".into());
        again.pending.mark_in_flight(in_flight.clone());
        again.set_base(engine.base.clone().unwrap());
        let _ = again.begin(Kind::Sync);
        let outcome = again.answer(Answer::Raw(Ok(Some(raw(&library)))), &local);
        assert!(again.pending().in_flight().is_none());
        assert!(matches!(
            outcome.request,
            Some(Request::Raw { etag: Some(_) })
        ));
        let outcome = again.answer(Answer::Raw(Ok(None)), &local);
        assert!(matches!(outcome.request, Some(Request::Put { .. })));

        // Landed under later changes: the commit list names it.
        let mut again = Engine::open(None, target());
        again
            .pending
            .add(mine.clone(), "Mark The Mummy as want".into());
        again.pending.mark_in_flight(in_flight.clone());
        again.set_base(engine.base.clone().unwrap());
        let later = with(&local, flip(&local, DIE_HARD));
        let _ = again.begin(Kind::Sync);
        let outcome = again.answer(Answer::Raw(Ok(Some(raw(&later)))), &local);
        assert_eq!(outcome.request, Some(Request::Commits { limit: 5 }));
        let commits = vec![
            CommitInfo {
                sha: "c2".into(),
                message: "Mark Die Hard as done".into(),
                date: Timestamp(1_200),
            },
            CommitInfo {
                sha: "c1".into(),
                message: "Mark The Mummy as want".into(),
                date: Timestamp(1_100),
            },
        ];
        let outcome = again.answer(Answer::Commits(Ok(commits)), &local);
        assert_eq!(again.status().pending, 0, "ours is among them");
        assert_eq!(again.base.as_ref().unwrap().remote_sha, sent);
        // The check that follows sees only GitHub's later change and
        // pulls it.
        assert!(matches!(outcome.request, Some(Request::Raw { etag: None })));
        let outcome = again.answer(Answer::Raw(Ok(Some(raw(&later)))), &local);
        assert_eq!(outcome.library, Some(later));
        assert_eq!(again.status().phase, Phase::Idle);
    }

    #[test]
    fn a_404_is_told_apart_and_other_failures_name_their_state() {
        let (mut engine, library) = synced();
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Err(failure(FailureKind::NotFound))), &library);
        assert_eq!(outcome.request, Some(Request::Repo));
        let outcome = engine.answer(
            Answer::Repo(Ok(RepoInfo {
                default_branch: "main".into(),
                can_push: true,
            })),
            &library,
        );
        assert_eq!(outcome.request, Some(Request::Commits { limit: 1 }));
        let _ = engine.answer(
            Answer::Commits(Err(failure(FailureKind::NotFound))),
            &library,
        );
        assert_eq!(engine.status().phase, Phase::NotFound(Missing::Branch));

        let _ = engine.begin(Kind::Sync);
        let _ = engine.answer(Answer::Raw(Err(failure(FailureKind::NotFound))), &library);
        let _ = engine.answer(Answer::Repo(Err(failure(FailureKind::NotFound))), &library);
        assert_eq!(engine.status().phase, Phase::NotFound(Missing::Repo));

        let _ = engine.begin(Kind::Sync);
        let _ = engine.answer(Answer::Raw(Err(failure(FailureKind::NotFound))), &library);
        let _ = engine.answer(
            Answer::Repo(Ok(RepoInfo {
                default_branch: "main".into(),
                can_push: true,
            })),
            &library,
        );
        let _ = engine.answer(Answer::Commits(Ok(Vec::new())), &library);
        assert_eq!(engine.status().phase, Phase::NotFound(Missing::File));

        for (kind, phase) in [
            (FailureKind::Offline, Phase::Offline),
            (FailureKind::Unauthorized, Phase::TokenProblem),
            (FailureKind::RateLimited, Phase::Failed),
            (FailureKind::Other, Phase::Failed),
        ] {
            let _ = engine.begin(Kind::Sync);
            let outcome = engine.answer(Answer::Raw(Err(failure(kind))), &library);
            assert_eq!(outcome.request, None);
            assert_eq!(engine.status().phase, phase, "{kind:?}");
            assert!(!engine.running());
        }
    }

    #[test]
    fn a_round_asked_for_while_one_runs_starts_when_it_ends() {
        let (mut engine, library) = synced();
        let _ = engine.begin(Kind::Sync);
        assert!(engine.running());
        let outcome = engine.begin(Kind::Sync);
        assert_eq!(outcome.request, None, "queued behind the running one");
        let outcome = engine.answer(Answer::Raw(Ok(None)), &library);
        assert!(
            matches!(outcome.request, Some(Request::Raw { .. })),
            "the queued round starts"
        );
        assert!(engine.running());
    }

    #[test]
    fn discard_drops_the_queue_and_keeps_the_base_only_with_the_library() {
        let (mut engine, _) = synced();
        engine.queue(status(MUMMY, EntryStatus::Want), "x".into());
        engine.discard(true);
        assert_eq!(engine.status().pending, 0);
        assert!(engine.has_base());
        engine.discard(false);
        assert!(!engine.has_base());
    }
}
