//! The freeze trace: what Libri's window goes through, written to the
//! same log as `scripts/freeze-monitor.sh` (TODO.md 1g, the desktop
//! freeze), so the two line up by timestamp. `app::run` starts it; the
//! tests never do, and then every call here is a no-op.
//!
//! Each line is `<local time with ms> libri <what>`. Libri writes its
//! window events (focus, size, scale, open, close) and its redraw rate
//! once a second, plus any gap of over a second between two redraws,
//! which is how a present call blocked inside the event loop shows up.
//! Warnings and errors from iced, winit and wgpu, which log through the
//! `log` crate, go to the same file (a lost or outdated surface, a
//! device error).

use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static FILE: OnceLock<Mutex<File>> = OnceLock::new();
/// Seconds east of UTC, so the lines carry the same local time as the
/// monitor's `date -Is`.
static OFFSET: OnceLock<i64> = OnceLock::new();

/// Opens `path` for appending and routes the `log` crate's warnings and
/// errors there too. A second call does nothing.
pub fn start(path: &Path) -> std::io::Result<()> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    if FILE.set(Mutex::new(file)).is_err() {
        return Ok(());
    }
    let _ = OFFSET.set(local_offset());
    log::set_max_level(log::LevelFilter::Warn);
    let _ = log::set_logger(&Logger);
    Ok(())
}

/// Appends one line, if the trace is on.
pub fn line(what: impl Display) {
    let Some(file) = FILE.get() else {
        return;
    };
    if let Ok(mut file) = file.lock() {
        let _ = writeln!(file, "{} libri {what}", timestamp());
    }
}

/// The `log` crate's warnings and errors, with the crate they came from.
struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            line(format_args!(
                "{} [{}] {}",
                record.level().as_str().to_lowercase(),
                record.target(),
                record.args()
            ));
        }
    }

    fn flush(&self) {}
}

/// Counts redraws and notices gaps between them. `redraw` returns the
/// lines worth logging, if any.
#[derive(Debug)]
pub struct Frames {
    count: u32,
    since: Instant,
    last: Option<Instant>,
}

impl Frames {
    pub fn new(now: Instant) -> Self {
        Frames {
            count: 0,
            since: now,
            last: None,
        }
    }

    pub fn redraw(&mut self, now: Instant) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(last) = self.last
            && now.duration_since(last) > Duration::from_secs(1)
        {
            lines.push(format!(
                "redraw after a gap of {:.1} s",
                now.duration_since(last).as_secs_f64()
            ));
        }
        self.last = Some(now);
        self.count += 1;
        let elapsed = now.duration_since(self.since);
        if elapsed >= Duration::from_secs(1) {
            lines.push(format!(
                "{} redraws in {:.1} s",
                self.count,
                elapsed.as_secs_f64()
            ));
            self.count = 0;
            self.since = now;
        }
        lines
    }
}

/// Local time for a file name: `20261027-213005`. UTC until the trace
/// has started, since that is when the offset is read.
pub fn file_stamp() -> String {
    let (year, month, day, secs, _) = local_now();
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// The local calendar date, the seconds into the day and the
/// milliseconds, now.
fn local_now() -> (i64, u32, u32, i64, u32) {
    let offset = *OFFSET.get().unwrap_or(&0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let local = now.as_secs() as i64 + offset;
    let (days, secs) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (year, month, day) = civil(days);
    (year, month, day, secs, now.subsec_millis())
}

/// Local time as `2026-10-24T21:25:03.417+02:00`.
fn timestamp() -> String {
    let offset = *OFFSET.get().unwrap_or(&0);
    let (year, month, day, secs, millis) = local_now();
    let (sign, magnitude) = if offset < 0 {
        ('-', -offset)
    } else {
        ('+', offset)
    };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}{sign}{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60,
        millis,
        magnitude / 3600,
        magnitude % 3600 / 60
    )
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The zone offset from `date +%z` (`+0200`), 0 when it cannot be read.
fn local_offset() -> i64 {
    let Ok(output) = std::process::Command::new("date").arg("+%z").output() else {
        return 0;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    let Some((sign, digits)) = text.split_at_checked(1) else {
        return 0;
    };
    let (Ok(hours), Ok(minutes)) = (
        digits.get(0..2).unwrap_or("").parse::<i64>(),
        digits.get(2..4).unwrap_or("").parse::<i64>(),
    ) else {
        return 0;
    };
    let seconds = hours * 3600 + minutes * 60;
    if sign == "-" { -seconds } else { seconds }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_750), (2026, 10, 24));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(19_782), (2024, 2, 29));
    }

    #[test]
    fn frames_report_the_rate_and_the_gaps() {
        let start = Instant::now();
        let mut frames = Frames::new(start);
        assert!(frames.redraw(start).is_empty());
        assert!(frames.redraw(start + Duration::from_millis(500)).is_empty());
        let lines = frames.redraw(start + Duration::from_millis(1100));
        assert_eq!(lines, vec!["3 redraws in 1.1 s".to_owned()]);
        let lines = frames.redraw(start + Duration::from_millis(4600));
        assert_eq!(
            lines,
            vec![
                "redraw after a gap of 3.5 s".to_owned(),
                "1 redraws in 3.5 s".to_owned()
            ]
        );
    }

    #[test]
    fn lines_without_a_file_are_dropped() {
        line("nothing happens");
    }
}
