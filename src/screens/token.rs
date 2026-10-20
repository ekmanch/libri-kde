//! The token expiry warning (docs/DESIGN.md 4.6): "Your GitHub token
//! expires in 6 days", with Later, Open GitHub and Replace token. Shown
//! once when Libri opens within seven days of the expiry; driven by fake
//! states until phase 3 reads GitHub's header.

use iced::widget::{column, container, row, space, span, text};
use iced::{Border, Center, Element, Padding, Task};

use super::setup;
use crate::app::{self, App, Message as AppMessage};
use crate::settings::SyncState;
use crate::theme::font;
use crate::ui::{self, ButtonKind, Icon};

/// GitHub's fine-grained token settings, for Open GitHub.
pub const TOKEN_SETTINGS_URL: &str = "https://github.com/settings/personal-access-tokens";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// Closes the warning until the next day.
    Later,
    /// Opens the token settings page in the browser.
    OpenGitHub,
    /// Opens the Connect GitHub step of setup with the repository kept.
    Replace,
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Token(message)
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Later => app.token_warning = false,
        Message::OpenGitHub => app::open_url(TOKEN_SETTINGS_URL),
        Message::Replace => {
            app.token_warning = false;
            return setup::update(app, setup::Message::ReplaceToken);
        }
    }
    Task::none()
}

/// The warning over `base` while it shows.
pub fn overlay<'a>(app: &'a App, base: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let dialog = app.token_warning.then(|| view(app));
    ui::modal(base, dialog, msg(Message::Later), &tokens)
}

fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let SyncState::ExpiresIn(days) = app.sync else {
        return space().into();
    };
    let date = app.today.add_days(i64::from(days)).month_day();
    let repo = app
        .source
        .as_ref()
        .map_or("your repository", |source| source.repo.as_str());
    let title = format!(
        "Your GitHub token expires in {days} {}",
        if days == 1 { "day" } else { "days" }
    );
    let body: iced::widget::text::Rich<'_, (), AppMessage> = iced::widget::rich_text([
        span("The access token for "),
        span(repo.to_owned()).font(font::MONO).size(13),
        span(format!(
            " stops working on {date}. After that, Libri can't sync until you add a new one."
        )),
    ])
    .size(14);
    let help = text(
        "On GitHub, regenerate the token with a new expiry date, then paste it here. Your library stays as it is.",
    )
    .size(13)
    .line_height(1.55)
    .color(tokens.sub);
    let (fill, ink) = (tokens.warning_fill, tokens.warning);
    let disc = container(ui::icon(Icon::Key, 22.0, ink))
        .center(44)
        .style(move |_| container::background(fill).border(Border::default().rounded(22)));
    let words = column![
        text(title).size(20).font(font::EXTRABOLD).line_height(1.25),
        body,
        help
    ]
    .spacing(8);
    let later = ui::labeled_button("Later", ButtonKind::Text, tokens, Some(msg(Message::Later)))
        .style(move |_, _| iced::widget::button::Style {
            text_color: tokens.sub,
            ..iced::widget::button::Style::default()
        });
    let open = iced::widget::button(
        row![
            text("Open GitHub").size(13.5),
            ui::icon(Icon::ExternalLink, 13.0, tokens.text)
        ]
        .spacing(6)
        .align_y(Center),
    )
    .height(36)
    .padding([0, 14])
    .on_press(msg(Message::OpenGitHub))
    .style(move |_, status| ui::button_style(ButtonKind::Outline, &tokens, status));
    let replace = ui::labeled_button(
        "Replace token",
        ButtonKind::Primary,
        tokens,
        Some(msg(Message::Replace)),
    );
    let footer = container(
        row![later, space::horizontal(), open, replace]
            .spacing(10)
            .align_y(Center),
    )
    .padding(Padding::new(0.0).top(8));
    let content = column![
        row![disc, words]
            .spacing(14)
            .align_y(iced::alignment::Vertical::Top),
        footer
    ]
    .spacing(14);
    container(content)
        .width(500)
        .padding(24)
        .style(move |_| ui::panel_style(&tokens, 14.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;

    #[test]
    fn later_closes_the_warning_and_replace_opens_the_connect_step() {
        let mut app = App::new(Screen::Library);
        app.sync = SyncState::ExpiresIn(6);
        app.token_warning = true;
        let _ = update(&mut app, Message::Later);
        assert!(!app.token_warning);

        app.token_warning = true;
        let _ = update(&mut app, Message::Replace);
        assert!(!app.token_warning);
        assert_eq!(app.screen, Screen::Setup);
        assert_eq!(app.setup.step, setup::Step::Connect);
        assert_eq!(app.setup.repo, "ekmanch/library");
        assert!(app.setup.token.is_empty());
    }
}
