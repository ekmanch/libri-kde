use iced::Element;
use iced::widget::space;

use super::shell::shell;
use crate::app::{App, Message};

/// The plans overview and plan screens come in 1e; for now the shell.
pub fn view(app: &App) -> Element<'_, Message> {
    shell(app, space())
}
