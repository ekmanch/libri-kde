//! Where Libri keeps its files on this computer, and how it writes them.
//!
//! The library lives under the XDG data home (`~/.local/share/libri/`)
//! and the display preferences under the config home (`~/.config/libri/`),
//! where the desktop's conventions put them; the freeze log is already
//! under the cache home (`app::freeze_log`). Every file is written whole
//! to a sibling `.tmp` first and then renamed into place, so a crash or a
//! full disk never leaves a half-written file (docs/DESIGN.md section 10,
//! phase 2).

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// `$XDG_DATA_HOME/libri`, or `~/.local/share/libri`. `None` without a
/// home folder.
pub fn data_dir() -> Option<PathBuf> {
    xdg_dir(
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
        &[".local", "share"],
    )
}

/// `$XDG_CACHE_HOME/libri`, or `~/.cache/libri`: what can be fetched
/// again (covers, track lists, episodes, series, the IGDB token) and the
/// freeze log.
pub fn cache_dir() -> Option<PathBuf> {
    xdg_dir(
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
        &[".cache"],
    )
}

/// `$XDG_CONFIG_HOME/libri`, or `~/.config/libri`.
pub fn config_dir() -> Option<PathBuf> {
    xdg_dir(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        &[".config"],
    )
}

/// The `libri` folder under the XDG base `var` names, or under `home`
/// plus `under_home` when the variable is unset or empty, as the base
/// directory specification says.
fn xdg_dir(var: Option<OsString>, home: Option<OsString>, under_home: &[&str]) -> Option<PathBuf> {
    let base = match var.filter(|value| !value.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => {
            let mut dir = PathBuf::from(home?);
            dir.extend(under_home);
            dir
        }
    };
    Some(base.join("libri"))
}

/// Writes `text` to `path` so that a crash or a full disk never leaves a
/// half-written file: the text goes to a sibling `.tmp` file first, is
/// flushed to disk, and then replaces `path` in one rename. The parent
/// folders are created as needed.
pub fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::other("the path has no parent folder"))?;
    fs::create_dir_all(parent)?;
    let temp = temp_path(path);
    let written = (|| {
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// `library.json.tmp` next to `library.json`.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The text of `path`, or `None` when there is no such file. Any other
/// trouble (no permission, not valid UTF-8) is an error.
pub fn read_if_exists(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Removes `path` if it is there; a file already gone is no error.
pub fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// A local timestamp for a file name: `20261027-213005`, from the same
/// clock as the freeze log.
pub fn file_stamp() -> String {
    crate::trace::file_stamp()
}

/// `library.json.20261027-213005.unreadable` next to `library.json`: where
/// a file that does not parse is moved, never overwritten.
pub fn aside_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(format!(".{}.unreadable", file_stamp()));
    path.with_file_name(name)
}

/// A folder of its own under the system's temp dir, removed when dropped,
/// for the tests that write files.
#[cfg(test)]
pub struct TempDir(pub PathBuf);

#[cfg(test)]
impl TempDir {
    pub fn new(name: &str) -> TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("libri-test-{}-{name}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

#[cfg(test)]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folders_follow_the_xdg_variables_and_fall_back_to_home() {
        let home = || Some(OsString::from("/home/c"));
        assert_eq!(
            xdg_dir(Some("/data".into()), home(), &[".local", "share"]),
            Some(PathBuf::from("/data/libri"))
        );
        assert_eq!(
            xdg_dir(None, home(), &[".local", "share"]),
            Some(PathBuf::from("/home/c/.local/share/libri"))
        );
        assert_eq!(
            xdg_dir(Some("".into()), home(), &[".config"]),
            Some(PathBuf::from("/home/c/.config/libri")),
            "an empty variable counts as unset"
        );
        assert_eq!(xdg_dir(None, None, &[".config"]), None);
    }

    #[test]
    fn an_atomic_write_creates_the_folders_replaces_the_file_and_leaves_no_temp() {
        let dir = TempDir::new("storage");
        let path = dir.path("deep/er/file.json");
        assert_eq!(read_if_exists(&path).unwrap(), None);
        write_atomically(&path, "one\n").unwrap();
        write_atomically(&path, "two\n").unwrap();
        assert_eq!(read_if_exists(&path).unwrap().as_deref(), Some("two\n"));
        assert!(!temp_path(&path).exists());
        remove_if_exists(&path).unwrap();
        remove_if_exists(&path).unwrap();
        assert_eq!(read_if_exists(&path).unwrap(), None);
    }
}
