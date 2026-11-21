//! The Following window (canvas revision of 2026-11-19; the phone's
//! Settings › Upcoming releases and its Follow lists, as one window): the
//! seven kinds on the left, each with what Discover finds for it and how
//! many are followed, and the picked kind's tick list on the right, with
//! a search, Follow all or Unfollow all, and the creators who are no
//! longer active in a group of their own. It opens from Settings ›
//! Discover and from the Following button on Coming soon.
//!
//! The list is `follows` (the app's `follows`, derived from the library);
//! a tick writes the exception to `settings.json` at once
//! (`App::change_shared`), so the right-click menus' Follow and Unfollow
//! come through here too.

use iced::widget::{
    Column, Space, button, column, container, operation, row, text, text::Wrapping,
};
use iced::{Background, Border, Center, Element, Fill, Padding, Task};

use crate::app::{App, Message as AppMessage};
use crate::follows::{Follow, Kind};
use crate::shared::{self, Edit};
use crate::theme::{Tokens, font};
use crate::ui::{self, ButtonKind, FocusExt, Icon};

/// The search field, focused when the window opens.
pub const SEARCH_ID: &str = "following-search";
/// The window's width, and its height in a window tall enough.
const WIDTH: f32 = 880.0;
const HEIGHT: f32 = 760.0;

/// The open window: the kind picked and the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub kind: Kind,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Opens the window, on a kind or on Authors.
    Open(Option<Kind>),
    Close,
    Pick(Kind),
    Search(String),
    /// A click on a row: the follow flips.
    Toggle(Kind, String),
    /// Follow all or Unfollow all, whichever the picked kind offers.
    All,
    /// A follow set on or off from a menu.
    Set {
        kind: Kind,
        id: String,
        on: bool,
    },
}

fn msg(message: Message) -> AppMessage {
    AppMessage::Following(message)
}

pub fn update(app: &mut App, message: Message) -> Task<AppMessage> {
    match message {
        Message::Open(kind) => {
            app.following = Some(State {
                kind: kind.unwrap_or(Kind::Authors),
                query: String::new(),
            });
            return operation::focus(SEARCH_ID);
        }
        Message::Close => app.following = None,
        Message::Pick(kind) => {
            if let Some(state) = &mut app.following {
                state.kind = kind;
                state.query.clear();
            }
        }
        Message::Search(query) => {
            if let Some(state) = &mut app.following {
                state.query = query;
            }
        }
        Message::Toggle(kind, id) => {
            let on = app.follows.find(kind, &id).is_some_and(|f| !f.followed);
            set(app, kind, &id, on);
        }
        Message::Set { kind, id, on } => set(app, kind, &id, on),
        Message::All => {
            let Some(kind) = app.following.as_ref().map(|state| state.kind) else {
                return Task::none();
            };
            let on = !app.follows.most_followed(kind);
            let ids = app.follows.to_set(kind, on);
            if ids.is_empty() {
                return Task::none();
            }
            // Only the active ones: each is what its kind is by default.
            let edit = Edit::Follow {
                kind,
                ids,
                on,
                default_on: kind.default_on(),
            };
            let summary = shared::summary(&edit, None);
            app.change_shared(edit, summary);
        }
    }
    Task::none()
}

/// Sets one follow on or off: written as the exception to what it is by
/// default, or taken out of the file when it is the default again. A
/// follow the library no longer has (a title found for it is still
/// listed) goes by its kind's default.
pub fn set(app: &mut App, kind: Kind, id: &str, on: bool) {
    let follow = app.follows.find(kind, id);
    let default_on = follow.map_or(kind.default_on(), Follow::default_on);
    let name = follow.map(|follow| follow.name.clone());
    let edit = Edit::Follow {
        kind,
        ids: vec![id.to_owned()],
        on,
        default_on,
    };
    let summary = shared::summary(&edit, name.as_deref());
    app.change_shared(edit, summary);
}

/// Esc closes the window; false when it was not open.
pub fn dismiss(app: &mut App) -> bool {
    app.following.take().is_some()
}

/// `base` with the window over it while it is open.
pub fn overlay<'a>(app: &'a App, base: Element<'a, AppMessage>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let dialog = app
        .following
        .as_ref()
        .map(|state| view(app, state, &tokens));
    ui::modal(base, dialog, msg(Message::Close), &tokens)
}

/// The rows of the picked kind that the search keeps: the active ones,
/// then those no longer active.
pub fn rows<'a>(app: &'a App, state: &State) -> (Vec<&'a Follow>, Vec<&'a Follow>) {
    let query = state.query.trim().to_lowercase();
    app.follows
        .of(state.kind)
        .iter()
        .filter(|follow| query.is_empty() || follow.name.to_lowercase().contains(&query))
        .partition(|follow| follow.is_active())
}

fn view<'a>(app: &'a App, state: &'a State, tokens: &Tokens) -> Element<'a, AppMessage> {
    let t = *tokens;
    let head = column![
        text("Following").size(20).font(font::EXTRABOLD),
        text(
            "Taken from your library: creators, tags and series. Add a book and its author is \
             followed. Untick what Discover should leave out."
        )
        .size(13.5)
        .color(t.sub),
    ]
    .spacing(4);

    let kinds = Column::with_children(Kind::ALL.into_iter().map(|kind| {
        let on = kind == state.kind;
        let count = ui::mono(
            app.follows.count_label(kind),
            12.0,
            if on { t.text } else { t.sub },
        )
        .wrapping(Wrapping::None);
        let words = column![
            text(kind.label()).size(14).font(font::SEMIBOLD),
            text(kind.what()).size(12).color(t.sub),
        ]
        .spacing(1)
        .width(Fill);
        button(row![words, count].spacing(10).align_y(Center).height(Fill))
            .width(Fill)
            .height(48)
            .padding([0, 12])
            .on_press(msg(Message::Pick(kind)))
            .style(ui::row_style(tokens, on, 9.0))
            .focusable(9.0, tokens)
    }))
    .spacing(2)
    .width(250);

    let kind = state.kind;
    let all = if app.follows.most_followed(kind) {
        "Unfollow all"
    } else {
        "Follow all"
    };
    let can_all = !app
        .follows
        .to_set(kind, !app.follows.most_followed(kind))
        .is_empty();
    let title = row![
        column![
            text(kind.label()).size(16).font(font::BOLD),
            text(format!("{} followed", app.follows.count_label(kind)))
                .size(12.5)
                .color(t.sub),
        ]
        .spacing(1)
        .width(Fill),
        ui::sized_button(
            all,
            ButtonKind::Outline,
            32.0,
            t,
            can_all.then(|| msg(Message::All))
        ),
    ]
    .spacing(12)
    .align_y(Center);
    let hint = kind.search_hint();
    let search = ui::search_field(
        &hint,
        &state.query,
        |query| msg(Message::Search(query)),
        None,
        Some(msg(Message::Search(String::new()))),
        Some(SEARCH_ID),
        tokens,
    );

    let (active, gone) = rows(app, state);
    let mut list = Column::new();
    if !active.is_empty() {
        list = list.push(
            container(group_head(kind.group(), tokens)).padding(Padding::new(0.0).top(2).bottom(4)),
        );
    }
    for follow in &active {
        list = list.push(tick_row(follow, tokens));
    }
    if !gone.is_empty() {
        list = list.push(
            container(
                row![
                    group_head("No longer active", tokens),
                    text("Skipped on their own. Tick one to check anyway.")
                        .size(12.5)
                        .color(t.sub),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(Padding::new(0.0).top(18).bottom(4)),
        );
    }
    for follow in &gone {
        list = list.push(tick_row(follow, tokens));
    }
    if active.is_empty() && gone.is_empty() {
        let why = if app.follows.of(kind).is_empty() {
            kind.empty_text()
        } else {
            kind.no_match_text(&state.query)
        };
        list = list
            .push(container(text(why).size(13.5).color(t.sub)).padding(Padding::new(14.0).left(2)));
    }
    // The right padding keeps the scroller off the rows.
    let list = ui::scroll(container(list).padding(Padding::new(0.0).right(10))).height(Fill);
    let picked = column![title, search, list].spacing(12).width(Fill);

    let note = if app.source.is_some() {
        "Changes apply right away and sync to your other devices."
    } else {
        "Changes apply right away."
    };
    let footer = row![
        text(note).size(12.5).color(t.sub).width(Fill),
        ui::labeled_button("Done", ButtonKind::Primary, t, Some(msg(Message::Close))),
    ]
    .spacing(12)
    .align_y(Center);

    let height = (app.window.height - 80.0).clamp(360.0, HEIGHT);
    container(column![head, row![kinds, picked].spacing(24).height(Fill), footer].spacing(16))
        .width(WIDTH)
        .height(height)
        .padding(24)
        .style(move |_| ui::panel_style(&t, 14.0))
        .into()
}

/// "FOLLOWED FOR NEW BOOKS".
fn group_head<'a>(label: &str, tokens: &Tokens) -> Element<'a, AppMessage> {
    text(label.to_uppercase())
        .size(11)
        .font(font::BOLD)
        .color(tokens.sub)
        .into()
}

/// One follow: a tick box, the name and what the library has of theirs,
/// with a line under it. The whole row flips the tick.
fn tick_row<'a>(follow: &Follow, tokens: &Tokens) -> Element<'a, AppMessage> {
    let t = *tokens;
    let on = follow.followed;
    let mark: Element<'a, AppMessage> = if on {
        ui::icon(Icon::Check, 11.0, t.on_accent)
    } else {
        Space::new().into()
    };
    let tick = container(mark).center(18).style(move |_| {
        container::Style::default()
            .background(if on {
                Background::Color(t.accent)
            } else {
                Background::Color(iced::Color::TRANSPARENT)
            })
            .border(Border::default().rounded(5).width(1.5).color(if on {
                t.accent
            } else {
                t.sub
            }))
    });
    // One no longer active is gray until it is ticked.
    let name_color = if follow.is_active() || on {
        t.text
    } else {
        t.sub
    };
    let words = column![
        text(follow.name.clone())
            .size(14)
            .font(font::SEMIBOLD)
            .color(name_color),
        text(follow.second_line()).size(12.5).color(t.sub),
    ]
    .spacing(1)
    .width(Fill);
    let line = button(
        container(row![tick, words].spacing(12).align_y(Center))
            .center_y(Fill)
            .width(Fill),
    )
    .width(Fill)
    .height(46)
    .padding([4, 2])
    .on_press(msg(Message::Toggle(follow.kind, follow.id.clone())))
    .style(move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then_some(Background::Color(t.hover)),
        border: Border::default().rounded(6),
        text_color: t.text,
        ..button::Style::default()
    })
    .focusable(6.0, tokens);
    column![line, ui::rule(t.line)].into()
}

/// The Following button of the Coming soon header: outlined like Filter
/// and Sort, with the count in the muted color.
pub fn button_for<'a>(app: &App, tokens: &Tokens) -> Element<'a, AppMessage> {
    let t = *tokens;
    let label = row![
        text("Following").size(13.5).font(font::SEMIBOLD),
        ui::mono(app.follows.total().to_string(), 12.0, t.sub),
    ]
    .spacing(7)
    .align_y(Center)
    .height(Fill);
    button(label)
        .height(36)
        .padding([0, 12])
        .on_press(msg(Message::Open(None)))
        .style(move |_, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then_some(Background::Color(t.hover)),
            border: Border::default().rounded(8).width(1).color(t.line),
            text_color: t.text,
            ..button::Style::default()
        })
        .focusable(8.0, tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;

    fn app() -> App {
        App::new(Screen::Settings)
    }

    fn send(app: &mut App, message: Message) {
        let _ = app.update(msg(message));
    }

    #[test]
    fn the_window_opens_on_authors_and_a_pick_empties_the_search() {
        let mut app = app();
        send(&mut app, Message::Open(None));
        assert_eq!(app.following.as_ref().unwrap().kind, Kind::Authors);
        send(&mut app, Message::Search("a".into()));
        send(&mut app, Message::Pick(Kind::Artists));
        let state = app.following.as_ref().unwrap();
        assert_eq!((state.kind, state.query.as_str()), (Kind::Artists, ""));
        send(&mut app, Message::Close);
        assert!(app.following.is_none());
        send(&mut app, Message::Open(Some(Kind::Actors)));
        assert_eq!(app.following.as_ref().unwrap().kind, Kind::Actors);
        assert!(dismiss(&mut app));
        assert!(!dismiss(&mut app));
    }

    #[test]
    fn a_tick_applies_at_once_and_is_written_as_the_exception() {
        let mut app = app();
        send(&mut app, Message::Open(Some(Kind::Artists)));
        let artist = app.follows.of(Kind::Artists)[0].clone();
        assert!(artist.followed);
        let total = app.follows.total();
        send(&mut app, Message::Toggle(Kind::Artists, artist.id.clone()));
        assert!(
            !app.follows
                .find(Kind::Artists, &artist.id)
                .unwrap()
                .followed
        );
        assert_eq!(app.follows.total(), total - 1);
        let settings = &app.shared.settings;
        assert!(!settings.is_followed(Kind::Artists, &artist.id, true));
        assert!(shared::encode(settings).contains("\"unfollowed\""));
        // Ticked again it is the default, and leaves the file.
        send(&mut app, Message::Toggle(Kind::Artists, artist.id.clone()));
        assert_eq!(app.shared.settings, shared::Settings::default());
        // An actor is off until ticked: the other list.
        let actor = app.follows.of(Kind::Actors)[0].clone();
        assert!(!actor.followed);
        send(&mut app, Message::Toggle(Kind::Actors, actor.id.clone()));
        assert!(app.shared.settings.is_ticked(Kind::Actors, &actor.id));
        assert_eq!(
            app.follows.count_label(Kind::Actors).split(' ').next(),
            Some("1")
        );
    }

    #[test]
    fn unfollow_all_is_offered_while_most_are_followed_and_follow_all_after() {
        let mut app = app();
        send(&mut app, Message::Open(Some(Kind::Artists)));
        let n = app.follows.of(Kind::Artists).len();
        assert!(app.follows.most_followed(Kind::Artists));
        send(&mut app, Message::All);
        assert_eq!(app.follows.followed(Kind::Artists), 0);
        assert_eq!(app.follows.count_label(Kind::Artists), format!("0 of {n}"));
        assert!(!app.follows.most_followed(Kind::Artists));
        send(&mut app, Message::All);
        assert_eq!(app.follows.followed(Kind::Artists), n);
        assert_eq!(app.shared.settings, shared::Settings::default());
        // Actors start unticked, so Follow all is what they offer.
        send(&mut app, Message::Pick(Kind::Actors));
        assert!(!app.follows.most_followed(Kind::Actors));
        send(&mut app, Message::All);
        let actors = app.follows.of(Kind::Actors);
        assert!(!actors.is_empty() && actors.iter().all(|a| a.followed));
    }

    #[test]
    fn the_search_narrows_both_groups_by_name() {
        let mut app = app();
        send(&mut app, Message::Open(Some(Kind::Artists)));
        let state = app.following.clone().unwrap();
        let (all, _) = rows(&app, &state);
        let name = all[0].name.clone();
        let n = all.len();
        send(&mut app, Message::Search(name.to_uppercase()));
        let state = app.following.clone().unwrap();
        let (found, gone) = rows(&app, &state);
        assert!(!found.is_empty() && found.len() < n);
        assert!(found.iter().any(|f| f.name == name) && gone.is_empty());
        send(&mut app, Message::Search("zzzz no such".into()));
        let state = app.following.clone().unwrap();
        let (found, gone) = rows(&app, &state);
        assert!(found.is_empty() && gone.is_empty());
        assert_eq!(
            Kind::Artists.no_match_text(&state.query),
            "No artists match \"zzzz no such\"."
        );
    }

    #[test]
    fn the_window_draws_its_kinds_groups_and_rows() {
        use iced_test::simulator;
        let mut app = app();
        send(&mut app, Message::Open(Some(Kind::Artists)));
        let name = app.follows.of(Kind::Artists)[0].name.clone();
        let view = app.view();
        let mut ui = simulator(view);
        for label in [
            "Following",
            "Shows and anime",
            "Series and collections",
            "New albums",
            "Unfollow all",
            "FOLLOWED FOR NEW ALBUMS",
            "Done",
            name.as_str(),
        ] {
            assert!(ui.find(label).is_ok(), "{label}");
        }
    }
}
