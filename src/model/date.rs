//! Calendar dates as `library.json` writes them.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A calendar date, written as "2026-10-01". Used for `added` and the
/// `from` and `to` of a plan item's `when`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Parses "2026-10-01". Out-of-range months and days are refused.
    pub fn parse(text: &str) -> Option<Date> {
        match Released::parse(text)? {
            Released {
                year,
                month: Some(month),
                day: Some(day),
            } => Some(Date { year, month, day }),
            _ => None,
        }
    }

    /// The day after this one.
    pub fn next_day(self) -> Date {
        if self.day < days_in_month(self.year, self.month) {
            Date {
                day: self.day + 1,
                ..self
            }
        } else if self.month < 12 {
            Date {
                month: self.month + 1,
                day: 1,
                ..self
            }
        } else {
            Date {
                year: self.year + 1,
                month: 1,
                day: 1,
            }
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// A release date with flexible precision: "1999", "2026-12" or
/// "2026-12-25".
///
/// A partial date counts as released only once its period has ended, so
/// "2027" is "Not released yet" until 2028-01-01 (DESIGN.md section 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Released {
    pub year: u16,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

impl Released {
    /// Parses "1999", "2026-12" or "2026-12-25". A day needs a month, and
    /// both must exist in the calendar.
    pub fn parse(text: &str) -> Option<Released> {
        let mut parts = text.split('-');
        let year = number(parts.next()?, 4)?;
        // Two digits always fit a u8.
        let mut two_digits = || match parts.next() {
            Some(part) => number(part, 2).map(|n| Some(n as u8)),
            None => Some(None),
        };
        let month = two_digits()?;
        let day = two_digits()?;
        if parts.next().is_some() {
            return None;
        }
        if let Some(month) = month
            && !(1..=12).contains(&month)
        {
            return None;
        }
        if let (Some(month), Some(day)) = (month, day)
            && !(1..=days_in_month(year, month)).contains(&day)
        {
            return None;
        }
        Some(Released { year, month, day })
    }

    /// The first day on which the title counts as released.
    pub fn available_from(self) -> Date {
        match (self.month, self.day) {
            (Some(month), Some(day)) => Date {
                year: self.year,
                month,
                day,
            },
            (Some(month), None) => Date {
                year: self.year,
                month,
                day: days_in_month(self.year, month),
            }
            .next_day(),
            _ => Date {
                year: self.year + 1,
                month: 1,
                day: 1,
            },
        }
    }

    pub fn is_released_on(self, today: Date) -> bool {
        today >= self.available_from()
    }
}

impl fmt::Display for Released {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(month) = self.month {
            write!(f, "-{month:02}")?;
        }
        if let Some(day) = self.day {
            write!(f, "-{day:02}")?;
        }
        Ok(())
    }
}

/// Exactly `digits` ASCII digits.
fn number(text: &str, digits: usize) -> Option<u16> {
    if text.len() != digits || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Date::parse(&text).ok_or_else(|| de::Error::custom(format!("not a date: \"{text}\"")))
    }
}

impl Serialize for Released {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Released {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Released::parse(&text)
            .ok_or_else(|| de::Error::custom(format!("not a release date: \"{text}\"")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> Date {
        Date::parse(text).unwrap()
    }

    #[test]
    fn parses_and_writes_the_three_precisions() {
        for text in ["1999", "2026-12", "2026-12-25"] {
            assert_eq!(Released::parse(text).unwrap().to_string(), text);
        }
    }

    #[test]
    fn refuses_impossible_dates() {
        for text in [
            "99",
            "2026-13",
            "2026-02-30",
            "2026-1-05",
            "2026-12-25-1",
            "",
        ] {
            assert_eq!(Released::parse(text), None, "{text}");
        }
        assert_eq!(Date::parse("2026-12"), None);
        assert!(Date::parse("2024-02-29").is_some());
    }

    #[test]
    fn a_partial_date_is_released_after_its_period_ends() {
        let year = Released::parse("2027").unwrap();
        assert!(!year.is_released_on(date("2027-12-31")));
        assert!(year.is_released_on(date("2028-01-01")));

        let month = Released::parse("2026-12").unwrap();
        assert!(!month.is_released_on(date("2026-12-31")));
        assert!(month.is_released_on(date("2027-01-01")));

        let day = Released::parse("2026-12-25").unwrap();
        assert!(!day.is_released_on(date("2026-12-24")));
        assert!(day.is_released_on(date("2026-12-25")));
    }
}
