//! Throwaway token sheet (TODO.md 0c): every color, text style, button,
//! icon and shared widget, to eyeball against the mockup. Remove at the end
//! of phase 1.

use iced::widget::{Column, Row, button, column, container, row, text};
use iced::{Background, Border, Color, Element, Fill};

use crate::app::{App, Message as AppMessage};
use crate::model::{MediaType, TagCategory};
use crate::theme::{self, Accent, Tokens, font, size};
use crate::ui::{self, Align, ButtonKind, Icon, Popover};

const SORTS: [&str; 5] = [
    "Newest added",
    "Title A to Z",
    "Year newest first",
    "Year oldest first",
    "Rating highest first",
];
const STATUSES: [(&str, usize); 4] = [("All", 52), ("Want", 14), ("Done", 31), ("DNF", 7)];

/// The gallery's own state: nothing here reaches the library.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub search: String,
    pub status: usize,
    pub picked: Vec<String>,
    pub open_groups: Vec<TagCategory>,
    pub rating: Option<u8>,
    pub rating_open: bool,
    pub sort: usize,
    pub sort_open: bool,
    pub dialog_open: bool,
    /// How often the dialog was confirmed, to see Enter work.
    pub confirmed: u32,
}

#[derive(Debug, Clone)]
pub enum Message {
    Search(String),
    Status(usize),
    ToggleTag(String),
    ToggleGroup(TagCategory),
    ToggleRating,
    Rate(Option<u8>),
    ToggleSort,
    Sort(usize),
    OpenDialog,
    CloseDialog,
    ConfirmDialog,
}

impl State {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Search(query) => self.search = query,
            Message::Status(index) => self.status = index,
            Message::ToggleTag(tag) => {
                if let Some(at) = self.picked.iter().position(|t| *t == tag) {
                    self.picked.remove(at);
                } else {
                    self.picked.push(tag);
                }
            }
            Message::ToggleGroup(category) => {
                if let Some(at) = self.open_groups.iter().position(|c| *c == category) {
                    self.open_groups.remove(at);
                } else {
                    self.open_groups.push(category);
                }
            }
            Message::ToggleRating => self.rating_open = !self.rating_open,
            Message::Rate(rating) => {
                self.rating = rating;
                self.rating_open = false;
            }
            Message::ToggleSort => self.sort_open = !self.sort_open,
            Message::Sort(index) => {
                self.sort = index;
                self.sort_open = false;
            }
            Message::OpenDialog => self.dialog_open = true,
            Message::CloseDialog => self.dialog_open = false,
            Message::ConfirmDialog => self.confirm(),
        }
    }

    /// Esc: the dialog first, then any popover.
    pub fn dismiss_top(&mut self) {
        if self.dialog_open {
            self.dialog_open = false;
        } else {
            self.sort_open = false;
            self.rating_open = false;
        }
    }

    /// Enter: confirms the dialog when one is open.
    pub fn confirm(&mut self) {
        if self.dialog_open {
            self.dialog_open = false;
            self.confirmed += 1;
        }
    }
}

pub fn view(app: &App, tokens: Tokens) -> Element<'_, AppMessage> {
    let themes = row(theme::Theme::ALL.map(|choice| {
        let kind = if choice == app.theme {
            ButtonKind::Primary
        } else {
            ButtonKind::Neutral
        };
        ui::labeled_button(
            choice.label(),
            kind,
            tokens,
            Some(AppMessage::SetTheme(choice)),
        )
    }))
    .spacing(8);

    let surfaces = [
        ("chrome", tokens.chrome),
        ("content", tokens.content),
        ("raised", tokens.raised),
        ("field", tokens.field),
        ("line", tokens.line),
        ("text", tokens.text),
        ("sub", tokens.sub),
        ("selection", tokens.selection),
        ("hover", tokens.hover),
        ("track", tokens.track),
        ("popover", tokens.popover),
        ("danger", tokens.danger),
        ("accent", tokens.accent),
        ("ink", tokens.ink),
        ("on accent", tokens.on_accent),
        ("Halloween", theme::HALLOWEEN),
        ("Christmas", theme::CHRISTMAS),
    ];
    let surfaces = Row::with_children(surfaces.map(|(name, color)| swatch(name, color, &tokens)))
        .spacing(12)
        .wrap();

    let types = row(MediaType::ALL.map(|media| {
        let color = tokens.type_color(media);
        row![
            ui::dot(color, 10.0, 5.0),
            text(media.label()).font(font::SEMIBOLD).color(color)
        ]
        .spacing(6)
        .align_y(iced::Center)
        .into()
    }))
    .spacing(20);

    let text_styles = column![
        ui::heading("Heading, Figtree ExtraBold 26", &tokens),
        text("Title, Figtree ExtraBold 18")
            .size(size::TITLE)
            .font(font::EXTRABOLD),
        text("Body, Figtree Regular 14").size(size::BODY),
        text("Italic, Figtree Italic 14")
            .size(size::BODY)
            .font(font::ITALIC),
        text("Medium, Figtree Medium 14")
            .size(size::BODY)
            .font(font::MEDIUM),
        text("Label, Figtree SemiBold 14")
            .size(size::BODY)
            .font(font::SEMIBOLD),
        text("Secondary, Figtree Regular 13")
            .size(size::SMALL)
            .color(tokens.sub),
        ui::section_header("Section header, 12.5 bold caps", &tokens),
        ui::mono(
            "1999 · 42 · 2026-10-18, JetBrains Mono 12",
            size::MONO,
            tokens.sub
        ),
        text("Counts, JetBrains Mono Medium 12")
            .size(size::MONO)
            .font(font::MONO_MEDIUM),
    ]
    .spacing(10);

    let buttons = row(ButtonKind::ALL.map(|kind| {
        let label = match kind {
            ButtonKind::Primary => "Primary",
            ButtonKind::Outline => "Outline",
            ButtonKind::Neutral => "Neutral",
            ButtonKind::Destructive => "Delete",
        };
        ui::labeled_button(label, kind, tokens, Some(AppMessage::SetAccent(app.accent)))
    }))
    .push(ui::labeled_button(
        "Disabled",
        ButtonKind::Primary,
        tokens,
        None,
    ))
    .spacing(8);

    let icons = row(Icon::ALL.map(|which| ui::icon(which, 20.0, tokens.text)))
        .push(ui::icon(Icon::Plus, 20.0, tokens.ink))
        .spacing(14);

    let content = column![
        ui::heading("Token sheet", &tokens),
        section("Theme", themes.into(), &tokens),
        section(
            "Accent, Vivid",
            accents(&theme::VIVID, app.accent, &tokens),
            &tokens
        ),
        section(
            "Accent, Catppuccin",
            accents(&theme::CATPPUCCIN, app.accent, &tokens),
            &tokens
        ),
        section("Colors", surfaces.into(), &tokens),
        section("Types", types.into(), &tokens),
        section("Text", text_styles.into(), &tokens),
        section("Buttons", buttons.into(), &tokens),
        section("Icons", icons.into(), &tokens),
        widgets(app, &tokens),
    ]
    .spacing(28)
    .padding(40);

    let content_bg = tokens.content;
    let page = container(ui::scroll(content).width(Fill).height(Fill))
        .style(move |_| container::background(content_bg).color(tokens.text));
    let state = &app.token_sheet;
    let dialog = state.dialog_open.then(|| demo_dialog(&tokens));
    let close = AppMessage::TokenSheet(Message::CloseDialog);
    ui::modal(page, dialog, close, &tokens)
}

/// The shared widgets, wired to the gallery's state.
fn widgets<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.token_sheet;
    let msg = AppMessage::TokenSheet;

    let search = container(ui::search_field(
        "Search movies",
        &state.search,
        move |query| msg(Message::Search(query)),
        Some("Ctrl K"),
        tokens,
    ))
    .width(280);

    let statuses =
        Row::with_children(STATUSES.iter().enumerate().map(|(index, &(label, count))| {
            ui::status_chip(
                label,
                count,
                state.status == index,
                tokens,
                msg(Message::Status(index)),
            )
        }))
        .spacing(6);
    let mut active = Row::new().spacing(6);
    for tag in &state.picked {
        active = active.push(ui::removable_chip(
            format!("#{tag}"),
            tokens,
            msg(Message::ToggleTag(tag.clone())),
        ));
    }
    let chips = column![
        statuses,
        active,
        row![
            ui::tag_chip("#horror".into(), tokens),
            ui::tag_chip("#stephen-king".into(), tokens),
            ui::add_chip("+ Tag", tokens, msg(Message::Status(0))),
        ]
        .spacing(6),
    ]
    .spacing(10);

    let library = app.repo.library();
    let counts = library.tag_counts();
    let groups = Column::with_children(library.tags.iter().take(3).map(|(&category, names)| {
        let chips = names
            .iter()
            .map(|name| {
                let count = counts.iter().find(|(tag, _)| tag == name).map(|(_, n)| *n);
                let on = state.picked.contains(name);
                ui::filter_chip(
                    format!("#{name}"),
                    count,
                    on,
                    tokens,
                    msg(Message::ToggleTag(name.clone())),
                )
            })
            .collect();
        let selected = names
            .iter()
            .filter(|name| state.picked.contains(name))
            .count();
        let open = state.open_groups.contains(&category);
        ui::tag_group(
            category.name(),
            names.len(),
            selected,
            open,
            msg(Message::ToggleGroup(category)),
            chips,
            tokens,
        )
    }))
    .width(520);

    let rating = ui::rating(
        state.rating,
        state.rating_open,
        msg(Message::ToggleRating),
        move |rating| msg(Message::Rate(rating)),
        tokens,
    );
    let sort_anchor = ui::labeled_button(
        SORTS[state.sort],
        ButtonKind::Outline,
        *tokens,
        Some(msg(Message::ToggleSort)),
    );
    let sort_menu = state.sort_open.then(|| {
        let items = SORTS
            .iter()
            .enumerate()
            .map(|(index, &label)| (label, index == state.sort, msg(Message::Sort(index))));
        ui::radio_menu("Sort by", items, tokens)
    });
    let sort = Popover::new(sort_anchor, sort_menu)
        .align(Align::Start)
        .on_dismiss(msg(Message::ToggleSort));
    let dialog = row![
        ui::labeled_button(
            "Open a dialog",
            ButtonKind::Neutral,
            *tokens,
            Some(msg(Message::OpenDialog))
        ),
        text(format!("Confirmed {} times", state.confirmed))
            .size(size::SMALL)
            .color(tokens.sub),
    ]
    .spacing(12)
    .align_y(iced::Center);

    column![
        section("Search field", search.into(), tokens),
        section("Chips", chips.into(), tokens),
        section("Collapsible tag groups", groups.into(), tokens),
        section(
            "Rating, sort menu and dialog",
            row![rating, sort, dialog].spacing(16).into(),
            tokens
        ),
    ]
    .spacing(28)
    .into()
}

fn demo_dialog<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    let msg = AppMessage::TokenSheet;
    let body = text("Enter confirms, Esc or a click on the scrim closes.")
        .size(size::BODY)
        .color(tokens.sub);
    let footer = row![
        iced::widget::space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            *tokens,
            Some(msg(Message::CloseDialog))
        ),
        ui::labeled_button(
            "Confirm",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::ConfirmDialog))
        ),
    ]
    .spacing(8);
    ui::dialog(
        "A dialog",
        body,
        footer,
        440,
        tokens,
        msg(Message::CloseDialog),
    )
}

fn section<'a>(
    label: &str,
    body: Element<'a, AppMessage>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![ui::section_header(label, tokens), body]
        .spacing(12)
        .into()
}

fn accents<'a>(palette: &[Accent], current: Accent, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let swatches = palette.iter().map(|&accent| {
        let label = row![
            ui::dot(accent.fill, 14.0, 7.0),
            text(accent.name).color(tokens.text)
        ]
        .spacing(8)
        .align_y(iced::Center);
        let selected = accent == current;
        button(label)
            .on_press(AppMessage::SetAccent(accent))
            .padding([6, 10])
            .style(move |_, _| button::Style {
                background: selected.then_some(Background::Color(tokens.selection)),
                border: Border::default().rounded(8),
                ..button::Style::default()
            })
            .into()
    });
    Row::with_children(swatches).spacing(6).wrap().into()
}

fn swatch<'a>(name: &'a str, color: Color, tokens: &Tokens) -> Element<'a, AppMessage> {
    let line = tokens.line;
    let chip = container(text("")).width(72).height(44).style(move |_| {
        container::background(color).border(Border::default().rounded(8).width(1).color(line))
    });
    Column::new()
        .push(chip)
        .push(text(name).size(size::SMALL).color(tokens.sub))
        .spacing(4)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::KeyPress;
    use crate::screens::Screen;

    fn run(app: &mut App, act: impl FnOnce(&mut iced_test::Simulator<'_, AppMessage>)) {
        let messages = {
            // Tall enough that the whole sheet is on screen.
            let size = iced::Size::new(1600.0, 2600.0);
            let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
            act(&mut ui);
            ui.into_messages().collect::<Vec<_>>()
        };
        for message in messages {
            let _ = app.update(message);
        }
    }

    #[test]
    fn picking_a_theme_and_an_accent_updates_the_app() {
        let mut app = App::new(Screen::TokenSheet);
        run(&mut app, |ui| {
            assert!(ui.find("Token sheet").is_ok());
            ui.click("Light").unwrap();
            ui.click("Lavender").unwrap();
        });
        assert_eq!(app.theme, theme::Theme::Light);
        assert_eq!(app.accent.name, "Lavender");
    }

    #[test]
    fn the_rating_picker_opens_sets_a_rating_and_closes_on_a_click_outside() {
        let mut app = App::new(Screen::TokenSheet);
        run(&mut app, |ui| {
            ui.click("Not rated").unwrap();
        });
        assert!(app.token_sheet.rating_open);
        // "9" is only in the picker; other numbers are also tag counts.
        run(&mut app, |ui| {
            ui.click("9").unwrap();
        });
        assert_eq!(app.token_sheet.rating, Some(9));
        assert!(!app.token_sheet.rating_open);

        run(&mut app, |ui| {
            ui.click("9").unwrap();
        });
        assert!(app.token_sheet.rating_open);
        run(&mut app, |ui| {
            ui.click("Token sheet").unwrap();
        });
        assert!(!app.token_sheet.rating_open);
        assert_eq!(app.token_sheet.rating, Some(9));
    }

    #[test]
    fn enter_confirms_the_dialog_and_esc_closes_the_top_layer_first() {
        let mut app = App::new(Screen::TokenSheet);
        let send = |app: &mut App, message| {
            let _ = app.update(message);
        };
        send(&mut app, AppMessage::TokenSheet(Message::ToggleSort));
        send(&mut app, AppMessage::TokenSheet(Message::OpenDialog));
        send(&mut app, AppMessage::Key(KeyPress::Escape));
        assert!(!app.token_sheet.dialog_open);
        assert!(
            app.token_sheet.sort_open,
            "the popover under the dialog stays"
        );
        send(&mut app, AppMessage::Key(KeyPress::Escape));
        assert!(!app.token_sheet.sort_open);

        send(&mut app, AppMessage::TokenSheet(Message::OpenDialog));
        send(&mut app, AppMessage::Key(KeyPress::Enter));
        assert_eq!(
            (app.token_sheet.dialog_open, app.token_sheet.confirmed),
            (false, 1)
        );
    }
}
