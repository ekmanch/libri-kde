//! Throwaway token sheet (TODO.md 0c): every color, text style, button and
//! icon, to eyeball against the mockup. Remove at the end of phase 1.

use iced::widget::{Column, Row, button, column, container, row, scrollable, text};
use iced::{Background, Border, Color, Element, Fill};

use crate::app::{App, Message};
use crate::model::MediaType;
use crate::theme::{self, Accent, Tokens, font, size};
use crate::ui::{self, ButtonKind, Icon};

pub fn view(app: &App, tokens: Tokens) -> Element<'_, Message> {
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
            Some(Message::SetTheme(choice)),
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
            dot(color, 10.0),
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
        text("1999 · 42 · 2026-10-18, JetBrains Mono 12")
            .size(size::MONO)
            .font(font::MONO)
            .color(tokens.sub),
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
        ui::labeled_button(label, kind, tokens, Some(Message::SetAccent(app.accent)))
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
    ]
    .spacing(28)
    .padding(40);

    let content_bg = tokens.content;
    container(scrollable(content).width(Fill).height(Fill))
        .style(move |_| container::background(content_bg).color(tokens.text))
        .into()
}

fn section<'a>(label: &str, body: Element<'a, Message>, tokens: &Tokens) -> Element<'a, Message> {
    column![ui::section_header(label, tokens), body]
        .spacing(12)
        .into()
}

fn accents<'a>(palette: &[Accent], current: Accent, tokens: &Tokens) -> Element<'a, Message> {
    let tokens = *tokens;
    let swatches = palette.iter().map(|&accent| {
        let label = row![dot(accent.fill, 14.0), text(accent.name).color(tokens.text)]
            .spacing(8)
            .align_y(iced::Center);
        let selected = accent == current;
        button(label)
            .on_press(Message::SetAccent(accent))
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

fn swatch<'a>(name: &'a str, color: Color, tokens: &Tokens) -> Element<'a, Message> {
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

fn dot<'a>(color: Color, diameter: f32) -> Element<'a, Message> {
    container(text(""))
        .width(diameter)
        .height(diameter)
        .style(move |_| {
            container::background(color).border(Border::default().rounded(diameter / 2.0))
        })
        .into()
}

#[cfg(test)]
mod tests {
    use iced_test::simulator;

    use super::*;
    use crate::screens::Screen;

    #[test]
    fn picking_a_theme_and_an_accent_updates_the_app() {
        let mut app = App::new(Screen::TokenSheet);
        let messages = {
            let mut ui = simulator(app.view());
            assert!(ui.find("Token sheet").is_ok());
            ui.click("Light").unwrap();
            ui.click("Lavender").unwrap();
            ui.into_messages().collect::<Vec<_>>()
        };
        for message in messages {
            app.update(message);
        }
        assert_eq!(app.theme, theme::Theme::Light);
        assert_eq!(app.accent.name, "Lavender");
    }
}
