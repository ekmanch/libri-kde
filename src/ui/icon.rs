//! The mockup's icons, exported to `assets/icons/` and drawn with iced's
//! `svg` widget, tinted by the theme. The two timeline markers keep their
//! own colors.

use iced::widget::svg::{self, Svg};
use iced::{Color, Element};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Calendar,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Close,
    Dnf,
    Document,
    ExternalLink,
    Filter,
    Key,
    MarkerGhost,
    MarkerStar,
    More,
    Pin,
    Plus,
    Search,
    Settings,
    Sliders,
    Stack,
    ViewGrid,
    ViewList,
    ViewTable,
}

impl Icon {
    pub const ALL: [Icon; 23] = [
        Icon::Calendar,
        Icon::Check,
        Icon::ChevronDown,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::Close,
        Icon::Dnf,
        Icon::Document,
        Icon::ExternalLink,
        Icon::Filter,
        Icon::Key,
        Icon::MarkerGhost,
        Icon::MarkerStar,
        Icon::More,
        Icon::Pin,
        Icon::Plus,
        Icon::Search,
        Icon::Settings,
        Icon::Sliders,
        Icon::Stack,
        Icon::ViewGrid,
        Icon::ViewList,
        Icon::ViewTable,
    ];

    fn bytes(self) -> &'static [u8] {
        macro_rules! file {
            ($name:literal) => {
                include_bytes!(concat!("../../assets/icons/", $name, ".svg"))
            };
        }
        match self {
            Icon::Calendar => file!("calendar"),
            Icon::Check => file!("check"),
            Icon::ChevronDown => file!("chevron-down"),
            Icon::ChevronLeft => file!("chevron-left"),
            Icon::ChevronRight => file!("chevron-right"),
            Icon::Close => file!("close"),
            Icon::Dnf => file!("dnf"),
            Icon::Document => file!("document"),
            Icon::ExternalLink => file!("external-link"),
            Icon::Filter => file!("filter"),
            Icon::Key => file!("key"),
            Icon::MarkerGhost => file!("marker-ghost"),
            Icon::MarkerStar => file!("marker-star"),
            Icon::More => file!("more"),
            Icon::Pin => file!("pin"),
            Icon::Plus => file!("plus"),
            Icon::Search => file!("search"),
            Icon::Settings => file!("settings"),
            Icon::Sliders => file!("sliders"),
            Icon::Stack => file!("stack"),
            Icon::ViewGrid => file!("view-grid"),
            Icon::ViewList => file!("view-list"),
            Icon::ViewTable => file!("view-table"),
        }
    }

    /// Whether the icon is drawn in one color and takes the theme's tint.
    fn is_tinted(self) -> bool {
        !matches!(self, Icon::MarkerGhost | Icon::MarkerStar)
    }
}

/// `which` at `size` px, tinted `color` unless it is a colored marker.
pub fn icon<'a, Message>(which: Icon, size: f32, color: Color) -> Element<'a, Message> {
    let tint = which.is_tinted().then_some(color);
    Svg::new(svg::Handle::from_memory(which.bytes()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: tint })
        .into()
}
