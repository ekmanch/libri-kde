//! The keyboard shortcuts Settings › Keyboard and mouse can change: each
//! action's keys, the defaults, and the keys that stay fixed (Enter, Tab,
//! the arrow keys). A key press becomes a [`Combo`]; the app looks up the
//! action bound to it, so a changed shortcut works at once. Only the
//! changed ones are saved, in `preferences.json` (`keys`).

use std::collections::BTreeMap;
use std::fmt;

use iced::keyboard::{Key, Modifiers, key::Named};

/// What a shortcut does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Search,
    Panel,
    Add,
    Edit,
    Close,
    Back,
    Forward,
    Settings,
    /// Coming soon: the slice before (Library ← Discover ← All) and after;
    /// on Music the tab before (Artists ← Albums ← Songs) and after.
    SoonPrev,
    SoonNext,
}

impl Action {
    /// In the order Settings lists them.
    pub const ALL: [Action; 10] = [
        Action::Search,
        Action::Panel,
        Action::Add,
        Action::Edit,
        Action::Close,
        Action::Back,
        Action::Forward,
        Action::Settings,
        Action::SoonPrev,
        Action::SoonNext,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Search => "Search",
            Action::Panel => "Pin or unpin the details panel",
            Action::Add => "Add a title",
            Action::Edit => "Edit the selected title",
            Action::Close => "Close details or a dialog",
            Action::Back => "Back",
            Action::Forward => "Forward",
            Action::Settings => "Settings",
            Action::SoonPrev => "Coming soon, Music and plans: previous list or tab",
            Action::SoonNext => "Coming soon, Music and plans: next list or tab",
        }
    }

    /// The key in `preferences.json`.
    pub fn key(self) -> &'static str {
        match self {
            Action::Search => "search",
            Action::Panel => "panel",
            Action::Add => "add",
            Action::Edit => "edit",
            Action::Close => "close",
            Action::Back => "back",
            Action::Forward => "forward",
            Action::Settings => "settings",
            Action::SoonPrev => "soon_prev",
            Action::SoonNext => "soon_next",
        }
    }

    pub fn from_key(key: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.key() == key)
    }

    pub fn default_combo(self) -> Combo {
        let (ctrl, alt, shift, key) = match self {
            Action::Search => (true, false, false, "K"),
            Action::Panel => (true, false, false, "I"),
            Action::Add => (true, false, false, "N"),
            Action::Edit => (false, false, false, "E"),
            Action::Close => (false, false, false, "Esc"),
            Action::Back => (false, true, false, "←"),
            Action::Forward => (false, true, false, "→"),
            Action::Settings => (true, false, false, ","),
            Action::SoonPrev => (false, false, true, "←"),
            Action::SoonNext => (false, false, true, "→"),
        };
        Combo {
            ctrl,
            alt,
            shift,
            logo: false,
            key: key.into(),
        }
    }
}

/// A key with the modifiers held, as Settings shows it: "Ctrl+I", "E",
/// "Alt+←".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    /// The key's name: a character in capitals, "Esc", "Space", "←", "F5".
    pub key: String,
}

impl Combo {
    /// The combo a key press makes, or `None` for a modifier on its own.
    pub fn of(key: &Key, modifiers: Modifiers) -> Option<Combo> {
        let name = match key {
            Key::Character(c) => c.to_uppercase(),
            Key::Named(named) => match named {
                Named::Control | Named::Shift | Named::Alt | Named::Super | Named::Meta => {
                    return None;
                }
                Named::Escape => "Esc".into(),
                Named::Space => "Space".into(),
                Named::ArrowLeft => "←".into(),
                Named::ArrowRight => "→".into(),
                Named::ArrowUp => "↑".into(),
                Named::ArrowDown => "↓".into(),
                other => format!("{other:?}"),
            },
            Key::Unidentified => return None,
        };
        Some(Combo {
            ctrl: modifiers.control(),
            alt: modifiers.alt(),
            shift: modifiers.shift(),
            logo: modifiers.logo(),
            key: name,
        })
    }

    /// Reads what [`Combo`]'s `Display` writes.
    pub fn parse(text: &str) -> Option<Combo> {
        let mut combo = Combo {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: String::new(),
        };
        let mut rest = text;
        loop {
            let (flag, prefix) = if rest.starts_with("Ctrl+") {
                (&mut combo.ctrl, "Ctrl+")
            } else if rest.starts_with("Alt+") {
                (&mut combo.alt, "Alt+")
            } else if rest.starts_with("Shift+") {
                (&mut combo.shift, "Shift+")
            } else if rest.starts_with("Meta+") {
                (&mut combo.logo, "Meta+")
            } else {
                break;
            };
            *flag = true;
            rest = &rest[prefix.len()..];
        }
        if rest.is_empty() {
            return None;
        }
        combo.key = rest.to_owned();
        Some(combo)
    }

    fn plain(key: &str) -> Combo {
        Combo {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: key.into(),
        }
    }

    fn with_ctrl(mut self) -> Combo {
        self.ctrl = true;
        self
    }

    fn with_shift(mut self) -> Combo {
        self.shift = true;
        self
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
            (self.logo, "Meta+"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

/// The keys that cannot be given to a shortcut, with what they do.
fn fixed() -> [(Combo, &'static str); 6] {
    [
        (Combo::plain("Enter"), "Confirm a dialog"),
        (Combo::plain("Enter").with_ctrl(), "Save the entry form"),
        (Combo::plain("Tab"), "Move the focus"),
        (Combo::plain("Tab").with_shift(), "Move the focus"),
        (Combo::plain("↑"), "Move the selection"),
        (Combo::plain("↓"), "Move the selection"),
    ]
}

/// The shortcuts as they are now: the defaults with the changed ones over
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keymap {
    changed: BTreeMap<Action, Combo>,
}

impl Keymap {
    pub fn combo(&self, action: Action) -> Combo {
        self.changed
            .get(&action)
            .cloned()
            .unwrap_or_else(|| action.default_combo())
    }

    /// The action `combo` runs. Ctrl+F also searches, as long as no
    /// shortcut has taken it.
    pub fn action_for(&self, combo: &Combo) -> Option<Action> {
        Action::ALL
            .into_iter()
            .find(|&a| self.combo(a) == *combo)
            .or_else(|| (*combo == Combo::plain("F").with_ctrl()).then_some(Action::Search))
    }

    pub fn is_changed(&self) -> bool {
        !self.changed.is_empty()
    }

    /// Gives `action` the keys `combo`, or says why not: "Ctrl+K is
    /// already used by “Search”."
    pub fn bind(&mut self, action: Action, combo: Combo) -> Result<(), String> {
        let clash = Action::ALL
            .into_iter()
            .filter(|&a| a != action)
            .find(|&a| self.combo(a) == combo)
            .map(Action::label)
            .or_else(|| {
                fixed()
                    .into_iter()
                    .find(|(c, _)| *c == combo)
                    .map(|(_, what)| what)
            });
        if let Some(what) = clash {
            return Err(format!("{combo} is already used by “{what}”."));
        }
        if combo == action.default_combo() {
            self.changed.remove(&action);
        } else {
            self.changed.insert(action, combo);
        }
        Ok(())
    }

    pub fn reset(&mut self) {
        self.changed.clear();
    }

    /// The changed shortcuts by their `preferences.json` key.
    pub fn to_prefs(&self) -> BTreeMap<String, String> {
        self.changed
            .iter()
            .map(|(a, c)| (a.key().to_owned(), c.to_string()))
            .collect()
    }

    /// From `preferences.json`: an action or keys Libri does not know, or
    /// keys another shortcut has, are left out.
    pub fn from_prefs(keys: &BTreeMap<String, String>) -> Keymap {
        let mut map = Keymap::default();
        for (action, combo) in keys {
            if let (Some(action), Some(combo)) = (Action::from_key(action), Combo::parse(combo)) {
                let _ = map.bind(action, combo);
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_read_back_what_they_write() {
        for text in ["Ctrl+I", "E", "Alt+←", "Ctrl+,", "Ctrl+Shift+Esc", "Ctrl++"] {
            assert_eq!(Combo::parse(text).unwrap().to_string(), text);
        }
        assert_eq!(Combo::parse("Ctrl+"), None);
    }

    #[test]
    fn a_key_press_names_its_combo() {
        let combo = Combo::of(&Key::Character("i".into()), Modifiers::CTRL).unwrap();
        assert_eq!(combo.to_string(), "Ctrl+I");
        let back = Combo::of(&Key::Named(Named::ArrowLeft), Modifiers::ALT).unwrap();
        assert_eq!(back.to_string(), "Alt+←");
        assert_eq!(
            Combo::of(&Key::Named(Named::Control), Modifiers::CTRL),
            None
        );
    }

    #[test]
    fn a_shortcut_moves_unless_another_has_the_keys() {
        let mut map = Keymap::default();
        let ctrl_k = Combo::parse("Ctrl+K").unwrap();
        assert_eq!(map.action_for(&ctrl_k), Some(Action::Search));
        assert_eq!(
            map.bind(Action::Panel, ctrl_k),
            Err("Ctrl+K is already used by “Search”.".into())
        );
        assert_eq!(
            map.bind(Action::Panel, Combo::parse("Enter").unwrap()),
            Err("Enter is already used by “Confirm a dialog”.".into())
        );
        let ctrl_p = Combo::parse("Ctrl+P").unwrap();
        map.bind(Action::Panel, ctrl_p.clone()).unwrap();
        assert!(map.is_changed());
        assert_eq!(map.action_for(&ctrl_p), Some(Action::Panel));
        assert_eq!(map.action_for(&Combo::parse("Ctrl+I").unwrap()), None);
        // Ctrl+F searches too, until a shortcut takes it.
        let ctrl_f = Combo::parse("Ctrl+F").unwrap();
        assert_eq!(map.action_for(&ctrl_f), Some(Action::Search));
        map.bind(Action::Add, ctrl_f.clone()).unwrap();
        assert_eq!(map.action_for(&ctrl_f), Some(Action::Add));

        let saved = Keymap::from_prefs(&map.to_prefs());
        assert_eq!(saved, map);
        map.reset();
        assert!(!map.is_changed());
        assert_eq!(map.to_prefs(), BTreeMap::new());
    }
}
