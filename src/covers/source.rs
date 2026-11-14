//! Where an upload comes from: a file picked in Plasma's file dialog, or
//! the clipboard (Ctrl+V in Pick a cover). Both block, so the app runs
//! them on tokio's blocking pool.
//!
//! The dialog is the desktop portal's (`org.freedesktop.portal.FileChooser`
//! over the session bus, which Plasma answers with its own dialog), with
//! `kdialog` when the portal does not answer. The clipboard is read with
//! `wl-paste`, since iced reads text alone: an image copied in a browser
//! or an image editor, else a copied file (Dolphin offers its URI) or a
//! path as text.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
/// The image types the clipboard is asked for, best first.
const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

/// The file dialog: the picked file, or none when it was closed.
pub fn browse() -> Result<Option<PathBuf>, String> {
    match portal() {
        Ok(picked) => Ok(picked),
        Err(error) => {
            eprintln!("libri: the file dialog's portal did not answer ({error}); trying kdialog");
            kdialog()
        }
    }
}

fn portal() -> Result<Option<PathBuf>, String> {
    let connection = Connection::session().map_err(|e| format!("no session bus: {e}"))?;
    let unique = connection
        .unique_name()
        .ok_or("the session bus gave no name")?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!(
        "libri{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    // The request's path is known ahead, so the answer is listened for
    // before the dialog is asked for and cannot be missed.
    let request_path = format!("{PORTAL_PATH}/request/{unique}/{token}");
    let request = Proxy::new(
        &connection,
        PORTAL,
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .map_err(|e| e.to_string())?;
    let mut answers = request
        .receive_signal("Response")
        .map_err(|e| e.to_string())?;

    let chooser = Proxy::new(
        &connection,
        PORTAL,
        PORTAL_PATH,
        "org.freedesktop.portal.FileChooser",
    )
    .map_err(|e| e.to_string())?;
    let patterns: Vec<(u32, &str)> = ["*.jpg", "*.jpeg", "*.png", "*.webp", "*.gif"]
        .into_iter()
        .map(|glob| (0u32, glob))
        .collect();
    let filters = vec![("Images", patterns)];
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("modal", Value::from(true));
    options.insert("filters", Value::from(filters));
    let _handle: OwnedObjectPath = chooser
        .call("OpenFile", &("", "Choose a cover", options))
        .map_err(|e| e.to_string())?;

    let answer = answers.next().ok_or("the dialog gave no answer")?;
    let (response, results): (u32, HashMap<String, OwnedValue>) =
        answer.body().deserialize().map_err(|e| e.to_string())?;
    if response != 0 {
        // 1 is Cancel; 2 the dialog ended some other way.
        return Ok(None);
    }
    let uris: Vec<String> = results
        .get("uris")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .unwrap_or_default();
    Ok(uris.first().and_then(|uri| path_of_uri(uri)))
}

fn kdialog() -> Result<Option<PathBuf>, String> {
    let output = Command::new("kdialog")
        .args([
            "--title",
            "Choose a cover",
            "--getopenfilename",
            ".",
            "Images (*.jpg *.jpeg *.png *.webp *.gif)",
        ])
        .output()
        .map_err(|e| format!("no file dialog: {e}"))?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((output.status.success() && !path.is_empty()).then(|| PathBuf::from(path)))
}

/// What Ctrl+V gives: the bytes of an image on the clipboard, or of the
/// file a copied path or URI names. None when the clipboard holds
/// neither.
pub fn clipboard() -> Result<Option<Vec<u8>>, String> {
    let types = wl_paste(&["--list-types"])
        .map_err(|e| format!("The clipboard could not be read ({e})."))?;
    let types = String::from_utf8_lossy(&types).into_owned();
    let offered: Vec<&str> = types.lines().map(str::trim).collect();
    for wanted in IMAGE_TYPES {
        if offered.contains(&wanted) {
            let bytes = wl_paste(&["--type", wanted])
                .map_err(|e| format!("The clipboard could not be read ({e})."))?;
            if !bytes.is_empty() {
                return Ok(Some(bytes));
            }
        }
    }
    let text = if offered.contains(&"text/uri-list") {
        wl_paste(&["--type", "text/uri-list"]).ok()
    } else if offered.iter().any(|t| t.starts_with("text/")) {
        wl_paste(&["--no-newline"]).ok()
    } else {
        None
    };
    let Some(text) = text else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&text);
    let Some(path) = text.lines().map(str::trim).find_map(path_of_text) else {
        return Ok(None);
    };
    std::fs::read(&path)
        .map(Some)
        .map_err(|e| format!("{} could not be read ({e}).", path.display()))
}

fn wl_paste(args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("wl-paste")
        .args(args)
        .output()
        .map_err(|e| format!("wl-paste: {e}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        // An empty clipboard is no error.
        Ok(Vec::new())
    }
}

/// A line of clipboard text as a file: a `file://` URI, or a path to a
/// file that is there.
fn path_of_text(line: &str) -> Option<PathBuf> {
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    if let Some(path) = path_of_uri(line) {
        return Some(path);
    }
    let path = PathBuf::from(line);
    (path.is_absolute() && path.is_file()).then_some(path)
}

/// `file:///home/me/a%20b.png` as a path.
fn path_of_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // A host before the path ("file://localhost/…") is dropped.
    let path = &rest[rest.find('/')?..];
    let mut bytes = Vec::with_capacity(path.len());
    let mut raw = path.bytes();
    while let Some(byte) = raw.next() {
        if byte == b'%' {
            let hex = [raw.next()?, raw.next()?];
            let hex = std::str::from_utf8(&hex).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    Some(PathBuf::from(String::from_utf8(bytes).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_uri_becomes_its_path() {
        assert_eq!(
            path_of_uri("file:///home/me/My%20Covers/sh%C5%8Dgun.png"),
            Some(PathBuf::from("/home/me/My Covers/shōgun.png"))
        );
        assert_eq!(
            path_of_uri("file://localhost/tmp/a.jpg"),
            Some(PathBuf::from("/tmp/a.jpg"))
        );
        assert_eq!(path_of_uri("https://example.org/a.jpg"), None);
        assert_eq!(path_of_text("# a comment"), None);
        assert_eq!(path_of_text("just some words"), None);
    }
}
