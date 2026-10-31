//! The tag picker, one widget for the Filter popover's "All tags…", the
//! entry form's Tags and the plan dialog's linked hashtags: a search
//! across every group, groups that fold (those holding a picked tag start
//! open), a tag in several categories as a chip under each with its own
//! tick, a group of more than 24 tags as one row that opens the full list
//! (search, letter headers, a letter rail, a check per row), and, in the
//! entry form, a right-click menu on every chip and "+ Add to Directors"
//! for a person found under another people category.
//!
//! The state and the messages are `crate::tags`; the caller maps them into
//! its own messages with `map`, and wraps `header`, `body` and `footer` in
//! its dialog.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use iced::widget::{Column, Row, button, column, container, mouse_area, row, space, text};
use iced::{Background, Border, Center, Element, Fill, Padding};

use super::{FocusExt, Icon, Popover, icon, mono, panel_style, row_style, section_header};
use crate::model::{TagCategory, sort_tags, tag_display_name, tag_sort_key};
use crate::tags::{DRILL_ID, PickerMessage, PickerState};
use crate::theme::{Tokens, font, size};

/// A letter header and a row in the long group's list, in px, for the
/// rail's jumps.
const LETTER_HEIGHT: f32 = 28.0;
const ROW_HEIGHT: f32 = 44.0;

/// A tag's categories as they will be once the caller saves.
pub type CategoriesOf<'a> = Box<dyn Fn(&str) -> Vec<TagCategory> + 'a>;
/// Whether a tag is picked, as the category for a tag in several.
pub type IsOn<'a> = Box<dyn Fn(&str, Option<TagCategory>) -> bool + 'a>;

/// What one picker shows, from its caller.
pub struct Picker<'a, M> {
    pub state: &'a PickerState,
    /// Every tag listed per category, in category order; a tag in several
    /// categories is under each of them.
    pub groups: Vec<(TagCategory, Vec<String>)>,
    /// How many entries use each tag.
    pub counts: HashMap<&'a str, usize>,
    pub categories_of: CategoriesOf<'a>,
    pub is_on: IsOn<'a>,
    /// Whether a group shows before anything is typed.
    pub visible: Box<dyn Fn(TagCategory) -> bool + 'a>,
    /// Whether a group starts open, given how many of its tags are picked.
    pub default_open: Box<dyn Fn(TagCategory, usize) -> bool + 'a>,
    /// A group with more tags than this is a row that opens the full list.
    pub long_group: usize,
    /// The right-click menu on chips (the entry form).
    pub with_menu: bool,
    /// "+ Add to Directors" rows while searching (the entry form).
    pub with_add_to: bool,
    /// The list's height in px.
    pub height: f32,
    pub map: Rc<dyn Fn(PickerMessage) -> M + 'a>,
}

impl<'a, M: Clone + 'a> Picker<'a, M> {
    fn send(&self, message: PickerMessage) -> M {
        (self.map)(message)
    }

    /// The search as a tag: lower case, a leading "#" dropped, spaces as
    /// hyphens.
    fn query(&self) -> String {
        normalize(&self.state.query)
    }

    /// The category a chip's tick is for: the chip's group for a tag in
    /// several categories, none for a tag in one.
    fn key(&self, tag: &str, category: TagCategory) -> Option<TagCategory> {
        ((self.categories_of)(tag).len() > 1).then_some(category)
    }

    /// The tags under `category`, A to Z.
    fn all_in(&self, category: TagCategory) -> Vec<String> {
        self.groups
            .iter()
            .find(|(c, _)| *c == category)
            .map(|(_, tags)| {
                sort_tags(tags.iter().map(String::as_str))
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The dialog's title row: `back` (the plan dialog's way back to the
    /// form), the title and subtitle, and Close; inside a long group, its
    /// own back arrow, name and count line instead.
    pub fn header(
        &self,
        title: &str,
        subtitle: Option<String>,
        back: Option<M>,
        on_close: M,
        tokens: &Tokens,
    ) -> Element<'a, M> {
        let mut heading = Row::new().spacing(8).align_y(Center);
        match &self.state.drill {
            Some(drill) => {
                let all = self.all_in(drill.category);
                let selected = all
                    .iter()
                    .filter(|tag| (self.is_on)(tag, self.key(tag, drill.category)))
                    .count();
                let mut line = format!("{} {}", all.len(), drill.category.name().to_lowercase());
                if selected > 0 {
                    line.push_str(&format!(" · {selected} selected"));
                }
                heading = heading
                    .push(back_button(self.send(PickerMessage::Back), tokens))
                    .push(
                        text(drill.category.name().to_owned())
                            .size(19)
                            .font(font::EXTRABOLD),
                    )
                    .push(text(line).size(13).color(tokens.sub));
            }
            None => {
                if let Some(back) = back {
                    heading = heading.push(back_button(back, tokens));
                }
                heading = heading.push(text(title.to_owned()).size(19).font(font::EXTRABOLD));
                if let Some(subtitle) = subtitle {
                    heading = heading.push(text(subtitle).size(13).color(tokens.sub));
                }
            }
        }
        heading
            .push(space::horizontal())
            .push(super::icon_button(Icon::Close, 16.0, tokens, on_close))
            .into()
    }

    /// The search and the groups, or the long group in full. `extra` sits
    /// between the search and the list (the form's create box).
    pub fn body(
        &self,
        placeholder: &str,
        extra: Option<Element<'a, M>>,
        tokens: &Tokens,
    ) -> Element<'a, M> {
        let state: &'a PickerState = self.state;
        if let Some(drill) = &state.drill {
            return self.drill(drill.category, &drill.query, tokens);
        }
        let map = self.map.clone();
        let search = super::search_field(
            placeholder,
            &self.state.query,
            move |query| map(PickerMessage::Query(query)),
            None,
            None,
            tokens,
        );
        let mut body = Column::new().spacing(12).push(search);
        if let Some(extra) = extra {
            body = body.push(extra);
        }
        let groups = self.group_elements(tokens);
        let list: Element<'a, M> = if groups.is_empty() {
            text("No tags match.").size(13.5).color(tokens.sub).into()
        } else {
            Column::with_children(groups).spacing(2).into()
        };
        body.push(super::scroll(list).height(self.height)).into()
    }

    /// One element per group shown: a folding group, or a long group's row.
    fn group_elements(&self, tokens: &Tokens) -> Vec<Element<'a, M>> {
        let query = self.query();
        let searching = !query.is_empty();
        let mut elements = Vec::new();
        // Every category, so a search can offer "+ Add to Authors" under a
        // group with no tags yet.
        for category in TagCategory::ALL {
            if !searching && !(self.visible)(category) {
                continue;
            }
            let all = self.all_in(category);
            let listed: Vec<&str> = all
                .iter()
                .map(String::as_str)
                .filter(|tag| !searching || tag.contains(&query))
                .collect();
            let selected = all
                .iter()
                .filter(|tag| (self.is_on)(tag, self.key(tag, category)))
                .count();
            let add_rows = if searching && self.with_add_to && listed.is_empty() {
                self.add_rows(category, &query, tokens)
            } else {
                Vec::new()
            };
            if !searching && all.len() > self.long_group {
                elements.push(self.long_row(category, all.len(), selected, tokens));
                continue;
            }
            if listed.is_empty() && add_rows.is_empty() {
                continue;
            }
            let open = searching
                || ((self.default_open)(category, selected)
                    != self.state.toggled.contains(&category));
            let chips = listed
                .iter()
                .map(|&tag| self.chip(tag, category, tokens))
                .collect();
            elements.push(super::tag_group(
                category.name(),
                all.len(),
                selected,
                open,
                self.send(PickerMessage::ToggleGroup(category)),
                chips,
                add_rows,
                tokens,
            ));
        }
        elements
    }

    /// A tag's chip under `category`, with the right-click menu when the
    /// caller has one.
    fn chip(&self, tag: &str, category: TagCategory, tokens: &Tokens) -> Element<'a, M> {
        let key = self.key(tag, category);
        let on = (self.is_on)(tag, key);
        let count = self.counts.get(tag).copied().unwrap_or(0);
        let chip = super::filter_chip(
            format!("#{tag}"),
            None,
            Some(count),
            on,
            tokens,
            self.send(PickerMessage::Toggle(tag.to_owned(), key)),
        );
        if !self.with_menu {
            return chip;
        }
        let chip = mouse_area(chip)
            .on_right_press(self.send(PickerMessage::Menu(Some((tag.to_owned(), category)))));
        let open = self
            .state
            .menu
            .as_ref()
            .is_some_and(|(t, c)| t == tag && *c == category);
        let menu = open.then(|| self.menu(tag, category, on, tokens));
        Popover::new(chip, menu)
            .on_dismiss(self.send(PickerMessage::Menu(None)))
            .into()
    }

    /// The right-click menu: "Categories…" for a people or genre tag,
    /// "Move to another group…" for one in a one-category group, then
    /// "Add to this entry" or "Remove from this entry".
    fn menu(&self, tag: &str, category: TagCategory, on: bool, tokens: &Tokens) -> Element<'a, M> {
        let categories = (self.categories_of)(tag);
        let first = categories.first().copied().unwrap_or(TagCategory::Other);
        let (label, hint) = if first.kind().is_some() {
            let names: Vec<&str> = categories.iter().map(|c| c.name()).collect();
            ("Categories…", names.join(", "))
        } else {
            ("Move to another group…", format!("Now in {}", first.name()))
        };
        let toggle = if on {
            "Remove from this entry"
        } else {
            "Add to this entry"
        };
        let items = [
            (label, hint, PickerMessage::Categories(tag.to_owned())),
            (
                toggle,
                String::new(),
                PickerMessage::Toggle(tag.to_owned(), self.key(tag, category)),
            ),
        ];
        let rows = items.into_iter().map(|(label, hint, message)| {
            button(
                row![
                    text(label).size(14).width(Fill),
                    text(hint).size(12).color(tokens.sub),
                ]
                .spacing(10)
                .align_y(Center)
                .height(Fill),
            )
            .width(Fill)
            .height(36)
            .padding([0, 10])
            .on_press(self.send(message))
            .style(row_style(tokens, false, 8.0))
            .focusable(8.0, tokens)
        });
        let title = container(section_header(&format!("#{tag}"), tokens)).padding([8, 10]);
        let tokens = *tokens;
        container(Column::new().push(title).extend(rows))
            .width(280)
            .padding(6)
            .style(move |_| panel_style(&tokens, 12.0))
            .into()
    }

    /// A long group as one row: "Authors 64 · 1 selected · Show all ›".
    fn long_row(
        &self,
        category: TagCategory,
        count: usize,
        selected: usize,
        tokens: &Tokens,
    ) -> Element<'a, M> {
        let picked = if selected > 0 {
            format!("{selected} selected")
        } else {
            String::new()
        };
        button(
            row![
                icon(Icon::ViewList, 14.0, tokens.sub),
                text(category.name().to_uppercase())
                    .size(size::SECTION)
                    .font(font::BOLD)
                    .color(tokens.sub),
                mono(count.to_string(), 11.0, tokens.sub),
                space::horizontal(),
                text(picked).size(12).color(tokens.text),
                text("Show all").size(12.5).color(tokens.sub),
                icon(Icon::ChevronRight, 14.0, tokens.sub),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .width(Fill)
        .height(36)
        .padding([0, 4])
        .on_press(self.send(PickerMessage::Drill(category)))
        .style(row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
    }

    /// Search hits from the other categories of `category`'s kind, under
    /// it: "#morgan-freeman · Not a director tag yet · + Add to Directors".
    fn add_rows(&self, category: TagCategory, query: &str, tokens: &Tokens) -> Vec<Element<'a, M>> {
        let Some(kind) = category.kind() else {
            return Vec::new();
        };
        let mut seen: Vec<String> = Vec::new();
        let mut rows = Vec::new();
        for other in TagCategory::ALL {
            if other == category || other.kind() != Some(kind) || !(self.visible)(other) {
                continue;
            }
            for tag in self.all_in(other) {
                if !tag.contains(query)
                    || seen.contains(&tag)
                    || (self.categories_of)(&tag).contains(&category)
                {
                    continue;
                }
                let tag = tag.as_str();
                seen.push(tag.to_owned());
                let words = column![
                    text(format!("#{tag}")).size(14),
                    text(format!("Not {} tag yet", category.role_with_article()))
                        .size(12)
                        .color(tokens.sub),
                ]
                .spacing(2)
                .width(Fill);
                let add = super::add_chip(
                    format!("+ Add to {}", category.name()),
                    tokens,
                    self.send(PickerMessage::AddTo(tag.to_owned(), category)),
                );
                rows.push(
                    container(row![words, add].spacing(10).align_y(Center))
                        .padding(Padding::new(0.0).top(4).bottom(6))
                        .into(),
                );
            }
        }
        rows
    }

    /// The long group in full: a search, letter headers, a check per row,
    /// and the letter rail that jumps.
    fn drill(&self, category: TagCategory, query: &'a str, tokens: &Tokens) -> Element<'a, M> {
        let all = self.all_in(category);
        let raw = query.trim().to_lowercase();
        let hyphenated = normalize(query);
        let listed = all.iter().map(String::as_str).filter(|tag| {
            raw.is_empty()
                || tag_display_name(tag).to_lowercase().contains(&raw)
                || tag.contains(&hyphenated)
        });
        let mut sections: BTreeMap<char, Vec<&str>> = BTreeMap::new();
        for tag in listed {
            let letter = tag_sort_key(tag)
                .chars()
                .next()
                .filter(char::is_ascii_alphabetic)
                .map_or('#', |c| c.to_ascii_uppercase());
            sections.entry(letter).or_default().push(tag);
        }

        let mut list = Column::new();
        let mut offsets: BTreeMap<char, f32> = BTreeMap::new();
        let mut offset = 0.0;
        for (&letter, tags) in &sections {
            offsets.insert(letter, offset);
            list = list.push(
                container(
                    text(letter.to_string())
                        .size(size::SECTION)
                        .font(font::BOLD)
                        .color(tokens.sub),
                )
                .height(LETTER_HEIGHT)
                .padding(Padding::new(0.0).left(10))
                .align_y(iced::alignment::Vertical::Bottom),
            );
            offset += LETTER_HEIGHT;
            for &tag in tags {
                list = list.push(self.drill_row(tag, category, tokens));
                offset += ROW_HEIGHT;
            }
        }
        let list: Element<'a, M> = if sections.is_empty() {
            container(text("No match.").size(13.5).color(tokens.sub))
                .padding([12, 10])
                .into()
        } else {
            list.into()
        };

        // The 26 letters share the list's height, at most 16 px each, so
        // the rail never runs past the list.
        let letter_height = (self.height / 26.0).min(16.0);
        let rail = Column::with_children(('A'..='Z').map(|letter| {
            let present = offsets.get(&letter).copied();
            let color = if present.is_some() {
                tokens.text
            } else {
                tokens.line
            };
            button(
                container(
                    text(letter.to_string())
                        .size(10.5)
                        .line_height(1.2)
                        .font(font::SEMIBOLD)
                        .color(color),
                )
                .center_y(Fill),
            )
            .height(letter_height)
            .padding([0, 5])
            .on_press_maybe(present.map(|offset| self.send(PickerMessage::Jump(offset))))
            .style(row_style(tokens, false, 4.0))
            .into()
        }))
        .align_x(Center);

        let map = self.map.clone();
        let placeholder = format!("Search {}", category.name().to_lowercase());
        let search = super::search_field(
            &placeholder,
            query,
            move |query| map(PickerMessage::DrillQuery(query)),
            None,
            None,
            tokens,
        );
        let scroll = super::scroll(list)
            .id(DRILL_ID)
            .height(self.height)
            .width(Fill);
        column![
            search,
            // `height` before `align_y`: `center_y(Fill)` would make the
            // rail as tall as the window and the dialog with it.
            row![
                scroll,
                container(rail)
                    .height(self.height)
                    .align_y(iced::alignment::Vertical::Center)
            ]
            .spacing(6)
        ]
        .spacing(12)
        .into()
    }

    /// One tag in the long group: its name, "#tag · 3 entries" and a check.
    fn drill_row(&self, tag: &str, category: TagCategory, tokens: &Tokens) -> Element<'a, M> {
        let key = self.key(tag, category);
        let on = (self.is_on)(tag, key);
        let count = self.counts.get(tag).copied().unwrap_or(0);
        let entries = if count == 1 { "entry" } else { "entries" };
        let words = column![
            text(tag_display_name(tag)).size(14),
            text(format!("#{tag} · {count} {entries}"))
                .size(12)
                .color(tokens.sub),
        ]
        .spacing(1)
        .width(Fill);
        let tokens_copy = *tokens;
        let mark: Element<'a, M> = if on {
            icon(Icon::Check, 12.0, tokens.on_accent)
        } else {
            space().into()
        };
        let check = container(mark).center(20).style(move |_| container::Style {
            background: on.then_some(Background::Color(tokens_copy.accent)),
            border: Border::default().rounded(10).width(1.5).color(if on {
                tokens_copy.accent
            } else {
                tokens_copy.sub
            }),
            ..container::Style::default()
        });
        button(row![words, check].spacing(10).align_y(Center).height(Fill))
            .width(Fill)
            .height(ROW_HEIGHT)
            .padding([0, 10])
            .on_press(self.send(PickerMessage::Toggle(tag.to_owned(), key)))
            .style(row_style(tokens, on, 8.0))
            .focusable(8.0, tokens)
    }
}

/// The picker's footer: an outlined button on the left, the summary, and
/// the main button on the right.
pub fn footer<'a, M: Clone + 'a>(
    clear_label: &'a str,
    on_clear: Option<M>,
    summary: String,
    done_label: String,
    on_done: M,
    tokens: &Tokens,
) -> Element<'a, M> {
    row![
        super::labeled_button(clear_label, super::ButtonKind::Outline, *tokens, on_clear),
        text(summary).size(13).color(tokens.sub).width(Fill),
        super::labeled_button(
            done_label,
            super::ButtonKind::Primary,
            *tokens,
            Some(on_done)
        ),
    ]
    .spacing(10)
    .align_y(Center)
    .into()
}

/// A 32 px back arrow.
fn back_button<'a, M: Clone + 'a>(on_press: M, tokens: &Tokens) -> Element<'a, M> {
    button(container(icon(Icon::ChevronLeft, 18.0, tokens.text)).center(Fill))
        .width(32)
        .height(32)
        .padding(0)
        .on_press(on_press)
        .style(row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
}

/// A search as a tag would be written: trimmed, lower case, a leading "#"
/// dropped, spaces as hyphens.
fn normalize(query: &str) -> String {
    query
        .trim()
        .trim_start_matches('#')
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}
