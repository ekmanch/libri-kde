//! Widgets shared by every screen. Anything that repeats across screens
//! (rows, chips, buttons, search field) lives here, never copied.

pub mod calendar;
pub mod chip;
pub mod context;
pub mod cover;
pub mod dialog;
pub mod empty;
pub mod entry;
pub mod field;
pub mod focus;
pub mod groups;
pub mod icon;
pub mod measure;
pub mod menu;
pub mod music;
pub mod picture;
pub mod popover;
pub mod rating;
pub mod search;
pub mod season;
pub mod seasonal;
pub mod section_rail;
pub mod segmented;
pub mod setting_rows;
pub mod slide;
pub mod spinner;
pub mod switch;
pub mod tag_group;
pub mod tag_picker;
pub mod toolbar;
pub mod type_rail;

use iced::alignment::Vertical;
use iced::widget::{self, Text, button, container, scrollable, text};
use iced::{Background, Border, Color, Element, Fill, Padding, Shadow, Vector};

use crate::theme::{Tokens, font, size};

pub use chip::{add_chip, filter_chip, removable_chip, status_chip, suggested_chip, tag_chip};
pub use cover::{cover, flat_cover, plain_cover, titled_cover};
pub use dialog::{dialog, modal, panel, pick_option};
pub use empty::empty_state;
pub use entry::{
    EntryLook, entry_card, entry_list_row, entry_table_header, entry_table_row, new_season_badge,
    rule,
};
pub use field::{labeled, text_field};
pub use focus::{Barrier, FocusExt, Focusable};
pub use groups::{Groups, Measure};
pub use icon::{Icon, icon, marker};
pub use measure::{clamp, fold, line_count, one_line, text_width, wrapped_rows};
pub use menu::radio_menu;
pub use picture::picture;
pub use popover::{Align, Edge, Placement, Popover};
pub use rating::{rating, track_rating};
pub use search::search_field;
pub use season::{season_pill, status_pill};
pub use section_rail::{RailLabel, SectionRail};
pub use segmented::segmented;
pub use setting_rows::{setting_row, setting_rows};
pub use slide::{scrolled, slide, slide_by};
pub use spinner::spinner;
pub use switch::{setting_switch, switch};
pub use tag_group::tag_group;
pub use tag_picker::Picker;
pub use toolbar::{add_button, icon_switch, menu_button};
pub use type_rail::TypeRail;

/// A section header in small gray caps.
pub fn section_header<'a, Message>(label: &str, tokens: &Tokens) -> Element<'a, Message> {
    text(label.to_uppercase())
        .size(size::SECTION)
        .font(font::BOLD)
        .color(tokens.sub)
        .into()
}

/// A link at the right end of a section header, in the ink with a
/// small chevron: "See all 12 ›", "All 29 ›", "Open ›", "More".
pub fn header_link<'a, Message: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    chevron: bool,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    let ink = tokens.ink;
    let mut content = widget::Row::new()
        .spacing(4)
        .align_y(Vertical::Center)
        .height(Fill)
        .push(text(label).size(13).font(font::SEMIBOLD));
    if chevron {
        content = content.push(icon(Icon::ChevronRight, 12.0, ink));
    }
    button(content)
        .height(24)
        .padding([0, 2])
        .on_press(on_press)
        .style(move |_, _| button::Style {
            text_color: ink,
            ..button::Style::default()
        })
        .focusable(6.0, tokens)
}

/// A screen or dialog title.
pub fn heading<'a, Message>(
    label: impl text::IntoFragment<'a>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    text(label)
        .size(size::HEADING)
        .font(font::EXTRABOLD)
        .color(tokens.text)
        .into()
}

/// Years, counts, dates, ratings and lengths, in Figtree. Its figures are
/// proportional: iced 0.14's `Font` has no OpenType features, so Figtree's
/// `tnum` cannot be turned on.
pub fn figure<'a>(value: impl text::IntoFragment<'a>, text_size: f32, color: Color) -> Text<'a> {
    text(value).size(text_size).font(font::REGULAR).color(color)
}

/// The small uppercase label over a title in the details pane and the
/// cover panes ("UPCOMING · MOVIE"), in SemiBold; the caller uppercases it.
pub fn kicker<'a>(value: impl text::IntoFragment<'a>, text_size: f32, color: Color) -> Text<'a> {
    text(value)
        .size(text_size)
        .font(font::SEMIBOLD)
        .color(color)
}

/// Code and identifiers: a repository, a branch, a file path or name, a
/// keyboard key.
pub fn mono<'a>(value: impl text::IntoFragment<'a>, text_size: f32, color: Color) -> Text<'a> {
    text(value).size(text_size).font(font::MONO).color(color)
}

/// The Libri logo (the book) at `width` px; it is 7:5.
pub fn logo<'a, Message>(width: f32) -> Element<'a, Message> {
    let bytes: &'static [u8] = include_bytes!("../../assets/logos/libri/svg/libri_book_rect.svg");
    widget::svg(widget::svg::Handle::from_memory(bytes))
        .width(width)
        .height(width * 5.0 / 7.0)
        .into()
}

/// A small colored square or circle: type dots, the sync dot, swatches.
pub fn dot<'a, Message: 'a>(color: Color, diameter: f32, radius: f32) -> Element<'a, Message> {
    container(widget::space())
        .width(diameter)
        .height(diameter)
        .style(move |_| container::background(color).border(Border::default().rounded(radius)))
        .into()
}

/// The surface of popovers and dialogs.
pub fn panel_style(tokens: &Tokens, radius: f32) -> container::Style {
    container::Style {
        text_color: Some(tokens.text),
        background: Some(Background::Color(tokens.popover)),
        border: Border::default().rounded(radius),
        shadow: Shadow {
            color: tokens.shadow,
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

/// A hover tooltip on `content`: `label` on the popover surface, by the
/// pointer after half a second (the canvas's `title`).
pub fn tooltip<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    label: String,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let tokens = *tokens;
    iced::widget::tooltip(
        content,
        text(label).size(12.5),
        iced::widget::tooltip::Position::FollowCursor,
    )
    .padding(8)
    .gap(12)
    .delay(std::time::Duration::from_millis(500))
    .style(move |_| panel_style(&tokens, 6.0))
    .into()
}

/// The mockup's thin scrollbar.
pub fn scrollbar() -> scrollable::Scrollbar {
    scrollable::Scrollbar::new()
        .width(4)
        .scroller_width(4)
        .margin(3)
}

/// A vertical scroll area with the mockup's thin gray scroller.
pub fn scroll<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> scrollable::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(scrollbar()))
        .style(|theme, status| {
            let mut style = scrollable::default(theme, status);
            let thumb = Color::from_rgba(0.5, 0.5, 0.5, 0.3);
            style.vertical_rail.background = None;
            style.vertical_rail.border = Border::default();
            style.vertical_rail.scroller.background = Background::Color(thumb);
            style.vertical_rail.scroller.border = Border::default().rounded(2);
            style
        })
}

/// A borderless button for rows and list items: the selection gray when
/// `selected`, a light hover otherwise.
pub fn row_style(
    tokens: &Tokens,
    selected: bool,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    row_fills(
        tokens,
        selected.then_some(tokens.selection),
        tokens.hover,
        radius,
    )
}

/// A row of the sidebar: the picked fill when `selected` (the selection
/// gray on the dark themes, a pale tint of the accent on Light). Light's
/// two-tone chrome is the hover gray itself, so the hover there is the
/// selection gray.
pub fn sidebar_row_style(
    tokens: &Tokens,
    selected: bool,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let hover = if tokens.hover == tokens.chrome {
        tokens.selection
    } else {
        tokens.hover
    };
    row_fills(
        tokens,
        selected.then_some(tokens.selection_tint),
        hover,
        radius,
    )
}

/// The sidebar's Settings gear: marked with the selection gray in every
/// theme while Settings is open, never Light's tint of the accent, which
/// is for a picked row.
pub fn sidebar_gear_style(
    tokens: &Tokens,
    selected: bool,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let hover = if tokens.hover == tokens.chrome {
        tokens.selection
    } else {
        tokens.hover
    };
    row_fills(tokens, selected.then_some(tokens.selection), hover, radius)
}

/// A row of a list of pages inside a dialog (the Following window's
/// kinds): selected as the sidebar's row is, with the plain hover, since
/// a dialog is never the chrome.
pub fn nav_row_style(
    tokens: &Tokens,
    selected: bool,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    row_fills(
        tokens,
        selected.then_some(tokens.selection_tint),
        tokens.hover,
        radius,
    )
}

/// A check box that shows whether its row is ticked; the row takes the
/// click. `size` square with `radius` corners and a `mark` px check.
/// Ticked, it is a main button's fill with the dark check: the accent on
/// the dark themes, and on Light the pale tint with a stronger tint as
/// its edge.
pub fn tick_box<'a, Message: 'a>(
    on: bool,
    size: f32,
    radius: f32,
    mark: f32,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let (fill, line) = if on {
        (Some(tokens.tint), tokens.tick_line())
    } else {
        (None, tokens.sub)
    };
    let check: Element<'a, Message> = if on {
        icon(Icon::Check, mark, tokens.on_accent)
    } else {
        widget::space().into()
    };
    container(check)
        .center(size)
        .style(move |_| container::Style {
            background: fill.map(Background::Color),
            border: Border::default().rounded(radius).width(1.5).color(line),
            ..container::Style::default()
        })
        .into()
}

fn row_fills(
    tokens: &Tokens,
    selected: Option<Color>,
    hover: Color,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let tokens = *tokens;
    move |_, status| {
        let background = match status {
            _ if selected.is_some() => selected,
            button::Status::Hovered | button::Status::Pressed => Some(hover),
            _ => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: tokens.text,
            border: Border::default().rounded(radius),
            ..button::Style::default()
        }
    }
}

/// A sidebar type's checkbox: a 15 px box in the type's color, filled
/// with a check when the type is picked.
pub fn type_checkbox<'a, Message: Clone + 'a>(
    color: Color,
    on: bool,
    tokens: &Tokens,
    on_toggle: Message,
) -> Element<'a, Message> {
    // The check reads on the type color: dark on the dark themes, white on
    // Light's darker inks. Solid even on Glass, where the chrome is not.
    let mark = if tokens.light {
        Color::WHITE
    } else {
        Color {
            a: 1.0,
            ..tokens.chrome
        }
    };
    let check: Element<'a, Message> = if on {
        icon(Icon::Check, 10.0, mark)
    } else {
        widget::space().into()
    };
    button(container(check).center(Fill))
        .width(15)
        .height(15)
        .padding(0)
        .on_press(on_toggle)
        .style(move |_, _| button::Style {
            background: on.then_some(Background::Color(color)),
            border: Border::default().rounded(4).width(1.5).color(color),
            ..button::Style::default()
        })
        .focusable(4.0, tokens)
}

/// A 30 px square icon button in the secondary color: Close, Settings.
pub fn icon_button<'a, Message: Clone + 'a>(
    which: Icon,
    icon_size: f32,
    tokens: &Tokens,
    on_press: Message,
) -> Element<'a, Message> {
    button(container(icon(which, icon_size, tokens.sub)).center(Fill))
        .width(30)
        .height(30)
        .padding(0)
        .on_press(on_press)
        .style(row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Accent fill.
    Primary,
    /// Line border, no fill.
    Outline,
    /// Raised fill.
    Neutral,
    /// Line border, no fill, red text: Delete, Disconnect. Never a red
    /// fill.
    Destructive,
    /// Plain text, no fill or border: Cancel under a stack of choices.
    Text,
    /// Raised fill, red text: a delete that is the dialog's own action
    /// (Unused covers). Never a red fill.
    NeutralDestructive,
}

impl ButtonKind {
    pub const ALL: [ButtonKind; 6] = [
        ButtonKind::Primary,
        ButtonKind::Outline,
        ButtonKind::Neutral,
        ButtonKind::Destructive,
        ButtonKind::Text,
        ButtonKind::NeutralDestructive,
    ];
}

/// A 36 px button with a SemiBold label.
pub fn labeled_button<'a, Message: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    kind: ButtonKind,
    tokens: Tokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    sized_button(label, kind, 36.0, tokens, on_press)
}

/// A labeled button of another height: the compact ones in Settings.
pub fn sized_button<'a, Message: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    kind: ButtonKind,
    height: f32,
    tokens: Tokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(size::BODY)
            .font(font::SEMIBOLD)
            .height(Fill)
            .align_y(Vertical::Center),
    )
    .height(height)
    .padding(Padding::from([0, 16]))
    .on_press_maybe(on_press)
    .style(move |_, status| button_style(kind, &tokens, status))
    .focusable(8.0, &tokens)
}

/// A labeled button that fills the width, its label centered: the plan
/// form's footer.
pub fn wide_button<'a, Message: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    kind: ButtonKind,
    tokens: Tokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    wide_button_keys(label, None, kind, tokens, on_press)
}

/// A wide button with its shortcut after the label in small mono: Edit
/// in the details pane.
pub fn wide_button_keys<'a, Message: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    shortcut: Option<String>,
    kind: ButtonKind,
    tokens: Tokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let mut words = widget::row![text(label).size(size::BODY).font(font::SEMIBOLD)]
        .spacing(8)
        .align_y(Vertical::Center);
    if let Some(shortcut) = shortcut {
        words = words.push(mono(shortcut, 11.0, tokens.sub));
    }
    button(container(words).center(Fill))
        .width(Fill)
        .height(36)
        .padding(Padding::from([0, 16]))
        .on_press_maybe(on_press)
        .style(move |_, status| button_style(kind, &tokens, status))
        .focusable(8.0, &tokens)
}

/// The fill, text and border of a `ButtonKind`, for buttons built by hand.
pub fn button_style(kind: ButtonKind, tokens: &Tokens, status: button::Status) -> button::Style {
    let (fill, text_color, border) = match kind {
        ButtonKind::Primary => (Some(tokens.tint), tokens.on_accent, None),
        ButtonKind::Outline => (None, tokens.text, Some(tokens.line)),
        ButtonKind::Neutral => (Some(tokens.raised), tokens.text, None),
        ButtonKind::Destructive => (None, tokens.danger, Some(tokens.line)),
        ButtonKind::Text => (None, tokens.text, None),
        // The selection gray: the raised fill is the dialog's own color.
        ButtonKind::NeutralDestructive => (Some(tokens.selection), tokens.danger, None),
    };
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => Some(match fill {
            Some(_) if kind == ButtonKind::Primary => tokens.tint_hover(),
            Some(color) => color.scale_alpha(0.85),
            None => tokens.hover,
        }),
        _ => fill,
    };
    let disabled = status == button::Status::Disabled;
    let fill = if disabled {
        fill.map(|c| c.scale_alpha(0.5))
    } else {
        fill
    };
    button::Style {
        background: fill.map(Background::Color),
        text_color: if disabled {
            text_color.scale_alpha(0.5)
        } else {
            text_color
        },
        border: Border {
            color: border.unwrap_or(Color::TRANSPARENT),
            width: if border.is_some() { 1.0 } else { 0.0 },
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{PEACH, Theme, Tokens};

    /// Dialogs and popovers have no outline (decided 2026-11-04). iced's
    /// quad shader puts the border color in place of the fill inside the
    /// border ring, not over it, so a see-through line such as Glass's 14 %
    /// white showed the scrimmed screen through the ring: a light fringe
    /// over bright covers, nothing over dark ones.
    #[test]
    fn panels_have_no_outline_on_any_theme() {
        for theme in Theme::ALL {
            for flat in [false, true] {
                let tokens = Tokens::styled(theme, PEACH, flat);
                let style = panel_style(&tokens, 12.0);
                assert_eq!(style.border.width, 0.0, "{} (flat: {flat})", theme.label());
                assert_eq!(style.background, Some(Background::Color(tokens.popover)));
            }
        }
    }
}
