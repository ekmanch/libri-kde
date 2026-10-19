//! The Halloween and Christmas artwork from the mockup (itself ported
//! from the phone's S11 and S12): the plan's banner strip, the overview
//! card's art, the collection card's art and the title icons. The SVGs in
//! `assets/seasonal/` keep `{{name}}` holes for the colors that follow the
//! theme; each picture is filled in once per theme and kept.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use iced::widget::svg::{self, Svg};
use iced::{Color, ContentFit, Element, Length};

use crate::plans::Season;
use crate::theme::{Theme, Tokens};

/// Which picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Art {
    /// The 120 px strip above a seasonal plan.
    Banner,
    /// The top of the plan's card on the overview.
    Card,
    /// The left of a seasonal collection's card.
    Collection,
    /// By the plan's title: the pumpkin, Santa.
    Icon,
    /// The cobweb in the Halloween banner's corner.
    Web,
}

fn source(season: Season, art: Art) -> &'static str {
    match (season, art) {
        (Season::Halloween, Art::Banner) => {
            include_str!("../../assets/seasonal/halloween-banner.svg")
        }
        (Season::Halloween, Art::Card) => include_str!("../../assets/seasonal/halloween-card.svg"),
        (Season::Halloween, Art::Collection) => {
            include_str!("../../assets/seasonal/halloween-collection.svg")
        }
        (Season::Halloween, Art::Icon) => include_str!("../../assets/seasonal/halloween-icon.svg"),
        (Season::Halloween, Art::Web) => include_str!("../../assets/seasonal/halloween-web.svg"),
        (Season::Christmas, Art::Banner) => {
            include_str!("../../assets/seasonal/christmas-banner.svg")
        }
        (Season::Christmas, Art::Card) => include_str!("../../assets/seasonal/christmas-card.svg"),
        (Season::Christmas, Art::Collection) => {
            include_str!("../../assets/seasonal/christmas-collection.svg")
        }
        (Season::Christmas, Art::Icon) => include_str!("../../assets/seasonal/christmas-icon.svg"),
        // Christmas has no web; the banner stands alone.
        (Season::Christmas, Art::Web) => "",
    }
}

fn css(color: Color) -> String {
    let [r, g, b, _] = color.into_rgba8();
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// The mockup's scene colors for a theme.
fn holes(tokens: &Tokens) -> Vec<(&'static str, String)> {
    let light = tokens.light;
    let hex = |rgb: u32| Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    let pick = |on_light: u32, on_dark: u32| css(hex(if light { on_light } else { on_dark }));
    let card = crate::theme::card_background(tokens);
    vec![
        ("bat", pick(0x7C838C, 0x5A6068)),
        (
            "ground",
            if light {
                css(hex(0xEEF0F3))
            } else {
                css(crate::theme::mix_white(tokens.content, 0.05))
            },
        ),
        (
            "cardGround",
            if light {
                css(hex(0xEEF0F3))
            } else {
                css(crate::theme::mix_white(card, 0.05))
            },
        ),
        (
            "snow",
            if light {
                "url(#sz-snow-l)".into()
            } else {
                "url(#sz-snow-d)".into()
            },
        ),
        ("snowGround", pick(0xE3EAEE, 0xEEF0F2)),
        ("figure", pick(0x8A919A, 0x33373D)),
        ("cat", pick(0x3A3E44, 0x0E0F11)),
        ("cape", pick(0x3A3E44, 0x2E3138)),
        ("sub", css(tokens.sub)),
    ]
}

/// The pictures filled in so far, per season, picture and theme.
type Cache = HashMap<(Season, Art, Theme), svg::Handle>;

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

/// The picture for `season`, filled in for the theme.
pub fn handle(season: Season, art: Art, tokens: &Tokens) -> svg::Handle {
    let key = (season, art, tokens.theme);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .entry(key)
        .or_insert_with(|| {
            let mut text = source(season, art).to_owned();
            for (name, value) in holes(tokens) {
                text = text.replace(&format!("{{{{{name}}}}}"), &value);
            }
            svg::Handle::from_memory(text.into_bytes())
        })
        .clone()
}

/// The picture at `width` × `height`, covering it as the mockup's
/// "slice" does.
pub fn art<'a, Message: 'a>(
    season: Season,
    which: Art,
    width: impl Into<Length>,
    height: impl Into<Length>,
    tokens: &Tokens,
) -> Element<'a, Message> {
    Svg::new(handle(season, which, tokens))
        .width(width)
        .height(height)
        .content_fit(ContentFit::Cover)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{PEACH, Theme};

    #[test]
    fn every_hole_is_filled() {
        for theme in [Theme::Graphite, Theme::Light] {
            let tokens = Tokens::new(theme, PEACH);
            for season in [Season::Halloween, Season::Christmas] {
                for which in [Art::Banner, Art::Card, Art::Collection, Art::Icon, Art::Web] {
                    let mut text = source(season, which).to_owned();
                    for (name, value) in holes(&tokens) {
                        text = text.replace(&format!("{{{{{name}}}}}"), &value);
                    }
                    assert!(!text.contains("{{"), "{season:?} {which:?}");
                }
            }
        }
    }
}
