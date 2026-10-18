//! One module per screen of the mockup. Each draws from the app state and
//! the theme tokens; shared pieces come from `ui`.

mod library;
mod plans;
mod settings;
mod setup;
mod splash;
mod token_sheet;

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
    match app.screen {
        Screen::Splash => splash::view(&tokens),
        Screen::Setup => setup::view(&tokens),
        Screen::Library => library::view(&tokens),
        Screen::Plans => plans::view(&tokens),
        Screen::Settings => settings::view(&tokens),
        Screen::TokenSheet => token_sheet::view(app, tokens),
    }
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
