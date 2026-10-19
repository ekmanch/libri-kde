//! Application state, messages, `update` and `view`.

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{operation, scrollable};
use iced::window::{self, settings::PlatformSpecific};
use iced::{Element, Size, Subscription, Task};

use crate::model::Date;
use crate::repo::{FixtureLibraryRepository, LibraryRepository};
use crate::screens::{self, Screen, library, token_sheet};
use crate::theme::{self, Accent, Theme, Tokens, font};

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

/// The sync row's state. Faked until phase 3; `LIBRI_SYNC=expired` shows
/// the expired token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncState {
    #[default]
    Synced,
    TokenExpired,
}

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub accent: Accent,
    pub sync: SyncState,
    /// Today, for Coming soon, Recently added and the plan card.
    pub today: Date,
    /// The keyboard modifiers held now, for Ctrl+click.
    pub modifiers: keyboard::Modifiers,
    pub repo: Box<dyn LibraryRepository>,
    pub library: library::State,
    pub token_sheet: token_sheet::State,
}

#[derive(Debug, Clone)]
pub enum Message {
    Show(Screen),
    SetTheme(Theme),
    SetAccent(Accent),
    Key(KeyPress),
    Modifiers(keyboard::Modifiers),
    Library(library::Message),
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
    Tab { back: bool },
    /// Ctrl+K or Ctrl+F: focuses the library's search.
    Search,
}

impl App {
    /// Starts on the screen named by `LIBRI_SCREEN` and with the sync state
    /// in `LIBRI_SYNC` (development only), or the library, synced.
    fn boot() -> App {
        let screen = std::env::var("LIBRI_SCREEN")
            .ok()
            .and_then(|name| Screen::from_name(&name))
            .unwrap_or_default();
        let mut app = App::new(screen);
        if std::env::var("LIBRI_SYNC").is_ok_and(|state| state == "expired") {
            app.sync = SyncState::TokenExpired;
        }
        app
    }

    pub fn new(screen: Screen) -> App {
        let repo = FixtureLibraryRepository::load().expect("the bundled fixture decodes");
        App {
            screen,
            theme: Theme::default(),
            accent: theme::PEACH,
            sync: SyncState::default(),
            today: Date::today(),
            modifiers: keyboard::Modifiers::default(),
            repo: Box::new(repo),
            library: library::State::default(),
            token_sheet: token_sheet::State::default(),
        }
    }

    pub fn tokens(&self) -> Tokens {
        Tokens::new(self.theme, self.accent)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Show(screen) => self.screen = screen,
            Message::SetTheme(theme) => self.theme = theme,
            Message::SetAccent(accent) => self.accent = accent,
            Message::Key(KeyPress::Tab { back: false }) => return operation::focus_next(),
            Message::Key(KeyPress::Tab { back: true }) => return operation::focus_previous(),
            Message::Key(KeyPress::Escape) => match self.screen {
                Screen::TokenSheet => self.token_sheet.dismiss_top(),
                // The sidebar's All tags popover also opens on Plans.
                _ => {
                    self.library.dismiss_top();
                }
            },
            Message::Key(KeyPress::Enter) => match self.screen {
                Screen::TokenSheet => self.token_sheet.confirm(),
                _ => self.library.confirm(),
            },
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
        keyboard::listen().filter_map(|event| match event {
            keyboard::Event::ModifiersChanged(modifiers) => Some(Message::Modifiers(modifiers)),
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                let press = match key {
                    Key::Named(Named::Escape) => KeyPress::Escape,
                    Key::Named(Named::Enter) => KeyPress::Enter,
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MediaType;

    #[test]
    fn starts_on_the_library_in_graphite_and_peach() {
        let app = App::new(Screen::default());
        assert_eq!(app.screen, Screen::Library);
        assert_eq!(app.theme, Theme::Graphite);
        assert_eq!(app.accent, theme::PEACH);
        assert_eq!(app.sync, SyncState::Synced);
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
        let _ = app.update(Message::SetTheme(Theme::Light));
        let _ = app.update(Message::SetAccent(violet));
        let tokens = app.tokens();
        assert!(tokens.light);
        assert_eq!((tokens.accent, tokens.ink), (violet.fill, violet.ink));
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
}
