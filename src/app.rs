//! Application state, messages, `update` and `view`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::theme::Mode as ThemeMode;
use iced::widget::{operation, scrollable, text_editor};
use iced::window::{self, settings::PlatformSpecific};
use iced::{Element, Event, Size, Subscription, Task, event, mouse, time};

use crate::animation;
use crate::artwork::{self, Artwork};
use crate::clock::Timestamp;
use crate::connection::{Connection, ConnectionFile, Target};
use crate::covers::{self, Covers};
use crate::decoration::Decoration;
use crate::discover;
use crate::follows::Follows;
use crate::github;
use crate::history::{Direction, History, Place};
use crate::keep;
use crate::keymap::{Action, Combo, Keymap};
use crate::lookup::{self, Service};
use crate::model::{Date, codec};
use crate::preferences::{Preferences, PreferencesFile};
use crate::repo::Operation;
use crate::repo::{FixtureLibraryRepository, LibraryRepository, LocalLibraryRepository};
use crate::screens::{self, Screen, entry, following, library, plans, setup, token};
use crate::settings::{Settings, Source, SyncState, ThemeChoice};
use crate::shared;
use crate::storage;
use crate::sync::{self, Answer, Base, Engine, Kind, Outcome, Phase, Request};
use crate::tags::{self, PickerState};
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
    let window = window_settings();
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::iced_theme)
        .subscription(App::subscription)
        .settings(settings())
        .window(window)
        .run()
}

fn window_settings() -> window::Settings {
    window::Settings {
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
        // A close comes to `App::update` as a message, so `quit` can run.
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

/// How long a closing app may take to shut down before the process ends
/// anyway. Well under the seconds after which KWin calls a window that
/// does not answer "not responding".
const EXIT_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

/// How often the artists are gone through for one with no photo to ask
/// Apple about, while nobody is being asked.
const PHOTOS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// How long after typing started a search a key no widget took still
/// goes after it: the search field has the cursor only once iced has run
/// the focus task, and two keys can come in one batch of events.
const TYPED_ON: std::time::Duration = std::time::Duration::from_millis(300);

/// Closes the app without waiting for the network. Dropping tokio's
/// runtime waits for every blocking call still running, up to
/// `lookup::http::TIMEOUT` a request, with the main thread blocked. So the
/// daily check is told to stop, and if the normal exit has not finished
/// after `EXIT_GRACE` the process ends where it stands. Nothing is lost:
/// the library and `pending.json` are on disk before a save is sent, and
/// the next start settles a save cut off on its way (`sync::Engine`).
fn quit() -> Task<Message> {
    Task::future(async {
        discover::cancel();
        lookup::apple_artists::cancel();
        std::thread::spawn(|| {
            std::thread::sleep(EXIT_GRACE);
            std::process::exit(0);
        });
    })
    .discard()
    .chain(iced::exit())
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
    /// When typing in the window last started a search, for the key
    /// pressed before the search field has the cursor (`TYPED_ON`).
    typed_at: Option<Instant>,
    /// The keyboard shortcuts, as Settings › Keyboard and mouse set them.
    pub keymap: Keymap,
    pub repo: Box<dyn LibraryRepository>,
    pub library: library::State,
    /// The details pane's note as its popover edits it (the text lives
    /// here, since the editor's content is not `Clone`), and the count of
    /// edits, so only the pause after the last one saves.
    pub note_editor: text_editor::Content,
    pub note_generation: u64,
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
    /// Plasma's animation speed, read from `kdeglobals` at the start;
    /// instant in the tests and `App::new`.
    pub speed: animation::Speed,
    /// Device pixels per logical pixel (2 at 200 %), asked when the window
    /// opens and kept up with `Rescaled`: a slide lands on whole device
    /// pixels, so its text and covers stay sharp on the way.
    pub scale: f32,
    /// The window's logical size, for layouts that give way when narrow.
    pub window: Size,
    /// The places visited, for Back and Forward.
    pub history: History,
    /// KWin's title bar, once the window is open on Plasma Wayland.
    pub decoration: Option<Box<Decoration>>,
    /// A lookup state forced for a look (`LIBRI_LOOKUP`, the captures);
    /// `Results` means the real one shows.
    pub lookup: lookup::LookupState,
    /// The lookup providers: the real ones with this run's keys, or the
    /// fixture catalog in a mock run and the tests.
    pub lookups: Arc<Service>,
    /// Covers, artist photos, episodes, series parts and track lists as
    /// fetched for the screens, with the cache under `~/.cache/libri/`.
    pub artwork: Artwork,
    /// The pictures kept decoded for the list on show and the lists
    /// beside it (`keep`).
    pub kept: keep::Kept,
    /// The owner's own covers in the library repo: the files GitHub
    /// lists under `covers/`, and the copies on this computer.
    pub covers: Covers,
    /// The artists' files (paths under `covers/`) whose picked picture a
    /// push dropped, since another device had put one there first; the
    /// artist's viewer says so once.
    pub photos_dropped: Vec<String>,
    /// The artist photos' fetching: who is being asked about, and the
    /// batch held for its commit (`covers::backfill`).
    pub photos: covers::backfill::State,
    /// Whether `photo_size` and `picked` are written to `_missing.json`
    /// (`covers::artists::WRITES_PHOTO_KEYS`; the tests turn it on).
    pub photo_keys: bool,
    /// Apple Music, for the artists' photos.
    pub apple: Arc<lookup::apple_artists::AppleArtists>,
    /// When the artists are next gone through for one to ask about.
    photos_due: Instant,
    /// Discover: what the daily check found for the Coming soon page, and
    /// the items marked Not interested.
    pub discover: discover::State,
    /// `settings.json` beside the library: the settings the phone shares,
    /// and what Discover follows.
    pub shared: shared::Store,
    /// What Discover follows, derived from the library, the settings and
    /// the check's creator records; made again when one of them changes
    /// (`refresh_follows`), since the Coming soon header counts it on
    /// every frame.
    pub follows: Follows,
    /// The Following window, over Settings or Coming soon.
    pub following: Option<following::State>,
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
    /// Counts the connections made, so an answer that comes back for a
    /// request of an earlier engine (a reconnect while a call was out) is
    /// dropped instead of taken for the new engine's.
    pub engine_generation: u64,
    /// When the gathered edits go out as one commit.
    pub save_due: Option<Instant>,
    /// Whether the window has keyboard focus: GitHub is asked for changes
    /// more often while it does.
    pub focused: bool,
    /// The splash while the first download of a fresh connection runs.
    pub loading: bool,
    /// Fake sync states and fixture lists in place of GitHub: every test,
    /// and a run with a screen switch (`LIBRI_SCREEN`, `LIBRI_SYNC`,
    /// `LIBRI_SETUP`, `LIBRI_FIRST_RUN`, `LIBRI_LOCAL`).
    pub mock: bool,
    /// When the message being handled arrived (`update_at`), so a slide
    /// it starts is timed from it.
    now: Instant,
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
    /// A key press no widget took, turned into a shortcut by the keymap
    /// (`App::shortcut`), or into a new shortcut while Settings records
    /// one. With the text the press types, if any: a plain key that is
    /// no shortcut starts a search with it (`App::typed`).
    KeyPressed(Key, keyboard::Modifiers, Option<String>),
    Modifiers(keyboard::Modifiers),
    /// The mouse's Back or Forward button, or Alt+Left and Alt+Right.
    History(Direction),
    /// A frame of the splash's bar.
    Tick,
    /// A frame while the details drawer slides.
    Frame(Instant),
    /// The renderer's allocations of pictures asked to stay decoded.
    Kept(keep::Batch),
    Resized(Size),
    /// The window's events: focus and the close request.
    Window(window::Event),
    /// The window's scale factor, asked when it opens.
    ScaleFactor(f32),
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
    /// A lookup task came back: a cover, an image, a season's episodes.
    Lookup(artwork::Message),
    /// The daily check behind Coming soon's Discover.
    Discover(discover::Message),
    /// GitHub's `settings.json` came, for the connection and the state of
    /// the settings it was asked in.
    SharedFetched(Box<github::Reply<Option<github::RawFile>>>, u64, u64),
    Following(following::Message),
    /// The owner's own covers: GitHub's list of them, and a file fetched.
    Covers(CoversMessage),
    /// Apple's answer about an artist's photo (`photos_next`).
    PhotoFound {
        work: covers::backfill::Work,
        result: Result<lookup::apple_artists::Found, lookup::Failure>,
    },
    /// The messages of the commits that touched the artists' photos,
    /// newest first, for the sharper photos' `picked` list.
    PhotoHistory(Result<Vec<String>, String>),
    /// Apple's sharper copy of an artist's photo; none when there is
    /// nothing to replace it with.
    PhotoSharper {
        work: covers::backfill::Work,
        result: Result<Option<Vec<u8>>, lookup::Failure>,
    },
}

#[derive(Debug, Clone)]
pub enum CoversMessage {
    /// The files under `covers/`, for the engine of that generation.
    Listed(Result<Vec<covers::File>, String>, u64),
    /// A cover file came from GitHub: its blob sha.
    Fetched {
        path: String,
        outcome: Result<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncMessage {
    /// "Could not save": try the save again.
    Retry,
    /// "Library not found": Settings › Library source.
    Fix,
    OpenConflict,
    CloseConflict,
    /// This computer's copy replaces GitHub's; also the way out of a
    /// file on GitHub that cannot be read.
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
    /// A GitHub call came back, for the engine of that generation.
    Answer(Box<Answer>, u64, Vec<sync::commit::Dropped>),
}

impl Message {
    /// Whether the message takes the details pane away from the note being
    /// typed there, so the note is saved first: another screen, a step
    /// back or forward, anything in the library or on a plan page but the
    /// popovers and the list's own measurements, the selection keys, and
    /// the dialogs from the pane.
    fn leaves_note(&self) -> bool {
        matches!(self, Message::Plans(_)) || self.leaves_pane()
    }

    /// The same for the plan note typed in the pane on a plan page: the
    /// plan's own typing, calendar and popovers stay.
    fn leaves_plan_note(&self) -> bool {
        use plans::Message as P;
        match self {
            Message::Plans(message) => !matches!(
                message,
                P::ItemNote(_)
                    | P::SaveItemNote
                    | P::ToggleWhen
                    | P::Month(_)
                    | P::TogglePopup(_)
                    | P::ClosePopup
                    | P::TogglePanel
            ),
            _ => self.leaves_pane(),
        }
    }

    fn leaves_pane(&self) -> bool {
        use entry::Message as E;
        use library::Message as L;
        match self {
            Message::Show(_) | Message::History(_) => true,
            Message::Library(message) => !matches!(
                message,
                L::Scrolled(_)
                    | L::HomeScrolled(_)
                    | L::GroupsMeasured(_)
                    | L::TogglePanel
                    | L::AnotherPick
                    | L::Toggle(_)
                    | L::ClosePopup
                    | L::SeasonMenu(_)
                    | L::Picker(_)
                    | L::Search(_)
                    | L::Typed(_)
                    | L::ScrollList(_)
                    | L::JumpToSection { .. }
                    | L::RailReleased
                    | L::SoonSlice(_)
                    | L::SoonStep(_)
                    | L::Wheel(_)
                    | L::SoonType(_)
                    | L::SoonToggleType(_)
                    | L::SoonAllTypes
                    | L::ShowHidden
                    | L::RestoreFound(_)
                    | L::OpenTypePop
                    | L::CloseTypePop
                    | L::Pointer(_)
                    | L::MenuSub(_)
                    | L::CloseMenu
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
                    | L::ShowSoon
                    | L::OpenSoonForTypes
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
    /// Backspace with no text field taking it: back from the tag picker's
    /// long group.
    Backspace,
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
    /// The Left and Right keys: the cover viewer's ‹ and › through a
    /// series' covers.
    Left,
    Right,
    /// E: edits the selected entry.
    Edit,
    /// The Menu key or Shift+F10: the selected title's right-click menu.
    Menu,
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
        app.speed = animation::Speed::from_plasma();
        // A run that looks at a screen or a state keeps the fixture and
        // the fake states; otherwise the connection on this computer is
        // real.
        app.mock = [
            "LIBRI_SCREEN",
            "LIBRI_SYNC",
            "LIBRI_SETUP",
            "LIBRI_FIRST_RUN",
            "LIBRI_LOCAL",
            "LIBRI_SOON",
            "LIBRI_DISCOVER",
        ]
        .iter()
        .any(|name| var(name).is_some());
        let token = app.open_files();
        let mut lookups = Task::none();
        if app.mock {
            // The mockup's files, so its cover boards have something to
            // show.
            app.covers = Covers::with_files(covers::demo());
        }
        if !app.mock {
            let cache = storage::cache_dir();
            if let Some(dir) = &cache
                && let Err(error) = std::fs::create_dir_all(dir)
            {
                eprintln!("libri: no cache folder at {}: {error}", dir.display());
            }
            let keys = lookup::Keys::load();
            if !keys.any() {
                eprintln!(
                    "libri: no API keys (secrets.json), so movies, shows and games cannot be looked up"
                );
            }
            app.lookups = Arc::new(Service::real(&keys, cache.as_deref()));
            app.discover = discover::State::at(cache.as_deref());
            if let Some(dir) = storage::data_dir() {
                app.shared = shared::Store::at(&dir);
            }
            if app.shared.on_disk() {
                app.shared_changed();
            } else {
                // No `settings.json` yet: the three settings that used to
                // be in the preferences start it. GitHub's copy replaces
                // them when it comes; a library kept on this computer has
                // only this file.
                let settings = &mut app.shared.settings;
                settings.timeline = app.settings.timeline;
                settings.seasonal = app.settings.seasonal;
                settings.platform = app.settings.platform.clone();
                if app.kept_locally {
                    app.shared.save();
                }
            }
            app.covers = Covers::at(
                storage::data_dir().map(|dir| dir.join(covers::FOLDER)),
                cache.as_deref(),
            );
            app.photos = covers::backfill::State::at(storage::data_dir().as_deref());
            app.apple = Arc::new(lookup::apple_artists::AppleArtists::at(cache.as_deref()));
            if app.kept_locally {
                app.covers.scan();
            }
            app.artwork = Artwork::at(cache);
            // Once per session, a little after the start: the records and
            // files of entries that are gone, and the daily check behind
            // Discover unless it ran today.
            lookups = Task::batch([
                Task::perform(
                    async { tokio::time::sleep(artwork::PRUNE_DELAY).await },
                    |_| Message::Lookup(artwork::Message::Prune),
                ),
                Task::perform(async { tokio::time::sleep(DISCOVER_DELAY).await }, |_| {
                    Message::Discover(discover::Message::Check)
                }),
            ]);
        }
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
        // `LIBRI_SOON=library|discover|all`: the Coming soon page on that
        // slice, with the fake's Discover items.
        if let Some(slice) = var("LIBRI_SOON") {
            app.library.update(library::Message::ShowSoon);
            let slice = match slice.as_str() {
                "discover" => library::Slice::Discover,
                "all" => library::Slice::All,
                _ => library::Slice::Library,
            };
            app.library.update(library::Message::SoonSlice(slice));
        }
        if app.mock {
            // The fake answers at once, so the mock screens have Discover
            // rows to look at.
            let _ = app.check_discover(false);
        }
        // `LIBRI_DISCOVER=checking|off`: Settings' Discover section while
        // a check runs (the canvas's numbers), or switched off.
        match var("LIBRI_DISCOVER").as_deref() {
            Some("checking") => app.discover.look_checking(184, 423),
            Some("off") => {
                app.shared.settings.enabled = false;
                app.shared_changed();
            }
            _ => {}
        }
        app.photos.look =
            var("LIBRI_PHOTOS").and_then(|value| covers::backfill::Look::from_env(&value));
        app.artwork.forced_tracks =
            var("LIBRI_TRACKS").and_then(|value| artwork::ForcedTracks::from_env(&value));
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
            lookups,
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
        let mut app = App {
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
            typed_at: None,
            keymap: Keymap::default(),
            repo: Box::new(repo),
            library: library::State::default(),
            note_editor: text_editor::Content::new(),
            note_generation: 0,
            dialog: None,
            plans: plans::State::default(),
            plan_dialog: None,
            settings_page: screens::settings::State::default(),
            splash_phase: 0.0,
            speed: animation::Speed::INSTANT,
            scale: 1.0,
            window: WINDOW_SIZE,
            history: History::default(),
            decoration: None,
            lookup: lookup::LookupState::Results,
            lookups: Arc::new(Service::fake()),
            artwork: Artwork::default(),
            kept: keep::Kept::default(),
            covers: Covers::default(),
            photos_dropped: Vec::new(),
            photos: covers::backfill::State::default(),
            photo_keys: covers::artists::WRITES_PHOTO_KEYS,
            apple: Arc::default(),
            photos_due: Instant::now(),
            discover: discover::State::default(),
            shared: shared::Store::default(),
            follows: Follows::default(),
            following: None,
            preferences: PreferencesFile::default(),
            connection_file: ConnectionFile::default(),
            connection: None,
            token: None,
            github: None,
            engine: None,
            engine_generation: 0,
            save_due: None,
            focused: true,
            loading: false,
            mock: true,
            now: std::time::Instant::now(),
        };
        app.refresh_follows();
        app
    }

    /// Applies one change to the library. A change that cannot apply (an entry gone meanwhile) or
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
        if let Err(error) = self.repo.apply(operation) {
            eprintln!("libri: change not applied ({summary}): {error}");
        }
        self.artwork.library_changed();
        self.refresh_follows();
        if self.engine.is_some() {
            self.save_due = Some(Instant::now() + sync::DEBOUNCE);
            self.refresh_sync_state();
        }
    }

    // `settings.json` beside the library (`shared`): what the phone and
    // the desktop agree on, and what Discover follows.

    /// What Discover follows, made again from the library, the settings
    /// and the check's record of who is no longer active.
    pub fn refresh_follows(&mut self) {
        self.follows = Follows::derive(
            self.repo.library(),
            &self.shared.settings,
            &self.discover.record().creators,
            self.lookups.is_instant(),
        );
    }

    /// The settings changed: Settings' own three follow them, and the
    /// follows are counted again.
    fn shared_changed(&mut self) {
        let shared = &self.shared.settings;
        self.settings.timeline = shared.timeline;
        self.settings.seasonal = shared.seasonal;
        self.settings.platform = shared.platform.clone();
        self.refresh_follows();
        // A previewed title that an unfollow took off the page closes.
        if let Some(key) = &self.library.found {
            let (visible, hidden) = self
                .discover
                .lists(self.repo.library(), &self.shared.settings);
            if !visible.iter().chain(&hidden).any(|item| item.key == *key) {
                self.library.found = None;
            }
        }
    }

    /// Makes a change to `settings.json`: on this computer at once, on
    /// GitHub with the next commit, where it is merged into the file as
    /// it is then. `summary` names a commit it makes on its own.
    pub fn change_shared(&mut self, edit: shared::Edit, summary: String) {
        if !self.shared.apply(&edit) {
            return;
        }
        self.shared_changed();
        if let Some(engine) = &mut self.engine {
            engine.queue_file(covers::Change::Settings(edit), Some(summary));
            self.save_due = Some(Instant::now() + sync::DEBOUNCE);
            self.refresh_sync_state();
        }
    }

    /// The changes to `settings.json` still waiting for their commit.
    fn queued_shared_edits(&self) -> Vec<shared::Edit> {
        let Some(engine) = &self.engine else {
            return Vec::new();
        };
        let changes = engine.pending().changes();
        changes
            .into_iter()
            .filter_map(|change| match change {
                covers::Change::Settings(edit) => Some(edit),
                _ => None,
            })
            .collect()
    }

    /// Asks GitHub for `settings.json`, conditionally, unless it is being
    /// asked: at the start and after every round of the sync, so a change
    /// made on the phone shows here as soon as one to the library would.
    fn fetch_shared(&mut self) -> Task<Message> {
        let (Some(client), Some(engine)) = (&self.github, &self.engine) else {
            return Task::none();
        };
        if self.shared.fetching {
            return Task::none();
        }
        self.shared.fetching = true;
        let (client, target) = (client.clone(), engine.target().clone());
        let (generation, epoch) = (self.engine_generation, self.shared.epoch);
        let etag = self.shared.etag.clone();
        Task::perform(
            blocking(move || {
                let path = shared::path_beside(&target.file);
                Ok(client.file_raw(&path, &target.branch, etag.as_deref()))
            }),
            move |result| {
                let reply = result.unwrap_or_else(|error| {
                    Err(github::Failure::new(github::FailureKind::Other, error))
                });
                Message::SharedFetched(Box::new(reply), generation, epoch)
            },
        )
    }

    /// GitHub's `settings.json` came: the settings are that copy with the
    /// changes still queued on top. No file there is the defaults; one of
    /// another version, or an answer that failed, leaves them as they are.
    fn shared_fetched(
        &mut self,
        reply: github::Reply<Option<github::RawFile>>,
        epoch: u64,
    ) -> Task<Message> {
        self.shared.fetching = false;
        // A change was made here or a commit landed while it was out: the
        // answer may be the file from before, so it is asked again.
        if epoch != self.shared.epoch {
            return self.fetch_shared();
        }
        let known = self.shared.known;
        let queued = self.queued_shared_edits();
        let taken = match reply {
            Ok(None) => {
                self.shared.known = true;
                Ok(false)
            }
            Ok(Some(raw)) => {
                let text = raw.text().unwrap_or("not UTF-8");
                let taken = self.shared.take_remote(Some(text), &queued);
                if taken.is_ok() {
                    self.shared.etag = raw.etag.clone();
                }
                taken
            }
            Err(failure) if failure.kind == github::FailureKind::NotFound => {
                self.shared.etag = None;
                self.shared.take_remote(None, &queued)
            }
            Err(failure) => Err(failure.describe()),
        };
        match taken {
            Ok(true) => self.shared_changed(),
            Ok(false) => {}
            Err(error) => eprintln!("libri: settings.json on GitHub was not read: {error}"),
        }
        // The daily check waited for what to follow.
        if !known && self.shared.known {
            return self.check_discover(false);
        }
        Task::none()
    }

    // GitHub sync (phase 3): the engine decides, the app runs each call
    // as a Task and hands the answer back.

    /// The repository just set up (or reconnected): the connection goes to
    /// `connection.json`, the token to KDE Wallet, and the first sync
    /// starts. With `base` the local library is what the setup just put
    /// on GitHub; without one GitHub's copy is downloaded behind the
    /// splash.
    ///
    /// A reconnect while a repository was connected (Settings › Change
    /// repository or file…, Pick a file) keeps the engine and its queue
    /// when the file is the same one, so edits made while it was gone go
    /// out as before once it is back. For another file the queue goes:
    /// its edits are in the library this computer uploaded over the file
    /// ("Which library?" › This computer's) or gave up (The one on
    /// GitHub), as on the phone.
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
        let mut engine = match self.engine.take() {
            Some(mut engine) if *engine.target() == target => {
                engine.restart();
                engine
            }
            Some(mut engine) => {
                engine.discard(true);
                Engine::open(self.data_dir().as_deref(), target)
            }
            None => Engine::open(self.data_dir().as_deref(), target),
        };
        self.engine_generation += 1;
        if let Some(base) = base {
            engine.set_base(base);
        }
        self.loading = !engine.has_base();
        self.engine = Some(engine);
        self.save_due = None;
        self.conflict_open = false;
        self.refresh_sync_state();
        self.screen = if self.loading {
            Screen::Splash
        } else {
            Screen::Library
        };
        let covers = self.list_covers();
        self.shared.forget_remote();
        let shared = self.fetch_shared();
        Task::batch([store, self.sync_now(), covers, shared])
    }

    /// The token from KDE Wallet at the start: the client and the engine
    /// for the saved connection, and the first check (which warns about
    /// the token if that is due). A fresh connection without a base downloads GitHub's
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
        let engine = Engine::open(self.data_dir().as_deref(), target);
        self.loading = !engine.has_base();
        self.engine = Some(engine);
        self.engine_generation += 1;
        self.refresh_sync_state();
        if self.loading {
            self.screen = Screen::Splash;
        }
        let covers = self.list_covers();
        self.shared.forget_remote();
        let shared = self.fetch_shared();
        Task::batch([self.sync_now(), covers, shared])
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

    /// Where the sync files live: next to the library, or nowhere in a
    /// mock run, whose engine keeps everything in memory.
    fn data_dir(&self) -> Option<std::path::PathBuf> {
        if self.mock { None } else { storage::data_dir() }
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
        self.shared.forget_remote();
        // Kept on this computer, the settings here are all there is.
        self.shared.known = true;
        self.save_due = None;
        self.loading = false;
        self.sync = SyncState::Synced;
        self.conflict_open = false;
        self.token_warning = false;
        self.artwork.forget_repo_images();
        if keep {
            // The covers fetched so far stay with the library; the
            // folder here is the whole list from now on.
            self.covers.clear(false);
            if !self.mock {
                self.covers.scan();
            }
        } else {
            self.covers.clear(true);
        }
        // A batch held for a commit has no repo to go to any more; its
        // photos stay in the folder when the library is kept.
        self.photos.clear();
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
            self.refresh_follows();
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
    /// back: a round without waiting for the debounce. The token warning
    /// is checked with every round, so a Libri left open past midnight
    /// warns on the day's first check instead of waiting for the next
    /// start; not over the setup, where a token may be on its way in.
    pub fn sync_now(&mut self) -> Task<Message> {
        self.save_due = None;
        if self.screen != Screen::Setup {
            self.warn_about_the_token();
        }
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
        let mut listing = Task::none();
        if let Some(library) = outcome.library {
            self.repo.replace(library);
            self.artwork.library_changed();
            self.refresh_follows();
            // GitHub's copy came: its covers may have changed with it.
            listing = self.list_covers();
        }
        let (Some(request), Some(client), Some(engine)) =
            (outcome.request, &self.github, &self.engine)
        else {
            // The first download is over (or failed): the library shows.
            if self.loading && !self.engine.as_ref().is_some_and(Engine::running) {
                self.loading = false;
                self.screen = Screen::Library;
            }
            return listing;
        };
        let (client, target) = (client.clone(), engine.target().clone());
        let generation = self.engine_generation;
        let covers_dir = self.covers_dir();
        let call = Task::perform(
            blocking(move || Ok(perform(&client, &target, covers_dir.as_deref(), request))),
            move |result| {
                let (answer, dropped) = result.unwrap_or_else(|error| {
                    let failure = github::Failure::new(github::FailureKind::Other, error);
                    (Answer::Raw(Err(failure)), Vec::new())
                });
                Message::Sync(SyncMessage::Answer(Box::new(answer), generation, dropped))
            },
        );
        Task::batch([call, listing])
    }

    // The owner's own covers (`covers`): files in the library repo.

    /// `covers/` beside the library on this computer; nowhere in a mock
    /// run.
    fn covers_dir(&self) -> Option<std::path::PathBuf> {
        self.data_dir().map(|dir| dir.join(covers::FOLDER))
    }

    /// Asks GitHub for the files under `covers/`, one tree call, unless a
    /// listing is on its way. With the library kept on this computer the
    /// folder here is read instead.
    pub fn list_covers(&mut self) -> Task<Message> {
        let (Some(client), Some(engine)) = (&self.github, &self.engine) else {
            if self.kept_locally && !self.mock {
                self.covers.scan();
                self.artwork.forget_repo_images();
            }
            return Task::none();
        };
        if self.covers.listing {
            return Task::none();
        }
        self.covers.listing = true;
        let (client, target) = (client.clone(), engine.target().clone());
        let generation = self.engine_generation;
        Task::perform(
            blocking(move || {
                covers::list(&client, &target.branch, &target.file).map_err(|f| f.to_string())
            }),
            move |result| Message::Covers(CoversMessage::Listed(result, generation)),
        )
    }

    /// The list again when Pick a cover opens on one that is a while
    /// old, so a file committed by hand meanwhile is offered.
    pub fn list_covers_if_stale(&mut self) -> Task<Message> {
        match self.covers.listed {
            Some(at) if at.elapsed() < covers::LISTING_AGE => Task::none(),
            _ => self.list_covers(),
        }
    }

    fn covers_update(&mut self, message: CoversMessage) -> Task<Message> {
        match message {
            CoversMessage::Listed(result, generation) => {
                if generation != self.engine_generation {
                    self.covers.listing = false;
                    return Task::none();
                }
                match result {
                    Ok(files) => {
                        let queued = self.queued_cover_changes();
                        self.covers.listed(files, &queued);
                        // Each cover on screen is looked for again: a
                        // file replaced on GitHub is fetched anew.
                        self.artwork.forget_repo_images();
                        // The artists tried with nothing found, when
                        // GitHub's record is not the copy here.
                        return self.download_covers(&[covers::artists::missing_path()]);
                    }
                    Err(error) => {
                        self.covers.listing = false;
                        eprintln!("libri: could not list the covers: {error}");
                    }
                }
            }
            CoversMessage::Fetched { path, outcome } => {
                self.covers.fetched(&path, &outcome);
                if path == covers::artists::missing_path() {
                    let queued = self.queued_cover_changes();
                    self.covers.load_missing();
                    self.covers.rebuild(&queued);
                    return Task::none();
                }
                let url = covers::cover_from(&path);
                match self.covers.ready(&path) {
                    Some(local) => self.artwork.set_image(&url, local),
                    None => self.artwork.set_image_failed(&url),
                }
            }
        }
        Task::none()
    }

    /// Hands the owner's covers the screens asked for to `artwork`: the
    /// copies on this computer at once, the others fetched from GitHub.
    fn fetch_covers(&mut self) -> Task<Message> {
        let wanted = self.artwork.take_repo_wanted();
        if wanted.is_empty() {
            return Task::none();
        }
        let mut missing = Vec::new();
        for url in wanted {
            let Some(path) = covers::repo_path(&url) else {
                continue;
            };
            match self.covers.ready(path) {
                Some(local) => self.artwork.set_image(&url, local),
                None => missing.push(path.to_owned()),
            }
        }
        self.download_covers(&missing)
    }

    /// The cover file changes waiting for a commit: the queue's, and the
    /// artist photos' batch that is not queued yet.
    fn queued_cover_changes(&self) -> Vec<covers::Change> {
        let mut changes = self
            .engine
            .as_ref()
            .map(|engine| engine.pending().changes())
            .unwrap_or_default();
        changes.extend(self.photos.held().iter().cloned());
        changes
    }

    // The artists' photos from Apple Music (`covers::backfill`,
    // `lookup::apple_artists`; DESIGN.md "Artist photos").

    /// Whether a lookup of the Add and Edit dialog is on its way.
    fn lookup_running(&self) -> bool {
        match &self.dialog {
            Some(entry::Dialog::Form(form) | entry::Dialog::Tags(form)) => {
                form.searching || form.fetching.is_some() || form.choices_loading
            }
            _ => false,
        }
    }

    /// Whether the photo work may ask Apple now: a real run that knows
    /// which files and records GitHub has (or keeps the library here),
    /// not paused, not refused by Apple in this session, not waiting
    /// after a failure.
    fn photos_can_run(&self) -> bool {
        // A forced look (`LIBRI_PHOTOS`) shows a state and asks nobody.
        if self.mock
            || self.loading
            || self.photos.paused
            || self.photos.stopped
            || self.photos.look.is_some()
        {
            return false;
        }
        if self.photos.retry_at.is_some_and(|at| self.now < at) {
            return false;
        }
        if self.engine.is_none() {
            return self.kept_locally;
        }
        // GitHub's list came, and its `_missing.json` is the copy here.
        let record = covers::artists::missing_path();
        self.github.is_some()
            && self.covers.listed.is_some()
            && (self.covers.get(&record).is_none() || self.covers.ready(&record).is_some())
    }

    /// Whether this run fetches photos at all: a real run with a repo to
    /// keep them in, or the library kept on this computer.
    pub fn photos_active(&self) -> bool {
        !self.mock && (self.engine.is_some() || self.kept_locally)
    }

    /// Settings' Artist photos row as it reads now.
    pub fn photos_row(&self) -> covers::backfill::Row {
        let library = self.repo.library();
        let left = covers::backfill::todo(
            library,
            &self.covers,
            self.today,
            self.photos.tried_now(),
            self.photos.again,
        )
        .len();
        let (found, yours) = covers::backfill::sharper_counts(library, &self.covers);
        let sharp = covers::backfill::Sharp {
            // On offer until the run's first batch has written the size.
            offered: self.photo_keys && self.covers.missing().photo_size.is_none(),
            found,
            yours,
            left: covers::backfill::sharper_todo(library, &self.covers, &self.photos).len(),
        };
        covers::backfill::Row::of(
            &self.photos,
            covers::backfill::totals(library, &self.covers),
            left,
            self.photos_active(),
            sharp,
        )
    }

    /// The row's button: Pause, Resume, Try again or Check again, as
    /// the row reads. Pause lets the artist being asked about finish.
    pub fn photos_button(&mut self) -> Task<Message> {
        use covers::backfill::{Look, Row};
        // A forced look only steps through its states.
        if let Some(look) = self.photos.look {
            self.photos.look = match look {
                Look::Fetching => Some(Look::Paused),
                Look::Paused | Look::Stopped => Some(Look::Fetching),
                Look::Sharper => Some(Look::Sharpening),
                Look::Sharpening => Some(Look::Sharper),
            };
            return Task::none();
        }
        match self.photos_row() {
            Row::Fetching { .. } | Row::Checking | Row::Sharpening { .. } => {
                self.photos.paused = true
            }
            Row::Sharper { .. } => self.photos.start_sharper(),
            Row::Paused { .. } => self.photos.paused = false,
            Row::Stopped { .. } => {
                self.photos.stopped = false;
                self.photos.retry_at = None;
            }
            Row::Done(_) => {
                self.photos.check_again();
                self.photos.disagreed.clear();
                self.photos.retry_at = None;
            }
        }
        self.photos.save();
        self.photos_next()
    }

    /// Asks Apple about the next artist with no photo and no fresh
    /// record, one at a time; queues the batch once fifty were tried or
    /// nobody is left.
    fn photos_next(&mut self) -> Task<Message> {
        self.photos_due = self.now + PHOTOS_INTERVAL;
        if self.photos.running.is_some() || !self.photos_can_run() {
            return Task::none();
        }
        let todo = covers::backfill::todo(
            self.repo.library(),
            &self.covers,
            self.today,
            self.photos.tried_now(),
            self.photos.again,
        );
        let Some(work) = todo.into_iter().next() else {
            // Nobody new: the sharper photos, while their run is on.
            if self.photos.sharper.is_some() {
                return self.sharper_next();
            }
            // Done only when nobody was passed over after a failure.
            self.queue_photo_batch(self.photos.none_passed_over());
            self.photos.again = false;
            return Task::none();
        };
        if self.photos.is_full() {
            self.queue_photo_batch(false);
        }
        self.photos.running = Some(work.name.clone());
        let apple = self.apple.clone();
        let asked = work.clone();
        // At the record's size: 1200 once the sharper photos' first
        // batch has written it, 640 before.
        let side = self.covers.missing().photo_side();
        Task::perform(
            blocking(move || Ok(apple.find(&asked.name, &asked.owned, &asked.songs, side))),
            move |result| Message::PhotoFound {
                work: work.clone(),
                result: result.unwrap_or_else(|error| Err(lookup::Failure::other(error))),
            },
        )
    }

    /// The sharper photos' next step: the `picked` list first, from this
    /// computer's memory and the commit history, then one artist at a
    /// time whose photo was found automatically, fifty to a commit.
    fn sharper_next(&mut self) -> Task<Message> {
        let Some(sharper) = &self.photos.sharper else {
            return Task::none();
        };
        if !sharper.checked {
            if self.photos.checking {
                return Task::none();
            }
            let (Some(client), Some(engine)) = (&self.github, &self.engine) else {
                // The library is kept on this computer: no history.
                return self.sharper_checked(None);
            };
            self.photos.checking = true;
            let target = engine.target().clone();
            let client = client.clone();
            let path = format!("{}/{}", covers::root(&target.file), covers::artists::FOLDER);
            return Task::perform(
                blocking(move || {
                    client
                        .commit_messages(&path, &target.branch)
                        .map_err(|failure| failure.describe())
                }),
                Message::PhotoHistory,
            );
        }
        let todo = covers::backfill::sharper_todo(self.repo.library(), &self.covers, &self.photos);
        let Some(work) = todo.into_iter().next() else {
            self.photos.sharper_done();
            self.queue_photo_batch(self.photos.none_passed_over());
            return Task::none();
        };
        if self.photos.is_full() {
            self.queue_photo_batch(false);
        }
        self.photos.running = Some(work.name.clone());
        let apple = self.apple.clone();
        let asked = work.clone();
        Task::perform(
            blocking(move || {
                let side = covers::artists::SHARP_SIDE;
                Ok(apple.sharper(&asked.name, &asked.owned, &asked.songs, side))
            }),
            move |result| Message::PhotoSharper {
                work: work.clone(),
                result: result.unwrap_or_else(|error| Err(lookup::Failure::other(error))),
            },
        )
    }

    /// The commit history came, or did not: without it nothing is
    /// replaced, and the row offers the sharper photos again.
    fn sharper_history(&mut self, result: Result<Vec<String>, String>) -> Task<Message> {
        self.photos.checking = false;
        match result {
            Ok(messages) => self.sharper_checked(Some(messages)),
            Err(error) => {
                eprintln!("libri: the photos' history could not be read: {error}");
                self.photos.sharper = None;
                self.photos.save();
                Task::none()
            }
        }
    }

    /// Builds the record's `picked` list before any photo is replaced:
    /// every artist whose photo this computer remembers as picked or
    /// uploaded, or whose last word in the history is "Set the photo
    /// of", on top of what the record has. The edits and the photo size
    /// lead the first batch.
    fn sharper_checked(&mut self, history: Option<Vec<String>>) -> Task<Message> {
        let (picked, disagreed) =
            covers::backfill::picked_from(self.repo.library(), &self.covers, history.as_deref());
        if !disagreed.is_empty() {
            eprintln!(
                "libri: counted as picked, since this computer and the history disagree: {}",
                disagreed.join(", ")
            );
        }
        let record = self.covers.missing();
        let mut changes: Vec<covers::Change> = picked
            .into_iter()
            .filter(|slug| !record.picked.contains(slug))
            .map(|slug| covers::Change::Missing(covers::artists::MissingEdit::Pick { slug }))
            .collect();
        if record.photo_size.is_none() {
            changes.push(covers::Change::Missing(
                covers::artists::MissingEdit::PhotoSize(covers::artists::SHARP_SIDE),
            ));
        }
        if self.engine.is_none() {
            for change in &changes {
                self.covers.apply(change, true);
            }
            changes.clear();
        }
        self.photos.sharper_checked(changes, 0, disagreed);
        let queued = self.queued_cover_changes();
        self.covers.rebuild(&queued);
        let of =
            covers::backfill::sharper_todo(self.repo.library(), &self.covers, &self.photos).len();
        if let Some(sharper) = &mut self.photos.sharper {
            sharper.of = of;
        }
        self.photos.save();
        self.photos_next()
    }

    /// Apple's sharper copy of an artist's photo came. It takes the
    /// file's place only while the artist still has one that is not the
    /// owner's own, and only when it is larger than the one there.
    fn photo_sharper(
        &mut self,
        work: covers::backfill::Work,
        result: Result<Option<Vec<u8>>, lookup::Failure>,
    ) -> Task<Message> {
        self.photos.running = None;
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(failure) if lookup::apple_artists::refused(&failure) => {
                eprintln!(
                    "libri: Apple stopped answering ({failure}); no more photos this session"
                );
                self.photos.stopped = true;
                return Task::none();
            }
            Err(failure) if failure.kind == lookup::FailureKind::Offline => {
                eprintln!("libri: no answer about {}: {failure}", work.name);
                self.photos.retry_at = Some(self.now + covers::backfill::RETRY);
                return Task::none();
            }
            Err(failure) => {
                eprintln!(
                    "libri: no sharper photo of {} for now: {failure}",
                    work.name
                );
                self.photos.skip_sharper(&work.slug);
                return self.photos_next();
            }
        };
        let path = covers::artists::file_path(&work.name);
        let change = bytes.zip(path).and_then(|(bytes, path)| {
            // Picked, uploaded or removed while Apple was being asked.
            if self.covers.get(&path).is_none() || self.covers.is_picked(&work.slug) {
                return None;
            }
            let side = |size: Option<(u32, u32)>| size.map(|(width, _)| width);
            let new = side(lookup::store::image_size(&bytes))?;
            let old = self
                .covers
                .ready(&path)
                .and_then(|local| side(lookup::store::image_size_of(&local)));
            if new <= old.unwrap_or(covers::artists::PHOTO_SIDE) {
                return None;
            }
            let seen = self.covers.seen(&path, &self.queued_cover_changes());
            if let Err(error) = self.covers.store(&path, &bytes) {
                eprintln!("libri: could not store the photo {path}: {error}");
                return None;
            }
            self.covers.set_kind(&path, covers::PhotoKind::Fetched);
            self.artwork.forget_image(&covers::cover_from(&path));
            Some(covers::Change::Sharpen { path, seen })
        });
        if self.engine.is_none() {
            // The library is kept on this computer: made at once.
            if let Some(change) = &change {
                self.covers.apply(change, true);
            }
            self.photos.sharpened(&work.slug, None);
        } else {
            self.photos.sharpened(&work.slug, change);
            let queued = self.queued_cover_changes();
            self.covers.rebuild(&queued);
        }
        self.photos_next()
    }

    /// Hands the held batch to the queue as one commit's worth. `done`
    /// says nobody is left to ask: the backfill's day is written then,
    /// once, for the phone to see.
    fn queue_photo_batch(&mut self, done: bool) {
        if self.engine.is_none() {
            return;
        }
        let day = (done && self.covers.missing().backfilled.is_none()).then_some(self.today);
        let Some((changes, summary)) = self.photos.take(day) else {
            return;
        };
        if let Some(engine) = &mut self.engine {
            for change in changes {
                engine.queue_file(change, Some(summary.clone()));
            }
        }
        let queued = self.queued_cover_changes();
        self.covers.rebuild(&queued);
        self.save_due = Some(Instant::now() + sync::DEBOUNCE);
        self.refresh_sync_state();
    }

    /// Apple answered about an artist. A photo is stored and shown at
    /// once; "nothing found" is recorded; both wait in the batch. A
    /// refusal stops the work for the session, and any other failure
    /// (offline, a server error) is not an answer: the artist is asked
    /// again later.
    fn photo_found(
        &mut self,
        work: covers::backfill::Work,
        result: Result<lookup::apple_artists::Found, lookup::Failure>,
    ) -> Task<Message> {
        use lookup::apple_artists::Found;
        self.photos.running = None;
        let local = self.engine.is_none();
        let change = match result {
            Ok(Found::Photo(bytes)) => {
                let Some(path) = covers::artists::file_path(&work.name) else {
                    return Task::none();
                };
                // Picked or uploaded while Apple was being asked: the
                // owner's stands.
                if self.covers.get(&path).is_some() {
                    self.photos.asked(&work.slug);
                    return self.photos_next();
                }
                let seen = self.covers.seen(&path, &self.queued_cover_changes());
                if let Err(error) = self.covers.store(&path, &bytes) {
                    eprintln!("libri: could not store the photo {path}: {error}");
                    self.photos.retry_at = Some(self.now + covers::backfill::RETRY);
                    return Task::none();
                }
                self.covers.set_kind(&path, covers::PhotoKind::Fetched);
                self.artwork.forget_image(&covers::cover_from(&path));
                covers::Change::Put {
                    path,
                    seen,
                    picked: false,
                }
            }
            Ok(Found::Missing(reason)) => {
                covers::Change::Missing(covers::artists::MissingEdit::Set {
                    slug: work.slug.clone(),
                    tried: covers::artists::Tried {
                        name: work.name.clone(),
                        tried: self.today,
                        reason,
                    },
                })
            }
            Err(failure) if lookup::apple_artists::refused(&failure) => {
                eprintln!(
                    "libri: Apple stopped answering ({failure}); no more photos this session"
                );
                self.photos.stopped = true;
                return Task::none();
            }
            Err(failure) if failure.kind == lookup::FailureKind::Offline => {
                eprintln!("libri: no answer about {}: {failure}", work.name);
                self.photos.retry_at = Some(self.now + covers::backfill::RETRY);
                return Task::none();
            }
            Err(failure) => {
                // Not an answer and not Apple refusing: this artist waits
                // for the next session and the others go on.
                eprintln!("libri: no photo of {} for now: {failure}", work.name);
                self.photos.pass_over(&work.slug);
                return self.photos_next();
            }
        };
        if local {
            // The library is kept on this computer: made at once.
            self.covers.apply(&change, true);
            self.photos.asked(&work.slug);
        } else {
            self.photos.tried(&work.slug, change);
            let queued = self.queued_cover_changes();
            self.covers.rebuild(&queued);
        }
        self.photos_next()
    }

    /// Fetches the files at `paths` under `covers/` that GitHub lists and
    /// this computer lacks.
    fn download_covers(&mut self, paths: &[String]) -> Task<Message> {
        let (Some(client), Some(engine)) = (&self.github, &self.engine) else {
            return Task::none();
        };
        let target = engine.target().clone();
        let tasks = self.covers.to_fetch(paths).into_iter().filter_map(|path| {
            let to = self.covers.local_path(&path)?;
            let (client, target) = (client.clone(), target.clone());
            let fetched = path.clone();
            Some(Task::perform(
                blocking(move || {
                    Ok(covers::download(
                        &client,
                        &target.branch,
                        &target.file,
                        &fetched,
                        &to,
                    ))
                }),
                move |result| {
                    Message::Covers(CoversMessage::Fetched {
                        path: path.clone(),
                        outcome: result.unwrap_or_else(Err),
                    })
                },
            ))
        });
        Task::batch(tasks.collect::<Vec<_>>())
    }

    /// Stores an upload under `covers/` at `path` and queues it for the
    /// next commit; the screens show it at once.
    pub fn add_cover_file(&mut self, path: &str, bytes: &[u8]) -> Option<covers::File> {
        let file = match self.covers.store(path, bytes) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("libri: could not store the cover {path}: {error}");
                return None;
            }
        };
        let url = file.url();
        self.artwork.forget_image(&url);
        if let Some(local) = self.covers.ready(path) {
            self.artwork.set_image(&url, local);
        }
        if let Some(engine) = &mut self.engine {
            engine.queue_file(
                covers::Change::Upload {
                    path: path.to_owned(),
                },
                None,
            );
            self.save_due = Some(Instant::now() + sync::DEBOUNCE);
            self.refresh_sync_state();
        }
        Some(file)
    }

    /// What a push dropped for GitHub's file: the copies here go, and the
    /// artists whose picture the owner had picked are kept to be told
    /// (`photos_dropped`).
    fn covers_dropped(&mut self, dropped: Vec<sync::commit::Dropped>) {
        for file in dropped {
            self.covers.dropped(&file.path);
            self.artwork.forget_image(&covers::cover_from(&file.path));
            if file.picked && !self.photos_dropped.contains(&file.path) {
                self.photos_dropped.push(file.path);
            }
        }
    }

    /// Saves `bytes` as the artist's photo, over the one there, and
    /// queues it: a JPEG as Apple served it or as an upload was scaled.
    /// `kind` says how it came to be. The artist is no longer missing.
    /// `summary` names a commit the file makes on its own. Nothing for a
    /// name that gets no file.
    pub fn set_artist_photo(
        &mut self,
        name: &str,
        bytes: &[u8],
        kind: covers::PhotoKind,
        summary: Option<String>,
    ) -> Option<covers::File> {
        let path = covers::artists::file_path(name)?;
        let seen = self.covers.seen(&path, &self.queued_cover_changes());
        let file = match self.covers.store(&path, bytes) {
            Ok(file) => file,
            Err(error) => {
                eprintln!("libri: could not store the photo {path}: {error}");
                return None;
            }
        };
        self.photos_dropped.retain(|dropped| *dropped != path);
        self.covers.set_kind(&path, kind);
        let url = file.url();
        self.artwork.forget_image(&url);
        // The owner chose the picture, also when it is Apple's.
        let mut changes = vec![covers::Change::Put {
            path,
            seen,
            picked: true,
        }];
        // The record's `picked` list: on it for a picture picked or
        // uploaded, off it for the suggested photo, which is what the
        // fetch would have stored.
        if let Some(slug) = covers::artists::slug(name).filter(|_| self.photo_keys) {
            use covers::artists::MissingEdit;
            let on_list = self.covers.missing().picked.contains(&slug);
            match kind {
                covers::PhotoKind::Picked | covers::PhotoKind::Uploaded if !on_list => {
                    changes.push(covers::Change::Missing(MissingEdit::Pick { slug }));
                }
                covers::PhotoKind::Fetched if on_list => {
                    changes.push(covers::Change::Missing(MissingEdit::Unpick { slug }));
                }
                _ => {}
            }
        }
        self.change_cover_files(changes, summary);
        Some(file)
    }

    /// Use automatic: the artist's photo is deleted, here at once and on
    /// GitHub with the next commit, while it is still the one seen.
    pub fn remove_artist_photo(&mut self, name: &str, summary: Option<String>) {
        let Some(path) = covers::artists::file_path(name) else {
            return;
        };
        if self.covers.get(&path).is_none() {
            return;
        }
        let seen = self.covers.seen(&path, &self.queued_cover_changes());
        // Off the record's `picked` list first, while the file is there.
        let unpick = covers::artists::slug(name)
            .filter(|slug| self.photo_keys && self.covers.missing().picked.contains(slug))
            .map(|slug| covers::Change::Missing(covers::artists::MissingEdit::Unpick { slug }));
        let mut changes: Vec<covers::Change> = unpick.into_iter().collect();
        changes.push(covers::Change::Remove { path, seen });
        self.change_cover_files(changes, summary);
    }

    /// Deletes or moves cover files: on this computer at once, on GitHub
    /// with the next commit. `summary` names a commit they make on their
    /// own (no library change goes with them).
    pub fn change_cover_files(&mut self, changes: Vec<covers::Change>, summary: Option<String>) {
        if changes.is_empty() {
            return;
        }
        let queued = self.engine.is_some();
        for change in changes {
            match &change {
                covers::Change::Delete { path } => {
                    self.artwork.forget_image(&covers::cover_from(path));
                }
                covers::Change::Move { from, to } => {
                    self.artwork.forget_image(&covers::cover_from(from));
                    self.artwork.forget_image(&covers::cover_from(to));
                }
                covers::Change::Remove { path, .. } => {
                    self.artwork.forget_image(&covers::cover_from(path));
                }
                covers::Change::Upload { .. }
                | covers::Change::Put { .. }
                | covers::Change::Sharpen { .. }
                | covers::Change::Missing(_)
                | covers::Change::Settings(_) => {}
            }
            self.covers.apply(&change, !queued);
            if let Some(engine) = &mut self.engine {
                engine.queue_file(change, summary.clone());
            }
        }
        if queued {
            self.save_due = Some(Instant::now() + sync::DEBOUNCE);
            self.refresh_sync_state();
        }
    }

    /// The engine's status as the sidebar row, the banner and Settings
    /// show it. A mock run keeps the faked state.
    pub fn refresh_sync_state(&mut self) {
        let Some(engine) = &self.engine else {
            return;
        };
        let status = engine.status();
        let remote_error = engine.remote_error().unwrap_or_default().to_owned();
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
            Phase::Unreadable => SyncState::Unreadable {
                kept: n,
                reason: remote_error,
            },
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

    /// Whether the last check is old enough for the window coming back
    /// to ask again: anything but one made in the last few seconds.
    fn check_is_stale(&self) -> bool {
        let Some(engine) = &self.engine else {
            return false;
        };
        match engine.status().last_checked {
            Some(at) => Timestamp::now().0 - at.0 >= sync::FOCUS_CHECK_GUARD.as_secs() as i64,
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
        self.update_at(message, Instant::now())
    }

    /// `update` at a given time: the drawer's slide is timed from `now`.
    pub fn update_at(&mut self, message: Message, now: Instant) -> Task<Message> {
        // Nothing the screens show changes with it.
        let message = match message {
            Message::Kept(batch) => {
                self.kept.arrived(batch);
                return Task::none();
            }
            message => message,
        };
        self.now = now;
        self.library.motion.settle(now);
        self.plans.motion.settle(now);
        if self
            .plans
            .tab_slide
            .as_ref()
            .is_some_and(|moving| moving.done(now))
        {
            self.plans.tab_slide = None;
        }
        if self
            .library
            .slide
            .as_ref()
            .is_some_and(|moving| moving.done(now))
        {
            self.library.slide = None;
        }
        if self
            .library
            .tab_slide
            .as_ref()
            .is_some_and(|moving| moving.done(now))
        {
            self.library.tab_slide = None;
        }
        // The search field has the cursor and takes the keys itself.
        if let Message::Library(library::Message::Search(_)) = &message {
            self.typed_at = None;
        }
        let message = match message {
            Message::KeyPressed(key, modifiers, text) => match self.shortcut(&key, modifiers) {
                Some(message) => {
                    // Shift+arrow over a plan's tabs slides the way the
                    // arrow points, where a click on a segment swaps at
                    // once.
                    let action =
                        Combo::of(&key, modifiers).and_then(|c| self.keymap.action_for(&c));
                    if let (Message::Plans(_), Some(Action::SoonPrev | Action::SoonNext)) =
                        (&message, action)
                    {
                        let back = action == Some(Action::SoonPrev);
                        self.plans.tab_step = Some(if back { -1 } else { 1 });
                    }
                    // And over Music's tabs it slides, where a click on a
                    // segment swaps at once.
                    if let (
                        Message::Library(library::Message::MusicTab(_)),
                        Some(Action::SoonPrev | Action::SoonNext),
                    ) = (&message, action)
                    {
                        self.library.tab_step = true;
                    }
                    message
                }
                None => match self.typed(modifiers, text.as_deref()) {
                    Some(text) => Message::Library(library::Message::Typed(text)),
                    None => return Task::none(),
                },
            },
            // A side-scroll or Shift+wheel over Music's tabs is the tab
            // before or after, as a click on its segment.
            Message::Library(library::Message::Wheel(delta)) if self.music_tabs_free() => {
                let shift = self.modifiers.shift();
                match self.library.wheel.feed(delta, shift, now) {
                    Some(step) => {
                        self.library.tab_step = true;
                        Message::Library(library::Message::MusicTab(
                            self.library.music_tab.step(step),
                        ))
                    }
                    None => return Task::none(),
                }
            }
            // The same over a plan's This plan | All.
            Message::Plans(plans::Message::Wheel(delta)) => {
                let shift = self.modifiers.shift();
                let step = self.plans.wheel.feed(delta, shift, now);
                match (step, plans::other_tab(self)) {
                    (Some(step), Some(tab)) => {
                        self.plans.tab_step = Some(step);
                        Message::Plans(tab)
                    }
                    _ => return Task::none(),
                }
            }
            message => self.click_with_modifiers(message),
        };
        self.act(message)
    }

    /// Does what `message` says, as it is: `update_at` has made a key
    /// press or a click with Ctrl held into it already. The right-click
    /// menu's items come here too, each as the message a click or a
    /// button elsewhere sends, whatever keys are held meanwhile.
    pub(crate) fn act(&mut self, message: Message) -> Task<Message> {
        let now = self.now;
        if let Message::Library(library::Message::Menu(action)) = message {
            return library::menu::run(self, action);
        }
        // Under the open menu the arrow keys move nothing, and a shortcut
        // that goes elsewhere closes it.
        if self.library.menu.is_some() {
            match &message {
                Message::Key(KeyPress::Up | KeyPress::Down | KeyPress::Left | KeyPress::Right) => {
                    return Task::none();
                }
                Message::Key(
                    KeyPress::Search | KeyPress::New | KeyPress::Edit | KeyPress::Menu,
                )
                | Message::History(_)
                | Message::Show(_) => self.library.menu = None,
                _ => {}
            }
        }
        if self.library.note_draft.is_some() && message.leaves_note() {
            entry::save_note(self);
        }
        if self.plans.item_note_draft.is_some() && message.leaves_plan_note() {
            plans::save_item_note(self);
        }
        let from = (message.navigates() && self.screen.in_history()).then(|| self.place());
        let selected = self.library.selected.clone();
        let page = self.library.page.is_some();
        let artist = self.library.artist.is_some();
        let drawer = library::drawer_state(self);
        let closes = library::closes_drawer(&message);
        let plan_drawer = plans::drawer_state(self);
        let plan_closes = plans::closes_drawer(&message);
        let plan_page = (self.screen == Screen::Plans).then(|| self.plans.page.clone());
        // A frame of a slide changes nothing the lookups or the kept
        // pictures go by, and a list that only scrolls leaves the lists
        // beside it as they were.
        let frame = matches!(message, Message::Frame(_));
        let scrolls = matches!(
            message,
            Message::Library(
                library::Message::Scrolled(_)
                    | library::Message::Wheel(_)
                    | library::Message::GroupsMeasured(_)
                    | library::Message::Pointer(_)
            )
        );
        let task = self.handle(message);
        let task = Task::batch([task, plans::page_turned(self, plan_page, now)]);
        if self.library.selected != selected {
            self.library.selection_moved();
        }
        library::slide_drawer(self, closes, drawer, now);
        plans::slide_drawer(self, plan_closes, plan_drawer, now);
        if !library::music_tabs_shown(&self.library) {
            self.library.tab_slide = None;
        }
        // Back from a page, the list or Home is where it was left; so is
        // Music's tab back from an artist, whose page starts at the top.
        let back = page && self.library.page.is_none();
        let turned = artist != self.library.artist.is_some()
            && (!artist || library::music_tabs_shown(&self.library));
        let task = if (back || turned) && self.screen == Screen::Library {
            Task::batch([task, library::restore_scroll(&self.library)])
        } else {
            task
        };
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
        self.keep_matches();
        let lookups = if frame {
            Task::none()
        } else {
            Task::batch([self.refresh_lookups(), self.keep_pictures(!scrolls)])
        };
        // The photo work gives way to a lookup in the Add dialog.
        self.apple.hold(self.lookup_running());
        let photos = if now >= self.photos_due {
            self.photos_next()
        } else {
            Task::none()
        };
        Task::batch([task, lookups, photos])
    }

    /// Writes the online matches found on this computer for albums with
    /// no `source` into the library (`Artwork::take_matches`), all that
    /// are known in one go, so they leave as one commit. Not in a mock
    /// run, nor before the library is here.
    fn keep_matches(&mut self) {
        if self.mock || self.loading {
            return;
        }
        for (id, source) in self.artwork.take_matches(self.repo.library()) {
            self.apply(Operation::SetSource {
                id: id.clone(),
                source,
            });
            if let Some(entry) = self.repo.library().entry(&id) {
                self.artwork.match_kept(entry);
            }
        }
    }

    /// What the screens show now that the lookups have not fetched yet
    /// (covers, photos, episodes, series, track lists), queued and started
    /// a few at a time; the answers come back as `Message::Lookup`.
    fn refresh_lookups(&mut self) -> Task<Message> {
        let wants = screens::wanted(self);
        let lookups = self
            .artwork
            .refresh(&wants, self.repo.library(), &self.lookups)
            .map(Message::Lookup);
        Task::batch([lookups, self.fetch_covers()])
    }

    /// Holds the pictures the screens want kept decoded (`screens::kept`)
    /// and asks the renderer for the ones not held yet; with `beside` the
    /// lists beside the one on show are worked out anew.
    fn keep_pictures(&mut self, beside: bool) -> Task<Message> {
        let known = std::mem::take(&mut self.kept.beside);
        let (on_show, beside) = screens::kept(self, beside, &known);
        let ask = self
            .kept
            .set(on_show.into_iter().chain(beside.iter().cloned()));
        self.kept.beside = beside;
        keep::allocate(ask).map(Message::Kept)
    }

    /// What a click does with Ctrl held, from the modifiers the keyboard
    /// subscription tracks. On a title in the library (a row, a card, a
    /// shelf's cover, a part of a series) Ctrl+click opens its cover in
    /// the viewer and Ctrl+Shift+click its Edit dialog, both without
    /// selecting it. On a title Discover found, on Coming soon, Ctrl+click
    /// opens its cover too. On a series' row in a list Ctrl+click opens the cover
    /// the row shows (the part that stands for the series), where the
    /// viewer steps through the other parts' covers. On a sidebar type
    /// Ctrl+click adds or removes it. On a plan's row Ctrl+click ticks it
    /// or unticks it, instead of selecting it, and Ctrl+Shift+click edits
    /// it.
    fn click_with_modifiers(&self, message: Message) -> Message {
        use library::Message as L;
        use plans::Message as P;
        if !self.modifiers.command() {
            return message;
        }
        let shift = self.modifiers.shift();
        if !shift
            && let Message::Library(L::OpenSeries(open)) = &message
            && open.highlight.is_none()
        {
            let parts = crate::series::entries_in(self.repo.library(), open.media, &open.name);
            if let Some(cover) = crate::series::cover_entry(&parts) {
                return Message::Entry(entry::Message::ViewSeriesCover {
                    id: cover.id.clone(),
                    media: open.media,
                    name: open.name.clone(),
                });
            }
        }
        match message {
            Message::Library(L::Select(id) | L::OpenSeriesPart(id))
            | Message::Plans(P::Select(id))
                if shift =>
            {
                Message::Entry(entry::Message::Edit(id))
            }
            Message::Library(L::Select(id) | L::OpenSeriesPart(id)) => {
                Message::Entry(entry::Message::ViewCover(id))
            }
            // A title Discover found, on Coming soon's lists and in its
            // panel: the cover, large, and nothing selected.
            Message::Library(L::SelectFound(key) | L::OpenFound(key)) if !shift => {
                Message::Entry(entry::Message::ViewFoundCover(key))
            }
            // An artist's row or card, in every view: the photo, large.
            Message::Library(L::OpenArtist(name)) if !shift => {
                Message::Entry(entry::Message::Artist(entry::artist::Message::View(name)))
            }
            Message::Library(L::ShowType(Some(media))) => Message::Library(L::ToggleType(media)),
            // A type chip on the Coming soon page adds or removes the type.
            Message::Library(L::SoonType(media)) => Message::Library(L::SoonToggleType(media)),
            Message::Plans(P::Select(id)) => Message::Plans(P::ToggleDone(id)),
            message => message,
        }
    }

    /// What a key press does: a shortcut from the keymap, else one of the
    /// fixed keys (Enter, Ctrl+Enter, Tab, the arrow keys). While Settings
    /// records a shortcut, the press is the new one.
    fn shortcut(&self, key: &Key, modifiers: keyboard::Modifiers) -> Option<Message> {
        // Ctrl+V in Pick a cover: an image from the clipboard.
        if modifiers.command()
            && !modifiers.shift()
            && !modifiers.alt()
            && matches!(key.as_ref(), Key::Character("v" | "V"))
            && entry::pick::takes_paste(self)
        {
            return Some(Message::Entry(entry::Message::Covers(
                entry::pick::Message::Paste,
            )));
        }
        let combo = Combo::of(key, modifiers)?;
        if self.screen == Screen::Settings && self.settings_page.recording.is_some() {
            return Some(Message::Settings(screens::settings::Message::Recorded(
                combo,
            )));
        }
        let free = self.dialog.is_none() && self.plan_dialog.is_none() && !self.token_warning;
        if let Some(action) = self.keymap.action_for(&combo) {
            return match action {
                Action::Search => Some(Message::Key(KeyPress::Search)),
                Action::Add => Some(Message::Key(KeyPress::New)),
                Action::Edit => Some(Message::Key(KeyPress::Edit)),
                Action::Menu => Some(Message::Key(KeyPress::Menu)),
                Action::Close => Some(Message::Key(KeyPress::Escape)),
                Action::Back => Some(Message::History(Direction::Back)),
                Action::Forward => Some(Message::History(Direction::Forward)),
                // The library's panel, or the plan page's.
                Action::Panel => match self.screen {
                    Screen::Library if free => {
                        Some(Message::Library(library::Message::TogglePanel))
                    }
                    Screen::Plans if free && self.plans.plan_id().is_some() => {
                        Some(Message::Plans(plans::Message::TogglePanel))
                    }
                    _ => None,
                },
                Action::Settings => (free
                    && matches!(
                        self.screen,
                        Screen::Library | Screen::Plans | Screen::Settings
                    ))
                .then_some(Message::Show(Screen::Settings)),
                // Only on the Coming soon page and over Music's tabs.
                Action::SoonPrev | Action::SoonNext => {
                    let step = if action == Action::SoonPrev { -1 } else { 1 };
                    if self.music_tabs_free() {
                        let tab = self.library.music_tab.step(step);
                        return Some(Message::Library(library::Message::MusicTab(tab)));
                    }
                    // And over a plan's This plan | All (`update_at`
                    // notes which way the arrow points).
                    if let Some(tab) = plans::other_tab(self) {
                        return Some(Message::Plans(tab));
                    }
                    self.soon_page_free()
                        .then_some(Message::Library(library::Message::SoonStep(step)))
                }
            };
        }
        let press = match key {
            Key::Named(Named::Enter) if modifiers.command() => KeyPress::Save,
            Key::Named(Named::Enter) => KeyPress::Enter,
            Key::Named(Named::ArrowUp) => KeyPress::Up,
            Key::Named(Named::ArrowDown) => KeyPress::Down,
            Key::Named(Named::ArrowLeft) if modifiers.is_empty() => KeyPress::Left,
            Key::Named(Named::ArrowRight) if modifiers.is_empty() => KeyPress::Right,
            Key::Named(Named::Tab) => KeyPress::Tab {
                back: modifiers.shift(),
            },
            Key::Named(Named::Backspace) if modifiers.is_empty() => KeyPress::Backspace,
            _ => return None,
        };
        Some(Message::Key(press))
    }

    /// What a key press that is no shortcut types into a new search: the
    /// text of a printable key without Ctrl, Alt or Meta, while the
    /// library shows its search field with no dialog, popover or menu
    /// over it. A press a text field took never comes here. Space,
    /// Enter, Tab and Backspace type nothing here and keep their meaning.
    fn typed(&self, modifiers: keyboard::Modifiers, text: Option<&str>) -> Option<String> {
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return None;
        }
        let text: String = text?.chars().filter(|c| !c.is_control()).collect();
        let free = self.screen == Screen::Library
            && self.dialog.is_none()
            && self.plan_dialog.is_none()
            && !self.token_warning
            && !self.conflict_open
            && self.library.search_free();
        (free && !text.trim().is_empty()).then_some(text)
    }

    /// The Coming soon page is on screen with nothing open over it, so a
    /// key or the wheel may switch its slice.
    fn soon_page_free(&self) -> bool {
        self.screen == Screen::Library
            && self.library.soon
            && self.library.details_pop.is_none()
            && !self.library.details_rating_open
            && self.dialog.is_none()
            && self.plan_dialog.is_none()
            && !self.token_warning
    }

    /// Music's Artists | Albums | Songs tabs are on screen with nothing
    /// open over them, so a key or the wheel may switch the tab.
    fn music_tabs_free(&self) -> bool {
        self.screen == Screen::Library
            && library::music_tabs_shown(&self.library)
            && self.library.details_pop.is_none()
            && !self.library.details_rating_open
            && self.dialog.is_none()
            && self.plan_dialog.is_none()
            && !self.token_warning
    }

    /// The tag picker open on screen: the entry dialog's, the plan
    /// dialog's or the Filter popover's.
    fn picker_mut(&mut self) -> Option<&mut PickerState> {
        match &mut self.dialog {
            Some(entry::Dialog::Form(form) | entry::Dialog::Tags(form)) => {
                return form.picker.as_mut();
            }
            Some(_) => return None,
            None => {}
        }
        if let Some(plans::Dialog::Form(form)) = &mut self.plan_dialog {
            return form.tag_view.as_mut();
        }
        self.library.picker.as_mut()
    }

    /// Esc and Back on the library close the details panel first: a part
    /// picked from a series goes back to the series, anything else in the
    /// panel closes (the drawer goes, Home's plan comes back). On a plan
    /// page they close the open title's details the same way. None when
    /// the panel holds nothing.
    fn close_panel(&mut self) -> Option<Task<Message>> {
        if self.screen == Screen::Plans && self.plans.selected.is_some() {
            return Some(plans::update(self, plans::Message::ClosePanel));
        }
        if self.screen != Screen::Library {
            return None;
        }
        if self.library.back_series().is_some() {
            return Some(self.handle(Message::Library(library::Message::BackToSeries)));
        }
        if self.library.found.is_some() {
            self.library.found = None;
            return Some(Task::none());
        }
        if self.library.pane_open() {
            self.library.update(library::Message::ClosePanel);
            return Some(Task::none());
        }
        None
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
        // Inside a tag picker's long group, Back is the way out of it.
        if direction == Direction::Back && self.picker_mut().is_some_and(PickerState::back) {
            return Task::none();
        }
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
                if direction == Direction::Back
                    && let Some(task) = self.close_panel()
                {
                    return task;
                }
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
            Message::Show(screen) => {
                if screen != Screen::Settings {
                    self.settings_page.keys_open = false;
                    self.settings_page.recording = None;
                    self.settings_page.key_warning = None;
                }
                self.screen = screen;
            }
            Message::History(direction) => return self.travel(direction),
            Message::SetTheme(choice) => self.settings.theme = choice,
            Message::SetAccent(accent) => self.settings.accent = accent,
            // No answer from the portal (`None`) keeps the scheme, which
            // is dark at the start: System then means Graphite.
            Message::SystemTheme(mode) if !self.plasma_fixed => match mode {
                ThemeMode::Dark => self.plasma_dark = true,
                ThemeMode::Light => self.plasma_dark = false,
                ThemeMode::None => {}
            },
            Message::SystemTheme(_) => {}
            // One sweep of the splash's bar takes 1.4 s; the setup's ring
            // turns twice in that time.
            Message::Tick => self.splash_phase = (self.splash_phase + 0.016 / 1.4) % 1.0,
            // The slide is timed in `update_at`; the frame only redraws.
            Message::Frame(_) => {}
            // Taken in `update_at`, before anything else.
            Message::Kept(batch) => self.kept.arrived(batch),
            Message::Resized(size) => self.window = size,
            Message::Window(event) => return self.window_event(event),
            Message::ScaleFactor(scale) => {
                if scale.is_finite() && scale > 0.0 {
                    self.scale = scale;
                }
            }
            Message::Decoration(decoration) => self.decoration = decoration,
            Message::Sync(SyncMessage::Retry | SyncMessage::Now) => {
                // "Check again" in the unreadable dialog closes it; the
                // five-minute check leaves an open dialog alone.
                if matches!(message, Message::Sync(SyncMessage::Retry)) {
                    self.conflict_open = false;
                }
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
                self.source = None;
                self.setup = setup::State::real(setup::Step::Connect, self.connection.as_ref());
                self.screen = Screen::Setup;
            }
            Message::Sync(SyncMessage::WalletDone(result)) => {
                if let Err(error) = result {
                    eprintln!("libri: {error}");
                }
            }
            Message::Sync(SyncMessage::Answer(_, generation, _))
                if generation != self.engine_generation => {}
            Message::Sync(SyncMessage::Answer(answer, _, dropped)) => {
                self.record_token_expiry();
                let Some(engine) = &mut self.engine else {
                    return Task::none();
                };
                let is_cover =
                    |change: &covers::Change| !matches!(change, covers::Change::Settings(_));
                let changes_before = engine.pending().changes();
                let files_before = changes_before.len();
                let covers_before = changes_before.iter().any(is_cover);
                let outcome = engine.answer(*answer, self.repo.library());
                let files_landed = engine.pending().files().len() < files_before;
                let round_over = !engine.running();
                self.refresh_sync_state();
                self.covers_dropped(dropped);
                let next = self.run(outcome);
                // Cover files went up: GitHub's list is asked for again,
                // which also brings `_missing.json` as it was merged and
                // the file GitHub kept over one dropped here.
                let listing = if files_landed && covers_before {
                    self.list_covers()
                } else {
                    Task::none()
                };
                // `settings.json` may have gone up with them: a download
                // asked for before that is the file from before.
                if files_landed {
                    self.shared.epoch += 1;
                }
                let shared = if round_over {
                    self.fetch_shared()
                } else {
                    Task::none()
                };
                return Task::batch([next, listing, shared]);
            }
            Message::Key(KeyPress::Backspace) => {
                if let Some(picker) = self.picker_mut() {
                    picker.back_on_backspace();
                }
            }
            Message::Key(KeyPress::Tab { back: false }) => return operation::focus_next(),
            Message::Key(KeyPress::Tab { back: true }) => return operation::focus_previous(),
            // The token warning sits over everything; then a dialog; the
            // sidebar's All tags popover also opens on Plans.
            Message::Key(KeyPress::Escape) => match self.screen {
                _ if self.token_warning => self.token_warning = false,
                _ if self.conflict_open => self.conflict_open = false,
                Screen::Setup => self.setup.which = None,
                _ => {
                    if !entry::dismiss_top(self)
                        && !following::dismiss(self)
                        && !plans::dismiss_top(self)
                        && !screens::settings::dismiss_top(self)
                        && !self.library.dismiss_top()
                        && let Some(task) = self.close_panel()
                    {
                        return task;
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
                Screen::Settings if self.settings_page.keys_open => {
                    return screens::settings::update(self, screens::settings::Message::CloseKeys);
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
            Message::Key(key @ (KeyPress::Left | KeyPress::Right)) => {
                if matches!(self.dialog, Some(entry::Dialog::Cover(_))) {
                    let step = if key == KeyPress::Left { -1 } else { 1 };
                    return entry::update(self, entry::Message::StepCover(step));
                }
            }
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
            Message::Key(KeyPress::Menu) => {
                if self.screen == Screen::Library
                    && self.dialog.is_none()
                    && self.plan_dialog.is_none()
                    && self.library.picker.is_none()
                    && !self.token_warning
                    && !self.conflict_open
                {
                    return library::menu::open_for_selected(self);
                }
            }
            // The menu opens on the library itself, never under a dialog.
            Message::Library(library::Message::OpenMenu(_) | library::Message::OpenMenuAt(..))
                if self.dialog.is_some()
                    || self.plan_dialog.is_some()
                    || self.token_warning
                    || self.conflict_open
                    || self.screen != Screen::Library => {}
            // The ✕ leaves the cursor in the field.
            Message::Library(library::Message::ClearSearch) => {
                self.library.update(library::Message::ClearSearch);
                return operation::focus(library::SEARCH_ID);
            }
            // Typing in the window: a new search with what the key types,
            // in place of the text there, and the cursor to the field
            // after it. The field has the cursor only once iced has run
            // the task, so a key pressed before that comes here as well
            // and goes after the first.
            Message::Library(library::Message::Typed(text)) => {
                let follows = self
                    .typed_at
                    .is_some_and(|at| self.now.saturating_duration_since(at) < TYPED_ON);
                let query = if follows {
                    format!("{}{text}", self.library.query)
                } else {
                    text
                };
                self.typed_at = Some(self.now);
                self.library.update(library::Message::Typed(query));
                return operation::focus(library::SEARCH_ID)
                    .chain(operation::move_cursor_to_end(library::SEARCH_ID));
            }
            // A text field took Esc and let go of the cursor. With nothing
            // open over the library that field is the search: holding
            // text, it is cleared and keeps the cursor, so the next Esc
            // does what Esc did before.
            Message::Library(library::Message::EscapeInField) => {
                let state = &self.library;
                let bare = self.screen == Screen::Library
                    && self.dialog.is_none()
                    && self.plan_dialog.is_none()
                    && !self.token_warning
                    && !self.conflict_open
                    && state.picker.is_none()
                    && state.popup.is_none()
                    && state.details_pop.is_none()
                    && state.menu.is_none()
                    && state.page.is_none()
                    && !state.soon;
                if bare && !state.query.is_empty() {
                    self.library.update(library::Message::ClearSearch);
                    return operation::focus(library::SEARCH_ID);
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
            // The cursor to the search field, its text selected.
            Message::Key(KeyPress::Search) => {
                if self.screen == Screen::Library {
                    return operation::focus(library::SEARCH_ID)
                        .chain(operation::select_all(library::SEARCH_ID));
                }
            }
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            // Turned into what it does at the top of `update`.
            Message::KeyPressed(..) => {}
            Message::Library(library::Message::ScrollList(y)) => {
                let offset = scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(y),
                };
                return operation::scroll_to(library::LIST_ID, offset);
            }
            Message::Library(library::Message::JumpToSection {
                label,
                offset,
                held,
            }) => {
                self.library.update(library::Message::JumpToSection {
                    label,
                    offset,
                    held,
                });
                let offset = scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(offset),
                };
                return operation::scroll_to(library::LIST_ID, offset);
            }
            Message::Library(library::Message::Picker(tags::PickerMessage::Jump(offset))) => {
                return tags::jump(offset);
            }
            // The groups holding a chosen tag start open.
            Message::Library(
                message @ (library::Message::OpenPicker | library::Message::MoreTags),
            ) => {
                let library = self.repo.library();
                let open = tags::groups_with_picks(
                    library.tags.iter().map(|(&c, names)| (c, names.as_slice())),
                    |tag| library.is_multi_category(tag),
                    |tag, category| self.library.filter.has_tag(tag, category),
                );
                self.library.update(message);
                if let Some(picker) = &mut self.library.picker {
                    picker.open = open;
                }
            }
            Message::Library(
                message @ (library::Message::OpenSeries(_)
                | library::Message::OpenSeriesPart(_)
                | library::Message::BackToSeries),
            ) => {
                self.library.update(message);
                // The pane starts at the top, or at the part it marks.
                let offset = self
                    .library
                    .open_series()
                    .map_or(0.0, |open| entry::series::offset(self, open));
                let offset = scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(offset),
                };
                return operation::scroll_to(entry::details::PANE_ID, offset);
            }
            Message::Library(library::Message::PickSeriesCover(id)) => {
                self.apply(Operation::SetSeriesCover { id });
            }
            // The Coming soon page's slices: each keeps its own scroll
            // offset, and on a step the list slides between them.
            Message::Library(
                library::Message::SoonSlice(_)
                | library::Message::SoonStep(_)
                | library::Message::OpenFound(_),
            ) => return self.switch_slice(message),
            // Opening the page puts its list back where the slice was left.
            Message::Library(
                message @ (library::Message::ShowSoon
                | library::Message::OpenSoonForTypes
                | library::Message::SeeAll(crate::browse::Section::ComingSoon)),
            ) => {
                self.screen = Screen::Library;
                self.library.update(message);
                let offset = scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(self.library.scroll.offset),
                };
                return operation::scroll_to(library::LIST_ID, offset);
            }
            // Music's tabs: the list slides between them as the Coming
            // soon page's does, and each keeps its own scroll offset.
            Message::Library(library::Message::MusicTab(tab)) => return self.switch_tab(tab),
            Message::Library(library::Message::Wheel(delta)) => {
                if self.soon_page_free()
                    && let Some(step) =
                        self.library
                            .wheel
                            .feed(delta, self.modifiers.shift(), self.now)
                {
                    return self.switch_slice(Message::Library(library::Message::SoonStep(step)));
                }
            }
            Message::Library(library::Message::AddFound(key)) => return self.add_found(&key),
            Message::Library(library::Message::HideFound(key)) => {
                let title = self.discover.item(&key).map(discover::Found::full_title);
                let edit = shared::Edit::Hide(key.clone());
                let summary = shared::summary(&edit, title.as_deref());
                self.change_shared(edit, summary);
                self.library.update(library::Message::HideFound(key));
            }
            Message::Library(library::Message::RestoreFound(key)) => {
                let title = self.discover.item(&key).map(discover::Found::full_title);
                let edit = shared::Edit::Restore(key);
                let summary = shared::summary(&edit, title.as_deref());
                self.change_shared(edit, summary);
            }
            Message::Discover(discover::Message::Check) => return self.check_discover(false),
            Message::Discover(discover::Message::Now) => return self.check_discover(true),
            Message::Discover(discover::Message::Stop) => self.discover.stop(),
            // Only there so the row is drawn again as the run counts on.
            Message::Discover(discover::Message::Tick) => {}
            Message::Discover(discover::Message::Done(outcome)) => {
                if let Some(why) = &outcome.stopped {
                    eprintln!(
                        "libri: the daily check stopped ({why}); asked {}",
                        outcome.sources.join(", ")
                    );
                }
                self.discover.finish(*outcome, Timestamp::now());
                // Who is no longer active may have been learned.
                self.refresh_follows();
            }
            Message::SharedFetched(_, generation, _) if generation != self.engine_generation => {
                self.shared.fetching = false;
            }
            Message::SharedFetched(reply, _, epoch) => return self.shared_fetched(*reply, epoch),
            Message::Following(message) => return following::update(self, message),
            Message::Library(message) => {
                if message.shows_library() {
                    self.screen = Screen::Library;
                }
                // Pinning or unpinning changes the list's width; the
                // selected entry stays in view. A narrow window has no
                // choice to make.
                if matches!(message, library::Message::TogglePanel) {
                    if library::panel(self) == library::Panel::Home || library::narrow(self) {
                        return Task::none();
                    }
                    self.library.update(message);
                    return match self.library.selected.as_deref() {
                        Some(id) => library::reveal(&self.library, id),
                        None => Task::none(),
                    };
                }
                self.library.update(message);
            }
            Message::Plans(message) => {
                self.screen = Screen::Plans;
                return plans::update(self, message);
            }
            Message::Setup(message) => return setup::update(self, message),
            Message::Settings(message) => return screens::settings::update(self, message),
            Message::Token(message) => return token::update(self, message),
            Message::Covers(message) => return self.covers_update(message),
            Message::PhotoFound { work, result } => return self.photo_found(work, result),
            Message::PhotoHistory(result) => return self.sharper_history(result),
            Message::PhotoSharper { work, result } => return self.photo_sharper(work, result),
            Message::Lookup(artwork::Message::Prune) => {
                // Discover's covers belong to no entry; the prune keeps
                // them too, hidden ones included.
                let covers: Vec<String> = self
                    .discover
                    .record()
                    .items
                    .iter()
                    .filter_map(|item| item.cover_url.clone())
                    .collect();
                return self
                    .artwork
                    .prune(self.repo.library(), &covers)
                    .map(Message::Lookup);
            }
            Message::Lookup(message) => {
                return self
                    .artwork
                    .update(message, self.repo.library())
                    .map(Message::Lookup);
            }
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
                // The arrow keys keep the selection in view; a click never
                // moves the list.
                if let Some(id) = self.library.selected.as_deref() {
                    return library::reveal(&self.library, id);
                }
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
            keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            } => Some(Message::KeyPressed(
                key,
                modifiers,
                text.map(|text| text.to_string()),
            )),
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
        let windows = window::events().map(|(_, event)| Message::Window(event));
        let scheme = iced::system::theme_changes().map(Message::SystemTheme);
        let mut subscriptions = vec![keys, buttons, resizes, windows, scheme];
        // A track dragged in the Tracks dialog is let go wherever the
        // button comes up.
        if entry::tracks::dragging(self) {
            subscriptions.push(event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(
                    Message::Entry(entry::Message::Tracks(entry::tracks::Message::Drop)),
                ),
                _ => None,
            }));
        }
        // Side-scrolling or Shift+wheel anywhere on the Coming soon page
        // or under Music's tabs; the list may have captured the event, so
        // its status is no matter.
        if self.screen == Screen::Library
            && (self.library.soon || library::music_tabs_shown(&self.library))
        {
            subscriptions.push(event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                    Some(Message::Library(library::Message::Wheel(delta)))
                }
                _ => None,
            }));
        }
        // Esc in the search field: the field takes it and lets go of the
        // cursor, so the press never comes as a shortcut.
        if self.screen == Screen::Library && !self.library.query.is_empty() {
            subscriptions.push(event::listen_with(|event, status, _window| match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(Named::Escape),
                    ..
                }) if status == event::Status::Captured => {
                    Some(Message::Library(library::Message::EscapeInField))
                }
                _ => None,
            }));
        }
        // Shift+← and Shift+→ in the music lookup's search field, which
        // takes them for its selection.
        if entry::music_lookup_open(self) {
            subscriptions.push(event::listen_with(|event, _status, _window| match event {
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(named @ (Named::ArrowLeft | Named::ArrowRight)),
                    modifiers,
                    ..
                }) if modifiers == keyboard::Modifiers::SHIFT => {
                    let step = if named == Named::ArrowLeft { -1 } else { 1 };
                    Some(Message::Entry(entry::Message::ScopeKeyPressed(step)))
                }
                _ => None,
            }));
        }
        // The same on a plan page, for This plan | All.
        if self.screen == Screen::Plans && self.plans.plan_id().is_some() {
            subscriptions.push(event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                    Some(Message::Plans(plans::Message::Wheel(delta)))
                }
                _ => None,
            }));
        }
        // The splash's bar, and the setup's ring while GitHub is asked.
        if self.screen == Screen::Splash
            || (self.screen == Screen::Setup && self.setup.busy.is_some())
            || entry::is_busy(self)
        {
            subscriptions.push(time::every(Duration::from_millis(16)).map(|_| Message::Tick));
        }
        // The drawer's slide, the Coming soon page's slice slide and the
        // slide between Music's tabs draw every frame while they run.
        if self.library.motion.running()
            || self.plans.motion.running()
            || self.plans.tab_slide.is_some()
            || self.library.slide.is_some()
            || self.library.tab_slide.is_some()
        {
            subscriptions.push(window::frames().map(Message::Frame));
        }
        // The debounce clock runs only while edits wait; while a
        // repository is connected GitHub is asked for changes every half
        // minute with the window focused and every five minutes without.
        if self.save_due.is_some() {
            subscriptions.push(
                time::every(Duration::from_millis(250)).map(|_| Message::Sync(SyncMessage::Tick)),
            );
        }
        if !self.mock {
            subscriptions.push(
                time::every(DISCOVER_INTERVAL).map(|_| Message::Discover(discover::Message::Check)),
            );
        }
        // While the check runs, Settings' row counts what it has asked.
        if self.discover.running && self.screen == Screen::Settings {
            subscriptions.push(
                time::every(Duration::from_millis(250))
                    .map(|_| Message::Discover(discover::Message::Tick)),
            );
        }
        if self.engine.is_some() {
            let interval = if self.focused {
                sync::FOCUSED_CHECK_INTERVAL
            } else {
                sync::UNFOCUSED_CHECK_INTERVAL
            };
            subscriptions.push(time::every(interval).map(|_| Message::Sync(SyncMessage::Now)));
        }
        Subscription::batch(subscriptions)
    }

    /// A window event: the close request quits, focus is remembered for
    /// the check interval, and the window coming back asks GitHub for
    /// changes unless a check was made moments ago.
    fn window_event(&mut self, event: window::Event) -> Task<Message> {
        match event {
            window::Event::CloseRequested => return quit(),
            window::Event::Focused => {
                self.focused = true;
                if self.check_is_stale() {
                    return self.sync_now();
                }
            }
            window::Event::Unfocused => self.focused = false,
            window::Event::Opened { .. } => {
                return window::latest()
                    .and_then(window::scale_factor)
                    .map(Message::ScaleFactor);
            }
            window::Event::Rescaled(scale) => {
                return self.update(Message::ScaleFactor(scale));
            }
            _ => {}
        }
        Task::none()
    }
}

/// How long after the start the daily check behind Discover runs.
const DISCOVER_DELAY: Duration = Duration::from_secs(5);
/// How often an open app sees whether the check is due: a new day, or a
/// run that did not finish. Focus has no part in it.
const DISCOVER_INTERVAL: Duration = Duration::from_secs(60 * 60);

impl App {
    /// A slice switch on the Coming soon page: the new slice is scrolled
    /// back to where it was left. By Shift+arrows, a side-scroll or
    /// Shift+wheel (`SoonStep`) the list slides from the slice left to
    /// the new one as KWin switches desktops, on its spring at Plasma's
    /// speed and from wherever a slide under way has it (a plain swap at
    /// Instant); a click (a segment, a row of New in Discover) swaps at
    /// once, ending any slide on its way.
    fn switch_slice(&mut self, message: Message) -> Task<Message> {
        let Message::Library(message) = message else {
            return Task::none();
        };
        if !self.library.soon {
            return Task::none();
        }
        let stepped = matches!(message, library::Message::SoonStep(_));
        if !stepped {
            self.library.slide = None;
        }
        let from = self.library.slice;
        self.library.update(message);
        let to = self.library.slice;
        if from == to {
            return Task::none();
        }
        if stepped && let Some(spring) = animation::Spring::desktop_switch(self.speed) {
            let width = if self.library.list_width > 0.0 {
                self.library.list_width
            } else {
                screens::shell::content_width(self, 0.0)
            };
            self.library.slide = Some(library::SliceSlide::toward(
                self.library.slide.as_ref(),
                from.index(),
                to.index(),
                library::Slice::ALL.len(),
                spring.slowed(library::soon::SLOWER),
                (width + library::soon::GAP) * self.scale,
                self.now,
            ));
        }
        let offset = scrollable::AbsoluteOffset {
            x: None,
            y: Some(self.library.scroll.offset),
        };
        operation::scroll_to(library::LIST_ID, offset)
    }

    /// A switch of Music's tab: the new tab is scrolled back to where it
    /// was left. By Shift+arrows, a side-scroll or Shift+wheel
    /// (`tab_step`) the list slides to it exactly as the Coming soon
    /// page's does between its slices (`switch_slice`); a click on a
    /// segment swaps at once, ending any slide on its way.
    fn switch_tab(&mut self, tab: library::MusicTab) -> Task<Message> {
        let stepped = std::mem::take(&mut self.library.tab_step);
        if !stepped {
            self.library.tab_slide = None;
        }
        let shown = self.screen == Screen::Library && library::music_tabs_shown(&self.library);
        let from = self.library.music_tab;
        self.screen = Screen::Library;
        self.library.update(library::Message::MusicTab(tab));
        if !shown || from == tab {
            return Task::none();
        }
        if stepped && let Some(spring) = animation::Spring::desktop_switch(self.speed) {
            let width = if self.library.list_width > 0.0 {
                self.library.list_width
            } else {
                screens::shell::content_width(self, 0.0)
            };
            self.library.tab_slide = Some(library::SliceSlide::toward(
                self.library.tab_slide.as_ref(),
                from.index(),
                tab.index(),
                library::MusicTab::ALL.len(),
                spring.slowed(library::soon::SLOWER),
                (width + library::soon::GAP) * self.scale,
                self.now,
            ));
        }
        let offset = scrollable::AbsoluteOffset {
            x: None,
            y: Some(self.library.scroll.offset),
        };
        operation::scroll_to(library::LIST_ID, offset)
    }

    /// Runs the daily check behind Discover, unless it ran today or is
    /// running: at once on the fake, else on the blocking pool.
    /// Runs the daily check when it is due, or at once with `force`
    /// (Check now): never with the check switched off, and not before
    /// GitHub's `settings.json` is known, since the defaults would follow
    /// everything the owner unticked.
    fn check_discover(&mut self, force: bool) -> Task<Message> {
        let now = Timestamp::now();
        let settings = self.shared.settings.clone();
        if !settings.enabled || self.discover.running {
            return Task::none();
        }
        if !force && !self.discover.is_due(now) {
            return Task::none();
        }
        if self.github.is_some() && !self.shared.known {
            return Task::none();
        }
        let options = self.discover.start(force);
        let service = self.lookups.clone();
        let library = self.repo.library().clone();
        let today = self.today;
        let previous = self.discover.record().clone();
        if service.is_instant() {
            let outcome = discover::run_with(
                &service, &library, today, &settings, &previous, None, &options,
            );
            self.discover.finish(outcome, now);
            self.refresh_follows();
            return Task::none();
        }
        let file = self.discover.file().map(std::path::Path::to_path_buf);
        Task::perform(
            blocking(move || {
                Ok(discover::run_with(
                    &service,
                    &library,
                    today,
                    &settings,
                    &previous,
                    file.as_deref(),
                    &options,
                ))
            }),
            move |result| {
                let outcome = result.unwrap_or_else(|error| {
                    eprintln!("libri: the daily check did not finish: {error}");
                    discover::Outcome::failed(error)
                });
                Message::Discover(discover::Message::Done(Box::new(outcome)))
            },
        )
    }

    /// "+ Add" or "Add to library" on a Discover item: a season goes onto
    /// its show as Want at once (the show is in the library already);
    /// anything else opens the Add dialog prefilled from the item, with
    /// the status Want.
    fn add_found(&mut self, key: &str) -> Task<Message> {
        let Some(found) = self.discover.item(key).cloned() else {
            return Task::none();
        };
        match &found.kind {
            discover::Kind::Season { show, number } => {
                let Some(entry) = self.repo.library().entry(show) else {
                    return Task::none();
                };
                let mut seasons = entry.season_list().to_vec();
                if seasons.iter().any(|s| s.season == *number) {
                    return Task::none();
                }
                seasons.push(crate::model::Season {
                    season: *number,
                    released: found.released,
                    status: Some(crate::model::EntryStatus::Want),
                    fresh: None,
                    manual: None,
                });
                seasons.sort_by_key(|s| s.season);
                self.apply(Operation::SetSeasons {
                    id: show.clone(),
                    seasons,
                    today: self.today,
                });
                self.library
                    .update(library::Message::HideFound(key.to_owned()));
                Task::none()
            }
            _ => {
                let mut probe =
                    lookup::Match::new(found.media, found.title.clone(), found.source.clone());
                probe.creator = found.creator.clone();
                probe.released = found.released;
                probe.description = found.description.clone();
                probe.cover_url = found.cover_url.clone();
                if found.media == crate::model::MediaType::Music {
                    probe.kind = Some(crate::model::EntryKind::Album);
                }
                let task = entry::update(self, entry::Message::AddFromMatch(Box::new(probe)));
                Task::batch([task, operation::focus(entry::LOOKUP_ID)])
            }
        }
    }
}

/// Runs a blocking call (GitHub, KDE Wallet, a lookup) on tokio's
/// blocking pool, so the window never waits on it.
pub(crate) async fn blocking<T, F>(call: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(call)
        .await
        .unwrap_or_else(|error| Err(format!("the call did not finish: {error}")))
}

/// A blocking call that answers in steps (a search's early rows, a stored
/// list before the fetched one), on the blocking pool; every step becomes
/// a message as it comes.
pub(crate) fn steps<T, F>(work: F) -> Task<T>
where
    F: FnOnce(&mut dyn FnMut(T)) + Send + 'static,
    T: Send + 'static,
{
    use iced::futures::{SinkExt, StreamExt};
    Task::stream(iced::stream::channel(
        16,
        move |mut output: iced::futures::channel::mpsc::Sender<T>| async move {
            let (tx, mut rx) = iced::futures::channel::mpsc::unbounded::<T>();
            let handle = tokio::task::spawn_blocking(move || {
                let mut send = |step: T| {
                    let _ = tx.unbounded_send(step);
                };
                work(&mut send);
            });
            while let Some(step) = rx.next().await {
                let _ = output.send(step).await;
            }
            let _ = handle.await;
        },
    ))
}

/// One of the engine's requests against the real API.
fn perform(
    client: &github::Client,
    target: &Target,
    covers_dir: Option<&std::path::Path>,
    request: Request,
) -> (Answer, Vec<sync::commit::Dropped>) {
    let mut dropped = Vec::new();
    let answer = match request {
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
        Request::Commit {
            library,
            message,
            sha,
            files,
        } => Answer::Put(
            sync::commit::run(
                client,
                target,
                covers_dir,
                library.as_deref(),
                &message,
                sha.as_deref(),
                &files,
            )
            .map(|committed| {
                dropped = committed.dropped;
                committed.result
            }),
        ),
        Request::Commits { limit } => {
            Answer::Commits(client.commits(&target.file, &target.branch, limit))
        }
        Request::Repo => Answer::Repo(client.repo()),
    };
    (answer, dropped)
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
    fn regaining_focus_never_starts_the_daily_check() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Window(window::Event::Focused));
        assert_eq!(app.discover.record().checked, None, "not on focus");
        let _ = app.update(Message::Discover(discover::Message::Check));
        assert!(app.discover.record().checked.is_some(), "when asked");
    }

    #[test]
    fn a_close_comes_to_the_app_so_it_never_waits_for_the_network() {
        assert!(!window_settings().exit_on_close_request);
    }

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
    fn closing_an_artist_page_returns_to_where_the_list_was_scrolled() {
        let mut app = App::new(Screen::Library);
        let _ = app.update(Message::Library(library::Message::ShowType(Some(
            MediaType::Music,
        ))));
        let open = || Message::Library(library::Message::OpenArtist("Metallica".into()));
        app.library.scroll.offset = 640.0;
        let _ = app.update(open());
        assert_eq!(app.library.scroll.offset, 0.0, "the page starts at the top");
        app.library.scroll.offset = 90.0;
        back(&mut app);
        assert_eq!(app.library.scroll.offset, 640.0, "Back");
        forward(&mut app);
        assert_eq!(app.library.scroll.offset, 0.0, "Forward");
        let _ = app.update(Message::Library(library::Message::CloseArtist));
        assert_eq!(app.library.scroll.offset, 640.0, "the header's arrow");

        // Each tab's own place, also when the page closes onto another.
        let _ = app.update(open());
        let _ = app.update(Message::Library(library::Message::MusicTab(
            library::MusicTab::Albums,
        )));
        assert_eq!(app.library.scroll.offset, 0.0, "a tab not seen yet");
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
        // Light draws its own shade of the accent; the setting is the same.
        assert_eq!(tokens.accent, violet.light);
        assert_eq!(app.settings.accent, violet);
        let _ = app.update(Message::SetTheme(ThemeChoice::Fixed(Theme::Graphite)));
        assert_eq!(app.tokens().accent, violet.fill);
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
    fn ctrl_click_on_a_title_opens_its_cover_and_with_shift_its_edit_dialog() {
        use crate::screens::entry::Dialog;
        let mut app = App::new(Screen::Library);
        let (open, other) = ("movies/the-mummy-1999", "movies/die-hard-1988");
        let click = |id: &str| Message::Library(library::Message::Select(id.into()));
        let editing = |app: &App, id: &str| matches!(&app.dialog, Some(Dialog::Form(form)) if form.editing.as_deref() == Some(id));
        let _ = app.update(click(open));

        // The cover viewer, on another title and on the selected one,
        // whose details stay open; the selection never moves, and the
        // viewer's cover is asked for before the list's.
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::CTRL));
        for id in [other, open] {
            let _ = app.update(click(id));
            assert!(matches!(&app.dialog, Some(Dialog::Cover(view)) if view.id == id));
            assert_eq!(app.library.selected.as_deref(), Some(open));
            assert_eq!(crate::screens::wanted(&app).covers.first().unwrap(), id);
            let _ = app.update(Message::Key(KeyPress::Escape));
            assert!(app.dialog.is_none());
        }

        // Edit, from the library and from a plan's row, which Ctrl alone
        // still ticks.
        let _ = app.update(Message::Modifiers(
            keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
        ));
        let _ = app.update(click(other));
        assert!(editing(&app, other));
        assert_eq!(app.library.selected.as_deref(), Some(open));
        let _ = app.update(Message::Key(KeyPress::Escape));
        assert!(app.dialog.is_none());
        let _ = app.update(Message::Plans(plans::Message::ShowPlan(
            "christmas-2026".into(),
        )));
        let _ = app.update(Message::Plans(plans::Message::Select(other.into())));
        assert!(editing(&app, other));
        assert!(app.plans.selected.is_none());
    }

    #[test]
    fn ctrl_click_on_a_series_row_opens_the_cover_it_shows_and_steps_through_the_parts() {
        use crate::screens::entry::Dialog;
        let mut app = App::new(Screen::Library);
        let part = |n: &str| format!("books/berserk-vol-{n}");
        let (first, middle, last) = (part("1-1990"), part("42-2021"), part("43-2025"));
        let row = |highlight: Option<String>| {
            Message::Library(library::Message::OpenSeries(library::OpenSeries {
                media: MediaType::Books,
                name: "Berserk".into(),
                highlight,
            }))
        };
        let shown = |app: &App| match &app.dialog {
            Some(Dialog::Cover(view)) => view.id.clone(),
            other => panic!("no viewer: {other:?}"),
        };
        let key = |app: &mut App, key: KeyPress| {
            let _ = app.update(Message::Key(key));
        };

        // The first part stands for the series until another is picked.
        let _ = app.update(Message::Modifiers(keyboard::Modifiers::CTRL));
        let _ = app.update(row(None));
        assert_eq!(shown(&app), first);
        assert!(app.library.open_series().is_none(), "the pane stays");
        key(&mut app, KeyPress::Escape);
        app.apply(Operation::SetSeriesCover { id: middle.clone() });
        let _ = app.update(row(None));
        assert_eq!(shown(&app), middle);

        // The parts on either side are fetched ahead; the arrows and the
        // keys stop at the ends.
        let wants = crate::screens::wanted(&app);
        assert_eq!(
            wants.covers[..3],
            [middle.clone(), last.clone(), first.clone()]
        );
        let _ = app.update(Message::Entry(entry::Message::StepCover(1)));
        assert_eq!(shown(&app), last);
        key(&mut app, KeyPress::Right);
        assert_eq!(shown(&app), last);
        key(&mut app, KeyPress::Left);
        key(&mut app, KeyPress::Left);
        key(&mut app, KeyPress::Left);
        assert_eq!(shown(&app), first);
        // Not while Pick a cover is open on this part.
        let _ = app.update(Message::Entry(entry::Message::ChangeCover));
        key(&mut app, KeyPress::Right);
        assert_eq!(shown(&app), first);
        key(&mut app, KeyPress::Escape);
        key(&mut app, KeyPress::Escape);
        assert!(app.dialog.is_none());

        // "All 43 volumes" in details and Ctrl+Shift+click open the series.
        let _ = app.update(row(Some(last.clone())));
        assert!(app.dialog.is_none() && app.library.open_series().is_some());
        key(&mut app, KeyPress::Escape);
        let _ = app.update(Message::Modifiers(
            keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
        ));
        let _ = app.update(row(None));
        assert!(app.dialog.is_none() && app.library.open_series().is_some());
    }

    #[test]
    fn typing_in_the_window_starts_a_search() {
        let start = Instant::now();
        let later = |ms: u64| start + Duration::from_millis(ms);
        let key = |app: &mut App, c: &str, modifiers, at: Instant| {
            let _ = app.update_at(
                Message::KeyPressed(Key::Character(c.into()), modifiers, Some(c.into())),
                at,
            );
        };
        let none = keyboard::Modifiers::empty();
        let mut app = App::new(Screen::Library);
        app.library
            .update(library::Message::Select("movies/the-mummy-1999".into()));

        // A plain letter is the search now, and no longer Edit.
        key(&mut app, "e", none, later(0));
        assert_eq!(app.library.query, "e");
        assert!(app.dialog.is_none());
        // The next key came before the field had the cursor: it goes
        // after the first. From then on the field takes the keys.
        key(&mut app, "x", none, later(20));
        assert_eq!(app.library.query, "ex");
        let _ = app.update_at(
            Message::Library(library::Message::Search("exo".into())),
            later(40),
        );
        // Typed with the cursor elsewhere, a key starts over; a composed
        // letter, a capital and a digit count as well.
        key(&mut app, "å", none, later(60));
        assert_eq!(app.library.query, "å");
        key(&mut app, "D", keyboard::Modifiers::SHIFT, later(2000));
        assert_eq!(app.library.query, "D");
        key(&mut app, "1", none, later(4000));
        assert_eq!(app.library.query, "1");
        // A dead key's letter comes as the text of the key after it.
        let _ = app.update_at(
            Message::KeyPressed(Key::Character("e".into()), none, Some("é".into())),
            later(6000),
        );
        assert_eq!(app.library.query, "é");

        // Space, Enter and a key with Ctrl, Alt or Meta type nothing.
        let _ = app.update_at(
            Message::KeyPressed(Key::Named(Named::Space), none, Some(" ".into())),
            later(8000),
        );
        let _ = app.update_at(
            Message::KeyPressed(Key::Named(Named::Enter), none, Some("\r".into())),
            later(8000),
        );
        key(&mut app, "q", keyboard::Modifiers::ALT, later(8000));
        key(&mut app, "q", keyboard::Modifiers::LOGO, later(8000));
        key(&mut app, "\u{11}", keyboard::Modifiers::CTRL, later(8000));
        assert_eq!(app.library.query, "é");

        // Not under a popover, a menu, a dialog or on a page without
        // the field, nor on another screen.
        app.library
            .update(library::Message::Toggle(library::Popup::Sort));
        key(&mut app, "a", none, later(10000));
        app.library.update(library::Message::ClosePopup);
        let _ = app.update(Message::Library(library::Message::ShowSoon));
        key(&mut app, "a", none, later(10000));
        let _ = app.update(Message::Library(library::Message::ShowType(None)));
        let _ = app.update(Message::Entry(entry::Message::Add));
        key(&mut app, "a", none, later(10000));
        app.dialog = None;
        let _ = app.update(Message::Show(Screen::Plans));
        key(&mut app, "a", none, later(10000));
        assert_eq!(app.library.query, "é");
        let _ = app.update(Message::Show(Screen::Library));
        key(&mut app, "a", none, later(10000));
        assert_eq!(app.library.query, "a");

        // Ctrl+E edits the selected title.
        app.library
            .update(library::Message::Select("movies/the-mummy-1999".into()));
        let _ = app.update_at(
            Message::KeyPressed(Key::Character("e".into()), keyboard::Modifiers::CTRL, None),
            later(12000),
        );
        assert!(matches!(app.dialog, Some(entry::Dialog::Form(_))));
    }

    #[test]
    fn a_shortcut_changed_in_settings_works_at_once() {
        use crate::keymap::Action;
        use screens::settings::Message as S;
        let press = |app: &mut App, c: &str, modifiers: keyboard::Modifiers| {
            let _ = app.update(Message::KeyPressed(
                Key::Character(c.into()),
                modifiers,
                None,
            ));
        };
        let mut app = App::new(Screen::Settings);
        let _ = app.update(Message::Settings(S::Record(Action::Panel)));
        // Ctrl+K belongs to Search: refused with a note, still waiting.
        press(&mut app, "k", keyboard::Modifiers::CTRL);
        assert_eq!(
            app.settings_page.key_warning.as_deref(),
            Some("Ctrl+K is already used by “Search”.")
        );
        assert_eq!(app.settings_page.recording, Some(Action::Panel));
        press(&mut app, "p", keyboard::Modifiers::CTRL);
        assert_eq!(app.settings_page.recording, None);
        assert_eq!(app.keymap.combo(Action::Panel).to_string(), "Ctrl+P");

        // Esc alone stops waiting and changes nothing.
        let _ = app.update(Message::Settings(S::Record(Action::Edit)));
        let _ = app.update(Message::KeyPressed(
            Key::Named(Named::Escape),
            keyboard::Modifiers::empty(),
            None,
        ));
        assert_eq!(app.settings_page.recording, None);
        assert_eq!(app.keymap.combo(Action::Edit).to_string(), "Ctrl+E");

        // A plain letter would start a search: refused, still waiting.
        let _ = app.update(Message::Settings(S::Record(Action::Edit)));
        let _ = app.update(Message::KeyPressed(
            Key::Character("e".into()),
            keyboard::Modifiers::empty(),
            Some("e".into()),
        ));
        assert_eq!(
            app.settings_page.key_warning.as_deref(),
            Some("Plain letters and digits start a search. Add Ctrl or Alt.")
        );
        assert_eq!(app.settings_page.recording, Some(Action::Edit));
        let _ = app.update(Message::Settings(S::Record(Action::Edit)));

        // Ctrl+, opens Settings; on the library Ctrl+P now pins and
        // unpins, Ctrl+I does nothing.
        let _ = app.update(Message::Show(Screen::Library));
        let _ = app.update(Message::Library(library::Message::ShowType(Some(
            crate::model::MediaType::Movies,
        ))));
        press(&mut app, "i", keyboard::Modifiers::CTRL);
        assert!(app.library.pinned());
        press(&mut app, "p", keyboard::Modifiers::CTRL);
        assert!(!app.library.pinned());
        press(&mut app, ",", keyboard::Modifiers::CTRL);
        assert_eq!(app.screen, Screen::Settings);
        let _ = app.update(Message::History(Direction::Back));
        assert_eq!(app.screen, Screen::Library, "a place the history keeps");

        let _ = app.update(Message::Settings(S::ResetKeys));
        assert!(!app.keymap.is_changed());
    }

    /// Inside the tag picker's long group, Backspace with the search empty,
    /// Back and Esc go back to the groups; the dialog stays. Backspace
    /// with text in the search, or the press that emptied it, stays.
    #[test]
    fn backspace_back_and_esc_leave_the_long_group() {
        use crate::model::TagCategory;
        use tags::PickerMessage;

        let mut app = App::new(Screen::Library);
        let picker = |app: &App| match &app.dialog {
            Some(entry::Dialog::Tags(form)) => form.picker.clone().unwrap(),
            other => panic!("no picker: {other:?}"),
        };
        let send = |app: &mut App, message: Message| {
            let _ = app.update(message);
        };
        let drill = |app: &mut App| {
            send(
                app,
                Message::Entry(entry::Message::Picker(PickerMessage::Drill(
                    TagCategory::Genres,
                ))),
            );
        };
        send(
            &mut app,
            Message::Entry(entry::Message::EditTags(
                "movies/million-dollar-baby-2004".into(),
            )),
        );

        drill(&mut app);
        send(
            &mut app,
            Message::Entry(entry::Message::Picker(PickerMessage::DrillQuery(
                "h".into(),
            ))),
        );
        send(&mut app, Message::Key(KeyPress::Backspace));
        assert!(picker(&app).drill.is_some(), "deletes text");
        send(
            &mut app,
            Message::Entry(entry::Message::Picker(PickerMessage::DrillQuery(
                String::new(),
            ))),
        );
        send(&mut app, Message::Key(KeyPress::Backspace));
        assert!(picker(&app).drill.is_some(), "the press that emptied it");
        send(&mut app, Message::Key(KeyPress::Backspace));
        assert!(picker(&app).drill.is_none(), "an empty search goes back");

        drill(&mut app);
        send(&mut app, Message::History(Direction::Back));
        assert!(picker(&app).drill.is_none());
        assert!(matches!(app.dialog, Some(entry::Dialog::Tags(_))));

        drill(&mut app);
        send(&mut app, Message::Key(KeyPress::Escape));
        assert!(picker(&app).drill.is_none());
        send(&mut app, Message::Key(KeyPress::Escape));
        assert!(app.dialog.is_none(), "Esc at the top closes the sheet");
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

    /// A plain square JPEG of `side` pixels.
    fn jpeg(side: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode_image(&image::RgbImage::new(side, side))
            .expect("a JPEG");
        bytes
    }

    #[test]
    fn a_pick_goes_on_the_picked_list_and_use_automatic_takes_it_off() {
        use covers::artists::MissingEdit;
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        let edits = |app: &App| -> Vec<MissingEdit> {
            let changes = app.engine.as_ref().unwrap().pending().changes();
            let edits = changes.into_iter().filter_map(|change| match change {
                covers::Change::Missing(edit) => Some(edit),
                _ => None,
            });
            edits.collect()
        };
        let pick = |slug: &str| MissingEdit::Pick { slug: slug.into() };
        let unpick = |slug: &str| MissingEdit::Unpick { slug: slug.into() };
        // Until the phone keeps the keys, nothing is written to the list.
        assert!(!covers::artists::WRITES_PHOTO_KEYS && !app.photo_keys);
        app.set_artist_photo("Massive Attack", b"a", covers::PhotoKind::Picked, None);
        assert!(edits(&app).is_empty());

        app.photo_keys = true;
        app.set_artist_photo("Massive Attack", b"b", covers::PhotoKind::Uploaded, None);
        assert_eq!(edits(&app), [pick("massive-attack")]);
        assert!(app.covers.missing().picked.contains("massive-attack"));
        // The suggested photo is what the fetch would have stored.
        app.set_artist_photo("Massive Attack", b"c", covers::PhotoKind::Fetched, None);
        assert_eq!(
            edits(&app),
            [pick("massive-attack"), unpick("massive-attack")]
        );
        assert!(app.covers.missing().picked.is_empty());
        app.set_artist_photo("Massive Attack", b"d", covers::PhotoKind::Picked, None);
        app.remove_artist_photo("Massive Attack", None);
        assert_eq!(edits(&app).last(), Some(&unpick("massive-attack")));
        assert!(app.covers.missing().picked.is_empty());
    }

    #[test]
    fn the_sharper_photos_build_the_picked_list_first_and_never_replace_a_pick() {
        use covers::artists::MissingEdit;
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        app.photo_keys = true;
        let work = |app: &App, name: &str| {
            covers::backfill::work_for(app.repo.library(), name).expect("in the library")
        };
        // Three photos from before the keys were written: one found, one
        // picked here, one set on the phone.
        app.photo_keys = false;
        app.set_artist_photo("Daft Punk", &jpeg(640), covers::PhotoKind::Fetched, None);
        app.set_artist_photo(
            "Alice in Chains",
            &jpeg(600),
            covers::PhotoKind::Picked,
            None,
        );
        app.set_artist_photo("Miles Davis", &jpeg(640), covers::PhotoKind::Fetched, None);
        app.photo_keys = true;
        let sha = |app: &App, name: &str| app.covers.artist_file(name).unwrap().sha.clone();
        let (alice, miles) = (sha(&app, "Alice in Chains"), sha(&app, "Miles Davis"));

        // Offered, since the record has no size yet.
        assert!(matches!(
            covers::backfill::Row::of(
                &app.photos,
                covers::backfill::Totals::default(),
                0,
                true,
                covers::backfill::Sharp {
                    offered: app.covers.missing().photo_size.is_none(),
                    found: 2,
                    yours: 1,
                    left: 0
                }
            ),
            covers::backfill::Row::Sharper { found: 2, yours: 1 }
        ));
        app.photos.start_sharper();
        // Nothing is replaced before the list is built.
        assert!(
            covers::backfill::sharper_todo(app.repo.library(), &app.covers, &app.photos).len() == 2
        );
        let history = vec![
            "Set the photo of Miles Davis".to_owned(),
            "Add photos of 2 artists".to_owned(),
        ];
        let _ = app.update(Message::PhotoHistory(Ok(history)));
        let held: Vec<MissingEdit> = app
            .photos
            .held()
            .iter()
            .filter_map(|change| match change {
                covers::Change::Missing(edit) => Some(edit.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            held,
            [
                MissingEdit::Pick {
                    slug: "alice-in-chains".into()
                },
                MissingEdit::Pick {
                    slug: "miles-davis".into()
                },
                MissingEdit::PhotoSize(1200),
            ]
        );
        // This computer remembered Miles Davis as found, the history says
        // set: picked, and named. Alice in Chains was picked here in a
        // commit the history does not name: picked, and named too.
        assert_eq!(app.photos.disagreed, ["Alice in Chains", "Miles Davis"]);
        assert_eq!(app.covers.missing().photo_side(), 1200);
        let todo = covers::backfill::sharper_todo(app.repo.library(), &app.covers, &app.photos);
        assert_eq!(todo.len(), 1);
        assert_eq!(todo[0].name, "Daft Punk");

        // An answer for a picked artist changes nothing, whatever it is.
        for name in ["Alice in Chains", "Miles Davis"] {
            let _ = app.update(Message::PhotoSharper {
                work: work(&app, name),
                result: Ok(Some(jpeg(1200))),
            });
        }
        assert_eq!(sha(&app, "Alice in Chains"), alice);
        assert_eq!(sha(&app, "Miles Davis"), miles);
        // A copy that is no larger is not stored; a larger one is.
        let before = sha(&app, "Daft Punk");
        app.photos.sharper.as_mut().unwrap().tried.clear();
        let _ = app.update(Message::PhotoSharper {
            work: work(&app, "Daft Punk"),
            result: Ok(Some(jpeg(640))),
        });
        assert_eq!(sha(&app, "Daft Punk"), before);
        app.photos.sharper.as_mut().unwrap().tried.clear();
        let _ = app.update(Message::PhotoSharper {
            work: work(&app, "Daft Punk"),
            result: Ok(Some(jpeg(1200))),
        });
        assert_ne!(sha(&app, "Daft Punk"), before);
        let sharpened: Vec<&covers::Change> = app
            .photos
            .held()
            .iter()
            .filter(|change| matches!(change, covers::Change::Sharpen { .. }))
            .collect();
        assert!(
            matches!(sharpened.as_slice(), [covers::Change::Sharpen { path, .. }] if path == "artists/daft-punk.jpg"),
            "{sharpened:?}"
        );
    }

    #[test]
    fn an_artists_photo_is_queued_over_what_was_seen_and_a_dropped_pick_is_kept_to_tell() {
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        let queued = |app: &App| app.engine.as_ref().unwrap().pending().changes();
        let file = app
            .set_artist_photo("Massive Attack", b"jpeg", covers::PhotoKind::Picked, None)
            .expect("a name with a slug");
        assert_eq!(file.path, "artists/massive-attack.jpg");
        assert_eq!(
            queued(&app),
            [covers::Change::Put {
                path: "artists/massive-attack.jpg".into(),
                seen: vec![None],
                picked: true,
            }]
        );
        assert!(app.covers.artist_file("Massive Attack").is_some());
        assert!(app.save_due.is_some());
        // A name with no letters or digits never gets a file.
        assert!(
            app.set_artist_photo("!!!", b"jpeg", covers::PhotoKind::Picked, None)
                .is_none()
        );
        // Use automatic takes it away again.
        app.remove_artist_photo("Massive Attack", None);
        assert!(app.covers.artist_file("Massive Attack").is_none());
        assert!(matches!(
            queued(&app).last(),
            Some(covers::Change::Remove { .. })
        ));
        // The push found another device's photo there: the pick is
        // dropped and kept to be told.
        app.set_artist_photo("Massive Attack", b"jpeg", covers::PhotoKind::Picked, None);
        app.covers_dropped(vec![
            sync::commit::Dropped {
                path: "artists/massive-attack.jpg".into(),
                picked: true,
            },
            sync::commit::Dropped {
                path: "artists/adele.jpg".into(),
                picked: false,
            },
        ]);
        assert_eq!(app.photos_dropped, ["artists/massive-attack.jpg"]);
        assert!(app.covers.artist_file("Massive Attack").is_none());
    }

    #[test]
    fn apples_answers_wait_in_a_batch_and_are_queued_whole() {
        use lookup::apple_artists::Found;
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        let work = |name: &str| covers::backfill::Work {
            name: name.into(),
            slug: covers::artists::slug(name).unwrap(),
            owned: Vec::new(),
            songs: Vec::new(),
        };
        let queued = |app: &App| app.engine.as_ref().unwrap().pending().files().len();
        let _ = app.photo_found(work("Adele"), Ok(Found::Photo(b"jpeg".to_vec())));
        let reason = covers::artists::Reason::NoMatch;
        let _ = app.photo_found(work("Burial"), Ok(Found::Missing(reason)));
        // Shown and recorded at once, with nothing queued yet.
        assert!(app.covers.artist_file("Adele").is_some());
        assert!(app.covers.missing().artists.contains_key("burial"));
        assert_eq!(queued(&app), 0);
        assert_eq!(app.photos.held().len(), 2);
        // Nobody left: the batch is queued with the backfill's day, under
        // one summary.
        app.queue_photo_batch(true);
        assert!(app.photos.held().is_empty());
        let files = app.engine.as_ref().unwrap().pending().files().to_vec();
        assert_eq!(files.len(), 3);
        assert!(files.iter().all(|file| {
            file.summary.as_deref() == Some("Add photos of 1 artist, 1 without one")
        }));
        assert!(matches!(
            files[2].change,
            covers::Change::Missing(covers::artists::MissingEdit::Backfilled(_))
        ));
        assert!(app.covers.artist_file("Adele").is_some());
        assert!(app.covers.missing().backfilled.is_some());
        // The day is written once.
        app.queue_photo_batch(true);
        assert_eq!(queued(&app), 3);

        // Apple refusing stops the work; a server error only passes the
        // artist over, and the backfill is then not done.
        let refused = lookup::Failure::new(lookup::FailureKind::RateLimited, "403");
        let _ = app.photo_found(work("Can"), Err(refused));
        assert!(app.photos.stopped);
        assert!(app.photos.none_passed_over());
        let _ = app.photo_found(work("Can"), Err(lookup::Failure::other("500")));
        assert!(!app.photos.none_passed_over());
        assert!(app.photos.tried_now().contains("can"));
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

    fn queued_settings(app: &App) -> Vec<shared::Edit> {
        app.queued_shared_edits()
    }

    fn fetched(app: &mut App, text: Option<&str>) {
        let reply = match text {
            Some(text) => Ok(Some(github::RawFile::new(
                text.as_bytes().to_vec(),
                Some("\"etag\"".into()),
            ))),
            None => Err(github::Failure::new(github::FailureKind::NotFound, "gone")),
        };
        let (generation, epoch) = (app.engine_generation, app.shared.epoch);
        let _ = app.update(Message::SharedFetched(Box::new(reply), generation, epoch));
    }

    #[test]
    fn a_settings_change_applies_at_once_and_is_queued_with_its_summary() {
        let mut app = App::new(Screen::Settings);
        connected(&mut app);
        let _ = app.update(Message::Settings(screens::settings::Message::Timeline(
            true,
        )));
        assert!(app.settings.timeline && app.shared.settings.timeline);
        assert_eq!(queued_settings(&app), [shared::Edit::Timeline(true)]);
        assert!(app.save_due.is_some());
        let files = app.engine.as_ref().unwrap().pending().files().to_vec();
        assert_eq!(
            files[0].summary.as_deref(),
            Some("Start new plans as a timeline")
        );
        // The same again changes nothing and is not queued.
        let _ = app.update(Message::Settings(screens::settings::Message::Timeline(
            true,
        )));
        assert_eq!(queued_settings(&app).len(), 1);
        // Not interested goes the same way, so it reaches the phone.
        let _ = app.update(Message::Discover(discover::Message::Check));
        let key = "fake:book:king-2027".to_owned();
        let _ = app.update(Message::Library(library::Message::HideFound(key.clone())));
        assert_eq!(queued_settings(&app)[1], shared::Edit::Hide(key.clone()));
        let _ = app.update(Message::Library(library::Message::RestoreFound(
            key.clone(),
        )));
        assert_eq!(queued_settings(&app)[2], shared::Edit::Restore(key));
        // Kept on this computer nothing is queued, and it still applies.
        let mut local = App::new(Screen::Settings);
        let none = screens::settings::Message::Platform(None);
        let _ = local.update(Message::Settings(none));
        assert_eq!(local.settings.platform, None);
        assert_eq!(local.shared.settings.platform, None);
        assert!(local.save_due.is_none());
    }

    #[test]
    fn githubs_settings_are_taken_with_the_queued_changes_on_top() {
        let mut app = App::new(Screen::Settings);
        connected(&mut app);
        let _ = app.update(Message::Settings(screens::settings::Message::Seasonal(
            false,
        )));
        // The phone picked another platform and unfollowed an artist.
        let theirs = "{\"version\": 1, \"games\": {\"default_platform\": \"Steam Deck\"},
            \"upcoming\": {\"unfollowed\": {\"artists\": [\"metallica\"]}}}";
        // The check has run, so who is no longer active is known already.
        let _ = app.update(Message::Discover(discover::Message::Check));
        let total = app.follows.total();
        fetched(&mut app, Some(theirs));
        assert!(app.shared.known);
        assert_eq!(app.settings.platform.as_deref(), Some("Steam Deck"));
        assert!(!app.settings.seasonal, "the change queued here stays");
        assert_eq!(
            app.follows.total(),
            total - 1,
            "the follows are counted again"
        );
        assert_eq!(app.shared.etag.as_deref(), Some("\"etag\""));
        // Another version's file is left alone, and so are the settings.
        fetched(&mut app, Some("{\"version\": 2}"));
        assert_eq!(app.settings.platform.as_deref(), Some("Steam Deck"));
        // No file there: the defaults, with what is queued.
        fetched(&mut app, None);
        assert_eq!(app.settings.platform.as_deref(), Some("PC"));
        assert!(!app.settings.seasonal);
        assert_eq!(app.follows.total(), total);
        // An answer asked for before a change made here is out of date.
        let (generation, stale) = (app.engine_generation, app.shared.epoch);
        let _ = app.update(Message::Settings(screens::settings::Message::Timeline(
            true,
        )));
        let old = github::RawFile::new(b"{\"version\": 1}".to_vec(), None);
        let reply = Box::new(Ok(Some(old)));
        let _ = app.update(Message::SharedFetched(reply, generation, stale));
        assert!(app.settings.timeline && !app.settings.seasonal);
    }

    #[test]
    fn the_daily_check_waits_for_githubs_settings_and_never_runs_switched_off() {
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        app.github = Some(client());
        let _ = app.update(Message::Discover(discover::Message::Check));
        assert_eq!(
            app.discover.record().checked,
            None,
            "what to follow is not known"
        );
        // The answer starts it.
        fetched(&mut app, Some("{\"version\": 1}"));
        assert!(app.discover.record().checked.is_some());
        // Switched off, not even Check now runs, and nothing is listed.
        let mut app = App::new(Screen::Settings);
        let _ = app.update(Message::Settings(screens::settings::Message::Discover(
            false,
        )));
        let _ = app.update(Message::Discover(discover::Message::Now));
        assert_eq!(app.discover.record().checked, None);
        // Switched on again, the check runs at once.
        let _ = app.update(Message::Settings(screens::settings::Message::Discover(
            true,
        )));
        assert!(app.discover.record().checked.is_some());
        let library = app.repo.library();
        assert!(
            !app.discover
                .visible(library, &app.shared.settings)
                .is_empty()
        );
        let _ = app.update(Message::Settings(screens::settings::Message::Discover(
            false,
        )));
        let library = app.repo.library();
        assert!(
            app.discover
                .visible(library, &app.shared.settings)
                .is_empty()
        );
    }

    #[test]
    fn the_scale_factor_is_kept_for_sharp_slides() {
        let mut app = App::new(Screen::Library);
        assert_eq!(app.scale, 1.0);
        let _ = app.update(Message::Window(window::Event::Rescaled(2.0)));
        assert_eq!(app.scale, 2.0);
        let _ = app.update(Message::ScaleFactor(1.5));
        assert_eq!(app.scale, 1.5);
        let _ = app.update(Message::ScaleFactor(0.0));
        assert_eq!(app.scale, 1.5, "nonsense is ignored");
    }

    #[test]
    fn the_window_coming_back_asks_github_unless_a_check_was_made_moments_ago() {
        let mut app = App::new(Screen::Library);
        connected(&mut app);
        // Checked just now: the focus is noted and GitHub is left alone.
        let _ = app.update(Message::Window(window::Event::Unfocused));
        assert!(!app.focused);
        let _ = app.update(Message::Window(window::Event::Focused));
        assert!(app.focused);
        assert!(!app.engine.as_ref().unwrap().running());
        // Checked longer ago than the guard: the window coming back asks.
        let text = codec::encode(app.repo.library());
        let sha = github::blob_sha(text.as_bytes());
        let old = Timestamp(Timestamp::now().0 - sync::FOCUS_CHECK_GUARD.as_secs() as i64);
        app.engine.as_mut().unwrap().set_base(Base {
            target: target().key(),
            etag: Some(format!("\"{sha}\"")),
            remote_sha: sha.clone(),
            canonical_sha: sha,
            commit_sha: None,
            last_synced: Some(old),
            last_checked: Some(old),
        });
        let _ = app.update(Message::Window(window::Event::Focused));
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

    fn scratch(file: &str) -> Target {
        Target {
            repository: "ekmanch/libri-scratch".into(),
            file: file.into(),
            branch: "main".into(),
        }
    }

    fn client() -> Arc<github::Client> {
        Arc::new(github::Client::new(
            "github_pat_test",
            "ekmanch/libri-scratch",
        ))
    }

    fn pending(app: &App) -> usize {
        app.engine.as_ref().map_or(0, |e| e.status().pending)
    }

    #[test]
    fn reconnecting_keeps_the_queue_for_the_same_file_and_drops_it_for_another() {
        let mut app = App::new(Screen::Library);
        let _ = app.connect(
            scratch("library.json"),
            "github_pat_test".into(),
            client(),
            None,
        );
        let first = app.engine_generation;
        assert!(app.loading, "no base: GitHub's copy is downloaded");
        app.apply(Operation::SetRating {
            id: "movies/the-mummy-1999".into(),
            rating: Some(7),
        });
        assert_eq!(pending(&app), 1);

        // The same file again (restored on GitHub): the queue stays, and
        // an answer to the first engine's request is dropped, so the new
        // round goes on.
        let _ = app.connect(
            scratch("library.json"),
            "github_pat_test".into(),
            client(),
            None,
        );
        assert_eq!(pending(&app), 1);
        assert!(app.engine_generation > first);
        assert!(app.engine.as_ref().unwrap().running());
        let _ = app.update(Message::Sync(SyncMessage::Answer(
            Box::new(Answer::Raw(Ok(None))),
            first,
            Vec::new(),
        )));
        assert!(
            app.engine.as_ref().unwrap().running(),
            "the stale answer changed nothing"
        );
        // GitHub's copy is the fixture, without the rating.
        let bytes = crate::fixtures::LIBRARY_JSON.as_bytes().to_vec();
        let _ = app.update(Message::Sync(SyncMessage::Answer(
            Box::new(Answer::Raw(Ok(Some(github::RawFile::new(bytes, None))))),
            app.engine_generation,
            Vec::new(),
        )));
        assert_eq!(
            app.engine.as_ref().unwrap().status().phase,
            Phase::Saving,
            "the queued change goes out on top of GitHub's copy"
        );

        // Another file: the queue goes with the old engine.
        let _ = app.connect(
            scratch("backup.json"),
            "github_pat_test".into(),
            client(),
            None,
        );
        assert_eq!(pending(&app), 0);
        assert_eq!(app.engine.as_ref().unwrap().target().file, "backup.json");
    }

    #[test]
    fn the_token_warning_is_checked_with_every_round_not_only_at_start() {
        let mut app = App::new(Screen::Library);
        let _ = app.connect(
            scratch("library.json"),
            "github_pat_test".into(),
            client(),
            None,
        );
        app.screen = Screen::Library;
        app.connection.as_mut().unwrap().token_expires =
            Some(Timestamp(Timestamp::now().0 + 3 * 24 * 3600));
        assert!(!app.token_warning);
        let _ = app.update(Message::Sync(SyncMessage::Now));
        assert!(app.token_warning, "the five-minute check warns");
        let today = Timestamp::now().local_date();
        assert_eq!(
            app.connection.as_ref().unwrap().token_warned_on,
            Some(today)
        );
        // Later: closed for the rest of the day.
        app.token_warning = false;
        let _ = app.update(Message::Sync(SyncMessage::Now));
        assert!(!app.token_warning);
        // Over the setup it waits, with a token perhaps on its way in.
        app.connection.as_mut().unwrap().token_warned_on = None;
        app.screen = Screen::Setup;
        let _ = app.update(Message::Sync(SyncMessage::Now));
        assert!(!app.token_warning);
        app.screen = Screen::Plans;
        let _ = app.update(Message::Sync(SyncMessage::Now));
        assert!(app.token_warning);
    }
}
