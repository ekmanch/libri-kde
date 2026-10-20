//! One module per screen of the mockup. Each draws from the app state and
//! the theme tokens; shared pieces come from `ui`.

pub mod entry;
pub mod library;
pub mod plans;
pub mod settings;
pub mod setup;
pub mod shell;
pub mod splash;
pub mod token;

use iced::Element;

use crate::app::{App, Message};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    Splash,
    Setup,
    #[default]
    Library,
    Plans,
    Settings,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Splash,
        Screen::Setup,
        Screen::Library,
        Screen::Plans,
        Screen::Settings,
    ];

    /// The `LIBRI_SCREEN` value, also used in screenshot file names.
    pub fn name(self) -> &'static str {
        match self {
            Screen::Splash => "splash",
            Screen::Setup => "setup",
            Screen::Library => "library",
            Screen::Plans => "plans",
            Screen::Settings => "settings",
        }
    }

    /// The screen for a `LIBRI_SCREEN` value.
    pub fn from_name(name: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|screen| screen.name() == name)
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let screen = match app.screen {
        Screen::Splash => splash::view(app),
        Screen::Setup => setup::view(app),
        Screen::Library => library::view(app),
        Screen::Plans => plans::view(app),
        Screen::Settings => settings::view(app),
    };
    // The token warning sits over every dialog.
    token::overlay(app, entry::overlay(app, plans::overlay(app, screen)))
}

/// The screen's part of the window title, or `None` for plain "Libri".
pub fn title(app: &App) -> Option<String> {
    let name = match app.screen {
        Screen::Splash => return None,
        Screen::Setup => return app.setup.in_wizard().then(|| "Set up Libri".into()),
        Screen::Library => return Some(app.library.title()),
        Screen::Plans => return Some(plans::title(app)),
        Screen::Settings => "Settings",
    };
    Some(name.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_has_a_name() {
        for screen in Screen::ALL {
            assert_eq!(Screen::from_name(screen.name()), Some(screen));
        }
        assert_eq!(Screen::from_name("nope"), None);
    }
}
