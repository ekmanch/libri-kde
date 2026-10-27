//! The display preferences on this computer: `preferences.json` under
//! `storage::config_dir()`, never synced (docs/DESIGN.md section 10,
//! phase 2). What is kept: Settings' choices (theme, background style,
//! accent, timeline default, seasonal touches, default platform), the
//! sidebar's tag mode, pins and type checkboxes, the view per type and
//! each plan's look. The file is written whenever one of them changes.
//!
//! The file is plain JSON with readable words, so a hand edit or an
//! older file reads back: a key that is missing or holds a value Libri
//! does not know keeps that preference's default.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::model::MediaType;
use crate::screens::library::{SidebarTags, View, ViewKey};
use crate::screens::plans::Style;
use crate::settings::{Settings, ThemeChoice};
use crate::storage;
use crate::theme::{Accent, CATPPUCCIN, VIVID};

/// The preferences as the file holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub version: u32,
    /// "system", "graphite", "charcoal", "mocha", "glass" or "light".
    pub theme: String,
    pub flat: bool,
    /// "vivid:Peach" or "catppuccin:Mauve": the group and the name, since
    /// both groups have a Blue and a Green.
    pub accent: String,
    pub timeline: bool,
    pub seasonal: bool,
    /// The default platform for a new game; `None` is Settings' "None".
    pub platform: Option<String>,
    /// "pinned" or "most_used".
    pub sidebar_tags: String,
    pub pins: Vec<String>,
    pub type_boxes: bool,
    /// The view ("grid", "list", "text") per list: "all", "several" or a
    /// type ("movies").
    pub views: BTreeMap<String, String>,
    /// Each plan's look, by plan id.
    pub plans: BTreeMap<String, PlanLook>,
}

/// How a plan's sections look on this device (`plans::Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLook {
    pub cards: bool,
    pub covers: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        let settings = Settings::default();
        Preferences {
            version: 1,
            theme: theme_key(settings.theme).to_owned(),
            flat: settings.flat,
            accent: accent_key(settings.accent),
            timeline: settings.timeline,
            seasonal: settings.seasonal,
            platform: settings.platform,
            sidebar_tags: sidebar_key(SidebarTags::default()).to_owned(),
            pins: Vec::new(),
            type_boxes: false,
            views: BTreeMap::new(),
            plans: BTreeMap::new(),
        }
    }
}

impl Preferences {
    /// The app's preferences as they are now.
    pub fn of(app: &App) -> Preferences {
        let settings = &app.settings;
        Preferences {
            version: 1,
            theme: theme_key(settings.theme).to_owned(),
            flat: settings.flat,
            accent: accent_key(settings.accent),
            timeline: settings.timeline,
            seasonal: settings.seasonal,
            platform: settings.platform.clone(),
            sidebar_tags: sidebar_key(app.library.sidebar_tags).to_owned(),
            pins: app.library.pins.clone(),
            type_boxes: app.library.type_boxes,
            views: app
                .library
                .views
                .iter()
                .map(|(key, view)| (view_key(*key).to_owned(), view_name(*view).to_owned()))
                .collect(),
            plans: app
                .plans
                .styles
                .iter()
                .map(|(id, style)| {
                    (
                        id.clone(),
                        PlanLook {
                            cards: style.cards,
                            covers: style.covers,
                        },
                    )
                })
                .collect(),
        }
    }

    /// Puts the preferences on the app. A value Libri does not know
    /// leaves that preference as it is.
    pub fn apply_to(&self, app: &mut App) {
        let settings = &mut app.settings;
        if let Some(theme) = ThemeChoice::from_key(&self.theme) {
            settings.theme = theme;
        }
        settings.flat = self.flat;
        if let Some(accent) = accent_from_key(&self.accent) {
            settings.accent = accent;
        }
        settings.timeline = self.timeline;
        settings.seasonal = self.seasonal;
        settings.platform = self.platform.clone();
        if let Some(mode) = sidebar_from_key(&self.sidebar_tags) {
            app.library.sidebar_tags = mode;
        }
        app.library.pins = self.pins.clone();
        app.library.type_boxes = self.type_boxes;
        app.library.views = self
            .views
            .iter()
            .filter_map(|(key, view)| Some((view_key_from(key)?, view_from_name(view)?)))
            .collect();
        app.plans.styles = self
            .plans
            .iter()
            .map(|(id, look)| {
                (
                    id.clone(),
                    Style {
                        cards: look.cards,
                        covers: look.covers,
                    },
                )
            })
            .collect();
    }

    /// Pretty JSON with a trailing newline.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("preferences encode");
        text.push('\n');
        text
    }

    pub fn from_json(text: &str) -> Result<Preferences, serde_json::Error> {
        serde_json::from_str(text)
    }
}

fn theme_key(theme: ThemeChoice) -> &'static str {
    theme.key()
}

fn accent_key(accent: Accent) -> String {
    let group = if VIVID.contains(&accent) {
        "vivid"
    } else {
        "catppuccin"
    };
    format!("{group}:{}", accent.name)
}

fn accent_from_key(key: &str) -> Option<Accent> {
    let (group, name) = key.split_once(':')?;
    let group: &[Accent] = match group {
        "vivid" => &VIVID,
        "catppuccin" => &CATPPUCCIN,
        _ => return None,
    };
    group.iter().copied().find(|accent| accent.name == name)
}

fn sidebar_key(mode: SidebarTags) -> &'static str {
    match mode {
        SidebarTags::Pinned => "pinned",
        SidebarTags::MostUsed => "most_used",
    }
}

fn sidebar_from_key(key: &str) -> Option<SidebarTags> {
    match key {
        "pinned" => Some(SidebarTags::Pinned),
        "most_used" => Some(SidebarTags::MostUsed),
        _ => None,
    }
}

fn view_key(key: ViewKey) -> &'static str {
    match key {
        ViewKey::All => "all",
        ViewKey::Several => "several",
        ViewKey::Type(media) => media.key(),
    }
}

fn view_key_from(key: &str) -> Option<ViewKey> {
    match key {
        "all" => Some(ViewKey::All),
        "several" => Some(ViewKey::Several),
        _ => MediaType::ALL
            .into_iter()
            .find(|media| media.key() == key)
            .map(ViewKey::Type),
    }
}

fn view_name(view: View) -> &'static str {
    match view {
        View::Grid => "grid",
        View::List => "list",
        View::Text => "text",
    }
}

fn view_from_name(name: &str) -> Option<View> {
    match name {
        "grid" => Some(View::Grid),
        "list" => Some(View::List),
        "text" => Some(View::Text),
        _ => None,
    }
}

/// The preferences file: where it is, and what it last held, so a change
/// writes it and anything else does not.
#[derive(Debug, Default)]
pub struct PreferencesFile {
    /// `None` keeps nothing: the tests, and a run whose look is faked by
    /// a dev switch.
    path: Option<PathBuf>,
    saved: Option<Preferences>,
}

impl PreferencesFile {
    /// `preferences.json` at `path`.
    pub fn at(path: PathBuf) -> PreferencesFile {
        PreferencesFile {
            path: Some(path),
            saved: None,
        }
    }

    /// Reads the file: `None` when there is none, or it cannot be read.
    pub fn load(&mut self) -> Option<Preferences> {
        let path = self.path.as_ref()?;
        let text = match storage::read_if_exists(path) {
            Ok(text) => text?,
            Err(error) => {
                eprintln!("libri: could not read {}: {error}", path.display());
                return None;
            }
        };
        match Preferences::from_json(&text) {
            Ok(preferences) => {
                self.saved = Some(preferences.clone());
                Some(preferences)
            }
            Err(error) => {
                eprintln!(
                    "libri: {} is not valid, using the defaults: {error}",
                    path.display()
                );
                None
            }
        }
    }

    /// Takes what is on the app as the saved state, so only later changes
    /// are written.
    pub fn mark_saved(&mut self, now: Preferences) {
        self.saved = Some(now);
    }

    /// Stops writing: the look is faked by a dev switch, and that must not
    /// become the preference.
    pub fn detach(&mut self) {
        self.path = None;
    }

    /// Whether changes are written somewhere.
    pub fn is_attached(&self) -> bool {
        self.path.is_some()
    }

    /// Writes `now` when it differs from what was last written.
    pub fn save_if_changed(&mut self, now: Preferences) {
        let Some(path) = &self.path else {
            return;
        };
        if self.saved.as_ref() == Some(&now) {
            return;
        }
        match storage::write_atomically(path, &now.to_json()) {
            Ok(()) => self.saved = Some(now),
            Err(error) => {
                let message = format!("could not write {}: {error}", path.display());
                eprintln!("libri: {message}");
                crate::trace::line(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;
    use crate::storage::TempDir;
    use crate::theme::Theme;

    #[test]
    fn the_preferences_round_trip_through_json() {
        let mut app = App::new(Screen::Library);
        app.settings.theme = ThemeChoice::Fixed(Theme::Mocha);
        app.settings.flat = true;
        app.settings.accent = CATPPUCCIN[7];
        app.settings.timeline = true;
        app.settings.seasonal = false;
        app.settings.platform = None;
        app.library.sidebar_tags = SidebarTags::MostUsed;
        app.library.pins = vec!["horror".into(), "classic".into()];
        app.library.type_boxes = true;
        app.library
            .views
            .insert(ViewKey::Type(MediaType::Books), View::Text);
        app.library.views.insert(ViewKey::All, View::List);
        app.plans.styles.insert(
            "halloween-2026".into(),
            Style {
                cards: true,
                covers: true,
            },
        );
        let preferences = Preferences::of(&app);
        assert_eq!(preferences.accent, "catppuccin:Blue");
        assert_eq!(preferences.theme, "mocha");
        let text = preferences.to_json();
        assert!(text.ends_with("}\n"));
        assert!(text.contains("\"sidebarTags\": \"most_used\""));
        let read = Preferences::from_json(&text).unwrap();
        assert_eq!(read, preferences);

        let mut again = App::new(Screen::Library);
        read.apply_to(&mut again);
        assert_eq!(again.settings, app.settings);
        assert_eq!(again.library.pins, app.library.pins);
        assert_eq!(again.library.views, app.library.views);
        assert_eq!(again.library.sidebar_tags, SidebarTags::MostUsed);
        assert!(again.library.type_boxes);
        assert_eq!(again.plans.styles, app.plans.styles);
    }

    #[test]
    fn a_partial_or_odd_file_falls_back_to_the_defaults() {
        let text = r#"{ "theme": "neon", "accent": "vivid:Blue", "views": { "movies": "text", "comics": "grid", "all": "cube" }, "pins": ["x"] }"#;
        let read = Preferences::from_json(text).unwrap();
        let mut app = App::new(Screen::Library);
        read.apply_to(&mut app);
        assert_eq!(app.settings.theme, ThemeChoice::System, "unknown theme");
        assert_eq!(app.settings.accent.name, "Blue");
        assert_eq!(
            app.settings.accent,
            VIVID.iter().copied().find(|a| a.name == "Blue").unwrap()
        );
        assert_eq!(app.library.pins, vec!["x".to_owned()]);
        assert_eq!(
            app.library.views.get(&ViewKey::Type(MediaType::Movies)),
            Some(&View::Text)
        );
        assert_eq!(
            app.library.views.len(),
            1,
            "unknown keys and views are dropped"
        );
        assert!(app.settings.seasonal, "a missing key keeps the default");
    }

    #[test]
    fn the_file_is_written_only_when_something_changed() {
        let dir = TempDir::new("preferences");
        let path = dir.path("preferences.json");
        let mut file = PreferencesFile::at(path.clone());
        assert_eq!(file.load(), None);
        let app = App::new(Screen::Library);
        let first = Preferences::of(&app);
        file.mark_saved(first.clone());
        file.save_if_changed(first.clone());
        assert!(!path.exists(), "nothing changed since the start");

        let changed = Preferences {
            flat: true,
            ..first.clone()
        };
        file.save_if_changed(changed.clone());
        let written = std::fs::metadata(&path).unwrap().modified().unwrap();
        file.save_if_changed(changed.clone());
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            written
        );

        let mut again = PreferencesFile::at(path);
        assert_eq!(again.load(), Some(changed));
        let mut detached = PreferencesFile::default();
        detached.save_if_changed(first);
        assert!(!dir.path("other.json").exists());
        assert!(!detached.is_attached());
    }
}
