//! The details panel at the library's right (decided 2026-10-31):
//! selecting a title never moves the list. Home keeps its own layout,
//! the details taking the place of its right column (`overview`).
//! Elsewhere the panel is pinned (always there, "At a glance" while nothing is
//! selected) or unpinned (the list gets the full width and a selection
//! slides a drawer over the list's right side), remembered per view key
//! like the view. A narrow window unpins it.

use iced::widget::{
    Column, Row, button, column, container, opaque, progress_bar, row, space, text,
};
use iced::{Background, Border, Center, Element, Fill, Padding, Shadow, Vector};

use super::overview::{self, RowStyle};
use super::{Listing, Message};
use crate::app::{App, Message as AppMessage};
use crate::browse;
use crate::model::EntryStatus;
use crate::screens::{entry, shell};
use crate::theme::{self, Tokens, font};
use crate::ui::{self, FocusExt, Icon};

fn msg(message: Message) -> AppMessage {
    AppMessage::Library(message)
}

/// Under this many px of list beside a pinned panel the window counts as
/// narrow, and the panel is a drawer: at the minimum window (1280 px) a
/// pinned panel would leave the list 696 px.
pub const NARROW: f32 = 720.0;

/// How the details panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Home: the details in its right column (`overview`).
    Home,
    /// Always there, At a glance when empty.
    Pinned,
    /// Only with a selection, over the list.
    Drawer,
}

/// Whether the window is too narrow for a pinned panel.
pub fn narrow(app: &App) -> bool {
    shell::content_width(app, entry::details::WIDTH) < NARROW
}

/// How the panel shows now.
pub fn panel(app: &App) -> Panel {
    let state = &app.library;
    if state.is_overview() && state.page.is_none() {
        Panel::Home
    } else if state.pinned() && !narrow(app) {
        Panel::Pinned
    } else {
        Panel::Drawer
    }
}

/// How many recently added titles At a glance lists, and how many tags.
const RECENT: usize = 5;
const TAGS: usize = 6;

/// What the selection puts in the panel: an entry's details or a series.
pub(super) fn details<'a>(app: &'a App) -> Option<Element<'a, AppMessage>> {
    let state = &app.library;
    let library = app.repo.library();
    match (
        state.selected.as_deref().and_then(|id| library.entry(id)),
        state.open_series(),
    ) {
        (Some(entry), _) => Some(entry::details::view(app, entry, None)),
        (None, Some(open)) => entry::series::view(app, open),
        (None, None) => None,
    }
}

/// The pinned panel beside the list: the selection's details, else At a
/// glance.
pub fn side<'a>(app: &'a App, listing: &Listing<'a>) -> Element<'a, AppMessage> {
    let tokens = app.tokens();
    let content =
        details(app).unwrap_or_else(|| entry::details::pane(glance(app, listing, &tokens), None));
    container(content).width(entry::details::WIDTH).into()
}

/// The drawer over the list while something is selected: the details
/// with a pin and a close button above them, a shadow to the left and no
/// scrim. It takes the clicks over it, never the list under it.
pub fn drawer<'a>(app: &'a App) -> Option<Element<'a, AppMessage>> {
    let details = details(app)?;
    let tokens = app.tokens();
    let mut buttons = Row::new().spacing(4);
    if !narrow(app) {
        buttons = buttons.push(small_button(
            Icon::Panel,
            &tokens,
            msg(Message::TogglePanel),
        ));
    }
    buttons = buttons.push(small_button(Icon::Close, &tokens, msg(Message::ClosePanel)));
    let head = container(buttons)
        .width(Fill)
        .align_x(iced::Right)
        .padding(Padding::new(0.0).top(10).right(10));
    let t = tokens;
    // No outline: the shadow alone sets the drawer off the list.
    let face = container(column![head, details])
        .width(entry::details::WIDTH)
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(t.drawer)),
            border: Border {
                radius: iced::border::Radius::default().top_left(12),
                ..Border::default()
            },
            shadow: Shadow {
                color: t.drawer_shadow,
                offset: Vector::new(-18.0, 0.0),
                blur_radius: 44.0,
            },
            ..container::Style::default()
        });
    Some(
        container(opaque(face))
            .width(Fill)
            .height(Fill)
            .align_x(iced::Right)
            .into(),
    )
}

/// A 32 px borderless icon button in the secondary color.
fn small_button<'a>(which: Icon, tokens: &Tokens, on_press: AppMessage) -> Element<'a, AppMessage> {
    button(container(ui::icon(which, 17.0, tokens.sub)).center(Fill))
        .width(32)
        .height(32)
        .padding(0)
        .on_press(on_press)
        .style(ui::row_style(tokens, false, 8.0))
        .focusable(8.0, tokens)
}

/// The toolbar's panel button, beside the view switch: filled while
/// pinned. None on Home and in a narrow window, where it cannot change
/// anything.
pub fn toggle<'a>(app: &App) -> Option<Element<'a, AppMessage>> {
    if panel(app) == Panel::Home || narrow(app) {
        return None;
    }
    let tokens = app.tokens();
    let pinned = app.library.pinned();
    let (which, color) = if pinned {
        (Icon::PanelPinned, tokens.text)
    } else {
        (Icon::Panel, tokens.sub)
    };
    let t = tokens;
    Some(
        button(container(ui::icon(which, 17.0, color)).center(Fill))
            .width(38)
            .height(36)
            .padding(0)
            .on_press(msg(Message::TogglePanel))
            .style(move |_, status| button::Style {
                background: if pinned {
                    Some(Background::Color(t.selection))
                } else if matches!(status, button::Status::Hovered) {
                    Some(Background::Color(t.hover))
                } else {
                    None
                },
                border: Border::default().rounded(8).width(1).color(t.line),
                ..button::Style::default()
            })
            .focusable(8.0, &tokens),
    )
}

/// The pick from Want for `pick` (moved on by "Another"), from the list's
/// own order so it is the same on every frame.
pub fn pick<'a>(listing: &Listing<'a>, pick: u64) -> Option<&'a crate::model::Entry> {
    let wants: Vec<_> = listing
        .shown
        .iter()
        .copied()
        .filter(|e| e.status == EntryStatus::Want)
        .collect();
    if wants.is_empty() {
        return None;
    }
    // A spread, so "Another" does not just step to the neighbor.
    let mixed = pick.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17);
    Some(wants[(mixed % wants.len() as u64) as usize])
}

/// Pinned with nothing selected: the list at a glance.
fn glance<'a>(app: &'a App, listing: &Listing<'a>, tokens: &Tokens) -> Element<'a, AppMessage> {
    let state = &app.library;
    let single = state.single();
    let color = single.map_or(tokens.sub, |m| tokens.type_color(m));
    let shown = &listing.shown;
    let narrowed = state.filter_count() > 0 || state.status.is_some() || !state.query.is_empty();
    let line = format!(
        "{} {} {}",
        shown.len(),
        if shown.len() == 1 { "title" } else { "titles" },
        if narrowed {
            "match your filters"
        } else {
            "in your library"
        }
    );
    let head = column![
        ui::mono("AT A GLANCE", 11.0, color),
        text(state.title()).size(24).font(font::EXTRABOLD),
        text(line).size(13.5).color(tokens.sub),
    ]
    .spacing(4);

    // The status counts over the type, the filter and the search, so every
    // bar is there to click whatever the status chips say.
    let statuses: Vec<EntryStatus> = browse::status_options(&state.types);
    let counts: Vec<(EntryStatus, usize)> = statuses
        .iter()
        .map(|&s| (s, listing.base.iter().filter(|e| e.status == s).count()))
        .collect();
    let most = counts.iter().map(|(_, n)| *n).max().unwrap_or(0).max(1);
    let (line_color, t) = (tokens.line, *tokens);
    let stats = Column::with_children(counts.into_iter().map(|(status, n)| {
        let bar = progress_bar(0.0..=most as f32, n as f32)
            .girth(6)
            .style(move |_| progress_bar::Style {
                background: Background::Color(line_color),
                bar: Background::Color(color),
                border: Border::default().rounded(3),
            });
        let label = match single {
            Some(media) => browse::status_label(Some(media), status),
            None => browse::status_label(None, status),
        };
        button(
            row![
                text(label).size(13.5).width(96),
                container(bar).width(Fill),
                container(ui::mono(n.to_string(), 12.0, t.sub))
                    .width(40)
                    .align_x(iced::Right),
            ]
            .spacing(10)
            .align_y(Center),
        )
        .width(Fill)
        .padding([3, 0])
        .on_press(msg(Message::Status(Some(status))))
        .style(move |_, _| button::Style {
            text_color: t.text,
            ..button::Style::default()
        })
        .focusable(6.0, tokens)
    }))
    .spacing(3);
    let mut items = Column::new()
        .spacing(24)
        .push(head)
        .push(column![ui::section_header("Status", tokens), stats].spacing(6));

    if let Some(entry) = pick(listing, state.pick) {
        let another = button(text("Another").size(13).font(font::SEMIBOLD))
            .padding(0)
            .on_press(msg(Message::AnotherPick))
            .style(move |_, _| button::Style {
                text_color: t.ink,
                ..button::Style::default()
            })
            .focusable(6.0, tokens);
        let sub = match entry.years(app.today) {
            Some(years) => format!("{} · {years}", browse::kind_label(entry)),
            None => browse::kind_label(entry).to_owned(),
        };
        let card = button(
            row![
                ui::plain_cover(
                    app.artwork.cover(entry),
                    theme::cover_color(&entry.id),
                    44.0,
                    66.0,
                    5.0
                ),
                column![
                    text(&entry.title).size(14.5).font(font::SEMIBOLD),
                    text(sub).size(12.5).color(t.sub),
                ]
                .spacing(3)
                .width(Fill),
            ]
            .spacing(12)
            .align_y(Center),
        )
        .width(Fill)
        .padding(10)
        .on_press(msg(Message::Select(entry.id.clone())))
        .style(move |_, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, button::Status::Hovered) {
                    theme::mix_white(t.raised, if t.light { 0.0 } else { 0.04 })
                } else {
                    t.raised
                },
            )),
            border: Border::default().rounded(10),
            text_color: t.text,
            ..button::Style::default()
        })
        .focusable(10.0, tokens);
        items = items.push(
            column![
                row![
                    ui::section_header("Pick something", tokens),
                    space::horizontal(),
                    another
                ]
                .align_y(Center),
                card
            ]
            .spacing(9),
        );
    }

    let mut recent: Vec<_> = shown.to_vec();
    browse::Sort::NewestAdded.apply(&mut recent);
    if !recent.is_empty() {
        let rows = Column::with_children(recent.into_iter().take(RECENT).map(|entry| {
            overview::overview_row(
                app,
                entry,
                overview::short_day(entry.added, app.today),
                RowStyle::Dot,
                false,
                tokens,
            )
        }));
        items = items.push(
            column![
                container(ui::section_header("Recently added", tokens))
                    .padding(Padding::new(0.0).bottom(4)),
                rows
            ]
            .spacing(0),
        );
    }

    let mut tags = std::collections::BTreeMap::<&str, usize>::new();
    for tag in shown.iter().flat_map(|e| &e.tags) {
        *tags.entry(tag).or_default() += 1;
    }
    let mut tags: Vec<_> = tags.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    if !tags.is_empty() {
        let chips = Row::with_children(tags.into_iter().take(TAGS).map(|(tag, n)| {
            button(
                row![
                    text(format!("#{tag}")).size(13),
                    ui::mono(n.to_string(), 11.5, t.sub)
                ]
                .spacing(6)
                .align_y(Center),
            )
            .height(28)
            .padding([0, 10])
            .on_press(msg(Message::PickTag(tag.to_owned())))
            .style(move |_, status| button::Style {
                background: Some(Background::Color(
                    if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        t.raised
                    },
                )),
                border: Border::default().rounded(14),
                text_color: t.text,
                ..button::Style::default()
            })
            .focusable(14.0, tokens)
        }))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
        items = items.push(column![ui::section_header("Most used tags", tokens), chips].spacing(9));
    }
    items = items.push(
        text("Click a title to see its details here.")
            .size(12.5)
            .color(tokens.sub),
    );
    items.into()
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers};

    use super::*;
    use crate::app::KeyPress;
    use crate::browse::{Overview, Section};
    use crate::history::Direction;
    use crate::model::{Date, MediaType};
    use crate::screens::Screen;
    use crate::screens::library::View;

    fn app() -> App {
        let mut app = App::new(Screen::Library);
        app.today = Date::parse("2026-10-19").unwrap();
        app.window = iced::Size::new(1600.0, 900.0);
        app
    }

    fn movies(app: &mut App) -> Vec<String> {
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Movies))));
        app.library
            .listing(app.repo.library(), app.today)
            .shown
            .iter()
            .map(|e| e.id.clone())
            .collect()
    }

    #[test]
    fn the_panel_is_pinned_per_view_and_home_always_has_it() {
        let mut app = app();
        assert_eq!(panel(&app), Panel::Home);
        // Ctrl+I on Home changes nothing.
        let _ = app.update(AppMessage::KeyPressed(
            Key::Character("i".into()),
            Modifiers::CTRL,
        ));
        assert!(app.library.panels.is_empty());

        movies(&mut app);
        assert_eq!(panel(&app), Panel::Pinned, "pinned by default");
        let _ = app.update(AppMessage::KeyPressed(
            Key::Character("i".into()),
            Modifiers::CTRL,
        ));
        assert_eq!(panel(&app), Panel::Drawer);
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Books))));
        assert_eq!(panel(&app), Panel::Pinned, "remembered per type");
        let _ = app.update(msg(Message::ShowType(Some(MediaType::Movies))));
        assert_eq!(panel(&app), Panel::Drawer);
        let _ = app.update(msg(Message::TogglePanel));
        assert_eq!(panel(&app), Panel::Pinned);
    }

    #[test]
    fn a_narrow_window_unpins_the_panel_and_hides_the_button() {
        let mut app = app();
        movies(&mut app);
        app.window = iced::Size::new(1280.0, 720.0);
        assert_eq!(panel(&app), Panel::Drawer);
        assert!(toggle(&app).is_none());
        let _ = app.update(msg(Message::TogglePanel));
        assert!(app.library.pinned(), "the choice stays for a wider window");
        app.window = iced::Size::new(1600.0, 900.0);
        assert_eq!(panel(&app), Panel::Pinned);
    }

    #[test]
    fn esc_and_back_close_the_panel_before_going_back() {
        let mut app = app();
        let ids = movies(&mut app);
        let _ = app.update(msg(Message::TogglePanel));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.library.selected, None, "Esc closes the drawer");

        let _ = app.update(msg(Message::Select(ids[1].clone())));
        let _ = app.update(AppMessage::History(Direction::Back));
        assert_eq!(app.library.selected, None, "Back closes it first");
        assert_eq!(app.library.single(), Some(MediaType::Movies));
        let _ = app.update(AppMessage::History(Direction::Back));
        assert!(app.library.is_overview(), "then goes back");

        // Home: Esc brings the plan back.
        let id = Overview::of(app.repo.library(), app.today).list(Section::RecentlyAdded)[0]
            .id
            .clone();
        let _ = app.update(msg(Message::Select(id)));
        let _ = app.update(AppMessage::Key(KeyPress::Escape));
        assert_eq!(app.library.selected, None);
    }

    #[test]
    fn at_a_glance_picks_from_want_and_another_moves_on() {
        let mut app = app();
        movies(&mut app);
        let listing = app.library.listing(app.repo.library(), app.today);
        let first = pick(&listing, 0).expect("the fixture has movies to want");
        assert_eq!(first.status, EntryStatus::Want);
        let wants = listing
            .shown
            .iter()
            .filter(|e| e.status == EntryStatus::Want)
            .count();
        if wants > 1 {
            assert!(
                (1..8).any(|n| pick(&listing, n).map(|e| &e.id) != Some(&first.id)),
                "Another finds a different one"
            );
        }
    }

    #[test]
    fn a_click_in_the_drawer_never_reaches_the_list() {
        let mut app = app();
        let ids = movies(&mut app);
        let _ = app.update(msg(Message::SetView(View::List)));
        let _ = app.update(msg(Message::TogglePanel));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        let size = crate::app::WINDOW_SIZE;
        app.window = size;
        let click = |x: f32, y: f32| {
            let mut ui = iced_test::Simulator::with_size(crate::app::settings(), size, app.view());
            ui.point_at(iced::Point::new(x, y));
            let _ = ui.simulate(iced_test::simulator::click());
            ui.into_messages()
                .filter(|m| matches!(m, AppMessage::Library(Message::Select(_))))
                .count()
        };
        // The list rows run under the drawer; a click on the drawer's empty
        // part selects nothing, a click left of it selects a row.
        let row_y = 250.0;
        assert_eq!(click(size.width - 40.0, size.height - 60.0), 0);
        assert_eq!(click(size.width - 200.0, row_y), 0);
        assert_eq!(click(shell::SIDEBAR_WIDTH + 200.0, row_y), 1);
    }

    #[test]
    fn the_toolbar_never_moves_with_the_panel() {
        let mut app = app();
        let ids = movies(&mut app);
        app.window = crate::app::WINDOW_SIZE;
        let place = |app: &App, label: &str| {
            let mut ui = iced_test::Simulator::with_size(
                crate::app::settings(),
                crate::app::WINDOW_SIZE,
                app.view(),
            );
            ui.find(label).expect(label).bounds()
        };
        let pinned = (place(&app, "Add"), place(&app, "Movies"));
        let _ = app.update(msg(Message::Select(ids[0].clone())));
        assert_eq!((place(&app, "Add"), place(&app, "Movies")), pinned);
        let _ = app.update(msg(Message::TogglePanel));
        assert_eq!(
            (place(&app, "Add"), place(&app, "Movies")),
            pinned,
            "drawer open"
        );
        let _ = app.update(msg(Message::ClosePanel));
        assert_eq!(
            (place(&app, "Add"), place(&app, "Movies")),
            pinned,
            "unpinned"
        );
        // The pinned panel starts under the toolbar and the chips.
        let _ = app.update(msg(Message::TogglePanel));
        let glance = place(&app, "AT A GLANCE");
        assert!(glance.y > place(&app, "All").y + 20.0);
    }
}
