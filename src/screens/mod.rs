//! One module per screen of the mockup. Each draws from the app state and
//! the theme tokens; shared pieces come from `ui`.

pub mod entry;
pub mod library;
pub mod plans;
mod settings;
mod setup;
pub mod shell;
mod splash;
pub mod token_sheet;

use iced::widget::{container, space};
use iced::{Element, Fill};

use crate::app::{App, Message};
use crate::theme::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    Splash,
    Setup,
    #[default]
    Library,
    Plans,
    Settings,
    /// Throwaway: every color, text style, button and icon, to compare with
    /// the mockup. Removed at the end of phase 1.
    TokenSheet,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Splash,
        Screen::Setup,
        Screen::Library,
        Screen::Plans,
        Screen::Settings,
        Screen::TokenSheet,
    ];

    /// The `LIBRI_SCREEN` value, also used in screenshot file names.
    pub fn name(self) -> &'static str {
        match self {
            Screen::Splash => "splash",
            Screen::Setup => "setup",
            Screen::Library => "library",
            Screen::Plans => "plans",
            Screen::Settings => "settings",
            Screen::TokenSheet => "tokens",
        }
    }

    /// The screen for a `LIBRI_SCREEN` value.
    pub fn from_name(name: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|screen| screen.name() == name)
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let tokens = app.tokens();
    let screen = match app.screen {
        Screen::Splash => splash::view(&tokens),
        Screen::Setup => setup::view(&tokens),
        Screen::Library => library::view(app),
        Screen::Plans => plans::view(app),
        Screen::Settings => settings::view(&tokens),
        Screen::TokenSheet => token_sheet::view(app, tokens),
    };
    entry::overlay(app, plans::overlay(app, screen))
}

/// The screen's part of the window title, or `None` for plain "Libri".
pub fn title(app: &App) -> Option<String> {
    let name = match app.screen {
        Screen::Splash | Screen::Setup => return None,
        Screen::Library => return Some(app.library.title()),
        Screen::Plans => return Some(plans::title(app)),
        Screen::Settings => "Settings",
        Screen::TokenSheet => "Token sheet",
    };
    Some(name.into())
}

/// An empty window in the theme's chrome, until the screen is built.
fn placeholder<'a>(tokens: &Tokens) -> Element<'a, Message> {
    let chrome = tokens.chrome;
    container(space())
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(chrome))
        .into()
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
