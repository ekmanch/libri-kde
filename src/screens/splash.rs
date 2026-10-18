use iced::Element;

use crate::app::Message;
use crate::theme::Tokens;

pub fn view<'a>(tokens: &Tokens) -> Element<'a, Message> {
    super::placeholder(tokens)
}
