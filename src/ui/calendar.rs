//! A month calendar for picking a day or a period: the plan item's
//! "When". A click picks one day; a second click on a later day makes it a
//! period.

use iced::widget::{Column, Row, button, column, container, row, space, text};
use iced::{Background, Border, Center, Element, Fill};

use super::{FocusExt, Icon, icon_button, mono};
use crate::model::{Date, PlanWhen};
use crate::theme::{Tokens, font};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// The month shown, as its first day.
pub fn month_start(date: Date) -> Date {
    Date { day: 1, ..date }
}

/// The first day of the month before or after `month`.
pub fn shift_month(month: Date, by: i32) -> Date {
    let index = i32::from(month.year) * 12 + i32::from(month.month) - 1 + by;
    Date {
        year: (index / 12) as u16,
        month: (index % 12 + 1) as u8,
        day: 1,
    }
}

/// The `When` after a click on `day`: the period from `anchor` when the
/// day comes after it, otherwise that day alone.
pub fn pick(anchor: Option<Date>, day: Date) -> PlanWhen {
    match anchor {
        Some(from) if day > from => PlanWhen { from, to: day },
        _ => PlanWhen { from: day, to: day },
    }
}

/// One month: a header with the month and arrows, the weekday letters,
/// and the days; the picked day or period in the accent, today ringed.
#[allow(clippy::too_many_arguments)]
pub fn calendar<'a, Message: Clone + 'a>(
    month: Date,
    picked: Option<PlanWhen>,
    today: Date,
    accent: iced::Color,
    on_accent: iced::Color,
    on_day: impl Fn(Date) -> Message + 'a,
    on_month: impl Fn(i32) -> Message + 'a,
    tokens: &Tokens,
) -> Element<'a, Message> {
    let header = row![
        icon_button(Icon::ChevronLeft, 16.0, tokens, on_month(-1)),
        text(format!(
            "{} {}",
            MONTHS[usize::from(month.month) - 1],
            month.year
        ))
        .size(14)
        .font(font::SEMIBOLD)
        .width(Fill)
        .center(),
        icon_button(Icon::ChevronRight, 16.0, tokens, on_month(1)),
    ]
    .align_y(Center);
    let names = Row::with_children(WEEKDAYS.map(|d| {
        container(text(d).size(11.5).color(tokens.sub))
            .center_x(36)
            .into()
    }));

    let first = month_start(month);
    let next = shift_month(month, 1);
    let mut weeks = Column::new().spacing(2);
    let mut day = first.add_days(-(first.weekday() as i64));
    let tokens = *tokens;
    while day < next {
        let mut week = Row::new();
        for _ in 0..7 {
            let in_month = day.month == month.month;
            let chosen = picked.is_some_and(|w| w.from <= day && day <= w.to);
            let is_today = day == today;
            let label = mono(
                day.day.to_string(),
                12.5,
                if chosen {
                    on_accent
                } else if in_month {
                    tokens.text
                } else {
                    tokens.sub.scale_alpha(0.5)
                },
            );
            week = week.push(
                button(container(label).center(Fill))
                    .width(36)
                    .height(32)
                    .padding(0)
                    .on_press(on_day(day))
                    .style(move |_, status| button::Style {
                        background: if chosen {
                            Some(Background::Color(accent))
                        } else {
                            matches!(status, button::Status::Hovered)
                                .then_some(Background::Color(tokens.hover))
                        },
                        border: Border::default()
                            .rounded(8)
                            .width(if is_today && !chosen { 1.0 } else { 0.0 })
                            .color(tokens.sub),
                        ..button::Style::default()
                    })
                    .focusable(8.0, &tokens),
            );
            day = day.add_days(1);
        }
        weeks = weeks.push(week);
    }
    column![header, names, weeks, space().height(2)]
        .spacing(6)
        .width(252)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn a_second_later_click_makes_a_period() {
        assert_eq!(
            pick(None, date("2026-10-19")),
            PlanWhen {
                from: date("2026-10-19"),
                to: date("2026-10-19")
            }
        );
        assert_eq!(
            pick(Some(date("2026-10-19")), date("2026-10-31")),
            PlanWhen {
                from: date("2026-10-19"),
                to: date("2026-10-31")
            }
        );
        assert_eq!(
            pick(Some(date("2026-10-19")), date("2026-10-02")).from,
            date("2026-10-02")
        );
    }

    #[test]
    fn months_shift_across_years() {
        assert_eq!(shift_month(date("2026-12-01"), 1), date("2027-01-01"));
        assert_eq!(shift_month(date("2026-01-01"), -1), date("2025-12-01"));
    }
}
