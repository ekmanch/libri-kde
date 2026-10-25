//! Settings (docs/DESIGN.md 4.5): Appearance, Library source, Plans, Games
//! and About, with an "On this page" rail. Changes apply right away; they
//! are app state until phase 2 saves them.

use iced::alignment::Vertical;
use iced::widget::{
    Column, Row, button, column, container, operation, row, scrollable, space, text,
};
use iced::{Background, Border, Center, Color, Element, Fill, FillPortion, Padding, Task};

use super::library::SidebarTags;
use super::setup;
use super::shell::shell;
use crate::app::{App, Message as AppMessage};
use crate::model::MediaType;
use crate::settings::{PLATFORMS, SyncState, ThemeChoice};
use crate::theme::{self, Accent, Theme, Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

/// The settings' scroll area, for the rail's jumps.
pub const SCROLL_ID: &str = "settings-scroll";
pub const SECTIONS: [&str; 5] = ["Appearance", "Library source", "Plans", "Games", "About"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// Where each section starts, from the top, and how far the page is
    /// scrolled, for the rail's current section.
    pub tops: Vec<f32>,
    pub offset: f32,
    pub at_end: bool,
    /// Disconnect, in two steps: confirm, then keep or delete the library
    /// on this computer.
    pub disconnect: Option<DisconnectStep>,
    /// The open-source licenses dialog.
    pub licenses_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectStep {
    Confirm,
    KeepOrDelete,
}

impl State {
    /// The section the rail marks: the last one scrolled past, or the
    /// last of all at the end of the page.
    pub fn current_section(&self) -> usize {
        if self.at_end && !self.tops.is_empty() {
            return self.tops.len() - 1;
        }
        self.tops
            .iter()
            .rposition(|&top| top <= self.offset + 1.0)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A rail link: scrolls to the section.
    Jump(usize),
    Scrolled {
        offset: f32,
        at_end: bool,
    },
    Measured(Vec<f32>),
    Theme(ThemeChoice),
    Flat(bool),
    SidebarTags(SidebarTags),
    TypeBoxes(bool),
    Accent(Accent),
    Repo(String),
    Branch(String),
    File(String),
    /// Nothing until phase 3.
    SyncNow,
    ReplaceToken,
    /// "Disconnect repository": the first step.
    Disconnect,
    /// The first step's Disconnect: on to keep or delete.
    ConfirmDisconnect,
    /// Keeps the library on this computer, disconnected.
    DisconnectKeep,
    /// Deletes it from this computer and opens the setup.
    DisconnectDelete,
    CloseDisconnect,
    /// "Library not found": the setup's file step with the repo kept.
    PickFile,
    /// Local mode's "Connect to GitHub".
    Connect,
    OpenLicenses,
    CloseLicenses,
    /// A data source's row: its website.
    OpenSource(&'static str),
    /// With no repository: opens the setup's Connect step.
    SetUp,
    Timeline(bool),
    Seasonal(bool),
    Platform(Option<String>),
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Settings(message)
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    let page = &mut app.settings_page;
    match message {
        Message::Jump(index) => {
            if let Some(&top) = page.tops.get(index) {
                let offset = scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(top),
                };
                return operation::scroll_to(SCROLL_ID, offset);
            }
        }
        Message::Scrolled { offset, at_end } => {
            page.offset = offset;
            page.at_end = at_end;
        }
        Message::Measured(tops) => page.tops = tops,
        Message::Theme(choice) => app.settings.theme = choice,
        Message::Flat(flat) => app.settings.flat = flat,
        Message::SidebarTags(mode) => app.library.sidebar_tags = mode,
        Message::TypeBoxes(on) => app.library.type_boxes = on,
        Message::Accent(accent) => app.settings.accent = accent,
        Message::Repo(value) => {
            if let Some(source) = &mut app.source {
                source.repo = value;
            }
        }
        Message::Branch(value) => {
            if let Some(source) = &mut app.source {
                source.branch = value;
            }
        }
        Message::File(value) => {
            if let Some(source) = &mut app.source {
                source.file = value;
            }
        }
        Message::SyncNow => {}
        Message::ReplaceToken => return setup::update(app, setup::Message::ReplaceToken),
        Message::Disconnect => page.disconnect = Some(DisconnectStep::Confirm),
        Message::ConfirmDisconnect => page.disconnect = Some(DisconnectStep::KeepOrDelete),
        Message::DisconnectKeep => {
            // The token, the queue and the sync meta go; the library stays
            // and can be browsed and edited until a repo is connected.
            page.disconnect = None;
            app.source = None;
            app.kept_locally = true;
            app.sync = SyncState::Synced;
            app.conflict_open = false;
            app.screen = super::Screen::Library;
        }
        Message::DisconnectDelete => {
            page.disconnect = None;
            app.source = None;
            app.kept_locally = false;
            app.sync = SyncState::Synced;
            app.conflict_open = false;
            app.setup = setup::State::at(setup::Step::Welcome, None);
            app.screen = super::Screen::Setup;
        }
        Message::CloseDisconnect => page.disconnect = None,
        Message::PickFile => {
            app.setup = setup::State::at(setup::Step::File, app.source.as_ref());
            app.screen = super::Screen::Setup;
        }
        Message::Connect | Message::SetUp => {
            app.setup = setup::State::at(setup::Step::Connect, None);
            app.screen = super::Screen::Setup;
        }
        Message::OpenLicenses => page.licenses_open = true,
        Message::CloseLicenses => page.licenses_open = false,
        Message::OpenSource(url) => crate::app::open_url(url),
        Message::Timeline(on) => app.settings.timeline = on,
        Message::Seasonal(on) => app.settings.seasonal = on,
        Message::Platform(platform) => app.settings.platform = platform,
    }
    Task::none()
}

/// Esc: closes the licenses or the Disconnect dialog. False when neither
/// was open.
pub fn dismiss_top(app: &mut App) -> bool {
    let page = &mut app.settings_page;
    std::mem::take(&mut page.licenses_open) || page.disconnect.take().is_some()
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let header = container(
        row![
            ui::heading("Settings", &tokens),
            space::horizontal(),
            text("Changes apply right away").size(13).color(tokens.sub),
        ]
        .align_y(Center),
    )
    .height(64)
    .padding(Padding::new(0.0).left(40).right(28));

    let sections = ui::Groups::new(
        [
            appearance(app, &tokens),
            source(app, &tokens),
            plans(app, &tokens),
            games(app, &tokens),
            about(app, &tokens),
        ],
        |measure| msg(Message::Measured(measure.tops)),
    );
    let scroll = ui::scroll(
        container(container(sections).max_width(860))
            .width(Fill)
            .padding(Padding::new(0.0).left(40).right(40).bottom(48)),
    )
    .id(SCROLL_ID)
    .on_scroll(|viewport| {
        let offset = viewport.absolute_offset().y;
        let at_end = offset + viewport.bounds().height >= viewport.content_bounds().height - 1.0;
        msg(Message::Scrolled { offset, at_end })
    })
    .height(Fill);
    let body = row![scroll, rail(app, &tokens)];
    let content = column![header, body];

    let page = &app.settings_page;
    let (dialog, close) = match page.disconnect {
        Some(step) => (
            Some(disconnect_dialog(app, step, &tokens)),
            Message::CloseDisconnect,
        ),
        None if page.licenses_open => (Some(licenses_dialog(&tokens)), Message::CloseLicenses),
        None => (None, Message::CloseDisconnect),
    };
    ui::modal(shell(app, content), dialog, msg(close), &tokens)
}

/// "On this page": one link per section, the current one marked.
fn rail<'a>(app: &App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let current = app.settings_page.current_section();
    let mut links = Column::new().spacing(2).push(
        container(
            text("ON THIS PAGE")
                .size(11)
                .font(font::SEMIBOLD)
                .color(tokens.sub),
        )
        .padding(Padding::new(0.0).left(10).right(10).bottom(8)),
    );
    for (index, name) in SECTIONS.into_iter().enumerate() {
        let on = index == current;
        let weight = if on { font::SEMIBOLD } else { font::REGULAR };
        // The links are in the text color, so they stand apart from the
        // gray header; the button centers its text itself, since a
        // button lays its content out at the top.
        let link = text(name).size(13.5).font(weight).color(tokens.text);
        links = links.push(
            button(container(link).align_y(Center).height(Fill))
                .width(Fill)
                .height(30)
                .padding([0, 10])
                .on_press(msg(Message::Jump(index)))
                .style(ui::row_style(tokens, on, 7.0))
                .focusable(7.0, tokens),
        );
    }
    container(links)
        .width(220)
        .padding(Padding::new(0.0).top(8).right(28))
        .into()
}

/// A section: its title, then its settings `gap` apart, with the 44 px
/// gap to the next section below.
fn section<'a>(
    title: &'a str,
    rows: Vec<Element<'a, AppMessage>>,
    gap: f32,
    top: f32,
    bottom: f32,
) -> Element<'a, AppMessage> {
    container(
        column![
            text(title).size(20).font(font::EXTRABOLD),
            Column::with_children(rows).spacing(gap)
        ]
        .spacing(gap),
    )
    .width(Fill)
    .padding(Padding::new(0.0).top(top).bottom(bottom))
    .into()
}

/// A setting: its name and help in a 230 px column, the control beside.
fn setting<'a>(
    name: &'a str,
    help: String,
    control: impl Into<Element<'a, AppMessage>>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let words = column![
        text(name).size(14.5).font(font::SEMIBOLD),
        text(help).size(13).line_height(1.45).color(tokens.sub),
    ]
    .spacing(4)
    .width(230);
    row![words, control.into()]
        .spacing(28)
        .align_y(Vertical::Top)
        .into()
}

fn appearance<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let settings = &app.settings;
    let flat = settings.flat;
    let style = segment(
        [
            ("Two-tone", !flat, Message::Flat(false)),
            ("Flat", flat, Message::Flat(true)),
        ],
        tokens,
    );
    let mode = app.library.sidebar_tags;
    let tags = segment(
        [
            (
                "Pinned",
                mode == SidebarTags::Pinned,
                Message::SidebarTags(SidebarTags::Pinned),
            ),
            (
                "Most used",
                mode == SidebarTags::MostUsed,
                Message::SidebarTags(SidebarTags::MostUsed),
            ),
        ],
        tokens,
    );
    let boxes = app.library.type_boxes;
    let type_boxes = segment(
        [
            ("Off", !boxes, Message::TypeBoxes(false)),
            ("On", boxes, Message::TypeBoxes(true)),
        ],
        tokens,
    );
    let tag_help = match mode {
        SidebarTags::Pinned => "The sidebar shows the tags you pin, up to 8.",
        SidebarTags::MostUsed => "The sidebar shows your 6 most-used tags and updates on its own.",
    };
    let box_help = if boxes {
        "Each type in the sidebar gets a checkbox in its color: the box adds or removes the type, the name shows only that type. Ctrl+click works too."
    } else {
        "Click a type to show only it; Ctrl+click adds or removes more. Picked types also show as chips, and Filter has a Type section."
    };
    let accents = column![
        accent_group("Vivid", &theme::VIVID, app, tokens),
        container(accent_group("Catppuccin", &theme::CATPPUCCIN, app, tokens))
            .padding(Padding::new(0.0).top(4)),
    ]
    .spacing(14);
    section(
        "Appearance",
        vec![
            setting(
                "Theme",
                settings.theme.help(app.plasma_dark),
                theme_cards(app, tokens),
                tokens,
            ),
            setting(
                "Background style",
                settings.flat_help(tokens.light).into(),
                style,
                tokens,
            ),
            setting("Sidebar tags", tag_help.into(), tags, tokens),
            setting("Type checkboxes", box_help.into(), type_boxes, tokens),
            setting(
                "Accent color",
                "Buttons, progress, selection and the pinned-tag marks. Text and icons on the accent are always plain black.".into(),
                accents,
                tokens,
            ),
        ],
        26.0,
        8.0,
        44.0,
    )
}

/// Two joined choices, the picked one in the selection gray.
fn segment<'a>(items: [(&'a str, bool, Message); 2], tokens: &Tokens) -> Element<'a, AppMessage> {
    container(ui::segmented(
        items.map(|(label, on, message)| (label, on, msg(message))),
        36.0,
        tokens,
    ))
    .width(220)
    .into()
}

/// The five theme cards, three to a row, each with a small preview.
fn theme_cards<'a>(app: &App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let mut rows = Column::new().spacing(14);
    for chunk in ThemeChoice::ALL.chunks(3) {
        let mut cards = Row::new().spacing(14);
        for choice in chunk {
            cards = cards.push(theme_card(*choice, app, tokens));
        }
        for _ in chunk.len()..3 {
            cards = cards.push(space().width(Fill));
        }
        rows = rows.push(cards);
    }
    rows.into()
}

fn theme_card<'a>(choice: ThemeChoice, app: &App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let on = app.settings.theme == choice;
    let words = column![
        text(choice.label()).size(14).font(font::SEMIBOLD),
        text(choice.blurb()).size(12.5).color(tokens.sub),
    ]
    .spacing(2)
    .padding([0, 2]);
    button(column![theme_preview(choice, &tokens), words].spacing(9))
        .width(Fill)
        .padding(8)
        .on_press(msg(Message::Theme(choice)))
        .style(move |_, status| button::Style {
            background: match status {
                _ if on => Some(Background::Color(tokens.selection)),
                button::Status::Hovered => Some(Background::Color(tokens.hover)),
                _ => None,
            },
            text_color: tokens.text,
            border: Border::default().rounded(12),
            ..button::Style::default()
        })
        .focusable(12.0, &tokens)
}

/// Which part of a theme's miniature a preview shows: the whole window,
/// or the left or right half for the System card's split.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Whole,
    Left,
    Right,
}

/// An 86 px miniature of the window in the theme: a sidebar with three
/// bars and a content pane with three covers. System is Graphite on the
/// left and Light on the right.
fn theme_preview<'a>(choice: ThemeChoice, tokens: &Tokens) -> Element<'a, AppMessage> {
    let ring = if tokens.light {
        Color::from_rgb8(0xE2, 0xE4, 0xE8)
    } else {
        tokens.line
    };
    let inner: Element<'a, AppMessage> = match choice {
        ThemeChoice::Fixed(theme) => miniature(Tokens::new(theme, theme::PEACH), Part::Whole),
        ThemeChoice::System => row![
            miniature(Tokens::new(Theme::Graphite, theme::PEACH), Part::Left),
            miniature(Tokens::new(Theme::Light, theme::PEACH), Part::Right),
        ]
        .into(),
    };
    container(inner)
        .width(Fill)
        .height(86)
        .clip(true)
        .style(move |_| {
            container::Style::default().border(Border::default().rounded(10).width(1).color(ring))
        })
        .into()
}

fn miniature<'a>(t: Tokens, part: Part) -> Element<'a, AppMessage> {
    const TILES: [u32; 3] = [0x4B5F8A, 0x8A6E3F, 0x3F8A6E];
    let faint = if t.light {
        Color::from_rgba8(0, 0, 0, 0.12)
    } else {
        Color::from_rgba8(255, 255, 255, 0.16)
    };
    let bar = |portion: u16, rest: u16, color: Color| {
        row![
            container(space())
                .width(FillPortion(portion))
                .height(5)
                .style(move |_| container::background(color).border(Border::default().rounded(3))),
            space().width(FillPortion(rest)),
        ]
    };
    let sidebar = container(
        column![
            bar(7, 3, t.type_color(MediaType::Movies)),
            bar(11, 9, faint),
            bar(31, 19, faint),
        ]
        .spacing(6),
    )
    .padding([12, 8]);
    let tiles = match part {
        Part::Whole => &TILES[..],
        Part::Left => &TILES[..1],
        Part::Right => &TILES[1..],
    };
    let tiles = Row::with_children(tiles.iter().map(|&tile| {
        let color = Color::from_rgb8((tile >> 16) as u8, (tile >> 8) as u8, tile as u8);
        container(space())
            .width(Fill)
            .height(Fill)
            .style(move |_| container::background(color).border(Border::default().rounded(4)))
            .into()
    }))
    .spacing(6);
    let content = t.content;
    let radius = if part == Part::Right {
        Border::default()
    } else {
        Border {
            radius: iced::border::Radius::default().top_left(8),
            ..Border::default()
        }
    };
    let pane = container(
        container(tiles)
            .padding(10)
            .height(Fill)
            .width(Fill)
            .style(move |_| container::background(content).border(radius)),
    )
    .padding(Padding::new(0.0).top(10))
    .height(Fill)
    .width(Fill);
    let layout: Element<'a, AppMessage> = match part {
        Part::Whole => row![sidebar.width(FillPortion(3)), pane.width(FillPortion(7))].into(),
        Part::Left => row![sidebar.width(FillPortion(3)), pane.width(FillPortion(2))].into(),
        Part::Right => pane.into(),
    };
    let chrome = t.chrome;
    container(layout)
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(chrome))
        .into()
}

/// A named group of accent swatches, wrapping as needed.
fn accent_group<'a>(
    name: &'a str,
    accents: &'static [Accent],
    app: &App,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let theme = app.theme();
    let current = app.settings.accent;
    let swatches = accents.iter().map(|&accent| {
        let tokens = *tokens;
        let on = accent == current;
        let color = accent.for_theme(theme).fill;
        let disc = container(space()).width(30).height(30).style(move |_| {
            let mut style = container::background(color).border(Border::default().rounded(15));
            if tokens.light {
                style.border = style
                    .border
                    .width(1)
                    .color(Color::from_rgba8(0, 0, 0, 0.08));
            }
            style
        });
        let (fg, weight) = if on {
            (tokens.text, font::SEMIBOLD)
        } else {
            (tokens.sub, font::MEDIUM)
        };
        button(
            column![disc, text(accent.name).size(12).font(weight).color(fg)]
                .spacing(6)
                .width(Fill)
                .align_x(Center),
        )
        .width(74)
        .padding(Padding::new(0.0).top(8).bottom(6))
        .on_press(msg(Message::Accent(accent)))
        .style(move |_, status| button::Style {
            background: match status {
                _ if on => Some(Background::Color(tokens.selection)),
                button::Status::Hovered => Some(Background::Color(tokens.hover)),
                _ => None,
            },
            text_color: fg,
            border: Border::default().rounded(10),
            ..button::Style::default()
        })
        .focusable(10.0, &tokens)
    });
    column![
        text(name.to_uppercase())
            .size(11)
            .font(font::SEMIBOLD)
            .color(tokens.sub),
        Row::with_children(swatches).spacing(6).wrap(),
    ]
    .spacing(10)
    .into()
}

/// Library source: the connection's state, its fields, the token and
/// Disconnect; or Set up when nothing is connected.
fn source<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let library = app.repo.library();
    let control: Element<'a, AppMessage> = match &app.source {
        Some(source) => {
            let (fill, dot, title, detail, action) = match app.sync {
                SyncState::TokenExpired => (
                    tokens.danger_fill,
                    tokens.danger,
                    "Token expired",
                    "Libri can't sync until you add a new token.".to_owned(),
                    ui::sized_button(
                        "Replace token",
                        ButtonKind::Primary,
                        32.0,
                        *tokens,
                        Some(msg(Message::ReplaceToken)),
                    ),
                ),
                SyncState::Conflict { .. } => (
                    tokens.danger_fill,
                    tokens.danger,
                    "Changed here and on GitHub",
                    "Pick which copy to keep".to_owned(),
                    ui::sized_button(
                        "Choose",
                        ButtonKind::Outline,
                        32.0,
                        *tokens,
                        Some(AppMessage::Sync(crate::app::SyncMessage::OpenConflict)),
                    ),
                ),
                SyncState::NotFound(missing) => (
                    tokens.danger_fill,
                    tokens.danger,
                    "Library not found on GitHub",
                    format!(
                        "{} Pick the file again, or restore it on GitHub.",
                        SyncState::missing_text(missing, Some(source))
                    ),
                    ui::sized_button(
                        "Pick a file",
                        ButtonKind::Outline,
                        32.0,
                        *tokens,
                        Some(msg(Message::PickFile)),
                    ),
                ),
                SyncState::Failed { kept } => (
                    tokens.danger_fill,
                    tokens.danger,
                    "Could not save",
                    format!(
                        "GitHub didn't answer. {kept} {} kept on this computer; Libri tries again by itself.",
                        if kept == 1 {
                            "change is"
                        } else {
                            "changes are"
                        }
                    ),
                    ui::sized_button(
                        "Retry",
                        ButtonKind::Outline,
                        32.0,
                        *tokens,
                        Some(AppMessage::Sync(crate::app::SyncMessage::Retry)),
                    ),
                ),
                SyncState::Offline { kept } => (
                    tokens.raised,
                    tokens.sub,
                    "Offline",
                    format!(
                        "{kept} {} kept on this computer and saved when you're back online.",
                        if kept == 1 {
                            "change is"
                        } else {
                            "changes are"
                        }
                    ),
                    ui::sized_button(
                        "Sync now",
                        ButtonKind::Outline,
                        32.0,
                        *tokens,
                        Some(msg(Message::SyncNow)),
                    ),
                ),
                sync => (
                    tokens.success_fill,
                    tokens.success,
                    "Connected",
                    format!(
                        "Last synced 2 min ago · {} entries · {} plans{}",
                        thousands(library.entries.len()),
                        library.plans.len(),
                        match sync {
                            SyncState::ExpiresIn(days) =>
                                format!(" · token expires in {days} days"),
                            _ => String::new(),
                        }
                    ),
                    ui::sized_button(
                        "Sync now",
                        ButtonKind::Outline,
                        32.0,
                        *tokens,
                        Some(msg(Message::SyncNow)),
                    ),
                ),
            };
            let card = container(
                row![
                    ui::dot(dot, 9.0, 4.5),
                    column![
                        text(title).size(14).font(font::SEMIBOLD),
                        text(detail).size(12.5).color(tokens.sub)
                    ]
                    .spacing(1)
                    .width(Fill),
                    action,
                ]
                .spacing(12)
                .align_y(Center),
            )
            .padding([12, 14])
            .style(move |_| container::background(fill).border(Border::default().rounded(10)));

            let field = |label, value: &str, make: fn(String) -> Message| {
                ui::labeled(
                    label,
                    ui::text_field("", value, move |v| msg(make(v)), font::MONO, tokens),
                    tokens,
                )
            };
            let (field_bg, line) = (tokens.field, tokens.line);
            let token = row![
                container(ui::mono(source.token_hint.clone(), 14.0, tokens.sub))
                    .width(Fill)
                    .height(38)
                    .padding([0, 12])
                    .align_y(Center)
                    .style(move |_| container::background(field_bg)
                        .border(Border::default().rounded(8).width(1).color(line))),
                ui::sized_button(
                    "Replace",
                    ButtonKind::Outline,
                    38.0,
                    *tokens,
                    Some(msg(Message::ReplaceToken)),
                ),
            ]
            .spacing(8);
            let fields = column![
                row![
                    field("Repository", &source.repo, Message::Repo),
                    field("Branch", &source.branch, Message::Branch),
                ]
                .spacing(14),
                row![
                    field("Library file", &source.file, Message::File),
                    ui::labeled("Access token", token, tokens),
                ]
                .spacing(14),
            ]
            .spacing(14);
            column![
                card,
                fields,
                text("The token is stored in your KDE Wallet, not in a config file.")
                    .size(12.5)
                    .color(tokens.sub),
                row![ui::sized_button(
                    "Disconnect repository",
                    ButtonKind::Destructive,
                    34.0,
                    *tokens,
                    Some(msg(Message::Disconnect)),
                )],
            ]
            .spacing(14)
            .into()
        }
        None => {
            let raised = tokens.raised;
            // Disconnected with the library kept: local mode, with the way
            // back to GitHub; else nothing connected at all.
            let (detail, action, message) = if app.kept_locally {
                (
                    "Your library is kept on this computer",
                    "Connect to GitHub",
                    Message::Connect,
                )
            } else {
                (
                    "Connect a GitHub repo to sync your library.",
                    "Set up",
                    Message::SetUp,
                )
            };
            let card = container(
                row![
                    ui::dot(tokens.sub, 9.0, 4.5),
                    column![
                        text("Not connected").size(14).font(font::SEMIBOLD),
                        text(detail).size(12.5).color(tokens.sub)
                    ]
                    .spacing(1)
                    .width(Fill),
                    ui::sized_button(
                        action,
                        ButtonKind::Primary,
                        32.0,
                        *tokens,
                        Some(msg(message))
                    ),
                ]
                .spacing(12)
                .align_y(Center),
            )
            .padding([12, 14])
            .style(move |_| container::background(raised).border(Border::default().rounded(10)));
            if app.kept_locally {
                column![
                    card,
                    text("You can keep browsing and editing. Changes stay on this computer until you connect a repo again; connecting then asks which library to keep.")
                        .size(12.5)
                        .line_height(1.45)
                        .color(tokens.sub),
                ]
                .spacing(12)
                .into()
            } else {
                card.into()
            }
        }
    };
    section(
        "Library source",
        vec![setting(
            "GitHub repository",
            "Where library.json lives. Libri saves in the background and bundles quick edits into one commit.".into(),
            control,
            tokens,
        )],
        22.0,
        0.0,
        44.0,
    )
}

/// Plans: the timeline default and the seasonal touches.
fn plans<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let settings = &app.settings;
    section(
        "Plans",
        vec![
            switch_row(
                "Timeline by default",
                settings.timeline_help(),
                settings.timeline,
                tokens,
                msg(Message::Timeline(!settings.timeline)),
            ),
            switch_row(
                "Seasonal touches",
                settings.seasonal_help(),
                settings.seasonal,
                tokens,
                msg(Message::Seasonal(!settings.seasonal)),
            ),
        ],
        22.0,
        0.0,
        44.0,
    )
}

/// A setting with a switch and its help beside it; the whole control
/// toggles it.
fn switch_row<'a>(
    name: &'a str,
    help: &'a str,
    on: bool,
    tokens: &Tokens,
    on_toggle: AppMessage,
) -> Element<'a, AppMessage> {
    let text_color = tokens.text;
    let control = button(
        row![
            ui::switch::track(on, 38.0, tokens),
            text(help)
                .size(13)
                .line_height(1.45)
                .color(tokens.sub)
                .width(Fill),
        ]
        .spacing(14)
        .align_y(Vertical::Top),
    )
    .width(Fill)
    .padding(0)
    .on_press(on_toggle)
    .style(move |_, _| button::Style {
        text_color,
        ..button::Style::default()
    })
    .focusable(6.0, tokens);
    row![
        container(text(name).size(14.5).font(font::SEMIBOLD))
            .width(230)
            .padding(Padding::new(0.0).top(2)),
        control
    ]
    .spacing(28)
    .align_y(Vertical::Top)
    .into()
}

/// Games: the default platform as pills.
fn games<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let current = app.settings.platform.as_deref();
    let pills = PLATFORMS
        .iter()
        .map(|&platform| pill(platform, current == Some(platform), tokens, Some(platform)))
        .chain(std::iter::once(pill(
            "None",
            current.is_none(),
            tokens,
            None,
        )));
    section(
        "Games",
        vec![setting(
            "Default platform",
            "Pre-selected when you add a game.".into(),
            Row::with_children(pills).spacing(8).wrap(),
            tokens,
        )],
        22.0,
        0.0,
        44.0,
    )
}

/// A 32 px pill: the accent when picked, an outline otherwise.
fn pill<'a>(
    label: &'a str,
    on: bool,
    tokens: &Tokens,
    platform: Option<&'a str>,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(
        container(text(label).size(13).font(font::SEMIBOLD))
            .height(Fill)
            .align_y(Center),
    )
    .height(32)
    .padding([0, 14])
    .on_press(msg(Message::Platform(platform.map(str::to_owned))))
    .style(move |_, status| button::Style {
        background: match status {
            _ if on => Some(Background::Color(tokens.accent)),
            button::Status::Hovered => Some(Background::Color(tokens.hover)),
            _ => None,
        },
        text_color: if on { tokens.on_accent } else { tokens.sub },
        border: Border::default().rounded(16).width(1).color(if on {
            tokens.accent
        } else {
            tokens.line
        }),
        ..button::Style::default()
    })
    .focusable(16.0, &tokens)
}

/// A service Libri asks, credited in About: its name, what it is used
/// for, its website and its logo where the service's terms want one.
struct DataSource {
    name: &'static str,
    text: &'static str,
    url: &'static str,
    logo: Option<Logo>,
}

#[derive(Clone, Copy)]
enum Logo {
    Tmdb,
    Deezer,
}

/// The only place a provider is named in the UI. TMDB's terms want its
/// logo and the exact sentence; Deezer's want its logo, in the light or
/// dark variant.
const DATA_SOURCES: [DataSource; 8] = [
    DataSource {
        name: "TMDB",
        text: "Movies and shows; also anime seasons and episodes, behind the scenes. This product uses the TMDB API but is not endorsed or certified by TMDB.",
        url: "https://www.themoviedb.org",
        logo: Some(Logo::Tmdb),
    },
    DataSource {
        name: "IGDB",
        text: "Games: details and covers from IGDB.com.",
        url: "https://www.igdb.com",
        logo: None,
    },
    DataSource {
        name: "Apple Books",
        text: "Books: descriptions and covers, and books Open Library lacks.",
        url: "https://books.apple.com",
        logo: None,
    },
    DataSource {
        name: "Open Library",
        text: "Books: details and covers from the Internet Archive.",
        url: "https://openlibrary.org",
        logo: None,
    },
    DataSource {
        name: "Google Books",
        text: "Books it found before Apple Books.",
        url: "https://books.google.com",
        logo: None,
    },
    DataSource {
        name: "Deezer",
        text: "Music: albums, songs and covers.",
        url: "https://www.deezer.com",
        logo: Some(Logo::Deezer),
    },
    DataSource {
        name: "AniList",
        text: "Anime: details, covers, and seasons when TMDB has none.",
        url: "https://anilist.co",
        logo: None,
    },
    DataSource {
        name: "Wikidata",
        text: "Book series and their order.",
        url: "https://www.wikidata.org",
        logo: None,
    },
];

/// The crates and fonts Libri ships, for the licenses dialog. Phase 5
/// generates the full list with their texts at build time.
const LICENSES: [(&str, &str); 8] = [
    ("iced", "MIT"),
    (
        "wgpu, winit and the Rust ecosystem iced builds on",
        "MIT, Apache-2.0, BSD and Zlib",
    ),
    ("serde and serde_json", "MIT or Apache-2.0"),
    ("raw-window-handle", "MIT, Apache-2.0 or Zlib"),
    ("wayland-client and wayland-protocols-plasma", "MIT"),
    ("Figtree", "SIL Open Font License 1.1"),
    ("JetBrains Mono", "SIL Open Font License 1.1"),
    ("Libri", "MIT"),
];

/// A service's logo at the height its terms allow: TMDB's 13 px, Deezer's
/// 22 px, the white wordmark on the dark themes.
fn source_logo<'a>(logo: Logo, tokens: &Tokens) -> Element<'a, AppMessage> {
    use iced::widget::image;
    // The 2.0x files, so the logos stay sharp at the usual 2x scale; the
    // size is set outright since iced decodes PNGs on a worker thread and
    // cannot measure them on the first frame.
    let (bytes, width, height): (&'static [u8], f32, f32) = match (logo, tokens.light) {
        (Logo::Tmdb, _) => (
            include_bytes!("../../assets/logos/tmdb/2.0x/tmdb.png"),
            101.0,
            13.0,
        ),
        (Logo::Deezer, true) => (
            include_bytes!("../../assets/logos/deezer/2.0x/deezer.png"),
            80.0,
            22.0,
        ),
        (Logo::Deezer, false) => (
            include_bytes!("../../assets/logos/deezer/2.0x/deezer_on_dark.png"),
            80.0,
            22.0,
        ),
    };
    image(image::Handle::from_bytes(bytes))
        .width(width)
        .height(height)
        .into()
}

/// About: the icon, the name, the version and build, the licenses row and
/// the data sources, each opening its website.
fn about<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let _ = app;
    let words = column![
        text("Libri").size(20).font(font::EXTRABOLD),
        text(format!(
            "Version {} (build {}) · Rust + iced · Your data stays in your own GitHub repo",
            env!("CARGO_PKG_VERSION"),
            env!("LIBRI_BUILD")
        ))
        .size(13)
        .color(tokens.sub),
    ]
    .spacing(2);
    let identity = row![ui::logo(64.0), words].spacing(16).align_y(Center);

    let licenses = button(
        row![
            text("Open-source licenses").size(14.5).width(Fill),
            ui::icon(Icon::ChevronRight, 15.0, tokens.sub),
        ]
        .align_y(Center)
        .height(Fill),
    )
    .width(Fill)
    .height(46)
    .padding([0, 4])
    .on_press(msg(Message::OpenLicenses))
    .style(ui::row_style(tokens, false, 8.0))
    .focusable(8.0, tokens);

    let sources_head = column![
        text("Data sources").size(14.5).font(font::SEMIBOLD),
        text("Used when you add something, to fetch details and covers. Each opens the service's website.")
            .size(13)
            .color(tokens.sub),
    ]
    .spacing(4);
    let rows = DATA_SOURCES.iter().map(|source| {
        let mut words = column![text(source.name).size(14.5).font(font::SEMIBOLD)].spacing(3);
        words = words.push(
            text(source.text)
                .size(12.5)
                .line_height(1.4)
                .color(tokens.sub),
        );
        if let Some(logo) = source.logo {
            words = words
                .push(container(source_logo(logo, tokens)).padding(iced::Padding::new(0.0).top(4)));
        }
        button(
            row![
                words.width(Fill),
                ui::icon(Icon::ExternalLink, 15.0, tokens.sub),
            ]
            .spacing(12)
            .align_y(Center),
        )
        .width(Fill)
        .padding([10, 4])
        .on_press(msg(Message::OpenSource(source.url)))
        .style(ui::row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
    });
    let mut list = Column::new();
    for (index, source_row) in rows.enumerate() {
        if index > 0 {
            list = list.push(ui::rule(tokens.line.scale_alpha(0.6)));
        }
        list = list.push(source_row);
    }
    section(
        "About Libri",
        vec![
            identity.into(),
            column![ui::rule(tokens.line), licenses, ui::rule(tokens.line)].into(),
            column![sources_head, list].spacing(10).into(),
        ],
        18.0,
        0.0,
        0.0,
    )
}

/// The open-source licenses, as a list of the crates and fonts and what
/// they are under.
fn licenses_dialog<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    let rows = LICENSES.iter().map(|(name, license)| {
        row![
            text(*name).size(14).width(Fill),
            text(*license).size(13).color(tokens.sub),
        ]
        .spacing(12)
        .into()
    });
    let body = column![
        text("Libri is open source and builds on these.").size(14),
        Column::with_children(rows).spacing(8),
        text("The full license texts come with the package (phase 5).")
            .size(12.5)
            .color(tokens.sub),
    ]
    .spacing(14);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            *tokens,
            Some(msg(Message::CloseLicenses))
        ),
    ];
    ui::dialog(
        "Open-source licenses",
        body,
        footer,
        520,
        tokens,
        msg(Message::CloseLicenses),
    )
}

/// Disconnect in two steps: "Disconnect from GitHub?" (Cancel /
/// Disconnect), then "Keep your library on this computer?" (Cancel /
/// Delete it from this computer / Keep it on this computer).
fn disconnect_dialog<'a>(
    app: &'a App,
    step: DisconnectStep,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let repo = app
        .source
        .as_ref()
        .map_or("the repository", |source| source.repo.as_str());
    match step {
        DisconnectStep::Confirm => {
            let body = column![
                text(format!(
                    "Libri stops syncing with {repo}. Next you choose what happens to the library on this computer."
                ))
                .size(14),
                text("The repo on GitHub is never touched.")
                    .size(13)
                    .color(tokens.sub),
            ]
            .spacing(10);
            let footer = row![
                space::horizontal(),
                ui::labeled_button(
                    "Cancel",
                    ButtonKind::Outline,
                    *tokens,
                    Some(msg(Message::CloseDisconnect))
                ),
                ui::labeled_button(
                    "Disconnect",
                    ButtonKind::Destructive,
                    *tokens,
                    Some(msg(Message::ConfirmDisconnect))
                ),
            ]
            .spacing(8);
            ui::dialog(
                "Disconnect from GitHub?",
                body,
                footer,
                480,
                tokens,
                msg(Message::CloseDisconnect),
            )
        }
        DisconnectStep::KeepOrDelete => {
            let warning = row![
                ui::dot(tokens.danger, 9.0, 4.5),
                text("If you delete it, Libri is empty until you connect a GitHub repo again. Your library on GitHub is not affected.")
                    .size(13)
                    .width(Fill),
            ]
            .spacing(10)
            .align_y(Center);
            let body = column![
                text("You can keep browsing and editing it. Changes stay on this computer until you connect a repo again.")
                    .size(14),
                warning,
            ]
            .spacing(12);
            let footer = row![
                ui::labeled_button(
                    "Cancel",
                    ButtonKind::Text,
                    *tokens,
                    Some(msg(Message::CloseDisconnect))
                ),
                space::horizontal(),
                ui::labeled_button(
                    "Delete it from this computer",
                    ButtonKind::Destructive,
                    *tokens,
                    Some(msg(Message::DisconnectDelete))
                ),
                ui::labeled_button(
                    "Keep it on this computer",
                    ButtonKind::Neutral,
                    *tokens,
                    Some(msg(Message::DisconnectKeep))
                ),
            ]
            .spacing(8)
            .align_y(Center);
            ui::dialog(
                "Keep your library on this computer?",
                body,
                footer,
                540,
                tokens,
                msg(Message::CloseDisconnect),
            )
        }
    }
}

/// "1,284".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;

    #[test]
    fn the_rail_marks_the_section_scrolled_to() {
        let mut page = State {
            tops: vec![0.0, 400.0, 900.0, 1100.0, 1300.0],
            ..State::default()
        };
        assert_eq!(page.current_section(), 0);
        page.offset = 450.0;
        assert_eq!(page.current_section(), 1);
        page.at_end = true;
        assert_eq!(page.current_section(), 4);
    }

    #[test]
    fn settings_change_the_app_at_once() {
        let mut app = App::new(Screen::Settings);
        let _ = update(&mut app, Message::Theme(ThemeChoice::Fixed(Theme::Light)));
        let _ = update(&mut app, Message::Flat(true));
        let _ = update(&mut app, Message::SidebarTags(SidebarTags::MostUsed));
        let _ = update(&mut app, Message::TypeBoxes(true));
        let _ = update(&mut app, Message::Timeline(true));
        let _ = update(&mut app, Message::Seasonal(false));
        let _ = update(&mut app, Message::Platform(None));
        assert_eq!(app.theme(), Theme::Light);
        assert_eq!(app.tokens().chrome, Color::WHITE);
        assert_eq!(app.library.sidebar_tags, SidebarTags::MostUsed);
        assert!(app.library.type_boxes);
        assert!(app.settings.timeline);
        assert!(!app.settings.seasonal);
        assert_eq!(app.settings.platform, None);
    }

    #[test]
    fn disconnecting_takes_two_steps_and_can_keep_the_library_on_this_computer() {
        let mut app = App::new(Screen::Settings);
        let _ = update(&mut app, Message::Disconnect);
        assert_eq!(app.settings_page.disconnect, Some(DisconnectStep::Confirm));
        assert!(dismiss_top(&mut app));
        assert!(app.source.is_some(), "Cancel changes nothing");
        let _ = update(&mut app, Message::Disconnect);
        let _ = update(&mut app, Message::ConfirmDisconnect);
        assert_eq!(
            app.settings_page.disconnect,
            Some(DisconnectStep::KeepOrDelete)
        );
        assert!(app.source.is_some(), "not before the second step");
        let _ = update(&mut app, Message::DisconnectKeep);
        assert!(app.source.is_none() && app.kept_locally);
        assert_eq!(app.screen, Screen::Library);
        assert!(
            !app.repo.library().entries.is_empty(),
            "the library stays and can be browsed"
        );
        let _ = update(&mut app, Message::Connect);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.step, setup::Step::Connect);
        assert!(app.setup.repo.is_empty());
    }

    #[test]
    fn deleting_the_library_opens_the_first_run_and_the_states_have_their_actions() {
        let mut app = App::new(Screen::Settings);
        let _ = update(&mut app, Message::Disconnect);
        let _ = update(&mut app, Message::ConfirmDisconnect);
        let _ = update(&mut app, Message::DisconnectDelete);
        assert!(app.source.is_none() && !app.kept_locally);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.step, setup::Step::Welcome);

        let mut app = App::new(Screen::Settings);
        app.sync = SyncState::NotFound(crate::settings::Missing::File);
        let _ = update(&mut app, Message::PickFile);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.step, setup::Step::File);
        assert_eq!(app.setup.repo, "ekmanch/library", "the repo is kept");

        let mut app = App::new(Screen::Settings);
        let _ = update(&mut app, Message::OpenLicenses);
        assert!(app.settings_page.licenses_open);
        assert!(dismiss_top(&mut app));
        assert!(!app.settings_page.licenses_open);
    }

    #[test]
    fn counts_get_thousands_separators() {
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(1284), "1,284");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }
}
