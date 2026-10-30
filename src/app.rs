//! Application state, messages, `update` and `view`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::theme::Mode as ThemeMode;
use iced::widget::{operation, scrollable};
use iced::window::{self, settings::PlatformSpecific};
use iced::{Element, Event, Size, Subscription, Task, event, mouse, time};

use crate::clock::Timestamp;
use crate::connection::{Connection, ConnectionFile, Target};
use crate::decoration::Decoration;
use crate::github;
use crate::history::{Direction, History, Place};
use crate::lookup;
use crate::model::{Date, codec};
use crate::preferences::{Preferences, PreferencesFile};
use crate::repo::Operation;
use crate::repo::{FixtureLibraryRepository, LibraryRepository, LocalLibraryRepository};
use crate::screens::{self, Screen, entry, library, plans, setup, token};
use crate::settings::{Settings, Source, SyncState, ThemeChoice};
use crate::storage;
use crate::sync::{self, Answer, Base, Engine, Kind, Outcome, Phase, Request};
use crate::tags;
use crate::theme::{Accent, Theme, Tokens, font};
use crate::wallet;

/// From how many days before its expiry the token warning shows.
const TOKEN_WARNING_DAYS: i64 = 7;

/// The default window size, and the minimum: 1080p at KDE's 150 %
/// scaling, the smallest screen Libri is laid out for (docs/DESIGN.md
/// section 10, phase 1g).
pub const WINDOW_SIZE: Size = Size::new(1600.0, 900.0);
pub const MIN_WINDOW_SIZE: Size = Size::new(1280.0, 720.0);

pub fn run() -> iced::Result {
    present_with_mailbox();
    let window = window::Settings {
        size: WINDOW_SIZE,
        min_size: Some(MIN_WINDOW_SIZE),
        // KDE groups the window and finds its icon by the app id.
        platform_specific: PlatformSpecific {
            application_id: "libri".into(),
            ..PlatformSpecific::default()
        },
        // Glass paints see-through surfaces over the blurred desktop; the
        // other themes paint every pixel, so they lose nothing by it.
        transparent: true,
        ..window::Settings::default()
    };
    freeze_log();
    let result = iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::iced_theme)
        .subscription(App::subscription)
        .settings(settings())
        .window(window)
        .run();
    crate::trace::line("exit");
    result
}

/// Mailbox presentation instead of iced's FIFO default, unless the
/// environment says otherwise (docs/DESIGN.md, "Presentation mode").
/// With FIFO, NVIDIA's Vulkan driver attaches a commit-timing target to
/// every frame, and after the window has been hidden it extrapolates a
/// target as far in the future as the window was hidden; KWin 6.8 holds
/// the frame until then and the whole desktop stands still for that long.
/// With mailbox the driver sends no timestamps at all. iced reads the
/// variable when it builds the renderer, so it is set before the runtime
/// starts, while this is still the only thread.
fn present_with_mailbox() {
    if std::env::var_os("ICED_PRESENT_MODE").is_some() || std::env::var_os("WGPU_BACKEND").is_some()
    {
        return;
    }
    // SAFETY: `run` is called first thing from `main`, before any other
    // thread exists, so nothing can read the environment concurrently.
    unsafe { std::env::set_var("ICED_PRESENT_MODE", "mailbox") };
}

/// The freeze log of this run (TODO.md 1g, the desktop freeze):
/// `~/.cache/libri/freeze-<date>.log`, which the trace (`crate::trace`)
/// and `scripts/freeze-monitor.sh` write together, so every session is
/// logged without anyone having to remember to. The monitor follows this
/// process and keeps sampling KWin for 90 s after it exits, in its own
/// process group so that a Ctrl+C in the terminal does not end it with
/// Libri: the 2026-10-24 21:42 freeze began the moment Libri's window
/// was closed. `LIBRI_NO_MONITOR=1` skips both; an installed binary has
/// no script to start and only writes the trace.
fn freeze_log() {
    use std::os::unix::process::CommandExt;

    if std::env::var_os("LIBRI_NO_MONITOR").is_some() {
        return;
    }
    let Some(dir) = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })
        .map(|cache| cache.join("libri"))
    else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "libri: no freeze log, could not create {}: {error}",
            dir.display()
        );
        return;
    }
    let log = dir.join(format!("freeze-{}.log", date_stamp()));
    if let Err(error) = crate::trace::start(&log) {
        eprintln!(
            "libri: no freeze log, could not open {}: {error}",
            log.display()
        );
        return;
    }
    eprintln!("libri: freeze log {}", log.display());
    crate::trace::line(format_args!(
        "start pid {} build {}",
        std::process::id(),
        option_env!("LIBRI_BUILD").unwrap_or("unknown")
    ));
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/freeze-monitor.sh");
    if !script.is_file() {
        return;
    }
    if let Err(error) = std::process::Command::new(&script)
        .arg(&log)
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .process_group(0)
        .spawn()
    {
        eprintln!("libri: could not start the freeze monitor: {error}");
    }
}

/// `20261024-212503`, from `date`, or the seconds since the epoch when
/// that fails.
fn date_stamp() -> String {
    std::process::Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|stamp| !stamp.is_empty())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default()
        })
}

/// The bundled fonts with Figtree as the default. Shared with the headless
/// screenshots so they render with the real fonts.
pub fn settings() -> iced::Settings {
    iced::Settings {
        fonts: font::FILES.map(Into::into).into(),
        default_font: font::REGULAR,
        ..iced::Settings::default()
    }
}

/// Opens `url` in the desktop's browser.
pub fn open_url(url: &str) {
    if let Err(error) = std::process::Command::new("xdg-open").arg(url).spawn() {
        eprintln!("libri: could not open {url}: {error}");
    }
}

pub struct App {
    pub screen: Screen,
    pub settings: Settings,
    /// Plasma's color scheme, for the System theme: the desktop portal's
    /// answer, through iced (`Message::SystemTheme`).
    pub plasma_dark: bool,
    /// `LIBRI_PLASMA` set: the scheme is faked and the portal's answers
    /// are ignored.
    pub plasma_fixed: bool,
    /// The connected repository, or `None` on a first run.
    pub source: Option<Source>,
    /// Disconnected with the library kept on this computer: browsing and
    /// editing go on without a sync until a repo is connected again.
    pub kept_locally: bool,
    pub sync: SyncState,
    /// "Keep which copy?", over everything.
    pub conflict_open: bool,
    /// The token expiry warning over the window (docs/DESIGN.md 4.6).
    pub token_warning: bool,
    /// Today, for Coming soon, Recently added and the plan card.
    pub today: Date,
    /// The keyboard modifiers held now, for Ctrl+click.
    pub modifiers: keyboard::Modifiers,
    pub repo: Box<dyn LibraryRepository>,
    pub library: library::State,
    /// The entry dialog open over the window: Add or Edit, Delete, Add to
    /// plan.
    pub dialog: Option<entry::Dialog>,
    pub plans: plans::State,
    /// The plan dialog open over the window: New or Edit plan, Delete
    /// plan, Remove from plan.
    pub plan_dialog: Option<plans::Dialog>,
    pub setup: setup::State,
    pub settings_page: screens::settings::State,
    /// Where the splash's bar is in its sweep, 0 to 1.
    pub splash_phase: f32,
    /// The window's logical size, for layouts that give way when narrow.
    pub window: Size,
    /// The redraw counter of the freeze trace.
    pub frames: crate::trace::Frames,
    /// The places visited, for Back and Forward.
    pub history: History,
    /// KWin's title bar, once the window is open on Plasma Wayland.
    pub decoration: Option<Box<Decoration>>,
    /// What the online lookup is doing, faked until phase 4 asks the real
    /// providers (`LIBRI_LOOKUP`).
    pub lookup: lookup::LookupState,
    /// `preferences.json` on this computer; kept nowhere in the tests.
    pub preferences: PreferencesFile,
    /// `connection.json` on this computer and what it holds: the repo,
    /// branch and file, local mode and the token's expiry (phase 3).
    pub connection_file: ConnectionFile,
    pub connection: Option<Connection>,
    /// The GitHub token from KDE Wallet, in memory for this run.
    pub token: Option<String>,
    pub github: Option<Arc<github::Client>>,
    /// The sync engine while a repository is connected.
    pub engine: Option<Engine>,
    /// When the gathered edits go out as one commit.
    pub save_due: Option<Instant>,
    /// The splash while the first download of a fresh connection runs.
    pub loading: bool,
    /// Fake sync states and fixture lists in place of GitHub: every test,
    /// and a run with a screen switch (`LIBRI_SCREEN`, `LIBRI_SYNC`,
    /// `LIBRI_SETUP`, `LIBRI_FIRST_RUN`, `LIBRI_LOCAL`).
    pub mock: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Show(Screen),
    SetTheme(ThemeChoice),
    SetAccent(Accent),
    /// Plasma's color scheme from the desktop portal, through iced: once
    /// at the start, then on every change.
    SystemTheme(ThemeMode),
    Key(KeyPress),
    Modifiers(keyboard::Modifiers),
    /// The mouse's Back or Forward button, or Alt+Left and Alt+Right.
    History(Direction),
    /// A frame of the splash's bar.
    Tick,
    Resized(Size),
    /// The window's events, for the freeze trace.
    Window(window::Event),
    /// The hold on KWin's title bar, once the window is open.
    Decoration(Option<Box<Decoration>>),
    /// The sync row's and the banner's actions, and the conflict dialog.
    Sync(SyncMessage),
    Library(library::Message),
    Entry(entry::Message),
    Plans(plans::Message),
    Setup(setup::Message),
    Settings(screens::settings::Message),
    Token(token::Message),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMessage {
    /// "Could not save": try the save again.
    Retry,
    /// "Library not found": Settings › Library source.
    Fix,
    OpenConflict,
    CloseConflict,
    /// This computer's copy replaces GitHub's.
    KeepMine,
    /// GitHub's copy replaces this computer's and the queue is dropped.
    KeepTheirs,
    /// Sync now, and the check every five minutes.
    Now,
    /// The debounce clock: gathered edits go out once it is due.
    Tick,
    /// The token from KDE Wallet at the start: none means the first run.
    TokenLoaded(Result<Option<String>, String>),
    /// The wallet was written or cleared.
    WalletDone(Result<(), String>),
    /// A GitHub call came back.
    Answer(Box<Answer>),
}

impl Message {
    /// Whether the message takes the details pane away from the note being
    /// typed there, so the note is saved first: another screen, a step
    /// back or forward, anything in the library or on a plan page but the
    /// popovers and the list's own measurements, the selection keys, and
    /// the dialogs from the pane.
    fn leaves_note(&self) -> bool {
        use entry::Message as E;
        use library::Message as L;
        match self {
            Message::Show(_) | Message::History(_) | Message::Plans(_) => true,
            Message::Library(message) => !matches!(
                message,
                L::Scrolled(_)
                    | L::GroupsMeasured(_)
                    | L::Toggle(_)
                    | L::ClosePopup
                    | L::SeasonMenu(_)
                    | L::Picker(_)
                    | L::Search(_)
                    | L::JumpToGroup(_)
            ),
            Message::Key(key) => matches!(
                key,
                KeyPress::Up | KeyPress::Down | KeyPress::Edit | KeyPress::New | KeyPress::Search
            ),
            Message::Entry(message) => matches!(
                message,
                E::Edit(_)
                    | E::EditTags(_)
                    | E::Delete(_)
                    | E::AddToPlan(_)
                    | E::Add
                    | E::AddWithTitle(..)
            ),
            _ => false,
        }
    }

    /// Whether the message moves to another place the history remembers:
    /// a screen, what the library shows, an artist page or a plan page.
    fn navigates(&self) -> bool {
        use library::Message as L;
        use plans::Message as P;
        match self {
            Message::Show(_) => true,
            Message::Library(message) => matches!(
                message,
                L::ShowType(_)
                    | L::ToggleType(_)
                    | L::SeeAll(_)
                    | L::PickTag(_)
                    | L::PickFromAllTags(..)
                    | L::OpenPage(_)
                    | L::ClosePage
                    | L::MusicTab(_)
                    | L::OpenArtist(_)
                    | L::CloseArtist
            ),
            Message::Plans(message) => matches!(
                message,
                P::ShowOverview | P::ShowPlan(_) | P::ShowCollection(_)
            ),
            _ => false,
        }
    }
}

/// The keys the app handles itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPress {
    /// Closes the topmost popover or dialog.
    Escape,
    /// Confirms the open dialog.
    Enter,
    /// Moves focus; `back` with Shift.
    Tab {
        back: bool,
    },
    /// Ctrl+K or Ctrl+F: focuses the library's search.
    Search,
    /// Ctrl+Enter: saves the entry form.
    Save,
    /// Ctrl+N: opens Add entry.
    New,
    /// The arrow keys: the entry form's lookup, or the selection in the
    /// library's list and the plan's checklist.
    Up,
    Down,
    /// E: edits the selected entry.
    Edit,
}

impl App {
    /// Starts on the screen named by `LIBRI_SCREEN` (development only),
    /// or the library, with the library and the preferences from this
    /// computer's files. `LIBRI_FIRST_RUN` starts with no repository,
    /// `LIBRI_SYNC` fakes the sync state ("expired", "expires-in-6"),
    /// `LIBRI_PLASMA=light` a light Plasma, `LIBRI_THEME` and `LIBRI_FLAT`
    /// the theme and style (a faked look is never saved as the
    /// preference), and `LIBRI_SETUP` picks the setup step (welcome,
    /// connect, branch, file, create, existing, done).
    fn boot() -> (App, Task<Message>) {
        let var = |name| std::env::var(name).ok();
        let screen = var("LIBRI_SCREEN")
            .and_then(|name| Screen::from_name(&name))
            .unwrap_or_default();
        let mut app = App::new(screen);
        // A run that looks at a screen or a state keeps the fixture and
        // the fake states; otherwise the connection on this computer is
        // real.
        app.mock = [
            "LIBRI_SCREEN",
            "LIBRI_SYNC",
            "LIBRI_SETUP",
            "LIBRI_FIRST_RUN",
            "LIBRI_LOCAL",
        ]
        .iter()
        .any(|name| var(name).is_some());
        let token = app.open_files();
        if var("LIBRI_FIRST_RUN").is_some() {
            app.source = None;
            app.setup = setup::State::at(setup::Step::Welcome, None);
        }
        // `LIBRI_LOCAL`: the library kept on this computer, not connected.
        if var("LIBRI_LOCAL").is_some() {
            app.source = None;
            app.kept_locally = true;
        }
        if let Some(sync) = var("LIBRI_SYNC").and_then(|value| SyncState::from_env(&value)) {
            app.token_warning = sync.warns();
            app.sync = sync;
        }
        if let Some(plasma) = var("LIBRI_PLASMA") {
            app.plasma_dark = plasma != "light";
            app.plasma_fixed = true;
        }
        if let Some(theme) = var("LIBRI_THEME").and_then(|value| ThemeChoice::from_key(&value)) {
            app.settings.theme = theme;
        }
        app.settings.flat |= var("LIBRI_FLAT").is_some();
        // A faked look is for looking at; it never becomes the preference.
        if ["LIBRI_THEME", "LIBRI_FLAT", "LIBRI_PLASMA"]
            .iter()
            .any(|name| var(name).is_some())
        {
            app.preferences.detach();
        }
        app.preferences.mark_saved(Preferences::of(&app));
        if let Some(step) = var("LIBRI_SETUP").and_then(|name| setup::Step::from_name(&name)) {
            app.setup = setup::State::at(step, app.source.as_ref());
        }
        app.library.force_long = var("LIBRI_LONG_LISTS").is_some();
        if let Some(state) =
            var("LIBRI_LOOKUP").and_then(|value| lookup::LookupState::from_env(&value))
        {
            app.lookup = state;
        }
        // The dropped-season dialog, until the daily check of phase 4 opens
        // it: "shows/stranger-things-2016:2".
        if let Some((id, season)) = var("LIBRI_DROPPED").and_then(|spec| {
            let (id, n) = spec.rsplit_once(':')?;
            Some((id.to_owned(), n.parse::<u32>().ok()?))
        }) {
            app.dialog = Some(entry::Dialog::SeasonDropped { id, season });
        }
        let tasks = Task::batch([
            Decoration::take().map(Message::Decoration),
            iced::system::theme().map(Message::SystemTheme),
            token,
        ]);
        (app, tasks)
    }

    /// The library, the connection and the preferences from this
    /// computer's files (`storage`), in place of the fixture in memory.
    /// With no library file yet, a mock run seeds it from the bundled
    /// fixture; a real one starts it empty, and GitHub's copy replaces it
    /// on the first sync. Without a home folder, or when the file cannot
    /// be opened, the fixture stays in memory and nothing is kept this
    /// run. Returns the task that reads the token from KDE Wallet when a
    /// repository is connected; with none, Libri opens on the setup.
    fn open_files(&mut self) -> Task<Message> {
        let seed = if self.mock {
            crate::fixtures::LIBRARY_JSON
        } else {
            codec::EMPTY
        };
        match storage::data_dir() {
            Some(dir) => {
                self.connection_file = ConnectionFile::at(dir.join("connection.json"));
                let path = dir.join("library.json");
                match LocalLibraryRepository::open(path.clone(), seed) {
                    Ok(repo) => {
                        crate::trace::line(format_args!("library {}", path.display()));
                        self.repo = Box::new(repo);
                    }
                    Err(error) => eprintln!(
                        "libri: {} cannot be opened, so nothing is saved this run: {error}",
                        path.display()
                    ),
                }
            }
            None => eprintln!("libri: no home folder, so nothing is saved this run"),
        }
        if let Some(dir) = storage::config_dir() {
            self.preferences = PreferencesFile::at(dir.join("preferences.json"));
            if let Some(preferences) = self.preferences.load() {
                preferences.apply_to(self);
            }
        }
        if self.mock {
            return Task::none();
        }
        self.connection = self.connection_file.load();
        match &self.connection {
            Some(connection) if connection.local => {
                self.source = None;
                self.kept_locally = true;
                Task::none()
            }
            Some(connection) => {
                self.source = Some(Source::of(&connection.target, "…"));
                crate::trace::line(format_args!("connected to {}", connection.target.key()));
                Task::perform(blocking(wallet::read_token), |result| {
                    Message::Sync(SyncMessage::TokenLoaded(result))
                })
            }
            None => {
                self.source = None;
                self.setup = setup::State::real(setup::Step::Welcome, None);
                self.screen = Screen::Setup;
                Task::none()
            }
        }
    }

    /// The app on `screen`, connected to the fixture repository in memory.
    pub fn new(screen: Screen) -> App {
        let repo = FixtureLibraryRepository::load().expect("the bundled fixture decodes");
        let source = Source::fixture();
        App {
            screen,
            settings: Settings::default(),
            plasma_dark: true,
            plasma_fixed: false,
            setup: setup::State::at(setup::Step::Welcome, Some(&source)),
            source: Some(source),
            kept_locally: false,
            sync: SyncState::default(),
            conflict_open: false,
            token_warning: false,
            today: Date::today(),
            modifiers: keyboard::Modifiers::default(),
            repo: Box::new(repo),
            library: library::State::default(),
            dialog: None,
            plans: plans::State::default(),
            plan_dialog: None,
            settings_page: screens::settings::State::default(),
            splash_phase: 0.0,
            window: WINDOW_SIZE,
            frames: crate::trace::Frames::new(std::time::Instant::now()),
            history: History::default(),
            decoration: None,
            lookup: lookup::LookupState::Results,
            preferences: PreferencesFile::default(),
            connection_file: ConnectionFile::default(),
            connection: None,
            token: None,
            github: None,
            engine: None,
            save_due: None,
            loading: false,
            mock: true,
        }
    }

    /// Applies one change to the library and notes its summary in the
    /// trace. A change that cannot apply (an entry gone meanwhile) or
    /// changes nothing leaves the library as it was and is not queued.
    /// With a repository connected the change is queued for GitHub before
    /// the local file applies it, and the gathered edits go out as one
    /// commit after a short pause.
    pub fn apply(&mut self, operation: Operation) {
        let summary = operation.summary(self.repo.library());
        if let Some(engine) = &mut self.engine {
            let mut probe = self.repo.library().clone();
            if let Err(error) = operation.clone().apply_to(&mut probe) {
                eprintln!("libri: change not applied ({summary}): {error}");
                return;
            }
            if probe == *self.repo.library() {
                return;
            }
            engine.queue(operation.clone(), summary.clone());
        }
        match self.repo.apply(operation) {
            Ok(_) => crate::trace::line(format_args!("change: {summary}")),
            Err(error) => eprintln!("libri: change not applied ({summary}): {error}"),
        }
        if self.engine.is_some() {
            self.save_due = Some(Instant::now() + sync::DEBOUNCE);
            self.refresh_sync_state();
        }
    }

    // GitHub sync (phase 3): the engine decides, the app runs each call
    // as a Task and hands the answer back.

    /// The repository just set up (or reconnected): the connection goes to
    /// `connection.json`, the token to KDE Wallet, and the first sync
    /// starts. With `base` the local library is what the setup just put
    /// on GitHub; without one GitHub's copy is downloaded behind the
    /// splash.
    pub fn connect(
        &mut self,
        target: Target,
        token: String,
        client: Arc<github::Client>,
        base: Option<Base>,
    ) -> Task<Message> {
        let mut connection = Connection::new(target.clone());
        connection.token_expires = client.token_expiry().filter(|e| *e > Timestamp::now());
        self.connection_file.save(&connection);
        self.connection = Some(connection);
        self.source = Some(Source::of(&target, setup::token_hint(&token)));
        self.kept_locally = false;
        self.github = Some(client);
        let store = self.store_token(token);
        let mut engine = Engine::open(storage::data_dir().as_deref(), target);
        if let Some(base) = base {
            engine.set_base(base);
        }
        self.loading = !engine.has_base();
        self.engine = Some(engine);
        self.refresh_sync_state();
        self.screen = if self.loading {
            Screen::Splash
        } else {
            Screen::Library
        };
        Task::batch([store, self.sync_now()])
    }

    /// The token from KDE Wallet at the start: the client and the engine
    /// for the saved connection, the token warning if it is due, and the
    /// first check. A fresh connection without a base downloads GitHub's
    /// copy behind the splash.
    fn connect_with(&mut self, token: String) -> Task<Message> {
        let Some(connection) = &self.connection else {
            return Task::none();
        };
        let target = connection.target.clone();
        self.source = Some(Source::of(&target, setup::token_hint(&token)));
        self.github = Some(Arc::new(github::Client::new(
            token.clone(),
            target.repository.clone(),
        )));
        self.token = Some(token);
        let engine = Engine::open(storage::data_dir().as_deref(), target);
        self.loading = !engine.has_base();
        self.engine = Some(engine);
        self.warn_about_the_token();
        self.refresh_sync_state();
        if self.loading {
            self.screen = Screen::Splash;
        }
        self.sync_now()
    }

    /// A new token for the same connection ("Replace token"): to the
    /// wallet, a new client, and a sync right away. Its expiry comes with
    /// the next answer from GitHub, so the old one goes.
    pub fn replace_token(&mut self, token: String, client: Arc<github::Client>) -> Task<Message> {
        self.github = Some(client);
        if let Some(connection) = &mut self.connection {
            connection.token_expires = None;
            connection.token_warned_on = None;
            self.connection_file.save(connection);
        }
        if let Some(source) = &mut self.source {
            source.token_hint = setup::token_hint(&token);
        }
        let store = self.store_token(token);
        Task::batch([store, self.sync_now()])
    }

    fn store_token(&mut self, token: String) -> Task<Message> {
        self.token = Some(token.clone());
        if self.mock {
            return Task::none();
        }
        Task::perform(blocking(move || wallet::write_token(&token)), |result| {
            Message::Sync(SyncMessage::WalletDone(result))
        })
    }

    /// Disconnect: the sync stops, the queue and the token leave this
    /// computer, and GitHub is never touched. With `keep` the library and
    /// its base stay for use without a sync and for the comparison on a
    /// reconnect; otherwise the file goes too.
    pub fn disconnect(&mut self, keep: bool) -> Task<Message> {
        if let Some(mut engine) = self.engine.take() {
            engine.discard(keep);
        }
        self.github = None;
        self.token = None;
        self.save_due = None;
        self.loading = false;
        self.sync = SyncState::Synced;
        self.conflict_open = false;
        self.token_warning = false;
        if keep {
            if let Some(connection) = &mut self.connection {
                connection.local = true;
                connection.token_expires = None;
                connection.token_warned_on = None;
                self.connection_file.save(connection);
            }
        } else {
            if let Err(error) = self.repo.delete() {
                eprintln!("libri: could not delete the library: {error}");
            }
            self.connection = None;
            self.connection_file.clear();
        }
        if self.mock {
            return Task::none();
        }
        Task::perform(blocking(wallet::delete_token), |result| {
            Message::Sync(SyncMessage::WalletDone(result))
        })
    }

    /// Sync now, Retry, the check every five minutes and the window coming
    /// back: a round without waiting for the debounce.
    pub fn sync_now(&mut self) -> Task<Message> {
        self.save_due = None;
        self.begin(Kind::Sync)
    }

    fn begin(&mut self, kind: Kind) -> Task<Message> {
        let Some(engine) = &mut self.engine else {
            return Task::none();
        };
        let outcome = engine.begin(kind);
        self.refresh_sync_state();
        self.run(outcome)
    }

    /// Takes what the engine asked for: GitHub's copy in place of this
    /// computer's, and the next call as a Task.
    fn run(&mut self, outcome: Outcome) -> Task<Message> {
        if let Some(library) = outcome.library {
            crate::trace::line("sync: taking GitHub's copy");
            self.repo.replace(library);
        }
        let (Some(request), Some(client), Some(engine)) =
            (outcome.request, &self.github, &self.engine)
        else {
            // The first download is over (or failed): the library shows.
            if self.loading && !self.engine.as_ref().is_some_and(Engine::running) {
                self.loading = false;
                self.screen = Screen::Library;
            }
            return Task::none();
        };
        let (client, target) = (client.clone(), engine.target().clone());
        Task::perform(
            blocking(move || Ok(perform(&client, &target, request))),
            |result| {
                let answer = result.unwrap_or_else(|error| {
                    Answer::Raw(Err(github::Failure::new(github::FailureKind::Other, error)))
                });
                Message::Sync(SyncMessage::Answer(Box::new(answer)))
            },
        )
    }

    /// The engine's status as the sidebar row, the banner and Settings
    /// show it. A mock run keeps the faked state.
    pub fn refresh_sync_state(&mut self) {
        let Some(engine) = &self.engine else {
            return;
        };
        let status = engine.status();
        let n = status.pending as u32;
        let expires_in = self.token_expires_in();
        self.sync = match status.phase {
            Phase::Idle | Phase::Checking => match expires_in {
                Some(days) if (0..=TOKEN_WARNING_DAYS).contains(&days) => {
                    SyncState::ExpiresIn(days as u32)
                }
                _ => SyncState::Synced,
            },
            Phase::Saving => SyncState::Saving { changes: n },
            Phase::Offline => SyncState::Offline { kept: n },
            Phase::Failed => SyncState::Failed { kept: n },
            Phase::TokenProblem => SyncState::TokenExpired,
            Phase::Conflict => SyncState::Conflict {
                local_changes: n,
                remote_commit: status
                    .remote_changed_at
                    .map_or_else(|| "unknown".to_owned(), Timestamp::month_day_time),
            },
            Phase::NotFound(missing) => SyncState::NotFound(missing),
        };
    }

    /// Whole days until the token expires, as GitHub last said; negative
    /// once it has. `None` for a token without an expiry, or none known.
    /// A mock run takes it from the faked state.
    pub fn token_expires_in(&self) -> Option<i64> {
        if let SyncState::ExpiresIn(days) = &self.sync
            && self.engine.is_none()
        {
            return Some(i64::from(*days));
        }
        let expires = self.connection.as_ref()?.token_expires?;
        Some(expires.days_from(Timestamp::now()))
    }

    /// GitHub said when the token expires (the header on a successful
    /// answer). Kept only when the date is ahead: a date that has passed
    /// while requests still succeed means nothing.
    fn record_token_expiry(&mut self) {
        let Some(expires) = self.github.as_ref().and_then(|c| c.token_expiry()) else {
            return;
        };
        if expires <= Timestamp::now() {
            return;
        }
        if let Some(connection) = &mut self.connection
            && connection.token_expires != Some(expires)
        {
            connection.token_expires = Some(expires);
            self.connection_file.save(connection);
        }
    }

    /// The warning dialog once a day from seven days before the expiry.
    fn warn_about_the_token(&mut self) {
        let today = Timestamp::now().local_date();
        let Some(days) = self.token_expires_in() else {
            return;
        };
        let Some(connection) = &mut self.connection else {
            return;
        };
        if !(0..=TOKEN_WARNING_DAYS).contains(&days) || connection.token_warned_on == Some(today) {
            return;
        }
        connection.token_warned_on = Some(today);
        self.connection_file.save(connection);
        self.token_warning = true;
    }

    /// "Synced just now", "Synced 5 minutes ago": the sidebar row's label
    /// while nothing is wrong. The mock state says "Synced 2 min ago".
    pub fn synced_label(&self) -> String {
        match self.engine.as_ref().and_then(|e| e.status().last_synced) {
            Some(at) => format!("Synced {}", at.ago(Timestamp::now())),
            None if self.engine.is_some() => "Not synced yet".into(),
            None => "Synced 2 min ago".into(),
        }
    }

    /// "Last synced 5 minutes ago", for Settings.
    pub fn last_synced_label(&self) -> String {
        match self.engine.as_ref().and_then(|e| e.status().last_synced) {
            Some(at) => format!("Last synced {}", at.ago(Timestamp::now())),
            None if self.engine.is_some() => "Not synced yet".into(),
            None => "Last synced 2 min ago".into(),
        }
    }

    /// Whether the last check is older than the interval, so the window
    /// coming back asks again.
    fn check_is_stale(&self) -> bool {
        let Some(engine) = &self.engine else {
            return false;
        };
        match engine.status().last_checked {
            Some(at) => Timestamp::now().0 - at.0 >= sync::CHECK_INTERVAL.as_secs() as i64,
            None => true,
        }
    }

    /// The theme drawn: the Theme setting, with System resolved by Plasma.
    pub fn theme(&self) -> Theme {
        self.settings.theme.resolve(self.plasma_dark)
    }

    pub fn tokens(&self) -> Tokens {
        Tokens::styled(self.theme(), self.settings.accent, self.settings.flat)
    }

    /// Handles `message`, and records the place left when it moved to a
    /// new one. The splash and the setup are not places to come back to.
    /// The title bar follows the theme.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        if self.library.note_draft.is_some() && message.leaves_note() {
            entry::save_note(self);
        }
        let from = (message.navigates() && self.screen.in_history()).then(|| self.place());
        let task = self.handle(message);
        if let Some(from) = from
            && from != self.place()
        {
            self.history.visit(from);
        }
        let (theme, flat) = (self.theme(), self.settings.flat);
        if let Some(decoration) = &mut self.decoration {
            decoration.set(theme, flat);
        }
        if self.preferences.is_attached() {
            let now = Preferences::of(self);
            self.preferences.save_if_changed(now);
        }
        task
    }

    /// Where the app is, for the history.
    pub fn place(&self) -> Place {
        Place {
            screen: self.screen,
            library: self.library.place(),
            plans: self.plans.page.clone(),
        }
    }

    /// Back or Forward: the place before or after this one, with the
    /// selection and the popovers closed. Nothing happens over a dialog or
    /// the token warning. On the setup, Back is the wizard's own Back.
    fn travel(&mut self, direction: Direction) -> Task<Message> {
        if self.token_warning || self.dialog.is_some() || self.plan_dialog.is_some() {
            return Task::none();
        }
        match self.screen {
            Screen::Splash => {}
            Screen::Setup => {
                if direction == Direction::Back {
                    return setup::update(self, setup::Message::Back);
                }
            }
            _ => {
                if let Some(place) = self.history.go(direction, self.place()) {
                    self.screen = place.screen;
                    self.library.go_to(place.library);
                    self.plans.go_to(place.plans);
                }
            }
        }
        Task::none()
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Show(screen) => self.screen = screen,
            Message::History(direction) => return self.travel(direction),
            Message::SetTheme(choice) => self.settings.theme = choice,
            Message::SetAccent(accent) => self.settings.accent = accent,
            // No answer from the portal (`None`) keeps the scheme, which
            // is dark at the start: System then means Graphite.
            Message::SystemTheme(mode) if !self.plasma_fixed => {
                crate::trace::line(format_args!("plasma scheme {mode:?}"));
                match mode {
                    ThemeMode::Dark => self.plasma_dark = true,
                    ThemeMode::Light => self.plasma_dark = false,
                    ThemeMode::None => {}
                }
            }
            Message::SystemTheme(_) => {}
            // One sweep of the bar takes 1.4 s.
            Message::Tick => self.splash_phase = (self.splash_phase + 0.016 / 1.4) % 1.0,
            Message::Resized(size) => self.window = size,
            Message::Window(event) => return self.window_event(event),
            Message::Decoration(decoration) => self.decoration = decoration,
            Message::Sync(SyncMessage::Retry | SyncMessage::Now) => {
                if self.engine.is_some() {
                    return self.sync_now();
                }
                // The faked state moves on.
                self.sync = SyncState::Saving { changes: 3 };
            }
            Message::Sync(SyncMessage::Tick) => {
                if self.save_due.is_some_and(|due| Instant::now() >= due) {
                    return self.sync_now();
                }
            }
            Message::Sync(SyncMessage::Fix) => {
                self.screen = Screen::Settings;
                return screens::settings::update(self, screens::settings::Message::Jump(1));
            }
            Message::Sync(SyncMessage::OpenConflict) => self.conflict_open = true,
            Message::Sync(SyncMessage::CloseConflict) => self.conflict_open = false,
            Message::Sync(SyncMessage::KeepMine) => {
                self.conflict_open = false;
                if self.engine.is_some() {
                    return self.begin(Kind::KeepMine);
                }
                self.sync = SyncState::Saving { changes: 3 };
            }
            Message::Sync(SyncMessage::KeepTheirs) => {
                self.conflict_open = false;
                if self.engine.is_some() {
                    return self.begin(Kind::KeepTheirs);
                }
                self.sync = SyncState::Synced;
            }
            Message::Sync(SyncMessage::TokenLoaded(Ok(Some(token)))) => {
                return self.connect_with(token);
            }
            Message::Sync(SyncMessage::TokenLoaded(result)) => {
                // No token, or a wallet that could not be read: the first
                // run, with the last repo and file prefilled.
                if let Err(error) = result {
                    eprintln!("libri: the token could not be read: {error}");
                }
                crate::trace::line("no token: the setup");
                self.source = None;
                self.setup = setup::State::real(setup::Step::Connect, self.connection.as_ref());
                self.screen = Screen::Setup;
            }
            Message::Sync(SyncMessage::WalletDone(result)) => {
                if let Err(error) = result {
                    eprintln!("libri: {error}");
                    crate::trace::line(format_args!("wallet: {error}"));
                }
            }
            Message::Sync(SyncMessage::Answer(answer)) => {
                self.record_token_expiry();
                let Some(engine) = &mut self.engine else {
                    return Task::none();
                };
                let outcome = engine.answer(*answer, self.repo.library());
                self.refresh_sync_state();
                return self.run(outcome);
            }
            Message::Key(KeyPress::Tab { back: false }) => return operation::focus_next(),
            Message::Key(KeyPress::Tab { back: true }) => return operation::focus_previous(),
            // The token warning sits over everything; then a dialog; the
            // sidebar's All tags popover also opens on Plans.
            Message::Key(KeyPress::Escape) => match self.screen {
                _ if self.token_warning => self.token_warning = false,
                _ if self.conflict_open => self.conflict_open = false,
                _ => {
                    if !entry::dismiss_top(self)
                        && !plans::dismiss_top(self)
                        && !screens::settings::dismiss_top(self)
                    {
                        self.library.dismiss_top();
                    }
                }
            },
            Message::Key(KeyPress::Enter) => match self.screen {
                _ if self.token_warning => return token::update(self, token::Message::Replace),
                Screen::Setup => return setup::update(self, setup::Message::Continue),
                // Enter in the Disconnect dialog takes the safe step: on to
                // keep or delete, then Keep it on this computer.
                Screen::Settings if self.settings_page.disconnect.is_some() => {
                    let step = self.settings_page.disconnect;
                    let message = match step {
                        Some(screens::settings::DisconnectStep::Confirm) => {
                            screens::settings::Message::ConfirmDisconnect
                        }
                        _ => screens::settings::Message::DisconnectKeep,
                    };
                    return screens::settings::update(self, message);
                }
                Screen::Settings if self.settings_page.licenses_open => {
                    return screens::settings::update(
                        self,
                        screens::settings::Message::CloseLicenses,
                    );
                }
                _ if self.dialog.is_some() => return entry::confirm(self),
                _ if self.plan_dialog.is_some() => return plans::confirm(self),
                _ => self.library.confirm(),
            },
            Message::Key(KeyPress::Save) if self.plan_dialog.is_some() => {
                return plans::update(self, plans::Message::Save);
            }
            Message::Key(KeyPress::Save) => return entry::update(self, entry::Message::Save),
            Message::Key(KeyPress::Up) => return self.step(-1),
            Message::Key(KeyPress::Down) => return self.step(1),
            Message::Key(KeyPress::New) => {
                if matches!(self.screen, Screen::Library | Screen::Plans)
                    && self.dialog.is_none()
                    && self.plan_dialog.is_none()
                    && !self.token_warning
                {
                    let _ = entry::update(self, entry::Message::Add);
                    return operation::focus(entry::LOOKUP_ID);
                }
            }
            Message::Key(KeyPress::Edit) => {
                let selected = match self.screen {
                    Screen::Library => self.library.selected.clone(),
                    Screen::Plans => self.plans.selected.clone(),
                    _ => None,
                };
                if self.dialog.is_none()
                    && self.plan_dialog.is_none()
                    && let Some(id) = selected
                {
                    return entry::update(self, entry::Message::Edit(id));
                }
            }
            Message::Entry(message) => {
                let opens_form = matches!(message, entry::Message::Add);
                let task = entry::update(self, message);
                if opens_form {
                    return operation::focus(entry::LOOKUP_ID);
                }
                return task;
            }
            Message::Library(library::Message::Add) => {
                let _ = entry::update(self, entry::Message::Add);
                return operation::focus(entry::LOOKUP_ID);
            }
            Message::Key(KeyPress::Search) => {
                if self.screen == Screen::Library {
                    return operation::focus(library::SEARCH_ID);
                }
            }
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::Library(library::Message::JumpToGroup(index)) => {
                if let Some(&top) = self.library.group_tops.get(index) {
                    let offset = scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(top),
                    };
                    return operation::scroll_to(library::LIST_ID, offset);
                }
            }
            Message::Library(library::Message::Picker(tags::PickerMessage::Jump(offset))) => {
                return tags::jump(offset);
            }
            Message::Library(library::Message::PickSeriesCover(id)) => {
                self.apply(Operation::SetSeriesCover { id });
            }
            Message::Library(message) => {
                // Ctrl+click on a sidebar type adds or removes it.
                let message = match message {
                    library::Message::ShowType(Some(media)) if self.modifiers.command() => {
                        library::Message::ToggleType(media)
                    }
                    message => message,
                };
                if message.shows_library() {
                    self.screen = Screen::Library;
                }
                self.library.update(message);
            }
            Message::Plans(message) => {
                // Ctrl+click on a checklist or timeline row ticks it or
                // unticks it, instead of selecting it.
                let message = match message {
                    plans::Message::Select(id) if self.modifiers.command() => {
                        plans::Message::ToggleDone(id)
                    }
                    message => message,
                };
                self.screen = Screen::Plans;
                return plans::update(self, message);
            }
            Message::Setup(message) => return setup::update(self, message),
            Message::Settings(message) => return screens::settings::update(self, message),
            Message::Token(message) => return token::update(self, message),
        }
        Task::none()
    }

    /// An arrow key: moves the lookup's pick in the entry form, or the
    /// selection in the library's list or the plan's checklist.
    fn step(&mut self, step: isize) -> Task<Message> {
        if self.dialog.is_some() {
            return entry::update(self, entry::Message::MovePick(step));
        }
        if self.plan_dialog.is_some() || self.token_warning {
            return Task::none();
        }
        match self.screen {
            Screen::Library => {
                let order = self.library.display_order(self.repo.library(), self.today);
                self.library.selected = next_in(&order, self.library.selected.as_deref(), step);
            }
            Screen::Plans => plans::step_selection(self, step),
            _ => {}
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        screens::view(self)
    }

    /// "Movies — Libri": the screen's name, then the app's.
    pub fn title(&self) -> String {
        match screens::title(self) {
            Some(name) => format!("{name} — Libri"),
            None => "Libri".into(),
        }
    }

    fn iced_theme(&self) -> iced::Theme {
        self.tokens().iced_theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::ModifiersChanged(modifiers) => Some(Message::Modifiers(modifiers)),
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                let press = match key {
                    Key::Named(Named::ArrowLeft) if modifiers.alt() => {
                        return Some(Message::History(Direction::Back));
                    }
                    Key::Named(Named::ArrowRight) if modifiers.alt() => {
                        return Some(Message::History(Direction::Forward));
                    }
                    Key::Named(Named::Escape) => KeyPress::Escape,
                    Key::Named(Named::Enter) if modifiers.command() => KeyPress::Save,
                    Key::Named(Named::Enter) => KeyPress::Enter,
                    Key::Named(Named::ArrowUp) => KeyPress::Up,
                    Key::Named(Named::ArrowDown) => KeyPress::Down,
                    Key::Character(c) if c == "e" && modifiers.is_empty() => KeyPress::Edit,
                    Key::Named(Named::Tab) => KeyPress::Tab {
                        back: modifiers.shift(),
                    },
                    Key::Character(c) if modifiers.command() && (c == "k" || c == "f") => {
                        KeyPress::Search
                    }
                    Key::Character(c) if modifiers.command() && c == "n" => KeyPress::New,
                    _ => return None,
                };
                Some(Message::Key(press))
            }
            _ => None,
        });
        // The Back and Forward buttons of a mouse; no widget takes them.
        let buttons = event::listen_with(|event, _status, _window| match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Back)) => {
                Some(Message::History(Direction::Back))
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Forward)) => {
                Some(Message::History(Direction::Forward))
            }
            _ => None,
        });
        let resizes = window::resize_events().map(|(_, size)| Message::Resized(size));
        let traced = window::events().map(|(_, event)| Message::Window(event));
        let scheme = iced::system::theme_changes().map(Message::SystemTheme);
        let mut subscriptions = vec![keys, buttons, resizes, traced, scheme];
        if self.screen == Screen::Splash {
            subscriptions.push(time::every(Duration::from_millis(16)).map(|_| Message::Tick));
        }
        // The debounce clock runs only while edits wait; the check every
        // five minutes while a repository is connected.
        if self.save_due.is_some() {
            subscriptions.push(
                time::every(Duration::from_millis(250)).map(|_| Message::Sync(SyncMessage::Tick)),
            );
        }
        if self.engine.is_some() {
            subscriptions
                .push(time::every(sync::CHECK_INTERVAL).map(|_| Message::Sync(SyncMessage::Now)));
        }
        Subscription::batch(subscriptions)
    }

    /// A window event into the freeze trace: redraws as a rate and
    /// gaps, everything else as it comes. The window coming back asks
    /// GitHub for changes when the last check is old.
    fn window_event(&mut self, event: window::Event) -> Task<Message> {
        match event {
            window::Event::RedrawRequested(now) => {
                for line in self.frames.redraw(now) {
                    crate::trace::line(line);
                }
            }
            window::Event::Moved(_) => {}
            event => {
                crate::trace::line(format_args!("window {event:?}"));
                if event == window::Event::Focused && self.check_is_stale() {
                    return self.sync_now();
                }
            }
        }
        Task::none()
    }
}

/// Runs a blocking call (GitHub, KDE Wallet) on tokio's blocking pool, so
/// the window never waits on it.
async fn blocking<T, F>(call: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(call)
        .await
        .unwrap_or_else(|error| Err(format!("the call did not finish: {error}")))
}

/// One of the engine's requests against the real API.
fn perform(client: &github::Client, target: &Target, request: Request) -> Answer {
    match request {
        Request::Raw { etag } => {
            Answer::Raw(client.file_raw(&target.file, &target.branch, etag.as_deref()))
        }
        Request::Put {
            bytes,
            message,
            sha,
        } => Answer::Put(client.put_file(
            &target.file,
            &target.branch,
            &bytes,
            &message,
            sha.as_deref(),
        )),
        Request::Commits { limit } => {
            Answer::Commits(client.commits(&target.file, &target.branch, limit))
        }
        Request::Repo => Answer::Repo(client.repo()),
    }
}

/// The id `step` places after `current` in `order`: the first with no
/// selection, and the ends stop.
pub fn next_in(order: &[String], current: Option<&str>, step: isize) -> Option<String> {
    if order.is_empty() {
        return current.map(str::to_owned);
    }
    let index = match current.and_then(|id| order.iter().position(|o| o == id)) {
        Some(index) => (index as isize + step).clamp(0, order.len() as isize - 1) as usize,
        None if step < 0 => order.len() - 1,
        None => 0,
    };
    Some(order[index].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MediaType;
    use crate::theme;

    #[test]
    fn starts_on_the_library_in_graphite_and_peach() {
        let app = App::new(Screen::default());
        assert_eq!(app.screen, Screen::Library);
        assert_eq!(app.settings.theme, ThemeChoice::System);
        assert_eq!(app.theme(), Theme::Graphite);
        assert_eq!(app.settings.accent, theme::PEACH);
        assert_eq!(app.sync, SyncState::Synced);
        assert!(app.source.is_some());
        assert!(!app.repo.library().entries.is_empty());
    }

    fn back(app: &mut App) {
        let _ = app.update(Message::History(Direction::Back));
    }

    fn forward(app: &mut App) {
        let _ = app.update(Message::History(Direction::Forward));
    }

    #[test]
    fn back_and_forward_walk_through_the_places_visited() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Library(library::Message::ShowType(Some(
            MediaType::Movies,
        ))));
        let _ = app.update(Message::Plans(plans::Message::ShowPlan(
            "halloween-2026".into(),
        )));
        let _ = app.update(Message::Show(Screen::Settings));
        assert_eq!(app.screen, Screen::Settings);

        back(&mut app);
        assert_eq!(app.screen, Screen::Plans);
        assert_eq!(app.plans.page, plans::Page::Plan("halloween-2026".into()));
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
        assert_eq!(app.library.single(), Some(MediaType::Movies));
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
        assert!(app.library.types.is_empty());
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
        assert!(app.library.types.is_empty());

        forward(&mut app);
        assert_eq!(app.library.single(), Some(MediaType::Movies));
        forward(&mut app);
        assert_eq!(app.screen, Screen::Plans);
        forward(&mut app);
        assert_eq!(app.screen, Screen::Settings);
        forward(&mut app);
        assert_eq!(app.screen, Screen::Settings);
    }

    #[test]
    fn a_new_place_after_going_back_drops_the_places_ahead() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Show(Screen::Settings));
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
        let _ = app.update(Message::Show(Screen::Plans));
        forward(&mut app);
        assert_eq!(app.screen, Screen::Plans);
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
    }

    #[test]
    fn back_closes_an_artist_page_and_keeps_the_search() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Library(library::Message::ShowType(Some(
            MediaType::Music,
        ))));
        let _ = app.update(Message::Library(library::Message::Search("met".into())));
        let _ = app.update(Message::Library(library::Message::OpenArtist(
            "Metallica".into(),
        )));
        back(&mut app);
        assert_eq!(app.library.artist, None);
        assert_eq!(app.library.single(), Some(MediaType::Music));
        assert_eq!(app.library.query, "met");
        forward(&mut app);
        assert_eq!(app.library.artist.as_deref(), Some("Metallica"));
    }

    #[test]
    fn a_pick_of_the_same_place_is_not_a_visit() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Library(library::Message::ShowType(None)));
        let _ = app.update(Message::Plans(plans::Message::ShowOverview));
        let _ = app.update(Message::Plans(plans::Message::ShowOverview));
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
        assert!(!app.history.can_go(Direction::Back));
    }

    #[test]
    fn back_waits_while_a_dialog_or_the_token_warning_is_open() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Show(Screen::Plans));
        let _ = app.update(Message::Plans(plans::Message::NewPlan));
        assert!(app.plan_dialog.is_some());
        back(&mut app);
        assert_eq!(app.screen, Screen::Plans);
        app.plan_dialog = None;
        app.token_warning = true;
        back(&mut app);
        assert_eq!(app.screen, Screen::Plans);
        app.token_warning = false;
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
    }

    #[test]
    fn on_the_setup_back_is_the_wizards_back_and_the_splash_is_no_place() {
        let mut app = App::new(Screen::Splash);
        let _ = app.update(Message::Show(Screen::Library));
        assert!(!app.history.can_go(Direction::Back));

        let _ = app.update(Message::Setup(setup::Message::ReplaceToken));
        assert_eq!(app.screen, Screen::Setup);
        forward(&mut app);
        assert_eq!(app.screen, Screen::Setup);
        back(&mut app);
        assert_eq!(app.screen, Screen::Library);
    }

    #[test]
    fn show_switches_the_screen() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Show(Screen::Plans));
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn theme_and_accent_change_the_tokens() {
        let mut app = App::new(Screen::Library);
        let violet = theme::VIVID[1];
        let _ = app.update(Message::SetTheme(ThemeChoice::Fixed(Theme::Light)));
        let _ = app.update(Message::SetAccent(violet));
        let tokens = app.tokens();
        assert!(tokens.light);
        assert_eq!((tokens.accent, tokens.ink), (violet.fill, violet.ink));
    }

    #[test]
    fn the_system_theme_follows_plasma() {
        let mut app = App::new(Screen::Library);
        app.plasma_dark = false;
        assert_eq!(app.theme(), Theme::Light);
        let _ = app.update(Message::SetTheme(ThemeChoice::Fixed(Theme::Mocha)));
        assert_eq!(app.theme(), Theme::Mocha);
    }

    #[test]
    fn the_portal_switches_the_system_theme_unless_plasma_is_faked() {
        let mut app = App::new(Screen::Library);
        assert_eq!(app.theme(), Theme::Graphite);
        let _ = app.update(Message::SystemTheme(ThemeMode::Light));
        assert_eq!(app.theme(), Theme::Light);
        let _ = app.update(Message::SystemTheme(ThemeMode::None));
        assert_eq!(app.theme(), Theme::Light, "no answer keeps the scheme");
        let _ = app.update(Message::SystemTheme(ThemeMode::Dark));
        assert_eq!(app.theme(), Theme::Graphite);
        app.plasma_fixed = true;
        let _ = app.update(Message::SystemTheme(ThemeMode::Light));
        assert_eq!(app.theme(), Theme::Graphite, "LIBRI_PLASMA wins");
        app.plasma_fixed = false;
        let _ = app.update(Message::SetTheme(ThemeChoice::Fixed(Theme::Mocha)));
        let _ = app.update(Message::SystemTheme(ThemeMode::Light));
        assert_eq!(app.theme(), Theme::Mocha, "a named theme ignores Plasma");
    }

    #[test]
    fn a_changed_preference_is_written_and_a_change_to_the_library_too() {
        let dir = crate::storage::TempDir::new("app");
        let mut app = App::new(Screen::Library);
        app.preferences = PreferencesFile::at(dir.path("preferences.json"));
        app.preferences.mark_saved(Preferences::of(&app));
        app.repo = Box::new(
            LocalLibraryRepository::open(dir.path("library.json"), crate::fixtures::LIBRARY_JSON)
                .unwrap(),
        );
        let _ = app.update(Message::Show(Screen::Plans));
        assert!(
            !dir.path("preferences.json").exists(),
            "a screen change is no preference"
        );
        let _ = app.update(Message::SetAccent(theme::VIVID[1]));
        let text = std::fs::read_to_string(dir.path("preferences.json")).unwrap();
        assert!(text.contains(&format!("\"accent\": \"vivid:{}\"", theme::VIVID[1].name)));
        let _ = app.update(Message::Library(library::Message::TogglePin(
            "horror".into(),
        )));
        let text = std::fs::read_to_string(dir.path("preferences.json")).unwrap();
        assert!(text.contains("\"horror\""));

        app.apply(Operation::SetRating {
            id: "movies/the-mummy-1999".into(),
            rating: Some(3),
        });
        let text = std::fs::read_to_string(dir.path("library.json")).unwrap();
        assert_eq!(text, crate::model::codec::encode(app.repo.library()));
        assert_eq!(
            app.repo
                .library()
                .entry("movies/the-mummy-1999")
                .unwrap()
                .rating,
            Some(3)
        );
    }

    #[test]
    fn the_window_title_names_the_screen() {
        let mut app = App::new(Screen::Library);
        assert_eq!(app.title(), "Library — Libri");
        let _ = app.update(Message::Library(library::Message::ShowType(Some(
            MediaType::Movies,
        ))));
        assert_eq!(app.title(), "Movies — Libri");
        let _ = app.update(Message::Show(Screen::Splash));
        assert_eq!(app.title(), "Libri");
        let _ = app.update(Message::Show(Screen::Settings));
        assert_eq!(app.title(), "Settings — Libri");
    }

    #[test]
    fn a_sidebar_pick_on_another_screen_shows_the_library() {
        let mut app = App::new(Screen::Plans);
        let _ = app.update(Message::Library(library::Message::PickTag("horror".into())));
        assert_eq!(app.screen, Screen::Library);
        assert_eq!(app.library.filter.tags, ["horror"]);
    }

    #[test]
    fn ctrl_click_on_a_sidebar_type_adds_it() {
        let mut app = App::new(Screen::Library);
        let show = |media| Message::Library(library::Message::ShowType(Some(media)));
        let _ = app.update(show(MediaType::Movies));
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::CTRL));
        let _ = app.update(show(MediaType::Books));
        assert_eq!(app.title(), "Movies + Books — Libri");
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::empty()));
        let _ = app.update(show(MediaType::Games));
        assert_eq!(app.title(), "Games — Libri");
    }

    #[test]
    fn ctrl_click_on_a_plan_row_ticks_it() {
        let mut app = App::new(Screen::Plans);
        let id = "movies/die-hard-1988";
        let click = || Message::Plans(plans::Message::Select(id.into()));
        let done = |app: &App| {
            app.repo
                .library()
                .plan("christmas-2026")
                .and_then(|plan| plan.item_for(id))
                .is_some_and(|item| item.done)
        };
        let _ = app.update(Message::Plans(plans::Message::ShowPlan(
            "christmas-2026".into(),
        )));
        assert!(!done(&app));
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::CTRL));
        let _ = app.update(click());
        assert!(done(&app) && app.plans.selected.is_none());
        let _ = app.update(click());
        assert!(!done(&app) && app.plans.selected.is_none());
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::empty()));
        let _ = app.update(click());
        assert!(!done(&app) && app.plans.selected.as_deref() == Some(id));
    }

    #[test]
    fn escape_closes_the_token_warning_first() {
        let mut app = App::new(Screen::Library);
        app.sync = SyncState::ExpiresIn(6);
        app.token_warning = true;
        let _ = app.update(Message::Library(library::Message::Toggle(
            library::Popup::Filter,
        )));
        let _ = app.update(Message::Key(KeyPress::Escape));
        assert!(!app.token_warning);
        assert_eq!(app.library.popup, Some(library::Popup::Filter));
        let _ = app.update(Message::Key(KeyPress::Escape));
        assert_eq!(app.library.popup, None);
    }

    #[test]
    fn arrow_keys_move_the_selection_through_the_list() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Library(library::Message::ShowType(Some(
            MediaType::Movies,
        ))));
        let order = app.library.display_order(app.repo.library(), app.today);
        assert!(order.len() > 2);
        let _ = app.update(Message::Key(KeyPress::Down));
        assert_eq!(app.library.selected.as_deref(), Some(order[0].as_str()));
        let _ = app.update(Message::Key(KeyPress::Down));
        assert_eq!(app.library.selected.as_deref(), Some(order[1].as_str()));
        let _ = app.update(Message::Key(KeyPress::Up));
        let _ = app.update(Message::Key(KeyPress::Up));
        assert_eq!(
            app.library.selected.as_deref(),
            Some(order[0].as_str()),
            "the top stops"
        );
        // In the entry form the arrows move the lookup's pick instead.
        let _ = app.update(Message::Key(KeyPress::New));
        assert!(app.dialog.is_some());
        let _ = app.update(Message::Key(KeyPress::Down));
        assert_eq!(app.library.selected.as_deref(), Some(order[0].as_str()));
    }

    #[test]
    fn next_in_starts_at_either_end_and_stops_there() {
        let order: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!(next_in(&order, None, 1).as_deref(), Some("a"));
        assert_eq!(next_in(&order, None, -1).as_deref(), Some("c"));
        assert_eq!(next_in(&order, Some("c"), 1).as_deref(), Some("c"));
        assert_eq!(next_in(&order, Some("zz"), 1).as_deref(), Some("a"));
        assert_eq!(next_in(&[], Some("a"), 1).as_deref(), Some("a"));
    }

    #[test]
    fn the_splash_ticks_its_bar() {
        let mut app = App::new(Screen::Splash);
        let _ = app.update(Message::Tick);
        assert!(app.splash_phase > 0.0 && app.splash_phase < 1.0);
    }

    // GitHub sync (phase 3): the app around the engine, without a client,
    // so no call ever goes out.

    fn target() -> Target {
        Target {
            repository: "ekmanch/libri-scratch".into(),
            file: "library.json".into(),
            branch: "main".into(),
        }
    }

    /// An app connected to the fixture as its base: in sync, nothing
    /// pending.
    fn connected(app: &mut App) {
        let text = codec::encode(app.repo.library());
        let sha = github::blob_sha(text.as_bytes());
        let mut engine = Engine::open(None, target());
        engine.set_base(Base {
            target: target().key(),
            etag: Some(format!("\"{sha}\"")),
            remote_sha: sha.clone(),
            canonical_sha: sha,
            commit_sha: None,
            last_synced: Some(Timestamp::now()),
            last_checked: Some(Timestamp::now()),
        });
        app.engine = Some(engine);
        app.connection = Some(Connection::new(target()));
        app.refresh_sync_state();
    }

    fn rate(id: &str, rating: Option<u8>) -> Operation {
        Operation::SetRating {
            id: id.into(),
            rating,
        }
    }

    #[test]
    fn a_change_is_queued_for_github_and_the_save_falls_due() {
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        assert_eq!(app.sync, SyncState::Synced);
        assert!(app.save_due.is_none());
        app.apply(rate("movies/the-mummy-1999", Some(3)));
        assert_eq!(app.engine.as_ref().unwrap().status().pending, 1);
        assert!(app.save_due.is_some());
        // The same change again changes nothing and is not queued; a
        // change that cannot apply neither.
        app.apply(rate("movies/the-mummy-1999", Some(3)));
        app.apply(rate("movies/nope", Some(3)));
        assert_eq!(app.engine.as_ref().unwrap().status().pending, 1);
        assert_eq!(
            app.engine.as_ref().unwrap().pending().operations()[0].summary,
            "Rate The Mummy 3 of 10"
        );
        // The tick starts the round once the save is due.
        app.save_due = Some(Instant::now() - Duration::from_secs(1));
        let _ = app.update(Message::Sync(SyncMessage::Tick));
        assert!(app.save_due.is_none());
        assert!(app.engine.as_ref().unwrap().running());
    }

    #[test]
    fn the_engines_phases_show_as_the_sidebar_states() {
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        app.apply(rate("movies/the-mummy-1999", Some(3)));
        let library = app.repo.library().clone();
        let fail = |app: &mut App, kind: github::FailureKind| {
            let engine = app.engine.as_mut().unwrap();
            let _ = engine.begin(Kind::Sync);
            let _ = engine.answer(Answer::Raw(Err(github::Failure::new(kind, ""))), &library);
            app.refresh_sync_state();
        };
        fail(&mut app, github::FailureKind::Offline);
        assert_eq!(app.sync, SyncState::Offline { kept: 1 });
        fail(&mut app, github::FailureKind::Other);
        assert_eq!(app.sync, SyncState::Failed { kept: 1 });
        fail(&mut app, github::FailureKind::Unauthorized);
        assert_eq!(app.sync, SyncState::TokenExpired);
        assert!(app.sync.is_error());

        // Saving, while the save is out.
        let engine = app.engine.as_mut().unwrap();
        let _ = engine.begin(Kind::Sync);
        let outcome = engine.answer(Answer::Raw(Ok(None)), &library);
        assert!(matches!(outcome.request, Some(Request::Put { .. })));
        app.refresh_sync_state();
        assert_eq!(app.sync, SyncState::Saving { changes: 1 });

        // A token that expires soon shows while nothing is wrong.
        let engine = app.engine.as_mut().unwrap();
        let _ = engine.answer(
            Answer::Put(Ok(github::PutResult {
                blob_sha: "s".into(),
                commit_sha: "c".into(),
            })),
            &library,
        );
        app.connection.as_mut().unwrap().token_expires =
            Some(Timestamp(Timestamp::now().0 + 3 * 86_400));
        app.refresh_sync_state();
        assert_eq!(app.sync, SyncState::ExpiresIn(3));
        assert_eq!(app.token_expires_in(), Some(3));
        assert!(app.synced_label().starts_with("Synced just now"));

        // The warning shows once a day.
        app.warn_about_the_token();
        assert!(app.token_warning);
        app.token_warning = false;
        app.warn_about_the_token();
        assert!(!app.token_warning, "already warned today");
    }

    #[test]
    fn without_a_token_libri_opens_the_setup_with_the_repo_prefilled() {
        let mut app = App::new(Screen::Library);
        app.mock = false;
        app.connection = Some(Connection::new(target()));
        let _ = app.update(Message::Sync(SyncMessage::TokenLoaded(Ok(None))));
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.step, setup::Step::Connect);
        assert_eq!(app.setup.repo, "ekmanch/libri-scratch");
        assert!(!app.setup.mock);
        assert!(app.source.is_none());
    }

    #[test]
    fn disconnecting_keeps_the_base_with_the_library_and_drops_it_otherwise() {
        let mut app = App::new(Screen::Settings);
        connected(&mut app);
        app.apply(rate("movies/the-mummy-1999", Some(3)));
        let _ = app.disconnect(true);
        assert!(app.engine.is_none() && app.github.is_none());
        assert!(app.connection.as_ref().unwrap().local);
        assert!(app.save_due.is_none());
        assert!(!app.repo.library().entries.is_empty());

        connected(&mut app);
        let _ = app.disconnect(false);
        assert!(app.connection.is_none());
        assert!(app.repo.library().entries.is_empty());
    }
}
