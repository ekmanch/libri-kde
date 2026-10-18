//! Application state, messages, `update` and `view`.

use iced::window::{self, settings::PlatformSpecific};
use iced::{Element, Size};

use crate::repo::{FixtureLibraryRepository, LibraryRepository};
use crate::screens::{self, Screen};
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
    let mut application = iced::application(App::boot, App::update, App::view)
        .title("Libri")
        .theme(App::iced_theme)
        .default_font(font::REGULAR)
        .window(window);
    for file in font::FILES {
        application = application.font(file);
    }
    application.run()
}

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub accent: Accent,
    pub repo: Box<dyn LibraryRepository>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Show(Screen),
    SetTheme(Theme),
    SetAccent(Accent),
}

impl App {
    /// Starts on the screen named by `LIBRI_SCREEN` (development only), or
    /// the library.
    fn boot() -> App {
        let screen = std::env::var("LIBRI_SCREEN")
            .ok()
            .and_then(|name| Screen::from_name(&name))
            .unwrap_or_default();
        App::new(screen)
    }

    pub fn new(screen: Screen) -> App {
        let repo = FixtureLibraryRepository::load().expect("the bundled fixture decodes");
        App {
            screen,
            theme: Theme::default(),
            accent: theme::PEACH,
            repo: Box::new(repo),
        }
    }

    pub fn tokens(&self) -> Tokens {
        Tokens::new(self.theme, self.accent)
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Show(screen) => self.screen = screen,
            Message::SetTheme(theme) => self.theme = theme,
            Message::SetAccent(accent) => self.accent = accent,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        screens::view(self)
    }

    fn iced_theme(&self) -> iced::Theme {
        self.tokens().iced_theme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_the_library_in_graphite_and_peach() {
        let app = App::new(Screen::default());
        assert_eq!(app.screen, Screen::Library);
        assert_eq!(app.theme, Theme::Graphite);
        assert_eq!(app.accent, theme::PEACH);
        assert!(!app.repo.library().entries.is_empty());
    }

    #[test]
    fn show_switches_the_screen() {
        let mut app = App::new(Screen::Library);
        app.update(Message::Show(Screen::Plans));
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn theme_and_accent_change_the_tokens() {
        let mut app = App::new(Screen::Library);
        let violet = theme::VIVID[1];
        app.update(Message::SetTheme(Theme::Light));
        app.update(Message::SetAccent(violet));
        let tokens = app.tokens();
        assert!(tokens.light);
        assert_eq!((tokens.accent, tokens.ink), (violet.fill, violet.ink));
    }
}
