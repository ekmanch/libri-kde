//! `settings.json` beside `library.json` in the library repo (the phone's
//! DESIGN.md section 8, version 1; its `app_settings.dart` and
//! `settings_codec.dart`): the settings that are about the library rather
//! than one device, so the phone and the desktop agree on them. The
//! timeline default, the seasonal theme default and the default platform,
//! the languages read, a movie's release dates, and what Discover
//! follows: the master switch, the unticked and ticked follows per kind
//! and the Not interested keys.
//!
//! The text is canonical like the library's: two-space indent, one
//! trailing newline, the keys in the phone's order, only the keys that
//! differ from the defaults, lists sorted. A new file is `{"version": 1}`.
//!
//! Sync: a change here is an [`Edit`], applied to this computer's copy at
//! once and queued beside the cover files (`covers::Change::Settings`).
//! When the commit is made the queued edits are merged into GitHub's copy
//! as it is then (`merged`, in `sync::commit`), so a change made on the
//! phone meanwhile is kept and there is no conflict to settle. GitHub's
//! copy is read with each sync check, and the edits still queued are
//! applied on top of it ([`Store::take_remote`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::ser::{SerializeMap, SerializeSeq};
use serde_json::ser::{PrettyFormatter, Serializer};
use serde_json::{Value, json};

use crate::follows::Kind;
use crate::storage;

pub const VERSION: u64 = 1;
/// The default platform until one is picked.
pub const DEFAULT_PLATFORM: &str = "PC";

/// The file as the app holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// What a new plan's Timeline switch starts as.
    pub timeline: bool,
    /// What a new plan's Seasonal theme switch starts as.
    pub seasonal: bool,
    /// Pre-selected on a new game, and what dates a game in Discover.
    pub platform: Option<String>,
    /// The languages read, as ISO 639-1 codes; `None` until a device has
    /// written the list.
    pub languages: Option<Vec<String>>,
    /// "cinema" or "digital"; `None` is both, the default.
    pub release_dates: Option<String>,
    /// The master switch of Discover.
    pub enabled: bool,
    unfollowed: BTreeMap<Kind, BTreeSet<String>>,
    followed: BTreeMap<Kind, BTreeSet<String>>,
    /// The keys of the Coming soon rows marked Not interested.
    pub not_interested: BTreeSet<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            timeline: false,
            seasonal: true,
            platform: Some(DEFAULT_PLATFORM.to_owned()),
            languages: None,
            release_dates: None,
            enabled: true,
            unfollowed: BTreeMap::new(),
            followed: BTreeMap::new(),
            not_interested: BTreeSet::new(),
        }
    }
}

impl Settings {
    /// The languages a found book must have an edition in: the file's
    /// list, or English until one is written.
    pub fn reading_languages(&self) -> Vec<String> {
        self.languages.clone().unwrap_or_else(|| vec!["en".into()])
    }

    fn listed(map: &BTreeMap<Kind, BTreeSet<String>>, kind: Kind, id: &str) -> bool {
        map.get(&kind).is_some_and(|ids| ids.contains(id))
    }

    /// Whether the follow is on: on by default unless unticked, off by
    /// default unless ticked.
    pub fn is_followed(&self, kind: Kind, id: &str, default_on: bool) -> bool {
        if default_on {
            !Settings::listed(&self.unfollowed, kind, id)
        } else {
            Settings::listed(&self.followed, kind, id)
        }
    }

    /// Ticked although it is off by default.
    pub fn is_ticked(&self, kind: Kind, id: &str) -> bool {
        Settings::listed(&self.followed, kind, id)
    }

    fn put(map: &mut BTreeMap<Kind, BTreeSet<String>>, kind: Kind, id: &str, present: bool) {
        if present {
            map.entry(kind).or_default().insert(id.to_owned());
        } else if let Some(ids) = map.get_mut(&kind) {
            ids.remove(id);
            if ids.is_empty() {
                map.remove(&kind);
            }
        }
    }

    /// Makes the change. A follow is kept as the exception to its default,
    /// or in neither list when it is the default.
    pub fn apply(&mut self, edit: &Edit) {
        match edit {
            Edit::Timeline(on) => self.timeline = *on,
            Edit::Seasonal(on) => self.seasonal = *on,
            Edit::Platform(platform) => self.platform = platform.clone(),
            Edit::Enabled(on) => self.enabled = *on,
            Edit::Follow {
                kind,
                ids,
                on,
                default_on,
            } => {
                let exception = on != default_on;
                for id in ids {
                    Settings::put(&mut self.unfollowed, *kind, id, exception && !on);
                    Settings::put(&mut self.followed, *kind, id, exception && *on);
                }
            }
            Edit::Hide(key) => {
                self.not_interested.insert(key.clone());
            }
            Edit::Restore(key) => {
                self.not_interested.remove(key);
            }
        }
    }
}

/// One change to the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Timeline(bool),
    Seasonal(bool),
    Platform(Option<String>),
    Enabled(bool),
    /// The follows `ids` of `kind` set `on`; `default_on` is what they are
    /// until changed (the kind's default, and the creator active).
    Follow {
        kind: Kind,
        ids: Vec<String>,
        on: bool,
        default_on: bool,
    },
    /// Not interested, by the row's key.
    Hide(String),
    Restore(String),
}

impl Edit {
    /// As `pending.json` keeps it, beside the cover file changes.
    pub fn to_json(&self) -> Value {
        match self {
            Edit::Timeline(on) => json!({"set": "timeline", "value": on}),
            Edit::Seasonal(on) => json!({"set": "seasonal", "value": on}),
            Edit::Platform(platform) => json!({"set": "platform", "value": platform}),
            Edit::Enabled(on) => json!({"set": "enabled", "value": on}),
            Edit::Follow {
                kind,
                ids,
                on,
                default_on,
            } => json!({
                "set": "follow",
                "kind": kind.key(),
                "ids": ids,
                "value": on,
                "default": default_on,
            }),
            Edit::Hide(key) => json!({"set": "hide", "key": key}),
            Edit::Restore(key) => json!({"set": "restore", "key": key}),
        }
    }

    pub fn from_json(json: &Value) -> Option<Edit> {
        let on = || json["value"].as_bool();
        let key = || json["key"].as_str().map(str::to_owned);
        Some(match json["set"].as_str()? {
            "timeline" => Edit::Timeline(on()?),
            "seasonal" => Edit::Seasonal(on()?),
            "platform" => Edit::Platform(json["value"].as_str().map(str::to_owned)),
            "enabled" => Edit::Enabled(on()?),
            "follow" => Edit::Follow {
                kind: Kind::from_key(json["kind"].as_str()?)?,
                ids: json["ids"]
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                on: on()?,
                default_on: json["default"].as_bool()?,
            },
            "hide" => Edit::Hide(key()?),
            "restore" => Edit::Restore(key()?),
            _ => return None,
        })
    }
}

/// JSON with its keys in the order given: `serde_json`'s own map sorts
/// them, and the file's order is the phone's.
enum Json {
    Null,
    Bool(bool),
    Number(u64),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl Serialize for Json {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Json::Null => serializer.serialize_unit(),
            Json::Bool(value) => serializer.serialize_bool(*value),
            Json::Number(value) => serializer.serialize_u64(*value),
            Json::Text(value) => serializer.serialize_str(value),
            Json::List(items) => {
                let mut list = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    list.serialize_element(item)?;
                }
                list.end()
            }
            Json::Object(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (key, value) in fields {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

fn texts<'a>(items: impl IntoIterator<Item = &'a String>) -> Json {
    Json::List(items.into_iter().cloned().map(Json::Text).collect())
}

fn ids(map: &BTreeMap<Kind, BTreeSet<String>>) -> Json {
    Json::Object(
        map.iter()
            .filter(|(_, ids)| !ids.is_empty())
            .map(|(kind, ids)| (kind.key(), texts(ids)))
            .collect(),
    )
}

/// The canonical text: what the phone's `encodeSettings` writes.
pub fn encode(settings: &Settings) -> String {
    let defaults = Settings::default();
    let mut root = vec![("version", Json::Number(VERSION))];
    if settings.timeline != defaults.timeline {
        root.push(("timeline_default", Json::Bool(settings.timeline)));
    }
    if settings.seasonal != defaults.seasonal {
        root.push(("seasonal_theme_default", Json::Bool(settings.seasonal)));
    }
    if settings.platform != defaults.platform {
        let platform = match &settings.platform {
            Some(platform) => Json::Text(platform.clone()),
            None => Json::Null,
        };
        root.push(("games", Json::Object(vec![("default_platform", platform)])));
    }
    if let Some(languages) = &settings.languages {
        let codes: BTreeSet<String> = languages.iter().map(|code| code.to_lowercase()).collect();
        root.push(("books", Json::Object(vec![("languages", texts(&codes))])));
    }
    if let Some(dates) = &settings.release_dates {
        let dates = Json::Text(dates.clone());
        root.push(("movies", Json::Object(vec![("release_dates", dates)])));
    }
    let mut upcoming = Vec::new();
    if settings.enabled != defaults.enabled {
        upcoming.push(("enabled", Json::Bool(settings.enabled)));
    }
    if settings.unfollowed.values().any(|ids| !ids.is_empty()) {
        upcoming.push(("unfollowed", ids(&settings.unfollowed)));
    }
    if settings.followed.values().any(|ids| !ids.is_empty()) {
        upcoming.push(("followed", ids(&settings.followed)));
    }
    if !settings.not_interested.is_empty() {
        upcoming.push(("not_interested", texts(&settings.not_interested)));
    }
    if !upcoming.is_empty() {
        root.push(("upcoming", Json::Object(upcoming)));
    }
    let mut bytes = Vec::new();
    let mut serializer =
        Serializer::with_formatter(&mut bytes, PrettyFormatter::with_indent(b"  "));
    Json::Object(root)
        .serialize(&mut serializer)
        .expect("settings always serialize");
    let mut text = String::from_utf8(bytes).expect("serde_json writes UTF-8");
    text.push('\n');
    text
}

fn ids_of(value: &Value) -> BTreeMap<Kind, BTreeSet<String>> {
    let mut map = BTreeMap::new();
    for (key, list) in value.as_object().into_iter().flatten() {
        let (Some(kind), Some(list)) = (Kind::from_key(key), list.as_array()) else {
            continue;
        };
        let ids: BTreeSet<String> = list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if !ids.is_empty() {
            map.insert(kind, ids);
        }
    }
    map
}

/// Reads the file's text. A key that is missing or of another shape is
/// its default, and a key this build does not know is dropped on the next
/// save, as on the phone. Text that is not a JSON object, or a file of
/// another version, is an error: the caller leaves that file alone.
pub fn decode(text: &str) -> Result<Settings, String> {
    let json: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if !json.is_object() {
        return Err("settings.json is not a JSON object".into());
    }
    match &json["version"] {
        Value::Null => {}
        version if version.as_u64() == Some(VERSION) => {}
        version => return Err(format!("settings.json version {version} is not supported")),
    }
    let defaults = Settings::default();
    let games = &json["games"];
    let upcoming = &json["upcoming"];
    Ok(Settings {
        timeline: json["timeline_default"]
            .as_bool()
            .unwrap_or(defaults.timeline),
        seasonal: json["seasonal_theme_default"]
            .as_bool()
            .unwrap_or(defaults.seasonal),
        platform: match games.get("default_platform") {
            None => defaults.platform,
            Some(platform) => platform.as_str().map(str::to_owned),
        },
        languages: json["books"]["languages"].as_array().map(|codes| {
            codes
                .iter()
                .filter_map(Value::as_str)
                .filter(|code| !code.is_empty())
                .map(str::to_lowercase)
                .collect()
        }),
        release_dates: json["movies"]["release_dates"]
            .as_str()
            .filter(|dates| matches!(*dates, "cinema" | "digital"))
            .map(str::to_owned),
        enabled: upcoming["enabled"].as_bool().unwrap_or(defaults.enabled),
        unfollowed: ids_of(&upcoming["unfollowed"]),
        followed: ids_of(&upcoming["followed"]),
        not_interested: upcoming["not_interested"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    })
}

/// `settings.json` in the folder of the library file: "settings.json" for
/// "library.json", "data/settings.json" for "data/library.json".
pub fn path_beside(library_path: &str) -> String {
    match library_path.rsplit_once('/') {
        Some((folder, _)) => format!("{folder}/settings.json"),
        None => "settings.json".to_owned(),
    }
}

/// The file as a commit writes it: `theirs` (GitHub's copy, or `None`
/// with no file there) with `edits` made, or `None` when they change
/// nothing. A copy that cannot be read is an error and is left as it is.
pub fn merged(theirs: Option<&str>, edits: &[Edit]) -> Result<Option<String>, String> {
    let before = match theirs {
        Some(text) => decode(text)?,
        None => Settings::default(),
    };
    let mut after = before.clone();
    for edit in edits {
        after.apply(edit);
    }
    Ok((after != before).then(|| encode(&after)))
}

/// What a commit says a change was, for one that makes a commit of its
/// own; `name` is who or what a single follow is.
pub fn summary(edit: &Edit, name: Option<&str>) -> String {
    match edit {
        Edit::Timeline(true) => "Start new plans as a timeline".into(),
        Edit::Timeline(false) => "Start new plans without a timeline".into(),
        Edit::Seasonal(true) => "Turn on seasonal touches".into(),
        Edit::Seasonal(false) => "Turn off seasonal touches".into(),
        Edit::Platform(Some(platform)) => format!("Set the default platform to {platform}"),
        Edit::Platform(None) => "Set no default platform".into(),
        Edit::Enabled(true) => "Turn on the check for new releases".into(),
        Edit::Enabled(false) => "Turn off the check for new releases".into(),
        Edit::Follow { kind, ids, on, .. } => {
            let verb = if *on { "Follow" } else { "Unfollow" };
            match (ids.as_slice(), name) {
                ([_], Some(name)) => format!("{verb} {name}"),
                ([id], None) => format!("{verb} {id}"),
                _ => format!("{verb} all {}", kind.label().to_lowercase()),
            }
        }
        Edit::Hide(key) => format!("Hide {} from Coming soon", name.unwrap_or(key.as_str())),
        Edit::Restore(key) => format!("Restore {} to Coming soon", name.unwrap_or(key.as_str())),
    }
}

/// The settings on the app: the copy on this computer and where it is
/// kept.
#[derive(Debug, Clone, Default)]
pub struct Store {
    pub settings: Settings,
    path: Option<PathBuf>,
    /// GitHub's ETag of the file, for the conditional read.
    pub etag: Option<String>,
    /// Whether the settings are more than a guess: read from this
    /// computer's file or from GitHub. Until then the daily check waits,
    /// since the defaults would follow everything the owner unticked.
    pub known: bool,
    /// Counts the changes made here and the commits landed; a download
    /// asked for before one of them is out of date and is dropped.
    pub epoch: u64,
    /// A download is on its way.
    pub fetching: bool,
}

impl Store {
    /// The file `settings.json` in `dir`, read when it is there.
    pub fn at(dir: &Path) -> Store {
        let path = dir.join("settings.json");
        let read = storage::read_if_exists(&path).ok().flatten();
        let settings = read.as_deref().and_then(|text| decode(text).ok());
        Store {
            known: settings.is_some(),
            settings: settings.unwrap_or_default(),
            path: Some(path),
            ..Store::default()
        }
    }

    /// Whether this computer has the file.
    pub fn on_disk(&self) -> bool {
        self.path.as_deref().is_some_and(Path::exists)
    }

    /// Makes `edit` here and writes the file; false when it changes
    /// nothing.
    pub fn apply(&mut self, edit: &Edit) -> bool {
        let before = self.settings.clone();
        self.settings.apply(edit);
        if self.settings == before {
            return false;
        }
        self.epoch += 1;
        self.save();
        true
    }

    /// GitHub's copy came (`None`: no file there), with the edits still
    /// queued for it: the settings are that copy with those on top. A copy
    /// that cannot be read leaves the settings as they are.
    pub fn take_remote(&mut self, text: Option<&str>, queued: &[Edit]) -> Result<bool, String> {
        let mut settings = match text {
            Some(text) => decode(text)?,
            None => Settings::default(),
        };
        for edit in queued {
            settings.apply(edit);
        }
        self.known = true;
        if settings == self.settings {
            return Ok(false);
        }
        self.settings = settings;
        self.save();
        Ok(true)
    }

    /// Writes the file as it is now.
    pub fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Err(error) = storage::write_atomically(path, &encode(&self.settings)) {
            eprintln!("libri: {} could not be written: {error}", path.display());
        }
    }

    /// The connection changed: nothing of the old repository's is kept
    /// for the conditional read.
    pub fn forget_remote(&mut self) {
        self.etag = None;
        self.fetching = false;
        self.epoch += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TempDir;

    /// The phone's sample (its DESIGN.md section 8), which its codec
    /// writes exactly so.
    const SAMPLE: &str = r#"{
  "version": 1,
  "timeline_default": true,
  "seasonal_theme_default": false,
  "games": {
    "default_platform": null
  },
  "books": {
    "languages": [
      "en",
      "sv"
    ]
  },
  "movies": {
    "release_dates": "digital"
  },
  "upcoming": {
    "enabled": false,
    "unfollowed": {
      "authors": [
        "j-k-rowling"
      ]
    },
    "followed": {
      "actors": [
        "jenna-ortega",
        "pedro-pascal"
      ]
    },
    "not_interested": [
      "movies/dune-part-three-2026",
      "shows/stranger-things-2016#5",
      "tmdb:movie:1234567"
    ]
  }
}
"#;

    fn follow(kind: Kind, id: &str, on: bool, default_on: bool) -> Edit {
        Edit::Follow {
            kind,
            ids: vec![id.into()],
            on,
            default_on,
        }
    }

    #[test]
    fn the_phones_sample_round_trips_to_the_same_bytes() {
        let settings = decode(SAMPLE).unwrap();
        assert!(settings.timeline && !settings.seasonal && !settings.enabled);
        assert_eq!(settings.platform, None);
        assert_eq!(settings.reading_languages(), ["en", "sv"]);
        assert!(!settings.is_followed(Kind::Authors, "j-k-rowling", true));
        assert!(settings.is_followed(Kind::Actors, "pedro-pascal", false));
        assert!(settings.not_interested.contains("tmdb:movie:1234567"));
        assert_eq!(encode(&settings), SAMPLE);
    }

    #[test]
    fn the_defaults_are_a_file_with_only_its_version() {
        assert_eq!(encode(&Settings::default()), "{\n  \"version\": 1\n}\n");
        assert_eq!(decode("{\"version\": 1}").unwrap(), Settings::default());
        // A file from before the version key is version 1.
        assert_eq!(decode("{}").unwrap(), Settings::default());
    }

    #[test]
    fn another_version_or_no_object_is_refused() {
        assert!(decode("{\"version\": 2}").is_err());
        assert!(decode("[]").is_err());
        assert!(decode("not json").is_err());
    }

    #[test]
    fn keys_are_written_in_the_phones_order_whatever_order_they_came_in() {
        let shuffled = r#"{"upcoming": {"not_interested": ["b", "a"], "enabled": false},
            "books": {"languages": ["sv", "EN", "en"]}, "unknown": 1, "version": 1,
            "seasonal_theme_default": false}"#;
        let text = encode(&decode(shuffled).unwrap());
        let order: Vec<usize> = [
            "\"version\"",
            "\"seasonal_theme_default\"",
            "\"books\"",
            "\"upcoming\"",
            "\"enabled\"",
            "\"not_interested\"",
        ]
        .iter()
        .map(|key| text.find(key).unwrap_or_else(|| panic!("no {key}")))
        .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
        assert!(!text.contains("unknown"), "an unknown key is dropped");
        assert!(text.contains("\"en\",\n      \"sv\""), "sorted, once each");
        assert!(text.contains("\"a\",\n      \"b\""), "{text}");
    }

    #[test]
    fn a_follow_is_kept_as_the_exception_to_its_default() {
        let mut settings = Settings::default();
        // Unticking what is on by default.
        settings.apply(&follow(Kind::Authors, "stephen-king", false, true));
        assert!(!settings.is_followed(Kind::Authors, "stephen-king", true));
        assert!(encode(&settings).contains("\"unfollowed\""));
        // Ticking it again is the default: it leaves the file.
        settings.apply(&follow(Kind::Authors, "stephen-king", true, true));
        assert_eq!(settings, Settings::default());
        // Ticking what is off by default.
        settings.apply(&follow(Kind::Actors, "pedro-pascal", true, false));
        assert!(settings.is_ticked(Kind::Actors, "pedro-pascal"));
        assert!(encode(&settings).contains("\"followed\""));
        settings.apply(&follow(Kind::Actors, "pedro-pascal", false, false));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn a_setting_back_at_its_default_leaves_the_file() {
        let mut settings = Settings::default();
        for edit in [
            Edit::Timeline(true),
            Edit::Seasonal(false),
            Edit::Platform(None),
            Edit::Enabled(false),
            Edit::Hide("tmdb:movie:1".into()),
        ] {
            settings.apply(&edit);
        }
        assert!(encode(&settings).contains("\"default_platform\": null"));
        for edit in [
            Edit::Timeline(false),
            Edit::Seasonal(true),
            Edit::Platform(Some("PC".into())),
            Edit::Enabled(true),
            Edit::Restore("tmdb:movie:1".into()),
        ] {
            settings.apply(&edit);
        }
        assert_eq!(encode(&settings), "{\n  \"version\": 1\n}\n");
    }

    #[test]
    fn every_edit_reads_back_from_the_queue() {
        let edits = [
            Edit::Timeline(true),
            Edit::Seasonal(false),
            Edit::Platform(None),
            Edit::Platform(Some("Steam Deck".into())),
            Edit::Enabled(false),
            follow(Kind::Series, "movies/halo", false, true),
            Edit::Hide("shows/severance-2022#3".into()),
            Edit::Restore("tmdb:movie:1".into()),
        ];
        for edit in edits {
            assert_eq!(Edit::from_json(&edit.to_json()), Some(edit));
        }
    }

    #[test]
    fn queued_edits_are_merged_into_githubs_copy() {
        // The phone unfollowed an author meanwhile; this computer hid a
        // title and unfollowed an artist.
        let theirs = "{\n  \"version\": 1,\n  \"upcoming\": {\n    \"unfollowed\": {\n      \
                      \"authors\": [\n        \"j-k-rowling\"\n      ]\n    }\n  }\n}\n";
        let edits = [
            Edit::Hide("tmdb:movie:1".into()),
            follow(Kind::Artists, "adele", false, true),
        ];
        let text = merged(Some(theirs), &edits).unwrap().expect("a change");
        let settings = decode(&text).unwrap();
        assert!(!settings.is_followed(Kind::Authors, "j-k-rowling", true));
        assert!(!settings.is_followed(Kind::Artists, "adele", true));
        assert!(settings.not_interested.contains("tmdb:movie:1"));
        // Made already (a lost answer): nothing to write.
        assert_eq!(merged(Some(&text), &edits), Ok(None));
        // No file and nothing but defaults: none is made.
        assert_eq!(merged(None, &[Edit::Timeline(false)]), Ok(None));
        assert!(merged(None, &edits).unwrap().is_some());
        // Another version is left alone.
        assert!(merged(Some("{\"version\": 2}"), &edits).is_err());
    }

    #[test]
    fn the_store_writes_each_change_and_takes_githubs_copy_under_the_queue() {
        let dir = TempDir::new("shared-store");
        let mut store = Store::at(&dir.0);
        assert!(!store.known && !store.on_disk());
        let hide = Edit::Hide("tmdb:movie:1".into());
        assert!(store.apply(&hide));
        assert!(!store.apply(&hide), "the same again changes nothing");
        assert_eq!(store.epoch, 1);
        let read = Store::at(&dir.0);
        assert!(read.known);
        assert!(read.settings.not_interested.contains("tmdb:movie:1"));
        // GitHub's copy has the phone's change; the hide is still queued.
        let theirs = encode(&{
            let mut settings = Settings::default();
            settings.apply(&Edit::Timeline(true));
            settings
        });
        assert_eq!(
            store.take_remote(Some(&theirs), std::slice::from_ref(&hide)),
            Ok(true)
        );
        assert!(store.settings.timeline);
        assert!(store.settings.not_interested.contains("tmdb:movie:1"));
        // Unreadable: kept as it is.
        assert!(store.take_remote(Some("{\"version\": 9}"), &[]).is_err());
        assert!(store.settings.timeline);
        // No file on GitHub is the defaults.
        assert_eq!(store.take_remote(None, &[]), Ok(true));
        assert_eq!(store.settings, Settings::default());
    }

    #[test]
    fn the_file_sits_beside_the_library() {
        assert_eq!(path_beside("library.json"), "settings.json");
        assert_eq!(path_beside("data/library.json"), "data/settings.json");
    }

    #[test]
    fn a_change_names_itself_for_its_commit() {
        let edit = follow(Kind::Authors, "stephen-king", false, true);
        assert_eq!(
            summary(&edit, Some("Stephen King")),
            "Unfollow Stephen King"
        );
        let all = Edit::Follow {
            kind: Kind::Actors,
            ids: vec!["a".into(), "b".into()],
            on: true,
            default_on: false,
        };
        assert_eq!(summary(&all, None), "Follow all actors");
        assert_eq!(
            summary(&Edit::Hide("k".into()), Some("Dune: Part Three")),
            "Hide Dune: Part Three from Coming soon"
        );
    }
}
