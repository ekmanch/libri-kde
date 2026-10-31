//! The text rules the lookups share with the phone (`text_clean.dart`,
//! `book_match.dart`, `languages.dart`, `creator_tags.dart`,
//! `similar_tags.dart`): plain text from HTML, title comparison, the
//! book merge rules, language and streaming tags.

use crate::model::{MediaType, text};

/// Plain text from the HTML-ish descriptions AniList, Apple Books and
/// Google Books return: tags dropped, `<br>` and paragraph ends become
/// line breaks, common entities decoded, whitespace collapsed.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = after[..end].trim().to_lowercase();
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if name == "br" {
            out.push('\n');
        } else if tag.starts_with('/') && (name == "p" || name == "div") {
            out.push_str("\n\n");
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    let text = decode_entities(&out).replace('\u{a0}', " ");
    collapse_whitespace(&text)
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(';').filter(|&end| end <= 10) else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let entity = &after[1..end];
        let decoded = match entity {
            "nbsp" => Some(' '),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Runs of spaces and tabs become one space, spaces around a line break
/// go, and three or more line breaks become two.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut newlines = 0;
    let mut space = false;
    for c in text.chars() {
        match c {
            '\n' => {
                space = false;
                newlines += 1;
            }
            ' ' | '\t' | '\r' => space = true,
            _ => {
                if newlines > 0 {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    for _ in 0..newlines.min(2) {
                        out.push('\n');
                    }
                    newlines = 0;
                    space = false;
                } else if space {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push(' ');
                    }
                    space = false;
                }
                out.push(c);
            }
        }
    }
    out.trim().to_owned()
}

/// What `same_title` compares: the title lowercased, without punctuation,
/// spaces or a leading "The". Two titles with the same key are the same.
pub fn title_key(title: &str) -> String {
    let mut key = String::with_capacity(title.len());
    let mut space = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        if c == '\'' || c == '’' {
            continue;
        }
        if c.is_alphanumeric() {
            if space && !key.is_empty() {
                key.push(' ');
            }
            space = false;
            key.push(c);
        } else {
            space = true;
        }
    }
    let key = key.strip_prefix("the ").unwrap_or(&key);
    key.replace(' ', "")
}

/// Whether two titles are the same once case, punctuation and a leading
/// "The" are set aside: "Salem's Lot" is "Salems lot".
pub fn same_title(a: &str, b: &str) -> bool {
    title_key(a) == title_key(b)
}

/// Edition notes a provider tacks on, which `loose_title` drops: "Dirt
/// (2022 Remaster)", "Rooster - Remastered", "Dune [Deluxe Edition]".
const EDITION_WORDS: [&str; 14] = [
    "remaster",
    "remastered",
    "edition",
    "deluxe",
    "expanded",
    "anniversary",
    "version",
    "bonus",
    "explicit",
    "live",
    "mono",
    "stereo",
    "reissue",
    "special",
];

fn has_edition_word(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("re-issue")
        || lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .any(|w| EDITION_WORDS.contains(&w))
}

/// The title without an edition note at its end.
pub fn loose_title(title: &str) -> String {
    let mut out = title.to_owned();
    // Bracketed notes anywhere: "(2022 Remaster)", "[Deluxe Edition]".
    while let Some((open, close)) = find_bracket_note(&out) {
        let mut start = open;
        while start > 0 && out.as_bytes()[start - 1] == b' ' {
            start -= 1;
        }
        out.replace_range(start..=close, "");
    }
    // " - Remastered", " - 2009 Remaster", " - Live": the rest goes.
    let lower = out.to_lowercase();
    let mut search = 0;
    while let Some(at) = lower[search..].find(" - ") {
        let at = search + at;
        let after = lower[at + 3..].trim_start();
        let after = after
            .split_once(' ')
            .filter(|(year, _)| year.len() == 4 && year.bytes().all(|b| b.is_ascii_digit()))
            .map_or(after, |(_, rest)| rest);
        let word: String = after.chars().take_while(|c| c.is_alphanumeric()).collect();
        if ["remaster", "remastered", "live", "mono", "stereo"].contains(&word.as_str()) {
            out.truncate(at);
            break;
        }
        search = at + 3;
    }
    out.trim().to_owned()
}

/// The first bracketed group that holds an edition word: its byte range.
fn find_bracket_note(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let close_char = match bytes[i] {
            b'(' => b')',
            b'[' => b']',
            _ => {
                i += 1;
                continue;
            }
        };
        let len = text[i + 1..].find(close_char as char)?;
        let close = i + 1 + len;
        if has_edition_word(&text[i + 1..close]) {
            return Some((i, close));
        }
        i = close + 1;
    }
    None
}

/// Whether two titles are the same once edition notes are set aside too.
pub fn same_loose_title(a: &str, b: &str) -> bool {
    same_title(&loose_title(a), &loose_title(b))
}

/// The key a stored track rating is matched to a listed track by, when
/// their ids differ.
pub fn track_title_key(title: &str) -> String {
    title_key(&loose_title(title))
}

/// The one hit for an entry: the same title in the same year; else the
/// same title a year off; else, when no year is known, the first with the
/// same title; else, with edition notes set aside, the same again. Nothing
/// when the title is not there.
pub fn pick_by_title_and_year<T>(
    hits: &[T],
    title: &str,
    year: Option<u16>,
    title_of: impl Fn(&T) -> &str,
    year_of: impl Fn(&T) -> Option<u16>,
) -> Option<usize> {
    let rules: [fn(&str, &str) -> bool; 2] = [same_title, same_loose_title];
    for same in rules {
        let matches: Vec<usize> = (0..hits.len())
            .filter(|&i| same(title_of(&hits[i]), title))
            .collect();
        if matches.is_empty() {
            continue;
        }
        let Some(year) = year else {
            return matches.first().copied();
        };
        if let Some(&i) = matches.iter().find(|&&i| year_of(&hits[i]) == Some(year)) {
            return Some(i);
        }
        if let Some(&i) = matches
            .iter()
            .find(|&&i| year_of(&hits[i]).is_some_and(|y| y.abs_diff(year) <= 1))
        {
            return Some(i);
        }
    }
    None
}

/// Whether a book subject or category makes sense as a tag: short and
/// plain, no catalog notes ("award:hugo_award=1960") or long headings.
pub fn is_tag_like(subject: &str) -> bool {
    subject.chars().count() <= 24
        && !subject.contains([':', '=', ',', '(', ')', '{', '}', '[', ']', '/'])
        && subject.split_whitespace().count() <= 3
}

/// How many subjects or genres become tag suggestions.
pub const MAX_BOOK_TAGS: usize = 6;

/// Spellings a lookup uses for a tag the library spells otherwise, so a
/// synonym is never offered as a new tag beside the one that is there.
const TAG_ALIASES: [(&str, &str); 24] = [
    ("science-fiction", "scifi"),
    ("sci-fi", "scifi"),
    ("sf", "scifi"),
    ("science-fiction-fiction", "scifi"),
    ("fantasy-fiction", "fantasy"),
    ("horror-fiction", "horror"),
    ("horror-tales", "horror"),
    ("horror-stories", "horror"),
    ("comedies", "comedy"),
    ("comedy-films", "comedy"),
    ("thrillers", "thriller"),
    ("thriller-fiction", "thriller"),
    ("mystery-fiction", "mystery"),
    ("detective-and-mystery-stories", "mystery"),
    ("romance-fiction", "romance"),
    ("love-stories", "romance"),
    ("role-playing-rpg", "rpg"),
    ("role-playing", "rpg"),
    ("platform", "platformer"),
    ("hack-and-slash-beat-em-up", "hack-and-slash"),
    ("turn-based-strategy-tbs", "strategy"),
    ("real-time-strategy-rts", "strategy"),
    ("slice-of-life", "slice-of-life"),
    ("adventure", "adventure"),
];

/// A lookup's genre as the library's tag: normalized with any brackets
/// dropped ("Role-playing (RPG)"), then the library's spelling when the
/// alias table has one.
pub fn canonical_tag(raw: &str) -> String {
    let name = text::normalize_tag(&raw.replace(['(', ')', '[', ']'], " "));
    TAG_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map_or(name, |(_, tag)| (*tag).to_owned())
}

/// The tag a language code names (#japanese for "ja"), keyed by the
/// two-letter codes TMDB and IGDB use and the three-letter ones Open
/// Library uses. A code not listed names no tag.
pub fn language_tag(code: &str) -> Option<&'static str> {
    let base = code
        .trim()
        .to_lowercase()
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_owned();
    LANGUAGES
        .iter()
        .find(|(codes, _)| codes.split(' ').any(|c| c == base))
        .map(|(_, tag)| *tag)
}

const LANGUAGES: [(&str, &str); 48] = [
    ("ar ara", "arabic"),
    ("bg bul", "bulgarian"),
    ("bn ben", "bengali"),
    ("ca cat", "catalan"),
    ("cs ces cze", "czech"),
    ("da dan", "danish"),
    ("de deu ger", "german"),
    ("el ell gre", "greek"),
    ("en eng", "english"),
    ("es spa", "spanish"),
    ("et est", "estonian"),
    ("fa fas per", "persian"),
    ("fi fin", "finnish"),
    ("fr fra fre", "french"),
    ("he heb", "hebrew"),
    ("hi hin", "hindi"),
    ("hr hrv", "croatian"),
    ("hu hun", "hungarian"),
    ("id ind", "indonesian"),
    ("is isl ice", "icelandic"),
    ("it ita", "italian"),
    ("ja jpn", "japanese"),
    ("ko kor", "korean"),
    ("la lat", "latin"),
    ("lt lit", "lithuanian"),
    ("lv lav", "latvian"),
    ("nb nob nn nno no nor", "norwegian"),
    ("nl nld dut", "dutch"),
    ("pl pol", "polish"),
    ("pt por", "portuguese"),
    ("ro ron rum", "romanian"),
    ("ru rus", "russian"),
    ("sk slk slo", "slovak"),
    ("sl slv", "slovenian"),
    ("sr srp", "serbian"),
    ("sv swe", "swedish"),
    ("sw swa", "swahili"),
    ("ta tam", "tamil"),
    ("th tha", "thai"),
    ("tl tgl", "tagalog"),
    ("tr tur", "turkish"),
    ("uk ukr", "ukrainian"),
    ("vi vie", "vietnamese"),
    ("zh zho chi cn", "chinese"),
    ("ms msa may", "malay"),
    ("ur urd", "urdu"),
    ("ga gle", "irish"),
    ("cy cym wel", "welsh"),
];

/// The language tag the form suggests for an entry of `media` whose lookup
/// says it is in `code`: none for English, which nearly everything is in,
/// and none for Japanese on an anime, for the same reason.
pub fn language_suggestion(media: MediaType, code: Option<&str>) -> Option<&'static str> {
    let tag = language_tag(code?)?;
    if tag == "english" || (media == MediaType::Anime && tag == "japanese") {
        return None;
    }
    Some(tag)
}

/// The language of an AniList entry from its country of origin.
pub fn language_of_country(country: Option<&str>) -> Option<&'static str> {
    match country?.to_uppercase().as_str() {
        "JP" => Some("ja"),
        "KR" => Some("ko"),
        "CN" | "TW" => Some("zh"),
        _ => None,
    }
}

// Books.

/// Whether two book hits are the same book: the same title and an author
/// in common, case and punctuation aside; two without an author need only
/// the title.
pub fn same_book(a: &super::Match, b: &super::Match) -> bool {
    same_title(&a.title, &b.title) && share_author(a.creator.as_deref(), b.creator.as_deref())
}

/// Whether two creator lines ("James Clavell", "A, B") name an author in
/// common, case and punctuation aside; two without an author count as
/// sharing one.
pub fn share_author(a: Option<&str>, b: Option<&str>) -> bool {
    let left = author_keys(a);
    let right = author_keys(b);
    if left.is_empty() || right.is_empty() {
        return left.is_empty() && right.is_empty();
    }
    left.iter().any(|key| right.contains(key))
}

fn author_keys(creator: Option<&str>) -> Vec<String> {
    creator
        .unwrap_or_default()
        .split(',')
        .filter(|name| !name.trim().is_empty())
        .map(|name| {
            name.chars()
                .flat_map(char::to_lowercase)
                .filter(|c| c.is_alphanumeric())
                .collect()
        })
        .collect()
}

/// The edition a book's cover is of, as Pick a cover captions it: a tie-in
/// from the title, else Open Library's physical format ("Mass Market
/// Paperback" reads "Paperback"), else `fallback` ("Ebook" for Apple).
pub fn edition_label(title: &str, format: Option<&str>, fallback: Option<&str>) -> Option<String> {
    let lower = title.to_lowercase();
    if lower.contains("tie-in") || lower.contains("tie in") || lower.contains("tiein") {
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        if words
            .iter()
            .any(|w| ["tv", "television", "series", "netflix", "hbo", "fx"].contains(w))
        {
            return Some("TV tie-in".into());
        }
        if words.iter().any(|w| ["movie", "film"].contains(w)) {
            return Some("Movie tie-in".into());
        }
        return Some("Tie-in".into());
    }
    let f = format.unwrap_or_default().to_lowercase();
    if f.contains("hardcover") || f.contains("hardback") {
        return Some("Hardcover".into());
    }
    if f.contains("paperback") || f.contains("softcover") {
        return Some("Paperback".into());
    }
    if f.contains("audio") {
        return Some("Audiobook".into());
    }
    if f.contains("ebook") || f.contains("e-book") || f.contains("electronic") {
        return Some("Ebook".into());
    }
    fallback.map(str::to_owned)
}

/// The first publication year among a work's edition years: the earliest,
/// except that a year more than `gap` years before the next is taken as a
/// mis-dated edition and skipped while the years after it still have two
/// within `gap` of each other. A book with one edition, or with editions
/// decades apart, keeps its earliest year.
pub fn first_publish_year(years: impl IntoIterator<Item = u16>, gap: u16) -> Option<u16> {
    let mut sorted: Vec<u16> = years.into_iter().collect();
    sorted.sort_unstable();
    sorted.dedup();
    for i in 0..sorted.len() {
        if i == sorted.len() - 1 || sorted[i + 1] - sorted[i] <= gap {
            return Some(sorted[i]);
        }
        let run = (i + 1..sorted.len().saturating_sub(1)).any(|j| sorted[j + 1] - sorted[j] <= gap);
        if !run {
            return Some(sorted[i]);
        }
    }
    None
}

/// A manga or comic volume number in a title: "Berserk, Vol. 43",
/// "Berserk Volume 43", "Berserk #43" give the series "Berserk" and 43.
pub fn volume_of(title: &str) -> Option<(String, u32)> {
    let lower = title.to_lowercase();
    let bytes = lower.as_bytes();
    let is_sep = |b: u8| b == b' ' || b == b',' || b == b':' || b == b'-';
    let mut i = 0;
    while i < bytes.len() {
        // A marker must follow at least one separator (or an en dash).
        let preceded = i > 0
            && (is_sep(bytes[i - 1]) || lower[..i].ends_with('–') || lower[..i].ends_with('—'));
        if preceded {
            for marker in ["volume", "vol.", "vol", "#"] {
                if !lower[i..].starts_with(marker) {
                    continue;
                }
                let after = lower[i + marker.len()..].trim_start();
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.is_empty()
                    || after[digits.len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric())
                {
                    continue;
                }
                let name = title[..i]
                    .trim_end_matches([' ', ',', ':', '–', '—', '-'])
                    .trim();
                if name.is_empty() {
                    continue;
                }
                let number: u32 = digits.parse().ok()?;
                return Some((name.to_owned(), number));
            }
        }
        i += lower[i..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// A title cut before a parenthesis, a colon, or ", Part", ", Book" or
/// ", Vol": "Tai-Pan (Asian Saga" and "Shōgun, Part One" give "Tai-Pan"
/// and "Shōgun". A title that would lose everything stays as it is.
pub fn core_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut cut = None;
    for (i, c) in lower.char_indices() {
        if c == '(' || c == '[' || c == ':' {
            cut = Some(i);
            break;
        }
        if c == ',' {
            let after = lower[i + 1..].trim_start();
            if ["part", "book", "vol"].iter().any(|w| {
                after.starts_with(w)
                    && after[w.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric())
            }) {
                cut = Some(i);
                break;
            }
        }
    }
    let core = match cut {
        Some(i) => title[..i].trim(),
        None => title,
    };
    if core.is_empty() {
        title.to_owned()
    } else {
        core.to_owned()
    }
}

/// "The Lord of the Rings" from TMDB's "The Lord of the Rings Collection".
pub fn collection_name(name: &str) -> String {
    let trimmed = name.trim();
    match trimmed.to_lowercase().strip_suffix("collection") {
        Some(rest) if rest.ends_with(char::is_whitespace) => {
            trimmed[..rest.len()].trim().to_owned()
        }
        _ => trimmed.to_owned(),
    }
}

// Creators and streaming.

/// Companies (as a lookup names them) whose films carry the studio's tag
/// rather than the director's, checked in this order.
const BRAND_STUDIOS: [(&str, &str); 6] = [
    ("studio ghibli", "studio-ghibli"),
    ("pixar", "pixar"),
    ("walt disney animation studios", "disney"),
    ("walt disney productions", "disney"),
    ("disneytoon studios", "disney"),
    ("dreamworks animation", "dreamworks"),
];

/// Directors whose name, with no lookup to say the company, still means
/// the studio.
const HOUSE_DIRECTORS: [(&str, &str); 2] = [
    ("hayao miyazaki", "studio-ghibli"),
    ("isao takahata", "studio-ghibli"),
];

/// The brand among `companies`, or none.
pub fn brand_studio_of<'a>(companies: impl IntoIterator<Item = &'a str>) -> Option<&'static str> {
    let names: Vec<String> = companies.into_iter().map(str::to_lowercase).collect();
    BRAND_STUDIOS
        .iter()
        .find(|(brand, _)| names.iter().any(|n| n.contains(brand)))
        .map(|(_, tag)| *tag)
}

/// The studio tag a director's name means, when it does.
pub fn house_of(director: &str) -> Option<&'static str> {
    let lower = director.trim().to_lowercase();
    HOUSE_DIRECTORS
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, tag)| *tag)
        .or_else(|| brand_studio_of([lower.as_str()]))
}

/// The names in a creator line: "A, B & C and D".
pub fn split_creators(creator: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    let words: Vec<&str> = creator.split(' ').collect();
    let flush = |current: &mut String, names: &mut Vec<String>| {
        let name = current.trim().to_owned();
        if !name.is_empty() {
            names.push(name);
        }
        current.clear();
    };
    for word in words {
        if word == "and" || word == "&" || word == "/" {
            flush(&mut current, &mut names);
            continue;
        }
        for (i, part) in word.split(['&', '/', ',']).enumerate() {
            if i > 0 {
                flush(&mut current, &mut names);
            }
            if !part.is_empty() {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(part);
            }
        }
    }
    flush(&mut current, &mut names);
    names
}

/// Streaming platforms as TMDB names a network or a production company,
/// and their tag, for names that do not slug to it.
const STREAMING_PLATFORMS: [(&str, &str); 16] = [
    ("netflix", "netflix"),
    ("hbo", "hbo"),
    ("hbo max", "hbo"),
    ("max", "hbo"),
    ("hbo films", "hbo"),
    ("apple tv+", "apple-tv"),
    ("apple tv", "apple-tv"),
    ("apple original films", "apple-tv"),
    ("apple studios", "apple-tv"),
    ("disney+", "disney-plus"),
    ("hulu", "hulu"),
    ("prime video", "amazon-prime"),
    ("amazon prime video", "amazon-prime"),
    ("amazon prime", "amazon-prime"),
    ("paramount+", "paramount-plus"),
    ("peacock", "peacock"),
];

/// The Streaming tag `name` (a network or a company) stands for: one of
/// the known platforms, else its slug when `streaming` (the library's
/// Streaming tags) has it.
pub fn streaming_tag(name: &str, streaming: &[String]) -> Option<String> {
    let lower = name.trim().to_lowercase();
    if let Some((_, tag)) = STREAMING_PLATFORMS.iter().find(|(n, _)| *n == lower) {
        return Some((*tag).to_owned());
    }
    let slug = text::slugify(name);
    streaming.contains(&slug).then_some(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_plain_text() {
        assert_eq!(
            strip_html("<p>One &amp; two<br/>three</p><p>Four&nbsp;&#39;five&#x27;   six</p>"),
            "One & two\nthree\n\nFour 'five' six"
        );
        assert_eq!(strip_html("  a <b>bold</b>  word  "), "a bold word");
        assert_eq!(strip_html("x\n\n\n\ny"), "x\n\ny");
    }

    #[test]
    fn titles_compare_without_case_punctuation_or_the() {
        assert!(same_title("Salem's Lot", "Salems lot"));
        assert!(same_title("The Mummy", "Mummy"));
        assert!(!same_title("The Mummy", "The Mummy Returns"));
        assert_eq!(title_key("Shōgun, Part One"), "shōgunpartone");
        assert_eq!(loose_title("Dirt (2022 Remaster)"), "Dirt");
        assert_eq!(loose_title("Rooster - Remastered"), "Rooster");
        assert_eq!(loose_title("Dune [Deluxe Edition]"), "Dune");
        assert_eq!(loose_title("Time - 2011 Remaster"), "Time");
        assert_eq!(loose_title("Weird Fishes/Arpeggi"), "Weird Fishes/Arpeggi");
        assert_eq!(loose_title("Us and Them (Live at Pompeii)"), "Us and Them");
        assert!(same_loose_title("Dirt (2022 Remaster)", "Dirt"));
        assert_eq!(track_title_key("Would? - Remastered"), "would");
    }

    #[test]
    fn the_best_hit_is_by_title_then_year() {
        let hits = [
            ("The Thing", Some(1982)),
            ("The Thing", Some(2011)),
            ("Thing", None),
        ];
        let pick = |title: &str, year: Option<u16>| {
            pick_by_title_and_year(&hits, title, year, |h| h.0, |h| h.1)
        };
        assert_eq!(pick("The Thing", Some(2011)), Some(1));
        assert_eq!(pick("The Thing", Some(1983)), Some(0), "a year off");
        assert_eq!(pick("the thing", None), Some(0));
        assert_eq!(pick("The Thing", Some(1990)), None);
        assert_eq!(pick("Alien", Some(1979)), None);
    }

    #[test]
    fn subjects_and_genres_become_tags() {
        assert!(is_tag_like("Horror"));
        assert!(!is_tag_like("award:hugo_award=1960"));
        assert!(!is_tag_like("Fiction, science fiction, general"));
        assert!(!is_tag_like("A very long heading about things"));
        assert_eq!(canonical_tag("Science Fiction"), "scifi");
        assert_eq!(canonical_tag("Role-playing (RPG)"), "rpg");
        assert_eq!(canonical_tag("Dark"), "dark");
    }

    #[test]
    fn languages_map_to_tags_with_the_phones_exceptions() {
        assert_eq!(language_tag("ja-JP"), Some("japanese"));
        assert_eq!(language_tag("jpn"), Some("japanese"));
        assert_eq!(language_tag("xx"), None);
        assert_eq!(language_suggestion(MediaType::Movies, Some("en")), None);
        assert_eq!(language_suggestion(MediaType::Anime, Some("ja")), None);
        assert_eq!(
            language_suggestion(MediaType::Movies, Some("ko")),
            Some("korean")
        );
        assert_eq!(language_of_country(Some("kr")), Some("ko"));
        assert_eq!(language_of_country(Some("US")), None);
    }

    #[test]
    fn books_merge_by_title_and_author() {
        assert!(!share_author(Some("James Clavell"), Some("Clavell, James")));
        assert!(share_author(
            Some("James Clavell, Someone"),
            Some("james clavell")
        ));
        assert!(share_author(None, None));
        assert!(!share_author(Some("A"), None));
        assert_eq!(
            edition_label("Shōgun (TV Tie-in)", None, None).as_deref(),
            Some("TV tie-in")
        );
        assert_eq!(
            edition_label("X", Some("Mass Market Paperback"), None).as_deref(),
            Some("Paperback")
        );
        assert_eq!(
            edition_label("X", None, Some("Ebook")).as_deref(),
            Some("Ebook")
        );
        assert_eq!(edition_label("X", None, None), None);
    }

    #[test]
    fn the_first_publish_year_skips_a_misdated_edition() {
        assert_eq!(first_publish_year([1962, 1950, 1963, 1970], 10), Some(1962));
        assert_eq!(
            first_publish_year([1897, 1950, 2003], 10),
            Some(1897),
            "decades apart"
        );
        assert_eq!(first_publish_year([2012], 10), Some(2012));
        assert_eq!(first_publish_year([], 10), None);
    }

    #[test]
    fn volumes_series_names_and_core_titles_are_read() {
        assert_eq!(volume_of("Berserk, Vol. 43"), Some(("Berserk".into(), 43)));
        assert_eq!(volume_of("Berserk Volume 43"), Some(("Berserk".into(), 43)));
        assert_eq!(volume_of("Berserk #43"), Some(("Berserk".into(), 43)));
        assert_eq!(volume_of("Volume 43"), None);
        assert_eq!(volume_of("Dracula"), None);
        assert_eq!(core_title("Tai-Pan (Asian Saga"), "Tai-Pan");
        assert_eq!(core_title("Shōgun, Part One"), "Shōgun");
        assert_eq!(core_title("Dune: Part Two"), "Dune");
        assert_eq!(core_title("(x)"), "(x)");
        assert_eq!(
            collection_name("The Lord of the Rings Collection"),
            "The Lord of the Rings"
        );
        assert_eq!(collection_name("Berserk"), "Berserk");
    }

    #[test]
    fn studios_creators_and_streaming_platforms_resolve_to_tags() {
        assert_eq!(
            brand_studio_of(["Walt Disney Pictures", "Pixar"]),
            Some("pixar")
        );
        assert_eq!(brand_studio_of(["Walt Disney Pictures"]), None);
        assert_eq!(house_of("Hayao Miyazaki"), Some("studio-ghibli"));
        assert_eq!(
            split_creators("Jack Thorne, Stephen Graham & Phil / Jo and Bo"),
            ["Jack Thorne", "Stephen Graham", "Phil", "Jo", "Bo"]
        );
        let streaming = vec!["netflix".to_owned(), "mubi".to_owned()];
        assert_eq!(
            streaming_tag("Disney+", &streaming).as_deref(),
            Some("disney-plus")
        );
        assert_eq!(streaming_tag("MUBI", &streaming).as_deref(), Some("mubi"));
        assert_eq!(streaming_tag("ABC", &streaming), None);
    }
}
