//! The phone app's text rules for ids, tags and platforms
//! (`entry_id.dart`, `similar_tags.dart`, `platform_names.dart`), so both
//! apps name things the same way.

use super::MediaType;

/// Lowercases, drops apostrophes and folds common Latin diacritics to
/// ASCII; every other character is kept.
pub fn fold_to_ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            '\'' | '’' | '`' => {}
            _ => match fold(c) {
                Some(folded) => out.push_str(folded),
                None => out.push(c),
            },
        }
    }
    out
}

fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ė' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ı' | 'ī' | 'į' => "i",
        'ð' | 'đ' => "d",
        'ñ' | 'ń' | 'ņ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ő' | 'ō' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ů' | 'ű' | 'ū' | 'ų' => "u",
        'ý' | 'ÿ' => "y",
        'þ' => "th",
        'ß' => "ss",
        'ł' | 'ļ' => "l",
        'ś' | 'š' | 'ş' | 'ș' => "s",
        'ż' | 'ź' | 'ž' => "z",
        'ř' => "r",
        'ğ' | 'ģ' => "g",
        'œ' => "oe",
        'ķ' => "k",
        'ț' => "t",
        _ => return None,
    })
}

/// "Salem's Lot" is `salems-lot`: folded, then every run of other
/// characters one hyphen, with no hyphen at either end.
pub fn slugify(text: &str) -> String {
    let folded = fold_to_ascii(text);
    let mut slug = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_owned()
}

/// A new entry id: `movies/the-mummy-1999`, `music/dirt-alice-in-chains`.
/// Music uses the artist as the suffix, every other type the year. Made
/// unique with "-2", "-3".
pub fn entry_id<'a>(
    media: MediaType,
    title: &str,
    year: Option<u16>,
    artist: Option<&str>,
    existing: impl IntoIterator<Item = &'a str> + Clone,
) -> String {
    let suffix = match media {
        MediaType::Music => artist.map(slugify),
        _ => year.map(|y| y.to_string()),
    };
    let parts: Vec<String> = [Some(slugify(title)), suffix]
        .into_iter()
        .flatten()
        .filter(|p| !p.is_empty())
        .collect();
    let slug = if parts.is_empty() {
        "untitled".to_owned()
    } else {
        parts.join("-")
    };
    let base = format!("{}/{slug}", media.key());
    let taken = |id: &str| existing.clone().into_iter().any(|e| e == id);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("an unused id")
}

/// A tag is lowercase ASCII letters, digits and hyphens, plus a dot
/// between two digits ("2.5d"). Drops a leading "#", folds, and turns every
/// run of other characters into one hyphen.
pub fn normalize_tag(raw: &str) -> String {
    let raw = raw.trim();
    let folded = fold_to_ascii(raw.strip_prefix('#').unwrap_or(raw));
    let chars: Vec<char> = folded.chars().collect();
    let mut tag = String::with_capacity(chars.len());
    for (i, &c) in chars.iter().enumerate() {
        let digit_dot = c == '.'
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_ascii_lowercase() || c.is_ascii_digit() || digit_dot {
            tag.push(c);
        } else if !tag.ends_with('-') {
            tag.push('-');
        }
    }
    tag.trim_matches('-').to_owned()
}

/// The existing tag most like `candidate`, or none when nothing is close:
/// the same with hyphens and dots ignored, a plural of the other, or one
/// edit apart (two for six letters or more), where swapping two neighbors
/// is one edit. An exact match is the same tag, not a similar one.
pub fn find_similar_tag<'a>(
    candidate: &str,
    existing: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let name = normalize_tag(candidate);
    if name.is_empty() {
        return None;
    }
    existing
        .into_iter()
        .filter(|&other| other != name)
        .filter_map(|other| similarity_rank(&name, other).map(|rank| (rank, other)))
        .min_by_key(|&(rank, _)| rank)
        .map(|(_, other)| other)
}

/// Lower is closer.
fn similarity_rank(a: &str, b: &str) -> Option<usize> {
    let bare = |s: &str| s.replace(['-', '.'], "");
    if bare(a) == bare(b) {
        return Some(0);
    }
    if is_plural_of(a, b) || is_plural_of(b, a) {
        return Some(1);
    }
    let limit = if a.len() >= 6 { 2 } else { 1 };
    let distance = edit_distance(a, b);
    (distance <= limit).then_some(1 + distance)
}

fn is_plural_of(plural: &str, singular: &str) -> bool {
    plural == format!("{singular}s")
        || plural == format!("{singular}es")
        || singular
            .strip_suffix('y')
            .is_some_and(|stem| plural == format!("{stem}ies"))
}

/// Optimal string alignment distance: substitutions, insertions,
/// deletions and swaps of two neighbors each cost one.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

/// A platform as the app shows and saves it: spelled out, one spelling per
/// platform ("PS5" is "PlayStation 5").
pub fn normalize_platform(name: &str) -> String {
    let trimmed = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let key: String = trimmed
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_' | '.' | '/'))
        .collect();
    let known = match key.as_str() {
        "ps" | "ps1" | "psx" | "psone" | "playstation" | "playstation1" => "PlayStation",
        "ps2" | "playstation2" => "PlayStation 2",
        "ps3" | "playstation3" => "PlayStation 3",
        "ps4" | "playstation4" => "PlayStation 4",
        "ps5" | "playstation5" => "PlayStation 5",
        "psp" | "playstationportable" => "PlayStation Portable",
        "psvita" | "vita" | "playstationvita" => "PlayStation Vita",
        "xbox" => "Xbox",
        "xbox360" | "x360" => "Xbox 360",
        "xb1" | "xbone" | "xboxone" => "Xbox One",
        "xsx" | "xss" | "xboxseries" | "xboxseriesx" | "xboxseriess" | "xboxseriesxs"
        | "xboxseriesx|s" => "Xbox Series X|S",
        "ns" | "switch" | "nintendoswitch" => "Switch",
        "ns2" | "switch2" | "nintendoswitch2" => "Switch 2",
        "pc" | "windows" | "win" => "PC",
        "mac" | "macos" => "Mac",
        "linux" => "Linux",
        "steamdeck" => "Steam Deck",
        "ios" => "iOS",
        "android" => "Android",
        _ => return trimmed,
    };
    known.to_owned()
}

/// "PlayStation 5, PC" in an entry's `platform` field, as a list.
pub fn split_platforms(text: Option<&str>) -> Vec<String> {
    text.unwrap_or_default()
        .split(',')
        .filter(|part| !part.trim().is_empty())
        .map(normalize_platform)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_fold_and_drop_apostrophes() {
        assert_eq!(slugify("Salem's Lot"), "salems-lot");
        assert_eq!(slugify("Amélie"), "amelie");
        assert_eq!(slugify("Jo Nesbø"), "jo-nesbo");
        assert_eq!(slugify("  Dune: Part Two! "), "dune-part-two");
    }

    #[test]
    fn entry_ids_follow_the_phone_and_stay_unique() {
        let existing = ["movies/the-mummy-1999"];
        assert_eq!(
            entry_id(MediaType::Movies, "The Mummy", Some(1999), None, existing),
            "movies/the-mummy-1999-2"
        );
        assert_eq!(
            entry_id(
                MediaType::Music,
                "Dirt",
                Some(1992),
                Some("Alice in Chains"),
                existing
            ),
            "music/dirt-alice-in-chains"
        );
        assert_eq!(
            entry_id(MediaType::Books, "", None, None, existing),
            "books/untitled"
        );
    }

    #[test]
    fn tags_are_cleaned_to_the_tag_rule() {
        assert_eq!(normalize_tag("#Science Fiction"), "science-fiction");
        assert_eq!(normalize_tag("2.5D"), "2.5d");
        assert_eq!(normalize_tag("rock 'n' roll"), "rock-n-roll");
        assert_eq!(normalize_tag("...dark..."), "dark");
    }

    #[test]
    fn similar_tags_are_found_before_creating_one() {
        let existing = ["scifi", "horror", "comedy", "metroidvania"];
        assert_eq!(find_similar_tag("sci-fi", existing), Some("scifi"));
        assert_eq!(find_similar_tag("jorror", existing), Some("horror"));
        assert_eq!(find_similar_tag("comedies", existing), Some("comedy"));
        assert_eq!(
            find_similar_tag("metroidvnaia", existing),
            Some("metroidvania")
        );
        assert_eq!(find_similar_tag("western", existing), None);
        assert_eq!(find_similar_tag("horror", existing), None, "the same tag");
    }

    #[test]
    fn the_similar_tag_check_folds_case_and_punctuation_and_prefers_the_closest() {
        let existing = [
            "scifi",
            "sci-fi-horror",
            "rpg",
            "arpg",
            "comedy",
            "comedies-of-manners",
        ];
        // Case, spaces and hyphens fold before comparing.
        assert_eq!(find_similar_tag("Sci Fi", existing), Some("scifi"));
        assert_eq!(find_similar_tag("SCI-FI", existing), Some("scifi"));
        // A short tag allows one edit, a long one two.
        assert_eq!(find_similar_tag("rpgs", existing), Some("rpg"), "a plural");
        assert_eq!(find_similar_tag("rpb", existing), Some("rpg"));
        assert_eq!(
            find_similar_tag("rxx", existing),
            None,
            "two edits on a short tag"
        );
        assert_eq!(find_similar_tag("comedx", existing), Some("comedy"));
        assert_eq!(
            find_similar_tag("comdey", existing),
            Some("comedy"),
            "a swap"
        );
        // The closest wins: the same letters beat a plural beats an edit.
        assert_eq!(find_similar_tag("comedies", existing), Some("comedy"));
        assert_eq!(find_similar_tag("", existing), None);
    }

    #[test]
    fn platforms_are_spelled_out() {
        assert_eq!(normalize_platform("PS5"), "PlayStation 5");
        assert_eq!(normalize_platform(" nintendo  switch "), "Switch");
        assert_eq!(normalize_platform("Amiga"), "Amiga");
        assert_eq!(split_platforms(Some("ps5, PC,")), ["PlayStation 5", "PC"]);
    }
}
