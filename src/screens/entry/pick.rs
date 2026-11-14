//! Pick a cover: one dialog for the cover viewer (Change cover…) and the
//! Add and Edit dialog (a click on the cover).
//!
//! Your covers come first: an Upload tile, then the files in the library
//! repo named after the title (`covers`), each marked "Yours". Online
//! lists the matches for the title as before, the match's own first.
//! Under them, folded, the other files in the type's folder: files whose
//! name matches no title. A hint at the foot shows the name a file needs
//! to show up here, with Copy name.
//!
//! Upload opens Plasma's file dialog, and Ctrl+V takes an image (or a
//! copied file) from the clipboard. The picture is scaled to 1000 px on
//! its longer side, saved as JPEG under `<slug>.<short sha>.jpg`, stored
//! and picked at once; the file is pushed in the background with the
//! change that picks it. There is no fit step by default: a picture
//! within a tenth of the type's shape is center-cropped where it shows,
//! like an online cover. One of another shape says "Not 2:3 · Adjust" on
//! its tile, and Adjust opens Fit your cover: Crop (the largest frame of
//! the shape, dragged along the picture's long side) or Whole image (all
//! of it on a blurred copy of itself).
//!
//! A song shows its album's cover, so a song's picker works on the album
//! while the album is in the library: its files, its matches, and a pick
//! changes the album.
//!
//! When Change cover… opens on a title that shows one of the owner's
//! files and the lookup has a cover for it by now, the dialog asks first
//! (An online cover is available): use it and delete the file, use it
//! and keep the file, or keep the file and never ask again for the
//! title; Later asks the next time.

use std::sync::Arc;

use iced::widget::image::Image;
use iced::widget::{Column, Row, Space, button, column, container, mouse_area, row, text};
use iced::{Border, Center, Color, ContentFit, Element, Fill, Point, Task};

use super::{Dialog, Form, Message as EntryMessage};
use crate::app::{App, Message as AppMessage};
use crate::artwork::{self, Wants};
use crate::covers::picture::{self, Fit, Prepared};
use crate::covers::{self, Change, Covers};
use crate::lookup::{Match, SearchResult};
use crate::model::{Entry, MediaType};
use crate::music;
use crate::repo::Operation;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(EntryMessage::Covers(message))
}

/// The panel, its five columns of tiles and the gaps.
pub const PANEL_WIDTH: f32 = 760.0;
const PANEL_PADDING: f32 = 22.0;
const COLUMNS: f32 = 5.0;
const TILE_GAP: f32 = 12.0;
/// Fit your cover and the online cover question.
const FIT_WIDTH: f32 = 720.0;
const AVAIL_WIDTH: f32 = 560.0;
/// The room Fit your cover's preview has.
const PREVIEW: (f32, f32) = (400.0, 300.0);

/// Pick a cover's own state, in the viewer and in the form.
#[derive(Debug, Default)]
pub struct PickState {
    /// "Other files in Movies" is unfolded.
    pub other_open: bool,
    /// Copy name was pressed: the link reads "Copied".
    pub copied: bool,
    /// A picked file or the clipboard is being read.
    pub busy: bool,
    /// Why the last upload did not work.
    pub error: Option<String>,
    /// The files uploaded while the dialog was open, "just uploaded".
    pub fresh: Vec<String>,
    /// The pictures' sizes as they were stored, by path, for the files of
    /// this session; the others are read from their files.
    pub shapes: Vec<(String, (u32, u32))>,
    pub fit: Option<FitState>,
    pub avail: Option<Avail>,
    /// The lookup was asked once whether it has a cover by now.
    pub checked: bool,
}

/// Fit your cover, open on one of the owner's files.
#[derive(Debug, Clone, PartialEq)]
pub struct FitState {
    pub path: String,
    pub mode: Fit,
    /// Where the crop frame is along the picture's long side, 0 to 1.
    pub at: f32,
    pub dragging: bool,
    /// The picture's width and height.
    pub size: (u32, u32),
    /// The size of the picture the fitted copy is made from: the one
    /// picked, when it is this session's, else the file's own.
    pub source: (u32, u32),
    /// The fitted copy is being made.
    pub working: bool,
    /// Where the pointer last was over the preview.
    pub cursor: Point,
}

/// "An online cover is available": the cover the lookup has now, and the
/// answer picked.
#[derive(Debug, Clone, PartialEq)]
pub struct Avail {
    pub url: String,
    pub choice: OnlineChoice,
}

/// What to do about an online cover for a title that shows the owner's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnlineChoice {
    /// Use the online cover and delete the file.
    #[default]
    UseAndDelete,
    /// Use the online cover; the file stays and is still offered.
    UseAndKeep,
    /// Keep the owner's cover and never ask again for the title.
    KeepMine,
}

impl OnlineChoice {
    pub const ALL: [OnlineChoice; 3] = [
        OnlineChoice::UseAndDelete,
        OnlineChoice::UseAndKeep,
        OnlineChoice::KeepMine,
    ];

    /// The answer as a row of the question.
    pub fn label(self) -> &'static str {
        match self {
            OnlineChoice::UseAndDelete => "Use online and delete mine",
            OnlineChoice::UseAndKeep => "Use online, keep my file",
            OnlineChoice::KeepMine => "Keep mine",
        }
    }

    /// The answer as a segment in the list of Settings.
    pub fn short(self) -> &'static str {
        match self {
            OnlineChoice::UseAndDelete => "Online, delete mine",
            OnlineChoice::UseAndKeep => "Online, keep file",
            OnlineChoice::KeepMine => "Keep mine",
        }
    }
}

/// A picture ready to store, with the picture it was made from.
#[derive(Debug, Clone)]
pub struct Upload {
    pub prepared: Arc<Prepared>,
    pub original: Arc<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The Upload tile: Plasma's file dialog.
    Upload,
    /// Ctrl+V: an image on the clipboard, or a copied file.
    Paste,
    /// The picked picture, scaled and encoded; none when the dialog was
    /// closed or the clipboard held no image.
    Prepared(Result<Option<Upload>, String>),
    ToggleOther,
    CopyName,
    /// "Not 2:3 · Adjust" on a tile: Fit your cover on that file.
    Adjust(String),
    FitMode(Fit),
    FitPress,
    FitMove(Point),
    FitRelease,
    FitBack,
    FitUse,
    /// The fitted copy of the file at `from`.
    Fitted {
        from: String,
        result: Result<Upload, String>,
    },
    /// Whether the lookup has a cover for the entry by now.
    Online {
        id: String,
        url: Option<String>,
    },
    AvailChoice(OnlineChoice),
    AvailLater,
    AvailApply,
}

/// What the picker works on: an entry of the library (the viewer's, an
/// edited one, or a song's album), or the entry the form is making.
#[derive(Debug, Clone, PartialEq)]
pub struct Subject {
    /// The entry's id, or the id a new entry would get now.
    pub id: String,
    pub media: MediaType,
    pub title: String,
    pub year: Option<u16>,
    /// The `cover_from` in effect: the library's, or the form's.
    pub current: Option<String>,
    pub keep_mine: bool,
    /// The entry is in the library and a pick is saved at once; else the
    /// pick waits in the form for Save.
    pub stored: bool,
    /// The song the picker was opened on, when this is its album.
    pub song: Option<String>,
}

impl Subject {
    fn of(entry: &Entry, song: Option<String>) -> Subject {
        Subject {
            id: entry.id.clone(),
            media: entry.media,
            title: entry.title.clone(),
            year: entry.year(),
            current: entry.cover_from.clone(),
            keep_mine: entry.cover_keep_mine == Some(true),
            stored: true,
            song,
        }
    }

    pub fn slug(&self) -> &str {
        covers::slug(&self.id)
    }

    fn square(&self) -> bool {
        ui::cover::is_square(self.media)
    }

    /// The path under `covers/` the subject shows, when it shows one of
    /// the owner's files.
    fn repo_path(&self) -> Option<&str> {
        self.current.as_deref().and_then(covers::repo_path)
    }
}

/// The entry whose cover `entry` shows and changes: its album for a song
/// whose album is in the library, else itself.
pub fn cover_owner<'a>(app: &'a App, entry: &'a Entry) -> &'a Entry {
    if entry.media == MediaType::Music && music::is_song(entry) {
        music::album_of_song(&app.repo.library().entries, entry).unwrap_or(entry)
    } else {
        entry
    }
}

/// What the open Pick a cover works on; none when none is open.
pub fn subject(app: &App) -> Option<Subject> {
    let library = app.repo.library();
    match app.dialog.as_ref()? {
        Dialog::Cover(view) if view.changing => {
            let entry = library.entry(&view.id)?;
            let owner = cover_owner(app, entry);
            let song = (owner.id != entry.id).then(|| entry.title.clone());
            Some(Subject::of(owner, song))
        }
        Dialog::Form(form) if form.cover_dialog => Some(form_subject(app, form)),
        _ => None,
    }
}

fn form_subject(app: &App, form: &Form) -> Subject {
    let library = app.repo.library();
    if let Some(album) = form.cover_album.as_deref().and_then(|id| library.entry(id)) {
        return Subject::of(album, Some(form.title.trim().to_owned()));
    }
    let year = form.released().map(|released| released.year);
    let id = match &form.editing {
        Some(id) => id.clone(),
        None => {
            let creator = Some(form.creator.trim()).filter(|c| !c.is_empty());
            crate::model::text::entry_id(
                form.media,
                form.title.trim(),
                year,
                creator,
                library.entries.iter().map(|e| e.id.as_str()),
            )
        }
    };
    Subject {
        id,
        media: form.media,
        title: form.title.trim().to_owned(),
        year,
        current: form.cover_from.clone(),
        keep_mine: form.cover_keep_mine == Some(true),
        stored: false,
        song: None,
    }
}

fn state(app: &App) -> Option<&PickState> {
    match app.dialog.as_ref()? {
        Dialog::Cover(view) => Some(&view.pick),
        Dialog::Form(form) => Some(&form.pick),
        _ => None,
    }
}

fn state_mut(app: &mut App) -> Option<&mut PickState> {
    match app.dialog.as_mut()? {
        Dialog::Cover(view) => Some(&mut view.pick),
        Dialog::Form(form) => Some(&mut form.pick),
        _ => None,
    }
}

/// Whether Ctrl+V belongs to Pick a cover right now.
pub fn takes_paste(app: &App) -> bool {
    subject(app).is_some() && state(app).is_some_and(|s| s.fit.is_none() && s.avail.is_none())
}

/// Esc inside the picker: Fit your cover or the online question closes
/// first. False when neither was open.
pub fn dismiss_inner(state: &mut PickState) -> bool {
    state.fit.take().is_some() || state.avail.take().is_some()
}

/// One online choice: the lookup id to store as `cover_from` (`None` for
/// the match's own cover), its words and its image.
pub struct Online {
    pub cover_from: Option<String>,
    pub title: String,
    pub sub: String,
    pub tint: Color,
    pub url: Option<String>,
}

/// The online choices for an entry of the library: the match's own cover
/// first (from the matches, or the cover the library shows while none is
/// picked), then the other matches for the title, one per image.
pub fn online_for_entry(app: &App, results: Option<&SearchResult>, entry: &Entry) -> Vec<Online> {
    let own_source = app.artwork.source_of(entry);
    let matches: Vec<&Match> = results.iter().flat_map(|r| r.matches.iter()).collect();
    let own_match = matches
        .iter()
        .copied()
        .find(|m| Some(m.source.as_str()) == own_source.as_deref());
    let own_url = match own_match {
        Some(found) => found.cache_cover_url().map(str::to_owned),
        None if entry.cover_from.is_none() => app.artwork.own_cover_url(entry).map(str::to_owned),
        None => None,
    };
    let year = entry.year().map(|y| y.to_string()).unwrap_or_default();
    let mut list = vec![Online {
        cover_from: None,
        title: own_match.map_or_else(|| entry.title.clone(), |m| m.title.clone()),
        sub: own_match.map_or(year, Match::year_and_edition),
        tint: theme::cover_color(&entry.id),
        url: own_url.clone(),
    }];
    for found in matches {
        if Some(found.source.as_str()) == own_source.as_deref() {
            continue;
        }
        let url = found.cache_cover_url().map(str::to_owned);
        if url.is_some() && url == own_url {
            continue;
        }
        list.push(Online {
            cover_from: Some(found.cover_from().to_owned()),
            title: found.title.clone(),
            sub: found.year_and_edition(),
            tint: theme::cover_color(&found.source),
            url,
        });
    }
    list
}

/// The online choices in the form: its match's own cover first, then the
/// other matches for the title.
fn online_for_form(form: &Form) -> Vec<Online> {
    let own = form.source.as_deref();
    form.cover_choices()
        .into_iter()
        .map(|found| {
            let is_own = Some(found.source.as_str()) == own;
            Online {
                cover_from: (!is_own).then(|| found.cover_from().to_owned()),
                sub: found.year_and_edition(),
                tint: theme::cover_color(&found.source),
                url: found
                    .cache_cover_url()
                    .or(found.cover_url.as_deref())
                    .map(str::to_owned),
                title: found.title,
            }
        })
        .collect()
}

/// The open picker's online choices, and whether they are still on their
/// way or some lookup did not answer.
fn online(app: &App, subject: &Subject) -> (Vec<Online>, bool, bool) {
    let (results, loading, form) = match app.dialog.as_ref() {
        Some(Dialog::Cover(view)) => (view.choices.as_ref(), view.loading, None),
        Some(Dialog::Form(form)) => (form.choices.as_ref(), form.choices_loading, Some(form)),
        _ => (None, false, None),
    };
    let failed = results.is_some_and(|c| c.partly_failed() || c.all_failed().is_some());
    let list = match (form, app.repo.library().entry(&subject.id)) {
        (Some(form), _) if !subject.stored => online_for_form(form),
        (_, Some(entry)) => online_for_entry(app, results, entry),
        _ => Vec::new(),
    };
    (list, loading, failed)
}

/// Whether the lookup is known to have a cover of its own for the
/// subject: the owner then picks their file over it knowingly, and is
/// not asked about an online cover later.
fn online_known(app: &App, subject: &Subject) -> bool {
    match subject.current.as_deref() {
        Some(current) if covers::repo_path(current).is_none() => true,
        _ => {
            subject.keep_mine
                || online(app, subject)
                    .0
                    .first()
                    .is_some_and(|own| own.cover_from.is_none() && own.url.is_some())
        }
    }
}

/// A file's size as the tile judges its shape: the session's record,
/// else read from the file.
fn shape_of(app: &App, state: &PickState, path: &str) -> Option<(u32, u32)> {
    state
        .shapes
        .iter()
        .find(|(known, _)| known == path)
        .map(|(_, size)| *size)
        .or_else(|| app.artwork.image_size(&covers::cover_from(path)))
}

// The update.

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Upload => {
            if !begin_reading(app) {
                return Task::none();
            }
            return Task::perform(
                crate::app::blocking(|| {
                    let Some(path) = covers::source::browse()? else {
                        return Ok(None);
                    };
                    let bytes = std::fs::read(&path)
                        .map_err(|e| format!("{} could not be read ({e}).", path.display()))?;
                    make(bytes).map(Some)
                }),
                |result| msg(Message::Prepared(result)),
            );
        }
        Message::Paste => {
            if !takes_paste(app) || !begin_reading(app) {
                return Task::none();
            }
            return Task::perform(
                crate::app::blocking(|| match covers::source::clipboard()? {
                    Some(bytes) => make(bytes).map(Some),
                    None => Err("The clipboard holds no image.".to_owned()),
                }),
                |result| msg(Message::Prepared(result)),
            );
        }
        Message::Prepared(result) => {
            if let Some(state) = state_mut(app) {
                state.busy = false;
            }
            match result {
                Ok(Some(upload)) => {
                    if let Some(path) = store(app, &upload) {
                        pick(app, Some(covers::cover_from(&path)));
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    if let Some(state) = state_mut(app) {
                        state.error = Some(error);
                    }
                }
            }
        }
        Message::ToggleOther => {
            if let Some(state) = state_mut(app) {
                state.other_open = !state.other_open;
            }
        }
        Message::CopyName => {
            let Some(subject) = subject(app) else {
                return Task::none();
            };
            if let Some(state) = state_mut(app) {
                state.copied = true;
            }
            return iced::clipboard::write(subject.slug().to_owned());
        }
        Message::Adjust(path) => {
            let size = state(app).and_then(|state| shape_of(app, state, &path));
            // The picture as it was picked is larger than its stored
            // copy; one turned by its EXIF data reads the other way
            // round in its header, and the copy's size is said then.
            let source = app
                .covers
                .original(&path)
                .and_then(|bytes| crate::lookup::store::image_size(&bytes))
                .zip(size)
                .filter(|((w, h), (sw, sh))| (w >= h) == (sw >= sh))
                .map(|(source, _)| source);
            if let (Some(size), Some(state)) = (size, state_mut(app)) {
                state.error = None;
                state.fit = Some(FitState {
                    path,
                    mode: Fit::Crop,
                    at: 0.5,
                    dragging: false,
                    size,
                    source: source.unwrap_or(size),
                    working: false,
                    cursor: Point::ORIGIN,
                });
            }
        }
        Message::FitMode(mode) => {
            if let Some(fit) = state_mut(app).and_then(|s| s.fit.as_mut()) {
                fit.mode = mode;
                fit.dragging = false;
            }
        }
        Message::FitPress | Message::FitMove(_) | Message::FitRelease => {
            let Some(square) = subject(app).map(|s| s.square()) else {
                return Task::none();
            };
            if let Some(fit) = state_mut(app).and_then(|s| s.fit.as_mut()) {
                match message {
                    Message::FitPress => fit.dragging = fit.mode == Fit::Crop,
                    Message::FitRelease => fit.dragging = false,
                    Message::FitMove(point) => fit.cursor = point,
                    _ => {}
                }
                if fit.dragging {
                    fit.at = frame_at(fit.size, square, fit.cursor);
                }
            }
        }
        Message::FitBack => {
            if let Some(state) = state_mut(app) {
                state.fit = None;
            }
        }
        Message::FitUse => return fit_use(app),
        Message::Fitted { from, result } => {
            if let Some(fit) = state_mut(app).and_then(|s| s.fit.as_mut()) {
                fit.working = false;
            }
            match result {
                Ok(upload) => fitted(app, &from, &upload),
                Err(error) => {
                    if let Some(state) = state_mut(app) {
                        state.error = Some(error);
                    }
                }
            }
        }
        Message::Online { id, url } => {
            let asked = subject(app).filter(|s| s.id == id && s.stored && !s.keep_mine);
            if let (Some(subject), Some(url)) = (asked, url)
                && subject.repo_path().is_some()
                && let Some(state) = state_mut(app)
            {
                state.avail = Some(Avail {
                    url,
                    choice: OnlineChoice::default(),
                });
            }
        }
        Message::AvailChoice(choice) => {
            if let Some(avail) = state_mut(app).and_then(|s| s.avail.as_mut()) {
                avail.choice = choice;
            }
        }
        Message::AvailLater => {
            if let Some(state) = state_mut(app) {
                state.avail = None;
            }
        }
        Message::AvailApply => {
            let choice = state_mut(app)
                .and_then(|s| s.avail.take())
                .map(|a| a.choice);
            if let (Some(choice), Some(subject)) = (choice, subject(app)) {
                answer_online(app, &subject.id, choice);
            }
        }
    }
    Task::none()
}

/// Marks the picker as reading a picture; false when it already is, or
/// none is open.
fn begin_reading(app: &mut App) -> bool {
    if subject(app).is_none() {
        return false;
    }
    match state_mut(app) {
        Some(state) if !state.busy => {
            state.busy = true;
            state.error = None;
            true
        }
        _ => false,
    }
}

/// The picture in `bytes` as an upload.
fn make(bytes: Vec<u8>) -> Result<Upload, String> {
    let prepared = picture::prepare(&bytes)?;
    Ok(Upload {
        prepared: Arc::new(prepared),
        original: Arc::new(bytes),
    })
}

/// Stores an upload for the open picker's subject under its name and
/// gives the path under `covers/`.
fn store(app: &mut App, upload: &Upload) -> Option<String> {
    let subject = subject(app)?;
    let name = covers::upload_name(subject.slug(), &upload.prepared.sha);
    let path = format!("{}/{name}", covers::folder(subject.media));
    app.add_cover_file(&path, &upload.prepared.bytes)?;
    app.covers.keep_original(&path, upload.original.clone());
    if let Some(state) = state_mut(app) {
        state.fresh.retain(|fresh| *fresh != path);
        state.fresh.push(path.clone());
        state.shapes.retain(|(known, _)| *known != path);
        state.shapes.push((
            path.clone(),
            (upload.prepared.width, upload.prepared.height),
        ));
    }
    Some(path)
}

/// A pick in the open picker: saved at once on an entry of the library,
/// kept in the form for Save otherwise. Picking one of the owner's files
/// over a cover the lookup has is remembered, so no online cover is
/// offered for the title later.
pub fn pick(app: &mut App, cover_from: Option<String>) {
    let Some(subject) = subject(app) else {
        // The form's own "Use the match's cover", with the picker shut.
        if let Some(Dialog::Form(form)) = &mut app.dialog {
            form.cover_from = cover_from;
            form.cover_keep_mine = None;
        }
        return;
    };
    let keep_mine =
        (covers::is_repo(cover_from.as_deref()) && online_known(app, &subject)).then_some(true);
    if subject.stored {
        set_cover(app, &subject.id, cover_from, keep_mine);
    } else if let Some(Dialog::Form(form)) = &mut app.dialog {
        form.cover_from = cover_from;
        form.cover_keep_mine = keep_mine;
    }
}

/// Saves `cover_from` on the entry `id`.
fn set_cover(app: &mut App, id: &str, cover_from: Option<String>, keep_mine: Option<bool>) {
    let Some(entry) = app.repo.library().entry(id) else {
        return;
    };
    if entry.cover_from == cover_from && entry.cover_keep_mine == keep_mine {
        return;
    }
    let entry = Entry {
        cover_from,
        cover_keep_mine: keep_mine,
        ..entry.clone()
    };
    app.apply(Operation::UpdateEntry(entry));
}

/// The answer to "An online cover is available" for the entry `id`, from
/// the viewer or from the list in Settings.
pub fn answer_online(app: &mut App, id: &str, choice: OnlineChoice) {
    let Some(entry) = app.repo.library().entry(id) else {
        return;
    };
    let Some(path) = entry
        .cover_from
        .as_deref()
        .and_then(covers::repo_path)
        .map(str::to_owned)
    else {
        return;
    };
    match choice {
        OnlineChoice::KeepMine => {
            let cover_from = entry.cover_from.clone();
            set_cover(app, id, cover_from, Some(true));
        }
        OnlineChoice::UseAndKeep => set_cover(app, id, None, None),
        OnlineChoice::UseAndDelete => {
            let shared = Covers::used_elsewhere(app.repo.library(), &path, id);
            set_cover(app, id, None, None);
            if !shared {
                app.change_cover_files(vec![Change::Delete { path }], None);
            }
        }
    }
}

/// Where the crop frame is (0 to 1) with the pointer at `cursor` in the
/// preview: the frame's middle follows the pointer along the long side.
fn frame_at(size: (u32, u32), square: bool, cursor: Point) -> f32 {
    let (pw, ph) = preview_size(size);
    let (_, _, fw, fh) = picture::crop_frame(size.0, size.1, square, 0.0);
    let scale = pw / size.0.max(1) as f32;
    let (fw, fh) = (fw as f32 * scale, fh as f32 * scale);
    let (room, at) = if pw - fw > ph - fh {
        (pw - fw, cursor.x - fw / 2.0)
    } else {
        (ph - fh, cursor.y - fh / 2.0)
    };
    if room <= 0.0 {
        0.5
    } else {
        (at / room).clamp(0.0, 1.0)
    }
}

/// The picture of `size` as large as the preview's room allows.
fn preview_size(size: (u32, u32)) -> (f32, f32) {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let scale = (PREVIEW.0 / w).min(PREVIEW.1 / h);
    ((w * scale).round(), (h * scale).round())
}

/// Use this cover: the fitted copy is made from the picture as it was
/// picked, or from the file when it is not this session's.
fn fit_use(app: &mut App) -> Task<AppMessage> {
    let Some(subject) = subject(app) else {
        return Task::none();
    };
    let square = subject.square();
    let Some(fit) = state(app).and_then(|s| s.fit.clone()) else {
        return Task::none();
    };
    if fit.working {
        return Task::none();
    }
    let original = app.covers.original(&fit.path);
    let local = app.covers.ready(&fit.path);
    if original.is_none() && local.is_none() {
        // In memory alone (the tests, a mock run): nothing to fit, the
        // tile just stops offering.
        if let Some(state) = state_mut(app) {
            state.shapes.retain(|(known, _)| *known != fit.path);
            state.fit = None;
        }
        return Task::none();
    }
    if let Some(state) = state_mut(app).and_then(|s| s.fit.as_mut()) {
        state.working = true;
    }
    let from = fit.path.clone();
    Task::perform(
        crate::app::blocking(move || {
            let bytes = match (original, local) {
                (Some(original), _) => original,
                (None, Some(local)) => Arc::new(
                    std::fs::read(&local)
                        .map_err(|e| format!("The picture could not be read ({e})."))?,
                ),
                (None, None) => return Err("The picture is not on this computer.".to_owned()),
            };
            let prepared = picture::fit(&bytes, fit.mode, fit.at, square)?;
            Ok(Upload {
                prepared: Arc::new(prepared),
                original: bytes,
            })
        }),
        move |result| {
            msg(Message::Fitted {
                from: from.clone(),
                result,
            })
        },
    )
}

/// The fitted copy takes the place of the file it was made from: stored
/// and picked, and the upload it replaces deleted. A file committed by
/// hand is never deleted for it.
fn fitted(app: &mut App, from: &str, upload: &Upload) {
    let Some(path) = store(app, upload) else {
        return;
    };
    if let Some(state) = state_mut(app) {
        state.fit = None;
    }
    pick(app, Some(covers::cover_from(&path)));
    if path == from {
        return;
    }
    let (label, _) = covers::label_and_extension(covers::name_of(from));
    let subject_id = subject(app).map(|s| s.id).unwrap_or_default();
    let shared = Covers::used_elsewhere(app.repo.library(), from, &subject_id);
    if covers::is_upload_label(label) && !shared {
        if let Some(state) = state_mut(app) {
            state.fresh.retain(|fresh| fresh != from);
        }
        app.change_cover_files(
            vec![Change::Delete {
                path: from.to_owned(),
            }],
            None,
        );
    }
}

/// When Pick a cover opens from the viewer on a title that shows one of
/// the owner's files: asks the lookup once whether it has a cover by now.
pub fn check_online(app: &mut App) -> Task<AppMessage> {
    let Some(subject) = subject(app) else {
        return Task::none();
    };
    if !subject.stored || subject.keep_mine || subject.repo_path().is_none() {
        return Task::none();
    }
    let Some(state) = state_mut(app) else {
        return Task::none();
    };
    if std::mem::replace(&mut state.checked, true) {
        return Task::none();
    }
    let Some(entry) = app.repo.library().entry(&subject.id) else {
        return Task::none();
    };
    let plain = Entry {
        cover_from: None,
        ..entry.clone()
    };
    let service = app.lookups.clone();
    let id = subject.id.clone();
    if service.is_instant() {
        let url = artwork::resolve_cover(&service, &plain)
            .ok()
            .and_then(|(url, _)| url);
        return update(app, Message::Online { id, url });
    }
    Task::perform(
        crate::app::blocking(move || Ok(artwork::resolve_cover(&service, &plain).ok())),
        move |result| {
            msg(Message::Online {
                id: id.clone(),
                url: result.ok().flatten().and_then(|(url, _)| url),
            })
        },
    )
}

// The views.

/// The open picker: Pick a cover, or over it Fit your cover or the
/// online cover question. `on_done` is what Done sends.
pub fn panel<'a>(app: &'a App, on_done: EntryMessage, tokens: &Tokens) -> Element<'a, AppMessage> {
    let (Some(subject), Some(state)) = (subject(app), state(app)) else {
        return Space::new().into();
    };
    if let Some(avail) = &state.avail {
        return avail_view(app, &subject, avail, tokens);
    }
    if let Some(fit) = &state.fit {
        return fit_view(app, &subject, state, fit, tokens);
    }
    picker(app, &subject, state, on_done, tokens)
}

/// A section's header: its name in small caps, and a mono note.
fn section_head<'a>(name: String, note: String, tokens: &Tokens) -> Element<'a, AppMessage> {
    row![
        text(name.to_uppercase())
            .size(12)
            .font(font::BOLD)
            .color(tokens.sub),
        ui::mono(note, 11.5, tokens.sub),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

fn grid<'a>(tiles: Vec<Element<'a, AppMessage>>) -> Element<'a, AppMessage> {
    Row::with_children(tiles)
        .spacing(TILE_GAP)
        .wrap()
        .vertical_spacing(TILE_GAP)
        .into()
}

fn picker<'a>(
    app: &'a App,
    subject: &Subject,
    state: &'a PickState,
    on_done: EntryMessage,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let square = subject.square();
    let tile = ((PANEL_WIDTH - 2.0 * PANEL_PADDING - (COLUMNS - 1.0) * TILE_GAP) / COLUMNS).floor();
    let year = subject.year.map(|y| format!(" · {y}")).unwrap_or_default();
    let lead = match &subject.song {
        Some(song) => format!(
            "{song} uses its album's cover. Changing it here changes {}'s cover.",
            subject.title
        ),
        None => format!("{}{year}. Only the cover changes.", subject.title),
    };
    let header = row![
        column![
            text("Pick a cover").size(20).font(font::EXTRABOLD),
            text(lead).size(13).color(tokens.sub),
        ]
        .spacing(4)
        .width(Fill),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            *tokens,
            Some(AppMessage::Entry(on_done))
        ),
    ]
    .spacing(12);

    let pick_message = |cover_from: Option<String>| {
        AppMessage::Entry(if subject.stored {
            EntryMessage::SetCover(cover_from)
        } else {
            EntryMessage::PickCover(cover_from)
        })
    };
    let file_tile = |file: &covers::File, mine: bool| -> Element<'a, AppMessage> {
        let url = file.url();
        let (label, extension) = covers::label_and_extension(file.name());
        let fresh = state.fresh.contains(&file.path);
        let (title, sub) = if mine {
            (
                if label.is_empty() { file.name() } else { label }.to_owned(),
                format!(
                    ".{extension}{}",
                    if fresh { " · just uploaded" } else { "" }
                ),
            )
        } else {
            (file.name().to_owned(), "Doesn't match a title".to_owned())
        };
        let choice = ui::cover::Choice {
            handle: app.artwork.image(&url),
            tint: theme::cover_color(&url),
            title: &title,
            sub,
            note: "",
            on: subject.current.as_deref() == Some(url.as_str()),
            square,
            badge: mine.then_some("Yours"),
        };
        let tile_element =
            ui::cover::choice_tile(choice, tile, pick_message(Some(url.clone())), tokens);
        let off = mine
            && shape_of(app, state, &file.path)
                .is_some_and(|(w, h)| picture::off_shape(w, h, square));
        if off {
            ui::cover::with_chip(
                tile_element,
                tile,
                square,
                format!("Not {} · ", if square { "1:1" } else { "2:3" }),
                "Adjust",
                msg(Message::Adjust(file.path.clone())),
                tokens,
            )
        } else {
            tile_element
        }
    };

    // Your covers: Upload, then the files named after the title.
    let files = app.covers.candidates_of(subject.media, &subject.id);
    let folder = format!("{}/{}/", covers::FOLDER, covers::folder(subject.media));
    let yours_note = if files.is_empty() {
        folder.clone()
    } else {
        format!("{} · {folder}", files.len())
    };
    let yours_name = match &subject.song {
        Some(_) => format!("{}'s covers", subject.title),
        None => "Your covers".to_owned(),
    };
    let mut yours: Vec<Element<'a, AppMessage>> = vec![ui::cover::upload_tile(
        tile,
        square,
        state.busy,
        msg(Message::Upload),
        tokens,
    )];
    yours.extend(files.iter().map(|file| file_tile(file, true)));

    // Online: the match's own cover first, then the other matches.
    let (choices, loading, failed) = online(app, subject);
    let with_image = choices.iter().filter(|c| c.url.is_some()).count();
    let online_tiles: Vec<Element<'a, AppMessage>> = choices
        .into_iter()
        .map(|choice| {
            let is_own = choice.cover_from.is_none();
            let on = match (&subject.current, &choice.cover_from) {
                (None, None) => true,
                (Some(current), Some(id)) => current == id,
                _ => false,
            };
            let tile_choice = ui::cover::Choice {
                handle: choice.url.as_deref().and_then(|url| app.artwork.image(url)),
                tint: choice.tint,
                title: &choice.title,
                sub: choice.sub,
                note: if is_own { "The match's cover" } else { "" },
                on,
                square,
                badge: None,
            };
            ui::cover::choice_tile(tile_choice, tile, pick_message(choice.cover_from), tokens)
        })
        .collect();

    let mut body = Column::new()
        .spacing(14)
        .push(section_head(yours_name, yours_note, tokens))
        .push(grid(yours));
    if let Some(error) = &state.error {
        body = body.push(text(error.clone()).size(13).color(tokens.danger));
    }
    body = body
        .push(section_head(
            "Online".to_owned(),
            if with_image == 0 {
                String::new()
            } else {
                with_image.to_string()
            },
            tokens,
        ))
        .push(grid(online_tiles));
    if loading {
        body = body.push(text("Looking for other covers…").size(13).color(tokens.sub));
    } else if failed {
        body = body.push(
            text("Some lookups could not be reached, so a cover may be missing.")
                .size(13)
                .color(tokens.sub),
        );
    }
    if subject.current.is_some() {
        body = body.push(ui::labeled_button(
            "Use the match's cover",
            ButtonKind::Text,
            *tokens,
            Some(pick_message(None)),
        ));
    }

    // Other files in the type's folder, folded.
    let others = app.covers.others(subject.media, library);
    if !others.is_empty() {
        let line = tokens.line;
        let chevron = if state.other_open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let toggle = button(
            row![
                ui::icon(chevron, 14.0, tokens.sub),
                text(format!("Other files in {}", subject.media.label())).size(13.5),
                ui::mono(others.len().to_string(), 11.5, tokens.sub),
                iced::widget::space::horizontal(),
                text("Files whose name doesn't match a title")
                    .size(12.5)
                    .color(tokens.sub),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .width(Fill)
        .padding([6, 0])
        .on_press(msg(Message::ToggleOther))
        .style({
            let text_color = tokens.text;
            move |_, _| button::Style {
                text_color,
                ..button::Style::default()
            }
        })
        .focusable(6.0, tokens);
        body = body
            .push(
                container(Space::new())
                    .width(Fill)
                    .height(1)
                    .style(move |_| container::background(line)),
            )
            .push(toggle);
        if state.other_open {
            body = body.push(grid(
                others.iter().map(|file| file_tile(file, false)).collect(),
            ));
        }
    }

    // The name a file needs, with Copy name.
    // A shade over the panel, whose own color the raised fill is.
    let raised = tokens.hover;
    let hint = container(
        row![
            column![
                row![
                    text("Files named").size(12.5).color(tokens.sub),
                    ui::mono(format!("{}.*", subject.slug()), 12.0, tokens.text),
                ]
                .spacing(6)
                .align_y(Center),
                row![
                    text("in").size(12.5).color(tokens.sub),
                    ui::mono(folder, 12.0, tokens.text),
                    text("show up here.").size(12.5).color(tokens.sub),
                ]
                .spacing(6)
                .align_y(Center),
            ]
            .spacing(3)
            .width(Fill),
            ui::labeled_button(
                if state.copied { "Copied" } else { "Copy name" },
                ButtonKind::Text,
                *tokens,
                Some(msg(Message::CopyName)),
            ),
        ]
        .spacing(8)
        .align_y(Center),
    )
    .padding([9, 12])
    .style(move |_| container::background(raised).border(Border::default().rounded(9)));
    body = body.push(hint);

    // The lists scroll once they outgrow the window.
    let max_height = (app.window.height - 190.0).max(260.0);
    let tokens = *tokens;
    container(
        column![
            header,
            container(ui::scroll(
                container(body).padding(iced::Padding::new(0.0).right(12))
            ))
            .max_height(max_height),
        ]
        .spacing(14),
    )
    .width(PANEL_WIDTH)
    .padding(PANEL_PADDING)
    .style(move |_| ui::panel_style(&tokens, 14.0))
    .into()
}

/// A small label in caps over a value.
fn caps<'a>(label: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(label)
        .size(12)
        .font(font::BOLD)
        .color(tokens.sub)
        .into()
}

/// Fit your cover: the picture with the frame that is kept (or all of it
/// on its blurred copy), the two ways, and where the result goes.
fn fit_view<'a>(
    app: &'a App,
    subject: &Subject,
    state: &'a PickState,
    fit: &'a FitState,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let square = subject.square();
    let url = covers::cover_from(&fit.path);
    let handle = app.artwork.image(&url);
    let tint = theme::cover_color(&url);
    let shape_name = if square { "square" } else { "2:3" };
    let (w, h) = fit.size;
    let name = covers::name_of(&fit.path).to_owned();
    let lead = format!("{name} · {w} × {h}. It isn't {shape_name}, so part of it is cut.");

    let (pw, ph) = preview_size(fit.size);
    let face = |width: f32, height: f32, fill: ContentFit| -> Element<'a, AppMessage> {
        match handle.clone() {
            Some(handle) => Image::new(handle)
                .width(width)
                .height(height)
                .content_fit(fill)
                .into(),
            None => container(Space::new())
                .width(width)
                .height(height)
                .style(move |_| container::background(tint))
                .into(),
        }
    };
    let dim = |width: f32, height: f32| -> Element<'a, AppMessage> {
        container(Space::new())
            .width(width)
            .height(height)
            .style(|_| container::background(theme::FIT_DIM))
            .into()
    };
    let framed = |width: f32, height: f32| {
        container(Space::new())
            .width(width)
            .height(height)
            .style(|_| {
                container::Style::default()
                    .border(Border::default().width(2).color(theme::FIT_FRAME))
            })
    };
    let preview: Element<'a, AppMessage> = match fit.mode {
        Fit::Crop => {
            let scale = pw / w.max(1) as f32;
            let (x, y, fw, fh) = picture::crop_frame(w, h, square, fit.at);
            let (x, y) = ((x as f32 * scale).round(), (y as f32 * scale).round());
            let (fw, fh) = ((fw as f32 * scale).round(), (fh as f32 * scale).round());
            // The part cut away is dimmed on either side of the frame.
            let overlay: Element<'a, AppMessage> = if pw - fw >= ph - fh {
                row![dim(x, ph), framed(fw, ph), dim((pw - x - fw).max(0.0), ph)].into()
            } else {
                column![dim(pw, y), framed(pw, fh), dim(pw, (ph - y - fh).max(0.0))].into()
            };
            let drag = if fit.dragging {
                iced::mouse::Interaction::Grabbing
            } else {
                iced::mouse::Interaction::Grab
            };
            mouse_area(iced::widget::stack![
                face(pw, ph, ContentFit::Fill),
                overlay
            ])
            .on_press(msg(Message::FitPress))
            .on_release(msg(Message::FitRelease))
            .on_exit(msg(Message::FitRelease))
            .on_move(|point| msg(Message::FitMove(point)))
            .interaction(drag)
            .into()
        }
        Fit::Whole => {
            // The result as it will be: all of the picture in a frame of
            // the shape, on its blurred copy.
            let (ow, oh) = picture::fitted_size(w, h, square, Fit::Whole);
            let scale = (PREVIEW.0 / ow as f32).min(PREVIEW.1 / oh as f32);
            let (bw, bh) = ((ow as f32 * scale).round(), (oh as f32 * scale).round());
            let backdrop: Element<'a, AppMessage> = match app.artwork.backdrop(&url) {
                Some(backdrop) => iced::widget::stack![
                    container(Space::new())
                        .width(bw)
                        .height(bh)
                        .style(|_| container::background(Color::BLACK)),
                    Image::new(backdrop)
                        .width(bw)
                        .height(bh)
                        .content_fit(ContentFit::Cover)
                        .opacity(0.6_f32),
                ]
                .into(),
                None => container(Space::new())
                    .width(bw)
                    .height(bh)
                    .style(move |_| container::background(tint.scale_alpha(0.6)))
                    .into(),
            };
            let inner_scale = (bw / w.max(1) as f32).min(bh / h.max(1) as f32);
            let (iw, ih) = (
                (w as f32 * inner_scale).round(),
                (h as f32 * inner_scale).round(),
            );
            container(iced::widget::stack![
                backdrop,
                container(face(iw, ih, ContentFit::Fill))
                    .width(bw)
                    .height(bh)
                    .align_x(Center)
                    .align_y(Center),
                framed(bw, bh),
            ])
            .width(PREVIEW.0)
            .align_x(Center)
            .into()
        }
    };

    let modes = ui::segmented(
        [
            (
                "Crop",
                fit.mode == Fit::Crop,
                msg(Message::FitMode(Fit::Crop)),
            ),
            (
                "Whole image",
                fit.mode == Fit::Whole,
                msg(Message::FitMode(Fit::Whole)),
            ),
        ],
        30.0,
        tokens,
    );
    let tall = (w as f32 / h.max(1) as f32) < picture::shape(square);
    let help = match fit.mode {
        Fit::Crop if tall => "Drag up or down to choose which part is kept.",
        Fit::Crop => "Drag sideways to choose which part is kept.",
        Fit::Whole => "The whole image, centered on a blurred copy of itself.",
    };
    let (ow, oh) = picture::fitted_size(fit.source.0, fit.source.1, square, fit.mode);
    let saved_as = format!(
        "{}/{}/{}.<hash>.jpg",
        covers::FOLDER,
        covers::folder(subject.media),
        subject.slug()
    );
    let mut side = column![
        container(modes).width(220),
        text(help).size(13).line_height(1.45).color(tokens.sub),
        Space::new().height(6),
        caps("SAVED AS", tokens),
        ui::mono(saved_as, 12.0, tokens.text),
        text(format!("{ow} × {oh}")).size(13).color(tokens.sub),
    ]
    .spacing(10)
    .width(Fill);
    if let Some(error) = &state.error {
        side = side.push(text(error.clone()).size(13).color(tokens.danger));
    }
    let footer = row![
        iced::widget::space::horizontal(),
        ui::labeled_button(
            "‹ Back",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::FitBack))
        ),
        ui::labeled_button(
            if fit.working {
                "Saving…"
            } else {
                "Use this cover"
            },
            ButtonKind::Primary,
            *tokens,
            (!fit.working).then(|| msg(Message::FitUse)),
        ),
    ]
    .spacing(10);
    let tokens = *tokens;
    container(
        column![
            column![
                text("Fit your cover").size(20).font(font::EXTRABOLD),
                text(lead).size(13).color(tokens.sub),
            ]
            .spacing(4),
            row![
                container(preview)
                    .width(PREVIEW.0)
                    .height(PREVIEW.1)
                    .align_x(Center)
                    .align_y(Center),
                side
            ]
            .spacing(22),
            footer,
        ]
        .spacing(14),
    )
    .width(FIT_WIDTH)
    .padding(PANEL_PADDING)
    .style(move |_| ui::panel_style(&tokens, 14.0))
    .into()
}

/// "An online cover is available": the owner's cover and the online one
/// side by side, the three answers, Later and Apply.
fn avail_view<'a>(
    app: &'a App,
    subject: &Subject,
    avail: &'a Avail,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let square = subject.square();
    let year = subject.year.map(|y| format!(" · {y}")).unwrap_or_default();
    let lead = format!(
        "{}{year} has an online cover now. You've been using your own.",
        subject.title
    );
    let mine = subject.current.clone().unwrap_or_default();
    let file = subject.repo_path().map(covers::name_of).unwrap_or_default();
    let width = 110.0;
    let height = ui::cover::height_for(width, square);
    let mine_cover = iced::widget::stack![
        ui::plain_cover(
            app.artwork.image(&mine),
            theme::cover_color(&mine),
            width,
            height,
            8.0
        ),
        container(ui::cover::badge("Yours")).padding(6),
    ];
    let online_cover = ui::plain_cover(
        app.artwork.image(&avail.url),
        theme::cover_color(&subject.id),
        width,
        height,
        8.0,
    );
    let pair = row![
        column![mine_cover, ui::mono(file.to_owned(), 11.0, tokens.sub)]
            .spacing(6)
            .align_x(Center),
        text("→").size(20).color(tokens.sub),
        column![
            online_cover,
            text("Online cover").size(12).color(tokens.sub)
        ]
        .spacing(6)
        .align_x(Center),
    ]
    .spacing(18)
    .align_y(Center);

    let folder = format!("{}/{}/", covers::FOLDER, covers::folder(subject.media));
    let options = Column::with_children(OnlineChoice::ALL.into_iter().map(|choice| {
        let help = match choice {
            OnlineChoice::UseAndDelete => format!("Removes {folder}{file} from GitHub."),
            OnlineChoice::UseAndKeep => {
                "Your file stays and is still offered under Your covers.".to_owned()
            }
            OnlineChoice::KeepMine => "Won't ask again for this title.".to_owned(),
        };
        ui::pick_option(
            choice.label(),
            help,
            choice == avail.choice,
            true,
            false,
            tokens,
            msg(Message::AvailChoice(choice)),
        )
    }))
    .spacing(8);
    let footer = row![
        iced::widget::space::horizontal(),
        ui::labeled_button(
            "Later",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::AvailLater))
        ),
        ui::labeled_button(
            "Apply",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::AvailApply))
        ),
    ]
    .spacing(10);
    let tokens = *tokens;
    container(
        column![
            column![
                text("An online cover is available")
                    .size(20)
                    .font(font::EXTRABOLD),
                text(lead).size(13).color(tokens.sub),
            ]
            .spacing(4),
            container(pair).center_x(Fill).padding([4, 0]),
            options,
            footer,
        ]
        .spacing(14),
    )
    .width(AVAIL_WIDTH)
    .padding(PANEL_PADDING)
    .style(move |_| ui::panel_style(&tokens, 14.0))
    .into()
}

/// What the open picker shows: the owner's files for the title (and the
/// folder's other files once unfolded), each online choice, and for Fit
/// your cover the file's blurred copy.
pub fn wanted(app: &App, wants: &mut Wants) {
    let (Some(subject), Some(state)) = (subject(app), state(app)) else {
        return;
    };
    if subject.stored {
        wants.covers.push(subject.id.clone());
    }
    if let Some(current) = &subject.current
        && covers::repo_path(current).is_some()
    {
        wants.images.push(current.clone());
    }
    if let Some(avail) = &state.avail {
        wants.images.push(avail.url.clone());
        return;
    }
    if let Some(fit) = &state.fit {
        let url = covers::cover_from(&fit.path);
        wants.backdrops.push(url.clone());
        wants.images.push(url);
        return;
    }
    for file in app.covers.candidates_of(subject.media, &subject.id) {
        wants.sizes.push(file.url());
        wants.images.push(file.url());
    }
    if state.other_open {
        for file in app.covers.others(subject.media, app.repo.library()) {
            wants.images.push(file.url());
        }
    }
    for choice in online(app, &subject).0 {
        if let Some(url) = choice.url {
            wants.images.push(url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Date, EntryKind};
    use crate::screens::Screen;
    use crate::storage::TempDir;

    const BABY: &str = "movies/million-dollar-baby-2004";
    const STEELBOOK: &str = "movies/million-dollar-baby-2004.steelbook.jpg";
    const SONG: &str = "music/bad-moon-rising-creedence-clearwater-revival";

    fn app() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app.covers = Covers::with_files(covers::demo());
        app
    }

    fn send(app: &mut App, message: EntryMessage) {
        let _ = super::super::update(app, message);
    }

    fn pick_says(app: &mut App, message: Message) {
        let _ = update(app, message);
    }

    fn entry<'a>(app: &'a App, id: &str) -> &'a Entry {
        app.repo.library().entry(id).unwrap()
    }

    /// A PNG of `width` × `height`.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbImage::from_pixel(width, height, image::Rgb([120, 60, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(picture)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn open_viewer(app: &mut App, id: &str) {
        send(app, EntryMessage::ViewCover(id.into()));
        send(app, EntryMessage::ChangeCover);
    }

    /// Marks the entry as keeping its own cover, so no online cover is
    /// offered when the picker opens.
    fn show_own(app: &mut App, id: &str, path: &str, keep: bool) {
        let changed = Entry {
            cover_from: Some(covers::cover_from(path)),
            cover_keep_mine: keep.then_some(true),
            ..entry(app, id).clone()
        };
        app.apply(Operation::UpdateEntry(changed));
    }

    #[test]
    fn the_picker_lists_the_titles_files_and_a_pick_is_saved_at_once() {
        let mut app = app();
        open_viewer(&mut app, BABY);
        let subject = subject(&app).unwrap();
        assert_eq!(subject.slug(), "million-dollar-baby-2004");
        let files = app.covers.candidates_of(subject.media, &subject.id);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, STEELBOOK);
        // A file committed by hand is offered, never applied on its own.
        assert!(entry(&app, BABY).cover_from.is_none());
        send(
            &mut app,
            EntryMessage::SetCover(Some(covers::cover_from(STEELBOOK))),
        );
        assert_eq!(
            entry(&app, BABY).cover_from.as_deref(),
            Some("repo:movies/million-dollar-baby-2004.steelbook.jpg")
        );
        // The image is the file itself: no lookup stands between.
        assert_eq!(
            app.artwork.cover_url(entry(&app, BABY)),
            Some("repo:movies/million-dollar-baby-2004.steelbook.jpg")
        );
        // The folder's other files are the ones named after no title.
        let others = app.covers.others(MediaType::Movies, app.repo.library());
        assert_eq!(others.len(), 4);
        send(&mut app, EntryMessage::SetCover(None));
        assert!(entry(&app, BABY).cover_from.is_none());
        assert!(entry(&app, BABY).cover_keep_mine.is_none());
    }

    #[test]
    fn an_upload_is_named_after_the_title_stored_and_picked() {
        let mut app = app();
        open_viewer(&mut app, BABY);
        let upload = make(png(400, 600)).unwrap();
        let sha = upload.prepared.sha.clone();
        pick_says(&mut app, Message::Prepared(Ok(Some(upload))));
        let path = format!("movies/million-dollar-baby-2004.{}.jpg", &sha[..7]);
        assert_eq!(entry(&app, BABY).cover_from, Some(format!("repo:{path}")));
        assert!(app.covers.get(&path).is_some(), "listed at once");
        assert_eq!(app.covers.candidates(entry(&app, BABY)).len(), 2);
        let state = state(&app).unwrap();
        assert_eq!(state.fresh, std::slice::from_ref(&path), "just uploaded");
        assert!(!state.busy);
        // 2:3: no Adjust.
        let (w, h) = shape_of(&app, state, &path).unwrap();
        assert!(!picture::off_shape(w, h, false));

        // What cannot be read says why, and nothing changes.
        pick_says(
            &mut app,
            Message::Prepared(Err("This file is not an image Libri can read.".into())),
        );
        assert!(state_mut(&mut app).unwrap().error.is_some());
        // A closed dialog is nothing.
        pick_says(&mut app, Message::Prepared(Ok(None)));
        assert_eq!(entry(&app, BABY).cover_from, Some(format!("repo:{path}")));
    }

    #[test]
    fn a_picture_of_another_shape_is_fitted_and_takes_the_uploads_place() {
        let dir = TempDir::new("pick");
        let mut app = app();
        app.covers = Covers::at(Some(dir.path("covers")), None);
        open_viewer(&mut app, BABY);
        pick_says(
            &mut app,
            Message::Prepared(Ok(Some(make(png(1920, 1080)).unwrap()))),
        );
        let wide = entry(&app, BABY)
            .cover_from
            .as_deref()
            .and_then(covers::repo_path)
            .unwrap()
            .to_owned();
        let size = shape_of(&app, state(&app).unwrap(), &wide).unwrap();
        assert_eq!(size, (1000, 563));
        assert!(
            picture::off_shape(size.0, size.1, false),
            "Not 2:3 · Adjust"
        );

        pick_says(&mut app, Message::Adjust(wide.clone()));
        let fit = state(&app).unwrap().fit.clone().unwrap();
        assert_eq!((fit.mode, fit.at), (Fit::Crop, 0.5));
        // Dragging moves the frame along the long side, to its ends.
        pick_says(&mut app, Message::FitMove(Point::new(0.0, 100.0)));
        pick_says(&mut app, Message::FitPress);
        assert_eq!(state(&app).unwrap().fit.as_ref().unwrap().at, 0.0);
        pick_says(&mut app, Message::FitMove(Point::new(400.0, 100.0)));
        assert_eq!(state(&app).unwrap().fit.as_ref().unwrap().at, 1.0);
        pick_says(&mut app, Message::FitRelease);
        pick_says(&mut app, Message::FitMove(Point::new(10.0, 100.0)));
        assert_eq!(state(&app).unwrap().fit.as_ref().unwrap().at, 1.0);
        pick_says(&mut app, Message::FitMode(Fit::Whole));

        // Use this cover: made from the picture as it was picked.
        let original = app.covers.original(&wide).unwrap();
        let prepared = picture::fit(&original, Fit::Whole, 1.0, false).unwrap();
        assert_eq!((prepared.width, prepared.height), (1000, 1500));
        pick_says(
            &mut app,
            Message::Fitted {
                from: wide.clone(),
                result: Ok(Upload {
                    prepared: Arc::new(prepared),
                    original,
                }),
            },
        );
        let fitted = entry(&app, BABY)
            .cover_from
            .as_deref()
            .and_then(covers::repo_path)
            .unwrap()
            .to_owned();
        assert_ne!(fitted, wide);
        assert!(state(&app).unwrap().fit.is_none());
        assert!(app.covers.ready(&fitted).is_some());
        // The upload it replaces is gone, here and from the list.
        assert!(app.covers.get(&wide).is_none());
        assert!(!dir.path("covers").join(&wide).exists());
    }

    const DRACULA: &str = "books/dracula-1897";
    const DRACULA_OWN: &str = "books/dracula-1897.first-edition.jpg";

    /// The demo's files and one for Dracula, which the fake's lookup has
    /// a cover for.
    fn app_with_dracula() -> App {
        let mut app = app();
        let mut files = covers::demo();
        files.push(covers::File {
            path: DRACULA_OWN.into(),
            size: 200_000,
            sha: "d".repeat(40),
        });
        app.covers = Covers::with_files(files);
        app
    }

    #[test]
    fn an_online_cover_for_a_title_with_your_own_is_asked_about_once_opened() {
        let mut app = app_with_dracula();
        let plain = entry(&app, DRACULA).clone();
        let online = artwork::resolve_cover(&app.lookups, &plain)
            .ok()
            .and_then(|(url, _)| url)
            .expect("the fake's lookup has a cover for Dracula");
        show_own(&mut app, DRACULA, DRACULA_OWN, false);
        open_viewer(&mut app, DRACULA);
        let avail = state(&app).unwrap().avail.clone().unwrap();
        assert_eq!(avail.url, online);
        assert_eq!(avail.choice, OnlineChoice::UseAndDelete, "the default");
        assert!(!takes_paste(&app), "the question is in the way");

        // Later: the picker, and the question again the next time.
        pick_says(&mut app, Message::AvailLater);
        assert!(state(&app).unwrap().avail.is_none());
        assert!(entry(&app, DRACULA).cover_from.is_some());
        send(&mut app, EntryMessage::Close);
        open_viewer(&mut app, DRACULA);
        assert!(state(&app).unwrap().avail.is_some());

        // Keep mine: remembered on the entry, and never asked again.
        pick_says(&mut app, Message::AvailChoice(OnlineChoice::KeepMine));
        pick_says(&mut app, Message::AvailApply);
        assert_eq!(entry(&app, DRACULA).cover_keep_mine, Some(true));
        assert!(entry(&app, DRACULA).cover_from.is_some());
        send(&mut app, EntryMessage::Close);
        open_viewer(&mut app, DRACULA);
        assert!(state(&app).unwrap().avail.is_none());
        send(&mut app, EntryMessage::Close);

        // Use online and delete mine: the file goes with the pick.
        show_own(&mut app, DRACULA, DRACULA_OWN, false);
        open_viewer(&mut app, DRACULA);
        pick_says(&mut app, Message::AvailApply);
        assert!(entry(&app, DRACULA).cover_from.is_none());
        assert!(entry(&app, DRACULA).cover_keep_mine.is_none());
        assert!(app.covers.get(DRACULA_OWN).is_none());

        // Use online, keep my file: still offered under Your covers.
        let mut app = app_with_dracula();
        show_own(&mut app, DRACULA, DRACULA_OWN, false);
        open_viewer(&mut app, DRACULA);
        pick_says(&mut app, Message::AvailChoice(OnlineChoice::UseAndKeep));
        pick_says(&mut app, Message::AvailApply);
        assert!(entry(&app, DRACULA).cover_from.is_none());
        assert_eq!(app.covers.candidates(entry(&app, DRACULA)).len(), 1);

        // Picking your own while the lookup's cover is there to see is a
        // choice made: remembered, and never asked about. (The app looks
        // the cover up after every update; here by hand.)
        let wants = crate::screens::wanted(&app);
        let _ = app
            .artwork
            .refresh(&wants, app.repo.library(), &app.lookups);
        send(
            &mut app,
            EntryMessage::SetCover(Some(covers::cover_from(DRACULA_OWN))),
        );
        assert_eq!(entry(&app, DRACULA).cover_keep_mine, Some(true));
    }

    #[test]
    fn a_songs_picker_works_on_its_album() {
        let mut app = app();
        let song = entry(&app, SONG).clone();
        let album = Entry {
            id: "music/green-river-creedence-clearwater-revival".into(),
            title: "Green River".into(),
            kind: Some(EntryKind::Album),
            album: None,
            cover_from: None,
            source: None,
            ..song.clone()
        };
        app.apply(Operation::AddEntry(album.clone()));
        open_viewer(&mut app, SONG);
        let subject = subject(&app).unwrap();
        assert_eq!(subject.id, album.id);
        assert_eq!(subject.song.as_deref(), Some("Bad Moon Rising"));
        // An upload is named after the album and changes the album.
        pick_says(
            &mut app,
            Message::Prepared(Ok(Some(make(png(500, 500)).unwrap()))),
        );
        let shown = entry(&app, &album.id).cover_from.clone().unwrap();
        assert!(shown.starts_with("repo:music/green-river-creedence-clearwater-revival."));
        assert!(entry(&app, SONG).cover_from.is_none());
        // And the song shows it, once a screen has shown the song.
        let mut wants = Wants::default();
        wants.covers.push(SONG.into());
        let _ = app
            .artwork
            .refresh(&wants, app.repo.library(), &app.lookups);
        assert_eq!(
            app.artwork.cover_url(entry(&app, SONG)),
            Some(shown.as_str())
        );
    }

    #[test]
    fn the_form_keeps_a_pick_until_save_and_a_type_change_moves_the_files() {
        let mut app = app();
        send(&mut app, EntryMessage::Edit(BABY.into()));
        send(&mut app, EntryMessage::OpenCover);
        let subject = subject(&app).unwrap();
        assert!(!subject.stored);
        send(
            &mut app,
            EntryMessage::PickCover(Some(covers::cover_from(STEELBOOK))),
        );
        assert!(
            entry(&app, BABY).cover_from.is_none(),
            "nothing before Save"
        );
        send(&mut app, EntryMessage::CloseCover);
        // The type changes: one of your own covers stays picked.
        send(&mut app, EntryMessage::Type(MediaType::Shows));
        send(&mut app, EntryMessage::Save);
        let saved = entry(&app, BABY);
        assert_eq!(saved.media, MediaType::Shows);
        assert_eq!(
            saved.cover_from.as_deref(),
            Some("repo:shows/million-dollar-baby-2004.steelbook.jpg")
        );
        assert!(app.covers.get(STEELBOOK).is_none());
        assert!(
            app.covers
                .get("shows/million-dollar-baby-2004.steelbook.jpg")
                .is_some()
        );
        // The id keeps its first type; the files are found all the same.
        assert_eq!(app.covers.candidates(entry(&app, BABY)).len(), 1);
    }

    #[test]
    fn deleting_a_title_offers_to_delete_its_cover_files() {
        let mut app = app();
        show_own(&mut app, BABY, STEELBOOK, true);
        send(&mut app, EntryMessage::Delete(BABY.into()));
        assert!(matches!(
            app.dialog,
            Some(Dialog::Delete { covers: true, .. })
        ));
        send(&mut app, EntryMessage::ConfirmDelete);
        assert!(app.repo.library().entry(BABY).is_none());
        assert!(app.covers.get(STEELBOOK).is_none());

        // Unticked, the file stays and is unused from then on.
        let mut app = self::app();
        show_own(&mut app, BABY, STEELBOOK, true);
        send(&mut app, EntryMessage::Delete(BABY.into()));
        send(&mut app, EntryMessage::ToggleDeleteCovers);
        send(&mut app, EntryMessage::ConfirmDelete);
        assert!(app.covers.get(STEELBOOK).is_some());
        let unused = app.covers.unused(app.repo.library());
        assert!(unused.iter().any(|file| file.path == STEELBOOK));
    }

    #[test]
    fn ctrl_v_belongs_to_the_open_picker_alone() {
        let mut app = app();
        assert!(!takes_paste(&app));
        send(&mut app, EntryMessage::ViewCover(BABY.into()));
        assert!(!takes_paste(&app), "the cover alone");
        send(&mut app, EntryMessage::ChangeCover);
        assert!(takes_paste(&app));
        // Esc closes the picker, then the viewer.
        assert!(super::super::dismiss_top(&mut app));
        assert!(matches!(app.dialog, Some(Dialog::Cover(_))));
        assert!(!takes_paste(&app));
    }

    #[test]
    fn the_picker_draws_its_sections() {
        let mut app = app();
        show_own(&mut app, BABY, STEELBOOK, true);
        open_viewer(&mut app, BABY);
        pick_says(&mut app, Message::ToggleOther);
        let mut ui = iced_test::Simulator::with_size(
            crate::app::settings(),
            crate::app::WINDOW_SIZE,
            app.view(),
        );
        for words in [
            "Pick a cover",
            "YOUR COVERS",
            "Upload",
            "Browse or Ctrl+V",
            "Yours",
            "steelbook",
            "ONLINE",
            "Other files in Movies",
            "old-poster.png",
            "Copy name",
            "Movie · 2004 · Your cover: million-dollar-baby-2004.steelbook.jpg",
        ] {
            assert!(ui.find(words).is_ok(), "{words}");
        }
    }
}
