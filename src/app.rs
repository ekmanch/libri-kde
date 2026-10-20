//! Application state, messages, `update` and `view`.

use std::time::Duration;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{operation, scrollable};
use iced::window::{self, settings::PlatformSpecific};
use iced::{Element, Size, Subscription, Task, time};

use crate::model::Date;
use crate::repo::Operation;
use crate::repo::{FixtureLibraryRepository, LibraryRepository};
use crate::screens::{self, Screen, entry, library, plans, setup, token, token_sheet};
use crate::settings::{Settings, Source, SyncState, ThemeChoice};
use crate::theme::{Accent, Theme, Tokens, font};

/// The default window size, and for now the minimum (settled in 1g).
pub const WINDOW_SIZE: Size = Size::new(1600.0, 900.0);
pub const MIN_WINDOW_SIZE: Size = Size::new(1100.0, 680.0);

pub fn run() -> iced::Result {
    let window = window::Settings {
        size: WINDOW_SIZE,
        min_size: Some(MIN_WINDOW_SIZE),
        // KDE groups the window and finds its icon by the app id.
        platform_specific: PlatformSpecific {
            application_id: "libri".into(),
            ..PlatformSpecific::default()
        },
        ..window::Settings::default()
    };
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::iced_theme)
        .subscription(App::subscription)
        .settings(settings())
        .window(window)
        .run()
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
    /// Plasma's color scheme, for the System theme. `LIBRI_PLASMA=light`
    /// fakes a light one until the desktop portal is read.
    pub plasma_dark: bool,
    /// The connected repository, or `None` on a first run.
    pub source: Option<Source>,
    pub sync: SyncState,
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
    pub token_sheet: token_sheet::State,
    /// Where the splash's bar is in its sweep, 0 to 1.
    pub splash_phase: f32,
}

#[derive(Debug, Clone)]
pub enum Message {
    Show(Screen),
    SetTheme(ThemeChoice),
    SetAccent(Accent),
    Key(KeyPress),
    Modifiers(keyboard::Modifiers),
    /// A frame of the splash's bar.
    Tick,
    Library(library::Message),
    Entry(entry::Message),
    Plans(plans::Message),
    Setup(setup::Message),
    Settings(screens::settings::Message),
    Token(token::Message),
    TokenSheet(token_sheet::Message),
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
    /// The arrow keys, in the entry form's lookup.
    Up,
    Down,
    /// E: edits the selected entry.
    Edit,
}

impl App {
    /// Starts on the screen named by `LIBRI_SCREEN` (development only),
    /// or the library. `LIBRI_FIRST_RUN` starts with no repository,
    /// `LIBRI_SYNC` fakes the sync state ("expired", "expires-in-6"),
    /// `LIBRI_PLASMA=light` a light Plasma, and `LIBRI_SETUP` picks the
    /// setup step (welcome, connect, branch, file, create, existing, done).
    fn boot() -> App {
        let var = |name| std::env::var(name).ok();
        let screen = var("LIBRI_SCREEN")
            .and_then(|name| Screen::from_name(&name))
            .unwrap_or_default();
        let mut app = App::new(screen);
        if var("LIBRI_FIRST_RUN").is_some() {
            app.source = None;
            app.setup = setup::State::at(setup::Step::Welcome, None);
        }
        if let Some(sync) = var("LIBRI_SYNC").and_then(|value| SyncState::from_env(&value)) {
            app.sync = sync;
            app.token_warning = sync.warns();
        }
        if var("LIBRI_PLASMA").is_some_and(|value| value == "light") {
            app.plasma_dark = false;
        }
        if let Some(step) = var("LIBRI_SETUP").and_then(|name| setup::Step::from_name(&name)) {
            app.setup = setup::State::at(step, app.source.as_ref());
        }
        app
    }

    /// The app on `screen`, connected to the fixture repository.
    pub fn new(screen: Screen) -> App {
        let repo = FixtureLibraryRepository::load().expect("the bundled fixture decodes");
        let source = Source::fixture();
        App {
            screen,
            settings: Settings::default(),
            plasma_dark: true,
            setup: setup::State::at(setup::Step::Welcome, Some(&source)),
            source: Some(source),
            sync: SyncState::default(),
            token_warning: false,
            today: Date::today(),
            modifiers: keyboard::Modifiers::default(),
            repo: Box::new(repo),
            library: library::State::default(),
            dialog: None,
            plans: plans::State::default(),
            plan_dialog: None,
            settings_page: screens::settings::State::default(),
            token_sheet: token_sheet::State::default(),
            splash_phase: 0.0,
        }
    }

    /// Applies one change to the library. A change that cannot apply (an
    /// entry gone meanwhile) leaves the library as it was.
    pub fn apply(&mut self, operation: Operation) {
        if let Err(error) = self.repo.apply(operation) {
            eprintln!("libri: change not applied: {error}");
        }
    }

    /// The theme drawn: the Theme setting, with System resolved by Plasma.
    pub fn theme(&self) -> Theme {
        self.settings.theme.resolve(self.plasma_dark)
    }

    pub fn tokens(&self) -> Tokens {
        Tokens::styled(self.theme(), self.settings.accent, self.settings.flat)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Show(screen) => self.screen = screen,
            Message::SetTheme(choice) => self.settings.theme = choice,
            Message::SetAccent(accent) => self.settings.accent = accent,
            // One sweep of the bar takes 1.4 s.
            Message::Tick => self.splash_phase = (self.splash_phase + 0.016 / 1.4) % 1.0,
            Message::Key(KeyPress::Tab { back: false }) => return operation::focus_next(),
            Message::Key(KeyPress::Tab { back: true }) => return operation::focus_previous(),
            Message::Key(KeyPress::Escape) => match self.screen {
                Screen::TokenSheet => self.token_sheet.dismiss_top(),
                // The token warning sits over everything; then a dialog;
                // the sidebar's All tags popover also opens on Plans.
                _ if self.token_warning => self.token_warning = false,
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
                Screen::TokenSheet => self.token_sheet.confirm(),
                _ if self.token_warning => return token::update(self, token::Message::Replace),
                Screen::Setup => return setup::update(self, setup::Message::Continue),
                Screen::Settings if self.settings_page.disconnect_open => {
                    return screens::settings::update(
                        self,
                        screens::settings::Message::ConfirmDisconnect,
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
            Message::Key(KeyPress::Up) => return entry::update(self, entry::Message::MovePick(-1)),
            Message::Key(KeyPress::Down) => {
                return entry::update(self, entry::Message::MovePick(1));
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
                self.screen = Screen::Plans;
                return plans::update(self, message);
            }
            Message::Setup(message) => return setup::update(self, message),
            Message::Settings(message) => return screens::settings::update(self, message),
            Message::Token(message) => return token::update(self, message),
            Message::TokenSheet(message) => self.token_sheet.update(message),
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
                    _ => return None,
                };
                Some(Message::Key(press))
            }
            _ => None,
        });
        if self.screen == Screen::Splash {
            let frames = time::every(Duration::from_millis(16)).map(|_| Message::Tick);
            Subscription::batch([keys, frames])
        } else {
            keys
        }
    }
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
    fn the_splash_ticks_its_bar() {
        let mut app = App::new(Screen::Splash);
        let _ = app.update(Message::Tick);
        assert!(app.splash_phase > 0.0 && app.splash_phase < 1.0);
    }
}
