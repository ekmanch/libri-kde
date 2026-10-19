//! The small dialogs: the delete confirmation and Add to plan. The wording
//! is the phone's (A9, A7).

use iced::widget::{Column, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill};

use super::Message;
use crate::app::{App, Message as AppMessage};
use crate::model::Plan;
use crate::theme::Tokens;
use crate::ui::{self, ButtonKind, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Entry(message)
}

/// "Delete Dracula?": names the plans the entry leaves with it.
pub fn delete<'a>(app: &'a App, id: &str) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let Some(entry) = library.entry(id) else {
        return space().into();
    };
    let plans: Vec<String> = library
        .plans
        .iter()
        .filter(|p| p.item_for(id).is_some())
        .map(Plan::title)
        .collect();
    let body =
        column![
        text(format!("It will be removed from your library{}.", plans_clause(&plans)))
            .size(14)
            .color(tokens.text),
        text("The change is saved to your GitHub repo, where earlier versions stay in the history.")
            .size(13)
            .color(tokens.sub),
    ]
        .spacing(10);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Cancel",
            ButtonKind::Outline,
            tokens,
            Some(msg(Message::Close))
        ),
        ui::labeled_button(
            "Delete",
            ButtonKind::Destructive,
            tokens,
            Some(msg(Message::ConfirmDelete))
        ),
    ]
    .spacing(8);
    let title = format!("Delete {}?", entry.title);
    ui::dialog(title, body, footer, 460, &tokens, msg(Message::Close))
}

/// " and from the plan Halloween 2026", or the plans listed.
fn plans_clause(plans: &[String]) -> String {
    match plans {
        [] => String::new(),
        [only] => format!(" and from the plan {only}"),
        [rest @ .., last] => format!(" and from the plans {} and {last}", rest.join(", ")),
    }
}

/// One row per plan with a check; a click adds or removes the entry at
/// once, and Done closes.
pub fn add_to_plan<'a>(app: &'a App, id: &'a str) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let library = app.repo.library();
    let Some(entry) = library.entry(id) else {
        return space().into();
    };
    let mut rows = Column::new().spacing(2);
    if library.plans.is_empty() {
        rows = rows.push(
            text("No plans yet. Make one on the Plans screen.")
                .size(14)
                .color(tokens.sub),
        );
    }
    for plan in &library.plans {
        let on = plan.item_for(id).is_some();
        let label = row![
            check(on, &tokens),
            text(&plan.name).size(14),
            text(plan.year.map(|y| y.to_string()).unwrap_or_default())
                .size(13)
                .color(tokens.sub),
        ]
        .spacing(10)
        .align_y(Center)
        .height(Fill);
        rows = rows.push(
            button(label)
                .width(Fill)
                .height(40)
                .padding([0, 10])
                .on_press(msg(Message::TogglePlan(plan.id.clone())))
                .style(ui::row_style(&tokens, false, 8.0)),
        );
    }
    let body = column![text(&entry.title).size(14).color(tokens.sub), rows].spacing(12);
    let footer = row![
        space::horizontal(),
        ui::labeled_button(
            "Done",
            ButtonKind::Primary,
            tokens,
            Some(msg(Message::Close))
        )
    ];
    ui::dialog(
        "Add to plan",
        body,
        footer,
        420,
        &tokens,
        msg(Message::Close),
    )
}

/// An 18 px check box in the accent.
fn check<'a>(on: bool, tokens: &Tokens) -> Element<'a, AppMessage> {
    let tokens = *tokens;
    let mark: Element<'a, AppMessage> = if on {
        ui::icon(Icon::Check, 12.0, tokens.on_accent)
    } else {
        space().into()
    };
    container(mark)
        .center(18)
        .style(move |_| container::Style {
            background: on.then_some(Background::Color(tokens.accent)),
            border: Border::default().rounded(5).width(1.5).color(if on {
                tokens.accent
            } else {
                tokens.sub
            }),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plans_clause_lists_every_plan() {
        assert_eq!(plans_clause(&[]), "");
        assert_eq!(
            plans_clause(&["Halloween 2026".into()]),
            " and from the plan Halloween 2026"
        );
        let three = ["A".to_owned(), "B".to_owned(), "C".to_owned()];
        assert_eq!(plans_clause(&three), " and from the plans A, B and C");
    }
}
