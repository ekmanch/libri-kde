//! The library: one type or everything. The toolbar, views and filters
//! come in 1b; for now the title and the count.

use iced::widget::{column, container, row, space, text};
use iced::{Center, Element, Fill, Padding};

use super::shell::shell;
use crate::app::{App, Message as AppMessage};
use crate::model::MediaType;
use crate::theme::font;
use crate::ui;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    /// The type shown; `None` is everything.
    pub media: Option<MediaType>,
    /// The tags picked in the sidebar or the filter.
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ShowType(Option<MediaType>),
    /// A sidebar tag: picks it alone, or clears it when it is the only
    /// one picked.
    PickTag(String),
}

impl State {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::ShowType(media) => self.media = media,
            Message::PickTag(tag) => {
                self.tags = if self.tags == [tag.as_str()] {
                    Vec::new()
                } else {
                    vec![tag]
                };
            }
        }
    }

    pub fn title(&self) -> &'static str {
        self.media.map_or("Everything", MediaType::label)
    }
}

pub fn view(app: &App) -> Element<'_, AppMessage> {
    let tokens = app.tokens();
    let state = &app.library;
    let title_color = state
        .media
        .map_or(tokens.text, |media| tokens.type_color(media));
    let count = app.repo.library().count(state.media);
    let toolbar = row![
        text(state.title())
            .size(26)
            .font(font::EXTRABOLD)
            .color(title_color),
        ui::mono(count.to_string(), 13.0, tokens.sub),
    ]
    .spacing(10)
    .align_y(Center);
    let toolbar = container(toolbar)
        .height(64)
        .padding(Padding::new(0.0).left(24).right(20))
        .align_y(Center);
    let footer = container(ui::mono("ekmanch/library · main", 12.0, tokens.sub))
        .height(30)
        .width(Fill)
        .padding([0, 24])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(Center);
    shell(app, column![toolbar, space::vertical(), footer])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidebar_tag_is_picked_alone_and_cleared_by_a_second_click() {
        let mut state = State {
            tags: vec!["horror".into(), "dark".into()],
            ..State::default()
        };
        state.update(Message::PickTag("dark".into()));
        assert_eq!(state.tags, ["dark"]);
        state.update(Message::PickTag("dark".into()));
        assert!(state.tags.is_empty());
    }

    #[test]
    fn the_title_is_the_type_or_everything() {
        let mut state = State::default();
        assert_eq!(state.title(), "Everything");
        state.update(Message::ShowType(Some(MediaType::Anime)));
        assert_eq!(state.title(), "Anime");
    }
}
