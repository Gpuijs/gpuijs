//! Resolved CSS longhands -> gpui `StyleRefinement`.

use gpui::{
    AbsoluteLength, AlignContent, AlignItems, AlignSelf, BorderStyle, BoxShadow, CursorStyle,
    DefiniteLength, Display, Fill, Filter as GpuiFilter, FlexDirection, FontFeatures, GridAutoFlow, FlexWrap, FontStyle, FontWeight, GridLocation,
    GridPlacement, GridTrack, Hsla, JustifyContent, Length, ObjectFit, Overflow, Point, Position, Rgba,
    SharedString, StrikethroughStyle, StyleRefinement, TextAlign, TextOverflow,
    LinearColorStop, TextStyleRefinement, UnderlineStyle, Visibility, WhiteSpace, WordBreak,
    linear_color_stop, linear_gradient_stops, point, px, radial_gradient,
};
use lightningcss::properties::Property;
use lightningcss::properties::align::{
    AlignContent as CssAlignContent, AlignItems as CssAlignItems, AlignSelf as CssAlignSelf,
    ContentDistribution, ContentPosition, GapValue, JustifyContent as CssJustify, JustifyItems,
    JustifySelf, SelfPosition,
};
use lightningcss::properties::animation::{
    AnimationDirection, AnimationFillMode as CssFillMode, AnimationIterationCount, AnimationName,
    AnimationPlayState,
};
use lightningcss::properties::background::{BackgroundClip, BackgroundRepeatKeyword, BackgroundSize};
use lightningcss::properties::border::{BorderSideWidth, LineStyle};
use lightningcss::properties::contain::{ContainerNameList, ContainerType};
use lightningcss::properties::display::{
    Display as CssDisplay, DisplayInside, DisplayKeyword, DisplayOutside,
    Visibility as CssVisibility,
};
use lightningcss::properties::effects::{Filter, FilterList};
use lightningcss::properties::flex::{FlexDirection as CssFlexDirection, FlexWrap as CssFlexWrap};
use lightningcss::properties::font::{
    AbsoluteFontSize, AbsoluteFontWeight, FontFamily, FontSize, FontStyle as CssFontStyle,
    FontWeight as CssFontWeight, GenericFontFamily, LineHeight, RelativeFontSize,
};
use lightningcss::properties::list::{CounterStyle, ListStylePosition, ListStyleType, PredefinedCounterStyle};
use lightningcss::properties::grid::{
    GridAutoFlow as CssGridAutoFlow, GridLine, GridTemplateAreas, RepeatCount, TrackBreadth, TrackListItem, TrackSize, TrackSizing,
};
use lightningcss::properties::outline::OutlineStyle;
use lightningcss::properties::overflow::{OverflowKeyword, TextOverflow as CssTextOverflow};
use lightningcss::properties::position::Position as CssPosition;
use lightningcss::properties::position::ZIndex;
use lightningcss::properties::size::{MaxSize, Size};
use lightningcss::properties::svg::SVGPaint;
use lightningcss::properties::text::{
    OverflowWrap, Spacing, TextTransformCase, WordBreak as CssWordBreak,
};
use lightningcss::properties::text::{
    TextAlign as CssTextAlign, TextDecorationLine, TextDecorationStyle, TextDecorationThickness,
    WhiteSpace as CssWhiteSpace,
};
use lightningcss::properties::transform::{Rotate, Scale, Transform, Translate};
use lightningcss::properties::ui::CursorKeyword;
use lightningcss::stylesheet::PrinterOptions;
use lightningcss::traits::{Parse, ToCss};
use lightningcss::values::calc::{Calc, MathFunction};
use lightningcss::values::color::CssColor;
use lightningcss::values::easing::EasingFunction;
use lightningcss::values::gradient::{
    Circle, Ellipse, EndingShape, Gradient, GradientItem, LineDirection, ShapeExtent,
};
use lightningcss::values::image::Image;
use lightningcss::values::length::{
    Length as CssLength, LengthPercentage, LengthPercentageOrAuto, LengthValue,
};
use lightningcss::values::percentage::DimensionPercentage;
use lightningcss::values::percentage::NumberOrPercentage;
use lightningcss::values::position::{
    HorizontalPositionKeyword, Position as CssPositionValue, PositionComponent, VerticalPositionKeyword,
};
use lightningcss::values::time::Time;

use super::cascade::over;
use super::warn_once;
use crate::transition::{Animation, Direction, Easing, Transition};

/// How an element participates in layout; gpui only knows block/flex/grid, so inline-ness is tracked here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Disp {
    None,
    Block,
    Flex,
    Grid,
    #[default]
    Inline,
    InlineBlock,
    InlineFlex,
    InlineGrid,
    Contents,
}

impl Disp {
    pub fn is_inline_level(self) -> bool {
        matches!(
            self,
            Disp::Inline | Disp::InlineBlock | Disp::InlineFlex | Disp::InlineGrid
        )
    }
}

/// Values lengths and colors resolve against.
#[derive(Clone, Copy)]
pub struct Ctx {
    /// Inherited `text-transform`.
    pub case: TextCase,
    /// Inherited `white-space`.
    pub ws: Ws,
    /// Inherited `pointer-events: none`: the pointer passes through to what is behind.
    pub no_pointer: bool,
    pub font_px: f32,
    /// `1ch` as a fraction of the font size: the width of the font's "0".
    pub ch: f32,
    pub parent_font_px: f32,
    pub color: Hsla,
    pub parent_color: Hsla,
    pub vw: f32,
    pub vh: f32,
    pub dark: bool,
    /// The opaque color showing through behind the element.
    pub backdrop: Hsla,
}

/// A custom title bar's parts, as in Electron: pressing a `Drag` box (or what is inside it) moves
/// the window, unless the press lands in a `NoDrag` box inside it (a title bar's buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppRegion {
    Drag,
    NoDrag,
}

/// `text-transform`, applied to text as the renderer lays it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextCase {
    #[default]
    None,
    Upper,
    Lower,
    Capitalize,
}

impl TextCase {
    pub fn apply<'a>(self, text: &'a str) -> std::borrow::Cow<'a, str> {
        match self {
            TextCase::None => text.into(),
            TextCase::Upper => text.to_uppercase().into(),
            TextCase::Lower => text.to_lowercase().into(),
            TextCase::Capitalize => {
                let mut out = String::with_capacity(text.len());
                let mut word_start = true;
                for c in text.chars() {
                    match word_start && c.is_alphanumeric() {
                        true => out.extend(c.to_uppercase()),
                        false => out.push(c),
                    }
                    word_start = c.is_whitespace();
                }
                out.into()
            }
        }
    }
}

/// `white-space`: which of the text's spaces and newlines survive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ws {
    #[default]
    Collapse,
    /// `pre-line`: newlines kept, other runs of whitespace collapsed.
    Lines,
    /// `pre`, `pre-wrap`, `break-spaces`: all kept.
    Preserve,
}

pub const REM: f32 = 16.0;

pub struct Builder<'f> {
    pub s: StyleRefinement,
    pub disp: Disp,
    pub ctx: Ctx,
    /// Border sides whose style is `none`; CSS computes their width to 0.
    border_none: [bool; 4],
    widths: [Option<f32>; 4],
    /// Border colors by side (top, right, bottom, left) in the order the cascade set them.
    border_colors: Vec<(usize, Option<Hsla>)>,
    fonts: &'f dyn Fn(&str) -> Option<f32>,
    /// The solid `background-color`, if any; gpui's `Fill` does not expose it.
    pub bg: Option<Hsla>,
    /// The midpoint color of a gradient background, standing in for it as a backdrop.
    pub gradient_mid: Option<Hsla>,
    /// `width: fit-content` and friends: size to content instead of stretching.
    pub fit_width: bool,
    /// `position: fixed`.
    pub fixed: bool,
    /// `position: sticky`.
    pub sticky: bool,
    /// `field-sizing: content`.
    pub field_grows: bool,
    /// `background-clip: text`: the background paints the text instead of the box.
    clip_text: bool,
    /// `content`, which a `::before`/`::after` box needs to exist: its text.
    pub content: Option<String>,
    pub counters: Option<Counters>,
    transition: TransitionParts,
    /// Outline width, whether its style is `none`, and its color (`None` is currentColor).
    outline: (f32, bool, Option<Hsla>),
    outline_offset: f32,
    /// `text-decoration-color`, wavy `-style`, `-thickness` in px, and `text-underline-offset`.
    decoration: (Option<Hsla>, bool, Option<f32>, Option<f32>),
    /// OpenType features from `font-variant-numeric` and `font-feature-settings`, in that order.
    features: [Option<Vec<(String, u32)>>; 2],
    /// `container-type: inline-size | size`: size containment keeps content from widening the box,
    /// and `@container` rules query it.
    pub contain_inline: bool,
    /// `container-name`.
    pub container_names: Vec<String>,
    /// gpui has no static positioning; a relative box with insets ignored behaves the same.
    position_static: bool,
    pub z_index: Option<i32>,
    /// `app-region` (Electron's `-webkit-app-region`).
    pub app_region: Option<AppRegion>,
    /// CSS `fill`/`stroke`, which win over an SVG element's own attributes.
    pub fill: Option<Paint>,
    pub stroke: Option<Paint>,
    /// Other SVG presentation properties set in CSS, as attributes for the drawing.
    pub presentation: Vec<(&'static str, String)>,
    animation: AnimationParts,
    /// `grid-template-areas`: each named area's lines (row start, row end, column start, column end).
    pub grid_areas: Option<GridAreas>,
    /// The parent's, which `grid-area: name` places this element in.
    pub parent_areas: Option<GridAreas>,
    /// An `<img>`'s `object-fit` and `object-position`.
    pub image: Placement,
    pub list: ListStyle,
    pub table: TableStyle,
    /// `text-shadow` set here (`Some(None)`: `none`), as color and offset: it inherits.
    pub text_shadow: Option<Option<(Hsla, f32, f32)>>,
    /// Sides drawn in different colors (see `border_sides`): the box's own border is then clear.
    split_border: bool,
    /// `background-image: url()`, and its `background-size` and `-position`.
    pub bg_url: Option<String>,
    pub bg_place: Placement,
    /// Each `background-image` layer's gradient (`None` for `url()` and `none`), then the
    /// `background-size`, `-position` and `-repeat` lists they take their values from, cycling.
    bg_layers: Vec<Option<BgLayer>>,
    /// The next `background-image`'s gradients interpolate in Oklab.
    bg_oklab: bool,
    bg_sizes: Vec<[Option<Lp>; 2]>,
    bg_at: [Vec<Lp>; 2],
    bg_repeats: Vec<[bool; 2]>,
    /// Flex and grid items are laid out in `order`, then tree order.
    pub order: i32,
}

pub type GridAreas = std::rc::Rc<Vec<(String, [i16; 4])>>;

/// Where an image sits in its box: how it is fitted, and its position as fractions of the room
/// left around it (`object-fit`/`-position`, or `background-size`/`-position`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub fit: ObjectFit,
    pub at: (f32, f32),
}

/// `counter-reset` (and `-set`) and `counter-increment`: (name, value) pairs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counters {
    pub reset: Vec<(String, i32)>,
    pub increment: Vec<(String, i32)>,
}

/// Where a `content` value's `attr()` or `counter()` goes: between these, `attr <name>` or
/// `counter <name> <style>`, which `render` fills in.
pub const GENERATED: (char, char) = ('\u{E000}', '\u{E001}');

/// A `content` value as text: strings, quotes, and markers for `attr()` and `counter()`.
// shortcut: `counters()` shows the innermost counter only; images and alt text are dropped.
fn content_text(value: &str) -> Option<String> {
    let value = value.trim();
    if matches!(value, "none" | "normal") {
        return None;
    }
    let (mut out, mut rest) = (String::new(), value);
    while let Some(c) = rest.chars().next() {
        if c.is_whitespace() {
            rest = &rest[1..];
        } else if c == '/' {
            break;
        } else if c == '"' || c == '\'' {
            let mut chars = rest[1..].char_indices().peekable();
            let mut end = rest.len();
            while let Some((i, ch)) = chars.next() {
                match ch {
                    _ if ch == c => {
                        end = i + 2;
                        break;
                    }
                    // An escape: up to six hex digits (and a space after them), or the character.
                    '\\' => {
                        let mut hex = String::new();
                        while let Some((_, h)) = chars.peek().filter(|(_, h)| h.is_ascii_hexdigit() && hex.len() < 6) {
                            hex.push(*h);
                            chars.next();
                        }
                        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            Some(ch) => {
                                out.push(ch);
                                chars.next_if(|(_, s)| *s == ' ');
                            }
                            None => out.extend(chars.next().map(|(_, ch)| ch)),
                        }
                    }
                    _ => out.push(ch),
                }
            }
            rest = &rest[end.min(rest.len())..];
        } else {
            let end = rest.find(|ch: char| ch.is_whitespace() || ch == '(').unwrap_or(rest.len());
            let word = &rest[..end];
            if !rest[end..].starts_with('(') {
                match word {
                    "open-quote" => out.push('\u{201C}'),
                    "close-quote" => out.push('\u{201D}'),
                    _ => {}
                }
                rest = &rest[end..];
                continue;
            }
            let close = rest.find(')').map_or(rest.len(), |i| i + 1);
            let args: Vec<&str> = rest[end + 1..close.max(end + 1) - 1]
                .split(',')
                .map(|a| a.trim().trim_matches(['"', '\'']))
                .collect();
            let (open, shut) = GENERATED;
            match word {
                "attr" => out.push_str(&format!("{open}attr {}{shut}", args[0])),
                "counter" => out.push_str(&format!("{open}counter {} {}{shut}", args[0], args.get(1).unwrap_or(&"decimal"))),
                "counters" => out.push_str(&format!("{open}counter {} {}{shut}", args[0], args.get(2).unwrap_or(&"decimal"))),
                _ => {}
            }
            rest = &rest[close..];
        }
    }
    Some(out)
}

/// The list marker a counter style name draws (`counter(n, lower-roman)`).
pub fn counter_style(name: &str) -> Marker {
    match name {
        "none" => Marker::None,
        "disc" => Marker::Disc,
        "circle" => Marker::Circle,
        "square" => Marker::Square,
        "decimal-leading-zero" => Marker::DecimalLeadingZero,
        "lower-roman" => Marker::LowerRoman,
        "upper-roman" => Marker::UpperRoman,
        "lower-alpha" | "lower-latin" => Marker::LowerAlpha,
        "upper-alpha" | "upper-latin" => Marker::UpperAlpha,
        _ => Marker::Decimal,
    }
}

/// A calc() as `min()`s and `max()`s of `fraction * size + px` terms: what `gpui::CalcExpr` runs.
#[derive(Clone, Debug, PartialEq)]
enum CalcTree {
    Linear(f32, f32),
    Min(Vec<CalcTree>),
    Max(Vec<CalcTree>),
}

impl CalcTree {
    /// Sums distribute over min and max: min(a, b) + c is min(a + c, b + c).
    fn add(self, other: CalcTree) -> CalcTree {
        use CalcTree::*;
        match (self, other) {
            (Linear(f, p), Linear(g, q)) => Linear(f + g, p + q),
            (Min(v), o) => Min(v.into_iter().map(|e| e.add(o.clone())).collect()),
            (Max(v), o) => Max(v.into_iter().map(|e| e.add(o.clone())).collect()),
            (l, o) => o.add(l),
        }
    }

    /// A min or max of terms with one fraction is one term (`min(10px, 1rem)`).
    fn simplified(self) -> CalcTree {
        let (v, min) = match &self {
            CalcTree::Min(v) => (v, true),
            CalcTree::Max(v) => (v, false),
            CalcTree::Linear(..) => return self,
        };
        let terms: Option<Vec<(f32, f32)>> =
            v.iter().map(|e| if let CalcTree::Linear(f, p) = e { Some((*f, *p)) } else { None }).collect();
        match terms {
            Some(t) if !t.is_empty() && t.iter().all(|(f, _)| *f == t[0].0) => {
                let px = t.iter().map(|(_, p)| *p);
                CalcTree::Linear(t[0].0, if min { px.fold(f32::INFINITY, f32::min) } else { px.fold(f32::NEG_INFINITY, f32::max) })
            }
            _ => self,
        }
    }

    /// A negative factor turns min into max.
    fn times(self, n: f32) -> CalcTree {
        use CalcTree::*;
        let all = |v: Vec<CalcTree>| v.into_iter().map(|e| e.times(n)).collect();
        match self {
            Linear(f, p) => Linear(f * n, p * n),
            Min(v) if n >= 0.0 => Min(all(v)),
            Max(v) if n >= 0.0 => Max(all(v)),
            Min(v) => Max(all(v)),
            Max(v) => Min(all(v)),
        }
    }

    /// Appends its `taffy::eval_calc` ops; returns how many values they leave pending at most.
    fn flatten(&self, out: &mut Vec<f32>) -> usize {
        let (op, v) = match self {
            CalcTree::Linear(f, p) => {
                out.extend([0.0, *f, *p]);
                return 1;
            }
            CalcTree::Min(v) => (1.0, v),
            CalcTree::Max(v) => (2.0, v),
        };
        let depth = v.iter().enumerate().map(|(i, e)| i + e.flatten(out)).max().unwrap_or(0);
        out.extend([op, v.len() as f32, 0.0]);
        depth.max(1)
    }
}

/// A gradient layer of `background-image` that one gpui `Fill` can't draw: one of several layers,
/// sized, placed or tiled, with px stops, repeating, conic, or a radial one off the center.
/// `crate::background` paints these.
#[derive(Clone, Debug, PartialEq)]
pub struct BgLayer {
    pub shape: BgShape,
    /// Colors (straight alpha) at fractions of the gradient line or px along it (`None`: spread
    /// between their neighbors).
    pub stops: Vec<(Hsla, Option<Lp>)>,
    pub repeating: bool,
    /// `background-size` (`None`: auto, the box), `-position` (of the room left), `-repeat`.
    pub size: [Option<Lp>; 2],
    pub at: [Lp; 2],
    pub repeat: [bool; 2],
    /// What the stops interpolate in (`in oklab`).
    pub space: gpui::ColorSpace,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BgShape {
    /// The angle, clockwise from "to top".
    Linear(f32),
    Radial { circle: bool, center: [Lp; 2], size: RadialSize },
    /// The start angle (clockwise from the top) and the center.
    Conic { from: f32, center: [Lp; 2] },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RadialSize {
    ClosestSide,
    FarthestSide,
    ClosestCorner,
    FarthestCorner,
    /// Radii along x and y (a circle's both the first).
    Radii([Lp; 2]),
}

/// `display: list-item` and the list properties set on the element (`None`: inherited).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListStyle {
    pub item: bool,
    pub marker: Option<Marker>,
    pub inside: Option<bool>,
}

/// Table properties set on the element (`None`: inherited, as `border-collapse` and
/// `border-spacing` are).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TableStyle {
    pub collapse: Option<bool>,
    /// `border-spacing`: horizontal, vertical.
    pub spacing: Option<(f32, f32)>,
    pub valign: Option<VAlign>,
    /// `table-layout: fixed`.
    pub fixed: bool,
    /// What part of a table `display` makes the element, whatever its tag.
    pub role: Option<TableRole>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TableRole {
    Table,
    RowGroup,
    Row,
    Cell,
}

/// `vertical-align`: a table cell's, or inline text's `sub`/`super`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VAlign {
    Baseline,
    Sub,
    Super,
    Top,
    Middle,
    Bottom,
}

/// A `list-style-type`.
#[derive(Clone, Debug, PartialEq)]
pub enum Marker {
    None,
    Text(String),
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerAlpha,
    UpperAlpha,
}

impl Marker {
    /// The marker of the `n`th item, with the space after it, as browsers draw it.
    pub fn label(&self, n: i32) -> Option<String> {
        let roman = |n: i32| {
            let digits = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
            let (mut n, mut out) = (n, String::new());
            for (v, d) in digits {
                while n >= v {
                    out.push_str(d);
                    n -= v;
                }
            }
            out
        };
        let alpha = |n: i32| {
            let (mut n, mut out) = (n, Vec::new());
            while n > 0 {
                out.push((b'a' + ((n - 1) % 26) as u8) as char);
                n = (n - 1) / 26;
            }
            out.iter().rev().collect::<String>()
        };
        // Outside 1..=3999 (roman) or below 1 (alphabetic), CSS falls back to decimal.
        Some(match self {
            Marker::None => return None,
            Marker::Text(t) => t.clone(),
            Marker::Disc => "• ".into(),
            Marker::Circle => "◦ ".into(),
            Marker::Square => "▪ ".into(),
            Marker::Decimal => format!("{n}. "),
            Marker::DecimalLeadingZero => format!("{n:02}. "),
            Marker::LowerRoman if (1..4000).contains(&n) => format!("{}. ", roman(n)),
            Marker::UpperRoman if (1..4000).contains(&n) => format!("{}. ", roman(n).to_uppercase()),
            Marker::LowerAlpha if n > 0 => format!("{}. ", alpha(n)),
            Marker::UpperAlpha if n > 0 => format!("{}. ", alpha(n).to_uppercase()),
            _ => format!("{n}. "),
        })
    }
}

/// An SVG paint set from CSS.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    None,
    Color(Hsla),
    /// A paint server in the drawing, `url(#gradient)`.
    Url(SharedString),
}

/// The `animation-*` longhands; the keyframes are resolved by the cascade.
#[derive(Default)]
struct AnimationParts {
    name: Option<String>,
    duration: Option<f32>,
    delay: Option<f32>,
    easing: Option<Easing>,
    iterations: Option<f32>,
    direction: Option<Direction>,
    fill_mode: Option<CssFillMode>,
    paused: bool,
}

#[derive(Default)]
struct TransitionParts {
    properties: Option<Vec<String>>,
    duration: Option<f32>,
    delay: Option<f32>,
    easing: Option<Easing>,
}

impl<'f> Builder<'f> {
    pub fn new(ctx: Ctx, fonts: &'f dyn Fn(&str) -> Option<f32>) -> Self {
        Builder {
            s: StyleRefinement::default(),
            disp: Disp::Inline,
            ctx,
            border_none: [true; 4],
            widths: [None; 4],
            border_colors: Vec::new(),
            fonts,
            bg: None,
            gradient_mid: None,
            fit_width: false,
            fixed: false,
            sticky: false,
            field_grows: false,
            clip_text: false,
            app_region: None,
            counters: None,
            content: None,
            transition: TransitionParts::default(),
            outline: (3.0, true, None),
            outline_offset: 0.0,
            decoration: (None, false, None, None),
            features: [None, None],
            contain_inline: false,
            container_names: Vec::new(),
            position_static: false,
            z_index: None,
            fill: None,
            stroke: None,
            presentation: Vec::new(),
            animation: AnimationParts::default(),
            grid_areas: None,
            parent_areas: None,
            image: Placement { fit: ObjectFit::Fill, at: (0.5, 0.5) },
            list: ListStyle::default(),
            table: TableStyle::default(),
            text_shadow: None,
            split_border: false,
            bg_url: None,
            // shortcut: `auto` draws the image once at its size; backgrounds don't repeat.
            bg_place: Placement { fit: ObjectFit::None, at: (0.0, 0.0) },
            bg_layers: Vec::new(),
            bg_oklab: false,
            bg_sizes: Vec::new(),
            bg_at: [Vec::new(), Vec::new()],
            bg_repeats: Vec::new(),
            order: 0,
        }
    }

    /// The element's `@keyframes` animation, frames still to be filled in, if it names one.
    pub fn animation(&self) -> Option<Animation> {
        let a = &self.animation;
        let name = a.name.clone().filter(|_| !a.paused)?;
        let fill = a.fill_mode.unwrap_or(CssFillMode::None);
        Some(Animation {
            name,
            frames: Vec::new(),
            duration: a.duration.unwrap_or(0.0),
            delay: a.delay.unwrap_or(0.0),
            easing: a
                .easing
                .unwrap_or(Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)),
            iterations: a.iterations.unwrap_or(1.0),
            direction: a.direction.unwrap_or(Direction::Normal),
            fill_backwards: matches!(fill, CssFillMode::Backwards | CssFillMode::Both),
            fill_forwards: matches!(fill, CssFillMode::Forwards | CssFillMode::Both),
        })
    }

    /// The element's transition, if it animates anything.
    pub fn transition(&self) -> Option<Transition> {
        let t = &self.transition;
        let duration = t.duration.unwrap_or(0.0);
        (duration > 0.0).then(|| Transition {
            properties: t.properties.clone().unwrap_or_else(|| vec!["all".into()]),
            duration,
            delay: t.delay.unwrap_or(0.0),
            easing: t
                .easing
                .unwrap_or(Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)),
        })
    }

    pub fn finish(mut self) -> (StyleRefinement, Disp) {
        // shortcut: text clipped to a gradient is drawn in its midpoint color (a solid one exact).
        if self.clip_text
            && let Some(c) = self.gradient_mid.or(self.bg)
        {
            self.text().color = Some(c);
            self.s.background = None;
        }
        if self.position_static {
            // Static isn't positioned: no insets, and z-index doesn't raise it (taffy's default is relative).
            self.s.inset = Default::default();
            self.s.position = None;
        }
        // As in CSS, `visible` on one axis computes to `auto` when the other axis isn't visible.
        let o = &mut self.s.overflow;
        let clips = |v: Option<Overflow>| v.is_some_and(|v| v != Overflow::Visible);
        if clips(o.y) && !clips(o.x) {
            o.x = Some(Overflow::Scroll);
        } else if clips(o.x) && !clips(o.y) {
            o.y = Some(Overflow::Scroll);
        }
        // Inline-size containment: content can't widen the box or, through it, its ancestors.
        if self.contain_inline {
            self.s.contain_inline_size = Some(true);
            self.s.min_size.width.get_or_insert(px(0.0).into());
        }
        // `text-decoration-*` refine the lines this element draws, whatever order they came in.
        let (color, wavy, thickness, offset) = self.decoration;
        if let Some(t) = &mut self.s.text {
            if let Some(u) = &mut t.underline {
                u.color = color.or(u.color);
                u.wavy = wavy;
                u.thickness = thickness.map_or(u.thickness, px);
                u.offset = offset.map(px).or(u.offset);
            }
            if let Some(s) = &mut t.strikethrough {
                s.color = color.or(s.color);
                s.thickness = thickness.map_or(s.thickness, px);
            }
        }
        if let [numeric, settings] = &self.features
            && (numeric.is_some() || settings.is_some())
        {
            let mut all = numeric.clone().unwrap_or_default();
            for (tag, v) in settings.iter().flatten() {
                all.retain(|(t, _)| t != tag);
                all.push((tag.clone(), *v));
            }
            self.text().font_features = Some(FontFeatures(std::sync::Arc::new(all)));
        }
        // gpui has no outlines; one looks like a solid ring shadow around the border box. An
        // offset one leaves a gap showing what is behind the box, painted over the ring.
        // shortcut: the gap is the backdrop color, as if what is behind were one color; a negative
        // offset (an outline inside the box) is drawn at 0.
        let (width, none, color) = self.outline;
        if !none && width > 0.0 {
            let offset = self.outline_offset.max(0.0);
            let ring = |color, spread| BoxShadow {
                color,
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(spread),
            };
            let shadows = self.s.box_shadow.get_or_insert_with(Vec::new);
            shadows.push(ring(color.unwrap_or(self.ctx.color), offset + width));
            if offset > 0.0 {
                shadows.push(ring(self.ctx.backdrop, offset));
            }
        }
        // gpui has one border color per element: the last one set on a side that draws, so a hidden
        // side (`border-left: 0` resets its color to currentColor) leaves the others' color alone.
        let draws = |i: usize| !self.border_none[i] && self.widths[i].is_some_and(|w| w > 0.0);
        let colors = self.border_colors.iter().rev();
        if let Some(&(_, color)) = colors
            .clone()
            .find(|(i, _)| draws(*i))
            .or(colors.clone().next())
        {
            self.s.border_color = color;
        }
        if self.split_border {
            self.s.border_color = Some(gpui::transparent_black());
        }
        for (i, w) in self.widths.iter().enumerate() {
            let w = if self.border_none[i] {
                w.map(|_| 0.0)
            } else {
                *w
            };
            if let Some(w) = w {
                let w = Some(AbsoluteLength::Pixels(px(w)));
                let b = &mut self.s.border_widths;
                match i {
                    0 => b.top = w,
                    1 => b.right = w,
                    2 => b.bottom = w,
                    _ => b.left = w,
                }
            }
        }
        (self.s, self.disp)
    }

    fn text(&mut self) -> &mut TextStyleRefinement {
        self.s.text.get_or_insert_with(Default::default)
    }

    /// `font-size` and `color` must be known before anything resolves `em` or `currentColor`.
    pub fn apply_early(&mut self, p: &Property) -> bool {
        match p {
            Property::FontSize(size) => {
                if let Some(v) = self.font_size(size) {
                    self.ctx.font_px = v;
                    self.text().font_size = Some(AbsoluteLength::Pixels(px(v)));
                }
            }
            // Early too: `ch` lengths are the font's.
            Property::FontFamily(families) => {
                if let Some((f, ch)) = self.font_family(families) {
                    self.text().font_family = Some(f);
                    self.ctx.ch = ch;
                }
            }
            Property::Color(c) => {
                if let Some(c) = self.color_with(c, self.ctx.parent_color) {
                    self.ctx.color = c;
                    self.text().color = Some(c);
                }
            }
            _ => return false,
        }
        true
    }

    pub fn apply(&mut self, p: &Property) {
        use Property as P;
        match p {
            P::Display(d) => self.display(d),
            P::Visibility(v) => {
                self.s.visibility = Some(match v {
                    CssVisibility::Visible => Visibility::Visible,
                    _ => Visibility::Hidden,
                })
            }
            P::Position(pos) => {
                self.s.position = Some(match pos {
                    CssPosition::Absolute => Position::Absolute,
                    // Absolute within the window; the renderer lifts the box out of the scrolling page.
                    CssPosition::Fixed => Position::Absolute,
                    _ => Position::Relative,
                });
                self.position_static = matches!(pos, CssPosition::Static);
                self.fixed = matches!(pos, CssPosition::Fixed);
                self.sticky = matches!(pos, CssPosition::Sticky(_));
            }
            P::Top(v) | P::InsetBlockStart(v) => self.s.inset.top = self.lpa(v),
            P::Right(v) | P::InsetInlineEnd(v) => self.s.inset.right = self.lpa(v),
            P::Bottom(v) | P::InsetBlockEnd(v) => self.s.inset.bottom = self.lpa(v),
            P::Left(v) | P::InsetInlineStart(v) => self.s.inset.left = self.lpa(v),

            P::Width(v) | P::InlineSize(v) => {
                self.fit_width = matches!(
                    v,
                    Size::FitContent(_) | Size::FitContentFunction(_) | Size::MinContent(_) | Size::MaxContent(_)
                );
                self.s.size.width = self.size(v);
            }
            P::Height(v) | P::BlockSize(v) => self.s.size.height = self.size(v),
            P::MinWidth(v) | P::MinInlineSize(v) => self.s.min_size.width = self.size(v),
            P::MinHeight(v) | P::MinBlockSize(v) => self.s.min_size.height = self.size(v),
            P::MaxWidth(v) | P::MaxInlineSize(v) => self.s.max_size.width = self.max_size(v),
            P::MaxHeight(v) | P::MaxBlockSize(v) => self.s.max_size.height = self.max_size(v),
            P::AspectRatio(a) => {
                self.s.aspect_ratio = a.ratio.as_ref().map(|r| r.0 / r.1);
            }

            P::MarginTop(v) | P::MarginBlockStart(v) => self.s.margin.top = self.lpa(v),
            P::MarginRight(v) | P::MarginInlineEnd(v) => self.s.margin.right = self.lpa(v),
            P::MarginBottom(v) | P::MarginBlockEnd(v) => self.s.margin.bottom = self.lpa(v),
            P::MarginLeft(v) | P::MarginInlineStart(v) => self.s.margin.left = self.lpa(v),
            P::PaddingTop(v) | P::PaddingBlockStart(v) => self.s.padding.top = self.lpa_definite(v),
            P::PaddingRight(v) | P::PaddingInlineEnd(v) => self.s.padding.right = self.lpa_definite(v),
            P::PaddingBottom(v) | P::PaddingBlockEnd(v) => self.s.padding.bottom = self.lpa_definite(v),
            P::PaddingLeft(v) | P::PaddingInlineStart(v) => self.s.padding.left = self.lpa_definite(v),

            P::FlexDirection(d, _) => {
                self.s.flex_direction = Some(match d {
                    CssFlexDirection::Row => FlexDirection::Row,
                    CssFlexDirection::RowReverse => FlexDirection::RowReverse,
                    CssFlexDirection::Column => FlexDirection::Column,
                    CssFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
                })
            }
            P::FlexWrap(w, _) => {
                self.s.flex_wrap = Some(match w {
                    CssFlexWrap::NoWrap => FlexWrap::NoWrap,
                    CssFlexWrap::Wrap => FlexWrap::Wrap,
                    CssFlexWrap::WrapReverse => FlexWrap::WrapReverse,
                })
            }
            P::FlexGrow(n, _) => self.s.flex_grow = Some(*n),
            P::FlexShrink(n, _) => self.s.flex_shrink = Some(*n),
            P::FlexBasis(v, _) => self.s.flex_basis = self.lpa(v),
            P::AlignItems(a, _) => self.s.align_items = align_items(a),
            P::AlignSelf(a, _) => self.s.align_self = align_self(a),
            P::AlignContent(a, _) => self.s.align_content = align_content(a),
            P::JustifyContent(j, _) => self.s.justify_content = justify_content(j),
            P::RowGap(g) => self.s.gap.height = self.gap(g),
            P::ColumnGap(g) => self.s.gap.width = self.gap(g),

            P::GridTemplateColumns(t) => self.s.grid_template_columns = self.grid_template(t),
            P::GridTemplateRows(t) => self.s.grid_template_rows = self.grid_template(t),
            P::GridAutoColumns(l) => self.s.grid_auto_columns = Some(l.0.iter().map(|t| self.track(t)).collect()),
            P::GridAutoRows(l) => self.s.grid_auto_rows = Some(l.0.iter().map(|t| self.track(t)).collect()),
            P::JustifySelf(j) => self.s.justify_self = justify_self(j),
            P::JustifyItems(j) => self.s.justify_items = justify_items(j),
            P::GridColumnStart(l) => self.grid_location().column.start = self.grid_line(l, 2),
            P::GridColumnEnd(l) => self.grid_location().column.end = self.grid_line(l, 3),
            P::GridRowStart(l) => self.grid_location().row.start = self.grid_line(l, 0),
            P::GridRowEnd(l) => self.grid_location().row.end = self.grid_line(l, 1),
            P::GridTemplateAreas(a) => self.grid_areas = grid_areas(a),
            P::GridAutoFlow(f) => {
                let dense = f.contains(CssGridAutoFlow::Dense);
                self.s.grid_auto_flow = Some(match f.contains(CssGridAutoFlow::Column) {
                    false if dense => GridAutoFlow::RowDense,
                    false => GridAutoFlow::Row,
                    true if dense => GridAutoFlow::ColumnDense,
                    true => GridAutoFlow::Column,
                })
            }

            P::BackgroundColor(c) => {
                if let Some(c) = self.color(c) {
                    self.s.background = Some(Fill::from(c));
                    self.bg = Some(c);
                    self.gradient_mid = None;
                }
            }
            P::BackgroundImage(images) => self.background_image(images),
            P::Opacity(a) => self.s.opacity = Some(a.0),
            P::BoxShadow(shadows, _) => self.box_shadow(shadows),
            P::BackgroundSize(sizes) => {
                let auto_or = |l: &LengthPercentageOrAuto| match l {
                    LengthPercentageOrAuto::Auto => None,
                    LengthPercentageOrAuto::LengthPercentage(l) => self.lp(l),
                };
                self.bg_sizes = sizes
                    .iter()
                    .map(|s| match s {
                        BackgroundSize::Explicit { width, height } => [auto_or(width), auto_or(height)],
                        // A gradient has no proportions to keep: both fill the box.
                        BackgroundSize::Cover | BackgroundSize::Contain => [None, None],
                    })
                    .collect();
                let full = |l: &LengthPercentageOrAuto| {
                    matches!(l, LengthPercentageOrAuto::LengthPercentage(l) if self.lp(l) == Some(Lp::Fraction(1.0)))
                };
                self.bg_place.fit = match sizes.first() {
                    Some(BackgroundSize::Cover) => ObjectFit::Cover,
                    Some(BackgroundSize::Contain) => ObjectFit::Contain,
                    Some(BackgroundSize::Explicit { width, height }) if full(width) && full(height) => ObjectFit::Fill,
                    Some(BackgroundSize::Explicit {
                        width: LengthPercentageOrAuto::Auto,
                        height: LengthPercentageOrAuto::Auto,
                    })
                    | None => ObjectFit::None,
                    Some(BackgroundSize::Explicit { .. }) => {
                        warn_once("background-size other than cover, contain, auto and 100% 100% (using contain)");
                        ObjectFit::Contain
                    }
                }
            }
            P::BackgroundPositionX(xs) => {
                self.bg_at[0] = xs
                    .iter()
                    .map(|x| origin_axis(self, x, |k| *k == HorizontalPositionKeyword::Right).unwrap_or(Lp::Px(0.0)))
                    .collect();
                let x = xs.first().map(|x| origin_axis(self, x, |k| *k == HorizontalPositionKeyword::Right));
                self.bg_place.at.0 = self.fraction(x.flatten());
            }
            P::BackgroundPositionY(ys) => {
                self.bg_at[1] = ys
                    .iter()
                    .map(|y| origin_axis(self, y, |k| *k == VerticalPositionKeyword::Bottom).unwrap_or(Lp::Px(0.0)))
                    .collect();
                let y = ys.first().map(|y| origin_axis(self, y, |k| *k == VerticalPositionKeyword::Bottom));
                self.bg_place.at.1 = self.fraction(y.flatten());
            }
            P::Custom(c) if c.name.as_ref() == "object-fit" => {
                self.image.fit = match super::value_text(p).trim() {
                    "contain" => ObjectFit::Contain,
                    "cover" => ObjectFit::Cover,
                    "none" => ObjectFit::None,
                    "scale-down" => ObjectFit::ScaleDown,
                    _ => ObjectFit::Fill,
                }
            }
            P::Custom(c) if c.name.as_ref() == "object-position" => {
                if let Ok(pos) = CssPositionValue::parse_string(super::value_text(p).trim()) {
                    let x = origin_axis(self, &pos.x, |k| *k == HorizontalPositionKeyword::Right);
                    let y = origin_axis(self, &pos.y, |k| *k == VerticalPositionKeyword::Bottom);
                    self.image.at = (self.fraction(x), self.fraction(y));
                }
            }
            P::Order(n, _) => self.order = *n,

            P::BorderTopWidth(w) | P::BorderBlockStartWidth(w) => self.widths[0] = self.border_width(w),
            P::BorderRightWidth(w) | P::BorderInlineEndWidth(w) => self.widths[1] = self.border_width(w),
            P::BorderBottomWidth(w) | P::BorderBlockEndWidth(w) => self.widths[2] = self.border_width(w),
            P::BorderLeftWidth(w) | P::BorderInlineStartWidth(w) => self.widths[3] = self.border_width(w),
            P::BorderTopStyle(s) | P::BorderBlockStartStyle(s) => self.border_style(0, s),
            P::BorderRightStyle(s) | P::BorderInlineEndStyle(s) => self.border_style(1, s),
            P::BorderBottomStyle(s) | P::BorderBlockEndStyle(s) => self.border_style(2, s),
            P::BorderLeftStyle(s) | P::BorderInlineStartStyle(s) => self.border_style(3, s),
            P::BorderTopColor(c) | P::BorderBlockStartColor(c) => self.border_colors.push((0, self.color(c))),
            P::BorderRightColor(c) | P::BorderInlineEndColor(c) => self.border_colors.push((1, self.color(c))),
            P::BorderBottomColor(c) | P::BorderBlockEndColor(c) => self.border_colors.push((2, self.color(c))),
            P::BorderLeftColor(c) | P::BorderInlineStartColor(c) => self.border_colors.push((3, self.color(c))),
            P::BorderTopLeftRadius(r, _) | P::BorderStartStartRadius(r) => self.s.corner_radii.top_left = self.radius(&r.0),
            P::BorderTopRightRadius(r, _) | P::BorderStartEndRadius(r) => self.s.corner_radii.top_right = self.radius(&r.0),
            P::BorderBottomRightRadius(r, _) | P::BorderEndEndRadius(r) => self.s.corner_radii.bottom_right = self.radius(&r.0),
            P::BorderBottomLeftRadius(r, _) | P::BorderEndStartRadius(r) => self.s.corner_radii.bottom_left = self.radius(&r.0),

            P::OverflowX(o) => self.s.overflow.x = Some(overflow(o)),
            P::OverflowY(o) => self.s.overflow.y = Some(overflow(o)),
            P::Cursor(c) => self.s.mouse_cursor = cursor(c.keyword),

            P::FontWeight(w) => {
                let w = match w {
                    CssFontWeight::Absolute(AbsoluteFontWeight::Weight(n)) => *n,
                    CssFontWeight::Absolute(AbsoluteFontWeight::Normal) => 400.0,
                    CssFontWeight::Absolute(AbsoluteFontWeight::Bold) => 700.0,
                    CssFontWeight::Bolder => 700.0,
                    CssFontWeight::Lighter => 300.0,
                };
                self.text().font_weight = Some(FontWeight(w));
            }
            P::FontStyle(s) => {
                self.text().font_style = Some(match s {
                    CssFontStyle::Normal => FontStyle::Normal,
                    CssFontStyle::Italic => FontStyle::Italic,
                    CssFontStyle::Oblique(_) => FontStyle::Oblique,
                })
            }
            P::LineHeight(l) => {
                let lh = match l {
                    LineHeight::Normal => DefiniteLength::Fraction(1.2),
                    LineHeight::Number(n) => DefiniteLength::Fraction(*n),
                    LineHeight::Length(DimensionPercentage::Percentage(p)) => DefiniteLength::Fraction(p.0),
                    LineHeight::Length(l) => match self.lp(l) {
                        Some(Lp::Px(v)) => DefiniteLength::Absolute(AbsoluteLength::Pixels(px(v))),
                        _ => return,
                    },
                };
                self.text().line_height = Some(lh);
            }
            P::TextAlign(a) => {
                self.text().text_align = Some(match a {
                    CssTextAlign::Center => TextAlign::Center,
                    CssTextAlign::Right | CssTextAlign::End => TextAlign::Right,
                    _ => TextAlign::Left,
                })
            }
            P::TextDecorationLine(line, _) => {
                let color = None;
                let thickness = px(1.0);
                let t = self.text();
                t.underline = line.contains(TextDecorationLine::Underline).then_some(UnderlineStyle { thickness, color, wavy: false, offset: None });
                t.strikethrough = line.contains(TextDecorationLine::LineThrough).then_some(StrikethroughStyle { thickness, color });
            }
            P::WhiteSpace(w) => {
                self.ctx.ws = match w {
                    CssWhiteSpace::Normal | CssWhiteSpace::NoWrap => Ws::Collapse,
                    CssWhiteSpace::PreLine => Ws::Lines,
                    _ => Ws::Preserve,
                };
                self.text().white_space = Some(match w {
                    CssWhiteSpace::NoWrap | CssWhiteSpace::Pre => WhiteSpace::Nowrap,
                    _ => WhiteSpace::Normal,
                })
            }
            P::TextDecorationColor(c, _) => self.decoration.0 = self.color(c),
            // shortcut: dotted, dashed and double lines are drawn solid.
            P::TextDecorationStyle(s, _) => self.decoration.1 = *s == TextDecorationStyle::Wavy,
            P::TextDecorationThickness(t) => {
                self.decoration.2 = match t {
                    TextDecorationThickness::LengthPercentage(l) => match self.lp(l) {
                        Some(Lp::Px(v)) => Some(v),
                        Some(Lp::Fraction(f)) => Some(f * self.ctx.font_px),
                        None => None,
                    },
                    _ => None,
                }
            }
            P::Custom(c) if c.name.as_ref() == "font-variant-numeric" => {
                self.features[0] = Some(numeric_features(&super::value_text(p)))
            }
            P::Custom(c) if c.name.as_ref() == "font-feature-settings" => {
                self.features[1] = Some(feature_settings(&super::value_text(p)))
            }
            P::TextOverflow(o, _) => {
                self.text().text_overflow = match o {
                    CssTextOverflow::Ellipsis => Some(TextOverflow::Truncate(SharedString::new_static("…"))),
                    CssTextOverflow::Clip => None,
                }
            }

            P::AnimationName(names, _) => {
                self.animation.name = names.first().and_then(|n| match n {
                    AnimationName::None => None,
                    AnimationName::Ident(i) => Some(i.0.to_string()),
                    AnimationName::String(s) => Some(s.to_string()),
                })
            }
            P::AnimationDuration(times, _) => self.animation.duration = times.first().map(seconds),
            P::AnimationDelay(times, _) => self.animation.delay = times.first().map(seconds),
            P::AnimationTimingFunction(fns, _) => self.animation.easing = fns.first().map(easing),
            P::AnimationIterationCount(counts, _) => {
                self.animation.iterations = counts.first().map(|c| match c {
                    AnimationIterationCount::Number(n) => n.max(0.0),
                    AnimationIterationCount::Infinite => f32::INFINITY,
                })
            }
            P::AnimationDirection(dirs, _) => {
                self.animation.direction = dirs.first().map(|d| match d {
                    AnimationDirection::Normal => Direction::Normal,
                    AnimationDirection::Reverse => Direction::Reverse,
                    AnimationDirection::Alternate => Direction::Alternate,
                    AnimationDirection::AlternateReverse => Direction::AlternateReverse,
                })
            }
            P::AnimationFillMode(modes, _) => self.animation.fill_mode = modes.first().copied(),
            P::AnimationPlayState(states, _) => {
                self.animation.paused = states.first() == Some(&AnimationPlayState::Paused)
            }
            // Scroll/view timelines and composition have no meaning without them; time it is.
            P::AnimationTimeline(..) | P::AnimationComposition(..) | P::AnimationRangeStart(..)
            | P::AnimationRangeEnd(..) => {}
            P::TextTransform(t) => {
                self.ctx.case = match t.case {
                    TextTransformCase::None => TextCase::None,
                    TextTransformCase::Uppercase => TextCase::Upper,
                    TextTransformCase::Lowercase => TextCase::Lower,
                    TextTransformCase::Capitalize => TextCase::Capitalize,
                }
            }
            P::Filter(list, _) => {
                let filters = match list {
                    FilterList::Filters(fs) => fs.as_slice(),
                    FilterList::None => &[],
                };
                let mut out = GpuiFilter::IDENTITY;
                for f in filters {
                    match f {
                        Filter::Blur(l) => out.blur += px(self.length(l).unwrap_or(0.0).max(0.0)),
                        Filter::DropShadow(_) | Filter::Url(_) => warn_once("filter: drop-shadow() or url()"),
                        f => out.matrix = compose(&color_matrix(f), &out.matrix),
                    }
                }
                // Kept even when it changes nothing: a state's `filter: none` must replace the
                // base's filter (`grayscale hover:grayscale-0`); gpui skips identity filters.
                self.s.filter = Some(out);
            }
            // `blur()` blurs what is behind the box (on macOS); other functions are skipped.
            P::BackdropFilter(list, _) => {
                let filters = match list {
                    FilterList::Filters(fs) => fs.as_slice(),
                    FilterList::None => &[],
                };
                let mut blur = None;
                for f in filters {
                    match f {
                        Filter::Blur(l) => blur = self.length(l).filter(|&r| r > 0.0).map(|r| px(r).into()),
                        _ => warn_once("backdrop-filter other than blur()"),
                    }
                }
                self.s.backdrop_blur = blur;
            }
            // Rotation and scaling (SVG only) turn about the center, which is what Radix's
            // transform-origins describe closely enough for an animation's few frames.
            P::TransformOrigin(pos, _) => {
                let x = origin_axis(self, &pos.x, |k| *k == HorizontalPositionKeyword::Right);
                let y = origin_axis(self, &pos.y, |k| *k == VerticalPositionKeyword::Bottom);
                let center = Some(Lp::Fraction(0.5));
                self.s.transform_origin = match (x, y) {
                    (Some(x), Some(y)) if (Some(x), Some(y)) != (center, center) => Some(point(x.into(), y.into())),
                    _ => None,
                };
            }
            // `inset(50%)` or more clips the whole box away (Tailwind's sr-only): as if hidden.
            P::ClipPath(..) => {
                let text = super::value_text(p);
                let fully_inset = text
                    .strip_prefix("inset(")
                    .and_then(|r| r.trim_end_matches(')').trim().strip_suffix('%'))
                    .and_then(|n| n.parse::<f32>().ok())
                    .is_some_and(|n| n >= 50.0);
                match fully_inset {
                    true => self.s.visibility = Some(Visibility::Hidden),
                    false if text != "none" => warn_once("clip-path"),
                    false => {}
                }
            }
            P::OverflowWrap(w) | P::WordWrap(w) => {
                self.text().word_break = Some(match w {
                    OverflowWrap::Normal => WordBreak::Normal,
                    OverflowWrap::BreakWord => WordBreak::BreakWord,
                    OverflowWrap::Anywhere => WordBreak::Anywhere,
                })
            }
            // shortcut: `normal` and `keep-all` leave `overflow-wrap`'s choice alone; CJK text
            // breaks as gpui breaks it either way.
            P::WordBreak(w) => {
                if matches!(w, CssWordBreak::BreakAll | CssWordBreak::BreakWord) {
                    self.text().word_break = Some(WordBreak::Anywhere);
                }
            }
            P::LetterSpacing(Spacing::Normal) => self.text().letter_spacing = Some(px(0.0)),
            P::LetterSpacing(Spacing::Length(l)) => {
                if let Some(v) = self.length(l) {
                    self.text().letter_spacing = Some(px(v));
                }
            }
            // Only meaningful with `display: -webkit-box`, which is the line-clamp idiom.
            P::BoxOrient(..) => {}
            // shortcut: one string (or none); counters, attr() and images generate nothing.
            P::Custom(c) if c.name.as_ref() == "content" => self.content = content_text(&super::value_text(p)),
            // shortcut: `counter-set` starts a counter over, as `counter-reset` does.
            P::Custom(c) if matches!(c.name.as_ref(), "counter-reset" | "counter-set" | "counter-increment") => {
                let reset = c.name.as_ref() != "counter-increment";
                let mut list = Vec::new();
                for word in super::value_text(p).split_whitespace() {
                    match (word.parse::<i32>(), list.last_mut()) {
                        (Ok(n), Some((_, v))) => *v = n,
                        (Ok(_), None) => {}
                        (Err(_), _) if word == "none" => {}
                        (Err(_), _) => list.push((word.to_owned(), if reset { 0 } else { 1 })),
                    }
                }
                let counters = self.counters.get_or_insert_with(Default::default);
                *(if reset { &mut counters.reset } else { &mut counters.increment }) = list;
            }
            // shortcut: `none` or not; the SVG-only values (`visiblePainted`, `stroke`...) count as `auto`.
            // SVG text placement, unknown to lightningcss; drawn as attributes (see `presentation`).
            P::Custom(c) if matches!(c.name.as_ref(), "text-anchor" | "dominant-baseline") => {
                let name = match c.name.as_ref() {
                    "text-anchor" => "text-anchor",
                    _ => "dominant-baseline",
                };
                self.presentation.retain(|(k, _)| *k != name);
                self.presentation.push((name, super::value_text(p)));
            }
            P::Custom(c) if matches!(c.name.as_ref(), "app-region" | "-webkit-app-region") => {
                self.app_region = match super::value_text(p).trim() {
                    "drag" => Some(AppRegion::Drag),
                    "no-drag" => Some(AppRegion::NoDrag),
                    _ => None,
                };
            }
            P::Custom(c) if c.name.as_ref() == super::cascade::OKLAB_GRADIENT => self.bg_oklab = true,
            // A control without its native look keeps none of the UA sheet's drawing of it (the
            // checkbox tick); author images set after this still apply.
            // shortcut: an author image from a rule cascading before `appearance: none` goes too.
            P::Appearance(a, _) => {
                if matches!(a, lightningcss::properties::ui::Appearance::None) {
                    self.bg_url = None;
                    self.bg_layers.clear();
                }
            }
            // shortcut: applies to the underline set on the same element, not one it inherits.
            P::Custom(c) if c.name.as_ref() == "text-underline-offset" => {
                let text = super::value_text(p);
                self.decoration.3 = CssLength::parse_string(text.trim()).ok().and_then(|l| self.length(&l));
            }
            // `text-wrap` is `-mode` (Tailwind's `text-nowrap`) and `-style` together.
            // shortcut: `pretty` wraps as `wrap` does.
            P::Custom(c) if matches!(c.name.as_ref(), "text-wrap" | "text-wrap-mode" | "text-wrap-style") => {
                for word in super::value_text(p).split_whitespace() {
                    match word {
                        "nowrap" => self.text().white_space = Some(WhiteSpace::Nowrap),
                        "wrap" => self.text().white_space = Some(WhiteSpace::Normal),
                        "balance" => self.text().balance = Some(true),
                        "auto" | "pretty" | "stable" => self.text().balance = Some(false),
                        _ => {}
                    }
                }
            }
            P::Custom(c) if c.name.as_ref() == "field-sizing" => {
                self.field_grows = super::value_text(p).trim() == "content";
            }
            P::Custom(c) if c.name.as_ref() == "pointer-events" => {
                self.ctx.no_pointer = super::value_text(p).trim() == "none";
            }
            P::Custom(c) if matches!(c.name.as_ref(), "-webkit-line-clamp" | "line-clamp") => {
                let n = super::value_text(p).trim().parse().ok();
                self.text().line_clamp = n;
                // A clamped block ends in an ellipsis, as browsers draw it.
                if n.is_some() {
                    self.text().text_overflow.get_or_insert(TextOverflow::Truncate("…".into()));
                }
            }
            P::ZIndex(z) => {
                self.z_index = match z {
                    ZIndex::Integer(i) => Some(*i),
                    ZIndex::Auto => None,
                }
            }
            P::Fill(paint) => self.fill = self.paint(paint),
            // Drawn as the attributes they also are, which CSS overrides (MUI X Charts' dashed grid
            // lines and translucent hover band).
            P::FillOpacity(..)
            | P::FillRule(..)
            | P::StrokeOpacity(..)
            | P::StrokeWidth(..)
            | P::StrokeLinecap(..)
            | P::StrokeLinejoin(..)
            | P::StrokeDasharray(..)
            | P::StrokeDashoffset(..)
            | P::ShapeRendering(..) => {
                let name = match p {
                    P::FillOpacity(..) => "fill-opacity",
                    P::FillRule(..) => "fill-rule",
                    P::StrokeOpacity(..) => "stroke-opacity",
                    P::StrokeWidth(..) => "stroke-width",
                    P::StrokeLinecap(..) => "stroke-linecap",
                    P::StrokeLinejoin(..) => "stroke-linejoin",
                    P::StrokeDasharray(..) => "stroke-dasharray",
                    P::ShapeRendering(..) => "shape-rendering",
                    _ => "stroke-dashoffset",
                };
                self.presentation.retain(|(k, _)| *k != name);
                self.presentation.push((name, super::value_text(p)));
            }
            P::Stroke(paint) => self.stroke = self.paint(paint),
            P::Rotate(r) => {
                self.s.rotate = match r {
                    Rotate::None => None,
                    // Only rotation in the screen plane; a 3D axis has no 2D meaning.
                    Rotate::XYZ { x, y, angle, .. } if *x == 0.0 && *y == 0.0 => Some(angle.to_degrees()),
                    Rotate::XYZ { .. } => {
                        warn_once("rotate about an x or y axis");
                        None
                    }
                }
            }
            P::Scale(sc) => {
                self.s.scale = match sc {
                    Scale::None => None,
                    Scale::XYZ { x, y, .. } => Some(point(number(x), number(y))),
                }
            }
            P::TransitionProperty(ids, _) => {
                self.transition.properties = Some(ids.iter().map(|id| id.name().to_owned()).collect())
            }
            P::TransitionDuration(times, _) => self.transition.duration = times.first().map(seconds),
            P::TransitionDelay(times, _) => self.transition.delay = times.first().map(seconds),
            P::TransitionTimingFunction(fns, _) => self.transition.easing = fns.first().map(easing),
            P::OutlineWidth(w) => self.outline.0 = self.border_width(w).unwrap_or(0.0),
            P::OutlineStyle(s) => {
                self.outline.1 = matches!(s, OutlineStyle::LineStyle(LineStyle::None | LineStyle::Hidden))
            }
            P::OutlineColor(c) => self.outline.2 = self.color(c),
            P::Custom(c) if c.name.as_ref() == "outline-offset" => {
                let text = super::value_text(p);
                self.outline_offset = CssLength::parse_string(text.trim()).ok().and_then(|l| self.length(&l)).unwrap_or(0.0);
            }

            // Effectively always true here, or purely cosmetic hints with no gpui equivalent.
            P::Translate(t) => {
                (self.s.translate, self.s.translate_px) = match t {
                    Translate::None => (None, None),
                    Translate::XYZ { x, y, .. } => self.translation(x, y),
                }
            }
            // Only the translate parts of a transform list; other functions are reported.
            P::Transform(list, _) => {
                // The list composes as translate(`sum`), rotate, scale, translate(`after`), about the
                // center. A translate following a rotation moves along the rotated axes: in px it
                // folds into `sum`; in % of the box it needs the box's size, so it stays in `after`.
                // shortcut: rotations and scales are each summed into one, as if every rotation came
                // before every scale and before any `after` translate.
                let mut sum = (Lp::Px(0.0), Lp::Px(0.0));
                let mut after = (Lp::Px(0.0), Lp::Px(0.0));
                let (mut rotate, mut scale) = (None::<f32>, None::<Point<f32>>);
                let mut skew = Point::<f32>::default();
                for f in &list.0 {
                    let (x, y) = match f {
                        Transform::Translate(x, y) | Transform::Translate3d(x, y, _) => (self.lp(x), self.lp(y)),
                        Transform::TranslateX(x) => (self.lp(x), Some(Lp::Px(0.0))),
                        Transform::TranslateY(y) => (Some(Lp::Px(0.0)), self.lp(y)),
                        Transform::TranslateZ(_) | Transform::Perspective(_) => continue,
                        Transform::Rotate(a) | Transform::RotateZ(a) => {
                            *rotate.get_or_insert(0.0) += a.to_degrees();
                            continue;
                        }
                        Transform::Scale(x, y) | Transform::Scale3d(x, y, _) => {
                            scale_by(&mut scale, number(x), number(y));
                            continue;
                        }
                        Transform::ScaleX(x) => {
                            scale_by(&mut scale, number(x), 1.0);
                            continue;
                        }
                        Transform::ScaleY(y) => {
                            scale_by(&mut scale, 1.0, number(y));
                            continue;
                        }
                        Transform::ScaleZ(_) => continue,
                        // shortcut: skews are summed like rotations, and a translate after one ignores it.
                        Transform::Skew(x, y) => {
                            skew = point(skew.x + x.to_degrees(), skew.y + y.to_degrees());
                            continue;
                        }
                        Transform::SkewX(x) => {
                            skew.x += x.to_degrees();
                            continue;
                        }
                        Transform::SkewY(y) => {
                            skew.y += y.to_degrees();
                            continue;
                        }
                        _ => {
                            warn_once("transform functions other than translate, rotate, skew and scale");
                            continue;
                        }
                    };
                    let (Some(x), Some(y)) = (x, y) else { continue };
                    let s = scale.unwrap_or(point(1.0, 1.0));
                    // Scaling keeps each axis's unit, so a scaled translate is still px or %.
                    let (x, y) = (x.times(s.x), y.times(s.y));
                    match (rotate.filter(|r| *r != 0.0), x, y) {
                        (None, x, y) => sum = (add(sum.0, x), add(sum.1, y)),
                        (Some(deg), Lp::Px(x), Lp::Px(y)) => {
                            let (sin, cos) = deg.to_radians().sin_cos();
                            sum = (
                                add(sum.0, Lp::Px(x * cos - y * sin)),
                                add(sum.1, Lp::Px(x * sin + y * cos)),
                            );
                        }
                        (Some(_), x, y) => after = (add(after.0, x), add(after.1, y)),
                    }
                }
                // No-op parts are left unset: a browser composes `transform` with the separate
                // `translate`, `rotate` and `scale` properties rather than replacing them, and
                // tw-animate-css writes a zero translate into every `enter`/`exit` keyframe —
                // which must not cancel an element's own `translate: -50% -50%` centering.
                let zero = |l: Lp| match l {
                    Lp::Px(p) => p == 0.0,
                    Lp::Fraction(f) => f == 0.0,
                };
                self.s.translate = match sum {
                    (a, b) if zero(a) && zero(b) => None,
                    _ => Some(point(sum.0.into(), sum.1.into())),
                };
                self.s.rotate = rotate.filter(|r| *r != 0.0);
                self.s.scale = scale.filter(|s| s.x != 1.0 || s.y != 1.0);
                self.s.skew = (skew != Point::default()).then_some(skew);
                // A translate in % following a rotation (sonner's `rotate(30deg) translate(146%)`) is
                // resolved against the laid-out size when painting.
                self.s.translate_after = match after {
                    (a, b) if zero(a) && zero(b) => None,
                    _ => Some(point(after.0.into(), after.1.into())),
                };
            }
            P::ContainerType(t) => {
                self.contain_inline = matches!(t, ContainerType::InlineSize | ContainerType::Size)
            }
            P::ContainerName(names) => {
                self.container_names = match names {
                    ContainerNameList::Names(n) => n.iter().map(|n| n.0.0.to_string()).collect(),
                    ContainerNameList::None => Vec::new(),
                }
            }
            // What `font: inherit` expands to; other values have no gpui equivalent.
            P::FontStretch(..) | P::FontVariantCaps(..)
                if matches!(super::value_text(p).as_str(), "normal" | "100%") => {}
            // shortcut: text is laid out without extra word spacing, which is right for 0.
            P::WordSpacing(..) if matches!(super::value_text(p).as_str(), "0" | "0px" | "normal") => {}
            P::BorderSpacing(s) => {
                self.table.spacing = Some((self.length(&s.0).unwrap_or(0.0), self.length(&s.1).unwrap_or(0.0)))
            }
            P::VerticalAlign(v) => {
                use lightningcss::properties::font::{VerticalAlign as CssVAlign, VerticalAlignKeyword as K};
                self.table.valign = Some(match v {
                    CssVAlign::Keyword(K::Top | K::TextTop) => VAlign::Top,
                    CssVAlign::Keyword(K::Middle) => VAlign::Middle,
                    CssVAlign::Keyword(K::Bottom | K::TextBottom) => VAlign::Bottom,
                    CssVAlign::Keyword(K::Sub) => VAlign::Sub,
                    CssVAlign::Keyword(K::Super) => VAlign::Super,
                    _ => VAlign::Baseline,
                })
            }
            P::Custom(c) if c.name.as_ref() == "border-collapse" => {
                self.table.collapse = Some(super::value_text(p).trim() == "collapse")
            }
            P::Custom(c) if c.name.as_ref() == "table-layout" => {
                self.table.fixed = super::value_text(p).trim() == "fixed"
            }
            // shortcut: border-box is the default (Tailwind's and MUI's); only a declared
            // content-box is honored, as `box-sizing: inherit` isn't resolved.
            P::BoxSizing(b, _) => {
                self.s.content_box = Some(*b == lightningcss::properties::size::BoxSizing::ContentBox)
            }
            // Old flexbox spellings Emotion writes beside the standard properties, which are applied.
            | P::BoxFlex(..)
            | P::BoxFlexGroup(..)
            | P::BoxOrdinalGroup(..)
            | P::BoxAlign(..)
            | P::BoxPack(..)
            | P::BoxDirection(..)
            | P::BoxLines(..)
            | P::FlexPositive(..)
            | P::FlexNegative(..)
            | P::FlexPreferredSize(..)
            | P::FlexOrder(..)
            | P::FlexPack(..)
            | P::FlexAlign(..)
            | P::FlexItemAlign(..)
            | P::FlexLinePack(..)
            // Only native widgets follow these (`light-dark()` follows the window's appearance).
            | P::ColorScheme(..)
            | P::PrintColorAdjust(..)
            // shortcut: the last line is aligned as the others; MUI sets it on one-line selects.
            | P::TextAlignLast(..)
            | P::TextSizeAdjust(..)
            | P::TabSize(..)
            | P::TextRendering(..)
            | P::ListStyleImage(..)
            | P::UserSelect(..)
            | P::TextIndent(..)
            | P::Resize(..)
            // shortcut: where scrolling into view stops; elements scroll flush to the edge.
            | P::ScrollMarginTop(..)
            | P::ScrollMarginBottom(..)
            | P::ScrollMarginLeft(..)
            | P::ScrollMarginRight(..)
            | P::ScrollMarginBlockStart(..)
            | P::ScrollMarginBlockEnd(..)
            | P::ScrollMarginInlineStart(..)
            | P::ScrollMarginInlineEnd(..)
            | P::ScrollPaddingTop(..)
            | P::ScrollPaddingBottom(..)
            | P::ScrollPaddingLeft(..)
            | P::ScrollPaddingRight(..)
            | P::ScrollPaddingBlockStart(..)
            | P::ScrollPaddingBlockEnd(..)
            | P::ScrollPaddingInlineStart(..)
            | P::ScrollPaddingInlineEnd(..)
            // Longhands every `background:` shorthand expands into that aren't drawn.
            | P::BackgroundAttachment(..)
            | P::BackgroundOrigin(..)
            | P::FontSize(..)
            | P::Color(..)
            | P::Custom(..)
            | P::Unparsed(..) => {}
            P::ListStyleType(t) => {
                self.list.marker = Some(match t {
                    ListStyleType::None => Marker::None,
                    ListStyleType::String(s) => Marker::Text(s.to_string()),
                    ListStyleType::CounterStyle(CounterStyle::Predefined(p)) => match p {
                        PredefinedCounterStyle::Disc => Marker::Disc,
                        PredefinedCounterStyle::Circle => Marker::Circle,
                        PredefinedCounterStyle::Square => Marker::Square,
                        PredefinedCounterStyle::DecimalLeadingZero => Marker::DecimalLeadingZero,
                        PredefinedCounterStyle::LowerRoman => Marker::LowerRoman,
                        PredefinedCounterStyle::UpperRoman => Marker::UpperRoman,
                        PredefinedCounterStyle::LowerAlpha | PredefinedCounterStyle::LowerLatin => Marker::LowerAlpha,
                        PredefinedCounterStyle::UpperAlpha | PredefinedCounterStyle::UpperLatin => Marker::UpperAlpha,
                        PredefinedCounterStyle::DisclosureOpen => Marker::Text("▼\u{a0}".into()),
                        PredefinedCounterStyle::DisclosureClosed => Marker::Text("▶\u{a0}".into()),
                        _ => Marker::Decimal,
                    },
                    ListStyleType::CounterStyle(_) => {
                        warn_once("custom counter styles (decimal)");
                        Marker::Decimal
                    }
                })
            }
            // shortcut: the first shadow, unblurred.
            P::TextShadow(shadows) => {
                self.text_shadow = Some(shadows.first().and_then(|t| {
                    Some((self.color(&t.color)?, self.length(&t.x_offset)?, self.length(&t.y_offset)?))
                }))
            }
            P::ListStylePosition(p) => self.list.inside = Some(*p == ListStylePosition::Inside),
            P::BackgroundRepeat(repeats) => {
                let on = |k: &BackgroundRepeatKeyword| *k != BackgroundRepeatKeyword::NoRepeat;
                self.bg_repeats = repeats.iter().map(|r| [on(&r.x), on(&r.y)]).collect();
            }
            P::BackgroundClip(clips, _) => self.clip_text = clips.contains(&BackgroundClip::Text),
            other => warn_once(other.property_id().name()),
        }
    }

    fn display(&mut self, d: &CssDisplay) {
        self.table.role = match d {
            CssDisplay::Keyword(DisplayKeyword::TableRowGroup | DisplayKeyword::TableHeaderGroup | DisplayKeyword::TableFooterGroup) => {
                Some(TableRole::RowGroup)
            }
            CssDisplay::Keyword(DisplayKeyword::TableRow) => Some(TableRole::Row),
            CssDisplay::Keyword(DisplayKeyword::TableCell) => Some(TableRole::Cell),
            CssDisplay::Pair(pair) if matches!(pair.inside, DisplayInside::Table) => Some(TableRole::Table),
            _ => None,
        };
        let disp = match d {
            CssDisplay::Keyword(DisplayKeyword::None) => Disp::None,
            CssDisplay::Keyword(DisplayKeyword::Contents) => Disp::Contents,
            // Tables are laid out by the renderer from their tags; parts are plain blocks here.
            CssDisplay::Keyword(
                DisplayKeyword::TableRowGroup
                | DisplayKeyword::TableHeaderGroup
                | DisplayKeyword::TableFooterGroup
                | DisplayKeyword::TableRow
                | DisplayKeyword::TableCell
                | DisplayKeyword::TableColumnGroup
                | DisplayKeyword::TableColumn
                | DisplayKeyword::TableCaption,
            ) => Disp::Block,
            CssDisplay::Keyword(k) => {
                warn_once(&format!("display: {k:?}"));
                Disp::Block
            }
            CssDisplay::Pair(pair) => {
                self.list.item = pair.is_list_item;
                let inline = pair.outside == DisplayOutside::Inline;
                match (&pair.inside, inline) {
                    (DisplayInside::Flex(_), false) => Disp::Flex,
                    (DisplayInside::Flex(_), true) => Disp::InlineFlex,
                    // `-webkit-box` today only means line-clamped text: a block of text.
                    (DisplayInside::Box(_), _) => Disp::Block,
                    (DisplayInside::Grid, false) => Disp::Grid,
                    (DisplayInside::Grid, true) => Disp::InlineGrid,
                    (DisplayInside::FlowRoot | DisplayInside::Table, true) => Disp::InlineBlock,
                    (_, true) => Disp::Inline,
                    (_, false) => Disp::Block,
                }
            }
        };
        self.disp = disp;
        self.s.display = Some(match disp {
            Disp::Flex | Disp::InlineFlex => Display::Flex,
            Disp::Grid | Disp::InlineGrid => Display::Grid,
            Disp::None => Display::None,
            _ => Display::Block,
        });
    }

    fn color(&self, c: &CssColor) -> Option<Hsla> {
        self.color_with(c, self.ctx.color)
    }

    fn color_with(&self, c: &CssColor, current: Hsla) -> Option<Hsla> {
        match c {
            CssColor::CurrentColor => Some(current),
            CssColor::LightDark(light, dark) => {
                self.color_with(if self.ctx.dark { dark } else { light }, current)
            }
            // Chrome's, light and dark.
            CssColor::System(s) => {
                use lightningcss::values::color::SystemColor as S;
                let (light, dark) = match s {
                    S::Canvas | S::Field => (0xffffff, if matches!(s, S::Canvas) { 0x121212 } else { 0x3b3b3b }),
                    S::CanvasText | S::FieldText | S::ButtonText | S::HighlightText | S::MarkText => (0x000000, 0xffffff),
                    S::ButtonFace => (0xefefef, 0x6b6b6b),
                    S::ButtonBorder => (0x767676, 0x6b6b6b),
                    S::GrayText => (0x6d6d6d, 0x6d6d6d),
                    S::Highlight => (0xb5d5ff, 0x3f638b),
                    S::LinkText => (0x0000ee, 0x9e9eff),
                    S::VisitedText => (0x551a8b, 0xd0adf0),
                    S::ActiveText => (0xff0000, 0xff9e9e),
                    S::Mark => (0xffff00, 0xffff00),
                    S::AccentColor | S::SelectedItem => (0x0075ff, 0x99c8ff),
                    S::AccentColorText | S::SelectedItemText => (0xffffff, 0x000000),
                    _ => {
                        warn_once("deprecated system colors");
                        return None;
                    }
                };
                Some(gpui::rgb(if self.ctx.dark { dark } else { light }).into())
            }
            c => match c.to_rgb() {
                Ok(CssColor::RGBA(rgba)) => Some(
                    Rgba {
                        r: rgba.red as f32 / 255.0,
                        g: rgba.green as f32 / 255.0,
                        b: rgba.blue as f32 / 255.0,
                        a: rgba.alpha_f32(),
                    }
                    .into(),
                ),
                _ => {
                    warn_once("color that cannot be converted to sRGB");
                    None
                }
            },
        }
    }

    fn length_value(&self, v: &LengthValue, font_px: f32) -> Option<f32> {
        Some(match v {
            LengthValue::Rem(n) => n * REM,
            LengthValue::Em(n) => n * font_px,
            LengthValue::Ch(n) => n * self.ctx.ch * font_px,
            LengthValue::Rch(n) => n * self.ctx.ch * REM,
            // shortcut: typical x-height, cap height and line height, not the font's or the element's.
            LengthValue::Ex(n) => n * 0.5 * font_px,
            LengthValue::Rex(n) => n * 0.5 * REM,
            LengthValue::Cap(n) => n * 0.7 * font_px,
            LengthValue::Rcap(n) => n * 0.7 * REM,
            LengthValue::Ic(n) => n * font_px,
            LengthValue::Ric(n) => n * REM,
            LengthValue::Lh(n) => n * 1.5 * font_px,
            LengthValue::Rlh(n) => n * 1.5 * REM,
            // A desktop window has no collapsing browser chrome: small, large and dynamic viewports match.
            LengthValue::Vw(n)
            | LengthValue::Svw(n)
            | LengthValue::Lvw(n)
            | LengthValue::Dvw(n) => n * self.ctx.vw / 100.0,
            LengthValue::Vh(n)
            | LengthValue::Svh(n)
            | LengthValue::Lvh(n)
            | LengthValue::Dvh(n) => n * self.ctx.vh / 100.0,
            LengthValue::Vmin(n) => n * self.ctx.vw.min(self.ctx.vh) / 100.0,
            LengthValue::Vmax(n) => n * self.ctx.vw.max(self.ctx.vh) / 100.0,
            v => match v.to_px() {
                Some(p) => p,
                None => {
                    warn_once(&format!("length unit in {v:?}"));
                    return None;
                }
            },
        })
    }

    fn length(&self, l: &CssLength) -> Option<f32> {
        match l {
            CssLength::Value(v) => self.length_value(v, self.ctx.font_px).map(finite),
            CssLength::Calc(c) => self.calc_px(c, &|v: &CssLength| self.length(v)),
        }
    }

    fn lp(&self, l: &LengthPercentage) -> Option<Lp> {
        let r = self.lp_exact(l);
        if r.is_none() && matches!(l, DimensionPercentage::Calc(_)) {
            warn_once("calc() mixing percentages and lengths");
        }
        r
    }

    /// `lp` without the warning, for callers that take a mixed calc() another way.
    fn lp_exact(&self, l: &LengthPercentage) -> Option<Lp> {
        match l {
            DimensionPercentage::Dimension(v) => self
                .length_value(v, self.ctx.font_px)
                .map(|p| Lp::Px(finite(p))),
            DimensionPercentage::Percentage(p) => Some(Lp::Fraction(p.0)),
            // All lengths or all percentages (Tailwind's `calc(1/2 * -100%)`); a mix needs layout.
            DimensionPercentage::Calc(c) => {
                let px = |v: &LengthPercentage| match self.lp_exact(v) {
                    Some(Lp::Px(p)) => Some(p),
                    _ => None,
                };
                let fraction = |v: &LengthPercentage| match self.lp_exact(v) {
                    Some(Lp::Fraction(f)) => Some(f),
                    _ => None,
                };
                match (self.calc_eval(c, &px), self.calc_eval(c, &fraction)) {
                    (Some(p), _) => Some(Lp::Px(p)),
                    (None, Some(f)) => Some(Lp::Fraction(f)),
                    (None, None) => None,
                }
            }
        }
    }

    /// Only calc() that reduces to a single absolute length is expressible in gpui.
    fn calc_px<V>(&self, c: &Calc<V>, value: &dyn Fn(&V) -> Option<f32>) -> Option<f32> {
        let r = self.calc_eval(c, value);
        if r.is_none() {
            warn_once("calc() mixing percentages and lengths");
        }
        r
    }

    /// calc() over values `value` can resolve to one number (all px, or all fractions).
    fn calc_eval<V>(&self, c: &Calc<V>, value: &dyn Fn(&V) -> Option<f32>) -> Option<f32> {
        let all = |cs: &[Calc<V>]| {
            cs.iter()
                .map(|c| self.calc_eval(c, value))
                .collect::<Option<Vec<f32>>>()
        };
        let r = match c {
            Calc::Value(v) => value(v),
            Calc::Sum(a, b) => self
                .calc_eval(a, value)
                .zip(self.calc_eval(b, value))
                .map(|(a, b)| a + b),
            Calc::Product(n, v) => self.calc_eval(v, value).map(|v| n * v),
            Calc::Function(f) => match f.as_ref() {
                MathFunction::Calc(c) => self.calc_eval(c, value),
                MathFunction::Min(cs) => all(cs).and_then(|v| v.into_iter().reduce(f32::min)),
                MathFunction::Max(cs) => all(cs).and_then(|v| v.into_iter().reduce(f32::max)),
                MathFunction::Clamp(lo, v, hi) => {
                    let (lo, v, hi) = (
                        self.calc_eval(lo, value)?,
                        self.calc_eval(v, value)?,
                        self.calc_eval(hi, value)?,
                    );
                    Some(v.clamp(lo, hi.max(lo)))
                }
                MathFunction::Abs(c) => self.calc_eval(c, value).map(f32::abs),
                _ => None,
            },
            _ => None,
        };
        r.map(finite)
    }

    fn paint(&self, paint: &SVGPaint) -> Option<Paint> {
        match paint {
            SVGPaint::None => Some(Paint::None),
            SVGPaint::Color(c) => self.color(c).map(Paint::Color),
            // shortcut: the fallback color is dropped; a missing gradient paints nothing.
            SVGPaint::Url { url, .. } => Some(Paint::Url(url.url.to_string().into())),
            _ => {
                warn_once("fill/stroke other than a color or none");
                None
            }
        }
    }

    /// A translate, plus the px part of any axis that mixes a percentage and a length.
    fn translation(
        &self,
        x: &LengthPercentage,
        y: &LengthPercentage,
    ) -> (Option<Point<DefiniteLength>>, Option<Point<gpui::Pixels>>) {
        if let (Some(x), Some(y)) = (self.lp_exact(x), self.lp_exact(y)) {
            return (Some(point(x.into(), y.into())), None);
        }
        // Each axis as `fraction * size + px`, which layout adds back together.
        let (Some((xf, xp)), Some((yf, yp))) = (self.lp_parts(x), self.lp_parts(y)) else {
            return (None, None);
        };
        let f = DefiniteLength::Fraction;
        (Some(point(f(xf), f(yf))), Some(point(px(xp), px(yp))))
    }

    /// A length-percentage as (fraction of the box, px), for a `calc()` that adds the two.
    fn lp_parts(&self, l: &LengthPercentage) -> Option<(f32, f32)> {
        match l {
            // Not parts by parts: max(10%, 20px) isn't max(10%, 0) + max(0, 20px).
            DimensionPercentage::Calc(c) => match self.calc_expr(c)? {
                CalcTree::Linear(fraction, length) => Some((fraction, length)),
                _ => None,
            },
            _ => match self.lp_exact(l)? {
                Lp::Px(p) => Some((0.0, p)),
                Lp::Fraction(f) => Some((f, 0.0)),
            },
        }
    }

    /// A layout length; a calc() adding a percentage and a length stays one (`calc(100% + 14px)`).
    fn definite(&self, l: &LengthPercentage) -> Option<DefiniteLength> {
        if let DimensionPercentage::Calc(_) = l
            && let Some((fraction, length)) = self.lp_parts(l)
            && fraction != 0.0
            && length != 0.0
        {
            return Some(DefiniteLength::Calc(fraction, px(length)));
        }
        if let DimensionPercentage::Calc(c) = l
            && self.lp_exact(l).is_none()
            && let Some(e) = self.calc_expr(c)
        {
            let mut ops = Vec::new();
            if e.flatten(&mut ops) <= gpui::CalcExpr::DEPTH {
                return Some(DefiniteLength::Expr(gpui::CalcExpr::new(&ops)));
            }
        }
        self.lp(l).map(Into::into)
    }

    /// A calc() of percentages and lengths, `min()`s, `max()`s and `clamp()`s included.
    fn calc_expr(&self, c: &Calc<LengthPercentage>) -> Option<CalcTree> {
        let all = |cs: &[Calc<LengthPercentage>]| cs.iter().map(|c| self.calc_expr(c)).collect::<Option<Vec<_>>>();
        Some(match c {
            Calc::Value(v) => match v.as_ref() {
                DimensionPercentage::Calc(c) => self.calc_expr(c)?,
                v => {
                    let (fraction, length) = self.lp_parts(v)?;
                    CalcTree::Linear(fraction, length)
                }
            },
            Calc::Sum(a, b) => self.calc_expr(a)?.add(self.calc_expr(b)?),
            Calc::Product(n, v) => self.calc_expr(v)?.times(*n),
            Calc::Function(f) => match f.as_ref() {
                MathFunction::Calc(c) => self.calc_expr(c)?,
                MathFunction::Min(cs) => CalcTree::Min(all(cs)?).simplified(),
                MathFunction::Max(cs) => CalcTree::Max(all(cs)?).simplified(),
                MathFunction::Clamp(lo, v, hi) => CalcTree::Max(vec![
                    self.calc_expr(lo)?,
                    CalcTree::Min(vec![self.calc_expr(v)?, self.calc_expr(hi)?]).simplified(),
                ])
                .simplified(),
                _ => return None,
            },
            _ => return None,
        })
    }

    fn lpa(&self, v: &LengthPercentageOrAuto) -> Option<Length> {
        match v {
            LengthPercentageOrAuto::Auto => Some(Length::Auto),
            LengthPercentageOrAuto::LengthPercentage(l) => self.definite(l).map(Length::Definite),
        }
    }

    fn lpa_definite(&self, v: &LengthPercentageOrAuto) -> Option<DefiniteLength> {
        match v {
            LengthPercentageOrAuto::Auto => Some(px(0.0).into()),
            LengthPercentageOrAuto::LengthPercentage(l) => self.definite(l),
        }
    }

    fn size(&self, v: &Size) -> Option<Length> {
        match v {
            Size::Auto => Some(Length::Auto),
            Size::LengthPercentage(l) => self.definite(l).map(Length::Definite),
            Size::Stretch(_) => Some(Length::Definite(DefiniteLength::Fraction(1.0))),
            // Content-sized widths are auto plus no stretching; see `fit_width`.
            Size::FitContent(_)
            | Size::FitContentFunction(_)
            | Size::MinContent(_)
            | Size::MaxContent(_) => Some(Length::Auto),
            other => {
                warn_once(&format!("size {other:?} (treated as auto)"));
                Some(Length::Auto)
            }
        }
    }

    fn max_size(&self, v: &MaxSize) -> Option<Length> {
        match v {
            MaxSize::None => Some(Length::Auto),
            MaxSize::LengthPercentage(l) => self.definite(l).map(Length::Definite),
            other => {
                warn_once(&format!("max size {other:?} (treated as none)"));
                Some(Length::Auto)
            }
        }
    }

    fn gap(&self, g: &GapValue) -> Option<DefiniteLength> {
        match g {
            GapValue::Normal => Some(px(0.0).into()),
            GapValue::LengthPercentage(l) => self.definite(l),
        }
    }

    fn border_width(&self, w: &BorderSideWidth) -> Option<f32> {
        match w {
            BorderSideWidth::Thin => Some(1.0),
            BorderSideWidth::Medium => Some(3.0),
            BorderSideWidth::Thick => Some(5.0),
            BorderSideWidth::Length(l) => self.length(l),
        }
    }

    fn border_style(&mut self, side: usize, s: &LineStyle) {
        self.border_none[side] = matches!(s, LineStyle::None | LineStyle::Hidden);
        // shortcut: one style for all sides (gpui's), the last side's that has one.
        match s {
            LineStyle::Dashed => self.s.border_style = Some(BorderStyle::Dashed),
            LineStyle::Dotted => self.s.border_style = Some(BorderStyle::Dotted),
            LineStyle::Double => self.s.border_style = Some(BorderStyle::Double),
            _ if !self.border_none[side] => self.s.border_style = Some(BorderStyle::Solid),
            _ => {}
        }
    }

    fn radius(&self, l: &LengthPercentage) -> Option<AbsoluteLength> {
        let v = match self.lp(l)? {
            Lp::Px(p) => p,
            // Percent radii need the box size; 50%+ is how "make it round" is spelled.
            Lp::Fraction(f) if f >= 0.5 => 9999.0,
            Lp::Fraction(_) => {
                warn_once("percentage border-radius below 50%");
                return None;
            }
        };
        Some(AbsoluteLength::Pixels(px(v)))
    }

    fn font_size(&self, size: &FontSize) -> Option<f32> {
        let parent = self.ctx.parent_font_px;
        Some(match size {
            FontSize::Length(DimensionPercentage::Percentage(p)) => parent * p.0,
            FontSize::Length(DimensionPercentage::Dimension(v)) => self.length_value(v, parent)?,
            FontSize::Length(l) => match self.lp(l)? {
                Lp::Px(p) => p,
                Lp::Fraction(f) => parent * f,
            },
            FontSize::Absolute(a) => match a {
                AbsoluteFontSize::XXSmall => 9.0,
                AbsoluteFontSize::XSmall => 10.0,
                AbsoluteFontSize::Small => 13.0,
                AbsoluteFontSize::Medium => 16.0,
                AbsoluteFontSize::Large => 18.0,
                AbsoluteFontSize::XLarge => 24.0,
                AbsoluteFontSize::XXLarge => 32.0,
                AbsoluteFontSize::XXXLarge => 48.0,
            },
            FontSize::Relative(RelativeFontSize::Smaller) => parent / 1.2,
            FontSize::Relative(RelativeFontSize::Larger) => parent * 1.2,
        })
    }

    /// The first installed family, and its `1ch` as a fraction of its size.
    fn font_family(&self, families: &[FontFamily]) -> Option<(SharedString, f32)> {
        for f in families {
            let name = match f {
                FontFamily::Generic(g) => match g {
                    GenericFontFamily::SansSerif
                    | GenericFontFamily::SystemUI
                    | GenericFontFamily::UISansSerif
                    | GenericFontFamily::UIRounded => ".SystemUIFont".into(),
                    GenericFontFamily::Monospace | GenericFontFamily::UIMonospace => "Menlo".into(),
                    GenericFontFamily::Serif | GenericFontFamily::UISerif => {
                        "Times New Roman".into()
                    }
                    _ => continue,
                },
                FontFamily::FamilyName(n) => {
                    let quoted = n
                        .to_css_string(PrinterOptions::default())
                        .unwrap_or_default();
                    match quoted.trim_matches(['"', '\'']) {
                        // Browsers' names for the platform UI font, first in Tailwind's default stack.
                        "-apple-system" | "BlinkMacSystemFont" => ".SystemUIFont".into(),
                        name => name.to_owned(),
                    }
                }
            };
            if let Some(ch) = (self.fonts)(&name) {
                return Some((SharedString::from(name), ch));
            }
        }
        None
    }

    fn background_image(&mut self, images: &[Image]) {
        self.bg_url = None;
        self.bg_layers.clear();
        let space = match std::mem::take(&mut self.bg_oklab) {
            true => gpui::ColorSpace::Oklab,
            false => gpui::ColorSpace::Srgb,
        };
        for image in images {
            let layer = match image {
                Image::Gradient(g) => self.gradient_layer(g).map(|l| BgLayer { space, ..l }),
                // shortcut: one url() layer, root-relative or a file path (see `css_asset`).
                Image::Url(url) => {
                    self.bg_url = Some(url.url.to_string());
                    None
                }
                Image::None => None,
                _ => {
                    warn_once("background-image other than url() and gradients");
                    None
                }
            };
            if let Some(l) = &layer {
                // The one-`Fill` drawing, which `bg_layers` keeps when it is exact.
                let stops: Vec<LinearColorStop> = self.flat_stops(&l.stops);
                if let (Some(first), Some(last)) = (stops.first(), stops.last()) {
                    self.gradient_mid = Some(midpoint(first.color, last.color));
                    self.s.background = match l.shape {
                        BgShape::Linear(angle) => Some(linear_gradient_stops(angle, &stops).color_space(space).into()),
                        BgShape::Radial { circle, .. } => Some(radial_gradient(circle, &stops).color_space(space).into()),
                        BgShape::Conic { .. } => self.s.background.take(),
                    };
                }
            }
            self.bg_layers.push(layer);
        }
    }

    fn gradient_layer(&self, g: &Gradient) -> Option<BgLayer> {
        let center = |p: &CssPositionValue| {
            [
                origin_axis(self, &p.x, |k| *k == HorizontalPositionKeyword::Right).unwrap_or(Lp::Fraction(0.5)),
                origin_axis(self, &p.y, |k| *k == VerticalPositionKeyword::Bottom).unwrap_or(Lp::Fraction(0.5)),
            ]
        };
        let lp_stops = |items: &[GradientItem<LengthPercentage>]| {
            items
                .iter()
                .filter_map(|item| match item {
                    GradientItem::ColorStop(s) => {
                        Some((self.color(&s.color)?, s.position.as_ref().and_then(|p| self.lp(p))))
                    }
                    GradientItem::Hint(_) => None,
                })
                .collect::<Vec<_>>()
        };
        let (shape, stops, repeating) = match g {
            Gradient::Linear(lg) | Gradient::RepeatingLinear(lg) => (
                BgShape::Linear(line_angle(&lg.direction)),
                lp_stops(&lg.items),
                matches!(g, Gradient::RepeatingLinear(_)),
            ),
            Gradient::Radial(rg) | Gradient::RepeatingRadial(rg) => {
                let extent = |e: &ShapeExtent| match e {
                    ShapeExtent::ClosestSide => RadialSize::ClosestSide,
                    ShapeExtent::FarthestSide => RadialSize::FarthestSide,
                    ShapeExtent::ClosestCorner => RadialSize::ClosestCorner,
                    ShapeExtent::FarthestCorner => RadialSize::FarthestCorner,
                };
                let (circle, size) = match &rg.shape {
                    EndingShape::Circle(Circle::Extent(e)) => (true, extent(e)),
                    EndingShape::Circle(Circle::Radius(r)) => {
                        let r = Lp::Px(self.length(r)?);
                        (true, RadialSize::Radii([r, r]))
                    }
                    EndingShape::Ellipse(Ellipse::Extent(e)) => (false, extent(e)),
                    EndingShape::Ellipse(Ellipse::Size { x, y }) => {
                        (false, RadialSize::Radii([self.lp(x)?, self.lp(y)?]))
                    }
                };
                (
                    BgShape::Radial { circle, center: center(&rg.position), size },
                    lp_stops(&rg.items),
                    matches!(g, Gradient::RepeatingRadial(_)),
                )
            }
            Gradient::Conic(cg) | Gradient::RepeatingConic(cg) => {
                let stops = cg
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        GradientItem::ColorStop(s) => Some((
                            self.color(&s.color)?,
                            s.position.as_ref().and_then(|p| match p {
                                DimensionPercentage::Dimension(a) => Some(Lp::Fraction(a.to_degrees() / 360.0)),
                                DimensionPercentage::Percentage(p) => Some(Lp::Fraction(p.0)),
                                DimensionPercentage::Calc(_) => None,
                            }),
                        )),
                        GradientItem::Hint(_) => None,
                    })
                    .collect();
                (
                    BgShape::Conic { from: cg.angle.to_degrees(), center: center(&cg.position) },
                    stops,
                    matches!(g, Gradient::RepeatingConic(_)),
                )
            }
            Gradient::WebKitGradient(_) => {
                warn_once("-webkit-gradient()");
                return None;
            }
        };
        (!stops.is_empty()).then_some(BgLayer {
            shape,
            stops,
            repeating,
            size: [None, None],
            at: [Lp::Px(0.0), Lp::Px(0.0)],
            repeat: [true, true],
            space: gpui::ColorSpace::Srgb,
        })
    }

    /// A gradient's stops for one gpui `Fill`, at the positions CSS gives them. gpui interpolates
    /// straight alpha, so a faint stop (`from-primary/5`) would gray the middle; browsers
    /// premultiply. Composited over what is behind, the stops are opaque and both agree.
    // shortcut: assumes the backdrop is one color; see-through to an image differs.
    fn flat_stops(&self, stops: &[(Hsla, Option<Lp>)]) -> Vec<LinearColorStop> {
        let mut stops: Vec<(Hsla, Option<f32>)> = stops
            .iter()
            .map(|&(c, at)| {
                let at = match at {
                    Some(Lp::Fraction(f)) => Some(f),
                    // px positions need the gradient line's length: `bg_layers` paints those.
                    _ => None,
                };
                (over(c, self.ctx.backdrop), at)
            })
            .collect();
        if let [only] = stops[..] {
            stops.push(only);
        }
        let mut at: Vec<Option<f32>> = stops.iter().map(|s| s.1).collect();
        fill_positions(&mut at);
        stops
            .iter()
            .zip(at)
            .map(|(&(c, _), at)| linear_color_stop(c, at.unwrap_or(0.0)))
            .collect()
    }

    /// Each side's border color (top, right, bottom, left; clear where it doesn't draw) when the
    /// sides that draw differ, which gpui's one border color can't show: `render` paints them.
    // shortcut: rounded boxes keep one color.
    pub fn border_sides(&mut self) -> Option<[Hsla; 4]> {
        let draws = |i: usize| !self.border_none[i] && self.widths[i].is_some_and(|w| w > 0.0);
        let mut sides = [None; 4];
        for &(i, c) in &self.border_colors {
            sides[i] = Some(c.unwrap_or(self.ctx.color));
        }
        let sides = std::array::from_fn(|i| match draws(i) {
            true => sides[i].unwrap_or(self.ctx.color),
            false => gpui::transparent_black(),
        });
        let drawn: Vec<Hsla> = (0..4).filter(|&i| draws(i)).map(|i| sides[i]).collect();
        let r = &self.s.corner_radii;
        let rounded = [r.top_left, r.top_right, r.bottom_right, r.bottom_left]
            .iter()
            .any(|r| r.is_some_and(|r| r.to_pixels(px(REM)) > px(0.0)));
        // A box drawing only some sides in one color is a plain border (`border-b`).
        self.split_border = !rounded && drawn.windows(2).any(|w| w[0] != w[1]);
        self.split_border.then_some(sides)
    }

    /// The gradient layers for `crate::background` to paint, top first. None when the background is
    /// one gradient that the element's own `Fill` draws exactly (the common case); otherwise that
    /// `Fill` goes back to the background color.
    pub fn bg_layers(&mut self) -> Vec<BgLayer> {
        fn cycle<T: Copy>(v: &[T], i: usize, default: T) -> T {
            if v.is_empty() { default } else { v[i % v.len()] }
        }
        let layers: Vec<BgLayer> = std::mem::take(&mut self.bg_layers)
            .into_iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let mut l = l?;
                l.size = cycle(&self.bg_sizes, i, [None, None]);
                l.at = [cycle(&self.bg_at[0], i, Lp::Px(0.0)), cycle(&self.bg_at[1], i, Lp::Px(0.0))];
                l.repeat = cycle(&self.bg_repeats, i, [true, true]);
                Some(l)
            })
            .collect();
        let whole = |s: Option<Lp>| s.is_none() || s == Some(Lp::Fraction(1.0));
        let one_fill = |l: &BgLayer| {
            !l.repeating
                && l.size.into_iter().all(whole)
                && l.stops.len() <= 8
                && l.stops.iter().all(|s| !matches!(s.1, Some(Lp::Px(_))))
                && match l.shape {
                    BgShape::Linear(_) => true,
                    BgShape::Radial { center, size, .. } => {
                        center == [Lp::Fraction(0.5); 2] && size == RadialSize::FarthestCorner
                    }
                    BgShape::Conic { .. } => false,
                }
        };
        let images = self.bg_url.is_some() as usize + layers.len();
        if layers.is_empty() || (images == 1 && one_fill(&layers[0])) {
            return Vec::new();
        }
        self.s.background = self.bg.map(Fill::from);
        layers
    }

    fn box_shadow(&mut self, shadows: &[lightningcss::properties::box_shadow::BoxShadow]) {
        let (mut out, mut inset) = (Vec::new(), Vec::new());
        for s in shadows {
            let (Some(color), Some(x), Some(y), Some(blur), Some(spread)) = (
                self.color(&s.color),
                self.length(&s.x_offset),
                self.length(&s.y_offset),
                self.length(&s.blur),
                self.length(&s.spread),
            ) else {
                continue;
            };
            // Transparent `0 0 #0000` placeholders (Tailwind stacks five) are kept so lists line up for
            // transitions; the renderer drops them before painting.
            let shadow = BoxShadow {
                color,
                offset: point(px(x), px(y)),
                blur_radius: px(blur),
                spread_radius: px(spread),
            };
            if s.inset { &mut inset } else { &mut out }.push(shadow);
        }
        self.s.box_shadow = Some(out);
        // shortcut: inset shadows snap rather than transition.
        self.s.inset_shadow = Some(inset);
    }

    fn grid_template(&self, t: &TrackSizing) -> Option<Vec<GridTrack>> {
        let TrackSizing::TrackList(list) = t else {
            return None;
        };
        let mut tracks = Vec::new();
        for item in &list.items {
            tracks.push(match item {
                TrackListItem::TrackSize(t) => self.track(t),
                TrackListItem::TrackRepeat(r) => match r.count {
                    RepeatCount::Number(n) => GridTrack::Repeat(
                        n as u16,
                        r.track_sizes.iter().map(|t| self.track(t)).collect(),
                    ),
                    RepeatCount::AutoFill | RepeatCount::AutoFit => GridTrack::RepeatAuto(
                        r.count == RepeatCount::AutoFit,
                        r.track_sizes.iter().map(|t| self.track(t)).collect(),
                    ),
                },
            });
        }
        Some(tracks)
    }

    fn track(&self, t: &TrackSize) -> GridTrack {
        match t {
            TrackSize::TrackBreadth(b) => self.breadth(b),
            TrackSize::MinMax { min, max } => {
                GridTrack::MinMax(Box::new(self.breadth(min)), Box::new(self.breadth(max)))
            }
            TrackSize::FitContent(l) => match self.lp(l) {
                Some(Lp::Px(v)) => GridTrack::FitContent(AbsoluteLength::Pixels(px(v))),
                _ => GridTrack::Auto,
            },
        }
    }

    fn breadth(&self, b: &TrackBreadth) -> GridTrack {
        match b {
            TrackBreadth::Length(l) => match self.lp_exact(l) {
                Some(Lp::Px(v)) => GridTrack::Length(AbsoluteLength::Pixels(px(v))),
                Some(Lp::Fraction(f)) => GridTrack::Percent(f),
                None => match self.definite(l) {
                    Some(DefiniteLength::Expr(e)) => GridTrack::Calc(e),
                    Some(DefiniteLength::Calc(f, p)) => GridTrack::Calc(gpui::CalcExpr::new(&[0.0, f, f32::from(p)])),
                    _ => GridTrack::Auto,
                },
            },
            TrackBreadth::Flex(f) => GridTrack::Fr(*f),
            TrackBreadth::MinContent => GridTrack::MinContent,
            TrackBreadth::MaxContent => GridTrack::MaxContent,
            TrackBreadth::Auto => GridTrack::Auto,
        }
    }

    fn grid_location(&mut self) -> &mut GridLocation {
        self.s.grid_location.get_or_insert_with(Default::default)
    }

    /// A placement, with an area's name resolved to the parent's line on `side` (an index into
    /// `GridAreas`' lines). shortcut: names of explicit lines (`[main-start]`) aren't known.
    fn grid_line(&self, l: &GridLine, side: usize) -> GridPlacement {
        let GridLine::Area { name } = l else {
            return grid_line(l);
        };
        let area = self.parent_areas.iter().flat_map(|a| a.iter());
        area.clone()
            .find(|(n, _)| n == name.0.as_ref())
            .map_or(GridPlacement::Auto, |(_, lines)| GridPlacement::Line(lines[side]))
    }
}

/// One axis of `transform-origin`; `far` tells the right/bottom keyword from left/top.
fn origin_axis<S>(
    b: &Builder<'_>,
    c: &PositionComponent<S>,
    far: impl Fn(&S) -> bool,
) -> Option<Lp> {
    match c {
        PositionComponent::Center => Some(Lp::Fraction(0.5)),
        PositionComponent::Length(l) => b.lp(l),
        PositionComponent::Side { side, offset } => {
            match (far(side), offset.as_ref().map(|o| b.lp(o))) {
                (false, None) => Some(Lp::Px(0.0)),
                (false, Some(o)) => o,
                (true, None) => Some(Lp::Fraction(1.0)),
                (true, Some(Some(Lp::Fraction(f)))) => Some(Lp::Fraction(1.0 - f)),
                (true, Some(_)) => {
                    warn_once("transform-origin offset in px from the right or bottom");
                    Some(Lp::Fraction(1.0))
                }
            }
        }
    }
}

impl Builder<'_> {
    /// A position along an axis as a fraction of the room left; px offsets would need its size.
    fn fraction(&self, at: Option<Lp>) -> f32 {
        match at {
            Some(Lp::Fraction(f)) => f,
            Some(Lp::Px(0.0)) | None => 0.0,
            Some(Lp::Px(_)) => {
                warn_once("object-position or background-position in px (using the start)");
                0.0
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lp {
    Px(f32),
    Fraction(f32),
}

impl Lp {
    fn times(self, k: f32) -> Lp {
        match self {
            Lp::Px(p) => Lp::Px(p * k),
            Lp::Fraction(f) => Lp::Fraction(f * k),
        }
    }
}

/// Two translates' sum along one axis.
fn add(a: Lp, b: Lp) -> Lp {
    match (a, b) {
        (Lp::Px(a), Lp::Px(b)) => Lp::Px(a + b),
        (Lp::Fraction(a), Lp::Fraction(b)) => Lp::Fraction(a + b),
        (Lp::Px(0.0), b) => b,
        // shortcut: px and % mixed across functions keeps the first; rare in practice.
        (a, _) => a,
    }
}

fn scale_by(scale: &mut Option<Point<f32>>, x: f32, y: f32) {
    let s = scale.get_or_insert(point(1.0, 1.0));
    *s = point(s.x * x, s.y * y);
}

impl From<Lp> for DefiniteLength {
    fn from(l: Lp) -> Self {
        match l {
            Lp::Px(p) => DefiniteLength::Absolute(AbsoluteLength::Pixels(px(p))),
            Lp::Fraction(f) => DefiniteLength::Fraction(f),
        }
    }
}

fn midpoint(a: Hsla, b: Hsla) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    let mid = |x: f32, y: f32| (x + y) / 2.0;
    Rgba {
        r: mid(a.r, b.r),
        g: mid(a.g, b.g),
        b: mid(a.b, b.b),
        a: mid(a.a, b.a),
    }
    .into()
}

fn number(n: &NumberOrPercentage) -> f32 {
    match n {
        NumberOrPercentage::Number(n) => *n,
        NumberOrPercentage::Percentage(p) => p.0,
    }
}

fn seconds(t: &Time) -> f32 {
    match t {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => ms / 1000.0,
    }
}

fn easing(f: &EasingFunction) -> Easing {
    match *f {
        EasingFunction::Linear => Easing::CubicBezier(0.0, 0.0, 1.0, 1.0),
        EasingFunction::Ease => Easing::CubicBezier(0.25, 0.1, 0.25, 1.0),
        EasingFunction::EaseIn => Easing::CubicBezier(0.42, 0.0, 1.0, 1.0),
        EasingFunction::EaseOut => Easing::CubicBezier(0.0, 0.0, 0.58, 1.0),
        EasingFunction::EaseInOut => Easing::CubicBezier(0.42, 0.0, 0.58, 1.0),
        EasingFunction::CubicBezier { x1, y1, x2, y2 } => Easing::CubicBezier(x1, y1, x2, y2),
        EasingFunction::Steps { count, .. } => Easing::Steps(count.max(1) as u32),
    }
}

/// `calc(infinity * 1px)` (Tailwind's `rounded-full`) must not reach layout as infinity.
fn finite(v: f32) -> f32 {
    v.clamp(-1e6, 1e6)
}

fn self_position(v: &SelfPosition) -> AlignItems {
    match v {
        SelfPosition::Center => AlignItems::Center,
        SelfPosition::Start | SelfPosition::SelfStart => AlignItems::Start,
        SelfPosition::End | SelfPosition::SelfEnd => AlignItems::End,
        SelfPosition::FlexStart => AlignItems::FlexStart,
        SelfPosition::FlexEnd => AlignItems::FlexEnd,
    }
}

fn align_items(a: &CssAlignItems) -> Option<AlignItems> {
    Some(match a {
        CssAlignItems::Normal | CssAlignItems::Stretch => AlignItems::Stretch,
        CssAlignItems::BaselinePosition(_) => AlignItems::Baseline,
        CssAlignItems::SelfPosition { value, .. } => self_position(value),
    })
}

fn align_self(a: &CssAlignSelf) -> Option<AlignSelf> {
    Some(match a {
        CssAlignSelf::Auto | CssAlignSelf::Normal => return None,
        CssAlignSelf::Stretch => AlignSelf::Stretch,
        CssAlignSelf::BaselinePosition(_) => AlignSelf::Baseline,
        CssAlignSelf::SelfPosition { value, .. } => self_position(value),
    })
}

fn content_position(v: &ContentPosition) -> AlignContent {
    match v {
        ContentPosition::Center => AlignContent::Center,
        ContentPosition::Start => AlignContent::Start,
        ContentPosition::End => AlignContent::End,
        ContentPosition::FlexStart => AlignContent::FlexStart,
        ContentPosition::FlexEnd => AlignContent::FlexEnd,
    }
}

fn content_distribution(v: &ContentDistribution) -> AlignContent {
    match v {
        ContentDistribution::SpaceBetween => AlignContent::SpaceBetween,
        ContentDistribution::SpaceAround => AlignContent::SpaceAround,
        ContentDistribution::SpaceEvenly => AlignContent::SpaceEvenly,
        ContentDistribution::Stretch => AlignContent::Stretch,
    }
}

fn align_content(a: &CssAlignContent) -> Option<AlignContent> {
    match a {
        CssAlignContent::Normal | CssAlignContent::BaselinePosition(_) => None,
        CssAlignContent::ContentDistribution(d) => Some(content_distribution(d)),
        CssAlignContent::ContentPosition { value, .. } => Some(content_position(value)),
    }
}

fn justify_content(j: &CssJustify) -> Option<JustifyContent> {
    match j {
        CssJustify::Normal => None,
        CssJustify::ContentDistribution(d) => Some(content_distribution(d)),
        CssJustify::ContentPosition { value, .. } => Some(content_position(value)),
        CssJustify::Left { .. } => Some(JustifyContent::Start),
        CssJustify::Right { .. } => Some(JustifyContent::End),
    }
}

fn overflow(o: &OverflowKeyword) -> Overflow {
    match o {
        OverflowKeyword::Visible => Overflow::Visible,
        OverflowKeyword::Hidden | OverflowKeyword::Clip => Overflow::Hidden,
        OverflowKeyword::Scroll | OverflowKeyword::Auto => Overflow::Scroll,
    }
}

fn cursor(k: CursorKeyword) -> Option<CursorStyle> {
    Some(match k {
        CursorKeyword::Auto | CursorKeyword::Default => CursorStyle::Arrow,
        CursorKeyword::None => CursorStyle::None,
        CursorKeyword::Pointer => CursorStyle::PointingHand,
        CursorKeyword::Text => CursorStyle::IBeam,
        CursorKeyword::VerticalText => CursorStyle::IBeamCursorForVerticalLayout,
        CursorKeyword::Crosshair => CursorStyle::Crosshair,
        CursorKeyword::ContextMenu => CursorStyle::ContextualMenu,
        CursorKeyword::Alias => CursorStyle::DragLink,
        CursorKeyword::Copy => CursorStyle::DragCopy,
        CursorKeyword::Grab => CursorStyle::OpenHand,
        CursorKeyword::Grabbing => CursorStyle::ClosedHand,
        CursorKeyword::NotAllowed | CursorKeyword::NoDrop => CursorStyle::OperationNotAllowed,
        CursorKeyword::EResize => CursorStyle::ResizeRight,
        CursorKeyword::WResize => CursorStyle::ResizeLeft,
        CursorKeyword::NResize => CursorStyle::ResizeUp,
        CursorKeyword::SResize => CursorStyle::ResizeDown,
        CursorKeyword::EwResize => CursorStyle::ResizeLeftRight,
        CursorKeyword::NsResize => CursorStyle::ResizeUpDown,
        CursorKeyword::NwseResize | CursorKeyword::NwResize | CursorKeyword::SeResize => {
            CursorStyle::ResizeUpLeftDownRight
        }
        CursorKeyword::NeswResize | CursorKeyword::NeResize | CursorKeyword::SwResize => {
            CursorStyle::ResizeUpRightDownLeft
        }
        CursorKeyword::ColResize => CursorStyle::ResizeColumn,
        CursorKeyword::RowResize => CursorStyle::ResizeRow,
        // help, wait, progress, cell, move, all-scroll, zoom-in/out: no native gpui cursor.
        _ => {
            warn_once(
                "cursor keywords without a native cursor (help, wait, move...) show the arrow",
            );
            CursorStyle::Arrow
        }
    })
}

fn justify_self(j: &JustifySelf) -> Option<AlignSelf> {
    Some(match j {
        JustifySelf::Auto | JustifySelf::Normal => return None,
        JustifySelf::Stretch => AlignSelf::Stretch,
        JustifySelf::BaselinePosition(_) => AlignSelf::Baseline,
        JustifySelf::SelfPosition { value, .. } => self_position(value),
        JustifySelf::Left { .. } => AlignSelf::Start,
        JustifySelf::Right { .. } => AlignSelf::End,
    })
}

fn justify_items(j: &JustifyItems) -> Option<AlignItems> {
    Some(match j {
        JustifyItems::Normal | JustifyItems::Legacy(_) => return None,
        JustifyItems::Stretch => AlignItems::Stretch,
        JustifyItems::BaselinePosition(_) => AlignItems::Baseline,
        JustifyItems::SelfPosition { value, .. } => self_position(value),
        JustifyItems::Left { .. } => AlignItems::Start,
        JustifyItems::Right { .. } => AlignItems::End,
    })
}

/// Each named area's bounding lines, 1-based, ends exclusive as CSS numbers lines.
fn grid_areas(a: &GridTemplateAreas) -> Option<GridAreas> {
    let GridTemplateAreas::Areas { columns, areas } = a else {
        return None;
    };
    let columns = (*columns).max(1) as usize;
    let mut out: Vec<(String, [i16; 4])> = Vec::new();
    for (i, name) in areas.iter().enumerate() {
        let Some(name) = name else { continue };
        let (row, col) = ((i / columns) as i16 + 1, (i % columns) as i16 + 1);
        match out.iter_mut().find(|(n, _)| n == name) {
            Some((_, l)) => *l = [l[0].min(row), l[1].max(row + 1), l[2].min(col), l[3].max(col + 1)],
            None => out.push((name.clone(), [row, row + 1, col, col + 1])),
        }
    }
    Some(std::rc::Rc::new(out))
}

fn grid_line(l: &GridLine) -> GridPlacement {
    match l {
        GridLine::Line { index, .. } => GridPlacement::Line(*index as i16),
        GridLine::Span { index, .. } => GridPlacement::Span(*index as u16),
        _ => GridPlacement::Auto,
    }
}

/// `font-variant-numeric` keywords as the OpenType features they turn on; `normal` is none.
fn numeric_features(value: &str) -> Vec<(String, u32)> {
    let tag = |k: &str| match k {
        "lining-nums" => Some("lnum"),
        "oldstyle-nums" => Some("onum"),
        "proportional-nums" => Some("pnum"),
        "tabular-nums" => Some("tnum"),
        "diagonal-fractions" => Some("frac"),
        "stacked-fractions" => Some("afrc"),
        "ordinal" => Some("ordn"),
        "slashed-zero" => Some("zero"),
        _ => None,
    };
    value.split_whitespace().filter_map(tag).map(|t| (t.to_owned(), 1)).collect()
}

/// `font-feature-settings: "tnum", "liga" 0, "ss01" on` as (tag, value) pairs.
fn feature_settings(value: &str) -> Vec<(String, u32)> {
    value
        .split(',')
        .filter_map(|part| {
            let mut words = part.split_whitespace();
            let tag = words.next()?.trim_matches(['"', '\'']);
            // OpenType tags are four characters; `normal` and anything malformed sets nothing.
            if tag.len() != 4 {
                return None;
            }
            let value = match words.next() {
                None | Some("on") => 1,
                Some("off") => 0,
                Some(n) => n.parse().ok()?,
            };
            Some((tag.to_owned(), value))
        })
        .collect()
}

/// Stop positions as CSS fixes them up: the first defaults to 0 and the last to 1, none comes
/// before an earlier one, and missing ones are spaced evenly between their neighbors.
pub(crate) fn fill_positions(at: &mut [Option<f32>]) {
    let n = at.len();
    if n == 0 {
        return;
    }
    at[0].get_or_insert(0.0);
    at[n - 1].get_or_insert(1.0);
    let mut max = f32::MIN;
    for p in at.iter_mut().flatten() {
        *p = p.max(max);
        max = *p;
    }
    let mut known = 0;
    for i in 1..n {
        if let Some(b) = at[i] {
            let a = at[known].unwrap_or(0.0);
            for k in known + 1..i {
                at[k] = Some(a + (b - a) * (k - known) as f32 / (i - known) as f32);
            }
            known = i;
        }
    }
}

/// A linear gradient's direction as an angle, clockwise from "to top".
fn line_angle(d: &LineDirection) -> f32 {
    match d {
        LineDirection::Angle(a) => a.to_degrees(),
        LineDirection::Vertical(VerticalPositionKeyword::Top) => 0.0,
        LineDirection::Vertical(_) => 180.0,
        LineDirection::Horizontal(HorizontalPositionKeyword::Left) => 270.0,
        LineDirection::Horizontal(_) => 90.0,
        // shortcut: the angle that reaches the corner depends on the box's proportions.
        LineDirection::Corner { horizontal, vertical } => match (horizontal, vertical) {
            (HorizontalPositionKeyword::Right, VerticalPositionKeyword::Top) => 45.0,
            (HorizontalPositionKeyword::Right, VerticalPositionKeyword::Bottom) => 135.0,
            (HorizontalPositionKeyword::Left, VerticalPositionKeyword::Bottom) => 225.0,
            (HorizontalPositionKeyword::Left, VerticalPositionKeyword::Top) => 315.0,
        },
    }
}

/// A `filter` color function as a 4×5 matrix: each output channel (r, g, b, a) from the input's
/// r, g, b, a and 1, as the Filter Effects spec defines them.
fn color_matrix(f: &Filter) -> [[f32; 5]; 4] {
    let amount = |n: &NumberOrPercentage| number(n).max(0.0);
    let diagonal = |k: f32, add: f32| {
        [[k, 0., 0., 0., add], [0., k, 0., 0., add], [0., 0., k, 0., add], [0., 0., 0., 1., 0.]]
    };
    // The 3×3 color part of a matrix that leaves alpha alone.
    let rgb = |m: [[f32; 3]; 3]| {
        let row = |r: [f32; 3]| [r[0], r[1], r[2], 0., 0.];
        [row(m[0]), row(m[1]), row(m[2]), [0., 0., 0., 1., 0.]]
    };
    match f {
        Filter::Brightness(n) => diagonal(amount(n), 0.0),
        Filter::Contrast(n) => diagonal(amount(n), 0.5 - 0.5 * amount(n)),
        Filter::Invert(n) => {
            let a = amount(n).min(1.0);
            diagonal(1.0 - 2.0 * a, a)
        }
        Filter::Opacity(n) => {
            let mut m = diagonal(1.0, 0.0);
            m[3][3] = amount(n).min(1.0);
            m
        }
        Filter::Grayscale(n) => {
            let k = 1.0 - amount(n).min(1.0);
            rgb([
                [0.2126 + 0.7874 * k, 0.7152 - 0.7152 * k, 0.0722 - 0.0722 * k],
                [0.2126 - 0.2126 * k, 0.7152 + 0.2848 * k, 0.0722 - 0.0722 * k],
                [0.2126 - 0.2126 * k, 0.7152 - 0.7152 * k, 0.0722 + 0.9278 * k],
            ])
        }
        Filter::Sepia(n) => {
            let k = 1.0 - amount(n).min(1.0);
            rgb([
                [0.393 + 0.607 * k, 0.769 - 0.769 * k, 0.189 - 0.189 * k],
                [0.349 - 0.349 * k, 0.686 + 0.314 * k, 0.168 - 0.168 * k],
                [0.272 - 0.272 * k, 0.534 - 0.534 * k, 0.131 + 0.869 * k],
            ])
        }
        Filter::Saturate(n) => {
            let s = amount(n);
            rgb([
                [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
                [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
                [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
            ])
        }
        Filter::HueRotate(a) => {
            let (sin, cos) = a.to_radians().sin_cos();
            rgb([
                [0.213 + cos * 0.787 - sin * 0.213, 0.715 - cos * 0.715 - sin * 0.715, 0.072 - cos * 0.072 + sin * 0.928],
                [0.213 - cos * 0.213 + sin * 0.143, 0.715 + cos * 0.285 + sin * 0.140, 0.072 - cos * 0.072 - sin * 0.283],
                [0.213 - cos * 0.213 - sin * 0.787, 0.715 - cos * 0.715 + sin * 0.715, 0.072 + cos * 0.928 + sin * 0.072],
            ])
        }
        Filter::Blur(_) | Filter::DropShadow(_) | Filter::Url(_) => diagonal(1.0, 0.0),
    }
}

/// `then` applied after `first`, as one matrix.
pub(super) fn compose(then: &[[f32; 5]; 4], first: &[[f32; 5]; 4]) -> [[f32; 5]; 4] {
    let mut out = [[0.0; 5]; 4];
    for (i, row) in then.iter().enumerate() {
        for j in 0..5 {
            let through: f32 = (0..4).map(|k| row[k] * first[k][j]).sum();
            out[i][j] = through + if j == 4 { row[4] } else { 0.0 };
        }
    }
    out
}
