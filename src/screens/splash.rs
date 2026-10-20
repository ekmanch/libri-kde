//! The splash (docs/DESIGN.md 4.1): logo, wordmark, an indeterminate bar
//! in the accent and a status line, while Libri starts. A first run says
//! "Getting things ready…" with the version; a returning user sees the
//! repo, branch and file.

use iced::alignment::Vertical;
use iced::widget::{column, container, space, stack, text};
use iced::{Border, Center, Element, Fill, Padding};

use crate::app::{App, Message};
use crate::theme::{Tokens, font};
use crate::ui;

pub fn view(app: &App) -> Element<'_, Message> {
    let tokens = app.tokens();
    let (status, footer) = match &app.source {
        None => (
            "Getting things ready…",
            format!("Version {}", env!("CARGO_PKG_VERSION")),
        ),
        Some(source) => ("Loading your library…", source.line()),
    };
    let progress = column![
        bar(app.splash_phase, &tokens),
        text(status).size(13).color(tokens.sub),
    ]
    .spacing(10)
    .align_x(Center);
    let block = column![
        ui::logo(168.0),
        text("Libri").size(30).font(font::EXTRABOLD),
        container(progress).padding(Padding::new(0.0).top(18)),
    ]
    .spacing(22)
    .align_x(Center);
    let centered = container(block)
        .center(Fill)
        .padding(Padding::new(0.0).bottom(60));
    let footer = container(ui::mono(footer, 12.0, tokens.sub))
        .width(Fill)
        .height(Fill)
        .align_x(Center)
        .align_y(Vertical::Bottom)
        .padding(Padding::new(0.0).bottom(18));
    let chrome = tokens.chrome;
    container(stack![centered, footer])
        .width(Fill)
        .height(Fill)
        .style(move |_| container::background(chrome).color(tokens.text))
        .into()
}

/// The 180 × 4 track with a 72 px accent segment sweeping across it and
/// back, eased at both ends.
fn bar<'a>(phase: f32, tokens: &Tokens) -> Element<'a, Message> {
    let (track, accent) = (tokens.track, tokens.accent);
    let sweep = 1.0 - (2.0 * phase - 1.0).abs();
    let eased = sweep * sweep * (3.0 - 2.0 * sweep);
    let x = (180.0 - 72.0) * eased;
    let segment = container(space())
        .width(72)
        .height(4)
        .style(move |_| container::background(accent).border(Border::default().rounded(2)));
    container(segment)
        .width(180)
        .height(4)
        .padding(Padding::new(0.0).left(x))
        .style(move |_| container::background(track).border(Border::default().rounded(2)))
        .into()
}
