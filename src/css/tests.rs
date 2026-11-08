use std::collections::HashMap;

use gpui::{AbsoluteLength, CursorStyle, DefiniteLength, Display, Hsla, Length, Rgba, px};

use super::cascade::{Changes, Computed, StyleCache, compute, restyle};
use super::props::{AppRegion, Disp, Paint};
use super::{Env, Stylesheet};
use crate::protocol::{Op, Props};
use crate::transition::Look;
use crate::tree::{BODY, IdMap, Tree};
use gpui::StyleRefinement;

const WIDE: Env = Env {
    width: 1024.0,
    height: 768.0,
    dark: false,
};

/// Lays out `<body>` > div#2 > div#3 with the given classes and inline style on #3.
fn styles(
    css: &str,
    outer: &str,
    inner: &str,
    inner_style: Option<&str>,
    env: Env,
) -> IdMap<std::rc::Rc<Computed>> {
    let sheet = Stylesheet::parse(css).unwrap();
    compute(&tree(outer, inner, inner_style), &sheet, &env, &|_| Some(0.5))
}

fn tree(outer: &str, inner: &str, inner_style: Option<&str>) -> Tree {
    let mut tree = Tree::default();
    let el = |id, class: &str, style: Option<&str>| {
        Op::El(
            id,
            "div".into(),
            Props {
                class: class.into(),
                style: style.map(Into::into),
                ..Default::default()
            },
        )
    };
    for op in [
        el(2, outer, None),
        el(3, inner, inner_style),
        Op::Append(BODY, 2),
        Op::Append(2, 3),
    ] {
        tree.apply(op);
    }
    tree
}

fn inner(css: &str, class: &str) -> Computed {
    std::rc::Rc::unwrap_or_clone(styles(css, "", class, None, WIDE).remove(&3).unwrap())
}

/// Nodes that take a twin's style get exactly what styling them alone gives.
#[test]
fn shared_styles_match_styling_each_node() {
    let css = "
        .list .row { padding-top: 1px } .other .row { padding-top: 2px }
        .row.odd { margin-top: 3px } .row span { padding-left: 4px } .other span { padding-left: 5px }
        .group:hover .g { opacity: .5 } [data-x='1'] { margin-left: 6px } input:disabled { margin-right: 1px }
        .c { container-type: inline-size } @container (min-width: 1px) { .q { padding-right: 7px } }
        span:empty { margin-bottom: 2px }
    ";
    let sheet = Stylesheet::parse(css).unwrap();
    let mut alone = Stylesheet::parse(css).unwrap();
    alone.reads_siblings = true; // which turns sharing off
    let mut tree = Tree::default();
    let mut next = 2;
    let mut add = |tree: &mut Tree, parent: u64, tag: &str, class: &str, attrs: &[(&str, &str)]| {
        let id = next;
        next += 1;
        let props = Props {
            class: class.into(),
            attrs: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            disabled: attrs.iter().any(|(k, _)| *k == "disabled"),
            ..Default::default()
        };
        tree.apply(Op::El(id, tag.into(), props));
        tree.apply(Op::Append(parent, id));
        id
    };
    for list in ["list", "other", "c"] {
        let l = add(&mut tree, BODY, "div", list, &[]);
        for i in 0..6 {
            let class = match i % 3 {
                0 => "row",
                1 => "row odd",
                _ => "row group q",
            };
            let attrs: &[(&str, &str)] = if i == 4 { &[("data-x", "1")] } else { &[] };
            let row = add(&mut tree, l, "div", class, attrs);
            let span = add(&mut tree, row, "span", "g", &[]);
            if i != 5 {
                tree.apply(Op::Txt(1000 + span, "t".into()));
                tree.apply(Op::Append(span, 1000 + span));
            }
            add(&mut tree, row, "input", "", if i == 3 { &[("disabled", "")] } else { &[] });
        }
    }
    let shared = compute(&tree, &sheet, &WIDE, &|_| Some(0.5));
    assert_eq!(shared, compute(&tree, &alone, &WIDE, &|_| Some(0.5)));
}

#[test]
fn paint_only_differences_lay_out_the_same() {
    use super::cascade::same_layout;
    let css = ".a { color: red; background: blue; opacity: .5; box-shadow: 0 0 2px black; padding: 4px }
        .b { color: green; background: white; border-radius: 4px; padding: 4px }
        .c { color: red; padding: 5px } .d { color: red; padding: 4px; font-weight: 700 }";
    let (a, b) = (inner(css, "a"), inner(css, "b"));
    assert!(same_layout(&a, &b));
    assert!(!same_layout(&a, &inner(css, "c")));
    assert!(!same_layout(&a, &inner(css, "d")));
}

fn pxd(v: f32) -> Option<DefiniteLength> {
    Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(v))))
}

fn pxl(v: f32) -> Option<Length> {
    Some(Length::Definite(DefiniteLength::Absolute(
        AbsoluteLength::Pixels(px(v)),
    )))
}

fn rgb(r: f32, g: f32, b: f32) -> Option<Hsla> {
    Some(Rgba { r, g, b, a: 1.0 }.into())
}

fn text_color(c: &Computed) -> Option<Hsla> {
    c.base.text.as_ref()?.color
}

#[test]
fn specificity_then_source_order() {
    let c = inner(
        ".a { color: red } div.a { color: blue } .b { color: lime }",
        "a b",
    );
    assert_eq!(text_color(&c), rgb(0.0, 0.0, 1.0));
    let c = inner(".a { color: red } .b { color: lime }", "a b");
    assert_eq!(text_color(&c), rgb(0.0, 1.0, 0.0));
}

#[test]
fn layers_and_important() {
    let css = "
        @layer a, b;
        @layer b { .x { padding-left: 1px; padding-right: 2px !important } }
        @layer a { .x { padding-left: 2px; padding-right: 1px !important } }
        @layer a { .x { padding-top: 10px } }
        .x { padding-top: 5px }
    ";
    let c = inner(css, "x");
    assert_eq!(c.base.padding.left, pxd(1.0), "later layer wins");
    assert_eq!(
        c.base.padding.right,
        pxd(1.0),
        "!important: earlier layer wins"
    );
    assert_eq!(c.base.padding.top, pxd(5.0), "unlayered beats layers");
}

#[test]
fn injected_style_follows_the_app_sheet() {
    let app = Stylesheet::parse(".x { cursor: pointer }").unwrap();
    // react-resizable-panels while dragging: every element shows the resize cursor.
    let dragging = app
        .with_injected("*{cursor: ew-resize !important;}")
        .unwrap();
    let cursor = |sheet: &Stylesheet| {
        let mut c = compute(&tree("", "x", None), sheet, &WIDE, &|_| Some(0.5));
        (
            c.remove(&2).unwrap().base.mouse_cursor,
            c.remove(&3).unwrap().base.mouse_cursor,
        )
    };
    let resize = Some(CursorStyle::ResizeLeftRight);
    assert_eq!(cursor(&dragging), (resize, resize));
    assert_eq!(
        cursor(&app),
        (None, Some(CursorStyle::PointingHand)),
        "the app sheet is untouched"
    );
}

#[test]
fn inline_style_vs_important() {
    let c = styles(
        ".x { padding-top: 1px; padding-left: 9px !important }",
        "",
        "x",
        Some("padding-top: 2px; padding-left: 3px"),
        WIDE,
    )
    .remove(&3)
    .unwrap();
    assert_eq!(c.base.padding.top, pxd(2.0));
    assert_eq!(c.base.padding.left, pxd(9.0));
}

#[test]
fn vars_inherit_fallback_and_calc() {
    let css = ":root { --s: 0.25rem } .p { padding-bottom: calc(var(--s) * 4); margin-top: var(--missing, 3px) }";
    let c = inner(css, "p");
    assert_eq!(c.base.padding.bottom, pxd(16.0));
    assert_eq!(c.base.margin.top, pxl(3.0));
}

#[test]
fn calc_mixing_percentages_and_lengths_keeps_both() {
    // TailAdmin's language submenu: `md:inset-e-[calc(100%+14px)]` beside its menu.
    let css =
        ".p { position: absolute; inset-inline-end: calc(100% + 14px); width: calc(50% - 2rem) }";
    let c = inner(css, "p");
    let calc = |f, p| Some(Length::Definite(DefiniteLength::Calc(f, px(p))));
    assert_eq!(c.base.inset.right, calc(1.0, 14.0));
    assert_eq!(c.base.size.width, calc(0.5, -32.0));
}

#[test]
fn registered_non_inherited_property_uses_initial() {
    let css = r#"
        @property --t { syntax: "*"; inherits: false; initial-value: 7px }
        .parent { --t: 20px; padding-top: var(--t) }
        .child { padding-top: var(--t) }
    "#;
    let s = styles(css, "parent", "child", None, WIDE);
    assert_eq!(s[&2].base.padding.top, pxd(20.0));
    assert_eq!(s[&3].base.padding.top, pxd(7.0));
}

#[test]
fn current_color_and_em() {
    let css = ".outer { font-size: 10px; color: #f00 } .c { font-size: 2em; border-top: 1px solid currentColor; padding-left: 2em }";
    let s = styles(css, "outer", "c", None, WIDE);
    let c = &s[&3];
    assert_eq!(c.base.border_color, rgb(1.0, 0.0, 0.0));
    assert_eq!(c.base.padding.left, pxd(40.0));
    assert_eq!(
        c.base.border_widths.top,
        Some(AbsoluteLength::Pixels(px(1.0)))
    );
}

#[test]
fn border_width_needs_a_style() {
    let c = inner(".b { border-width: 2px }", "b");
    assert_eq!(
        c.base.border_widths.top,
        Some(AbsoluteLength::Pixels(px(0.0)))
    );
}

#[test]
fn media_queries_follow_the_window() {
    let css = "@media (width >= 48rem) { .m { display: flex } }";
    let wide = styles(css, "", "m", None, WIDE).remove(&3).unwrap();
    let narrow = styles(
        css,
        "",
        "m",
        None,
        Env {
            width: 500.0,
            ..WIDE
        },
    )
    .remove(&3)
    .unwrap();
    assert_eq!(wide.disp, Disp::Flex);
    assert_eq!(narrow.disp, Disp::Block);
    assert_eq!(narrow.base.display, Some(Display::Block));
}

#[test]
fn hover_rules_keep_svg_paints() {
    // TailAdmin's map regions: the highlight comes from CSS `:hover` over an `!important` fill.
    let css = ".r { fill: #d0d5dd !important } @media (hover: hover) { .r:hover { fill: #465fff !important } }";
    let c = inner(css, "r");
    let color = |p: Option<Paint>| match p {
        Some(Paint::Color(c)) => Some(Rgba::from(c)),
        _ => None,
    };
    assert!(close(
        color(c.fill).map(Hsla::from),
        rgb(208.0 / 255.0, 213.0 / 255.0, 221.0 / 255.0).unwrap()
    ));
    let (fill, _) = c.hover_paints.expect("hover paints");
    assert!(close(
        color(fill).map(Hsla::from),
        rgb(70.0 / 255.0, 95.0 / 255.0, 1.0).unwrap()
    ));
}

#[test]
fn hover_rules_become_state_styles() {
    let c = inner(".h { color: blue } .h:hover { background-color: red }", "h");
    assert!(c.base.background.is_none());
    let hover = c.hover.expect("hover style");
    assert!(hover.background.is_some());
    assert_eq!(
        hover.text.as_ref().unwrap().color,
        rgb(0.0, 0.0, 1.0),
        "hover style keeps base declarations"
    );
    assert!(c.focus.is_none());
}

#[test]
fn tailwind_group_hover_keys_to_the_group() {
    // Tailwind v4's output; the group rule wins over `md:opacity-0` on specificity.
    let css = r".group-hover\/item\:opacity-100 { &:is(:where(.group\/item):hover *) {
        @media (hover: hover) { opacity: 100% } } }
        .md\:opacity-0 { @media (width >= 48rem) { opacity: 0% } }";
    let c = styles(
        css,
        "group/item",
        "group-hover/item:opacity-100 md:opacity-0",
        None,
        WIDE,
    )
    .remove(&3)
    .map(std::rc::Rc::unwrap_or_clone)
    .unwrap();
    assert_eq!(c.base.opacity, Some(0.0));
    let (group, style) = c.group.expect("group hover style");
    assert_eq!((group, style.opacity), (2, Some(1.0)));
    assert_eq!(c.hover.expect("hover style").opacity, Some(1.0));
}

#[test]
fn hover_scale_becomes_a_state_style() {
    let c = inner(".t:hover { scale: 1.05 }", "t");
    assert_eq!(c.base.scale, None);
    assert_eq!(
        c.hover.expect("hover style").scale,
        Some(gpui::point(1.05, 1.05))
    );
}

#[test]
fn tailwind_hover_scale_in_a_fixed_translated_box() {
    let css = "@property --tw-scale-x { syntax: \"*\"; inherits: false; initial-value: 1 }
        @property --tw-scale-y { syntax: \"*\"; inherits: false; initial-value: 1 }
        .f { position: fixed; --tw-translate-x: -50%; translate: var(--tw-translate-x) var(--tw-translate-y); transition-property: all; transition-duration: 300ms; transition-delay: 100ms }
        @media (hover: hover) { .h:hover { --tw-scale-x: 105%; --tw-scale-y: 105%; scale: var(--tw-scale-x) var(--tw-scale-y) } }";
    let c = inner(css, "f h");
    assert_eq!(
        c.hover.expect("hover style").scale,
        Some(gpui::point(1.05, 1.05))
    );
}

#[test]
fn nesting_and_shorthands() {
    let css = ".n { padding: 1px 2px; .outer & { margin: auto } }";
    let c = styles(css, "outer", "n", None, WIDE).remove(&3).unwrap();
    assert_eq!(c.base.padding.top, pxd(1.0));
    assert_eq!(c.base.padding.right, pxd(2.0));
    assert_eq!(c.base.margin.left, Some(Length::Auto));
}

#[test]
fn substitute_handles_nesting_fallbacks_and_cycles() {
    use super::cascade::substitute;
    let vars: HashMap<&str, &str> = HashMap::from([
        ("--a", "1px"),
        ("--b", "var(--a)"),
        ("--loop", "var(--loop)"),
    ]);
    let lookup = |n: &str| vars.get(n).copied();
    assert_eq!(
        substitute("calc(var(--b) + var(--x, var(--a)))", &lookup, 0).as_deref(),
        Some("calc(1px + 1px)")
    );
    assert_eq!(substitute("var(--x,)", &lookup, 0).as_deref(), Some(""));
    assert_eq!(substitute("var(--x)", &lookup, 0), None);
    assert_eq!(substitute("var(--loop)", &lookup, 0), None);
}

/// Real Tailwind v4 output: `tailwindcss -i examples/shadcn-demo/src/app.css -o tests/fixtures/tailwind.css`.
const TAILWIND: &str = include_str!("../../tests/fixtures/tailwind.css");

const BUTTON: &str = "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] bg-primary text-primary-foreground hover:bg-primary/90 h-9 px-4 py-2";
const INPUT: &str = "dark:bg-input/30 border-input h-9 w-full min-w-0 rounded-md border bg-transparent px-3 py-1 text-base shadow-xs transition-[color,box-shadow] md:text-sm focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]";

fn close(a: Option<Hsla>, b: Hsla) -> bool {
    a.is_some_and(|a| {
        let (a, b) = (gpui::Rgba::from(a), gpui::Rgba::from(b));
        (a.r - b.r).abs() < 0.01
            && (a.g - b.g).abs() < 0.01
            && (a.b - b.b).abs() < 0.01
            && (a.a - b.a).abs() < 0.01
    })
}

fn fill_color(fill: &Option<gpui::Fill>) -> Option<Hsla> {
    match fill.as_ref()? {
        gpui::Fill::Color(bg) => bg.solid_color(),
    }
}

#[test]
fn tailwind_shadcn_button() {
    let c = inner(TAILWIND, BUTTON);
    assert_eq!(c.disp, Disp::InlineFlex);
    assert_eq!(c.base.size.height, pxl(36.0));
    assert_eq!(
        c.base.padding.left,
        pxd(16.0),
        "px-4 via padding-inline + calc(var(--spacing) * 4)"
    );
    assert_eq!(c.base.padding.top, pxd(8.0));
    assert_eq!(
        c.base.corner_radii.top_left,
        Some(AbsoluteLength::Pixels(px(8.0))),
        "--radius - 2px"
    );
    // oklch(0.205 0 0) is sRGB #171717.
    let primary = Rgba {
        r: 23.0 / 255.0,
        g: 23.0 / 255.0,
        b: 23.0 / 255.0,
        a: 1.0,
    }
    .into();
    assert!(
        close(fill_color(&c.base.background), primary),
        "{:?}",
        c.base.background
    );
    let text = c.base.text.as_ref().unwrap();
    assert_eq!(text.font_size, Some(AbsoluteLength::Pixels(px(14.0))));
    assert_eq!(text.font_weight, Some(gpui::FontWeight(500.0)));
    let hover = c.hover.expect("hover:bg-primary/90");
    let faded = Hsla { a: 0.9, ..primary };
    assert!(
        close(fill_color(&hover.background), faded),
        "{:?}",
        hover.background
    );
}

#[test]
fn tailwind_shadcn_input_media_dark_and_shadow() {
    let wide = inner(TAILWIND, INPUT);
    let narrow = styles(
        TAILWIND,
        "",
        INPUT,
        None,
        Env {
            width: 500.0,
            ..WIDE
        },
    )
    .remove(&3)
    .unwrap();
    let size = |c: &Computed| c.base.text.as_ref().and_then(|t| t.font_size);
    assert_eq!(
        size(&wide),
        Some(AbsoluteLength::Pixels(px(14.0))),
        "md:text-sm at 1024px"
    );
    assert_eq!(
        size(&narrow),
        Some(AbsoluteLength::Pixels(px(16.0))),
        "text-base below md"
    );
    assert_eq!(
        wide.base.border_widths.top,
        Some(AbsoluteLength::Pixels(px(1.0)))
    );

    let visible = |c: &gpui::StyleRefinement| -> Vec<gpui::BoxShadow> {
        let all = c.box_shadow.clone().unwrap_or_default();
        assert_eq!(
            all.len(),
            5,
            "Tailwind's five shadow slots are kept so states line up for transitions"
        );
        all.into_iter().filter(|s| s.color.a > 0.0).collect()
    };
    let shadows = visible(&wide.base);
    assert_eq!(shadows.len(), 1, "{shadows:?}");
    assert!((shadows[0].color.a - 0.05).abs() < 0.01);

    // focus-visible:ring-[3px] ring-ring/50: a 3px spread ring at half opacity, plus the shadow.
    let focused = visible(wide.focus_visible.as_ref().expect("focus-visible ring"));
    let ring = focused
        .iter()
        .find(|s| s.spread_radius == px(3.0))
        .expect("ring");
    assert!((ring.color.a - 0.5).abs() < 0.01, "{ring:?}");
    assert_eq!(focused.len(), 2);

    // transition-[color,box-shadow] with Tailwind's default 150ms ease.
    let t = wide.transition.as_ref().expect("transition");
    assert_eq!(t.properties, ["color", "box-shadow"]);
    assert!((t.duration - 0.15).abs() < 1e-4);
    assert_eq!(
        t.easing,
        crate::transition::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)
    );
    // bg-transparent + shadow-xs: filled with the white page behind it so gpui's shadow stays outside.
    assert_eq!(fill_color(&wide.base.background), Some(white_hsla()));
}

fn white_hsla() -> Hsla {
    Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    }
    .into()
}

#[test]
fn tailwind_dark_variant_follows_root_class() {
    let mut tree = Tree::default();
    tree.apply(Op::Set(
        crate::tree::HTML,
        Props {
            class: "dark".into(),
            ..Default::default()
        },
    ));
    tree.apply(Op::El(
        2,
        "input".into(),
        Props {
            class: INPUT.into(),
            ..Default::default()
        },
    ));
    tree.apply(Op::Append(BODY, 2));
    let sheet = Stylesheet::parse(TAILWIND).unwrap();
    let s = compute(&tree, &sheet, &WIDE, &|_| Some(0.5));
    // .dark --input is oklch(1 0 0 / 15%); dark:bg-input/30 makes it 4.5% white, composited over the
    // #0a0a0a page because the input has a shadow.
    let bg = fill_color(&s[&2].base.background).expect("dark background");
    assert!(bg.a == 1.0 && (bg.l - 0.081).abs() < 0.005, "{bg:?}");
    // body picks up bg-background from the .dark variables.
    let body = fill_color(&s[&BODY].base.background).unwrap();
    assert!(
        close(
            Some(body),
            Rgba {
                r: 10.0 / 255.0,
                g: 10.0 / 255.0,
                b: 10.0 / 255.0,
                a: 1.0
            }
            .into()
        ),
        "{body:?}"
    );
}

#[test]
fn selectors_the_matcher_cannot_handle_are_skipped() {
    let c = inner(
        ".x:nth-child(1 of .x) { color: red } .p:has(.x) .x { color: red } .x { color: blue }",
        "x",
    );
    assert_eq!(text_color(&c), rgb(0.0, 0.0, 1.0));
}

#[test]
fn browser_system_font_names_map_to_the_ui_font() {
    let c = inner(
        ".f { font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif }",
        "f",
    );
    assert_eq!(
        c.base.text.unwrap().font_family.map(|f| f.to_string()),
        Some(".SystemUIFont".to_string())
    );
}

#[test]
fn grid_track_lists_are_mapped() {
    use gpui::GridTrack as T;
    let c = inner(
        ".g { display: grid; grid-template-rows: auto auto; grid-template-columns: 1fr auto repeat(2, minmax(0, 1fr)) 10px; grid-auto-rows: min-content }",
        "g",
    );
    assert_eq!(c.base.grid_template_rows, Some(vec![T::Auto, T::Auto]));
    let zero = AbsoluteLength::Pixels(px(0.0));
    assert_eq!(
        c.base.grid_template_columns,
        Some(vec![
            T::Fr(1.0),
            T::Auto,
            T::Repeat(
                2,
                vec![T::MinMax(Box::new(T::Length(zero)), Box::new(T::Fr(1.0)))]
            ),
            T::Length(AbsoluteLength::Pixels(px(10.0))),
        ])
    );
    assert_eq!(c.base.grid_auto_rows, Some(vec![T::MinContent]));
}

#[test]
fn auto_repeated_tracks_are_mapped() {
    use gpui::GridTrack as T;
    let c = inner(".g { display: grid; grid-template-columns: repeat(auto-fill, minmax(30px, 1fr)) }", "g");
    let track = T::MinMax(Box::new(T::Length(AbsoluteLength::Pixels(px(30.0)))), Box::new(T::Fr(1.0)));
    assert_eq!(c.base.grid_template_columns, Some(vec![T::RepeatAuto(false, vec![track.clone()])]));
    let c = inner(".g { display: grid; grid-template-columns: repeat(auto-fit, minmax(30px, 1fr)) }", "g");
    assert_eq!(c.base.grid_template_columns, Some(vec![T::RepeatAuto(true, vec![track])]));
}

#[test]
fn see_through_boxes_with_shadows_are_filled_with_their_backdrop() {
    let css = ".bg { background-color: #f00 } .s { box-shadow: 0 1px 2px #0000000d; background-color: #0000ff80 }";
    let s = styles(css, "bg", "s", None, WIDE);
    let fill = fill_color(&s[&3].base.background).unwrap();
    // 50% blue over red, made opaque so gpui's under-the-box shadow stays hidden.
    assert!(
        close(
            Some(fill),
            Rgba {
                r: 0.5,
                g: 0.0,
                b: 0.5,
                a: 1.0
            }
            .into()
        ),
        "{fill:?}"
    );
}

#[test]
fn grid_areas_place_items_by_name() {
    use gpui::GridPlacement::{Auto, Line};
    let css = r#".g { display: grid; grid-template-areas: "toolbar toolbar" "chart legend" "chart ." }
        .chart { grid-area: chart } .nope { grid-area: nope }"#;
    let loc = |inner: &str| styles(css, "g", inner, None, WIDE)[&3].base.grid_location.clone().unwrap();
    let chart = loc("chart");
    assert_eq!((chart.row.start, chart.row.end), (Line(2), Line(4)));
    assert_eq!((chart.column.start, chart.column.end), (Line(1), Line(2)));
    let nope = loc("nope");
    assert_eq!((nope.row.start, nope.column.end), (Auto, Auto));
    // Outside a grid with areas the name places nothing.
    let outside = styles(css, "", "chart", None, WIDE)[&3].base.grid_location.clone().unwrap();
    assert_eq!(outside.row.start, Auto);
}

#[test]
fn has_matches_children_descendants_and_siblings() {
    let color_of = |css: &str| text_color(&styles(css, "p", "c", None, WIDE)[&2]);
    let red = rgb(1.0, 0.0, 0.0);
    assert_eq!(color_of(".p:has(> .c) { color: red }"), red);
    assert_eq!(
        color_of(".p:has(.nope, .c) { color: red }"),
        red,
        "any argument may match"
    );
    assert_eq!(color_of("div:has(*[class=c]) { color: red }"), red);
    assert_eq!(color_of(".p:has(.nope) { color: red }"), None);
    assert_eq!(
        color_of(".p:has(+ .c) { color: red }"),
        None,
        ".c is a child, not a sibling"
    );
    // Arguments with more combinators (MUI's `:has(> .select ~ .adornment)`) anchor at the subject.
    let body = |css: &str| text_color(&styles(css, "p", "c", None, WIDE)[&BODY]);
    assert_eq!(body("body:has(> .p > .c) { color: red }"), red);
    assert_eq!(body("body:has(.p .c) { color: red }"), red);
    assert_eq!(body("body:has(> .c > .p) { color: red }"), None);
    assert_eq!(color_of(".p:has(> .c ~ .nope) { color: red }"), None);
}

#[test]
fn tailwind_card_header_grid_with_action() {
    use gpui::GridTrack as T;
    let header = "@container/card-header grid auto-rows-min grid-rows-[auto_auto] items-start gap-2 px-6 has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-6";
    let action = "col-start-2 row-span-2 row-start-1 self-start justify-self-end";
    let mut tree = Tree::default();
    tree.apply(Op::El(
        2,
        "div".into(),
        Props {
            class: header.into(),
            ..Default::default()
        },
    ));
    let mut attrs = HashMap::new();
    attrs.insert("data-slot".to_string(), "card-action".to_string());
    tree.apply(Op::El(
        3,
        "div".into(),
        Props {
            class: action.into(),
            attrs,
            ..Default::default()
        },
    ));
    tree.apply(Op::Append(BODY, 2));
    tree.apply(Op::Append(2, 3));
    let s = compute(&tree, &Stylesheet::parse(TAILWIND).unwrap(), &WIDE, &|_| Some(0.5));
    assert_eq!(
        s[&2].base.grid_template_columns,
        Some(vec![T::Fr(1.0), T::Auto]),
        "has-data-[slot=card-action]"
    );
    assert_eq!(s[&2].base.grid_template_rows, Some(vec![T::Auto, T::Auto]));
    assert_eq!(s[&2].base.grid_auto_rows, Some(vec![T::MinContent]));
    assert_eq!(
        s[&3].base.justify_self,
        Some(gpui::AlignItems::FlexEnd),
        "Tailwind emits flex-end"
    );
}

#[test]
fn fit_content_width_stops_stretching() {
    let fit = ".fit { width: fit-content }";
    let in_grid = styles(
        &format!("{fit} .p {{ display: grid }}"),
        "p",
        "fit",
        None,
        WIDE,
    );
    assert_eq!(in_grid[&3].base.justify_self, Some(gpui::AlignItems::Start));
    let in_column = styles(
        &format!("{fit} .p {{ display: flex; flex-direction: column }}"),
        "p",
        "fit",
        None,
        WIDE,
    );
    assert_eq!(in_column[&3].base.align_self, Some(gpui::AlignItems::Start));
    let centered = styles(
        &format!("{fit} .p {{ display: flex; flex-direction: column; align-items: center }}"),
        "p",
        "fit",
        None,
        WIDE,
    );
    assert_eq!(centered[&3].base.align_self, None, "already not stretched");
    let in_flow = styles(fit, "p", "fit", None, WIDE);
    assert!(in_flow[&3].fit_block);
}

#[test]
fn gradient_parents_give_shadowed_children_their_midpoint_backdrop() {
    let css = ".p { background: linear-gradient(to right, #f00, #00f) } .s { box-shadow: 0 1px 2px #0000000d }";
    let s = styles(css, "p", "s", None, WIDE);
    let fill = fill_color(&s[&3].base.background).unwrap();
    assert!(
        close(
            Some(fill),
            Rgba {
                r: 0.5,
                g: 0.0,
                b: 0.5,
                a: 1.0
            }
            .into()
        ),
        "{fill:?}"
    );
}

#[test]
fn outlines_paint_as_a_ring() {
    let c = inner(".o { color: #f00; outline: 2px solid }", "o");
    let shadows = c.base.box_shadow.unwrap();
    assert_eq!(shadows.len(), 1);
    assert_eq!(shadows[0].spread_radius, px(2.0));
    assert_eq!(
        Some(shadows[0].color),
        rgb(1.0, 0.0, 0.0),
        "currentColor by default"
    );
    let none = inner(".o { outline-color: red; outline-width: 4px }", "o");
    assert!(
        none.base.box_shadow.is_none(),
        "outline-style defaults to none"
    );
}

#[test]
fn container_properties_are_accepted_and_container_rules_apply() {
    let c = inner(
        ".c { container-type: inline-size; padding-top: 1px } @container (width > 1px) { .c { padding-top: 9px } }",
        "c",
    );
    // A container's own rules query its ancestors, and it has none.
    assert_eq!(c.base.padding.top, pxd(1.0));
    // Size containment: content can't widen an inline-size container (shadcn's `@container/content` inset).
    assert_eq!(c.base.min_size.width, pxl(0.0));
}

#[test]
fn css_wide_keywords_cancel_earlier_declarations() {
    // Tailwind preflight: `a { color: inherit; text-decoration: inherit }` over the UA's blue underline.
    let css = ".ua { color: blue; text-decoration: underline } .reset { color: inherit; text-decoration: inherit }";
    let c = inner(css, "ua reset");
    assert_eq!(text_color(&c), None);
    assert!(c.base.text.as_ref().is_none_or(|t| t.underline.is_none()));
}

#[test]
fn container_queries_use_the_window_size_before_layout() {
    // Tailwind's `@container/content`, `@2xl/content:hidden` and `@max-3xl/content:hidden`.
    let css = ".content { container: content / inline-size } @container content (width >= 42rem) { .a { display: none } } @container content (width < 48rem) { .b { display: none } }";
    let disp = |class| {
        styles(css, "content", class, None, WIDE)
            .remove(&3)
            .unwrap()
            .disp
    };
    assert_eq!(disp("a"), Disp::None);
    assert_eq!(disp("b"), Disp::Block);
}

#[test]
fn translate_from_property_and_transform() {
    let c = inner(".t { translate: -50% 4px }", "t");
    assert_eq!(
        c.base.translate,
        Some(gpui::point(
            DefiniteLength::Fraction(-0.5),
            pxd(4.0).unwrap()
        ))
    );
    let c = inner(
        ".t { transform: translateY(2px) translateX(1rem) translateY(3px) }",
        "t",
    );
    assert_eq!(
        c.base.translate,
        Some(gpui::point(pxd(16.0).unwrap(), pxd(5.0).unwrap()))
    );
}

#[test]
fn translate_after_rotate_moves_along_the_rotated_axes() {
    // sonner's spinner bars: the % translate needs the box size, so the renderer resolves it.
    let bar = inner(".b { transform: rotate(30deg) translate(146%) }", "b");
    assert_eq!(bar.base.rotate, Some(30.0));
    assert_eq!(bar.base.translate, None);
    let after = bar.base.translate_after.expect("kept for the renderer");
    assert_eq!(
        (Some(after.x), Some(after.y)),
        (Some(DefiniteLength::Fraction(1.46)), pxd(0.0))
    );
    // In px it folds into the plain translate, rotated: 10px along a 90° turn points down.
    let px_after = inner(
        ".b { transform: translateX(2px) rotate(90deg) translateX(10px) }",
        "b",
    );
    let t = px_after.base.translate.expect("translate");
    assert_eq!(px_after.base.translate_after, None);
    let (
        DefiniteLength::Absolute(AbsoluteLength::Pixels(x)),
        DefiniteLength::Absolute(AbsoluteLength::Pixels(y)),
    ) = (t.x, t.y)
    else {
        panic!("px translate, got {t:?}");
    };
    assert!((f32::from(x) - 2.0).abs() < 1e-4 && (f32::from(y) - 10.0).abs() < 1e-4);
}

#[test]
fn keyframe_transform_composes_with_the_translate_property() {
    // tw-animate-css's `enter` keyframe as `animate-in zoom-in-95 fade-in-0` plays it, over a
    // dialog centered by Tailwind v4's `translate` property (not by `transform`).
    let c = inner(
        "
        @keyframes enter { from { opacity: 0; transform: translate3d(0,0,0) scale3d(.95,.95,.95) rotate(0) } }
        .d { translate: -50% -50%; animation: enter .15s ease both; --tw-enter-opacity: 0; --tw-enter-scale: .95 }
        ",
        "d",
    );
    let a = c.animation.as_ref().expect("animation resolved");
    let look = |elapsed: f32| {
        let mut s = StyleRefinement::default();
        a.sample(&Look::of(&c.base), elapsed)
            .expect("runs at once")
            .apply(&mut s);
        s
    };
    let (from, to) = (look(0.0), look(1.0));
    let centered = gpui::point(
        DefiniteLength::Fraction(-0.5),
        DefiniteLength::Fraction(-0.5),
    );
    assert_eq!(
        from.translate,
        Some(centered),
        "the keyframe's zero translate must not replace the element's centering"
    );
    assert_eq!(from.scale, Some(gpui::point(0.95, 0.95)));
    assert_eq!(from.opacity, Some(0.0));
    assert_eq!(to.translate, Some(centered));
}

#[test]
fn static_ignores_insets_and_one_clipped_axis_clips_both() {
    let c = inner(
        ".a { position: absolute; left: 100% } .s { position: static } .o { overflow-y: hidden }",
        "a s o",
    );
    assert_eq!(c.base.inset.left, None);
    assert_eq!(c.base.overflow.x, Some(gpui::Overflow::Scroll));
}

#[test]
fn small_and_dynamic_viewport_units_are_the_window() {
    let c = inner(
        ".s { min-height: 100svh; height: 50dvh; width: 10lvw }",
        "s",
    );
    assert_eq!(c.base.min_size.height, pxl(768.0));
    assert_eq!(c.base.size.height, pxl(384.0));
    assert_eq!(c.base.size.width, pxl(102.4));
}

#[test]
fn calc_of_only_percentages_is_a_percentage() {
    // Tailwind v4's `-translate-y-1/2`.
    let c = inner(
        ".t { --y: calc(calc(1 / 2 * 100%) * -1); translate: 0 var(--y) }",
        "t",
    );
    assert_eq!(
        c.base.translate.map(|t| t.y),
        Some(DefiniteLength::Fraction(-0.5))
    );
}

/// After any change, the incremental restyle must equal a restyle from scratch.
#[test]
fn incremental_restyle_matches_full_restyle() {
    use super::cascade::{Changes, StyleCache, restyle};
    let css = "
        .a { color: red } .a.on { color: blue }
        [data-open] .b { padding-top: 3px }
        .c:last-child { padding-left: 5px }
        .on + .c { padding-bottom: 2px }
        .p:has(.on) { margin-top: 7px }
        .p:has(.on) > .c { margin-right: 4px }
        .v { --x: 9px } .u { padding-right: var(--x) }
        .x { margin-left: 1px }
    ";
    let sheet = Stylesheet::parse(css).unwrap();
    let el = |id, class: &str| {
        Op::El(
            id,
            "div".into(),
            Props {
                class: class.into(),
                ..Default::default()
            },
        )
    };
    let mut tree = Tree::default();
    // body > p#2 > (a#3 > b#4, c#5, c#6), v#7 > u#8
    for op in [
        el(2, "p"),
        el(3, "a"),
        el(4, "b"),
        el(5, "c"),
        el(6, "c"),
        el(7, "v"),
        el(8, "u"),
    ] {
        tree.apply(op);
    }
    for (parent, child) in [(BODY, 2), (2, 3), (3, 4), (2, 5), (2, 6), (BODY, 7), (7, 8)] {
        tree.apply(Op::Append(parent, child));
    }
    let cache = StyleCache::default();
    let all = Changes {
        all: true,
        ..Default::default()
    };
    let mut styles = restyle(
        &tree,
        &sheet,
        &WIDE,
        &|_| Some(0.5),
        &cache,
        &IdMap::default(),
        IdMap::default(),
        &all,
    );
    let set = |class: &str, attrs: &[(&str, &str)]| Props {
        class: class.into(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Default::default()
    };
    let steps: Vec<(Vec<Op>, Changes)> = vec![
        // A grandchild's class reaches its grandparent's other children through `.p:has(.on) > .c`.
        (
            vec![Op::Set(4, set("b on", &[]))],
            Changes {
                props: vec![4],
                ..Default::default()
            },
        ),
        (
            vec![Op::Set(4, set("b", &[]))],
            Changes {
                props: vec![4],
                ..Default::default()
            },
        ),
        // Own class: .a.on, and the ancestor's :has(.on).
        (
            vec![Op::Set(3, set("a on", &[]))],
            Changes {
                props: vec![3],
                ..Default::default()
            },
        ),
        // An ancestor attribute changes a descendant's match.
        (
            vec![Op::Set(2, set("p", &[("data-open", "")]))],
            Changes {
                props: vec![2],
                ..Default::default()
            },
        ),
        // A sibling's class reaches the next sibling (`+`); repeated ids, as a bulk commit sends.
        (
            vec![Op::Set(5, set("c on", &[]))],
            Changes {
                props: vec![5, 5],
                children: vec![2, 2],
                ..Default::default()
            },
        ),
        // Removing the last child makes its sibling the last child.
        (
            vec![Op::Remove(2, 6)],
            Changes {
                children: vec![2],
                gone: vec![6],
                ..Default::default()
            },
        ),
        // A subtree leaves, changes while out of the document, and comes back.
        (
            vec![Op::Detach(BODY, 7)],
            Changes {
                children: vec![BODY],
                gone: vec![7, 8],
                ..Default::default()
            },
        ),
        (
            vec![Op::Set(8, set("u x", &[]))],
            Changes {
                props: vec![8],
                ..Default::default()
            },
        ),
        (
            vec![Op::Append(BODY, 7)],
            Changes {
                children: vec![BODY],
                ..Default::default()
            },
        ),
        // A variable from an ancestor reaches a descendant through inheritance.
        (
            vec![Op::Set(7, set("", &[]))],
            Changes {
                props: vec![7],
                ..Default::default()
            },
        ),
    ];
    for (ops, changes) in steps {
        for op in ops {
            tree.apply(op);
        }
        styles = restyle(
            &tree,
            &sheet,
            &WIDE,
            &|_| Some(0.5),
            &cache,
            &IdMap::default(),
            styles,
            &changes,
        );
        let full = compute(&tree, &sheet, &WIDE, &|_| Some(0.5));
        assert_eq!(styles, full);
    }
}

/// Without sibling selectors only changed and attached nodes restyle; a node moved under another
/// ancestor still rematches descendant selectors below it.
#[test]
fn incremental_restyle_without_sibling_selectors() {
    use super::cascade::{Changes, StyleCache, restyle};
    let sheet = Stylesheet::parse(".a .b { padding-top: 3px } .p > .b { margin-top: 1px }").unwrap();
    assert!(!sheet.reads_siblings);
    let el = |id, class: &str| {
        Op::El(
            id,
            "div".into(),
            Props {
                class: class.into(),
                ..Default::default()
            },
        )
    };
    let mut tree = Tree::default();
    // body > a#2 > x#3 > b#4, p#5 > b#6
    for op in [el(2, "a"), el(3, "x"), el(4, "b"), el(5, "p"), el(6, "b")] {
        tree.apply(op);
    }
    for (parent, child) in [(BODY, 2), (2, 3), (3, 4), (BODY, 5), (5, 6)] {
        tree.apply(Op::Append(parent, child));
    }
    let cache = StyleCache::default();
    let all = Changes {
        all: true,
        ..Default::default()
    };
    let mut styles = restyle(&tree, &sheet, &WIDE, &|_| Some(0.5), &cache, &IdMap::default(), IdMap::default(), &all);
    let steps: Vec<(Op, Changes)> = vec![
        // x (with b inside) moves from under .a to under .p: b loses `.a .b`.
        (
            Op::Append(5, 3),
            Changes {
                children: vec![5],
                attached: vec![3],
                ..Default::default()
            },
        ),
        // A new b straight under .p.
        (el(7, "b"), Changes::default()),
        (
            Op::Insert(5, 7, 6),
            Changes {
                children: vec![5],
                attached: vec![7],
                ..Default::default()
            },
        ),
        // Removing a sibling restyles nothing else.
        (
            Op::Remove(5, 6),
            Changes {
                children: vec![5],
                gone: vec![6],
                ..Default::default()
            },
        ),
    ];
    for (op, changes) in steps {
        tree.apply(op);
        styles = restyle(&tree, &sheet, &WIDE, &|_| Some(0.5), &cache, &IdMap::default(), styles, &changes);
        assert_eq!(styles, compute(&tree, &sheet, &WIDE, &|_| Some(0.5)));
    }
}

#[test]
fn keyframes_resolve_through_the_elements_variables() {
    // tw-animate-css: `.animate-in` runs `enter`, whose `from` reads variables set by other classes.
    let css = "
        @keyframes enter { from { opacity: var(--tw-enter-opacity, 1); transform: translate3d(0, var(--tw-enter-translate-y, 0), 0) scale3d(var(--tw-enter-scale, 1), var(--tw-enter-scale, 1), var(--tw-enter-scale, 1)) rotate(var(--tw-enter-rotate, 0)) } }
        .animate-in { animation: enter .2s ease-out 0s 1 normal both }
        .fade-in-0 { --tw-enter-opacity: 0 } .zoom-in-95 { --tw-enter-scale: .95 } .slide { --tw-enter-translate-y: -8px }
    ";
    let c = inner(css, "animate-in fade-in-0 zoom-in-95 slide");
    let a = c.animation.expect("animation");
    assert_eq!(
        (a.name.as_str(), a.duration, a.frames.len()),
        ("enter", 0.2, 1)
    );
    assert!(a.fill_backwards && a.fill_forwards);
    let mut start = StyleRefinement::default();
    a.sample(&Look::of(&start), 0.0).unwrap().apply(&mut start);
    assert_eq!(start.opacity, Some(0.0));
    assert_eq!(start.scale, Some(gpui::point(0.95, 0.95)));
    // `rotate(0)` and a zero translate are no-ops that compose with the element's own
    // `rotate`/`translate` properties, not values that replace them; a real translate stays.
    assert_eq!(start.rotate, None);
    assert_eq!(start.translate.map(|t| t.y), pxd(-8.0));
    assert!(
        inner(css, "fade-in-0").animation.is_none(),
        "no animation without animation-name"
    );
}

#[test]
fn rotate_scale_z_index_and_svg_paint() {
    let c = inner(
        ".r { rotate: 90deg; scale: .5 2; z-index: 50; fill: currentColor; stroke: none; color: #00f }",
        "r",
    );
    assert_eq!(c.base.rotate, Some(90.0));
    assert_eq!(c.base.scale, Some(gpui::point(0.5, 2.0)));
    assert_eq!(c.z_index, Some(50));
    assert_eq!(c.fill, Some(Paint::Color(rgb(0.0, 0.0, 1.0).unwrap())));
    assert_eq!(c.stroke, Some(Paint::None));
    let tick = ".t { background-image: url(data:image/png;base64,YWJj) }";
    assert!(inner(tick, "t").bg_image.is_some());
    // (Distinct selectors: lightningcss merges adjacent rules with the same one.)
    assert!(inner(&format!("{tick} div.t {{ appearance: none }}"), "t").bg_image.is_none());
    assert!(inner(&format!(".t {{ appearance: none }} div{tick}"), "t").bg_image.is_some());
    let link = inner(".u { text-underline-offset: 4px; text-decoration-line: underline }", "u");
    assert_eq!(link.base.text.and_then(|t| t.underline).and_then(|u| u.offset), Some(px(4.0)));
    let dense = inner(".d { grid-auto-flow: dense }", "d");
    assert_eq!(dense.base.grid_auto_flow, Some(gpui::GridAutoFlow::RowDense));
    let wrap = |v: &str| inner(&format!(".t {{ text-wrap: {v} }}"), "t").base.text.unwrap_or_default();
    assert_eq!((wrap("balance").balance, wrap("nowrap").white_space), (Some(true), Some(gpui::WhiteSpace::Nowrap)));
    let content = |v: &str| inner(&format!(".x::before {{ content: {v} }}"), "x").content;
    assert_eq!(content(r#""a" attr(data-x) " \\201C""#), None, "the element itself has no content");
    let css = r#".x::before { content: "(" counter(step, upper-roman) ") \201C" open-quote / "alt" }"#;
    let out = styles(css, "", "x", None, WIDE);
    let text = out[&super::cascade::pseudo_id(3, super::Pseudo::Before)].content.clone().unwrap();
    assert_eq!(text, "(\u{E000}counter step upper-roman\u{E001}) \u{201C}\u{201C}");
    let canvas = inner(".s { background-color: Canvas; color: CanvasText }", "s");
    assert_eq!((fill_color(&canvas.base.background), canvas.color), (Some(gpui::white()), gpui::black()));
    let k = inner(".k { transform: skew(10deg, 5deg) skewX(5deg) }", "k");
    assert_eq!(k.base.skew, Some(gpui::point(15.0, 5.0)));
    let t = inner(".t { transform: rotate(45deg) scale(2) }", "t");
    assert_eq!(
        (t.base.rotate, t.base.scale),
        (Some(45.0), Some(gpui::point(2.0, 2.0)))
    );
}

#[test]
fn text_transform_inherits_and_line_clamp_applies() {
    use super::props::TextCase;
    let s = styles(
        ".o { text-transform: capitalize } .c { -webkit-line-clamp: 2; display: -webkit-box }",
        "o",
        "c",
        None,
        WIDE,
    );
    assert_eq!(
        s[&3].case,
        TextCase::Capitalize,
        "inherited from the parent"
    );
    assert_eq!(
        TextCase::Capitalize.apply("hello  wide-world"),
        "Hello  Wide-world"
    );
    assert_eq!(s[&3].base.text.as_ref().and_then(|t| t.line_clamp), Some(2));
    assert_eq!(s[&3].disp, Disp::Block);
}

#[test]
fn sr_only_clip_hides() {
    let c = inner(".sr-only { clip-path: inset(50%) }", "sr-only");
    assert_eq!(c.base.visibility, Some(gpui::Visibility::Hidden));
    assert_eq!(inner(".v { clip-path: none }", "v").base.visibility, None);
}

#[test]
fn tw_animate_css_minified_enter_animation() {
    // Verbatim from tw-animate-css as Tailwind emits it: `var()`s back to back, no spaces.
    let css = "
        .animate-in { animation: enter var(--tw-animation-duration,var(--tw-duration,.15s))var(--tw-ease,ease)var(--tw-animation-delay,0s)var(--tw-animation-iteration-count,1)var(--tw-animation-direction,normal)var(--tw-animation-fill-mode,none); }
        .fade-in-0 { --tw-enter-opacity: calc(0/100); --tw-enter-opacity: 0; }
        .zoom-in-95 { --tw-enter-scale: calc(95*1%); --tw-enter-scale: .95; }
        @keyframes enter { from {
            opacity: var(--tw-enter-opacity,1);
            transform: translate3d(var(--tw-enter-translate-x,0),var(--tw-enter-translate-y,0),0)scale3d(var(--tw-enter-scale,1),var(--tw-enter-scale,1),var(--tw-enter-scale,1))rotate(var(--tw-enter-rotate,0));
            filter: blur(var(--tw-enter-blur,0));
        } }
    ";
    let a = inner(css, "animate-in fade-in-0 zoom-in-95")
        .animation
        .expect("animation parses");
    assert_eq!((a.duration, a.iterations), (0.15, 1.0));
    let mut s = StyleRefinement::default();
    a.sample(&Look::of(&s), 0.0).unwrap().apply(&mut s);
    assert_eq!(
        (s.opacity, s.scale),
        (Some(0.0), Some(gpui::point(0.95, 0.95)))
    );
}

#[test]
fn dir_follows_the_nearest_dir_attribute_and_defaults_to_ltr() {
    let css = ".a:dir(ltr) { opacity: 0.5 } .a:dir(rtl) { opacity: 0.25 }";
    assert_eq!(inner(css, "a").base.opacity, Some(0.5));
    let mut tree = Tree::default();
    let mut attrs = HashMap::new();
    attrs.insert("dir".to_string(), "RTL".to_string());
    tree.apply(Op::El(
        2,
        "div".into(),
        Props {
            attrs,
            ..Default::default()
        },
    ));
    tree.apply(Op::El(
        3,
        "div".into(),
        Props {
            class: "a".into(),
            ..Default::default()
        },
    ));
    tree.apply(Op::Append(BODY, 2));
    tree.apply(Op::Append(2, 3));
    let s = compute(&tree, &Stylesheet::parse(css).unwrap(), &WIDE, &|_| Some(0.5));
    assert_eq!(s[&3].base.opacity, Some(0.25));
}

#[test]
fn mixed_calc_translate_keeps_both_parts() {
    // shadcn's tooltip arrow: `translate-y-[calc(-50%-2px)]`.
    let c = inner(".a { translate: 0 calc(-50% - 2px) }", "a");
    let f = DefiniteLength::Fraction;
    assert_eq!(c.base.translate, Some(gpui::point(f(0.0), f(-0.5))));
    assert_eq!(c.base.translate_px, Some(gpui::point(px(0.0), px(-2.0))));
    let plain = inner(".a { translate: 4px 50% }", "a");
    assert_eq!(plain.base.translate_px, None);
}

#[test]
fn transform_origin_keywords_lengths_and_percentages() {
    let origin = |v: &str| {
        inner(&format!(".a {{ transform-origin: {v} }}"), "a")
            .base
            .transform_origin
    };
    let f = DefiniteLength::Fraction;
    assert_eq!(
        origin("0 0"),
        Some(gpui::point(pxd(0.0).unwrap(), pxd(0.0).unwrap()))
    );
    assert_eq!(
        origin("100% 0"),
        Some(gpui::point(f(1.0), pxd(0.0).unwrap()))
    );
    assert_eq!(
        origin("center 0"),
        Some(gpui::point(f(0.5), pxd(0.0).unwrap()))
    );
    assert_eq!(origin("right bottom"), Some(gpui::point(f(1.0), f(1.0))));
    assert_eq!(origin("center"), None, "the default");
}

#[test]
fn tailwind_gradients_survive_their_interpolation_color_space() {
    // Tailwind v4's `bg-gradient-to-t from-primary/5 to-card`, as compiled.
    let css = r#"
      :root { --primary: oklch(0.205 0 0); --card: oklch(1 0 0); }
      .g { --tw-gradient-position: to top in oklab; background-image: linear-gradient(var(--tw-gradient-stops)); }
      .f { --tw-gradient-from: var(--primary);
           @supports (color: color-mix(in lab, red, red)) { --tw-gradient-from: color-mix(in oklab, var(--primary) 5%, transparent); }
           --tw-gradient-stops: var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position)); }
      .t { --tw-gradient-to: var(--card); }
      @property --tw-gradient-from-position { syntax: "<length-percentage>"; inherits: false; initial-value: 0%; }
      @property --tw-gradient-to-position { syntax: "<length-percentage>"; inherits: false; initial-value: 100%; }
    "#;
    let bg = format!("{:?}", inner(css, "g f t").base.background);
    assert!(bg.contains("LinearGradient(0,") && bg.contains("Oklab"), "{bg}");
    let plain = format!("{:?}", inner(".g { background-image: linear-gradient(red, blue) }", "g").base.background);
    assert!(plain.contains("sRGB"), "{plain}");
    // Primary at 5% over the white behind it: opaque, so gpui's straight-alpha blend matches a browser's.
    assert!(bg.contains("l: 0.953") && bg.contains("a: 1.0"), "{bg}");
    let radial = inner(".g { background-image: radial-gradient(in oklab, red, blue) }", "g");
    assert!(format!("{:?}", radial.base.background).contains("RadialGradient"));
}

#[test]
fn container_queries_read_the_named_containers_size() {
    // Tailwind v4's `@container/main` and `@xl/main:grid-cols-2`.
    let css = r#"
      .main { container-type: inline-size; container-name: main; }
      @container main (width >= 36rem) { .x { color: red } }
      @container card (width >= 1px) { .x { background-color: red } }
    "#;
    let sheet = Stylesheet::parse(css).unwrap();
    let tree = tree("main", "x", None);
    let all = Changes {
        all: true,
        ..Default::default()
    };
    let at = |width: Option<f32>| {
        let sizes: IdMap<(f32, f32)> = width.map(|w| (2, (w, 100.0))).into_iter().collect();
        let cache = StyleCache::default();
        let out = restyle(
            &tree,
            &sheet,
            &WIDE,
            &|_| Some(0.5),
            &cache,
            &sizes,
            IdMap::default(),
            &all,
        );
        assert!(out[&2].container);
        // No container named `card`: that query never matches.
        assert!(out[&3].base.background.is_none());
        text_color(&out[&3])
    };
    assert_eq!(at(Some(600.0)), rgb(1.0, 0.0, 0.0));
    assert_ne!(at(Some(500.0)), rgb(1.0, 0.0, 0.0));
    // Before the first layout, the window (1024px) stands in.
    assert_eq!(at(None), rgb(1.0, 0.0, 0.0));
}

#[test]
fn before_and_after_boxes_cascade_from_their_element() {
    use super::Pseudo;
    use super::cascade::pseudo_id;
    // Tailwind's `after:absolute` (content via its @property default), vaul's drawer `::after`,
    // and a `::before` whose `content: none` makes no box.
    let css = r#"
      @property --tw-content { syntax: "*"; inherits: false; initial-value: ""; }
      .o { color: red }
      .t { background-color: rgb(0, 0, 255) }
      .t::after { content: var(--tw-content); position: absolute; width: 10px; background: inherit }
      [data-vaul-drawer]::after { content: ''; background: inherit }
      .t::before { content: none }
      .t::placeholder { color: blue }
    "#;
    let out = styles(css, "o", "t", None, WIDE);
    let after = &out[&pseudo_id(3, Pseudo::After)];
    assert_eq!(out[&3].placeholder, Some(rgb(0.0, 0.0, 1.0).unwrap()));
    assert_eq!(out[&1].placeholder, None);
    assert_eq!(after.content.as_deref(), Some(""));
    assert_eq!(after.base.position, Some(gpui::Position::Absolute));
    assert_eq!(after.base.size.width, pxl(10.0));
    assert_eq!(
        after.color,
        rgb(1.0, 0.0, 0.0).unwrap(),
        "inherits from its element"
    );
    let bg = fill_color(&after.base.background);
    assert_eq!(
        bg,
        rgb(0.0, 0.0, 1.0),
        "background: inherit takes the element's"
    );
    assert!(!out.contains_key(&pseudo_id(3, Pseudo::Before)));
    // The element itself is untouched.
    assert_ne!(out[&3].base.position, Some(gpui::Position::Absolute));
}

#[test]
fn hidden_border_sides_keep_the_drawn_sides_color() {
    // FullCalendar: gray borders, with the sides it hides reset by a shorthand (to currentColor).
    let css = ".c { color: #000; border: 1px solid #ddd } .h { border-left: 0 !important; border-right: 0 !important }";
    let c = inner(css, "c h");
    assert_eq!(
        c.base.border_color,
        rgb(
            0xdd as f32 / 255.0,
            0xdd as f32 / 255.0,
            0xdd as f32 / 255.0
        )
    );
    assert_eq!(
        c.base.border_widths.left,
        Some(AbsoluteLength::Pixels(px(0.0)))
    );
}

#[test]
fn checked_inputs_style_their_siblings() {
    let css = ".r:checked ~ span { color: #f00 }";
    let red = |checked: bool| {
        let mut tree = Tree::default();
        let attrs = if checked {
            HashMap::from([("checked".to_string(), String::new())])
        } else {
            HashMap::new()
        };
        let input = Props {
            class: "r".into(),
            attrs,
            ..Default::default()
        };
        for op in [
            Op::El(2, "input".into(), input),
            Op::El(3, "span".into(), Props::default()),
            Op::Append(BODY, 2),
            Op::Append(BODY, 3),
        ] {
            tree.apply(op);
        }
        let s = compute(&tree, &Stylesheet::parse(css).unwrap(), &WIDE, &|_| Some(0.5));
        text_color(&s[&3]) == rgb(1.0, 0.0, 0.0)
    };
    assert!(red(true));
    assert!(!red(false));
}

#[test]
fn backdrop_blur_from_tailwind() {
    // Tailwind v4's `backdrop-blur-[32px]`, unset siblings and the -webkit- copy included.
    let css = ".b { --tw-backdrop-blur: blur(32px); -webkit-backdrop-filter: var(--tw-backdrop-blur,) var(--tw-backdrop-brightness,); backdrop-filter: var(--tw-backdrop-blur,) var(--tw-backdrop-brightness,) } .n { backdrop-filter: none }";
    assert_eq!(inner(css, "b").base.backdrop_blur, Some(px(32.0).into()));
    assert_eq!(inner(css, "n").base.backdrop_blur, None);
}

#[test]
fn pointer_events_none_inherits_and_auto_restores() {
    let s = styles(".o { pointer-events: none }", "o", "", None, WIDE);
    assert!(s[&2].no_pointer && s[&3].no_pointer, "inherited from the parent");
    let s = styles(".o { pointer-events: none } .c { pointer-events: auto }", "o", "c", None, WIDE);
    assert!(!s[&3].no_pointer);
}

#[test]
fn focus_visible_rules_apply_only_to_the_visible_focus_style() {
    let s = styles(".c:focus { opacity: .5 } .c:focus-visible { margin-left: 3px }", "", "c", None, WIDE);
    let c = &s[&3];
    let focus = c.focus.as_deref().expect("a :focus style");
    assert_eq!(focus.opacity, Some(0.5));
    assert_eq!(focus.margin.left, None, ":focus-visible isn't plain focus");
    let visible = c.focus_visible.as_deref().expect("a :focus-visible style");
    assert_eq!(visible.opacity, Some(0.5), "plain :focus rules hold too");
    assert!(visible.margin.left.is_some());
}

#[test]
fn ch_is_the_width_of_the_fonts_zero() {
    let fonts = |f: &str| match f {
        "Inter" => Some(0.625),
        ".SystemUIFont" => Some(0.5),
        _ => None,
    };
    let css = ".o { font-family: Missing, Inter; font-size: 10px } .c { width: 25ch; font-size: 16px }";
    let sheet = Stylesheet::parse(css).unwrap();
    let s = compute(&tree("o", "c", None), &sheet, &WIDE, &fonts);
    // The inner box inherits Inter: 25 × 0.625 × its own 16px.
    assert_eq!(s[&3].base.size.width, Some(px(250.0).into()));
    let s = compute(&tree("", "c", None), &sheet, &WIDE, &fonts);
    assert_eq!(s[&3].base.size.width, Some(px(200.0).into()), "the system font's zero");
}

#[test]
fn svg_presentation_properties_pass_through_as_attributes() {
    let c = inner(".c { fill-opacity: 0.1; stroke-dasharray: 5 2; stroke-width: 2px }", "c");
    let get = |name| c.presentation.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str());
    assert_eq!(get("fill-opacity"), Some(".1"));
    assert_eq!(get("stroke-dasharray"), Some("5 2"));
    assert_eq!(get("stroke-width"), Some("2px"));
}

#[test]
fn box_shadow_none_overrides_an_earlier_shadow() {
    // MUI's contained button: its theme drops the hover shadow MUI gives it.
    let css = ":root { --s4: 0 4px 8px black }
        .b { box-shadow: 0 1px 2px red } .b:hover { box-shadow: var(--s4) }
        @media (hover: none) { .b:hover { box-shadow: 0 0 red } }
        .b:hover { background-image: none; box-shadow: none }";
    let c = inner(css, "b");
    assert_eq!(c.base.box_shadow.as_ref().map(Vec::len), Some(1));
    let hover = c.hover.as_deref().expect("a hover style");
    assert_eq!(hover.box_shadow, Some(vec![]));
}

#[test]
fn inset_shadows_are_kept_apart_and_leave_see_through_boxes_alone() {
    // MUI's dashboard cards: two inset highlights and an outer shadow.
    let css = ".card { box-shadow: inset 0 1px 0 1px #fff9, inset 0 -1px 0 1px #ddd, 0 2px 4px #0003 }
        .flat { box-shadow: inset 0 1px 0 1px #fff9 }";
    let card = inner(css, "card");
    let inset = card.base.inset_shadow.clone().unwrap();
    assert_eq!(inset.len(), 2);
    assert_eq!((inset[0].offset.y, inset[0].spread_radius), (px(1.0), px(1.0)));
    assert_eq!(card.base.box_shadow.as_ref().map(Vec::len), Some(1));
    // Only an outer shadow needs the box filled to hide what gpui draws under it.
    let flat = inner(css, "flat");
    assert_eq!(flat.base.box_shadow, Some(vec![]));
    assert!(flat.base.background.is_none());
}

#[test]
fn only_hover_on_an_ancestor_can_make_a_group() {
    let css = r".a:hover { color: red } .group:hover .b { color: red } .c:is(:where(.group):hover *) { color: red }";
    let sheet = Stylesheet::parse(css).unwrap();
    // After the UA stylesheet's.
    let flags: Vec<(bool, bool)> = sheet.entries[sheet.entries.len() - 3..]
        .iter()
        .map(|e| (e.states != 0, e.ancestor_states != 0))
        .collect();
    assert_eq!(flags, [(true, false), (true, true), (true, true)]);
}

#[test]
fn app_region_marks_title_bar_parts() {
    // Electron's `-webkit-app-region` too; not inherited: the press decides by the innermost box.
    let css = ".bar { app-region: drag } .btn { -webkit-app-region: no-drag }";
    let s = styles(css, "bar", "btn", None, WIDE);
    assert_eq!(s[&2].app_region, Some(AppRegion::Drag));
    assert_eq!(s[&3].app_region, Some(AppRegion::NoDrag));
    let s = styles(css, "bar", "", None, WIDE);
    assert_eq!(s[&3].app_region, None);
}

#[test]
fn text_decoration_parts_refine_the_line() {
    let css = ".c { text-decoration: underline wavy red 2px }";
    let c = inner(css, "c");
    let u = c.base.text.as_ref().and_then(|t| t.underline).unwrap();
    assert!(u.wavy);
    assert_eq!(u.thickness, px(2.0));
    assert_eq!(u.color, Some(Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }.into()));
}

#[test]
fn numeric_variants_and_feature_settings_become_font_features() {
    // Tailwind v4's `tabular-nums`, then a setting that overrides one of its features.
    let css = ".c { --tw-numeric-spacing: tabular-nums; font-variant-numeric: var(--tw-ordinal,) var(--tw-numeric-spacing,); font-feature-settings: \"tnum\" 0, \"ss01\" }";
    let c = inner(css, "c");
    let f = c.base.text.as_ref().and_then(|t| t.font_features.clone()).unwrap();
    assert_eq!(f.tag_value_list(), [("tnum".to_owned(), 0), ("ss01".to_owned(), 1)]);
}

#[test]
fn white_space_inherits() {
    use super::props::Ws;
    let s = styles(".o { white-space: pre-line }", "o", "", None, WIDE);
    assert_eq!(s[&3].ws, Ws::Lines);
    let s = styles(".o { white-space: pre-wrap } .c { white-space: normal }", "o", "c", None, WIDE);
    assert_eq!((s[&2].ws, s[&3].ws), (Ws::Preserve, Ws::Collapse));
}

#[test]
fn object_fit_and_background_url_place_images() {
    use gpui::ObjectFit;
    let c = inner(".c { object-fit: cover; object-position: right 25% }", "c");
    assert_eq!((c.image.fit, c.image.at), (ObjectFit::Cover, (1.0, 0.25)));
    let c = inner("", "c");
    assert_eq!((c.image.fit, c.image.at), (ObjectFit::Fill, (0.5, 0.5)), "CSS's initial values");
    let c = inner(".c { background: red url(/images/a.png) center / contain no-repeat }", "c");
    let (url, at) = c.bg_image.unwrap();
    assert_eq!((url.as_str(), at.fit, at.at), ("/images/a.png", ObjectFit::Contain, (0.5, 0.5)));
    assert!(c.base.background.is_some(), "the color is still drawn under it");
    let c = inner(".c { background-image: url(a.png); background-image: linear-gradient(red, blue) }", "c");
    assert!(c.bg_image.is_none(), "a later background-image replaces the url");
}

#[test]
fn gradients_one_fill_cannot_draw_become_layers() {
    use super::props::{BgShape, Lp};
    let c = inner(".c { background: linear-gradient(90deg, red, blue) }", "c");
    assert!(c.bg_layers.is_empty() && c.base.background.is_some(), "a plain gradient is the Fill");
    let c = inner(".c { background: radial-gradient(blue 2px, transparent 3px) 0 0 / 12px 12px, white }", "c");
    let [l] = &c.bg_layers[..] else { panic!("{:?}", c.bg_layers) };
    assert_eq!((l.size, l.repeat), ([Some(Lp::Px(12.0)), Some(Lp::Px(12.0))], [true, true]));
    assert_eq!(l.stops[0].1, Some(Lp::Px(2.0)));
    assert!(c.base.background.is_some(), "the color stays the Fill under the layer");
    let c = inner(".c { background: linear-gradient(white, transparent), conic-gradient(red, blue) }", "c");
    assert_eq!(c.bg_layers.len(), 2);
    assert!(matches!(c.bg_layers[1].shape, BgShape::Conic { .. }));
    let c = inner(".c { background: repeating-linear-gradient(45deg, #000 0 4px, #fff 4px 8px) }", "c");
    assert!(c.bg_layers[0].repeating);
}

#[test]
fn table_properties_are_kept() {
    use super::props::VAlign;
    let c = inner(".c { border-collapse: collapse; table-layout: fixed; vertical-align: bottom }", "c");
    assert_eq!((c.table.collapse, c.table.fixed, c.table.valign), (Some(true), true, Some(VAlign::Bottom)));
    let c = inner(".c { border-spacing: 6px 2px }", "c");
    assert_eq!(c.table.spacing, Some((6.0, 2.0)));
}

#[test]
fn text_shadow_keeps_its_first_shadow() {
    let c = inner(".c { text-shadow: 2px 3px 0 #ff0000, 1px 1px blue }", "c");
    let (color, x, y) = c.text_shadow.flatten().unwrap();
    assert_eq!((x, y, color.to_rgb()), (2.0, 3.0, gpui::rgb(0xff0000).into()));
    assert_eq!(inner(".c { text-shadow: none }", "c").text_shadow, Some(None));
}

#[test]
fn border_sides_of_different_colors_are_split() {
    let c = inner(".c { border-style: solid; border-width: 5px 5px 0 5px; border-color: red transparent transparent transparent }", "c");
    let sides = c.border_sides.expect("a triangle's sides differ");
    assert_eq!(sides[0], gpui::red());
    assert!(sides[1].is_transparent() && sides[2].is_transparent());
    assert!(c.base.border_color.is_some_and(|c| c.is_transparent()), "the box's own border is clear");
    let c = inner(".c { border-bottom: 1px solid red }", "c");
    assert!(c.border_sides.is_none(), "one side, one color");
    let c = inner(".c { border: 2px solid; border-color: red blue; border-radius: 4px }", "c");
    assert!(c.border_sides.is_none(), "rounded boxes keep one color");
}

#[test]
fn order_is_kept_per_item() {
    assert_eq!(inner(".c { order: -1 }", "c").order, -1);
    assert_eq!(inner(".c { order: 9999 }", "c").order, 9999);
}

#[test]
fn grid_auto_flow_maps_to_taffy_flows() {
    use gpui::GridAutoFlow;
    let flow = |css: &str| inner(css, "c").base.grid_auto_flow;
    assert_eq!(flow(".c { grid-auto-flow: column }"), Some(GridAutoFlow::Column));
    assert_eq!(flow(".c { grid-auto-flow: dense }"), Some(GridAutoFlow::RowDense));
    assert_eq!(flow(".c { grid-auto-flow: column dense }"), Some(GridAutoFlow::ColumnDense));
}

#[test]
fn outline_offset_leaves_a_gap_in_the_backdrop_color() {
    let c = inner(".c { outline: 2px solid red; outline-offset: 3px }", "c");
    let spreads: Vec<f32> = c.base.box_shadow.iter().flatten().map(|s| f32::from(s.spread_radius)).collect();
    assert_eq!(spreads, [5.0, 3.0], "the ring, then the gap painted over it");
    let gap = c.base.box_shadow.unwrap()[1].color;
    assert_eq!(gap, Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }.into(), "the white page behind");
}

#[test]
fn word_breaking_and_letter_spacing_reach_the_text_style() {
    use gpui::WordBreak;
    let text = |css: &str| inner(css, "c").base.text.unwrap();
    assert_eq!(text(".c { overflow-wrap: break-word }").word_break, Some(WordBreak::BreakWord));
    assert_eq!(text(".c { overflow-wrap: anywhere }").word_break, Some(WordBreak::Anywhere));
    assert_eq!(text(".c { word-break: break-all }").word_break, Some(WordBreak::Anywhere));
    // Tailwind's `tracking-wide` at `text-sm`: em resolves against the element's own font size.
    let t = text(".c { font-size: 14px; letter-spacing: 0.025em }");
    assert_eq!(t.letter_spacing, Some(px(0.35)));
}

#[test]
fn gradient_stop_positions_are_filled_in_as_css_does() {
    use super::props::fill_positions;
    let filled = |mut at: Vec<Option<f32>>| {
        fill_positions(&mut at);
        at.into_iter().map(Option::unwrap).collect::<Vec<_>>()
    };
    assert_eq!(filled(vec![None, None, None]), [0.0, 0.5, 1.0]);
    assert_eq!(filled(vec![None, Some(0.2), None, None, Some(0.8)]), [0.0, 0.2, 0.4, 0.6, 0.8]);
    assert_eq!(filled(vec![Some(0.5), Some(0.3), None]), [0.5, 0.5, 1.0], "never backwards");
}

#[test]
fn three_stop_and_radial_gradients_are_drawn() {
    let c = inner(".c { background-image: linear-gradient(to right, red, lime, blue) }", "c");
    let Some(gpui::Fill::Color(bg)) = c.base.background else { panic!("no gradient") };
    let [from, to] = bg.gradient_stops().unwrap();
    assert_eq!((from.percentage, to.percentage), (0.0, 1.0));
    let c = inner(".c { background-image: radial-gradient(circle, red, blue) }", "c");
    let Some(gpui::Fill::Color(bg)) = c.base.background else { panic!("no gradient") };
    assert!(bg.gradient_stops().is_some());
}

#[test]
fn filters_compose_into_one_color_matrix() {
    use super::props::compose;
    let f = |css: &str| inner(css, "c").base.filter.unwrap();
    // Tailwind v4's `grayscale`, through its list of `var()`s.
    let g = f(".c { --tw-grayscale: grayscale(100%); filter: var(--tw-blur,) var(--tw-grayscale,) }");
    assert!((g.matrix[0][0] - 0.2126).abs() < 1e-6 && (g.matrix[1][0] - 0.2126).abs() < 1e-6);
    let b = f(".c { filter: blur(4px) brightness(0.5) }");
    assert_eq!((b.blur, b.matrix[0][0], b.matrix[3][3]), (px(4.0), 0.5, 1.0));
    assert_eq!(f(".c { filter: blur(0) }"), gpui::Filter::IDENTITY, "kept, so it can override");
    // Applied in order: invert(1) then brightness(0.5) maps black to half gray.
    let i = f(".c { filter: invert(1) brightness(0.5) }");
    assert_eq!((i.matrix[0][0], i.matrix[0][4]), (-0.5, 0.5));
    assert_eq!(compose(&gpui::Filter::IDENTITY.matrix, &i.matrix), i.matrix);
}

/// Base UI's progress indicator sets `height: inherit` over `h-full`; it fills its track.
#[test]
fn inherited_height_fills_the_parent() {
    let s = styles(".i { height: 0 }", "", "i", Some("height: inherit"), WIDE);
    assert_eq!(s[&3].base.size.height, Some(gpui::relative(1.).into()));
}

/// Tailwind's `supports-backdrop-filter:` variant asks with a `var()`, valid for a known property.
#[test]
fn supports_a_var_value_of_a_known_property() {
    let css = "@supports (backdrop-filter: var(--tw)) { .i { opacity: .5 } }
               @supports (not-a-property: var(--tw)) { .i { opacity: .25 } }";
    assert_eq!(inner(css, "i").base.opacity, Some(0.5));
}

/// The 404 page's `bg-clip-text text-transparent` gradient: the text shows, the box doesn't.
#[test]
fn text_clipped_to_a_gradient_takes_its_middle_color() {
    let c = inner(
        ".i { background-image: linear-gradient(to bottom, white, transparent); background-clip: text; color: transparent }",
        "i",
    );
    assert!(c.base.background.is_none());
    // Stops are flattened onto the backdrop (white here), so the middle is opaque white.
    assert_eq!(text_color(&c), rgb(1.0, 1.0, 1.0));
}

/// `:has()` away from the subject compound: on an ancestor (shadcn's `has-[>x]:[&>input]:pt-3`),
/// inside `:is()` (Tailwind's `group-has-*`), and under `:not()` (the last of a button group).
#[test]
fn has_matches_on_ancestors_and_inside_other_pseudo_classes() {
    let inner_color = |css: &str| text_color(&styles(css, "p", "c", None, WIDE)[&3]);
    let red = rgb(1.0, 0.0, 0.0);
    assert_eq!(inner_color(".p:has(> .c) > .c { color: red }"), red);
    assert_eq!(inner_color(".p:has(> .nope) > .c { color: red }"), None);
    assert_eq!(inner_color(".c:is(:where(.p):has(.c) *) { color: red }"), red);
    assert_eq!(inner_color(".c:is(:where(.p):has(.nope) *) { color: red }"), None);
    assert_eq!(inner_color(".p > .c:not(:has(~ *)) { color: red }"), red, "no later sibling");
    assert_eq!(inner_color(".p > .c:not(:has(*)) { color: red }"), red, "no children");
}

#[test]
fn min_max_and_clamp_mix_percentages_and_lengths() {
    let width = |decl: &str| match inner(&format!(".w {{ width: {decl} }}"), "w").base.size.width {
        Some(Length::Definite(DefiniteLength::Expr(e))) => e,
        other => panic!("{decl}: {other:?}"),
    };
    let min = width("min(100%, 200px)");
    assert_eq!((min.eval(300.0), min.eval(100.0)), (200.0, 100.0));
    let clamp = width("clamp(10px, 50%, 200px)");
    assert_eq!((clamp.eval(10.0), clamp.eval(100.0), clamp.eval(1000.0)), (10.0, 50.0, 200.0));
    // Sums and factors distribute over them; a negative factor turns min into max.
    let sum = width("calc(min(100%, 200px) - 2rem)");
    assert_eq!((sum.eval(1000.0), sum.eval(100.0)), (168.0, 68.0));
    let grid = inner(".g { grid-template-columns: repeat(auto-fit, minmax(min(100%, 200px), 1fr)) }", "g");
    assert!(format!("{:?}", grid.base.grid_template_columns).contains("MinMax(Calc(calc["), "{:?}", grid.base.grid_template_columns);
    let neg = width("calc(100% - 2 * max(10%, 20px))");
    assert_eq!((neg.eval(100.0), neg.eval(1000.0)), (60.0, 800.0));
}

#[test]
fn form_fields_match_required_and_read_only() {
    let css = "input:required { margin-top: 1px } input:optional { margin-top: 2px } input:read-only { margin-left: 3px } input:read-write { margin-left: 4px }";
    let sheet = Stylesheet::parse(css).unwrap();
    let mut tree = Tree::default();
    for (id, attrs) in [(2, vec![("required", "true")]), (3, vec![("readonly", "true")])] {
        let attrs = attrs.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        tree.apply(Op::El(id, "input".into(), Props { attrs, ..Default::default() }));
        tree.apply(Op::Append(BODY, id));
    }
    let out = compute(&tree, &sheet, &WIDE, &|_| Some(0.5));
    let m = |id: u64| {
        let s = &out[&id].base.margin;
        (s.top, s.left)
    };
    let l = |p: f32| Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(p)))));
    assert_eq!(m(2), (l(1.0), l(4.0)));
    assert_eq!(m(3), (l(2.0), l(3.0)));
}

#[test]
fn editable_content_is_read_write() {
    let sheet = Stylesheet::parse("p:read-write { margin-left: 4px } p:read-only { margin-left: 3px }").unwrap();
    let mut tree = Tree::default();
    let editable = |v: &str| Props { attrs: [("contenteditable".to_string(), v.to_string())].into(), ..Default::default() };
    // <div contenteditable><p/><span contenteditable=false><p/></span></div><p/>
    tree.apply(Op::El(2, "div".into(), editable("true")));
    tree.apply(Op::El(3, "p".into(), Props::default()));
    tree.apply(Op::El(4, "span".into(), editable("false")));
    tree.apply(Op::El(5, "p".into(), Props::default()));
    tree.apply(Op::El(6, "p".into(), Props::default()));
    for (parent, child) in [(BODY, 2), (2, 3), (2, 4), (4, 5), (BODY, 6)] {
        tree.apply(Op::Append(parent, child));
    }
    let out = compute(&tree, &sheet, &WIDE, &|_| Some(0.5));
    let l = |p: f32| Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(p)))));
    let left = |id: u64| out[&id].base.margin.left;
    assert_eq!([left(3), left(5), left(6)], [l(4.0), l(3.0), l(3.0)]);
}

#[test]
fn display_gives_table_roles() {
    use super::props::TableRole;
    let role = |d: &str| inner(&format!(".t {{ display: {d} }}"), "t").table.role;
    assert_eq!(
        ["table", "inline-table", "table-row-group", "table-row", "table-cell", "block"].map(role),
        [Some(TableRole::Table), Some(TableRole::Table), Some(TableRole::RowGroup), Some(TableRole::Row), Some(TableRole::Cell), None]
    );
}

#[test]
fn modal_dialogs_match_modal_and_carry_their_backdrop() {
    let mut tree = Tree::default();
    let dialog = |id, modal| {
        let attrs = [("open".to_owned(), String::new())].into();
        Op::El(id, "dialog".into(), Props { modal, attrs, ..Default::default() })
    };
    for op in [dialog(2, true), dialog(3, false), Op::Append(BODY, 2), Op::Append(BODY, 3)] {
        tree.apply(op);
    }
    let css = "dialog:modal { z-index: 5 } dialog:open { order: 2 } dialog::backdrop { background: blue }";
    let sheet = Stylesheet::parse(css).unwrap();
    let out = compute(&tree, &sheet, &WIDE, &|_| Some(0.5));
    assert_eq!((out[&2].z_index, out[&3].z_index), (Some(5), None));
    assert_eq!((out[&2].order, out[&3].order), (2, 2));
    let backdrop = out[&2].backdrop_box.as_ref().expect("::backdrop");
    assert_eq!(fill_color(&backdrop.base.background), rgb(0.0, 0.0, 1.0));
}
