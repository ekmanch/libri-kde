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
    /// The screen for a `LIBRI_SCREEN` value.
    pub fn from_name(name: &str) -> Option<Screen> {
        Some(match name {
            "splash" => Screen::Splash,
            "setup" => Screen::Setup,
            "library" => Screen::Library,
            "plans" => Screen::Plans,
            "settings" => Screen::Settings,
            "tokens" => Screen::TokenSheet,
            _ => return None,
        })
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
        for name in ["splash", "setup", "library", "plans", "settings", "tokens"] {
            assert!(Screen::from_name(name).is_some(), "{name}");
        }
        assert_eq!(Screen::from_name("nope"), None);
    }
}
