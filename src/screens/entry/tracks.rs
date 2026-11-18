//! The Tracks dialog: an album's own track list, made by hand. Only an
//! album without an online match has one; a matched album's list is
//! fetched and never stored. Numbered rows (a title, an optional length,
//! ✕, the grip to drag a row to another place), "+ Add track", and Paste
//! a list, where every line becomes a track. Nothing reaches the library
//! until Save; a row keeps its rating through a rename and a move.

use iced::widget::{
    Column, column, container, mouse_area, operation, row, rule, space, text, text_editor,
};
use iced::{Background, Border, Center, Element, Fill, Task, mouse};

use super::{Dialog, Message as EntryMessage};
use crate::app::{App, Message as AppMessage};
use crate::model::{Entry, Track};
use crate::repo::Operation;
use crate::theme::font;
use crate::ui::{self, ButtonKind, Icon};

/// A row's field, and the gap to the next row.
const ROW_H: f32 = 34.0;
const ROW_GAP: f32 = 6.0;
/// The rows scroll past this height.
const ROWS_MAX_H: f32 = 360.0;
/// The longest length kept: "1:02:03".
const LENGTH_MAX: usize = 8;

#[derive(Debug, Clone)]
pub enum Message {
    Title(usize, String),
    Length(usize, String),
    Remove(usize),
    Add,
    /// Enter in a title: on to the next row, a new one after the last.
    Next(usize),
    Paste(text_editor::Action),
    AddPasted,
    /// The grip of a row is pressed; the pointer's height over the rows
    /// then moves it, until the button is let go.
    Grab(usize),
    DragTo(f32),
    Drop,
    Save,
}

/// One row: what is typed, and what the track carries unseen.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    /// Names the row's fields, so focus follows a row that moves.
    key: u64,
    pub title: String,
    pub length: String,
    id: Option<String>,
    rating: Option<u8>,
}

#[derive(Debug)]
pub struct Form {
    /// The album.
    pub id: String,
    pub rows: Vec<Row>,
    pub paste: text_editor::Content,
    /// The row being dragged, where it is now.
    pub dragging: Option<usize>,
    next_key: u64,
}

impl Form {
    /// The dialog on the album's stored tracks.
    pub fn open(album: &Entry) -> Form {
        let mut form = Form {
            id: album.id.clone(),
            rows: Vec::new(),
            paste: text_editor::Content::new(),
            dragging: None,
            next_key: 0,
        };
        for track in album.tracks.iter().flatten() {
            let key = form.key();
            form.rows.push(Row {
                key,
                title: track.title.clone(),
                length: track.length.clone().unwrap_or_default(),
                id: track.id.clone(),
                rating: track.rating,
            });
        }
        form
    }

    fn key(&mut self) -> u64 {
        self.next_key += 1;
        self.next_key
    }

    fn push(&mut self, title: String, length: String) {
        let key = self.key();
        self.rows.push(Row {
            key,
            title,
            length,
            ..Row::default()
        });
    }

    /// The pasted lines that hold a track.
    fn pasted(&self) -> Vec<(String, String)> {
        parse_list(&self.paste.text())
    }

    /// What Save stores: the rows with a title, in order.
    pub fn tracks(&self) -> Vec<Track> {
        self.rows
            .iter()
            .filter(|row| !row.title.trim().is_empty())
            .zip(1..)
            .map(|(row, n)| Track {
                n: Some(n),
                title: row.title.trim().to_owned(),
                length: Some(row.length.trim().to_owned()).filter(|l| !l.is_empty()),
                id: row.id.clone(),
                rating: row.rating,
            })
            .collect()
    }
}

/// The tracks of a pasted list, a title and a length ("" when none) per
/// line that holds either. A number in front goes ("1.", "01 -", "3)"; a
/// bare "1 " only when every line starts with one, so "99 Problems" alone
/// keeps its name), and a length at the end is picked up ("4:31",
/// "(4:31)").
pub fn parse_list(pasted: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = pasted
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let all_numbered =
        lines.len() > 1 && lines.iter().all(|line| strip_number(line, true).is_some());
    lines
        .iter()
        .map(|line| {
            let rest = strip_number(line, all_numbered).unwrap_or(line);
            let (title, length) = split_length(rest);
            (title.to_owned(), length)
        })
        .filter(|(title, length)| !title.is_empty() || !length.is_empty())
        .collect()
}

/// `line` without its leading track number, when it has one: up to three
/// digits, then a mark (". ) - – — :" with spaces around it), or with
/// `bare` only a space. A line that is nothing but the number keeps it.
fn strip_number(line: &str, bare: bool) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let after = &line[digits..];
    let spaced = after.trim_start();
    let marked = spaced
        .strip_prefix(['.', ')', '-', '–', '—'])
        // "4:31" in front is a length, not a number.
        .or_else(|| {
            spaced
                .strip_prefix(':')
                .filter(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()))
        })
        .map(|rest| {
            rest.trim_start()
                .trim_start_matches(['-', '–', '—'])
                .trim_start()
        });
    let rest = match marked {
        Some(rest) => rest,
        None if bare && spaced.len() < after.len() => spaced,
        None => return None,
    };
    (!rest.is_empty()).then_some(rest)
}

/// The title and the length at the end of `line`: "4:31", "1:02:03", in
/// brackets or not, after a dash or not.
fn split_length(line: &str) -> (&str, String) {
    let trimmed = line.trim_end();
    let inner = trimmed
        .strip_suffix([')', ']'])
        .map_or(trimmed, str::trim_end);
    let bracketed = inner.len() < trimmed.len();
    let start = inner
        .rfind(|c: char| !(c.is_ascii_digit() || c == ':'))
        .map_or(0, |at| {
            at + inner[at..].chars().next().map_or(1, char::len_utf8)
        });
    let length = &inner[start..];
    let before = &inner[..start];
    let opened = before.ends_with(['(', '[']);
    let alone = before.is_empty() || before.ends_with(char::is_whitespace) || opened;
    if !is_length(length) || bracketed != opened || !alone {
        return (trimmed, String::new());
    }
    let title = before
        .trim_end_matches(['(', '['])
        .trim_end()
        .trim_end_matches(['-', '–', '—'])
        .trim_end();
    (title, length.to_owned())
}

/// "4:31", "12:05", "1:02:03".
fn is_length(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    (2..=3).contains(&parts.len())
        && (1..=3).contains(&parts[0].len())
        && parts[1..].iter().all(|part| part.len() == 2)
        && parts
            .iter()
            .all(|part| part.chars().all(|c| c.is_ascii_digit()))
}

fn title_id(key: u64) -> String {
    format!("track-title-{key}")
}

/// Opens the dialog on the album.
pub fn open(app: &mut App, id: &str) {
    if let Some(album) = app.repo.library().entry(id) {
        app.library.track_rating_open = None;
        app.dialog = Some(Dialog::Tracks(Box::new(Form::open(album))));
    }
}

/// Whether a row of the open dialog is being dragged.
pub fn dragging(app: &App) -> bool {
    matches!(&app.dialog, Some(Dialog::Tracks(form)) if form.dragging.is_some())
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    let Some(Dialog::Tracks(form)) = &mut app.dialog else {
        return Task::none();
    };
    match message {
        Message::Title(index, title) => {
            if let Some(row) = form.rows.get_mut(index) {
                row.title = title;
            }
        }
        Message::Length(index, length) => {
            if let Some(row) = form.rows.get_mut(index) {
                row.length = length
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == ':')
                    .take(LENGTH_MAX)
                    .collect();
            }
        }
        Message::Remove(index) => {
            if index < form.rows.len() {
                form.rows.remove(index);
            }
        }
        Message::Add => {
            form.push(String::new(), String::new());
            return focus_row(form, form.rows.len() - 1);
        }
        Message::Next(index) => {
            if index + 1 >= form.rows.len() {
                form.push(String::new(), String::new());
            }
            return focus_row(form, index + 1);
        }
        Message::Paste(action) => form.paste.perform(action),
        Message::AddPasted => {
            let pasted = form.pasted();
            if !pasted.is_empty() {
                // Rows left empty make way for the list.
                form.rows
                    .retain(|row| !row.title.trim().is_empty() || !row.length.is_empty());
                for (title, length) in pasted {
                    form.push(title, length);
                }
                form.paste = text_editor::Content::new();
            }
        }
        Message::Grab(index) => form.dragging = (index < form.rows.len()).then_some(index),
        Message::DragTo(y) => {
            if let Some(from) = form.dragging {
                let last = form.rows.len().saturating_sub(1);
                let to = ((y / (ROW_H + ROW_GAP)).floor().max(0.0) as usize).min(last);
                if to != from && from < form.rows.len() {
                    let row = form.rows.remove(from);
                    form.rows.insert(to, row);
                    form.dragging = Some(to);
                }
            }
        }
        Message::Drop => form.dragging = None,
        Message::Save => {
            let tracks = form.tracks();
            let album = form.id.clone();
            app.dialog = None;
            let stored = app
                .repo
                .library()
                .entry(&album)
                .map(|entry| entry.tracks.clone().unwrap_or_default());
            if stored.is_some_and(|stored| stored != tracks) {
                app.apply(Operation::SetTracks { album, tracks });
            }
        }
    }
    Task::none()
}

fn focus_row(form: &Form, index: usize) -> Task<AppMessage> {
    match form.rows.get(index) {
        Some(row) => operation::focus(title_id(row.key)),
        None => Task::none(),
    }
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(EntryMessage::Tracks(message))
}

pub fn view<'a>(app: &'a App, form: &'a Form) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let album = app.repo.library().entry(&form.id);
    let name = album.map_or(String::new(), |album| match album.creator.as_deref() {
        Some(artist) => format!("{} · {artist}. ", album.title),
        None => format!("{}. ", album.title),
    });
    let lead = text(format!(
        "{name}This album has no online match, so the list is yours to keep. Saved with the album in library.json."
    ))
    .size(13.5)
    .line_height(1.45)
    .color(tokens.sub);

    let mut rows = Column::new().spacing(ROW_GAP);
    for (index, track) in form.rows.iter().enumerate() {
        let held = form.dragging == Some(index);
        let grip = mouse_area(
            container(ui::icon(Icon::Grip, 14.0, tokens.sub))
                .center_y(ROW_H)
                .center_x(18),
        )
        .on_press(msg(Message::Grab(index)))
        .interaction(if held {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Grab
        });
        let title = ui::text_field(
            "Title",
            &track.title,
            move |title| msg(Message::Title(index, title)),
            font::REGULAR,
            &tokens,
        )
        .id(title_id(track.key))
        .on_submit(msg(Message::Next(index)))
        .size(14)
        .padding([7, 10]);
        let length = ui::text_field(
            "0:00",
            &track.length,
            move |length| msg(Message::Length(index, length)),
            font::MONO,
            &tokens,
        )
        .on_submit(msg(Message::Next(index)))
        .align_x(iced::alignment::Horizontal::Right)
        .size(13)
        .padding([7, 10])
        .width(70);
        let line = row![
            grip,
            container(ui::mono((index + 1).to_string(), 12.0, tokens.sub))
                .width(24)
                .align_x(iced::alignment::Horizontal::Right),
            title,
            length,
            ui::icon_button(Icon::Close, 14.0, &tokens, msg(Message::Remove(index))),
        ]
        .spacing(8)
        .align_y(Center)
        .height(ROW_H);
        // The row in hand stands out from the ones it passes.
        rows = rows.push(container(line).style(move |_| container::Style {
            background: held.then_some(Background::Color(tokens.hover)),
            border: Border::default().rounded(8),
            ..container::Style::default()
        }));
    }
    let list: Element<'a, AppMessage> = if form.rows.is_empty() {
        text("No tracks yet. Add them one by one, or paste a whole list below.")
            .size(13)
            .color(tokens.sub)
            .into()
    } else {
        let mut area = mouse_area(rows);
        if form.dragging.is_some() {
            area = area.on_move(|point| msg(Message::DragTo(point.y)));
        }
        container(ui::scroll(
            container(area).padding(iced::Padding::ZERO.right(10)),
        ))
        .max_height(ROWS_MAX_H)
        .into()
    };

    let pasted = form.pasted().len();
    let paste_label = match pasted {
        0 => "Add from paste".to_owned(),
        1 => "Add 1 track".to_owned(),
        n => format!("Add {n} tracks"),
    };
    let editor = text_editor(&form.paste)
        .placeholder(
            "One track per line, e.g. from Wikipedia or Discogs:\n1. Hometown Glory 4:31\n2. I'll Be Waiting 4:07",
        )
        .on_action(|action| msg(Message::Paste(action)))
        .font(font::MONO)
        .height(84)
        .padding([8, 10])
        .size(13)
        .style(move |_, status| text_editor::Style {
            background: Background::Color(tokens.field),
            border: Border::default().rounded(8).width(1).color(match status {
                text_editor::Status::Focused { .. } => tokens.accent,
                _ => tokens.line,
            }),
            placeholder: tokens.sub,
            value: tokens.text,
            selection: tokens.selection,
        });
    let paste = column![
        text("Paste a list")
            .size(12.5)
            .font(font::SEMIBOLD)
            .color(tokens.sub),
        editor,
        row![
            text(
                "Numbers in front and lengths at the end are picked up. Pasted tracks are added after the ones above."
            )
            .size(12)
            .color(tokens.sub)
            .width(Fill),
            ui::sized_button(
                paste_label,
                ButtonKind::Outline,
                32.0,
                tokens,
                (pasted > 0).then(|| msg(Message::AddPasted)),
            ),
        ]
        .spacing(10)
        .align_y(Center),
    ]
    .spacing(6);

    let add = row![ui::header_link(
        "+ Add track",
        false,
        &tokens,
        msg(Message::Add)
    )];
    let line = tokens.line;
    let divider = rule::horizontal(1).style(move |_| rule::Style {
        color: line,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    });
    let body = column![lead, list, add, divider, paste].spacing(14);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            tokens,
            Some(AppMessage::Entry(EntryMessage::Close)),
        ),
        ui::labeled_button(
            "Save",
            ButtonKind::Primary,
            tokens,
            Some(msg(Message::Save))
        ),
    ]
    .spacing(10);
    ui::dialog(
        "Tracks",
        body,
        footer,
        640,
        &tokens,
        AppMessage::Entry(EntryMessage::Close),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Vec<(String, String)> {
        parse_list(text)
    }

    fn pair(title: &str, length: &str) -> (String, String) {
        (title.to_owned(), length.to_owned())
    }

    #[test]
    fn a_pasted_list_loses_its_numbers_and_keeps_its_lengths() {
        assert_eq!(
            parsed(
                "1. Hometown Glory 4:31\n01 - I'll Be Waiting (4:07)\n3) Don't You Remember\n\n"
            ),
            [
                pair("Hometown Glory", "4:31"),
                pair("I'll Be Waiting", "4:07"),
                pair("Don't You Remember", ""),
            ]
        );
        assert_eq!(
            parsed("4 – Turning Tables – 4:51\n5\tSet Fire to the Rain [1:04:20]"),
            [
                pair("Turning Tables", "4:51"),
                pair("Set Fire to the Rain", "1:04:20"),
            ]
        );
    }

    #[test]
    fn a_title_that_starts_with_a_number_keeps_it() {
        // One line, or a list where not every line has a number.
        assert_eq!(parsed("99 Problems"), [pair("99 Problems", "")]);
        assert_eq!(
            parsed("99 Problems\nEncore 4:11"),
            [pair("99 Problems", ""), pair("Encore", "4:11")]
        );
        // Every line numbered with a bare number: the numbers go.
        assert_eq!(
            parsed("1 Angel 6:18\n2 Risingson\n3 1979"),
            [
                pair("Angel", "6:18"),
                pair("Risingson", ""),
                pair("1979", "")
            ]
        );
        // A year, a number alone and a time in a title are not lengths
        // or numbers.
        assert_eq!(parsed("1979"), [pair("1979", "")]);
        assert_eq!(parsed("2. 7"), [pair("7", "")]);
        assert_eq!(parsed("Live at 9:30 Club"), [pair("Live at 9:30 Club", "")]);
        assert_eq!(parsed("Track(4:31"), [pair("Track(4:31", "")]);
    }

    fn form(titles: &[&str]) -> Form {
        let mut form = Form {
            id: "music/album".into(),
            rows: Vec::new(),
            paste: text_editor::Content::new(),
            dragging: None,
            next_key: 0,
        };
        for title in titles {
            form.push((*title).to_owned(), String::new());
        }
        form
    }

    #[test]
    fn saving_numbers_the_titled_rows_and_keeps_their_ratings() {
        let mut form = form(&["  One ", "", "Three"]);
        form.rows[2].rating = Some(9);
        form.rows[2].length = "3:05".into();
        let tracks = form.tracks();
        assert_eq!(tracks.len(), 2);
        assert_eq!((tracks[0].n, tracks[0].title.as_str()), (Some(1), "One"));
        assert_eq!(tracks[0].length, None);
        assert_eq!(
            (tracks[1].n, tracks[1].length.as_deref(), tracks[1].rating),
            (Some(2), Some("3:05"), Some(9))
        );
    }
}
