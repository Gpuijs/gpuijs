//! Inline `<svg>` subtrees -> SVG markup, which gpui rasterizes like an image file.

use std::collections::HashMap;
use std::fmt::Write;
use std::rc::Rc;

use gpui::{Hsla, Rgba};
use lightningcss::traits::Parse;
use lightningcss::values::color::CssColor;

use crate::css::cascade::substitute;
use crate::css::props::Paint;
use crate::tree::Tree;

/// What CSS says about one SVG element: its `color` (what `currentColor` means there) and any
/// `fill`/`stroke`, which win over the element's own attributes.
pub struct Paints {
    pub color: Hsla,
    pub fill: Option<Paint>,
    pub stroke: Option<Paint>,
    /// Custom properties in scope, for `var()`s in attributes (`stop-color="var(--color-desktop)"`).
    pub vars: Rc<HashMap<String, String>>,
    /// CSS `opacity`, which wins over the attribute (ApexCharts hides crosshairs with it).
    pub opacity: Option<f32>,
    /// Other presentation properties from CSS (`stroke-dasharray`...), which win over attributes.
    pub presentation: Vec<(&'static str, String)>,
    /// `display: none` or `visibility: hidden`: the element isn't drawn.
    pub hidden: bool,
}

/// A CSS transform of the `<svg>` box itself: gpui can't transform boxes, so it is drawn into
/// the image, about the box's center in CSS px as in CSS. The box's background goes along
/// (shadcn's tooltip arrow is a square background turned into a diamond).
pub struct BoxTransform {
    pub rotate: f32,
    pub scale: (f32, f32),
    /// Background color and corner radius in CSS px.
    pub background: Option<(Hsla, f32)>,
}

impl BoxTransform {
    /// How far (x, y) the turned box reaches past each side of its own `(w, h)` box, in CSS px;
    /// the image is that much larger so the corners aren't cut off.
    pub fn overflow(&self, (w, h): (f32, f32)) -> (f32, f32) {
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let (sw, sh) = (w * self.scale.0.abs(), h * self.scale.1.abs());
        let half_w = (cos.abs() * sw + sin.abs() * sh) / 2.0;
        let half_h = (sin.abs() * sw + cos.abs() * sh) / 2.0;
        ((half_w - w / 2.0).max(0.0), (half_h - h / 2.0).max(0.0))
    }
}

/// The markup for the `<svg>` at `id`, `css_size` CSS px drawn at `size` device px, with text
/// defaulting to `font`.
pub fn markup(
    tree: &Tree,
    id: u64,
    paints_of: &dyn Fn(u64) -> Option<Paints>,
    (size, css_size): ((f32, f32), (f32, f32)),
    font: Option<&str>,
    transform: Option<&BoxTransform>,
) -> String {
    let mut out = String::new();
    let Some(t) = transform else {
        let root = Root {
            size,
            css_size,
            font,
        };
        write_node(tree, id, Some(&root), paints_of, &mut out);
        return out;
    };
    // An outer image in CSS px holds the turned box, with room for its corners; the element's
    // own svg sits inside it.
    let (w, h) = css_size;
    let (mx, my) = t.overflow(css_size);
    let k = (size.0 / w.max(f32::EPSILON), size.1 / h.max(f32::EPSILON));
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}"><g transform="translate({} {}) rotate({}) scale({} {}) translate({} {})">"#,
        (w + 2.0 * mx) * k.0,
        (h + 2.0 * my) * k.1,
        -mx,
        -my,
        w + 2.0 * mx,
        h + 2.0 * my,
        w / 2.0,
        h / 2.0,
        t.rotate,
        t.scale.0,
        t.scale.1,
        -w / 2.0,
        -h / 2.0,
    );
    if let Some((color, radius)) = t.background {
        let _ = write!(
            out,
            r#"<rect width="{w}" height="{h}" rx="{radius}" fill="{}"/>"#,
            css_rgba(color)
        );
    }
    write_node(
        tree,
        id,
        Some(&Root {
            size: css_size,
            css_size,
            font,
        }),
        paints_of,
        &mut out,
    );
    out.push_str("</g></svg>");
    out
}

/// A big `<svg>`'s markup in pieces, for drawing it in stacked layers that are cached apart: a
/// change to one part (a chart's hover highlight) redraws that layer alone.
pub struct Parts {
    /// The root's opening tag.
    pub open: String,
    /// Each top-level child: its id, markup, and the definitions (gradients, clip paths...) inside
    /// it, which every layer needs, since anything may refer to them.
    pub children: Vec<(u64, String, String)>,
}

impl Parts {
    /// The markup drawing `range` of the children over a transparent background.
    pub fn layer(&self, range: std::ops::Range<usize>) -> String {
        let mut out = self.open.clone();
        out.push_str("<defs>");
        for (i, (_, _, defs)) in self.children.iter().enumerate() {
            if !range.contains(&i) {
                out.push_str(defs);
            }
        }
        out.push_str("</defs>");
        for (_, markup, _) in &self.children[range] {
            out.push_str(markup);
        }
        out.push_str("</svg>");
        out
    }
}

/// `markup` in parts, or `None` when drawing parts apart could look different: `<use>` can draw
/// another part, and the root's own opacity, filter, mask or clip would apply to each layer.
pub fn parts(
    tree: &Tree,
    id: u64,
    paints_of: &dyn Fn(u64) -> Option<Paints>,
    (size, css_size): ((f32, f32), (f32, f32)),
    font: Option<&str>,
) -> Option<Parts> {
    let node = tree.get(id);
    let effects = ["opacity", "filter", "mask", "clip-path"];
    if node.props.attrs.keys().any(|k| effects.contains(&k.as_str()))
        || paints_of(id).is_some_and(|p| p.hidden || p.opacity.is_some_and(|o| o < 1.0))
        || subtree_has_use(tree, id)
    {
        return None;
    }
    let root = Root {
        size,
        css_size,
        font,
    };
    let mut open = String::new();
    write_open(tree, id, Some(&root), paints_of, &mut open)?;
    let children = node
        .children
        .iter()
        .map(|&child| {
            let (mut markup, mut defs) = (String::new(), String::new());
            write_node(tree, child, None, paints_of, &mut markup);
            write_defs(tree, child, paints_of, &mut defs);
            (child, markup, defs)
        })
        .collect();
    Some(Parts { open, children })
}

fn subtree_has_use(tree: &Tree, id: u64) -> bool {
    let node = tree.get(id);
    node.tag == "use" || node.children.iter().any(|&c| subtree_has_use(tree, c))
}

/// The definitions at or under `id`, whole.
fn write_defs(tree: &Tree, id: u64, paints_of: &dyn Fn(u64) -> Option<Paints>, out: &mut String) {
    let node = tree.get(id);
    let definition = matches!(
        node.tag.as_str(),
        "defs" | "clippath" | "mask" | "lineargradient" | "radialgradient" | "pattern" | "filter"
            | "marker" | "symbol"
    );
    match definition {
        true => write_node(tree, id, None, paints_of, out),
        false => {
            for &child in &node.children {
                write_defs(tree, child, paints_of, out);
            }
        }
    }
}

/// What only the element's own `<svg>` gets.
struct Root<'a> {
    size: (f32, f32),
    /// The box in CSS px: the user space of an svg without a `viewBox` (ApexCharts draws in px).
    css_size: (f32, f32),
    font: Option<&'a str>,
}

fn css_rgba(color: Hsla) -> String {
    let c = Rgba::from(color);
    let byte = |v: f32| (v * 255.0).round();
    format!("rgba({},{},{},{})", byte(c.r), byte(c.g), byte(c.b), c.a)
}

/// Width and height from the element's attributes, falling back to its viewBox like a browser does.
pub fn intrinsic_size(tree: &Tree, id: u64) -> Option<(f32, f32)> {
    let attrs = &tree.get(id).props.attrs;
    let num = |k: &str| attrs.get(k)?.trim_end_matches("px").parse::<f32>().ok();
    let view_box: Option<Vec<f32>> = attrs
        .get("viewBox")
        .map(|v| v.split([' ', ',']).filter_map(|n| n.parse().ok()).collect());
    let vb = view_box.filter(|v| v.len() == 4).map(|v| (v[2], v[3]));
    match (num("width"), num("height"), vb) {
        (Some(w), Some(h), _) => Some((w, h)),
        (Some(w), None, Some((vw, vh))) if vw > 0.0 => Some((w, w * vh / vw)),
        (None, Some(h), Some((vw, vh))) if vh > 0.0 => Some((h * vw / vh, h)),
        (None, None, vb) => vb,
        _ => None,
    }
}

fn write_node(
    tree: &Tree,
    id: u64,
    root: Option<&Root>,
    paints_of: &dyn Fn(u64) -> Option<Paints>,
    out: &mut String,
) {
    let node = tree.get(id);
    if let Some(text) = &node.text {
        escape(text, out);
        return;
    }
    let Some(tag) = write_open(tree, id, root, paints_of, out) else {
        return;
    };
    for &child in &node.children {
        write_node(tree, child, None, paints_of, out);
    }
    let _ = write!(out, "</{tag}>");
}

/// An element's opening tag, and its tag name to close it with; nothing for a hidden one.
fn write_open<'t>(
    tree: &'t Tree,
    id: u64,
    root: Option<&Root>,
    paints_of: &dyn Fn(u64) -> Option<Paints>,
    out: &mut String,
) -> Option<&'t str> {
    let node = tree.get(id);
    let paints = paints_of(id);
    if paints.as_ref().is_some_and(|p| p.hidden) {
        return None;
    }
    let color = paints.as_ref().map(|p| css_rgba(p.color));
    let mut attrs: Vec<(&str, String)> = node
        .props
        .attrs
        .iter()
        .filter(|(k, _)| {
            let own_size = root.is_some() && matches!(k.as_str(), "width" | "height" | "xmlns");
            !(own_size || k.starts_with("aria-") || k.starts_with("data-") || k.as_str() == "role")
        })
        // An empty presentation attribute is invalid, so browsers ignore it (`stroke=""` inherits).
        .filter(|(_, v)| !v.trim().is_empty())
        // So do Chromium and Safari a CSS filter function there (MUI X Charts' area paths say
        // `filter="brightness(120%)"`); only a reference to an SVG filter applies.
        .filter(|(k, v)| k.as_str() != "filter" || v.trim_start().starts_with("url("))
        .map(|(k, v)| {
            let v = match (&paints, &color) {
                (_, Some(c)) if v.eq_ignore_ascii_case("currentColor") => c.clone(),
                (Some(p), _) => attr_value(k, v, &|n: &str| p.vars.get(n).map(String::as_str)),
                _ => v.clone(),
            };
            (k.as_str(), v)
        })
        .collect();
    let paint = |p: Paint| match p {
        Paint::None => "none".to_owned(),
        Paint::Color(c) => css_rgba(c),
        Paint::Url(url) => format!("url({url})"),
    };
    for (name, css) in [
        ("fill", paints.as_ref().and_then(|p| p.fill.clone())),
        ("stroke", paints.as_ref().and_then(|p| p.stroke.clone())),
    ] {
        if let Some(css) = css {
            attrs.retain(|(k, _)| *k != name);
            attrs.push((name, paint(css)));
        }
    }
    if let Some(opacity) = paints.as_ref().and_then(|p| p.opacity) {
        attrs.retain(|(k, _)| *k != "opacity");
        attrs.push(("opacity", opacity.to_string()));
    }
    for (name, value) in paints.iter().flat_map(|p| &p.presentation) {
        attrs.retain(|(k, _)| k != name);
        attrs.push((name, value.clone()));
    }
    attrs.sort(); // Stable markup, so the rasterized image is cached across renders.
    let tag = svg_tag(&node.tag);
    let _ = write!(out, "<{tag}");
    if let Some(root) = root {
        out.push_str(r#" xmlns="http://www.w3.org/2000/svg""#);
        let _ = write!(out, r#" width="{}" height="{}""#, root.size.0, root.size.1);
        if !node.props.attrs.contains_key("viewBox") {
            let (w, h) = root.css_size;
            let _ = write!(out, r#" viewBox="0 0 {w} {h}""#);
        }
        if let Some(font) = root
            .font
            .filter(|_| !node.props.attrs.contains_key("font-family"))
        {
            out.push_str(r#" font-family=""#);
            // gpui's name for the platform UI font; resvg draws nothing with that variable font file.
            escape(
                if font == ".SystemUIFont" {
                    "sans-serif"
                } else {
                    font
                },
                out,
            );
            out.push('"');
        }
    }
    for (k, v) in attrs {
        let _ = write!(out, r#" {k}=""#);
        escape(&v, out);
        out.push('"');
    }
    out.push('>');
    Some(tag)
}

/// The tree keeps tags lowercase, as HTML does; SVG's mixed-case names need their case back.
fn svg_tag(tag: &str) -> &str {
    match tag {
        "clippath" => "clipPath",
        "lineargradient" => "linearGradient",
        "radialgradient" => "radialGradient",
        "foreignobject" => "foreignObject",
        "textpath" => "textPath",
        "animatemotion" => "animateMotion",
        "animatetransform" => "animateTransform",
        // resvg skips unknown primitives, and an element whose filter has none isn't drawn at all.
        "feblend" => "feBlend",
        "fecolormatrix" => "feColorMatrix",
        "fecomponenttransfer" => "feComponentTransfer",
        "fecomposite" => "feComposite",
        "feconvolvematrix" => "feConvolveMatrix",
        "fediffuselighting" => "feDiffuseLighting",
        "fedisplacementmap" => "feDisplacementMap",
        "fedistantlight" => "feDistantLight",
        "fedropshadow" => "feDropShadow",
        "feflood" => "feFlood",
        "fefunca" => "feFuncA",
        "fefuncb" => "feFuncB",
        "fefuncg" => "feFuncG",
        "fefuncr" => "feFuncR",
        "fegaussianblur" => "feGaussianBlur",
        "feimage" => "feImage",
        "femerge" => "feMerge",
        "femergenode" => "feMergeNode",
        "femorphology" => "feMorphology",
        "feoffset" => "feOffset",
        "fepointlight" => "fePointLight",
        "fespecularlighting" => "feSpecularLighting",
        "fespotlight" => "feSpotLight",
        "fetile" => "feTile",
        "feturbulence" => "feTurbulence",
        _ => tag,
    }
}

/// An attribute value as a browser reads it: `var()`s substituted, and colors in a form resvg
/// parses (it has no `oklch()`, which shadcn's themes use).
fn attr_value<'a>(name: &str, value: &str, lookup: &dyn Fn(&str) -> Option<&'a str>) -> String {
    let value = match value.contains("var(") {
        true => substitute(value, lookup, 0).unwrap_or_default(),
        false => value.to_owned(),
    };
    if !matches!(
        name,
        "fill" | "stroke" | "stop-color" | "flood-color" | "lighting-color" | "color"
    ) {
        return value;
    }
    match CssColor::parse_string(value.trim())
        .ok()
        .and_then(|c| c.to_rgb().ok())
    {
        Some(CssColor::RGBA(c)) => {
            format!("rgba({},{},{},{})", c.red, c.green, c.blue, c.alpha_f32())
        }
        _ => value, // url(#gradient), none, currentColor
    }
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn turned_boxes_reach_past_their_sides() {
        let t = |rotate, scale| BoxTransform {
            rotate,
            scale,
            background: None,
        };
        let (mx, my) = t(45.0, (1.0, 1.0)).overflow((10.0, 10.0));
        assert!((mx - (50f32.sqrt() - 5.0)).abs() < 1e-4 && (my - mx).abs() < 1e-4);
        assert_eq!(t(90.0, (1.0, 1.0)).overflow((20.0, 10.0)).1, 5.0);
        assert_eq!(t(0.0, (0.5, 0.5)).overflow((10.0, 10.0)), (0.0, 0.0));
    }

    #[test]
    fn mixed_case_tags_and_var_colors_reach_resvg() {
        assert_eq!(svg_tag("lineargradient"), "linearGradient");
        assert_eq!(svg_tag("fecolormatrix"), "feColorMatrix");
        let vars = HashMap::from([
            ("--color-desktop", "var(--primary)"),
            ("--primary", "oklch(0% 0 0)"),
        ]);
        let lookup = |n: &str| vars.get(n).copied();
        assert_eq!(
            attr_value("stop-color", "var(--color-desktop)", &lookup),
            "rgba(0,0,0,1)"
        );
        assert_eq!(
            attr_value("fill", "url(#fillDesktop)", &lookup),
            "url(#fillDesktop)"
        );
        assert_eq!(attr_value("d", "M0 0", &lookup), "M0 0");
    }

    use super::*;
    use crate::protocol::{Op, Props};
    use crate::tree::BODY;

    #[test]
    fn layers_carry_every_parts_definitions() {
        let mut tree = Tree::default();
        let el = |id, tag: &str, attrs: &[(&str, &str)]| {
            let attrs = attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            Op::El(id, tag.into(), Props { attrs, ..Default::default() })
        };
        for op in [
            el(2, "svg", &[("viewBox", "0 0 10 10")]),
            // A series with its clip path inside, and a highlight that uses it.
            el(3, "g", &[]),
            el(4, "clippath", &[("id", "c")]),
            el(5, "rect", &[("width", "5")]),
            el(6, "path", &[("d", "M0 0")]),
            el(7, "path", &[("d", "M1 1"), ("clip-path", "url(#c)")]),
            Op::Append(BODY, 2),
            Op::Append(2, 3),
            Op::Append(3, 4),
            Op::Append(4, 5),
            Op::Append(3, 6),
            Op::Append(2, 7),
        ] {
            tree.apply(op);
        }
        let plain = |_| None;
        let parts = parts(&tree, 2, &plain, ((20.0, 20.0), (10.0, 10.0)), None).unwrap();
        assert_eq!(parts.children.len(), 2);
        let open = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 10 10">"#;
        assert_eq!(
            parts.layer(1..2),
            format!(
                r#"{open}<defs><clipPath id="c"><rect width="5"></rect></clipPath></defs><path clip-path="url(#c)" d="M1 1"></path></svg>"#
            )
        );
        // The whole drawing in one layer is the plain markup, plus an empty <defs>.
        assert_eq!(
            parts.layer(0..2).replace("<defs></defs>", ""),
            markup(&tree, 2, &plain, ((20.0, 20.0), (10.0, 10.0)), None, None)
        );
        // `<use>` can draw another part: one image.
        tree.apply(el(8, "use", &[("href", "#c")]));
        tree.apply(Op::Append(2, 8));
        assert!(super::parts(&tree, 2, &plain, ((20.0, 20.0), (10.0, 10.0)), None).is_none());
    }

    #[test]
    fn serializes_subtree_with_size_and_current_color() {
        let mut tree = Tree::default();
        let el = |id, tag: &str, attrs: &[(&str, &str)]| {
            let attrs = attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            Op::El(
                id,
                tag.into(),
                Props {
                    attrs,
                    ..Default::default()
                },
            )
        };
        for op in [
            el(
                2,
                "svg",
                &[
                    ("viewBox", "0 0 24 24"),
                    ("stroke", "currentColor"),
                    ("aria-hidden", "true"),
                ],
            ),
            el(3, "path", &[("d", "M1 1L2 2"), ("stroke", ""), ("filter", "brightness(120%)")]),
            Op::Txt(4, "a<b".into()),
            el(5, "text", &[]),
            Op::Append(BODY, 2),
            Op::Append(2, 3),
            Op::Append(2, 5),
            Op::Append(5, 4),
        ] {
            tree.apply(op);
        }
        assert_eq!(intrinsic_size(&tree, 2), Some((24.0, 24.0)));
        let red = Rgba {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }
        .into();
        assert_eq!(
            markup(
                &tree,
                2,
                &|_| Some(Paints {
                    color: red,
                    fill: None,
                    stroke: None,
                    vars: Rc::default(),
                    opacity: None,
                    presentation: Vec::new(),
                    hidden: false,
                }),
                ((32.0, 32.0), (16.0, 16.0)),
                Some("Inter"),
                None
            ),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" font-family="Inter" stroke="rgba(255,0,0,1)" viewBox="0 0 24 24"><path d="M1 1L2 2"></path><text>a&lt;b</text></svg>"#
        );
        // CSS fill and opacity win over the attributes, hidden elements are left out, and a box
        // transform wraps the svg, background included.
        let blue = Rgba {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        }
        .into();
        let css = |n: u64| {
            Some(Paints {
                color: red,
                fill: (n == 3).then_some(Paint::Color(blue)),
                stroke: (n == 2).then_some(Paint::None),
                vars: Rc::default(),
                opacity: (n == 3).then_some(0.5),
                presentation: Vec::new(),
                hidden: n == 5,
            })
        };
        let turn = BoxTransform {
            rotate: 45.0,
            scale: (1.0, 1.0),
            background: Some((blue, 2.0)),
        };
        assert_eq!(
            markup(
                &tree,
                2,
                &css,
                ((20.0, 20.0), (10.0, 10.0)),
                None,
                Some(&turn)
            ),
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="28.284271" height="28.284271" viewBox="-2.0710678 -2.0710678 14.142136 14.142136">"#,
                r#"<g transform="translate(5 5) rotate(45) scale(1 1) translate(-5 -5)"><rect width="10" height="10" rx="2" fill="rgba(0,0,255,1)"/>"#,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" stroke="none" viewBox="0 0 24 24"><path d="M1 1L2 2" fill="rgba(0,0,255,1)" opacity="0.5"></path></svg></g></svg>"#
            )
        );
    }
}
