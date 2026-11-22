//! Measuring text before it is laid out (2026-11-07), for the details
//! panel, which fits its sections to its height instead of scrolling: how
//! wide a label is, how many lines a paragraph takes at a width, and where
//! to cut it with an ellipsis. The paragraphs are shaped with iced's own
//! font system, so the measure is the one the widgets get.

use iced::advanced::graphics::text::Paragraph;
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::{Font, Pixels, Point, Size};

fn paragraph(content: &str, size: f32, line_height: f32, font: Font, width: f32) -> Paragraph {
    Paragraph::with_text(Text {
        content,
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(size),
        line_height: text::LineHeight::Relative(line_height),
        font,
        align_x: text::Alignment::Left,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Auto,
        wrapping: text::Wrapping::Word,
    })
}

/// How wide `content` is on one line of `size` px in `font`.
pub fn text_width(content: &str, size: f32, font: Font) -> f32 {
    if content.is_empty() {
        return 0.0;
    }
    paragraph(content, size, 1.0, font, f32::INFINITY).min_width()
}

/// How many lines `content` takes wrapped at `width`, at `size` px with
/// `line_height` times the size per line. Nothing is no lines.
pub fn line_count(content: &str, size: f32, line_height: f32, font: Font, width: f32) -> usize {
    if content.trim().is_empty() {
        return 0;
    }
    let height = paragraph(content, size, line_height, font, width).min_height();
    (height / (size * line_height)).round().max(1.0) as usize
}

/// `content` as one run of text (its line breaks become spaces, the full
/// text being a click away), cut to at most `max_lines` lines at `width`
/// and ending in an ellipsis when it ran longer; whether it was cut.
pub fn clamp(
    content: &str,
    size: f32,
    line_height: f32,
    font: Font,
    width: f32,
    max_lines: usize,
) -> (String, bool) {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if max_lines == 0 {
        return (String::new(), !flat.is_empty());
    }
    if line_count(&flat, size, line_height, font, width) <= max_lines {
        return (flat, false);
    }
    let ellipsis = text_width("…", size, font);
    let line_px = size * line_height;
    let shaped = paragraph(&flat, size, line_height, font, width);
    // The character level with the end of the last line that shows, the
    // ellipsis' width before it.
    let point = Point::new(
        (width - ellipsis - 2.0).max(0.0),
        (max_lines as f32 - 0.5) * line_px,
    );
    let mut cut = shaped
        .hit_test(point)
        .map(|hit| hit.cursor())
        .unwrap_or(flat.len())
        .min(flat.len());
    while !flat.is_char_boundary(cut) {
        cut -= 1;
    }
    // Back to the end of a word, then shorter still if the ellipsis makes
    // the text wrap once more.
    let mut end = flat[..cut].trim_end().len();
    for _ in 0..8 {
        let candidate = format!("{}…", flat[..end].trim_end());
        if line_count(&candidate, size, line_height, font, width) <= max_lines {
            return (candidate, true);
        }
        end = match flat[..end].trim_end().rfind(' ') {
            Some(space) => space,
            None => return ("…".into(), true),
        };
    }
    (format!("{}…", flat[..end].trim_end()), true)
}

/// `content` and then `tail` on one line no wider than `width`: as they
/// are when they fit, else `content` cut with an ellipsis after it and
/// the tail kept whole ("The Duffer Bro… · 5 seasons"). The cut is at the
/// last letter that fits or, with `whole_words`, back at the end of a word
/// (a single long word to a letter). The rows of a list ask this on every
/// layout, so the answers are remembered.
pub fn one_line(
    content: &str,
    tail: &str,
    size: f32,
    font: Font,
    width: f32,
    whole_words: bool,
) -> String {
    use std::cell::RefCell;
    use std::collections::HashMap;
    type Key = (String, String, u32, Font, u32, bool);
    thread_local! {
        static CUTS: RefCell<HashMap<Key, String>> = RefCell::new(HashMap::new());
    }
    let key = (
        content.to_owned(),
        tail.to_owned(),
        size.to_bits(),
        font,
        width.to_bits(),
        whole_words,
    );
    if let Some(cut) = CUTS.with_borrow(|cuts| cuts.get(&key).cloned()) {
        return cut;
    }
    let whole = format!("{content}{tail}");
    let cut = if text_width(&whole, size, font) <= width {
        whole
    } else {
        let room = width - text_width(tail, size, font);
        format!("{}{tail}", cut_line(content, size, font, room, whole_words))
    };
    CUTS.with_borrow_mut(|cuts| {
        // A window being resized asks at every width on the way.
        if cuts.len() >= 4096 {
            cuts.clear();
        }
        cuts.insert(key, cut.clone());
    });
    cut
}

/// `content`, too wide for `width`, cut to it with an ellipsis.
fn cut_line(content: &str, size: f32, font: Font, width: f32, whole_words: bool) -> String {
    let room = width - text_width("…", size, font);
    let shaped = paragraph(content, size, 1.0, font, f32::INFINITY);
    let mut end = shaped
        .hit_test(Point::new(room.max(0.0), size * 0.5))
        .map(|hit| hit.cursor())
        .unwrap_or(content.len())
        .min(content.len());
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    // In the middle of a word: back to the end of the one before.
    if whole_words
        && !content[end..].starts_with(char::is_whitespace)
        && let Some(space) = content[..end].rfind(char::is_whitespace)
    {
        end = space;
    }
    loop {
        let kept = content[..end].trim_end();
        let candidate = format!("{kept}…");
        if kept.is_empty() || text_width(&candidate, size, font) <= width {
            return candidate;
        }
        // Still too wide (the hit is to the nearest letter): one less.
        end = kept.char_indices().next_back().map_or(0, |(at, _)| at);
    }
}

/// How many rows `widths` take laid out in a row that wraps at
/// `row_width` with `spacing` between items; nothing is no rows.
pub fn wrapped_rows(widths: &[f32], row_width: f32, spacing: f32) -> usize {
    let mut rows = 0;
    let mut x = 0.0;
    for &w in widths {
        let need = if x > 0.0 { spacing + w } else { w };
        if x > 0.0 && x + need > row_width {
            rows += 1;
            x = w;
        } else {
            if x == 0.0 {
                rows += 1;
            }
            x += need;
        }
    }
    rows
}

/// How many of `widths` fit in `max_rows` rows of `row_width` with the
/// tail items after them: `tail(rest)` is the widths of what follows
/// the kept ones when `rest` of them are left out (a "+N" chip when
/// some are, and the "+ Tag" chip). The largest number that fits, which
/// may be all of them.
pub fn fold(
    widths: &[f32],
    row_width: f32,
    spacing: f32,
    max_rows: usize,
    tail: impl Fn(usize) -> Vec<f32>,
) -> usize {
    let fits = |keep: usize| {
        let mut all: Vec<f32> = widths[..keep].to_vec();
        all.extend(tail(widths.len() - keep));
        wrapped_rows(&all, row_width, spacing) <= max_rows
    };
    (0..=widths.len())
        .rev()
        .find(|&keep| fits(keep))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::font;

    #[test]
    fn one_line_is_kept_when_it_fits_and_cut_with_an_ellipsis_before_its_tail() {
        let (name, tail) = ("The Duffer Brothers", " · 5 seasons");
        let f = font::REGULAR;
        let full = text_width("The Duffer Brothers · 5 seasons", 14.0, f);
        assert_eq!(
            one_line(name, tail, 14.0, f, full + 1.0, false),
            "The Duffer Brothers · 5 seasons"
        );
        // The name is cut in a word; the seasons stay whole.
        let cut = one_line(name, tail, 14.0, f, full - 10.0, false);
        assert!(cut.starts_with("The Duffer Bro"), "{cut}");
        assert!(cut.ends_with("… · 5 seasons"), "{cut}");
        assert!(text_width(&cut, 14.0, f) <= full - 10.0);
        // The tags keep their two spaces and are cut at a whole tag; one
        // long word is cut to a letter.
        let tags = "#comedy  #horror  #lighthearted";
        let width = text_width("#comedy  #horror  #lig", 13.0, f);
        assert_eq!(
            one_line(tags, "", 13.0, f, width, true),
            "#comedy  #horror…"
        );
        let word = one_line("#lighthearted", "", 13.0, f, 50.0, true);
        assert!(word.starts_with("#li") && word.ends_with('…'), "{word}");
        assert_eq!(one_line("#lighthearted", "", 13.0, f, 2.0, true), "…");
    }

    #[test]
    fn a_short_text_fits_and_a_long_one_is_cut_with_an_ellipsis() {
        let short = "A quiet film.";
        assert_eq!(
            clamp(short, 14.0, 1.55, font::REGULAR, 308.0, 2),
            (short.to_owned(), false)
        );
        let long = "The Lord of the Rings: The Fellowship of the Ring is the first part \
                    of the trilogy, following Frodo as he leaves the Shire with the One \
                    Ring and a fellowship of nine sets out for Mordor.";
        let (cut, was_cut) = clamp(long, 14.0, 1.55, font::REGULAR, 308.0, 2);
        assert!(was_cut, "longer than 2 lines");
        assert!(cut.ends_with('…'));
        assert!(cut.len() < long.len());
        assert!(line_count(&cut, 14.0, 1.55, font::REGULAR, 308.0) <= 2);
        assert!(
            cut.len() > 40,
            "two lines hold more than a few words: {cut}"
        );
        assert_eq!(
            clamp(long, 14.0, 1.55, font::REGULAR, 308.0, 0),
            (String::new(), true)
        );
    }

    #[test]
    fn line_breaks_become_spaces_and_an_empty_text_is_no_lines() {
        assert_eq!(line_count("", 14.0, 1.5, font::REGULAR, 300.0), 0);
        assert_eq!(line_count("One line", 14.0, 1.5, font::REGULAR, 300.0), 1);
        let two = "First paragraph that runs on for a while to be sure it wraps around.\n\n\
                   Second paragraph, also long enough to wrap when the width is narrow.";
        let (cut, was_cut) = clamp(two, 14.0, 1.45, font::ITALIC, 280.0, 1);
        assert!(was_cut && !cut.contains('\n'));
        let (flat, was_cut) = clamp("Two\nlines", 14.0, 1.45, font::ITALIC, 280.0, 2);
        assert_eq!((flat.as_str(), was_cut), ("Two lines", false));
    }

    #[test]
    fn rows_wrap_and_chips_fold_to_what_fits() {
        assert_eq!(wrapped_rows(&[], 300.0, 6.0), 0);
        assert_eq!(wrapped_rows(&[100.0, 100.0, 80.0], 300.0, 6.0), 1);
        assert_eq!(wrapped_rows(&[100.0, 100.0, 100.0], 300.0, 6.0), 2);
        assert_eq!(wrapped_rows(&[320.0, 10.0], 300.0, 6.0), 2);
        // Ten chips of 90 px in two rows of 300 with a "+N" (40) and a
        // "+ Tag" (60) after them: five fit, with 40 + 60 on the second row.
        let widths = [90.0; 10];
        let keep = fold(&widths, 300.0, 6.0, 2, |rest| {
            let mut tail = Vec::new();
            if rest > 0 {
                tail.push(40.0);
            }
            tail.push(60.0);
            tail
        });
        assert_eq!(keep, 5);
        // Three chips fit outright, with the "+ Tag" alone after them.
        assert_eq!(
            fold(&widths[..3], 300.0, 6.0, 2, |rest| if rest > 0 {
                vec![40.0, 60.0]
            } else {
                vec![60.0]
            }),
            3
        );
    }
}
