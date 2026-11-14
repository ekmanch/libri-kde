//! The All tab's panel with nothing selected (2026-11-14): What's left
//! (the plan's items not done), By type (the plan's progress per type)
//! and Pick something (a tagged Want title not in the plan). It never
//! scrolls: the sections are fitted to the panel's height (`fit`), and
//! "See all N ›" opens the full list in a popover over the panel. Nothing
//! in it is by date, since `library.json` does not say when an item was
//! done.

use iced::widget::{
    Column, Row, button, column, container, progress_bar, responsive, row, space, stack, text,
};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{GlancePop, Message};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::{Entry, EntryStatus, Plan, PlanItem};
use crate::plans::{self as rules, TypeProgress};
use crate::screens::{entry, library};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt};

fn msg(message: Message) -> AppMessage {
    AppMessage::Plans(message)
}

/// What's left shows three rows at most, and By type shrinks to three.
const LEFT_MAX: usize = 3;
const TYPES_MIN: usize = 3;
/// The measures: the gap between sections, a section's header row and the
/// space under it, a row of What's left without and with a cover, a row
/// of By type, and Pick something's card under its header.
const SECTION_GAP: f32 = 14.0;
const HEADER_H: f32 = 24.0;
const HEADER_GAP: f32 = 4.0;
const TEXT_ROW_H: f32 = 46.0;
const COVER_ROW_H: f32 = 52.0;
const TYPE_ROW_H: f32 = 30.0;
const PICK_GAP: f32 = 8.0;
const PICK_CARD_H: f32 = 90.0;
/// The pane's padding under the sections (`entry::details::pane`).
const PANE_BOTTOM: f32 = 18.0;
/// A popover's inset from the panel's sides and bottom.
const POP_INSET: f32 = 10.0;

/// What the panel shows for the height it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    /// Rows of What's left; 0 leaves the section out.
    pub left: usize,
    /// Rows of By type.
    pub types: usize,
    pub pick: bool,
}

/// The rows for `avail` px, with What's left rows `row_h` tall, `left`
/// items left, `types` types and Pick something `pick_h` tall when there
/// is a title to pick. Everything shows when it fits. When it does not,
/// Pick something goes first, whole; then By type shrinks to three rows;
/// What's left drops below three only once By type is at three, and
/// keeps one row.
pub fn fit(avail: f32, row_h: f32, left: usize, types: usize, pick_h: Option<f32>) -> Fit {
    let need = |fit: Fit| {
        let mut parts = Vec::new();
        if fit.left > 0 {
            parts.push(HEADER_H + HEADER_GAP + fit.left as f32 * row_h);
        }
        if fit.types > 0 {
            parts.push(HEADER_H + HEADER_GAP + fit.types as f32 * TYPE_ROW_H);
        }
        if let (true, Some(pick_h)) = (fit.pick, pick_h) {
            parts.push(pick_h);
        }
        parts.iter().sum::<f32>() + parts.len().saturating_sub(1) as f32 * SECTION_GAP
    };
    let mut fit = Fit {
        left: left.min(LEFT_MAX),
        types,
        pick: pick_h.is_some(),
    };
    if need(fit) <= avail {
        return fit;
    }
    fit.pick = false;
    while need(fit) > avail && fit.types > TYPES_MIN {
        fit.types -= 1;
    }
    while need(fit) > avail && fit.left > 1 {
        fit.left -= 1;
    }
    fit
}

/// Pick something's title: a Want title with the plan's tags that is out
/// and not in the plan, moved on by "Another".
pub fn pick<'a>(app: &'a App, plan: &Plan) -> Option<&'a Entry> {
    let wants: Vec<&Entry> = rules::collection(plan, app.repo.library())
        .into_iter()
        .filter(|e| {
            plan.item_for(&e.id).is_none()
                && e.status == EntryStatus::Want
                && e.is_released_on(app.today)
        })
        .collect();
    if wants.is_empty() {
        return None;
    }
    Some(wants[library::spread(app.plans.pick, wants.len())])
}

/// The panel, built for its height. `tokens` carry the plan's accent.
pub fn view<'a>(app: &'a App, plan: &'a Plan, tokens: Tokens) -> Element<'a, AppMessage> {
    responsive(move |size| build(app, plan, &tokens, size.height)).into()
}

fn build<'a>(
    app: &'a App,
    plan: &'a Plan,
    tokens: &Tokens,
    height: f32,
) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let covers = app.plans.style(&plan.id, tokens.light).covers;
    let row_h = if covers { COVER_ROW_H } else { TEXT_ROW_H };
    let left: Vec<(&PlanItem, &Entry)> = rules::left(plan)
        .filter_map(|item| library.entry(&item.entry).map(|e| (item, e)))
        .collect();
    let types = rules::by_type(plan, library);
    let picked = pick(app, plan);
    let pick_h = picked.map(|_| HEADER_H + PICK_GAP + PICK_CARD_H);
    let fit = fit(
        height - entry::details::PAD - PANE_BOTTOM,
        row_h,
        left.len(),
        types.len(),
        pick_h,
    );

    let header = |label: &str, right: Element<'a, AppMessage>| {
        row![
            ui::section_header(label, tokens),
            space::horizontal(),
            right
        ]
        .align_y(Center)
        .height(HEADER_H)
    };
    // "See all N ›" when the section is capped, else the plain count.
    let count = |total: usize, shown: usize, pop: GlancePop| -> Element<'a, AppMessage> {
        if total > shown {
            ui::header_link(
                format!("See all {total}"),
                true,
                tokens,
                msg(Message::ToggleGlancePop(pop)),
            )
        } else {
            ui::mono(total.to_string(), 12.0, tokens.sub).into()
        }
    };
    let left_rows = |n: usize| {
        Column::with_children(
            left.iter()
                .take(n)
                .map(|&(item, entry)| left_row(app, item, entry, covers, tokens)),
        )
    };
    let type_rows =
        |n: usize| Column::with_children(types.iter().take(n).map(|row| type_row(*row, tokens)));

    let mut sections = Column::new().spacing(SECTION_GAP);
    // Where the By type popover starts: under that section's header.
    let mut types_top = entry::details::PAD + HEADER_H + HEADER_GAP;
    if fit.left > 0 {
        sections = sections.push(
            column![
                header("What's left", count(left.len(), fit.left, GlancePop::Left)),
                left_rows(fit.left)
            ]
            .spacing(HEADER_GAP),
        );
        types_top += HEADER_H + HEADER_GAP + fit.left as f32 * row_h + SECTION_GAP;
    }
    if fit.types > 0 {
        let right = if types.len() > fit.types {
            count(types.len(), fit.types, GlancePop::Types)
        } else {
            space().into()
        };
        sections = sections
            .push(column![header("By type", right), type_rows(fit.types)].spacing(HEADER_GAP));
    }
    if fit.pick
        && let Some(entry) = picked
    {
        let another = ui::header_link("Another", false, tokens, msg(Message::AnotherPick));
        sections = sections.push(
            column![
                header("Pick something", another),
                pick_card(app, entry, tokens)
            ]
            .spacing(PICK_GAP),
        );
    }
    let pane = entry::details::pane(sections.into(), None);

    let (title, total, rows, top) = match app.plans.glance_pop {
        Some(GlancePop::Left) => (
            "What's left",
            left.len(),
            left_rows(left.len()),
            entry::details::PAD + HEADER_H + HEADER_GAP,
        ),
        Some(GlancePop::Types) => ("By type", types.len(), type_rows(types.len()), types_top),
        None => return pane,
    };
    let list = column![
        header(title, ui::mono(total.to_string(), 12.0, tokens.sub).into()),
        ui::scroll(rows).height(iced::Shrink)
    ]
    .spacing(HEADER_GAP);
    // A panel too short for the popover where it belongs moves it up.
    let top = top.min((height - 160.0).max(POP_INSET));
    let t = *tokens;
    let dialog = container(list)
        .padding(Padding::new(14.0).top(8).bottom(10))
        .max_height(height - top - POP_INSET)
        .width(Fill)
        .style(move |_| ui::panel_style(&t, 12.0));
    let outside = button(space().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .padding(0)
        .on_press(msg(Message::CloseGlancePop))
        .style(|_, _| button::Style::default());
    stack![
        pane,
        outside,
        container(dialog)
            .width(Fill)
            .padding(Padding::new(POP_INSET).top(top))
    ]
    .into()
}

/// A row of What's left: an optional cover, the type's dot, the title
/// over "Movie · Want", and the open ring of an item not done. A click
/// opens the title's details.
fn left_row<'a>(
    app: &'a App,
    item: &PlanItem,
    entry: &'a Entry,
    covers: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let state = if item.repeat {
        rules::repeat_label(entry.media)
    } else if entry.status == EntryStatus::InProgress {
        "in progress"
    } else {
        browse::status_label(Some(entry.media), entry.status)
    };
    let sub = format!("{} · {state}", browse::kind_label(entry));
    let mut content = Row::new().spacing(10).align_y(Center);
    if covers {
        content = content.push(ui::cover::thumb(
            app.artwork.cover(entry),
            theme::cover_color(&entry.id),
            28.0,
            entry.media,
            4.0,
        ));
    }
    let ring = tokens.sub;
    content = content
        .push(ui::dot(tokens.type_color(entry.media), 8.0, 3.0))
        .push(
            column![
                text(&entry.title)
                    .size(14)
                    .font(font::SEMIBOLD)
                    .wrapping(text::Wrapping::None),
                text(sub).size(12).color(tokens.sub),
            ]
            .width(Fill)
            .clip(true),
        )
        .push(
            container(space())
                .width(18)
                .height(18)
                .style(move |_| container::Style {
                    border: Border::default().rounded(9).width(2).color(ring),
                    ..container::Style::default()
                }),
        );
    let height = if covers { COVER_ROW_H } else { TEXT_ROW_H };
    column![
        button(content.height(Fill))
            .width(Fill)
            .height(height - 1.0)
            .padding([0, 4])
            .on_press(msg(Message::Select(entry.id.clone())))
            .style(ui::row_style(tokens, false, 0.0))
            .focusable(0.0, tokens),
        ui::rule(tokens.line.scale_alpha(0.5)),
    ]
    .into()
}

/// A row of By type: the type, its bar in the type's color, and "2/5".
fn type_row<'a>(progress: TypeProgress, tokens: &Tokens) -> Element<'a, AppMessage> {
    let (track, color) = (tokens.track, tokens.type_color(progress.media));
    let bar = progress_bar(0.0..=progress.total as f32, progress.done as f32)
        .girth(6)
        .style(move |_| progress_bar::Style {
            background: Background::Color(track),
            bar: Background::Color(color),
            border: Border::default().rounded(3),
        });
    row![
        text(progress.media.label()).size(14).width(64),
        bar,
        container(ui::mono(
            format!("{}/{}", progress.done, progress.total),
            12.0,
            tokens.sub
        ))
        .width(34)
        .align_x(iced::Right),
    ]
    .spacing(10)
    .align_y(Center)
    .height(TYPE_ROW_H)
    .into()
}

/// Pick something's card: the cover, the title over "Movie · 1982 · Want"
/// and "+ Add to plan". A click on the card opens the title's details.
fn pick_card<'a>(app: &'a App, entry: &'a Entry, tokens: &Tokens) -> Element<'a, AppMessage> {
    let t = *tokens;
    let mut sub = browse::kind_label(entry).to_owned();
    if let Some(year) = entry.year() {
        sub.push_str(&format!(" · {year}"));
    }
    sub.push_str(" · Want");
    let words = column![
        text(&entry.title)
            .size(15)
            .font(font::SEMIBOLD)
            .wrapping(text::Wrapping::None),
        text(sub).size(12.5).color(t.sub),
        container(ui::sized_button(
            "+ Add to plan",
            ButtonKind::Outline,
            28.0,
            t,
            Some(msg(Message::AddItem(entry.id.clone()))),
        ))
        .padding(Padding::new(0.0).top(6)),
    ]
    .spacing(2)
    .width(Fill)
    .clip(true);
    let cover = ui::cover::thumb(
        app.artwork.cover(entry),
        theme::cover_color(&entry.id),
        44.0,
        entry.media,
        5.0,
    );
    button(row![cover, words].spacing(12).align_y(Center).height(Fill))
        .width(Fill)
        .height(PICK_CARD_H)
        .padding(12)
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(move |_, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, button::Status::Hovered) {
                    theme::mix_white(t.raised, if t.light { 0.0 } else { 0.04 })
                } else {
                    t.raised
                },
            )),
            border: Border::default().rounded(12),
            text_color: t.text,
            ..button::Style::default()
        })
        .focusable(12.0, tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICK: Option<f32> = Some(HEADER_H + PICK_GAP + PICK_CARD_H);

    fn rows(avail: f32, left: usize, types: usize) -> (usize, usize, bool) {
        let fit = fit(avail, TEXT_ROW_H, left, types, PICK);
        (fit.left, fit.types, fit.pick)
    }

    /// The drop order of the canvas's note: Pick something goes first,
    /// whole; By type shrinks to three rows; What's left only then.
    #[test]
    fn the_sections_drop_in_order_as_the_height_runs_out() {
        // Tall: three of the six left, every type, the pick.
        assert_eq!(rows(900.0, 6, 6), (3, 6, true));
        // Without the pick everything else still fits.
        let all = (HEADER_H + HEADER_GAP + 3.0 * TEXT_ROW_H)
            + SECTION_GAP
            + (HEADER_H + HEADER_GAP + 6.0 * TYPE_ROW_H);
        assert_eq!(rows(all, 6, 6), (3, 6, false));
        // By type gives rows up before What's left does.
        assert_eq!(rows(all - 1.0, 6, 6), (3, 5, false));
        assert_eq!(rows(all - 3.0 * TYPE_ROW_H, 6, 6), (3, 3, false));
        // Only at three types does What's left drop, and it keeps a row.
        assert_eq!(rows(all - 3.0 * TYPE_ROW_H - 1.0, 6, 6), (2, 3, false));
        assert_eq!(rows(0.0, 6, 6), (1, 3, false));
    }

    #[test]
    fn a_finished_plan_has_no_whats_left_and_few_types_are_never_capped() {
        assert_eq!(rows(900.0, 0, 2), (0, 2, true));
        assert_eq!(rows(0.0, 0, 2), (0, 2, false));
        assert_eq!(rows(900.0, 2, 2), (2, 2, true));
    }
}
