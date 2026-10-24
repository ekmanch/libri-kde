//! What Settings edits: the display preferences, the library's source and
//! the sync state. App state until phase 2 saves the preferences under
//! `~/.config/libri/` and phase 3 connects to GitHub for real.

use crate::theme::{Accent, PEACH, Theme};

/// The Theme setting: System follows Plasma's color scheme, the rest are
/// explicit overrides (docs/DESIGN.md 4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Fixed(Theme),
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 5] = [
        ThemeChoice::System,
        ThemeChoice::Fixed(Theme::Graphite),
        ThemeChoice::Fixed(Theme::Charcoal),
        ThemeChoice::Fixed(Theme::Mocha),
        ThemeChoice::Fixed(Theme::Light),
    ];

    /// The theme drawn: System is Graphite with a dark Plasma and Light
    /// with a light one.
    pub fn resolve(self, plasma_dark: bool) -> Theme {
        match self {
            ThemeChoice::System if plasma_dark => Theme::Graphite,
            ThemeChoice::System => Theme::Light,
            ThemeChoice::Fixed(theme) => theme,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "System",
            ThemeChoice::Fixed(theme) => theme.label(),
        }
    }

    /// The line under the theme's name on its Settings card.
    pub fn blurb(self) -> &'static str {
        match self {
            ThemeChoice::System => "Graphite or Light, following Plasma. The default.",
            ThemeChoice::Fixed(Theme::Graphite) => "Slightly cool gray.",
            ThemeChoice::Fixed(Theme::Charcoal) => "Pure neutral gray. Matches Darkly.",
            ThemeChoice::Fixed(Theme::Mocha) => {
                "Mocha's base and mantle, with Catppuccin type colors."
            }
            ThemeChoice::Fixed(Theme::Light) => "White content, light gray sidebar.",
        }
    }

    /// The help under the Theme setting.
    pub fn help(self, plasma_dark: bool) -> String {
        match self {
            ThemeChoice::System => format!(
                "Follows your Plasma color scheme and switches with it. Right now Plasma is {}",
                if plasma_dark {
                    "dark, so Libri uses Graphite."
                } else {
                    "light, so Libri uses Light."
                }
            ),
            ThemeChoice::Fixed(_) => {
                "Stays the same whatever Plasma uses. Pick System to follow Plasma again.".into()
            }
        }
    }
}

/// The platforms Settings offers as the default for a new game, plus None.
pub const PLATFORMS: [&str; 5] = [
    "PC",
    "Steam Deck",
    "PlayStation 5",
    "Xbox Series X|S",
    "Switch 2",
];

/// The display preferences. The sidebar's tag mode and type checkboxes
/// live with the library state that draws them.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub theme: ThemeChoice,
    /// Background style: one shade for the whole window instead of the
    /// two-tone chrome and content.
    pub flat: bool,
    pub accent: Accent,
    /// New plans start as a timeline.
    pub timeline: bool,
    /// Seasonal touches: themed artwork and colors in Halloween and
    /// Christmas plans.
    pub seasonal: bool,
    /// Picked first when a game is added from a lookup that lists it.
    pub platform: Option<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: ThemeChoice::System,
            flat: false,
            accent: PEACH,
            timeline: false,
            seasonal: true,
            platform: Some(PLATFORMS[0].to_owned()),
        }
    }
}

impl Settings {
    /// The help under Background style.
    pub fn flat_help(&self, light: bool) -> &'static str {
        match (self.flat, light) {
            (true, _) => "One shade for the whole window, like Dolphin.",
            (false, true) => "Light gray sidebar and title bar, white content area.",
            (false, false) => "Darker sidebar and title bar, lighter content area.",
        }
    }

    pub fn timeline_help(&self) -> &'static str {
        if self.timeline {
            "New plans place entries on a timeline. Each plan can still turn it off."
        } else {
            "New plans are a plain checklist without dates. Each plan can still turn the timeline on."
        }
    }

    pub fn seasonal_help(&self) -> &'static str {
        if self.seasonal {
            "Halloween and Christmas plans get themed artwork and colors."
        } else {
            "All plans look the same."
        }
    }
}

/// The GitHub repository the library lives in, as the setup wizard and
/// Settings → Library source show it. Fixed values until phase 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub repo: String,
    pub branch: String,
    pub file: String,
    /// The token's start and end, as Settings shows it; the token itself
    /// goes to KDE Wallet in phase 3.
    pub token_hint: String,
}

impl Source {
    /// The mockup's connection.
    pub fn fixture() -> Source {
        Source {
            repo: "ekmanch/library".into(),
            branch: "main".into(),
            file: "library.json".into(),
            token_hint: "github_pat_••••••a91F".into(),
        }
    }

    /// "ekmanch/library · main", for the content footer.
    pub fn footer(&self) -> String {
        format!("{} · {}", self.repo, self.branch)
    }

    /// "ekmanch/library · main · library.json", for the splash.
    pub fn line(&self) -> String {
        format!("{} · {} · {}", self.repo, self.branch, self.file)
    }
}

/// What is gone on GitHub when the library is not found there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    File,
    Branch,
    Repo,
}

/// The sync row's state, the banner over the content and the token
/// warning (docs/DESIGN.md 4.6 and 10). Faked until phase 3 syncs for
/// real; `LIBRI_SYNC` picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncState {
    #[default]
    Synced,
    /// A commit in flight with this many queued changes.
    Saving {
        changes: u32,
    },
    /// No connection; the changes wait on this computer.
    Offline {
        kept: u32,
    },
    /// GitHub did not answer a save; Libri retries by itself.
    Failed {
        kept: u32,
    },
    /// A 404: the file, the branch or the repo is gone.
    NotFound(Missing),
    /// Changed here and on GitHub since they last matched; nothing syncs
    /// until one copy is picked.
    Conflict {
        local_changes: u32,
        remote_commit: &'static str,
    },
    /// The token expires in this many days; from 7 days before, the
    /// warning dialog shows.
    ExpiresIn(u32),
    TokenExpired,
}

impl SyncState {
    /// `LIBRI_SYNC`: "saving", "offline", "error", "not-found" (or
    /// "not-found-branch", "not-found-repo"), "conflict", "expired", or
    /// "expires-in-6" for a warning.
    pub fn from_env(value: &str) -> Option<SyncState> {
        Some(match value {
            "synced" => SyncState::Synced,
            "saving" => SyncState::Saving { changes: 2 },
            "offline" => SyncState::Offline { kept: 3 },
            "error" | "failed" => SyncState::Failed { kept: 3 },
            "not-found" => SyncState::NotFound(Missing::File),
            "not-found-branch" => SyncState::NotFound(Missing::Branch),
            "not-found-repo" => SyncState::NotFound(Missing::Repo),
            "conflict" => SyncState::Conflict {
                local_changes: 3,
                remote_commit: "Oct 13, 20:12",
            },
            "expired" => SyncState::TokenExpired,
            _ => {
                let days = value.strip_prefix("expires-in-")?.parse().ok()?;
                SyncState::ExpiresIn(days)
            }
        })
    }

    /// Whether the warning dialog shows when Libri opens.
    pub fn warns(self) -> bool {
        matches!(self, SyncState::ExpiresIn(days) if days <= 7)
    }

    /// Whether the state needs the owner to act, so a banner shows over
    /// the content on Library and Plans besides the sidebar row.
    pub fn needs_action(self) -> bool {
        matches!(
            self,
            SyncState::Failed { .. } | SyncState::NotFound(_) | SyncState::Conflict { .. }
        )
    }

    /// Whether the connection is in trouble, for the red dot.
    pub fn is_error(self) -> bool {
        self.needs_action() || self == SyncState::TokenExpired
    }

    /// "library.json is gone from ekmanch/library (main).", for the
    /// not-found states.
    pub fn missing_text(missing: Missing, source: Option<&Source>) -> String {
        let (repo, branch, file) = match source {
            Some(s) => (s.repo.as_str(), s.branch.as_str(), s.file.as_str()),
            None => ("the repository", "main", "library.json"),
        };
        match missing {
            Missing::File => format!("{file} is gone from {repo} ({branch})."),
            Missing::Branch => format!("The branch {branch} is gone from {repo}."),
            Missing::Repo => format!("{repo} is gone, or the token no longer sees it."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sync_switch_names_every_state() {
        assert_eq!(
            SyncState::from_env("offline"),
            Some(SyncState::Offline { kept: 3 })
        );
        assert_eq!(
            SyncState::from_env("not-found-repo"),
            Some(SyncState::NotFound(Missing::Repo))
        );
        assert!(SyncState::from_env("conflict").unwrap().needs_action());
        assert!(!SyncState::from_env("saving").unwrap().needs_action());
        assert!(SyncState::from_env("expired").unwrap().is_error());
        assert_eq!(SyncState::from_env("nope"), None);
        assert_eq!(
            SyncState::missing_text(Missing::File, Some(&Source::fixture())),
            "library.json is gone from ekmanch/library (main)."
        );
    }

    #[test]
    fn system_follows_plasma_and_a_fixed_theme_does_not() {
        assert_eq!(ThemeChoice::System.resolve(true), Theme::Graphite);
        assert_eq!(ThemeChoice::System.resolve(false), Theme::Light);
        let mocha = ThemeChoice::Fixed(Theme::Mocha);
        assert_eq!(mocha.resolve(false), Theme::Mocha);
        assert!(
            ThemeChoice::System
                .help(false)
                .ends_with("Libri uses Light.")
        );
    }

    #[test]
    fn the_sync_state_comes_from_the_environment_value() {
        assert_eq!(
            SyncState::from_env("expired"),
            Some(SyncState::TokenExpired)
        );
        assert_eq!(
            SyncState::from_env("expires-in-6"),
            Some(SyncState::ExpiresIn(6))
        );
        assert_eq!(SyncState::from_env("expires-in-x"), None);
        assert_eq!(SyncState::from_env("synced"), Some(SyncState::Synced));
        assert!(SyncState::ExpiresIn(1).warns());
        assert!(!SyncState::ExpiresIn(12).warns());
        assert!(!SyncState::TokenExpired.warns());
    }

    #[test]
    fn the_source_lines_name_the_repo_branch_and_file() {
        let source = Source::fixture();
        assert_eq!(source.footer(), "ekmanch/library · main");
        assert_eq!(source.line(), "ekmanch/library · main · library.json");
    }
}
