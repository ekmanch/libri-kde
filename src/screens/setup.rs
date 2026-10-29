//! First-time setup, the whole window (docs/DESIGN.md 4.2): Welcome,
//! Connect GitHub, Branch, Library file, Create or Pick, All set. The steps
//! rail sits where the sidebar normally is, the form in the content pane,
//! Back and the main action pinned at the bottom. Talks to GitHub through
//! `github::Client` (phase 3): Connect checks the token against the repo,
//! Branch lists the repo's branches, Pick file opens the JSON files on the
//! branch to tell libraries from other JSON, Create commits the new file,
//! and Open Libri keeps the token in KDE Wallet and the connection in
//! `connection.json`. A mock state (`State::at`: the tests, the captures,
//! `LIBRI_SETUP`) keeps the mockup's fixture branches and files instead.

use std::sync::Arc;

use iced::alignment::{Horizontal, Vertical};
use iced::border::Radius;
use iced::widget::{Column, button, column, container, rich_text, row, space, span, text};
use iced::{Background, Border, Center, Element, Fill, Padding, Task};

use super::Screen;
use crate::app::{self, App, Message as AppMessage};
use crate::clock::Timestamp;
use crate::connection::{Connection, Target};
use crate::github::{BranchInfo, Client, Failure, FailureKind, PutResult, RepoInfo};
use crate::model::{Library, codec};
use crate::settings::{Source, SyncState};
use crate::sync::{Base, BaseFile};
use crate::theme::{Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

/// GitHub's guide to fine-grained tokens, for "How do I create a token?".
pub const TOKEN_HELP_URL: &str = "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-fine-grained-personal-access-token";

/// The commit that creates a new library file.
pub const CREATE_MESSAGE: &str = "Create Libri library";
/// The commit that puts the library kept on this computer over a file.
pub const REPLACE_MESSAGE: &str = "Replace with the library from this computer";
/// Pick file opens at most this many JSON files, each of at most this
/// size, to tell libraries from other JSON.
const MAX_FILES: usize = 20;
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// A fixture token, so the field shows something in the captures.
const FIXTURE_TOKEN: &str = "github_pat_11A2B3C4D5E6F7G8H9I0J1K2L3M4N5O6a91F";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    #[default]
    Welcome,
    Connect,
    Branch,
    File,
    /// Create a new library file (the choice on File).
    Create,
    /// Pick an existing file (the other choice).
    Existing,
    Done,
}

impl Step {
    pub const ALL: [Step; 7] = [
        Step::Welcome,
        Step::Connect,
        Step::Branch,
        Step::File,
        Step::Create,
        Step::Existing,
        Step::Done,
    ];

    /// The `LIBRI_SETUP` value, also used in screenshot file names.
    pub fn name(self) -> &'static str {
        match self {
            Step::Welcome => "welcome",
            Step::Connect => "connect",
            Step::Branch => "branch",
            Step::File => "file",
            Step::Create => "create",
            Step::Existing => "existing",
            Step::Done => "done",
        }
    }

    pub fn from_name(name: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|step| step.name() == name)
    }

    /// The step's place on the rail, 0 to 3; Welcome and Done are outside
    /// it.
    fn index(self) -> Option<usize> {
        match self {
            Step::Welcome | Step::Done => None,
            Step::Connect => Some(0),
            Step::Branch => Some(1),
            Step::File => Some(2),
            Step::Create | Step::Existing => Some(3),
        }
    }

    fn back(self) -> Step {
        match self {
            Step::Branch => Step::Connect,
            Step::File => Step::Branch,
            Step::Create | Step::Existing => Step::File,
            _ => Step::Welcome,
        }
    }
}

/// The Library file step's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    New,
    Existing,
}

/// A branch of the repository, as listed: the default first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// "Updated 2 days ago".
    pub info: String,
    pub default: bool,
}

/// A JSON file in the repository; only Libri's own format can be picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    /// "Libri library · 214 entries · updated 2 days ago", or "Not a
    /// Libri library file".
    pub info: String,
    /// The blob sha, which a save over the file needs.
    pub sha: String,
    /// How many entries the library holds; `None` for JSON that is not
    /// a library.
    pub entries: Option<usize>,
    /// When the file was last committed, when known.
    pub updated: Option<Timestamp>,
}

impl File {
    pub fn usable(&self) -> bool {
        self.entries.is_some()
    }
}

/// The mockup's branches: `main` (the default, updated two days ago),
/// `app-sync-test` (last week) and `dev` (August).
pub fn fixture_branches() -> Vec<Branch> {
    [
        ("main", "Updated 2 days ago", true),
        ("app-sync-test", "Updated last week", false),
        ("dev", "Updated in August", false),
    ]
    .into_iter()
    .map(|(name, info, default)| Branch {
        name: name.into(),
        info: info.into(),
        default,
    })
    .collect()
}

/// The mockup's files: the library, a smaller backup from March, and
/// JSON that is not a library.
pub fn fixture_files() -> Vec<File> {
    [
        (
            "library.json",
            "Libri library · 214 entries · updated 2 days ago",
            Some(214),
        ),
        (
            "backup/library-2025.json",
            "Libri library · 168 entries · updated in March",
            Some(168),
        ),
        ("data/old-lists.json", "Not a Libri library file", None),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (path, info, entries))| File {
        path: path.into(),
        info: info.into(),
        sha: format!("fixture-{i}"),
        entries,
        updated: None,
    })
    .collect()
}

/// What the wizard is waiting on GitHub for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    /// "Checking access to owner/repo…"
    Connecting,
    /// "Loading branches…"
    Branches,
    /// "Looking for library files…"
    Files,
    /// "Creating library.json and pushing to GitHub…"
    Creating,
    /// "Replacing library.json with the library from this computer…"
    Replacing,
}

/// The client for the repository and token typed, shared with the tasks.
#[derive(Clone)]
pub struct ClientHandle(pub Arc<Client>);

impl PartialEq for ClientHandle {
    fn eq(&self, other: &ClientHandle) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ClientHandle {}

impl std::fmt::Debug for ClientHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub step: Step,
    pub repo: String,
    pub token: String,
    pub branch: usize,
    pub choice: Choice,
    pub file: usize,
    /// The name of a new library file.
    pub file_name: String,
    /// Replacing the token from the warning: the Connect step alone, then
    /// back to this screen.
    pub replacing: Option<Screen>,
    /// "Which library?": reconnecting a library kept on this computer to
    /// an existing file, and which copy was picked.
    pub which: Option<Which>,
    /// The mockup's fixture branches and files, no GitHub: the tests, the
    /// captures and `LIBRI_SETUP`.
    pub mock: bool,
    pub busy: Option<Busy>,
    /// What went wrong with the last call, under the form.
    pub error: Option<String>,
    pub branches: Vec<Branch>,
    pub files: Vec<File>,
    /// The repo's default branch, from Connect.
    pub default_branch: String,
    pub client: Option<ClientHandle>,
    /// The base of the file the setup created or replaced, for the sync.
    pub base: Option<Base>,
    /// How many entries the chosen file holds, for All set.
    pub loaded_entries: Option<usize>,
}

/// The copy that stays when a kept library meets a file on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Which {
    /// This computer's library, uploaded over the file.
    #[default]
    Here,
    /// The file on GitHub, which replaces this computer's library.
    GitHub,
}

impl State {
    /// The mock wizard at `step`, on the fixture branches and files,
    /// filled in from `source` when there is one (the tests, the captures
    /// and `LIBRI_SETUP`).
    pub fn at(step: Step, source: Option<&Source>) -> State {
        let mut state = State {
            step,
            file_name: "library.json".into(),
            mock: true,
            branches: fixture_branches(),
            files: fixture_files(),
            default_branch: "main".into(),
            ..State::default()
        };
        if let Some(source) = source {
            state.repo = source.repo.clone();
            state.token = FIXTURE_TOKEN.into();
            state.branch = state
                .branches
                .iter()
                .position(|b| b.name == source.branch)
                .unwrap_or(0);
            state.file = state
                .files
                .iter()
                .position(|f| f.path == source.file)
                .unwrap_or(0);
            state.file_name = source.file.clone();
        }
        state
    }

    /// The real wizard at `step`, with the last connection's repo and
    /// file prefilled when there was one.
    pub fn real(step: Step, last: Option<&Connection>) -> State {
        let mut state = State {
            step,
            file_name: "library.json".into(),
            default_branch: "main".into(),
            ..State::default()
        };
        if let Some(last) = last {
            state.repo = last.target.repository.clone();
            state.file_name = last.target.file.clone();
            state.default_branch = last.target.branch.clone();
        }
        state
    }

    /// On one of the four numbered steps, where the window is titled "Set
    /// up Libri".
    pub fn in_wizard(&self) -> bool {
        self.step.index().is_some()
    }

    /// The branch picked, or the default branch of a repo without any.
    fn branch_name(&self) -> String {
        self.branches
            .get(self.branch)
            .map_or_else(|| self.default_branch.clone(), |b| b.name.clone())
    }

    fn file_name(&self) -> String {
        match self.choice {
            Choice::New => self.file_name.trim().to_owned(),
            Choice::Existing => self
                .files
                .get(self.file)
                .map(|f| f.path.clone())
                .unwrap_or_default(),
        }
    }

    fn picked_file(&self) -> Option<&File> {
        self.files.get(self.file).filter(|f| f.usable())
    }

    /// Where the wizard's choices point.
    fn target(&self) -> Target {
        Target {
            repository: self.repo.trim().to_owned(),
            file: self.file_name(),
            branch: self.branch_name(),
        }
    }

    /// Whether the step's main action can go ahead.
    pub fn can_continue(&self) -> bool {
        if self.busy.is_some() {
            return false;
        }
        match self.step {
            Step::Connect => !self.repo.trim().is_empty() && !self.token.trim().is_empty(),
            Step::Create => {
                let name = self.file_name.trim();
                name.len() > 5 && name.ends_with(".json")
            }
            Step::Existing => self.picked_file().is_some(),
            _ => true,
        }
    }

    fn primary_label(&self) -> &'static str {
        match (self.step, self.busy) {
            (Step::Connect, Some(_)) => "Connecting…",
            (Step::Connect, None) => "Connect",
            (Step::Create, Some(_)) => "Creating…",
            (Step::Create, None) => "Create library",
            (Step::Existing, _) => "Use this file",
            _ => "Continue",
        }
    }

    /// The line under the form while a call runs.
    fn busy_line(&self) -> Option<String> {
        Some(match self.busy? {
            Busy::Connecting => format!("Checking access to {}…", self.repo.trim()),
            Busy::Branches => "Loading branches…".into(),
            Busy::Files => "Looking for library files…".into(),
            Busy::Creating => format!("Creating {} and pushing to GitHub…", self.file_name.trim()),
            Busy::Replacing => format!(
                "Replacing {} with the library from this computer…",
                self.file_name()
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Repo(String),
    Token(String),
    PickBranch(usize),
    Choose(Choice),
    PickFile(usize),
    FileName(String),
    Back,
    /// The step's main action: Get started, Connect, Continue, Create
    /// library, Use this file.
    Continue,
    /// All set's Open Libri.
    Open,
    /// From the token warning and the sync row: the Connect step with the
    /// repository kept.
    ReplaceToken,
    /// "How do I create a token?"
    TokenHelp,
    /// "Which library?": a pick, Cancel, and "Use this one".
    PickWhich(Which),
    CloseWhich,
    ConfirmWhich,
    /// GitHub's answers: the repo (Connect), its branches, the library
    /// files on the branch, and the commit that created or replaced the
    /// file.
    Connected(Result<RepoInfo, Failure>),
    Branches(Result<Vec<BranchInfo>, Failure>),
    Files(Result<Vec<File>, Failure>),
    Created(Result<PutResult, Failure>),
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Setup(message)
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    let state = &mut app.setup;
    match message {
        Message::Repo(value) => state.repo = value,
        Message::Token(value) => state.token = value,
        Message::PickBranch(index) => state.branch = index,
        Message::Choose(choice) => state.choice = choice,
        Message::PickFile(index) => {
            if state.files.get(index).is_some_and(File::usable) {
                state.file = index;
            }
        }
        Message::FileName(value) => state.file_name = value,
        Message::Back => {
            state.busy = None;
            state.error = None;
            match state.replacing.take() {
                Some(screen) => app.screen = screen,
                None => state.step = state.step.back(),
            }
        }
        Message::Continue => {
            if !state.can_continue() {
                return Task::none();
            }
            state.error = None;
            match state.step {
                Step::Welcome => state.step = Step::Connect,
                Step::Connect if state.mock => match state.replacing.take() {
                    Some(screen) => {
                        app.sync = SyncState::Synced;
                        app.screen = screen;
                    }
                    None => state.step = Step::Branch,
                },
                // The token against the repo: a token that can read but
                // not write is refused here, not at the first save.
                Step::Connect => {
                    let client = Arc::new(Client::new(state.token.trim(), state.repo.trim()));
                    state.client = Some(ClientHandle(client.clone()));
                    state.busy = Some(Busy::Connecting);
                    return github_task(move || client.repo(), Message::Connected);
                }
                Step::Branch => state.step = Step::File,
                Step::File => match state.choice {
                    Choice::New => state.step = Step::Create,
                    Choice::Existing => {
                        state.step = Step::Existing;
                        if let (false, Some(client)) = (state.mock, state.client.clone()) {
                            state.files.clear();
                            state.busy = Some(Busy::Files);
                            let branch = state.branch_name();
                            return github_task(
                                move || library_files(&client.0, &branch),
                                Message::Files,
                            );
                        }
                    }
                },
                // A kept library meeting an existing file asks which to
                // keep, unless the file is the kept library's own; a new
                // file is made from the kept library, no question asked.
                Step::Existing
                    if app.kept_locally
                        && state.which.is_none()
                        && !is_kept_librarys_own(&state.target()) =>
                {
                    state.which = Some(Which::Here);
                }
                Step::Existing => {
                    state.loaded_entries = state.picked_file().and_then(|f| f.entries);
                    state.step = Step::Done;
                }
                Step::Create if state.mock => state.step = Step::Done,
                Step::Create => {
                    let Some(client) = state.client.clone() else {
                        return Task::none();
                    };
                    let kept = app.kept_locally.then(|| app.repo.library().clone());
                    state.loaded_entries = Some(kept.as_ref().map_or(0, |l| l.entries.len()));
                    let bytes = kept.map_or_else(
                        || codec::EMPTY.as_bytes().to_vec(),
                        |library| codec::encode(&library).into_bytes(),
                    );
                    let (file, branch) = (state.file_name.trim().to_owned(), state.branch_name());
                    state.busy = Some(Busy::Creating);
                    return github_task(
                        move || {
                            client
                                .0
                                .put_file(&file, &branch, &bytes, CREATE_MESSAGE, None)
                        },
                        Message::Created,
                    );
                }
                Step::Done => return update(app, Message::Open),
            }
        }
        Message::PickWhich(which) => state.which = Some(which),
        Message::CloseWhich => state.which = None,
        Message::ConfirmWhich => match (state.which, state.mock, state.client.clone()) {
            (None, ..) => {}
            // This computer's library goes over the file before connecting.
            (Some(Which::Here), false, Some(client)) => {
                let Some(file) = state.picked_file() else {
                    return Task::none();
                };
                let (path, sha, branch) =
                    (file.path.clone(), file.sha.clone(), state.branch_name());
                let bytes = codec::encode(app.repo.library()).into_bytes();
                state.loaded_entries = Some(app.repo.library().entries.len());
                state.busy = Some(Busy::Replacing);
                state.which = None;
                return github_task(
                    move || {
                        client
                            .0
                            .put_file(&path, &branch, &bytes, REPLACE_MESSAGE, Some(&sha))
                    },
                    Message::Created,
                );
            }
            (Some(which), ..) => {
                state.loaded_entries = match which {
                    Which::Here => Some(app.repo.library().entries.len()),
                    Which::GitHub => state.picked_file().and_then(|f| f.entries),
                };
                state.step = Step::Done;
            }
        },
        Message::Open if state.mock => {
            app.source = Some(Source::of(&state.target(), token_hint(&state.token)));
            app.kept_locally = false;
            app.screen = Screen::Library;
        }
        Message::Open => {
            let Some(client) = state.client.take() else {
                return Task::none();
            };
            let target = state.target();
            let token = state.token.trim().to_owned();
            let base = state.base.take();
            // A new empty file: the local library matches what was
            // created. A new file from the kept library, or a replaced
            // file: the local library is what was pushed. An existing
            // file without a base: GitHub's copy replaces it on the
            // first sync.
            if base.is_some() && !app.kept_locally && state.choice == Choice::New {
                app.repo.replace(Library::default());
            }
            crate::trace::line(format_args!("setup: connecting {}", target.key()));
            return app.connect(target, token, client.0, base);
        }
        Message::ReplaceToken => {
            let from = match app.screen {
                Screen::Setup => Screen::Library,
                screen => screen,
            };
            *state = if app.mock {
                State::at(Step::Connect, app.source.as_ref())
            } else {
                State::real(Step::Connect, app.connection.as_ref())
            };
            state.token.clear();
            state.replacing = Some(from);
            app.screen = Screen::Setup;
        }
        Message::TokenHelp => app::open_url(TOKEN_HELP_URL),
        Message::Connected(result) => {
            if state.busy != Some(Busy::Connecting) {
                return Task::none();
            }
            state.busy = None;
            let info = match result {
                Ok(info) => info,
                Err(failure) => {
                    state.error = Some(failure.describe());
                    return Task::none();
                }
            };
            if !info.can_push {
                state.error = Some(
                    "The token can read this repo but not write to it. It needs Contents: read and write."
                        .into(),
                );
                return Task::none();
            }
            state.default_branch = info.default_branch;
            let Some(client) = state.client.clone() else {
                return Task::none();
            };
            if let Some(screen) = state.replacing.take() {
                let token = state.token.trim().to_owned();
                app.screen = screen;
                return app.replace_token(token, client.0);
            }
            state.step = Step::Branch;
            state.branches.clear();
            state.busy = Some(Busy::Branches);
            return github_task(move || client.0.branches(), Message::Branches);
        }
        Message::Branches(result) => {
            if state.busy != Some(Busy::Branches) {
                return Task::none();
            }
            state.busy = None;
            match result {
                Ok(list) => state.branches = branch_rows(list, &state.default_branch),
                Err(failure) => {
                    state.error = Some(failure.describe());
                    state.branches = branch_rows(Vec::new(), &state.default_branch);
                }
            }
            state.branch = 0;
        }
        Message::Files(result) => {
            if state.busy != Some(Busy::Files) {
                return Task::none();
            }
            state.busy = None;
            match result {
                Ok(files) => {
                    state.file = files.iter().position(File::usable).unwrap_or(0);
                    state.files = files;
                }
                Err(failure) => {
                    state.error = Some(failure.describe());
                    state.files.clear();
                }
            }
        }
        Message::Created(result) => {
            let Some(busy) = state.busy.take() else {
                return Task::none();
            };
            match result {
                Ok(result) => {
                    let now = Timestamp::now();
                    state.base = Some(Base {
                        target: state.target().key(),
                        etag: Some(format!("\"{}\"", result.blob_sha)),
                        remote_sha: result.blob_sha.clone(),
                        canonical_sha: result.blob_sha,
                        commit_sha: Some(result.commit_sha),
                        last_synced: Some(now),
                        last_checked: Some(now),
                    });
                    state.step = Step::Done;
                }
                Err(failure) if busy == Busy::Creating && failure.kind == FailureKind::Conflict => {
                    state.error = Some("A file with that name already exists.".into());
                }
                Err(failure) => state.error = Some(failure.describe()),
            }
        }
    }
    Task::none()
}

/// Runs one GitHub call on tokio's blocking pool and brings the answer
/// back as a setup message.
fn github_task<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, Failure> + Send + 'static,
    wrap: impl Fn(Result<T, Failure>) -> Message + Send + 'static,
) -> Task<AppMessage> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(call)
                .await
                .unwrap_or_else(|error| Err(Failure::new(FailureKind::Other, error.to_string())))
        },
        move |result| msg(wrap(result)),
    )
}

/// The repo's branches as rows: the default first with its badge, the
/// rest A to Z, each with when its head was committed. A repo without
/// commits has no branches; the default branch is then where the first
/// commit goes.
fn branch_rows(list: Vec<BranchInfo>, default_branch: &str) -> Vec<Branch> {
    let now = Timestamp::now();
    let info = |b: &BranchInfo| {
        b.updated
            .map(|at| format!("Updated {}", at.ago(now)))
            .unwrap_or_default()
    };
    let mut rows: Vec<Branch> = list
        .iter()
        .filter(|b| b.name == default_branch)
        .map(|b| Branch {
            name: b.name.clone(),
            info: info(b),
            default: true,
        })
        .collect();
    let mut others: Vec<&BranchInfo> = list.iter().filter(|b| b.name != default_branch).collect();
    others.sort_by_key(|b| b.name.to_lowercase());
    rows.extend(others.into_iter().map(|b| Branch {
        name: b.name.clone(),
        info: info(b),
        default: false,
    }));
    if rows.is_empty() {
        rows.push(Branch {
            name: default_branch.to_owned(),
            info: "No commits yet".into(),
            default: true,
        });
    }
    rows
}

/// The JSON files on `branch`, each opened to tell a Libri library (with
/// its entry count and last commit) from other JSON. At most `MAX_FILES`
/// of at most `MAX_FILE_BYTES`.
fn library_files(client: &Client, branch: &str) -> Result<Vec<File>, Failure> {
    let now = Timestamp::now();
    let tree = client.files(branch, ".json")?;
    let mut files = Vec::new();
    for blob in tree
        .into_iter()
        .filter(|b| b.size <= MAX_FILE_BYTES)
        .take(MAX_FILES)
    {
        let raw = client.file_raw(&blob.path, branch, None)?;
        let entries = raw
            .as_ref()
            .and_then(|r| r.text().ok())
            .and_then(|text| codec::decode(text).ok())
            .map(|library| library.entries.len());
        let updated = match entries {
            Some(_) => client
                .commits(&blob.path, branch, 1)?
                .first()
                .map(|c| c.date),
            None => None,
        };
        let info = match entries {
            Some(n) => format!(
                "Libri library · {n} {}{}",
                if n == 1 { "entry" } else { "entries" },
                updated
                    .map(|at| format!(" · updated {}", at.ago(now)))
                    .unwrap_or_default()
            ),
            None => "Not a Libri library file".into(),
        };
        files.push(File {
            path: blob.path,
            info,
            sha: blob.sha,
            entries,
            updated,
        });
    }
    Ok(files)
}

/// Whether `target` is the file the library kept on this computer came
/// from: its base is in `sync.json`, so the sync compares both copies to
/// it and no question is asked.
fn is_kept_librarys_own(target: &Target) -> bool {
    crate::storage::data_dir()
        .map(|dir| BaseFile::at(dir.join("sync.json")))
        .is_some_and(|file| file.load(&target.key()).is_some())
}

/// "github_pat_••••••a91F": the token's prefix and last four characters.
pub fn token_hint(token: &str) -> String {
    let token = token.trim();
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let prefix = if token.starts_with("github_pat_") {
        "github_pat_"
    } else {
        ""
    };
    format!("{prefix}••••••{tail}")
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let state = &app.setup;
    let body = match state.step {
        Step::Welcome => welcome(&tokens),
        Step::Done => done(app, &tokens),
        _ => wizard(app, &tokens),
    };
    let chrome = tokens.chrome;
    let screen = container(body)
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(chrome).color(tokens.text));
    // "Which library?" asks over the file step.
    let which = state
        .which
        .filter(|_| state.step == Step::Existing)
        .map(|picked| which_dialog(app, picked, &tokens));
    ui::modal(screen, which, msg(Message::CloseWhich), &tokens)
}

/// "Which library?": this computer's kept library or the file on GitHub,
/// as radio rows with what each means; Cancel, or "Use this one".
fn which_dialog<'a>(app: &'a App, picked: Which, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.setup;
    let here = app.repo.library().entries.len();
    let file = state.file_name();
    let theirs = match state.picked_file() {
        Some(picked) if !state.mock => format!(
            "{} entries{}. Replaces the library on this computer.",
            thousands(picked.entries.unwrap_or(0)),
            picked
                .updated
                .map(|at| format!(" · last commit {}", at.month_day_time()))
                .unwrap_or_default()
        ),
        _ => {
            "1,279 entries · last commit Oct 13. Replaces the library on this computer.".to_owned()
        }
    };
    let options = [
        (
            Which::Here,
            "This computer's",
            format!(
                "{} entries. Uploaded over {file} on GitHub.",
                thousands(here)
            ),
        ),
        (Which::GitHub, "The one on GitHub", theirs),
    ];
    let rows = options.into_iter().map(|(which, title, sub)| {
        radio_row(
            text(title).size(14.5).font(font::SEMIBOLD).into(),
            sub,
            which == picked,
            true,
            tokens,
            msg(Message::PickWhich(which)),
        )
    });
    let body = column![
        text(format!(
            "This computer still has the library you kept when you disconnected, and {file} on GitHub has its own. Keep one; the other is replaced."
        ))
        .size(14)
        .line_height(1.45),
        Column::with_children(rows).spacing(6),
        text("Earlier versions stay in the repo's history either way.")
            .size(12.5)
            .color(tokens.sub),
    ]
    .spacing(14);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ui::ButtonKind::Outline,
            *tokens,
            Some(msg(Message::CloseWhich))
        ),
        ui::labeled_button(
            "Use this one",
            ui::ButtonKind::Primary,
            *tokens,
            Some(msg(Message::ConfirmWhich))
        ),
    ]
    .spacing(8);
    ui::dialog(
        "Which library?",
        body,
        footer,
        560,
        tokens,
        msg(Message::CloseWhich),
    )
}

/// "1,284".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Logo, "Welcome to Libri", a line about the app and Get started.
fn welcome<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    let block = column![
        ui::logo(112.0),
        container(text("Welcome to Libri").size(34).font(font::EXTRABOLD))
            .padding(Padding::new(0.0).top(10)),
        text("Keep track of your movies, shows, books, games, music and anime, and plan what to enjoy next.")
            .size(16)
            .line_height(1.55)
            .color(tokens.sub)
            .align_x(Horizontal::Center),
        container(big_button("Get started", tokens, Some(msg(Message::Continue))))
            .padding(Padding::new(0.0).top(14)),
        text("Your library is saved in a GitHub repo you own.")
            .size(12.5)
            .color(tokens.sub),
    ]
    .spacing(18)
    .width(560)
    .align_x(Center);
    container(block)
        .center(Fill)
        .padding(Padding::new(0.0).bottom(38))
        .into()
}

/// The steps rail and the current step's form.
fn wizard<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.setup;
    let current = state.step.index().unwrap_or(0);
    let values = [
        state.repo.trim().to_owned(),
        state.branch_name(),
        match state.choice {
            Choice::New => "New library".to_owned(),
            Choice::Existing => "Existing file".to_owned(),
        },
        state.file_name(),
    ];
    let labels = [
        "Connect GitHub",
        "Branch",
        "Library file",
        match state.choice {
            Choice::New => "Create library",
            Choice::Existing => "Pick file",
        },
    ];
    let mut rail = Column::new().spacing(4).push(
        container(
            row![
                ui::logo(28.0),
                text("Set up Libri").size(15).font(font::BOLD)
            ]
            .spacing(10)
            .align_y(Center),
        )
        .padding(Padding::new(0.0).left(10).right(10).bottom(22)),
    );
    for (index, label) in labels.into_iter().enumerate() {
        rail = rail.push(step_row(
            index,
            label,
            &values[index],
            index == current,
            index < current,
            tokens,
        ));
    }
    rail = rail.push(space::vertical()).push(
        container(
            text("You can change all of this later in Settings → Library source.")
                .size(12.5)
                .line_height(1.5)
                .color(tokens.sub),
        )
        .padding([0, 10]),
    );
    let rail = container(rail).width(280).height(Fill).padding([18, 14]);

    let form = match state.step {
        Step::Connect => connect(state, tokens),
        Step::Branch => branch(state, tokens),
        Step::File => file(state, tokens),
        Step::Create => create(state, tokens),
        _ => existing(state, tokens),
    };
    let mut form = column![
        ui::section_header(&format!("Step {} of 4", current + 1), tokens),
        form
    ]
    .spacing(22)
    .max_width(680);
    if let Some(line) = state.busy_line() {
        form = form.push(busy_row(line, tokens));
    }
    if let Some(error) = &state.error {
        form = form.push(
            text(error.clone())
                .size(13.5)
                .line_height(1.5)
                .color(tokens.danger),
        );
    }
    let scroll = ui::scroll(
        container(form)
            .width(Fill)
            .align_x(Center)
            .padding(Padding::new(40.0).top(64).bottom(20)),
    )
    .height(Fill);

    let back = button(
        row![
            ui::icon(Icon::ChevronLeft, 15.0, tokens.text),
            text("Back").size(14)
        ]
        .spacing(6)
        .align_y(Center),
    )
    .height(40)
    .padding([0, 16])
    .on_press(msg(Message::Back))
    .style(action_style(ButtonKind::Outline, tokens))
    .focusable(9.0, tokens);
    let primary = button(text(state.primary_label()).size(14).font(font::BOLD))
        .height(40)
        .padding([0, 22])
        .on_press_maybe(state.can_continue().then_some(msg(Message::Continue)))
        .style(action_style(ButtonKind::Primary, tokens))
        .focusable(9.0, tokens);
    let footer = container(
        container(
            row![back, space::horizontal(), primary]
                .align_y(Center)
                .width(Fill),
        )
        .max_width(680),
    )
    .width(Fill)
    .align_x(Center)
    .padding(Padding::new(40.0).top(16).bottom(28));

    let content = tokens.content;
    let pane = container(column![scroll, footer])
        .width(Fill)
        .height(Fill)
        .style(move |_| {
            container::background(content).border(Border {
                radius: Radius::default().top_left(12),
                ..Border::default()
            })
        });
    row![rail, pane].into()
}

/// One step on the rail: a numbered disc (a check once done), the label
/// and its value.
fn step_row<'a>(
    index: usize,
    label: &'a str,
    value: &str,
    current: bool,
    done: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let ring = if done || current {
        tokens.accent
    } else {
        tokens.line
    };
    let mark: Element<'a, AppMessage> = if done {
        ui::icon(Icon::Check, 12.0, tokens.on_accent)
    } else {
        text((index + 1).to_string())
            .size(12)
            .font(font::BOLD)
            .color(if current { tokens.text } else { tokens.sub })
            .into()
    };
    let disc = container(mark).center(24).style(move |_| container::Style {
        background: done.then_some(Background::Color(tokens.accent)),
        border: Border::default().rounded(12).width(2).color(ring),
        ..container::Style::default()
    });
    let (color, weight) = match (current, done) {
        (true, _) => (tokens.text, font::BOLD),
        (false, true) => (tokens.text, font::MEDIUM),
        (false, false) => (tokens.sub, font::MEDIUM),
    };
    let value = match (current, done) {
        (true, _) => "In progress".to_owned(),
        (false, true) => value.to_owned(),
        (false, false) => String::new(),
    };
    let words = column![
        text(label).size(14).font(weight).color(color),
        text(value).size(12).color(tokens.sub),
    ]
    .spacing(1);
    container(row![disc, words].spacing(12).align_y(Center))
        .width(Fill)
        .height(44)
        .padding([0, 10])
        .style(move |_| container::Style {
            background: current.then_some(Background::Color(tokens.selection)),
            border: Border::default().rounded(9),
            ..container::Style::default()
        })
        .into()
}

fn connect<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let ink = tokens.ink;
    let help = text("A fine-grained token that can read and write this one repo. It's stored in KDE Wallet and never leaves this computer.")
        .size(12.5)
        .line_height(1.5)
        .color(tokens.sub);
    let link = button(
        text("How do I create a token?")
            .size(13.5)
            .font(font::SEMIBOLD),
    )
    .padding(0)
    .on_press(msg(Message::TokenHelp))
    .style(move |_, _| button::Style {
        text_color: ink,
        ..button::Style::default()
    })
    .focusable(4.0, tokens);
    column![
        intro(
            "Connect GitHub",
            "Libri reads and saves your library in a GitHub repo you own. A private repo is fine.",
            tokens
        ),
        field(
            "Repository",
            ui::text_field(
                "owner/repo",
                &state.repo,
                |v| msg(Message::Repo(v)),
                font::MONO,
                tokens
            ),
            tokens
        ),
        column![
            field(
                "Access token",
                ui::text_field(
                    "",
                    &state.token,
                    |v| msg(Message::Token(v)),
                    font::MONO,
                    tokens
                )
                .secure(true),
                tokens
            ),
            help
        ]
        .spacing(7),
        link,
    ]
    .spacing(22)
    .into()
}

fn branch<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let lead = rich_text([
        span("Branches in "),
        mono_span(&state.repo, tokens),
        span(". Libri reads and saves your library on the one you pick."),
    ])
    .size(14.5)
    .line_height(1.55)
    .color(tokens.sub);
    let rows = state.branches.iter().enumerate().map(|(index, branch)| {
        let on = index == state.branch;
        let mut title = row![ui::mono(branch.name.as_str(), 14.0, tokens.text)]
            .spacing(8)
            .align_y(Center);
        if branch.default {
            let selection = tokens.selection;
            title = title.push(
                container(text("Default").size(11.5).font(font::SEMIBOLD))
                    .height(20)
                    .padding([0, 8])
                    .align_y(Center)
                    .style(move |_| {
                        container::background(selection).border(Border::default().rounded(10))
                    }),
            );
        }
        radio_row(
            title.into(),
            branch.info.as_str(),
            on,
            true,
            tokens,
            msg(Message::PickBranch(index)),
        )
    });
    column![
        column![heading("Pick a branch", tokens), lead].spacing(8),
        Column::with_children(rows).spacing(4),
        note(
            "Libri commits straight to this branch, with no pull requests.",
            tokens
        ),
    ]
    .spacing(18)
    .into()
}

fn file<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let lead = rich_text([
        span("Libri keeps everything in one JSON file in "),
        mono_span(&state.repo, tokens),
        span(" on "),
        mono_span(&state.branch_name(), tokens),
        span(". Start fresh, or use a file you already have."),
    ])
    .size(14.5)
    .line_height(1.55)
    .color(tokens.sub);
    let cards = [
        (
            Choice::New,
            "Create a new library",
            "Libri makes an empty library file in your repo. Pick this if you're starting out.",
        ),
        (
            Choice::Existing,
            "Use a file I already have",
            "Pick a Libri library file that's already in the repo.",
        ),
    ]
    .map(|(choice, title, body)| {
        let on = state.choice == choice;
        let tokens = *tokens;
        button(
            column![
                row![
                    radio(on, true, &tokens),
                    text(title).size(15).font(font::BOLD)
                ]
                .spacing(10)
                .align_y(Center),
                text(body).size(12.5).line_height(1.5).color(tokens.sub),
            ]
            .spacing(10),
        )
        .width(Fill)
        .padding(18)
        .on_press(msg(Message::Choose(choice)))
        .style(move |_, _| button::Style {
            background: on.then_some(Background::Color(tokens.selection)),
            text_color: tokens.text,
            border: Border::default().rounded(12).width(2).color(if on {
                tokens.accent
            } else {
                tokens.line
            }),
            ..button::Style::default()
        })
        .focusable(12.0, &tokens)
    });
    column![
        column![heading("Your library file", tokens), lead].spacing(8),
        iced::widget::Row::with_children(cards).spacing(14),
    ]
    .spacing(18)
    .into()
}

fn create<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let (chrome, line) = (tokens.chrome, tokens.line);
    let preview = container(
        text(codec::EMPTY.trim_end())
            .size(13)
            .font(font::MONO)
            .line_height(1.55),
    )
    .width(Fill)
    .padding([14, 16])
    .style(move |_| {
        container::background(chrome).border(Border::default().rounded(10).width(1).color(line))
    });
    column![
        intro(
            "Create a new library",
            "Libri adds an empty library file to your repo and commits it for you.",
            tokens
        ),
        column![
            field(
                "File name",
                container(ui::text_field(
                    "library.json",
                    &state.file_name,
                    |v| msg(Message::FileName(v)),
                    font::MONO,
                    tokens
                ))
                .max_width(360),
                tokens
            ),
            note(
                &format!(
                    "Saved at the top of {} on {}. Libri never overwrites a file that already exists.",
                    state.repo.trim(),
                    state.branch_name()
                ),
                tokens
            ),
        ]
        .spacing(7),
        column![
            label("What gets added", tokens),
            preview,
            note("Commit message: “Create Libri library”", tokens),
        ]
        .spacing(7),
    ]
    .spacing(18)
    .into()
}

fn existing<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let rows = state.files.iter().enumerate().map(|(index, file)| {
        let usable = file.usable();
        let on = index == state.file && usable;
        let color = if usable { tokens.text } else { tokens.sub };
        radio_row(
            ui::mono(file.path.as_str(), 14.0, color).into(),
            file.info.as_str(),
            on,
            usable,
            tokens,
            msg(Message::PickFile(index)),
        )
    });
    column![
        intro(
            "Pick your file",
            &format!(
                "JSON files found in {} on {}. Only files in Libri's format can be used.",
                state.repo.trim(),
                state.branch_name()
            ),
            tokens
        ),
        Column::with_children(rows).spacing(4),
        note(
            "Don't see your file? Make sure it's in this repo and ends in .json.",
            tokens
        ),
    ]
    .spacing(18)
    .into()
}

/// "You're all set": a check, the summary and Open Libri.
fn done<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.setup;
    let kept = app.kept_locally.then(|| app.repo.library().entries.len());
    let tokens = *tokens;
    let disc = container(ui::icon(Icon::Check, 36.0, tokens.success))
        .center(72)
        .style(move |_| {
            container::background(tokens.selection).border(Border::default().rounded(36))
        });
    let words = match (state.choice, kept, state.which) {
        (Choice::New, Some(n), _) => format!(
            "{} is now in {} on {}, made from the {} entries kept on this computer.",
            state.file_name(),
            state.repo.trim(),
            state.branch_name(),
            thousands(n)
        ),
        (Choice::New, None, _) => format!(
            "{} is now in {} on {}. Add your first entry with the Add button.",
            state.file_name(),
            state.repo.trim(),
            state.branch_name()
        ),
        (Choice::Existing, Some(n), Some(Which::Here)) => format!(
            "Replaced {} with the {} entries kept on this computer.",
            state.file_name(),
            thousands(n)
        ),
        (Choice::Existing, _, _) => format!(
            "Loaded {} entries from {}.",
            thousands(state.loaded_entries.unwrap_or(214)),
            state.file_name()
        ),
    };
    let summary = |label: &'static str, value: String| {
        row![
            container(ui::mono(label, 13.0, tokens.sub)).width(62),
            ui::mono(value, 13.0, tokens.text)
        ]
    };
    let card = container(
        column![
            summary("repo", state.repo.trim().to_owned()),
            summary("branch", state.branch_name()),
            summary("file", state.file_name()),
        ]
        .spacing(6),
    )
    .padding([14, 18])
    .style(move |_| {
        let border = if tokens.light {
            Border::default().rounded(12).width(1).color(tokens.line)
        } else {
            Border::default().rounded(12)
        };
        container::background(tokens.content).border(border)
    });
    let block = column![
        disc,
        container(text("You're all set").size(32).font(font::EXTRABOLD))
            .padding(Padding::new(0.0).top(6)),
        text(words)
            .size(15.5)
            .line_height(1.55)
            .color(tokens.sub)
            .align_x(Horizontal::Center),
        card,
        container(big_button("Open Libri", &tokens, Some(msg(Message::Open))))
            .padding(Padding::new(0.0).top(10)),
    ]
    .spacing(16)
    .width(560)
    .align_x(Center);
    container(block)
        .center(Fill)
        .padding(Padding::new(0.0).bottom(38))
        .into()
}

/// A step's title and its lead paragraph.
fn intro<'a>(title: &'a str, lead: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    column![
        heading(title, tokens),
        text(lead.to_owned())
            .size(14.5)
            .line_height(1.55)
            .color(tokens.sub)
    ]
    .spacing(8)
    .into()
}

fn heading<'a>(title: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(title)
        .size(26)
        .font(font::EXTRABOLD)
        .color(tokens.text)
        .into()
}

/// A field with its label in the text color, as the wizard's forms have.
fn field<'a>(
    name: &'a str,
    control: impl Into<Element<'a, AppMessage>>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![label(name, tokens), control.into()]
        .spacing(7)
        .into()
}

fn label<'a>(name: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(name)
        .size(13)
        .font(font::SEMIBOLD)
        .color(tokens.text)
        .into()
}

/// A ring in the accent and the words, while GitHub is asked: the
/// mockup's "Checking access to ekmanch/library…" (its ring spins; this
/// one stands still, iced has no animation for a border).
fn busy_row<'a>(words: String, tokens: &Tokens) -> Element<'a, AppMessage> {
    let accent = tokens.accent;
    let ring = container(space()).width(14).height(14).style(move |_| {
        container::Style::default().border(Border::default().rounded(7).width(2).color(accent))
    });
    row![ring, text(words).size(13.5).color(tokens.sub)]
        .spacing(10)
        .align_y(Center)
        .into()
}

fn note<'a>(words: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(words.to_owned())
        .size(12.5)
        .line_height(1.5)
        .color(tokens.sub)
        .into()
}

fn mono_span<'a>(words: &str, tokens: &Tokens) -> iced::widget::text::Span<'a, ()> {
    span(words.to_owned()).font(font::MONO).color(tokens.text)
}

/// A pickable row: a radio mark, a title and a line of detail. A row that
/// cannot be picked is dimmed.
fn radio_row<'a>(
    title: Element<'a, AppMessage>,
    info: impl text::IntoFragment<'a>,
    on: bool,
    enabled: bool,
    tokens: &Tokens,
    on_press: AppMessage,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let words = column![title, text(info).size(12.5).color(tokens.sub)].spacing(2);
    button(
        row![radio(on, enabled, &tokens), words]
            .spacing(12)
            .align_y(Center),
    )
    .width(Fill)
    .height(54)
    .padding([8, 14])
    .on_press_maybe(enabled.then_some(on_press))
    .style(move |_, _| button::Style {
        background: on.then_some(Background::Color(tokens.selection)),
        text_color: tokens.text,
        border: Border::default().rounded(10),
        ..button::Style::default()
    })
    .focusable(10.0, &tokens)
}

/// An 18 px radio mark: the accent ring and dot when on.
fn radio<'a>(on: bool, enabled: bool, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let ring = match (on, enabled) {
        (true, _) => tokens.accent,
        (false, true) => tokens.sub,
        (false, false) => tokens.line,
    };
    let dot = container(space())
        .width(8)
        .height(8)
        .style(move |_| container::Style {
            background: on.then_some(Background::Color(tokens.accent)),
            border: Border::default().rounded(4),
            ..container::Style::default()
        });
    container(dot)
        .center(18)
        .style(move |_| {
            container::Style::default().border(Border::default().rounded(9).width(2).color(ring))
        })
        .into()
}

/// Get started and Open Libri: 44 px, bold, a 10 px radius.
fn big_button<'a>(
    words: &'a str,
    tokens: &Tokens,
    on_press: Option<AppMessage>,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(
        text(words)
            .size(15)
            .font(font::BOLD)
            .height(Fill)
            .align_y(Vertical::Center),
    )
    .height(44)
    .padding([0, 26])
    .on_press_maybe(on_press)
    .style(move |_, status| {
        let mut style = ui::button_style(ButtonKind::Primary, &tokens, status);
        style.border.radius = 10.0.into();
        style
    })
    .focusable(10.0, &tokens)
}

/// Back and the main action at the bottom: 40 px with a 9 px radius.
fn action_style(
    kind: ButtonKind,
    tokens: &Tokens,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let tokens = *tokens;
    move |_, status| {
        let mut style = ui::button_style(kind, &tokens, status);
        style.border.radius = 9.0.into();
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_at(step: Step) -> App {
        let mut app = App::new(Screen::Setup);
        app.setup = State::at(step, app.source.as_ref());
        app
    }

    #[test]
    fn the_steps_follow_the_choice_of_file() {
        let mut app = app_at(Step::Welcome);
        app.source = None;
        app.setup = State::at(Step::Welcome, None);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Connect);
        // Connect needs a repository and a token.
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Connect);
        let _ = update(&mut app, Message::Repo("ekmanch/library".into()));
        let _ = update(&mut app, Message::Token("github_pat_x".into()));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Branch);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::File);
        let _ = update(&mut app, Message::Choose(Choice::Existing));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Existing);
        let _ = update(&mut app, Message::Back);
        let _ = update(&mut app, Message::Choose(Choice::New));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Create);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Done);
        let _ = update(&mut app, Message::Open);
        assert_eq!(app.screen, Screen::Library);
        let source = app.source.unwrap();
        assert_eq!(source.repo, "ekmanch/library");
        assert_eq!(source.branch, "main");
        assert_eq!(source.file, "library.json");
        assert_eq!(source.token_hint, "github_pat_••••••at_x");
    }

    #[test]
    fn a_file_that_is_not_a_library_cannot_be_picked() {
        let mut app = app_at(Step::Existing);
        let _ = update(&mut app, Message::PickFile(2));
        assert_eq!(app.setup.file, 0);
        let _ = update(&mut app, Message::PickFile(1));
        assert_eq!(app.setup.file, 1);
    }

    #[test]
    fn a_new_file_needs_a_json_name() {
        let mut app = app_at(Step::Create);
        let _ = update(&mut app, Message::FileName("notes".into()));
        assert!(!app.setup.can_continue());
        let _ = update(&mut app, Message::FileName("lists.json".into()));
        assert!(app.setup.can_continue());
    }

    #[test]
    fn replacing_the_token_returns_to_the_screen_it_came_from() {
        let mut app = App::new(Screen::Plans);
        app.sync = SyncState::TokenExpired;
        let _ = update(&mut app, Message::ReplaceToken);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.replacing, Some(Screen::Plans));
        assert!(app.setup.token.is_empty());
        let _ = update(&mut app, Message::Token("github_pat_new".into()));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.screen, Screen::Plans);
        assert_eq!(app.sync, SyncState::Synced);
        assert_eq!(app.setup.replacing, None);
    }

    #[test]
    fn the_window_is_titled_only_on_the_numbered_steps() {
        assert!(!State::at(Step::Welcome, None).in_wizard());
        assert!(State::at(Step::Create, None).in_wizard());
        assert!(!State::at(Step::Done, None).in_wizard());
    }

    #[test]
    fn a_kept_library_meeting_an_existing_file_asks_which_to_keep() {
        let mut app = App::new(Screen::Setup);
        app.source = None;
        app.kept_locally = true;
        app.setup = State::at(Step::File, None);
        app.setup.repo = "ekmanch/library".into();
        let _ = update(&mut app, Message::Choose(Choice::Existing));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Existing);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.which, Some(Which::Here), "asked, not done");
        assert_eq!(app.setup.step, Step::Existing);
        let _ = update(&mut app, Message::CloseWhich);
        assert!(app.setup.which.is_none(), "Cancel goes back to the file");
        let _ = update(&mut app, Message::Continue);
        let _ = update(&mut app, Message::PickWhich(Which::GitHub));
        let _ = update(&mut app, Message::ConfirmWhich);
        assert_eq!(app.setup.step, Step::Done);
        let _ = update(&mut app, Message::Open);
        assert!(!app.kept_locally && app.source.is_some());

        // A new file asks nothing: it is made from the kept library.
        let mut app = App::new(Screen::Setup);
        app.kept_locally = true;
        app.setup = State::at(Step::Create, None);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Done);
    }
}
