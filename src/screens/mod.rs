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
use crate::artwork::Wants;

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

    /// Whether Back and Forward can return here: the splash and the setup
    /// are passed through, not visited.
    pub fn in_history(self) -> bool {
        matches!(self, Screen::Library | Screen::Plans | Screen::Settings)
    }
}

/// What the screen on show draws from the lookups right now (covers,
/// photos, episodes, series, track lists, match thumbnails), so the app
/// fetches what is missing. A text view wants no covers.
pub fn wanted(app: &App) -> Wants {
    let mut wants = Wants::default();
    match app.screen {
        Screen::Library => library::wanted(app, &mut wants),
        Screen::Plans => plans::wanted(app, &mut wants),
        _ => {}
    }
    entry::wanted(app, &mut wants);
    wants
}

/// The inspector's wants for the entry it shows: its cover, its series'
/// parts and, for an album, its track list.
pub fn details_wants(app: &App, id: &str, wants: &mut Wants) {
    let Some(entry) = app.repo.library().entry(id) else {
        return;
    };
    wants.covers.push(entry.id.clone());
    if entry.series.is_some() && entry.can_have_series() {
        wants.series.push(entry.id.clone());
    }
    if entry.media == crate::model::MediaType::Music && !crate::music::is_song(entry) {
        wants.tracks.push(entry.id.clone());
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
    // The conflict dialog and the token warning sit over every dialog.
    let screen = entry::overlay(app, plans::overlay(app, screen));
    token::overlay(app, shell::conflict_overlay(app, screen))
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
