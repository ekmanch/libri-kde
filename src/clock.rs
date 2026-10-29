//! Moments in time as GitHub reports them and as the sync files keep
//! them: seconds since the epoch, written as RFC 3339 in UTC
//! ("2026-10-27T19:12:03Z"). Local dates and times use the computer's
//! zone offset, read once through `trace::utc_offset`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::Date;

/// A moment, as seconds since 1970-01-01T00:00:00Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl Timestamp {
    pub fn now() -> Timestamp {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Timestamp(secs as i64)
    }

    /// Parses what GitHub and the phone write: "2026-10-27T19:12:03Z",
    /// "2026-10-27T19:12:03.123456Z", "2026-10-27T21:12:03+02:00",
    /// "2026-10-27 19:12:03 UTC" and "2026-10-27 19:12:03 +0500" (the
    /// token expiration header). Fractions of a second are dropped.
    pub fn parse(text: &str) -> Option<Timestamp> {
        let text = text.trim();
        let (date, rest) = text.split_at_checked(10)?;
        let date = Date::parse(date)?;
        let rest = rest.strip_prefix(['T', 't', ' '])?;
        let (hours, rest) = number(rest, 2)?;
        let rest = rest.strip_prefix(':')?;
        let (minutes, rest) = number(rest, 2)?;
        let rest = rest.strip_prefix(':')?;
        let (seconds, mut rest) = number(rest, 2)?;
        if hours > 23 || minutes > 59 || seconds > 60 {
            return None;
        }
        if let Some(after) = rest.strip_prefix('.') {
            rest = after.trim_start_matches(|c: char| c.is_ascii_digit());
        }
        let offset = zone_offset(rest.trim())?;
        let secs = date.days() * 86_400 + hours * 3600 + minutes * 60 + seconds - offset;
        Some(Timestamp(secs))
    }

    /// "2026-10-27T19:12:03Z".
    pub fn to_rfc3339(self) -> String {
        let (date, secs) = self.split(0);
        format!(
            "{date}T{:02}:{:02}:{:02}Z",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        )
    }

    /// The calendar date and the seconds into the day, `offset` seconds
    /// east of UTC.
    fn split(self, offset: i64) -> (Date, i64) {
        let local = self.0 + offset;
        let days = local.div_euclid(86_400);
        (Date::from_days(days), local.rem_euclid(86_400))
    }

    /// The date on this computer's clock.
    pub fn local_date(self) -> Date {
        self.split(crate::trace::utc_offset()).0
    }

    /// Whole calendar days from `now`'s local date to this one: 0 for
    /// today, 1 for tomorrow, negative for a day gone by.
    pub fn days_from(self, now: Timestamp) -> i64 {
        now.local_date().days_until(self.local_date())
    }

    /// "Oct 13, 20:12", in local time: the conflict dialog's last commit.
    pub fn month_day_time(self) -> String {
        let (date, secs) = self.split(crate::trace::utc_offset());
        format!(
            "{} {}, {:02}:{:02}",
            MONTHS[usize::from(date.month) - 1],
            date.day,
            secs / 3600,
            secs % 3600 / 60
        )
    }

    /// How long ago this was, in the phone's words: "just now", "1 minute
    /// ago", "5 minutes ago", "2 hours ago", "yesterday", "3 days ago",
    /// and from 30 days "Mar 14, 2026".
    pub fn ago(self, now: Timestamp) -> String {
        let secs = now.0 - self.0;
        if secs < 60 {
            return "just now".into();
        }
        let minutes = secs / 60;
        if minutes < 60 {
            return plural(minutes, "minute");
        }
        let hours = minutes / 60;
        if hours < 24 {
            return plural(hours, "hour");
        }
        let days = hours / 24;
        if days == 1 {
            return "yesterday".into();
        }
        if days < 30 {
            return format!("{days} days ago");
        }
        let date = self.local_date();
        format!(
            "{} {}, {}",
            MONTHS[usize::from(date.month) - 1],
            date.day,
            date.year
        )
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{n} {unit}s ago")
    }
}

/// `digits` decimal digits at the start of `text`, and the rest.
fn number(text: &str, digits: usize) -> Option<(i64, &str)> {
    let (head, rest) = text.split_at_checked(digits)?;
    if !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head.parse().ok()?, rest))
}

/// Seconds east of UTC for a zone suffix: "Z", "UTC", "GMT", "+02:00",
/// "+0200", "-05:30", or nothing (taken as UTC).
fn zone_offset(text: &str) -> Option<i64> {
    match text {
        "" | "Z" | "z" | "UTC" | "GMT" => return Some(0),
        _ => {}
    }
    let (sign, digits) = text.split_at_checked(1)?;
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let digits = digits.replace(':', "");
    let (hours, rest) = number(&digits, 2)?;
    let (minutes, rest) = number(rest, 2)?;
    if !rest.is_empty() || hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_formats_github_and_the_phone_write() {
        let t = Timestamp::parse("2026-10-27T19:12:03Z").unwrap();
        assert_eq!(t.to_rfc3339(), "2026-10-27T19:12:03Z");
        assert_eq!(Timestamp::parse("2026-10-27T19:12:03.123456Z"), Some(t));
        assert_eq!(Timestamp::parse("2026-10-27T21:12:03+02:00"), Some(t));
        assert_eq!(Timestamp::parse("2026-10-27 19:12:03 UTC"), Some(t));
        assert_eq!(Timestamp::parse("2026-10-28 00:12:03 +0500"), Some(t));
        assert_eq!(Timestamp::parse("2026-10-27 14:12:03 -0500"), Some(t));
        assert_eq!(Timestamp::parse("2026-10-27"), None);
        assert_eq!(Timestamp::parse("2026-10-27T25:00:00Z"), None);
        assert_eq!(Timestamp::parse("yesterday"), None);
        assert_eq!(Timestamp(0).to_rfc3339(), "1970-01-01T00:00:00Z");
        assert_eq!(Timestamp(-1).to_rfc3339(), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn ago_uses_the_phones_words() {
        let now = Timestamp::parse("2026-10-27T12:00:00Z").unwrap();
        let at = |s: &str| Timestamp::parse(s).unwrap();
        assert_eq!(at("2026-10-27T11:59:30Z").ago(now), "just now");
        assert_eq!(at("2026-10-27T11:59:00Z").ago(now), "1 minute ago");
        assert_eq!(at("2026-10-27T11:40:00Z").ago(now), "20 minutes ago");
        assert_eq!(at("2026-10-27T11:00:00Z").ago(now), "1 hour ago");
        assert_eq!(at("2026-10-27T01:00:00Z").ago(now), "11 hours ago");
        assert_eq!(at("2026-10-26T01:00:00Z").ago(now), "yesterday");
        assert_eq!(at("2026-10-24T12:00:00Z").ago(now), "3 days ago");
        assert_eq!(at("2026-03-14T12:00:00Z").ago(now), "Mar 14, 2026");
        assert_eq!(at("2026-10-27T12:00:05Z").ago(now), "just now");
    }

    #[test]
    fn the_zone_offset_reads_both_spellings() {
        assert_eq!(zone_offset("+02:00"), Some(7200));
        assert_eq!(zone_offset("-0530"), Some(-19_800));
        assert_eq!(zone_offset("UTC"), Some(0));
        assert_eq!(zone_offset("+2"), None);
        assert_eq!(zone_offset("CET"), None);
    }
}
