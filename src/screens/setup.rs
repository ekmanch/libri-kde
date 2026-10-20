//! First-time setup, the whole window (docs/DESIGN.md 4.2): Welcome,
//! Connect GitHub, Branch, Library file, Create or Pick, All set. The steps
//! rail sits where the sidebar normally is, the form in the content pane,
//! Back and the main action pinned at the bottom. UI only, on fixture
//! branches and files, until phase 3 talks to GitHub.

use iced::alignment::{Horizontal, Vertical};
use iced::border::Radius;
use iced::widget::{Column, button, column, container, rich_text, row, space, span, text};
use iced::{Background, Border, Center, Element, Fill, Padding, Task};

use super::Screen;
use crate::app::{self, App, Message as AppMessage};
use crate::settings::{Source, SyncState};
use crate::theme::{Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

/// GitHub's guide to fine-grained tokens, for "How do I create a token?".
pub const TOKEN_HELP_URL: &str = "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-fine-grained-personal-access-token";

/// What setup commits for a new library.
const EMPTY_LIBRARY: &str =
    "{\n  \"version\": 2,\n  \"tags\": {},\n  \"entries\": [],\n  \"plans\": []\n}";

/// A fixture token, so the field shows something before phase 3.
const FIXTURE_TOKEN: &str = "github_pat_11A2B3C4D5E6F7G8H9I0J1K2L3M4N5O6a91F";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    #[default]
    Welcome,
    Connect,
    Branch,
    File,
    /// Create a new library file (the choice on File).
    Create,
    /// Pick an existing file (the other choice).
    Existing,
    Done,
}

impl Step {
    pub const ALL: [Step; 7] = [
        Step::Welcome,
        Step::Connect,
        Step::Branch,
        Step::File,
        Step::Create,
        Step::Existing,
        Step::Done,
    ];

    /// The `LIBRI_SETUP` value, also used in screenshot file names.
    pub fn name(self) -> &'static str {
        match self {
            Step::Welcome => "welcome",
            Step::Connect => "connect",
            Step::Branch => "branch",
            Step::File => "file",
            Step::Create => "create",
            Step::Existing => "existing",
            Step::Done => "done",
        }
    }

    pub fn from_name(name: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|step| step.name() == name)
    }

    /// The step's place on the rail, 0 to 3; Welcome and Done are outside
    /// it.
    fn index(self) -> Option<usize> {
        match self {
            Step::Welcome | Step::Done => None,
            Step::Connect => Some(0),
            Step::Branch => Some(1),
            Step::File => Some(2),
            Step::Create | Step::Existing => Some(3),
        }
    }

    fn back(self) -> Step {
        match self {
            Step::Branch => Step::Connect,
            Step::File => Step::Branch,
            Step::Create | Step::Existing => Step::File,
            _ => Step::Welcome,
        }
    }
}

/// The Library file step's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    New,
    Existing,
}

/// A branch of the repository, as the fixture lists it.
pub struct Branch {
    pub name: &'static str,
    pub info: &'static str,
    pub default: bool,
}

pub const BRANCHES: [Branch; 3] = [
    Branch {
        name: "main",
        info: "Updated 2 days ago",
        default: true,
    },
    Branch {
        name: "app-sync-test",
        info: "Updated last week",
        default: false,
    },
    Branch {
        name: "dev",
        info: "Updated in August",
        default: false,
    },
];

/// A JSON file in the repository; only Libri's own format can be picked.
pub struct File {
    pub path: &'static str,
    pub info: &'static str,
    pub usable: bool,
}

pub const FILES: [File; 3] = [
    File {
        path: "library.json",
        info: "Libri library · 214 entries · updated 2 days ago",
        usable: true,
    },
    File {
        path: "backup/library-2025.json",
        info: "Libri library · 168 entries · updated in March",
        usable: true,
    },
    File {
        path: "data/old-lists.json",
        info: "Not a Libri library file",
        usable: false,
    },
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub step: Step,
    pub repo: String,
    pub token: String,
    pub branch: usize,
    pub choice: Choice,
    pub file: usize,
    /// The name of a new library file.
    pub file_name: String,
    /// Replacing the token from the warning: the Connect step alone, then
    /// back to this screen.
    pub replacing: Option<Screen>,
}

impl State {
    /// The wizard at `step`, filled in from `source` when there is one
    /// (`LIBRI_SETUP` and the screen captures).
    pub fn at(step: Step, source: Option<&Source>) -> State {
        let mut state = State {
            step,
            file_name: "library.json".into(),
            ..State::default()
        };
        if let Some(source) = source {
            state.repo = source.repo.clone();
            state.token = FIXTURE_TOKEN.into();
            state.branch = BRANCHES
                .iter()
                .position(|b| b.name == source.branch)
                .unwrap_or(0);
            state.file = FILES
                .iter()
                .position(|f| f.path == source.file)
                .unwrap_or(0);
            state.file_name = source.file.clone();
        }
        state
    }

    /// On one of the four numbered steps, where the window is titled "Set
    /// up Libri".
    pub fn in_wizard(&self) -> bool {
        self.step.index().is_some()
    }

    fn branch_name(&self) -> &'static str {
        BRANCHES[self.branch].name
    }

    fn file_name(&self) -> String {
        match self.choice {
            Choice::New => self.file_name.clone(),
            Choice::Existing => FILES[self.file].path.to_owned(),
        }
    }

    /// Whether the step's main action can go ahead.
    pub fn can_continue(&self) -> bool {
        match self.step {
            Step::Connect => !self.repo.trim().is_empty() && !self.token.trim().is_empty(),
            Step::Create => {
                let name = self.file_name.trim();
                name.len() > 5 && name.ends_with(".json")
            }
            Step::Existing => FILES[self.file].usable,
            _ => true,
        }
    }

    fn primary_label(&self) -> &'static str {
        match self.step {
            Step::Connect => "Connect",
            Step::Create => "Create library",
            Step::Existing => "Use this file",
            _ => "Continue",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Repo(String),
    Token(String),
    PickBranch(usize),
    Choose(Choice),
    PickFile(usize),
    FileName(String),
    Back,
    /// The step's main action: Get started, Connect, Continue, Create
    /// library, Use this file.
    Continue,
    /// All set's Open Libri.
    Open,
    /// From the token warning and the sync row: the Connect step with the
    /// repository kept.
    ReplaceToken,
    /// "How do I create a token?"
    TokenHelp,
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Setup(message)
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    let state = &mut app.setup;
    match message {
        Message::Repo(value) => state.repo = value,
        Message::Token(value) => state.token = value,
        Message::PickBranch(index) => state.branch = index,
        Message::Choose(choice) => state.choice = choice,
        Message::PickFile(index) => {
            if FILES[index].usable {
                state.file = index;
            }
        }
        Message::FileName(value) => state.file_name = value,
        Message::Back => match state.replacing.take() {
            Some(screen) => app.screen = screen,
            None => state.step = state.step.back(),
        },
        Message::Continue => {
            if !state.can_continue() {
                return Task::none();
            }
            match state.step {
                Step::Welcome => state.step = Step::Connect,
                Step::Connect => match state.replacing.take() {
                    // Phase 3 checks the token; for now it just works.
                    Some(screen) => {
                        app.sync = SyncState::Synced;
                        app.screen = screen;
                    }
                    None => state.step = Step::Branch,
                },
                Step::Branch => state.step = Step::File,
                Step::File => {
                    state.step = match state.choice {
                        Choice::New => Step::Create,
                        Choice::Existing => Step::Existing,
                    }
                }
                Step::Create | Step::Existing => state.step = Step::Done,
                Step::Done => return update(app, Message::Open),
            }
        }
        Message::Open => {
            app.source = Some(Source {
                repo: state.repo.trim().to_owned(),
                branch: state.branch_name().to_owned(),
                file: state.file_name(),
                token_hint: token_hint(&state.token),
            });
            app.screen = Screen::Library;
        }
        Message::ReplaceToken => {
            let from = match app.screen {
                Screen::Setup => Screen::Library,
                screen => screen,
            };
            *state = State::at(Step::Connect, app.source.as_ref());
            state.token.clear();
            state.replacing = Some(from);
            app.screen = Screen::Setup;
        }
        Message::TokenHelp => app::open_url(TOKEN_HELP_URL),
    }
    Task::none()
}

/// "github_pat_••••••a91F": the token's prefix and last four characters.
fn token_hint(token: &str) -> String {
    let token = token.trim();
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let prefix = if token.starts_with("github_pat_") {
        "github_pat_"
    } else {
        ""
    };
    format!("{prefix}••••••{tail}")
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let state = &app.setup;
    let body = match state.step {
        Step::Welcome => welcome(&tokens),
        Step::Done => done(state, &tokens),
        _ => wizard(app, &tokens),
    };
    let chrome = tokens.chrome;
    container(body)
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(chrome).color(tokens.text))
        .into()
}

/// Logo, "Welcome to Libri", a line about the app and Get started.
fn welcome<'a>(tokens: &Tokens) -> Element<'a, AppMessage> {
    let block = column![
        ui::logo(112.0),
        container(text("Welcome to Libri").size(34).font(font::EXTRABOLD))
            .padding(Padding::new(0.0).top(10)),
        text("Keep track of your movies, shows, books, games, music and anime, and plan what to enjoy next.")
            .size(16)
            .line_height(1.55)
            .color(tokens.sub)
            .align_x(Horizontal::Center),
        container(big_button("Get started", tokens, Some(msg(Message::Continue))))
            .padding(Padding::new(0.0).top(14)),
        text("Your library is saved in a GitHub repo you own.")
            .size(12.5)
            .color(tokens.sub),
    ]
    .spacing(18)
    .width(560)
    .align_x(Center);
    container(block)
        .center(Fill)
        .padding(Padding::new(0.0).bottom(38))
        .into()
}

/// The steps rail and the current step's form.
fn wizard<'a>(app: &'a App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.setup;
    let current = state.step.index().unwrap_or(0);
    let values = [
        state.repo.trim().to_owned(),
        state.branch_name().to_owned(),
        match state.choice {
            Choice::New => "New library".to_owned(),
            Choice::Existing => "Existing file".to_owned(),
        },
        state.file_name(),
    ];
    let labels = [
        "Connect GitHub",
        "Branch",
        "Library file",
        match state.choice {
            Choice::New => "Create library",
            Choice::Existing => "Pick file",
        },
    ];
    let mut rail = Column::new().spacing(4).push(
        container(
            row![
                ui::logo(28.0),
                text("Set up Libri").size(15).font(font::BOLD)
            ]
            .spacing(10)
            .align_y(Center),
        )
        .padding(Padding::new(0.0).left(10).right(10).bottom(22)),
    );
    for (index, label) in labels.into_iter().enumerate() {
        rail = rail.push(step_row(
            index,
            label,
            &values[index],
            index == current,
            index < current,
            tokens,
        ));
    }
    rail = rail.push(space::vertical()).push(
        container(
            text("You can change all of this later in Settings → Library source.")
                .size(12.5)
                .line_height(1.5)
                .color(tokens.sub),
        )
        .padding([0, 10]),
    );
    let rail = container(rail).width(280).height(Fill).padding([18, 14]);

    let form = match state.step {
        Step::Connect => connect(state, tokens),
        Step::Branch => branch(state, tokens),
        Step::File => file(state, tokens),
        Step::Create => create(state, tokens),
        _ => existing(state, tokens),
    };
    let form = column![
        ui::section_header(&format!("Step {} of 4", current + 1), tokens),
        form
    ]
    .spacing(22)
    .max_width(680);
    let scroll = ui::scroll(
        container(form)
            .width(Fill)
            .align_x(Center)
            .padding(Padding::new(40.0).top(64).bottom(20)),
    )
    .height(Fill);

    let back = button(
        row![
            ui::icon(Icon::ChevronLeft, 15.0, tokens.text),
            text("Back").size(14)
        ]
        .spacing(6)
        .align_y(Center),
    )
    .height(40)
    .padding([0, 16])
    .on_press(msg(Message::Back))
    .style(action_style(ButtonKind::Outline, tokens))
    .focusable(9.0, tokens);
    let primary = button(text(state.primary_label()).size(14).font(font::BOLD))
        .height(40)
        .padding([0, 22])
        .on_press_maybe(state.can_continue().then_some(msg(Message::Continue)))
        .style(action_style(ButtonKind::Primary, tokens))
        .focusable(9.0, tokens);
    let footer = container(
        container(
            row![back, space::horizontal(), primary]
                .align_y(Center)
                .width(Fill),
        )
        .max_width(680),
    )
    .width(Fill)
    .align_x(Center)
    .padding(Padding::new(40.0).top(16).bottom(28));

    let content = tokens.content;
    let pane = container(column![scroll, footer])
        .width(Fill)
        .height(Fill)
        .style(move |_| {
            container::background(content).border(Border {
                radius: Radius::default().top_left(12),
                ..Border::default()
            })
        });
    row![rail, pane].into()
}

/// One step on the rail: a numbered disc (a check once done), the label
/// and its value.
fn step_row<'a>(
    index: usize,
    label: &'a str,
    value: &str,
    current: bool,
    done: bool,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let ring = if done || current {
        tokens.accent
    } else {
        tokens.line
    };
    let mark: Element<'a, AppMessage> = if done {
        ui::icon(Icon::Check, 12.0, tokens.on_accent)
    } else {
        text((index + 1).to_string())
            .size(12)
            .font(font::BOLD)
            .color(if current { tokens.text } else { tokens.sub })
            .into()
    };
    let disc = container(mark).center(24).style(move |_| container::Style {
        background: done.then_some(Background::Color(tokens.accent)),
        border: Border::default().rounded(12).width(2).color(ring),
        ..container::Style::default()
    });
    let (color, weight) = match (current, done) {
        (true, _) => (tokens.text, font::BOLD),
        (false, true) => (tokens.text, font::MEDIUM),
        (false, false) => (tokens.sub, font::MEDIUM),
    };
    let value = match (current, done) {
        (true, _) => "In progress".to_owned(),
        (false, true) => value.to_owned(),
        (false, false) => String::new(),
    };
    let words = column![
        text(label).size(14).font(weight).color(color),
        text(value).size(12).color(tokens.sub),
    ]
    .spacing(1);
    container(row![disc, words].spacing(12).align_y(Center))
        .width(Fill)
        .height(44)
        .padding([0, 10])
        .style(move |_| container::Style {
            background: current.then_some(Background::Color(tokens.selection)),
            border: Border::default().rounded(9),
            ..container::Style::default()
        })
        .into()
}

fn connect<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let ink = tokens.ink;
    let help = text("A fine-grained token that can read and write this one repo. It's stored in KDE Wallet and never leaves this computer.")
        .size(12.5)
        .line_height(1.5)
        .color(tokens.sub);
    let link = button(
        text("How do I create a token?")
            .size(13.5)
            .font(font::SEMIBOLD),
    )
    .padding(0)
    .on_press(msg(Message::TokenHelp))
    .style(move |_, _| button::Style {
        text_color: ink,
        ..button::Style::default()
    })
    .focusable(4.0, tokens);
    column![
        intro(
            "Connect GitHub",
            "Libri reads and saves your library in a GitHub repo you own. A private repo is fine.",
            tokens
        ),
        field(
            "Repository",
            ui::text_field(
                "owner/repo",
                &state.repo,
                |v| msg(Message::Repo(v)),
                font::MONO,
                tokens
            ),
            tokens
        ),
        column![
            field(
                "Access token",
                ui::text_field(
                    "",
                    &state.token,
                    |v| msg(Message::Token(v)),
                    font::MONO,
                    tokens
                )
                .secure(true),
                tokens
            ),
            help
        ]
        .spacing(7),
        link,
    ]
    .spacing(22)
    .into()
}

fn branch<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let lead = rich_text([
        span("Branches in "),
        mono_span(&state.repo, tokens),
        span(". Libri reads and saves your library on the one you pick."),
    ])
    .size(14.5)
    .line_height(1.55)
    .color(tokens.sub);
    let rows = BRANCHES.iter().enumerate().map(|(index, branch)| {
        let on = index == state.branch;
        let mut title = row![ui::mono(branch.name, 14.0, tokens.text)]
            .spacing(8)
            .align_y(Center);
        if branch.default {
            let selection = tokens.selection;
            title = title.push(
                container(text("Default").size(11.5).font(font::SEMIBOLD))
                    .height(20)
                    .padding([0, 8])
                    .align_y(Center)
                    .style(move |_| {
                        container::background(selection).border(Border::default().rounded(10))
                    }),
            );
        }
        radio_row(
            title.into(),
            branch.info,
            on,
            true,
            tokens,
            msg(Message::PickBranch(index)),
        )
    });
    column![
        column![heading("Pick a branch", tokens), lead].spacing(8),
        Column::with_children(rows).spacing(4),
        note(
            "Libri commits straight to this branch, with no pull requests.",
            tokens
        ),
    ]
    .spacing(18)
    .into()
}

fn file<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let lead = rich_text([
        span("Libri keeps everything in one JSON file in "),
        mono_span(&state.repo, tokens),
        span(" on "),
        mono_span(state.branch_name(), tokens),
        span(". Start fresh, or use a file you already have."),
    ])
    .size(14.5)
    .line_height(1.55)
    .color(tokens.sub);
    let cards = [
        (
            Choice::New,
            "Create a new library",
            "Libri makes an empty library file in your repo. Pick this if you're starting out.",
        ),
        (
            Choice::Existing,
            "Use a file I already have",
            "Pick a Libri library file that's already in the repo.",
        ),
    ]
    .map(|(choice, title, body)| {
        let on = state.choice == choice;
        let tokens = *tokens;
        button(
            column![
                row![
                    radio(on, true, &tokens),
                    text(title).size(15).font(font::BOLD)
                ]
                .spacing(10)
                .align_y(Center),
                text(body).size(12.5).line_height(1.5).color(tokens.sub),
            ]
            .spacing(10),
        )
        .width(Fill)
        .padding(18)
        .on_press(msg(Message::Choose(choice)))
        .style(move |_, _| button::Style {
            background: on.then_some(Background::Color(tokens.selection)),
            text_color: tokens.text,
            border: Border::default().rounded(12).width(2).color(if on {
                tokens.accent
            } else {
                tokens.line
            }),
            ..button::Style::default()
        })
        .focusable(12.0, &tokens)
    });
    column![
        column![heading("Your library file", tokens), lead].spacing(8),
        iced::widget::Row::with_children(cards).spacing(14),
    ]
    .spacing(18)
    .into()
}

fn create<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let (chrome, line) = (tokens.chrome, tokens.line);
    let preview = container(
        text(EMPTY_LIBRARY)
            .size(13)
            .font(font::MONO)
            .line_height(1.55),
    )
    .width(Fill)
    .padding([14, 16])
    .style(move |_| {
        container::background(chrome).border(Border::default().rounded(10).width(1).color(line))
    });
    column![
        intro(
            "Create a new library",
            "Libri adds an empty library file to your repo and commits it for you.",
            tokens
        ),
        column![
            field(
                "File name",
                container(ui::text_field(
                    "library.json",
                    &state.file_name,
                    |v| msg(Message::FileName(v)),
                    font::MONO,
                    tokens
                ))
                .max_width(360),
                tokens
            ),
            note(
                &format!(
                    "Saved at the top of {} on {}. Libri never overwrites a file that already exists.",
                    state.repo.trim(),
                    state.branch_name()
                ),
                tokens
            ),
        ]
        .spacing(7),
        column![
            label("What gets added", tokens),
            preview,
            note("Commit message: “Create Libri library”", tokens),
        ]
        .spacing(7),
    ]
    .spacing(18)
    .into()
}

fn existing<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let rows = FILES.iter().enumerate().map(|(index, file)| {
        let on = index == state.file && file.usable;
        let color = if file.usable { tokens.text } else { tokens.sub };
        radio_row(
            ui::mono(file.path, 14.0, color).into(),
            file.info,
            on,
            file.usable,
            tokens,
            msg(Message::PickFile(index)),
        )
    });
    column![
        intro(
            "Pick your file",
            &format!(
                "JSON files found in {} on {}. Only files in Libri's format can be used.",
                state.repo.trim(),
                state.branch_name()
            ),
            tokens
        ),
        Column::with_children(rows).spacing(4),
        note(
            "Don't see your file? Make sure it's in this repo and ends in .json.",
            tokens
        ),
    ]
    .spacing(18)
    .into()
}

/// "You're all set": a check, the summary and Open Libri.
fn done<'a>(state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let disc = container(ui::icon(Icon::Check, 36.0, tokens.success))
        .center(72)
        .style(move |_| {
            container::background(tokens.selection).border(Border::default().rounded(36))
        });
    let words = match state.choice {
        Choice::New => format!(
            "{} is now in {} on {}. Add your first entry with the Add button.",
            state.file_name(),
            state.repo.trim(),
            state.branch_name()
        ),
        Choice::Existing => format!("Loaded 214 entries from {}.", state.file_name()),
    };
    let summary = |label: &'static str, value: String| {
        row![
            container(ui::mono(label, 13.0, tokens.sub)).width(62),
            ui::mono(value, 13.0, tokens.text)
        ]
    };
    let card = container(
        column![
            summary("repo", state.repo.trim().to_owned()),
            summary("branch", state.branch_name().to_owned()),
            summary("file", state.file_name()),
        ]
        .spacing(6),
    )
    .padding([14, 18])
    .style(move |_| {
        let border = if tokens.light {
            Border::default().rounded(12).width(1).color(tokens.line)
        } else {
            Border::default().rounded(12)
        };
        container::background(tokens.content).border(border)
    });
    let block = column![
        disc,
        container(text("You're all set").size(32).font(font::EXTRABOLD))
            .padding(Padding::new(0.0).top(6)),
        text(words)
            .size(15.5)
            .line_height(1.55)
            .color(tokens.sub)
            .align_x(Horizontal::Center),
        card,
        container(big_button("Open Libri", &tokens, Some(msg(Message::Open))))
            .padding(Padding::new(0.0).top(10)),
    ]
    .spacing(16)
    .width(560)
    .align_x(Center);
    container(block)
        .center(Fill)
        .padding(Padding::new(0.0).bottom(38))
        .into()
}

/// A step's title and its lead paragraph.
fn intro<'a>(title: &'a str, lead: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    column![
        heading(title, tokens),
        text(lead.to_owned())
            .size(14.5)
            .line_height(1.55)
            .color(tokens.sub)
    ]
    .spacing(8)
    .into()
}

fn heading<'a>(title: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(title)
        .size(26)
        .font(font::EXTRABOLD)
        .color(tokens.text)
        .into()
}

/// A field with its label in the text color, as the wizard's forms have.
fn field<'a>(
    name: &'a str,
    control: impl Into<Element<'a, AppMessage>>,
    tokens: &Tokens,
) -> Element<'a, AppMessage> {
    column![label(name, tokens), control.into()]
        .spacing(7)
        .into()
}

fn label<'a>(name: &'a str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(name)
        .size(13)
        .font(font::SEMIBOLD)
        .color(tokens.text)
        .into()
}

fn note<'a>(words: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(words.to_owned())
        .size(12.5)
        .line_height(1.5)
        .color(tokens.sub)
        .into()
}

fn mono_span<'a>(words: &str, tokens: &Tokens) -> iced::widget::text::Span<'a, ()> {
    span(words.to_owned()).font(font::MONO).color(tokens.text)
}

/// A pickable row: a radio mark, a title and a line of detail. A row that
/// cannot be picked is dimmed.
fn radio_row<'a>(
    title: Element<'a, AppMessage>,
    info: &'a str,
    on: bool,
    enabled: bool,
    tokens: &Tokens,
    on_press: AppMessage,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let words = column![title, text(info).size(12.5).color(tokens.sub)].spacing(2);
    button(
        row![radio(on, enabled, &tokens), words]
            .spacing(12)
            .align_y(Center),
    )
    .width(Fill)
    .height(54)
    .padding([8, 14])
    .on_press_maybe(enabled.then_some(on_press))
    .style(move |_, _| button::Style {
        background: on.then_some(Background::Color(tokens.selection)),
        text_color: tokens.text,
        border: Border::default().rounded(10),
        ..button::Style::default()
    })
    .focusable(10.0, &tokens)
}

/// An 18 px radio mark: the accent ring and dot when on.
fn radio<'a>(on: bool, enabled: bool, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let ring = match (on, enabled) {
        (true, _) => tokens.accent,
        (false, true) => tokens.sub,
        (false, false) => tokens.line,
    };
    let dot = container(space())
        .width(8)
        .height(8)
        .style(move |_| container::Style {
            background: on.then_some(Background::Color(tokens.accent)),
            border: Border::default().rounded(4),
            ..container::Style::default()
        });
    container(dot)
        .center(18)
        .style(move |_| {
            container::Style::default().border(Border::default().rounded(9).width(2).color(ring))
        })
        .into()
}

/// Get started and Open Libri: 44 px, bold, a 10 px radius.
fn big_button<'a>(
    words: &'a str,
    tokens: &Tokens,
    on_press: Option<AppMessage>,
) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    button(
        text(words)
            .size(15)
            .font(font::BOLD)
            .height(Fill)
            .align_y(Vertical::Center),
    )
    .height(44)
    .padding([0, 26])
    .on_press_maybe(on_press)
    .style(move |_, status| {
        let mut style = ui::button_style(ButtonKind::Primary, &tokens, status);
        style.border.radius = 10.0.into();
        style
    })
    .focusable(10.0, &tokens)
}

/// Back and the main action at the bottom: 40 px with a 9 px radius.
fn action_style(
    kind: ButtonKind,
    tokens: &Tokens,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let tokens = *tokens;
    move |_, status| {
        let mut style = ui::button_style(kind, &tokens, status);
        style.border.radius = 9.0.into();
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_at(step: Step) -> App {
        let mut app = App::new(Screen::Setup);
        app.setup = State::at(step, app.source.as_ref());
        app
    }

    #[test]
    fn the_steps_follow_the_choice_of_file() {
        let mut app = app_at(Step::Welcome);
        app.source = None;
        app.setup = State::at(Step::Welcome, None);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Connect);
        // Connect needs a repository and a token.
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Connect);
        let _ = update(&mut app, Message::Repo("ekmanch/library".into()));
        let _ = update(&mut app, Message::Token("github_pat_x".into()));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Branch);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::File);
        let _ = update(&mut app, Message::Choose(Choice::Existing));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Existing);
        let _ = update(&mut app, Message::Back);
        let _ = update(&mut app, Message::Choose(Choice::New));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Create);
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.setup.step, Step::Done);
        let _ = update(&mut app, Message::Open);
        assert_eq!(app.screen, Screen::Library);
        let source = app.source.unwrap();
        assert_eq!(source.repo, "ekmanch/library");
        assert_eq!(source.branch, "main");
        assert_eq!(source.file, "library.json");
        assert_eq!(source.token_hint, "github_pat_••••••at_x");
    }

    #[test]
    fn a_file_that_is_not_a_library_cannot_be_picked() {
        let mut app = app_at(Step::Existing);
        let _ = update(&mut app, Message::PickFile(2));
        assert_eq!(app.setup.file, 0);
        let _ = update(&mut app, Message::PickFile(1));
        assert_eq!(app.setup.file, 1);
    }

    #[test]
    fn a_new_file_needs_a_json_name() {
        let mut app = app_at(Step::Create);
        let _ = update(&mut app, Message::FileName("notes".into()));
        assert!(!app.setup.can_continue());
        let _ = update(&mut app, Message::FileName("lists.json".into()));
        assert!(app.setup.can_continue());
    }

    #[test]
    fn replacing_the_token_returns_to_the_screen_it_came_from() {
        let mut app = App::new(Screen::Plans);
        app.sync = SyncState::TokenExpired;
        let _ = update(&mut app, Message::ReplaceToken);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.replacing, Some(Screen::Plans));
        assert!(app.setup.token.is_empty());
        let _ = update(&mut app, Message::Token("github_pat_new".into()));
        let _ = update(&mut app, Message::Continue);
        assert_eq!(app.screen, Screen::Plans);
        assert_eq!(app.sync, SyncState::Synced);
        assert_eq!(app.setup.replacing, None);
    }

    #[test]
    fn the_window_is_titled_only_on_the_numbered_steps() {
        assert!(!State::at(Step::Welcome, None).in_wizard());
        assert!(State::at(Step::Create, None).in_wizard());
        assert!(!State::at(Step::Done, None).in_wizard());
    }
}
