//! The details pane beside the list, shown while an entry is selected:
//! cover, type, title, creator, released, then Status and Rating on one
//! row each, Seasons or Series, Tags, Note, About and In plans, with Edit
//! and Delete at the bottom. Inside a plan it also shows the entry's place
//! in it (when, and the plan's note), and Delete asks whether to remove it
//! from the plan only.
//!
//! The pane never scrolls (2026-11-07): it is built for the height it is
//! given, and the parts that can give way do: About loses lines down to
//! two, then the note goes to one line, then About keeps only its header.
//! Seasons (the latest three), the tags (two rows of chips, the rest in a
//! "+N" chip) and everything else keep their space, until even that is
//! too tall: then the Seasons or Series list keeps one row, and should
//! that still not fit (a short window), the pane scrolls after all rather
//! than cut anything off. The note, the full About and every tag open in
//! a popover over their section, and the status in a menu under its pill.

use iced::widget::{
    Column, Row, button, column, container, mouse_area, responsive, row, space, text, text_editor,
};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{Message, seasons};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Entry, MediaType, Plan, Series};
use crate::music;
use crate::screens::library::{self, DetailsPop, Page};
use crate::screens::plans;
use crate::series;
use crate::theme::{self, Tokens, font};
use crate::ui::{self, Align, ButtonKind, Edge, FocusExt, Icon, Placement, Popover};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// The pane is this wide.
pub const WIDTH: f32 = 352.0;
/// The pane's scroll area (the series pane, which scrolls), scrolled to
/// the top for each series it opens on, and to the marked part.
pub const PANE_ID: &str = "details-pane";

/// The pane's padding at its sides, and the body's width inside it.
pub const PAD: f32 = 22.0;
pub const INNER: f32 = WIDTH - 2.0 * PAD;
/// The gap between sections, and under a section's header.
const GAP: f32 = 14.0;
const HEADER_GAP: f32 = 8.0;
/// A header of caps alone, and one with a link or a count at the right.
const HEADER_H: f32 = 18.0;
const LINK_HEADER_H: f32 = 24.0;
/// The cover, and the gap between it and the facts. An album's is a
/// square of `ALBUM_COVER`, which the facts beside it keep within too.
const COVER_W: f32 = 96.0;
const COVER_H: f32 = 144.0;
const ALBUM_COVER: f32 = 120.0;
const COVER_GAP: f32 = 16.0;
/// The title's sizes, largest first, at 1.1 per line: the facts beside
/// the cover never run past its height.
const TITLE_SIZES: [f32; 3] = [24.0, 21.0, 18.0];
const TITLE_LINE: f32 = 1.1;
/// The Status row and the Rating button.
const STATUS_H: f32 = 32.0;
const RATING_H: f32 = 36.0;
/// A season's or a series part's row, and the gap between rows.
const ROW_H: f32 = 50.0;
const ROW_GAP: f32 = 2.0;
/// A tag chip, the gap between chips, and how many rows of them show.
const CHIP_H: f32 = 26.0;
const CHIP_GAP: f32 = 6.0;
const TAG_ROWS: usize = 2;
/// About's type: 14 px at 1.55.
const ABOUT_SIZE: f32 = 14.0;
const ABOUT_LINE: f32 = 1.55;
/// The note's type, 14 px italic at 1.45, and its field's padding.
const NOTE_SIZE: f32 = 14.0;
const NOTE_LINE: f32 = 1.45;
const NOTE_PAD: f32 = 7.0;
/// A plan chip in the In plans row, and the gap between chips.
const PLAN_CHIP_H: f32 = 28.0;
const PLAN_CHIP_GAP: f32 = 6.0;
/// A track row, and how many the track list keeps at least.
const TRACK_H: f32 = 36.0;
const TRACK_MIN_ROWS: f32 = 3.0;
/// Edit and Delete with the padding around them, and the body's padding
/// under its last section.
const ACTIONS_H: f32 = 36.0 + 6.0 + 18.0;
const BODY_BOTTOM: f32 = 8.0;
/// The popovers are the section plus this much on each side.
const POP_INSET: f32 = 12.0;
/// How tall the About popover grows at most, and the lines About shows at
/// most when there is room.
const ABOUT_POP_MAX: f32 = 520.0;
const ABOUT_MAX_LINES: usize = 10;

/// What the pane shows of the parts that give way: About's lines (0 is
/// its header alone), the note's lines, the track list's height, whether
/// the Seasons or Series list keeps one row, and whether the pane has to
/// scroll after all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub about_lines: usize,
    pub note_lines: usize,
    pub tracks_height: f32,
    pub one_row: bool,
    pub scrolls: bool,
}

/// The title at `width` in `budget` px: the first of `TITLE_SIZES` whose
/// wrapped lines fit, else the smallest cut to the lines that fit (at
/// least one) with an ellipsis. The text, its size and its height.
pub fn fit_title(title: &str, width: f32, budget: f32) -> (String, f32, f32) {
    let flat = title.split_whitespace().collect::<Vec<_>>().join(" ");
    for size in TITLE_SIZES {
        let n = ui::line_count(&flat, size, TITLE_LINE, font::EXTRABOLD, width).max(1);
        let height = n as f32 * size * TITLE_LINE;
        if height <= budget + 0.5 {
            return (flat, size, height);
        }
    }
    let size = TITLE_SIZES[TITLE_SIZES.len() - 1];
    let line = size * TITLE_LINE;
    let max_lines = ((budget / line).floor() as usize).max(1);
    let (cut, _) = ui::clamp(&flat, size, TITLE_LINE, font::EXTRABOLD, width, max_lines);
    (cut, size, max_lines as f32 * line)
}

fn about_line_px() -> f32 {
    ABOUT_SIZE * ABOUT_LINE
}

fn note_field_h(lines: usize) -> f32 {
    2.0 * NOTE_PAD + lines as f32 * NOTE_SIZE * NOTE_LINE
}

/// The fit for `avail` px of body with `fixed` px taken by everything
/// but About's lines, the note's lines and the track list: About takes
/// what is left (at most ten lines) with the note at two lines; under two
/// lines the note gives one up and About takes what that leaves, down to
/// one line; when not even one fits About folds to its header, and the
/// note gets its second line back if it fits. The tracks, when there are
/// any, get the rest and at least three rows. When the body is still too tall, the
/// Seasons or Series list keeps one row, freeing `spare_rows` px, and
/// when that is not enough either the pane scrolls.
pub fn fit(fixed: f32, avail: f32, has_about: bool, has_tracks: bool, spare_rows: f32) -> Fit {
    let mut fit = fit_lines(fixed, avail, has_about, has_tracks);
    let used = |fit: &Fit| {
        fixed
            + fit.about_lines as f32 * about_line_px()
            + note_field_h(fit.note_lines)
            + fit.tracks_height
    };
    if used(&fit) > avail && spare_rows > 0.0 {
        fit = fit_lines(fixed - spare_rows, avail, has_about, has_tracks);
        fit.one_row = true;
        fit.scrolls = used(&fit) - spare_rows > avail;
    } else {
        fit.scrolls = used(&fit) > avail;
    }
    fit
}

fn fit_lines(fixed: f32, avail: f32, has_about: bool, has_tracks: bool) -> Fit {
    let tracks_min = if has_tracks {
        TRACK_MIN_ROWS * TRACK_H
    } else {
        0.0
    };
    let rest = avail - fixed - tracks_min;
    let lines = |note: usize| {
        let left = rest - note_field_h(note);
        ((left / about_line_px()).floor().max(0.0) as usize).min(ABOUT_MAX_LINES)
    };
    let (about_lines, note_lines) = if !has_about {
        (0, if rest - note_field_h(2) >= 0.0 { 2 } else { 1 })
    } else if lines(2) >= 2 {
        (lines(2), 2)
    } else if lines(1) >= 1 {
        (lines(1), 1)
    } else {
        (0, if rest - note_field_h(2) >= 0.0 { 2 } else { 1 })
    };
    let used = about_lines as f32 * about_line_px() + note_field_h(note_lines);
    let tracks_height = if has_tracks {
        (rest + tracks_min - used).max(tracks_min)
    } else {
        0.0
    };
    Fit {
        about_lines,
        note_lines,
        tracks_height,
        one_row: false,
        scrolls: false,
    }
}

pub fn view<'a>(app: &'a App, entry: &'a Entry, plan: Option<&'a Plan>) -> Element<'a, AppMessage> {
    responsive(move |size| build(app, entry, plan, size.width, size.height)).into()
}

/// The pane for `width` by `height` px (Home's column is wider than the
/// panel).
fn build<'a>(
    app: &'a App,
    entry: &'a Entry,
    plan: Option<&'a Plan>,
    width: f32,
    height: f32,
) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let inner = (width - 2.0 * PAD).max(INNER);
    let id = || entry.id.clone();
    let color = tokens.type_color(entry.media);
    let pop = app.library.details_pop.as_ref();
    let today = app.today;
    let mut fixed = 0.0;

    // The header: the cover, and the facts beside it.
    let tint = theme::cover_color(&entry.id);
    let (cover_w, cover_h) = if ui::cover::is_square(entry.media) {
        (ALBUM_COVER, ALBUM_COVER)
    } else {
        (COVER_W, COVER_H)
    };
    // A click opens the cover viewer, where the cover can be changed.
    let cover = mouse_area(
        button(ui::titled_cover(
            app.artwork.cover(entry),
            tint,
            &entry.title,
            cover_w,
            cover_h,
            14.0,
        ))
        .padding(0)
        .on_press(msg(Message::ViewCover(entry.id.clone())))
        .style(|_, _| button::Style::default())
        .focusable(8.0, &tokens),
    )
    .on_enter(msg(Message::WarmCover(entry.id.clone())))
    .interaction(iced::mouse::Interaction::ZoomIn);
    let seasons = entry.season_list();
    // A show's kicker carries the years it ran: "SHOW · 2025–2026".
    let mut kicker = browse::kind_label(entry).to_uppercase();
    if !seasons.is_empty()
        && let Some(years) = entry.years(today)
    {
        kicker = format!("{kicker} · {years}");
    }
    // The facts beside the cover, each with its height, so they never run
    // past the cover: the title takes what the other facts leave, smaller
    // when it has to be (`fit_title`).
    let facts_w = inner - cover_w - COVER_GAP;
    let lines = |words: &str, size: f32, face| {
        ui::line_count(words, size, 1.3, face, facts_w).max(1) as f32 * size * 1.3
    };
    let mut facts: Vec<(Element<'a, AppMessage>, f32)> =
        vec![(ui::mono(kicker, 11.0, color).into(), 11.0 * 1.3)];
    if let Some(creator) = &entry.creator {
        facts.push((
            text(creator).size(14).color(tokens.sub).into(),
            lines(creator, 14.0, font::REGULAR),
        ));
    }
    if let Some(pages) = entry.pages {
        facts.push((
            text(format!("{pages} pages"))
                .size(14)
                .color(tokens.sub)
                .into(),
            14.0 * 1.3,
        ));
    }
    if let Some(album) = &entry.album {
        facts.push((
            text(album)
                .size(14)
                .font(font::ITALIC)
                .color(tokens.sub)
                .into(),
            lines(album, 14.0, font::ITALIC),
        ));
    }
    if seasons.is_empty() {
        if let Some(released) = entry.first_released() {
            facts.push((
                ui::mono(released.to_string(), 12.5, tokens.sub).into(),
                12.5 * 1.3,
            ));
        }
    } else {
        let mut line = entry.years(today).unwrap_or_default();
        if seasons.len() > 1 {
            line = format!("{line} · {}", entry.season_count_label());
        }
        facts.push((ui::mono(line, 12.5, tokens.sub).into(), 12.5 * 1.3));
    }
    if entry.has_new_season(today) {
        facts.push((ui::new_season_badge(&tokens), 20.0));
    }
    // The other facts with the gap each adds, then the title in what is
    // left of the cover's height, under the kicker.
    let rest_h = 2.0 + facts.iter().map(|(_, h)| h + 6.0).sum::<f32>();
    let (title, title_size, title_h) = fit_title(&entry.title, facts_w, cover_h - rest_h);
    facts.insert(
        1,
        (
            text(title)
                .size(title_size)
                .font(font::EXTRABOLD)
                .line_height(TITLE_LINE)
                .into(),
            title_h,
        ),
    );
    let facts = container(
        Column::with_children(facts.into_iter().map(|(line, _)| line))
            .spacing(6)
            .padding(Padding::new(0.0).top(2))
            .width(Fill),
    )
    .height(cover_h)
    .clip(true);
    let header = row![cover, facts].spacing(COVER_GAP);
    fixed += cover_h;

    // A part picked from a series in the pane: ‹ the series' name, back.
    let header: Element<'a, AppMessage> = match app.library.back_series() {
        Some(back) if plan.is_none() => {
            fixed += 30.0 + GAP;
            column![back_link(&back.name, &tokens), header]
                .spacing(GAP)
                .into()
        }
        _ => header.into(),
    };
    let mut sections = Column::new().spacing(GAP).push(header);

    if let Some(plan) = plan {
        fixed += GAP + HEADER_H + HEADER_GAP + if plan.timeline { 67.0 } else { 0.0 } + 59.0;
        sections = sections.push(section(
            "This plan",
            in_plan(app, plan, entry, &tokens),
            &tokens,
        ));
    }

    // Status: the pill with its menu at the row's right end. A show with
    // several seasons has its status set by them; with one, the pill sets
    // Season 1, which is the show's.
    let status: Element<'a, AppMessage> = match seasons {
        [] => {
            let for_entry = id();
            ui::status_pill(
                entry.media,
                entry.status,
                pop == Some(&DetailsPop::Status),
                msg(Message::TogglePop(DetailsPop::Status)),
                move |status| msg(Message::SetStatus(for_entry.clone(), status)),
                &tokens,
            )
        }
        [only] => seasons::pill(app, entry, only, false, &tokens),
        _ => row![
            text(browse::status_label(Some(entry.media), entry.status))
                .size(14)
                .font(font::SEMIBOLD),
            text("· from seasons").size(14).color(tokens.sub),
        ]
        .spacing(5)
        .into(),
    };
    sections = sections.push(labeled_row("Status", status, STATUS_H, &tokens));
    fixed += GAP + STATUS_H;

    let rating = ui::rating(
        entry.rating,
        app.library.details_rating_open,
        msg(Message::ToggleDetailsRating),
        move |rating| msg(Message::SetRating(entry.id.clone(), rating)),
        Align::End,
        &tokens,
    );
    sections = sections.push(labeled_row("Rating", rating, RATING_H, &tokens));
    fixed += GAP + RATING_H;

    // Seasons or Series: measured with three rows, built after the fit,
    // which may keep one row.
    let listed = if !seasons.is_empty() {
        seasons.len().min(3)
    } else {
        match &entry.series {
            Some(part_of) if entry.can_have_series() => series_rows(app, entry, part_of).1.len(),
            _ => 0,
        }
    };
    let spare_rows = rows_h(listed) - rows_h(listed.min(1));
    if !seasons.is_empty() {
        fixed += GAP + LINK_HEADER_H + HEADER_GAP + rows_h(listed);
    } else if listed > 0 {
        fixed += GAP + LINK_HEADER_H + HEADER_GAP + 13.0 * 1.3 + ROW_GAP + rows_h(listed);
    }

    // Tags: two rows of chips at most, the rest folded into "+N", which
    // opens every tag in a popover over the section.
    let (tags, tags_h) = tags_section(app, entry, inner, pop == Some(&DetailsPop::Tags), &tokens);
    fixed += GAP + tags_h;

    let has_tracks = entry.media == MediaType::Music && !music::is_song(entry);
    let has_about = !entry.description.is_empty();
    // The note's header (its field is `note_field_h`, padding and all),
    // About's header, and In plans.
    fixed += GAP + HEADER_H + HEADER_GAP;
    if has_about {
        fixed += GAP + LINK_HEADER_H;
    }
    let (plans_row, plans_h) = plans_section(app, entry, inner, &tokens);
    fixed += GAP + plans_h;
    if has_tracks {
        fixed += GAP + LINK_HEADER_H + 6.0;
    }
    let avail = height - PAD - BODY_BOTTOM - ACTIONS_H;
    let fit = fit(fixed, avail, has_about, has_tracks, spare_rows);

    if !seasons.is_empty() {
        sections = sections.push(seasons_section(app, entry, fit.one_row, &tokens));
    } else if let Some(part_of) = &entry.series
        && entry.can_have_series()
    {
        sections = sections.push(series_section(app, entry, part_of, fit.one_row, &tokens));
    }
    if has_tracks {
        sections = sections.push(tracks(app, entry, fit.tracks_height, &tokens));
    }
    sections = sections.push(tags);
    sections = sections.push(note_section(
        app,
        entry,
        inner,
        fit.note_lines,
        pop,
        &tokens,
    ));
    if has_about {
        sections = sections.push(about_section(
            entry,
            inner,
            fit.about_lines,
            pop == Some(&DetailsPop::About),
            &tokens,
        ));
    }
    sections = sections.push(plans_row);

    // Inside a plan, Delete asks: this plan only, or everywhere (D4).
    let delete = match plan {
        Some(_) => AppMessage::Plans(plans::Message::RemoveItem(id())),
        None => msg(Message::Delete(id())),
    };
    let actions = row![
        ui::wide_button(
            "Edit",
            ButtonKind::Neutral,
            tokens,
            Some(msg(Message::Edit(id())))
        ),
        ui::labeled_button("Delete", ButtonKind::Destructive, tokens, Some(delete)),
    ]
    .spacing(8);

    if fit.scrolls {
        return scrolling_pane(sections.into(), Some(actions.into()));
    }
    pane(sections.into(), Some(actions.into()))
}

/// The pane around `body`, which never scrolls, with `actions` kept at
/// the bottom. As wide as its place: `WIDTH` beside a list, Home's right
/// column. It paints no background: the content pane's surface is
/// already under it, and a second coat of a see-through content color
/// (Glass) would make the panel darker than the content.
pub fn pane<'a>(
    body: Element<'a, AppMessage>,
    actions: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let bottom = if actions.is_some() { BODY_BOTTOM } else { 18.0 };
    let mut column = Column::new().height(Fill).push(
        container(body)
            .width(Fill)
            .height(Fill)
            .padding(Padding::new(PAD).bottom(bottom)),
    );
    if let Some(actions) = actions {
        column = column.push(container(actions).padding(Padding::new(PAD).top(6).bottom(18)));
    }
    container(column).width(Fill).height(Fill).into()
}

/// The pane around a `body` that scrolls: the series pane, which lists
/// every part and scrolls to the marked one, and the details when even
/// their shortest form is taller than the panel.
pub fn scrolling_pane<'a>(
    body: Element<'a, AppMessage>,
    actions: Option<Element<'a, AppMessage>>,
) -> Element<'a, AppMessage> {
    let bottom = if actions.is_some() { BODY_BOTTOM } else { 18.0 };
    let mut column = Column::new().height(Fill).push(
        ui::scroll(container(body).padding(Padding::new(PAD).bottom(bottom)))
            .id(PANE_ID)
            .height(Fill),
    );
    if let Some(actions) = actions {
        column = column.push(container(actions).padding(Padding::new(PAD).top(6).bottom(18)));
    }
    container(column).width(Fill).height(Fill).into()
}

/// "‹ Kung Fu Panda" above a part's details, back to its series.
fn back_link<'a>(name: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(
        row![
            ui::icon(Icon::ChevronLeft, 16.0, tokens.sub),
            text(Series::display_name(name))
                .size(13.5)
                .color(tokens.sub)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(4)
        .height(Fill)
        .align_y(iced::Center),
    )
    .height(30)
    .padding(Padding::new(0.0).left(2).right(10))
    .on_press(AppMessage::Library(library::Message::BackToSeries))
    .style(move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered)
            .then_some(Background::Color(tokens.hover)),
        text_color: tokens.sub,
        border: Border::default().rounded(8),
        ..button::Style::default()
    })
    .focusable(8.0, &tokens)
}

/// A section header with `control` at the row's right end, on one row.
fn labeled_row<'a>(
    label: &str,
    control: Element<'a, AppMessage>,
    height: f32,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    row![
        ui::section_header(label, tokens),
        space::horizontal(),
        control
    ]
    .align_y(Center)
    .height(height)
    .into()
}

/// A section header with something at its right end: a link, or a count.
fn header_with<'a>(
    label: &str,
    right: Option<Element<'a, AppMessage>>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let mut header = Row::new()
        .align_y(Center)
        .height(LINK_HEADER_H)
        .push(ui::section_header(label, tokens))
        .push(space::horizontal());
    if let Some(right) = right {
        header = header.push(right);
    }
    header.into()
}

/// Seasons: the latest three rows (the latest alone when `one_row`), and
/// "All N ›" in the heading when the show has more.
fn seasons_section<'a>(
    app: &'a App,
    entry: &'a Entry,
    one_row: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let seasons = entry.season_list();
    let keep = if one_row { 1 } else { 3 };
    let shown = &seasons[seasons.len().saturating_sub(keep)..];
    let mut rows = Column::new().spacing(ROW_GAP);
    for season in shown {
        rows = rows.push(seasons::season_row(app, entry, season, false, tokens));
    }
    let link = (seasons.len() > shown.len()).then(|| {
        ui::header_link(
            format!("All {}", seasons.len()),
            true,
            tokens,
            AppMessage::Library(library::Message::OpenPage(Page::Seasons {
                id: entry.id.clone(),
            })),
        )
    });
    column![header_with("Seasons", link, tokens), rows]
        .spacing(HEADER_GAP)
        .into()
}

fn rows_h(n: usize) -> f32 {
    if n == 0 {
        0.0
    } else {
        n as f32 * ROW_H + (n - 1) as f32 * ROW_GAP
    }
}

/// The series' parts, and the window of them the section lists.
fn series_rows<'a>(
    app: &'a App,
    entry: &'a Entry,
    part_of: &'a Series,
) -> (Vec<series::SeriesRow<'a>>, Vec<series::SeriesRow<'a>>) {
    let library = app.repo.library();
    let state = app.artwork.series_for(entry);
    let fetched = state.and_then(|s| match &s.loaded {
        crate::artwork::Loaded::Ready(parts) => Some(parts),
        _ => None,
    });
    let all = series::rows(
        library,
        entry.media,
        &part_of.name,
        Some(&entry.id),
        fetched,
    );
    let shown = series::window(&all);
    (all, shown)
}

/// Series: the series name, up to three parts around this one (this one
/// alone when `one_row`), and "All N ›" in the heading when the series
/// has more (else "2 of 3 in your library" there).
fn series_section<'a>(
    app: &'a App,
    entry: &'a Entry,
    part_of: &'a Series,
    one_row: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let (all, mut shown) = series_rows(app, entry, part_of);
    if one_row {
        let this = shown.iter().position(|r| r.is_this).unwrap_or(0);
        shown = vec![shown[this].clone()];
    }
    let owned = all.iter().filter(|r| r.owned()).count();
    let mut rows = Column::new().spacing(ROW_GAP).push(
        text(Series::display_name(&part_of.name))
            .size(13)
            .color(tokens.sub),
    );
    for part in &shown {
        rows = rows.push(seasons::series_row(app, part, entry.media, false, tokens));
    }
    let right: Element<'a, AppMessage> = if all.len() > shown.len() {
        ui::header_link(
            format!("All {}", all.len()),
            true,
            tokens,
            AppMessage::Library(library::Message::OpenSeries(library::OpenSeries {
                media: entry.media,
                name: part_of.name.clone(),
                highlight: Some(entry.id.clone()),
            })),
        )
    } else {
        text(format!("{owned} of {} in your library", all.len()))
            .size(12.5)
            .color(tokens.sub)
            .into()
    };
    column![header_with("Series", Some(right), tokens), rows]
        .spacing(HEADER_GAP)
        .into()
}

/// Tags: the entry's tags as chips in at most two rows with "+ Tag",
/// the ones that do not fit folded into a "+N" chip that opens every tag
/// in a popover over the section (`open`); its height.
fn tags_section<'a>(
    app: &'a App,
    entry: &'a Entry,
    inner: f32,
    open: bool,
    tokens: &Tokens,
) -> (Element<'a, AppMessage>, f32) {
    let library = app.repo.library();
    // A tag in several categories shows its roles here, merged.
    let chips: Vec<(String, Option<String>)> = entry
        .tags
        .iter()
        .map(|tag| {
            let roles = library.roles_on(entry, tag);
            let detail = (!roles.is_empty()).then(|| {
                let roles: Vec<&str> = roles.iter().map(|c| c.role_label()).collect();
                roles.join(", ")
            });
            (format!("#{tag}"), detail)
        })
        .collect();
    let chip_w = |(label, detail): &(String, Option<String>)| {
        ui::text_width(label, 13.0, font::REGULAR)
            + detail.as_ref().map_or(0.0, |d| {
                5.0 + ui::text_width(&format!("· {d}"), 12.5, font::REGULAR)
            })
            + 20.0
    };
    let widths: Vec<f32> = chips.iter().map(chip_w).collect();
    let add_w = ui::text_width("+ Tag", 13.0, font::REGULAR) + 22.0;
    let more_w = |rest: usize| ui::text_width(&format!("+{rest}"), 13.0, font::SEMIBOLD) + 20.0;
    let tail = |rest: usize| {
        let mut tail = Vec::new();
        if rest > 0 {
            tail.push(more_w(rest));
        }
        tail.push(add_w);
        tail
    };
    let keep = ui::fold(&widths, inner, CHIP_GAP, TAG_ROWS, tail);
    let mut laid: Vec<f32> = widths[..keep].to_vec();
    laid.extend(tail(chips.len() - keep));
    let rows = ui::wrapped_rows(&laid, inner, CHIP_GAP).max(1);

    let mut shown = Row::new().spacing(CHIP_GAP);
    for (label, detail) in &chips[..keep] {
        shown = shown.push(ui::tag_chip(label.clone(), detail.clone(), tokens));
    }
    if keep < chips.len() {
        let t = *tokens;
        shown = shown.push(
            button(
                text(format!("+{}", chips.len() - keep))
                    .size(13)
                    .font(font::SEMIBOLD)
                    .height(Fill)
                    .align_y(Center),
            )
            .height(CHIP_H)
            .padding([0, 10])
            .on_press(msg(Message::TogglePop(DetailsPop::Tags)))
            .style(move |_, status| button::Style {
                background: Some(Background::Color(
                    if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        t.raised
                    },
                )),
                text_color: t.ink,
                border: Border::default().rounded(13),
                ..button::Style::default()
            })
            .focusable(13.0, tokens),
        );
    }
    let shown = shown
        .push(ui::add_chip(
            "+ Tag",
            tokens,
            msg(Message::EditTags(entry.id.clone())),
        ))
        .wrap()
        .vertical_spacing(CHIP_GAP);
    let anchor = column![ui::section_header("Tags", tokens), shown].spacing(HEADER_GAP);

    let popup = open.then(|| {
        let mut all = Row::new().spacing(CHIP_GAP);
        for (label, detail) in &chips {
            all = all.push(ui::tag_chip(label.clone(), detail.clone(), tokens));
        }
        popover(
            "Tags",
            all.wrap().vertical_spacing(CHIP_GAP).into(),
            None,
            inner,
            tokens,
        )
    });
    let height = HEADER_H + HEADER_GAP + rows as f32 * CHIP_H + (rows - 1) as f32 * CHIP_GAP;
    (
        Popover::new(anchor, popup)
            .placement(Placement::Over(Edge::Top))
            .gap(POP_INSET)
            .on_dismiss(msg(Message::ClosePop))
            .into(),
        height,
    )
}

/// Note: the note cut to `lines` lines in a field, which opens the note
/// popover with the editor over the section; the note saves as it is
/// typed.
fn note_section<'a>(
    app: &'a App,
    entry: &'a Entry,
    inner: f32,
    lines: usize,
    pop: Option<&DetailsPop>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let t = *tokens;
    let open = matches!(pop, Some(DetailsPop::Note(id)) if *id == entry.id);
    let (shown, color) = if entry.note.is_empty() {
        ("Add a personal note…".to_owned(), tokens.sub)
    } else {
        let (shown, _) = ui::clamp(
            &entry.note,
            NOTE_SIZE,
            NOTE_LINE,
            font::ITALIC,
            inner - 24.0,
            lines,
        );
        (shown, tokens.text)
    };
    let field = button(
        text(shown)
            .size(NOTE_SIZE)
            .font(font::ITALIC)
            .line_height(NOTE_LINE)
            .color(color)
            .width(Fill),
    )
    .width(Fill)
    .padding(Padding::new(NOTE_PAD).left(12).right(12))
    .on_press(msg(Message::TogglePop(DetailsPop::Note(entry.id.clone()))))
    .style(move |_, _| button::Style {
        background: Some(Background::Color(t.field)),
        text_color: color,
        border: Border::default().rounded(8).width(1).color(t.line),
        ..button::Style::default()
    })
    .focusable(8.0, tokens);
    let anchor = column![ui::section_header("Note", tokens), field].spacing(HEADER_GAP);

    let popup = open.then(|| {
        let editor = text_editor(&app.note_editor)
            .placeholder("Add a personal note…")
            .on_action(|action| msg(Message::NoteEdit(action)))
            .height(180)
            .padding([10, 12])
            .size(14)
            .style(move |_, status| text_editor::Style {
                background: Background::Color(t.field),
                border: Border::default().rounded(8).width(1).color(match status {
                    text_editor::Status::Focused { .. } => t.ink,
                    _ => t.line,
                }),
                placeholder: t.sub,
                value: t.text,
                selection: t.selection,
            });
        popover(
            "Note",
            editor.into(),
            Some("Saved as you type. Esc or click outside to close."),
            inner,
            tokens,
        )
    });
    Popover::new(anchor, popup)
        .placement(Placement::Over(Edge::Bottom))
        .gap(POP_INSET)
        .on_dismiss(msg(Message::ClosePop))
        .into()
}

/// About: the description cut to `lines` lines (none leaves the header
/// alone), with More in the header when there is more to read; More opens
/// the full text in a popover over the section.
fn about_section<'a>(
    entry: &'a Entry,
    inner: f32,
    lines: usize,
    open: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let (shown, cut) = ui::clamp(
        &entry.description,
        ABOUT_SIZE,
        ABOUT_LINE,
        font::REGULAR,
        inner,
        lines,
    );
    let more = cut.then(|| {
        ui::header_link(
            "More",
            false,
            tokens,
            msg(Message::TogglePop(DetailsPop::About)),
        )
    });
    let mut anchor = Column::new()
        .spacing(HEADER_GAP)
        .push(header_with("About", more, tokens));
    if lines > 0 {
        anchor = anchor.push(
            text(shown)
                .size(ABOUT_SIZE)
                .color(tokens.sub)
                .line_height(ABOUT_LINE),
        );
    }
    let popup = open.then(|| {
        let body = ui::scroll(
            text(&entry.description)
                .size(ABOUT_SIZE)
                .line_height(1.6)
                .color(tokens.text),
        );
        popover(
            "About",
            container(body).max_height(ABOUT_POP_MAX - 76.0).into(),
            None,
            inner,
            tokens,
        )
    });
    Popover::new(anchor, popup)
        .placement(Placement::Over(Edge::Bottom))
        .gap(POP_INSET)
        .on_dismiss(msg(Message::ClosePop))
        .into()
}

/// In plans, on one row: the header, then the plans the entry is in as
/// chips (a click opens the plan) and "+" to add it to one ("+ Add to
/// plan" when it is in none), at the right; its height, with the chips
/// wrapping when they must.
fn plans_section<'a>(
    app: &'a App,
    entry: &'a Entry,
    inner: f32,
    tokens: &Tokens,
) -> (Element<'a, AppMessage>, f32) {
    let t = *tokens;
    let plans: Vec<&Plan> = app
        .repo
        .library()
        .plans
        .iter()
        .filter(|p| p.item_for(&entry.id).is_some())
        .collect();
    let add_label = if plans.is_empty() {
        "+ Add to plan"
    } else {
        "+"
    };
    let mut widths: Vec<f32> = plans
        .iter()
        .map(|p| ui::text_width(&p.title(), 13.0, font::REGULAR) + 22.0)
        .collect();
    widths.push(ui::text_width(add_label, 13.0, font::REGULAR) + 22.0);
    let header_w = ui::text_width("IN PLANS", theme::size::SECTION, font::BOLD) + 8.0;
    let rows = ui::wrapped_rows(&widths, inner - header_w, PLAN_CHIP_GAP).max(1);
    let mut chips = Row::new().spacing(PLAN_CHIP_GAP).width(Fill);
    for plan in plans {
        chips = chips.push(
            button(text(plan.title()).size(13).height(Fill).align_y(Center))
                .height(PLAN_CHIP_H)
                .padding([0, 11])
                .on_press(AppMessage::Plans(plans::Message::ShowPlan(plan.id.clone())))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(
                        if matches!(status, button::Status::Hovered) {
                            t.hover
                        } else {
                            t.raised
                        },
                    )),
                    text_color: t.text,
                    border: Border::default().rounded(14),
                    ..button::Style::default()
                })
                .focusable(14.0, tokens),
        );
    }
    chips = chips.push(ui::add_chip(
        add_label,
        tokens,
        msg(Message::AddToPlan(entry.id.clone())),
    ));
    let chips = chips
        .wrap()
        .vertical_spacing(PLAN_CHIP_GAP)
        .align_x(iced::alignment::Horizontal::Right);
    let height = (rows as f32 * PLAN_CHIP_H + (rows - 1) as f32 * PLAN_CHIP_GAP).max(30.0);
    (
        row![
            container(ui::section_header("In plans", tokens))
                .height(PLAN_CHIP_H)
                .align_y(Center),
            chips
        ]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Top)
        .into(),
        height,
    )
}

/// A popover over a section: the section's caps and an X, the body, and
/// a hint under it. Solid, as wide as the section and its insets.
fn popover<'a>(
    title: &str,
    body: Element<'a, AppMessage>,
    hint: Option<&'a str>,
    inner: f32,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let t = *tokens;
    let header = row![
        ui::section_header(title, tokens),
        space::horizontal(),
        ui::icon_button(Icon::Close, 15.0, tokens, msg(Message::ClosePop)),
    ]
    .align_y(Center)
    .height(28);
    let mut column = Column::new().spacing(10).push(header).push(body);
    if let Some(hint) = hint {
        column = column.push(text(hint).size(12).color(tokens.sub));
    }
    container(column)
        .width(inner + 2.0 * POP_INSET)
        .padding(Padding::new(14.0).top(12))
        .style(move |_| ui::panel_style(&t, 12.0))
        .into()
}

/// The entry in the open plan: its When (a day or a period, on a
/// timeline plan) and the plan's own note for it.
fn in_plan<'a>(
    app: &'a App,
    plan: &'a Plan,
    entry: &'a Entry,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let Some(item) = plan.item_for(&entry.id) else {
        return text("Not in this plan").size(14).color(tokens.sub).into();
    };
    let mut column = Column::new().spacing(10);
    if plan.timeline {
        let open = app.plans.when_open;
        let (label, color) = match item.when {
            Some(when) => (crate::plans::format_when(when, plan.year), tokens.text),
            None => ("Whenever".to_owned(), tokens.sub),
        };
        let tokens_copy = *tokens;
        let field = button(
            row![
                ui::icon(Icon::Calendar, 15.0, tokens.sub),
                text(label).size(14).color(color).width(Fill),
            ]
            .spacing(8)
            .align_y(iced::Center)
            .height(Fill),
        )
        .width(Fill)
        .height(36)
        .padding([0, 10])
        .on_press(AppMessage::Plans(plans::Message::ToggleWhen))
        .style(move |_, _| button::Style {
            background: Some(Background::Color(tokens_copy.field)),
            border: Border::default()
                .rounded(8)
                .width(1)
                .color(tokens_copy.line),
            ..button::Style::default()
        })
        .focusable(8.0, tokens);
        let calendar = open.then(|| {
            let picked = ui::calendar::calendar(
                plans::shown_month(app),
                item.when,
                app.today,
                tokens.accent,
                tokens.on_accent,
                |day| AppMessage::Plans(plans::Message::PickDay(day)),
                |by| AppMessage::Plans(plans::Message::Month(by)),
                tokens,
            );
            let hint = if app.plans.anchor.is_some() {
                "Click a later day for a period."
            } else {
                "Click a day, then a later one for a period."
            };
            let footer = row![
                text(hint).size(12).color(tokens_copy.sub).width(Fill),
                ui::labeled_button(
                    "Clear",
                    ButtonKind::Text,
                    tokens_copy,
                    item.when
                        .is_some()
                        .then_some(AppMessage::Plans(plans::Message::ClearWhen))
                ),
            ]
            .spacing(8)
            .align_y(iced::Center);
            container(column![picked, footer].spacing(6).width(252))
                .padding(12)
                .style(move |_| ui::panel_style(&tokens_copy, 12.0))
                .into()
        });
        let when = Popover::new(field, calendar)
            .align(Align::Start)
            .on_dismiss(AppMessage::Plans(plans::Message::ToggleWhen));
        column = column.push(labeled("When", when.into(), tokens));
    }
    // Typed here, like the entry's note: Enter saves, Esc restores, moving
    // on saves.
    let draft = app
        .plans
        .item_note_draft
        .as_ref()
        .filter(|(for_plan, for_entry, _)| *for_plan == plan.id && *for_entry == entry.id)
        .map(|(_, _, note)| note.as_str());
    let field = ui::text_field(
        "A note for this plan",
        draft.unwrap_or(&item.note),
        |note| AppMessage::Plans(plans::Message::ItemNote(note)),
        font::REGULAR,
        tokens,
    )
    .on_submit(AppMessage::Plans(plans::Message::SaveItemNote));
    let mut note = Column::new().spacing(5).push(field);
    if draft.is_some_and(|draft| draft.trim() != item.note) {
        note = note.push(
            text("Enter saves · Esc puts the saved note back")
                .size(12)
                .color(tokens.sub),
        );
    }
    column
        .push(labeled("Plan note", note.into(), tokens))
        .into()
}

/// A small label over a control.
fn labeled<'a>(
    label: &'a str,
    control: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![text(label).size(12.5).color(tokens.sub), control]
        .spacing(5)
        .into()
}

/// An album's tracks: the track list with a rating per track, rated
/// tracks the list lacks after it, in `height` px; a list taller than
/// that scrolls within it, so the pane itself never does. Only rated
/// tracks are stored.
fn tracks<'a>(
    app: &'a App,
    album: &'a Entry,
    height: f32,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let rated = album.tracks.as_deref().unwrap_or_default();
    // A stored rating is matched to a listed track by its title with case,
    // punctuation and edition notes set aside.
    let rating_of = |title: &str| {
        rated
            .iter()
            .find(|t| {
                crate::lookup::text::track_title_key(&t.title)
                    == crate::lookup::text::track_title_key(title)
            })
            .map(|t| t.rating)
    };
    use crate::artwork::Loaded;
    use crate::lookup::FailureKind;
    let state = app.artwork.tracks_for(album);
    let key = crate::lookup::text::track_title_key;
    let listed: Vec<&str> = match state.map(|s| &s.loaded) {
        Some(Loaded::Ready(tracks)) => tracks.iter().map(|t| t.title.as_str()).collect(),
        _ => Vec::new(),
    };
    let extra = rated
        .iter()
        .filter(|t| !listed.iter().any(|l| key(l) == key(&t.title)))
        .map(|t| t.title.as_str());
    let open = app.library.track_rating_open.as_deref();

    let mut rows = Column::new();
    for (index, title) in listed.iter().copied().chain(extra).enumerate() {
        let number = if index < listed.len() {
            (index + 1).to_string()
        } else {
            String::new()
        };
        let picker = ui::track_rating(
            rating_of(title),
            open == Some(title),
            msg(Message::ToggleTrackRating(title.to_owned())),
            move |rating| {
                msg(Message::RateTrack {
                    album: album.id.clone(),
                    title: title.to_owned(),
                    rating,
                })
            },
            tokens,
        );
        rows = rows.push(
            row![
                container(ui::mono(number, 12.0, tokens.sub)).width(22),
                container(text(title).size(14).wrapping(text::Wrapping::None))
                    .width(Fill)
                    .clip(true),
                picker,
            ]
            .spacing(8)
            .align_y(iced::Center)
            .height(TRACK_H),
        );
    }
    let note = match state.map(|s| &s.loaded) {
        None | Some(Loaded::Loading) => Some("Loading the track list…"),
        Some(Loaded::Failed(FailureKind::Offline)) => {
            Some("The full track list needs a connection.")
        }
        Some(Loaded::Failed(_)) | Some(Loaded::None) => {
            Some("No track list was found for this album.")
        }
        Some(Loaded::Ready(_)) if rated.is_empty() => Some("No tracks rated yet."),
        Some(Loaded::Ready(_)) => None,
    };
    let mut body = Column::new().spacing(6);
    if let Some(note) = note {
        body = body.push(text(note).size(13).color(tokens.sub));
    }
    body = body.push(rows);
    let header = header_with(
        "Tracks",
        Some(
            text(format!("{} rated", rated.len()))
                .size(12.5)
                .color(tokens.sub)
                .into(),
        ),
        tokens,
    );
    column![header, ui::scroll(body).height(height)]
        .spacing(6)
        .into()
}

fn section<'a>(
    label: &str,
    body: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![ui::section_header(label, tokens), body]
        .spacing(HEADER_GAP)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_steps_down_to_fit_beside_the_cover_and_is_cut_at_the_smallest() {
        // The facts column of the 352 px pane: 352 - 44 - 96 - 16.
        let width = INNER - COVER_W - COVER_GAP;
        // The kicker, a creator and a date leave the title about 75 px.
        let budget = COVER_H - (2.0 + 11.0 * 1.3 + 6.0) - (14.0 * 1.3 + 6.0) - (12.5 * 1.3 + 6.0);
        let (short, size, height) = fit_title("Dune", width, budget);
        assert_eq!((short.as_str(), size, height), ("Dune", 24.0, 24.0 * 1.1));
        let long = "Donkey Kong Country 2: Diddy's Kong Quest";
        let (kept, size, height) = fit_title(long, width, budget);
        assert_eq!(kept, long);
        assert!(size < 24.0, "{size}");
        assert!(height <= budget, "{height} > {budget}");
        let endless = "The Hitchhiker's Guide to the Galaxy and Other Stories Told at Great Length";
        let (cut, size, height) = fit_title(endless, width, budget);
        assert_eq!(size, 18.0);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(height <= budget, "{height} > {budget}");
        // Too little room for even one line still keeps one.
        let (one, _, height) = fit_title(endless, width, 10.0);
        assert!(one.ends_with('…'));
        assert_eq!(height, 18.0 * 1.1);
    }

    #[test]
    fn about_gives_way_first_then_the_note_and_about_folds_only_under_one_line() {
        // Plenty of room: About caps at ten lines, the note keeps two.
        let roomy = fit(300.0, 900.0, true, false, 0.0);
        assert_eq!((roomy.about_lines, roomy.note_lines), (10, 2));
        // Room for about four lines of About beside a two-line note.
        let some = fit(
            300.0,
            300.0 + note_field_h(2) + 4.5 * about_line_px(),
            true,
            false,
            0.0,
        );
        assert_eq!((some.about_lines, some.note_lines), (4, 2));
        // Under two lines of About, the note goes to one line first.
        let tight = fit(
            300.0,
            300.0 + note_field_h(1) + 2.2 * about_line_px(),
            true,
            false,
            0.0,
        );
        assert_eq!((tight.about_lines, tight.note_lines), (2, 1));
        // One line of About beats the note's second line.
        let one_line = fit(
            300.0,
            300.0 + note_field_h(1) + 1.2 * about_line_px(),
            true,
            false,
            0.0,
        );
        assert_eq!((one_line.about_lines, one_line.note_lines), (1, 1));
        // Under one line About keeps only its header, and the note gets
        // its second line back when it fits.
        let folded = fit(300.0, 300.0 + note_field_h(2) + 1.0, true, false, 0.0);
        assert_eq!((folded.about_lines, folded.note_lines), (0, 2));
        let tighter = fit(300.0, 300.0 + note_field_h(1) + 1.0, true, false, 0.0);
        assert_eq!((tighter.about_lines, tighter.note_lines), (0, 1));
        assert!(!tighter.one_row && !tighter.scrolls);
        // Without a description the note alone gives way.
        let plain = fit(300.0, 300.0 + note_field_h(2) - 1.0, false, false, 0.0);
        assert_eq!((plain.about_lines, plain.note_lines), (0, 1));
    }

    #[test]
    fn past_the_shrink_order_the_list_keeps_one_row_and_then_the_pane_scrolls() {
        // Too tall even with About folded: the list goes to one row, and
        // the room that frees goes back to About.
        let one_row = fit(
            400.0,
            300.0 + note_field_h(2) + 2.0 * about_line_px(),
            true,
            false,
            104.0,
        );
        assert!(one_row.one_row && !one_row.scrolls);
        assert_eq!((one_row.about_lines, one_row.note_lines), (2, 2));
        // Still too tall with one row: the pane scrolls, at its shortest.
        let scrolls = fit(600.0, 300.0, true, false, 104.0);
        assert!(scrolls.one_row && scrolls.scrolls);
        assert_eq!((scrolls.about_lines, scrolls.note_lines), (0, 1));
        // Nothing to cut: it scrolls outright.
        assert!(fit(600.0, 300.0, false, false, 0.0).scrolls);
    }

    #[test]
    fn an_albums_tracks_take_the_rest_and_keep_three_rows() {
        let roomy = fit(300.0, 900.0, false, true, 0.0);
        assert_eq!(roomy.about_lines, 0);
        assert_eq!(roomy.note_lines, 2);
        assert!((roomy.tracks_height - (600.0 - note_field_h(2))).abs() < 0.01);
        let tight = fit(300.0, 320.0, false, true, 0.0);
        assert_eq!(tight.tracks_height, TRACK_MIN_ROWS * TRACK_H);
    }
}
