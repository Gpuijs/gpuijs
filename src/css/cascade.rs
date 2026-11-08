//! Per-node cascade: matched rules + inline style -> resolved gpui styles, walking the tree top-down.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{AlignItems, FlexDirection, Hsla, Rgba, StyleRefinement, black, white};
use lightningcss::declaration::DeclarationBlock;
use lightningcss::properties::custom::{CustomProperty, CustomPropertyName, TokenList};
use lightningcss::properties::{Property, PropertyId};
use lightningcss::values::string::CowArcStr;
use lightningcss::stylesheet::{ParserOptions, StyleAttribute};
use lightningcss::values::color::{CssColor, RGBA};
use static_self::IntoOwned;

use super::props::{AppRegion, BgLayer, Builder, Counters, Ctx, Disp, GridAreas, ListStyle, Paint, Placement, REM, TableStyle, TextCase, Ws};
use super::{
    ACTIVE, ContainerQuery, Entry, Env, FOCUS, FOCUS_VISIBLE, HOVER, Pseudo, States, Stylesheet, media_matches,
    select, value_text,
};
use crate::transition::{Animation, Look, Transition};
use crate::tree::{HTML, IdMap, IdSet, Tree};

#[derive(Clone, Debug, PartialEq)]
pub struct Computed {
    pub disp: Disp,
    pub base: StyleRefinement,
    /// The element's `color`, inherited or set; what `currentColor` means inside it.
    pub color: Hsla,
    /// `text-transform` for the text directly inside it.
    pub case: TextCase,
    /// `white-space` for the text directly inside it.
    pub ws: Ws,
    /// `pointer-events: none`: presses and hovers go to what is behind it.
    pub no_pointer: bool,
    /// The opaque color showing through behind the element, for host-drawn popups.
    pub backdrop: Hsla,
    /// Full styles with the state applied; gpui refines `base` with them while the state holds.
    /// Boxed: most elements have none, and an inline one would make every node several KB.
    pub hover: Option<Box<StyleRefinement>>,
    pub active: Option<Box<StyleRefinement>>,
    pub focus: Option<Box<StyleRefinement>>,
    /// Hovered while focused; only computed when both states have rules.
    pub hover_focus: Option<Box<StyleRefinement>>,
    /// Focused visibly (`:focus-visible` as well as `:focus`); only computed when it has rules.
    pub focus_visible: Option<Box<StyleRefinement>>,
    /// The ancestor whose hover restyles this element (`group-hover:`), and the style it gives.
    pub group: Option<(u64, Box<StyleRefinement>)>,
    pub transition: Option<Transition>,
    /// A block-level `width: fit-content` box in block flow; gpui's block layout always stretches,
    /// so the renderer wraps it in a row that lets it shrink to its content.
    pub fit_block: bool,
    /// `position: fixed`: placed against the window, outside the scrolling page.
    pub fixed: bool,
    /// `position: sticky` (its `top` is kept in `base.inset.top`).
    pub sticky: bool,
    /// `field-sizing: content`: a text field grows with its text.
    pub field_grows: bool,
    /// Paint order among overlapping positioned boxes.
    pub z_index: Option<i32>,
    pub app_region: Option<AppRegion>,
    /// CSS `fill`/`stroke` for SVG elements.
    pub fill: Option<Paint>,
    pub stroke: Option<Paint>,
    /// `fill` and `stroke` while hovered: an SVG shape's `:hover` (a map region's highlight).
    pub hover_paints: Option<(Option<Paint>, Option<Paint>)>,
    /// Custom properties in scope, for `var()`s in SVG attributes.
    pub vars: Rc<HashMap<String, String>>,
    /// A query container: the renderer reports its size for `@container` rules.
    pub container: bool,
    /// A `::before`/`::after` box's text (see `pseudo_id`).
    pub content: Option<String>,
    /// `counter-reset`, `-set` and `-increment`.
    pub counters: Option<Rc<Counters>>,
    /// SVG presentation properties CSS sets beyond fill and stroke, by attribute name.
    pub presentation: Vec<(&'static str, String)>,
    /// A running `@keyframes` animation; the renderer tracks its clock.
    pub animation: Option<Animation>,
    /// An `<img>`'s `object-fit` and `object-position`.
    pub image: Placement,
    pub list: ListStyle,
    pub table: TableStyle,
    /// `text-shadow` set on the element (`Some(None)`: `none`); it inherits.
    pub text_shadow: Option<Option<(Hsla, f32, f32)>>,
    /// Border sides of different colors, which `render` paints (see `Builder::border_sides`).
    pub border_sides: Option<[Hsla; 4]>,
    /// A `background-image: url()` and where it sits.
    pub bg_image: Option<(String, Placement)>,
    /// Gradient layers the element's own background can't draw, top first (`crate::background`).
    pub bg_layers: Rc<[BgLayer]>,
    /// `order` among flex and grid siblings.
    pub order: i32,
    /// A text field's `::placeholder` color.
    pub placeholder: Option<Hsla>,
    /// A dialog's `::backdrop` box, which a modal one covers the page with.
    pub backdrop_box: Option<Rc<Computed>>,
}

/// How a parent lays out its children, which decides how `fit-content` widths avoid stretching.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ParentLayout {
    Flow,
    FlexRow,
    FlexColumn { stretch: bool },
    Grid { stretch: bool },
}

impl ParentLayout {
    fn of(disp: Disp, s: &StyleRefinement) -> Self {
        match disp {
            Disp::Flex | Disp::InlineFlex => match s.flex_direction {
                Some(FlexDirection::Column | FlexDirection::ColumnReverse) => {
                    ParentLayout::FlexColumn {
                        stretch: matches!(s.align_items, None | Some(AlignItems::Stretch)),
                    }
                }
                _ => ParentLayout::FlexRow,
            },
            Disp::Grid | Disp::InlineGrid => ParentLayout::Grid {
                stretch: matches!(s.justify_items, None | Some(AlignItems::Stretch)),
            },
            _ => ParentLayout::Flow,
        }
    }
}

#[derive(Clone)]
struct Cascaded {
    fixed: bool,
    sticky: bool,
    field_grows: bool,
    app_region: Option<AppRegion>,
    z_index: Option<i32>,
    fill: Option<Paint>,
    stroke: Option<Paint>,
    presentation: Vec<(&'static str, String)>,
    animation: Option<Animation>,
    style: StyleRefinement,
    disp: Disp,
    inherited: Inherited,
    fit_block: bool,
    transition: Option<Transition>,
    /// `container-type` makes it a query container, with these `container-name`s.
    container: Option<Vec<String>>,
    content: Option<String>,
    counters: Option<Rc<Counters>>,
    image: Placement,
    list: ListStyle,
    table: TableStyle,
    text_shadow: Option<Option<(Hsla, f32, f32)>>,
    border_sides: Option<[Hsla; 4]>,
    bg_image: Option<(String, Placement)>,
    bg_layers: Rc<[BgLayer]>,
    order: i32,
}

#[derive(Clone)]
struct Inherited {
    vars: Rc<HashMap<String, String>>,
    /// Ancestor query containers, innermost last, with their names.
    containers: Rc<Vec<(u64, Vec<String>)>>,
    color: Hsla,
    case: TextCase,
    ws: Ws,
    no_pointer: bool,
    font_px: f32,
    /// `1ch` as a fraction of the font size, for the font in use.
    ch: f32,
    /// The opaque color showing through behind this element.
    backdrop: Hsla,
    layout: ParentLayout,
    /// The parent's `grid-template-areas`, for `grid-area: name`.
    grid_areas: Option<GridAreas>,
}

/// Cascade results kept between restyles. A cascade depends only on the rules that matched, the
/// inline style and what the parent passes down, so equal inputs give equal results; most nodes
/// (rows, list items, chart parts) repeat their neighbours' or their own previous inputs exactly.
/// Valid for one stylesheet and environment: `Root` makes a new one when either changes.
#[derive(Default)]
pub struct StyleCache {
    /// Kept so the root's variables, and so every key built on them, stay the same between restyles.
    root_vars: Rc<HashMap<String, String>>,
    cascades: RefCell<HashMap<MemoKey, Cascaded>>,
    inline: RefCell<HashMap<String, Rc<DeclarationBlock<'static>>>>,
    /// What each node passed down to its children last time, so an unchanged node can be skipped.
    passed: RefCell<IdMap<Inherited>>,
}

/// What changed in the tree since the last restyle, as reported by the ops that changed it.
#[derive(Default)]
pub struct Changes {
    /// Restyle everything (new stylesheet, window or a fresh cache).
    pub all: bool,
    /// Nodes whose class, attributes or inline style changed.
    pub props: Vec<u64>,
    /// Nodes that gained, lost or reordered children.
    pub children: Vec<u64>,
    /// Nodes appended or inserted (maybe moved from elsewhere); their subtrees restyle.
    pub attached: Vec<u64>,
    /// Nodes (whole subtrees) detached or removed; they keep no styles unless back in the tree.
    pub gone: Vec<u64>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        !self.all
            && self.props.is_empty()
            && self.children.is_empty()
            && self.attached.is_empty()
            && self.gone.is_empty()
    }
}

impl Inherited {
    fn same(&self, other: &Inherited) -> bool {
        Rc::ptr_eq(&self.vars, &other.vars)
            && self.containers == other.containers
            && self.color == other.color
            && self.case == other.case
            && self.ws == other.ws
            && self.no_pointer == other.no_pointer
            && self.font_px == other.font_px
            && self.ch == other.ch
            && self.backdrop == other.backdrop
            && self.layout == other.layout
            && self.grid_areas == other.grid_areas
    }
}

/// Everything a cascade reads. Variables are compared by identity: the cache hands out the same
/// `Rc` for the same inputs, so unchanged ancestors keep their children's keys stable.
#[derive(PartialEq, Eq, Hash)]
struct MemoKey {
    /// Matched entries by address; entries live as long as the stylesheet, which outlives the cache.
    entries: Vec<usize>,
    /// Inline styles are interned by text in the cache, so identity is equality here too.
    inline: usize,
    vars: usize,
    color: [u32; 4],
    case: TextCase,
    ws: Ws,
    no_pointer: bool,
    font_px: u32,
    ch: u32,
    backdrop: [u32; 4],
    layout: ParentLayout,
    /// The parent's grid areas by identity: cascades are cached, so equal areas share an `Rc`.
    grid_areas: usize,
}

/// Bound on remembered cascades; dropped wholesale when exceeded (e.g. a long animation of
/// changing inline styles), which only costs the next restyle its hits.
const CACHE_LIMIT: usize = 50_000;

/// A full restyle from scratch.
#[cfg(test)]
pub fn compute(
    tree: &Tree,
    sheet: &Stylesheet,
    env: &Env,
    fonts: &dyn Fn(&str) -> Option<f32>,
) -> IdMap<Rc<Computed>> {
    let changes = Changes {
        all: true,
        ..Default::default()
    };
    restyle(
        tree,
        sheet,
        env,
        fonts,
        &StyleCache::default(),
        &IdMap::default(),
        IdMap::default(),
        &changes,
    )
}

/// Styles for the tree after `changes`, reusing `prev` wherever nothing that a node's style
/// depends on changed. See `restyle_in_place`.
#[cfg(test)]
pub fn restyle(
    tree: &Tree,
    sheet: &Stylesheet,
    env: &Env,
    fonts: &dyn Fn(&str) -> Option<f32>,
    cache: &StyleCache,
    sizes: &IdMap<(f32, f32)>,
    mut prev: IdMap<Rc<Computed>>,
    changes: &Changes,
) -> IdMap<Rc<Computed>> {
    restyle_in_place(tree, sheet, env, fonts, cache, sizes, &mut prev, changes);
    prev
}

/// What a restyle did: see `restyle_in_place`.
#[derive(Debug, Default)]
pub struct Restyled {
    /// Nodes whose computed style (or generated boxes) changed in a way that can change layout.
    pub changed: Vec<u64>,
    /// Every node styled again; any of its computed style or generated boxes may differ.
    pub visited: IdSet,
}

/// Updates `styles` for `changes`, visiting only the nodes that may restyle; returns those and
/// the ones whose computed style (or generated boxes) changed. A node is redone when:
/// - its own or an ancestor's props changed (descendant selectors read ancestors),
/// - a sibling's props or its parent's children changed (`:last-child`, `+`, `~`), for the node
///   itself; its subtree follows only if what it passes down changes,
/// - with `:has()` rules in the sheet, a descendant changed,
/// - what its parent passes down changed.
pub fn restyle_in_place(
    tree: &Tree,
    sheet: &Stylesheet,
    env: &Env,
    fonts: &dyn Fn(&str) -> Option<f32>,
    cache: &StyleCache,
    sizes: &IdMap<(f32, f32)>,
    styles: &mut IdMap<Rc<Computed>>,
    changes: &Changes,
) -> Restyled {
    if cache.cascades.borrow().len() > CACHE_LIMIT || cache.inline.borrow().len() > CACHE_LIMIT {
        // The variables in `passed` point into dropped cascades' keys' world; start over cleanly.
        cache.cascades.borrow_mut().clear();
        cache.inline.borrow_mut().clear();
        cache.passed.borrow_mut().clear();
    }
    let mut subtrees = IdSet::default();
    let mut nodes = IdSet::default();
    let alive = |id: &&u64| tree.nodes.contains_key(*id);
    let ancestors = |id: u64| std::iter::successors(tree.get(id).parent, |&a| tree.get(a).parent);
    // Collected as sets first: a 10k-row mount or update names the same parent 10k times, and
    // adding its children once per mention is quadratic.
    let mut sibling_parents = IdSet::default();
    for &id in changes.props.iter().filter(alive) {
        subtrees.insert(id);
        sibling_parents.extend(tree.get(id).parent);
    }
    // An attached node may have moved under new ancestors, which descendant selectors read.
    subtrees.extend(changes.attached.iter().filter(alive));
    let reparented: IdSet = changes.children.iter().filter(alive).copied().collect();
    for &id in sibling_parents.iter().chain(&reparented) {
        // Without sibling selectors, siblings can't restyle: an unchanged child (a row next to
        // the one that changed or moved) keeps its style.
        if sheet.reads_siblings {
            nodes.extend(tree.get(id).children.iter().copied());
        }
        if sheet.has_has {
            nodes.extend(ancestors(id));
        }
    }
    nodes.extend(&reparented);
    if sheet.has_has {
        // A changed node's own ancestors start at its parent.
        nodes.extend(&sibling_parents);
        // And what a `:has()` on one of them styles further down (`.a:has(> b) > input`).
        let anchors: Vec<u64> = nodes.iter().copied().collect();
        for anchor in anchors {
            nodes.extend(super::select::has_dependents(anchor).iter().filter(alive));
        }
    }
    let root = Inherited {
        vars: cache.root_vars.clone(),
        containers: Rc::default(),
        color: black(),
        case: TextCase::None,
        ws: Ws::Collapse,
        no_pointer: false,
        font_px: REM,
        ch: fonts(".SystemUIFont").unwrap_or(0.5),
        backdrop: white(),
        layout: ParentLayout::Flow,
        grid_areas: None,
    };
    let walk = Walk {
        tree,
        sheet,
        env,
        fonts,
        cache,
        sizes,
        all: changes.all,
        subtrees: &subtrees,
        nodes: &nodes,
        done: RefCell::default(),
        changed: RefCell::default(),
        twins: RefCell::default(),
        recent: RefCell::default(),
    };
    // Depth below the root, or `None` for a node outside the document (detached).
    let depth = |id: u64| {
        let mut n = 0;
        let mut at = id;
        while let Some(p) = tree.get(at).parent {
            (n, at) = (n + 1, p);
        }
        (at == HTML).then_some(n)
    };
    if changes.all {
        styles.clear();
        walk.visit(HTML, &root, false, false, styles);
        cache.passed.borrow_mut().retain(|id, _| styles.contains_key(id));
        return walk.finish();
    }
    // Nodes that left the document drop their styles, so they restyle in full if they come back.
    for &id in &changes.gone {
        if tree.nodes.contains_key(&id) && depth(id).is_some() {
            continue;
        }
        for key in [id, pseudo_id(id, Pseudo::Before), pseudo_id(id, Pseudo::After)] {
            styles.remove(&key);
        }
        cache.passed.borrow_mut().remove(&id);
    }
    // Grown once, not rehashed step by step while a big mount inserts thousands of nodes.
    styles.reserve(tree.nodes.len().saturating_sub(styles.len()));
    // Outermost first, so each one's parent has passed down its new values.
    let mut roots: Vec<(usize, u64)> = subtrees
        .iter()
        .chain(&nodes)
        .filter(|id| tree.nodes.contains_key(*id))
        .filter_map(|&id| Some((depth(id)?, id)))
        .collect();
    roots.sort_unstable();
    roots.dedup();
    for (_, id) in roots {
        if walk.done.borrow().contains(&id) {
            continue;
        }
        let passed = match tree.get(id).parent {
            None => Some(root.clone()),
            Some(p) => cache.passed.borrow().get(&p).cloned(),
        };
        // A parent never styled is new; its own restyle (it is a root too) covers this node.
        if let Some(passed) = passed {
            walk.visit(id, &passed, false, false, styles);
        }
    }
    walk.finish()
}

fn hsla_bits(c: Hsla) -> [u32; 4] {
    [c.h.to_bits(), c.s.to_bits(), c.l.to_bits(), c.a.to_bits()]
}

/// (origin tier, layer rank, specificity, source order); sorted ascending, later wins.
type CascadeKey = (u8, u32, u32, u32);

struct Walk<'a> {
    tree: &'a Tree,
    sheet: &'a Stylesheet,
    env: &'a Env,
    /// An installed font's `1ch` (its "0") as a fraction of its size; `None` if not installed.
    fonts: &'a dyn Fn(&str) -> Option<f32>,
    cache: &'a StyleCache,
    /// Query containers' laid-out sizes (width, height) from the last frame.
    sizes: &'a IdMap<(f32, f32)>,
    all: bool,
    /// Redo these nodes and everything below them.
    subtrees: &'a IdSet,
    /// Redo these nodes; their subtrees only if what they pass down changes.
    nodes: &'a IdSet,
    /// Nodes redone in this restyle.
    done: RefCell<IdSet>,
    /// Nodes whose computed style or generated boxes changed in a way that can change layout.
    changed: RefCell<Vec<u64>>,
    /// Nodes styled as a copy of another's (`twin`), by the original's id.
    twins: RefCell<IdMap<u64>>,
    /// Recently styled children by (the original of) their parent: candidates for `twin`.
    recent: RefCell<IdMap<Vec<u64>>>,
}

/// Whether two styles lay out the same: they differ at most in what is only painted (colors,
/// shadows, opacity, visibility, the styles of states, which apply only while hovered or focused).
pub fn same_layout(a: &Computed, b: &Computed) -> bool {
    let layout = |c: &Computed| {
        let mut s = c.base.clone();
        s.background = None;
        s.border_color = None;
        s.border_style = None;
        s.corner_radii = Default::default();
        s.box_shadow = None;
        s.inset_shadow = None;
        s.mouse_cursor = None;
        s.opacity = None;
        s.visibility = None;
        if let Some(t) = s.text.as_mut() {
            t.color = None;
            t.background_color = None;
            t.underline = None;
            t.strikethrough = None;
        }
        s
    };
    a.disp == b.disp
        && a.case == b.case
        && a.ws == b.ws
        && a.order == b.order
        && a.fit_block == b.fit_block
        && a.fixed == b.fixed
        && a.sticky == b.sticky
        && a.field_grows == b.field_grows
        && a.container == b.container
        && a.content == b.content
        && a.counters == b.counters
        && a.animation == b.animation
        && layout(a) == layout(b)
}

impl Walk<'_> {
    /// A node styled in this pass whose style this one can take as is (style sharing, as browsers
    /// do): same tag, classes, attributes and inline style, under a parent that is itself the same
    /// (so descendant selectors match alike) and passes down the same values. Not with sibling or
    /// `:has()` selectors, which read more than that, nor for styles naming an ancestor's id.
    fn twin(&self, id: u64, parent: &Inherited, styles: &IdMap<Rc<Computed>>) -> Option<u64> {
        if self.sheet.reads_siblings || self.sheet.has_has {
            return None;
        }
        let node = self.tree.get(id);
        let p = node.parent?;
        let key = self.twins.borrow().get(&p).copied().unwrap_or(p);
        let recent = self.recent.borrow();
        let passed = self.cache.passed.borrow();
        let empty = |n: &crate::tree::Node| n.props.value.as_deref().unwrap_or("").is_empty();
        recent.get(&key)?.iter().copied().find(|&t| {
            let other = self.tree.get(t);
            let (a, b) = (&other.props, &node.props);
            styles.get(&t).is_some_and(|c| c.group.is_none() && !c.container)
                && other.tag == node.tag
                && a.id.is_none()
                && b.id.is_none()
                && a.class == b.class
                && a.style == b.style
                && a.attrs == b.attrs
                && a.disabled == b.disabled
                && a.modal == b.modal
                && a.placeholder.is_some() == b.placeholder.is_some()
                && empty(other) == empty(node)
                && other.children.is_empty() == node.children.is_empty()
                && other
                    .parent
                    .and_then(|op| passed.get(&op))
                    .is_some_and(|i| i.same(parent))
        })
    }

    /// Notes how `id` was styled, for `twin`: from `twin`, or as a new candidate.
    fn shared(&self, id: u64, twin: Option<u64>) {
        let Some(p) = self.tree.get(id).parent else {
            return;
        };
        let mut twins = self.twins.borrow_mut();
        let key = twins.get(&p).copied().unwrap_or(p);
        match twin {
            Some(t) => {
                let root = twins.get(&t).copied().unwrap_or(t);
                twins.insert(id, root);
            }
            None => {
                // A few, for siblings that alternate (zebra rows).
                let mut recent = self.recent.borrow_mut();
                let list = recent.entry(key).or_default();
                list.insert(0, id);
                list.truncate(4);
            }
        }
    }

    /// `parent_changed`: the parent passes down something different from last time.
    /// `in_subtree`: an ancestor's subtree is being redone.
    fn finish(self) -> Restyled {
        Restyled {
            changed: self.changed.into_inner(),
            visited: self.done.into_inner(),
        }
    }

    /// A node that needn't be redone keeps its styles, and so does its subtree.
    fn visit(
        &self,
        id: u64,
        parent: &Inherited,
        parent_changed: bool,
        in_subtree: bool,
        styles: &mut IdMap<Rc<Computed>>,
    ) {
        let node = self.tree.get(id);
        if node.is_text() {
            return;
        }
        let in_subtree = in_subtree || self.all || self.subtrees.contains(&id);
        let redo = in_subtree || parent_changed || self.nodes.contains(&id);
        let passed = self.cache.passed.borrow().get(&id).cloned();
        if !redo && passed.is_some() && styles.contains_key(&id) {
            return;
        }
        if !self.done.borrow_mut().insert(id) {
            return;
        }
        let generated = [pseudo_id(id, Pseudo::Before), pseudo_id(id, Pseudo::After)];
        let old = generated.map(|g| styles.remove(&g));
        let was = styles.remove(&id);
        let twin = self.twin(id, parent, styles);
        let inherited = match twin {
            Some(twin) => {
                for (from, to) in [(twin, id), (pseudo_id(twin, Pseudo::Before), generated[0]), (pseudo_id(twin, Pseudo::After), generated[1])] {
                    if let Some(c) = styles.get(&from).cloned() {
                        styles.insert(to, c);
                    }
                }
                self.cache.passed.borrow()[&twin].clone()
            }
            None => self.node(id, parent, styles),
        };
        self.shared(id, twin);
        let same = |a: Option<&Computed>, b: Option<&Computed>| match (a, b) {
            (Some(a), Some(b)) => same_layout(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        let same = same(was.as_deref(), styles.get(&id).map(|c| &**c))
            && old.iter().zip(&generated).all(|(o, g)| same(o.as_deref(), styles.get(g).map(|c| &**c)));
        if !same {
            self.changed.borrow_mut().push(id);
        }
        let changed = passed.is_none_or(|p| !p.same(&inherited));
        self.cache.passed.borrow_mut().insert(id, inherited.clone());
        // Children that restyle anyway are roots of their own (`restyle_in_place`).
        if !changed && !in_subtree {
            return;
        }
        for &child in &node.children {
            self.visit(child, &inherited, changed, in_subtree, styles);
        }
    }

    /// Matches and cascades one node; returns what it passes down to its children.
    fn node(&self, id: u64, parent: &Inherited, out: &mut IdMap<Rc<Computed>>) -> Inherited {
        let node = self.tree.get(id);
        let mut base = Vec::new();
        let mut stateful = Vec::new();
        let mut pseudo: [Vec<&Entry>; 4] = Default::default();
        for e in self
            .sheet
            .candidates(&node.tag, node.props.id.as_deref(), &node.classes)
        {
            let rule = &self.sheet.rules[e.rule];
            if !media_matches(&rule.media, self.env)
                || !self.containers_match(&rule.containers, parent)
            {
                continue;
            }
            // shortcut: a generated box's state rules (`hover:after:`) don't apply.
            if let Some(kind) = e.pseudo {
                if e.states == 0 && select::matches_entry(e, self.tree, id, 0, None) {
                    pseudo[kind as usize].push(e);
                }
                continue;
            }
            if e.states != 0 {
                stateful.push(e);
            } else if select::matches_entry(e, self.tree, id, 0, None) {
                base.push(e);
            }
        }
        // Looked up by `&str` first: a list's rows share a few texts, and copying each to probe
        // the map cost a String per node.
        let inline = node.props.style.as_deref().map(|text| {
            let mut parsed = self.cache.inline.borrow_mut();
            if let Some(block) = parsed.get(text) {
                return block.clone();
            }
            let block = Rc::new(parse_inline(text));
            parsed.insert(text.to_owned(), block.clone());
            block
        });

        let Cascaded {
            style,
            disp,
            mut inherited,
            fit_block,
            transition,
            fixed,
            sticky,
            field_grows,
            app_region,
            z_index,
            fill,
            stroke,
            presentation,
            animation,
            container,
            content: _,
            counters,
            image,
            list,
            table,
            text_shadow,
            border_sides,
            bg_image,
            bg_layers,
            order,
        } = self.memo_cascade(&base, &inline, parent);
        inherited.layout = ParentLayout::of(disp, &style);
        inherited.containers = match &container {
            Some(names) => {
                let mut chain = (*parent.containers).clone();
                chain.push((id, names.clone()));
                Rc::new(chain)
            }
            None => parent.containers.clone(),
        };
        // shortcut: only the nearest such ancestor counts; a second `group-hover` level is ignored.
        let group = stateful
            .iter()
            // Only those hovering an ancestor can find a group; walking up for `hover:` ones can't.
            .filter(|e| e.ancestor_states & HOVER != 0)
            .find_map(|e| {
                let mut a = self.tree.get(id).parent;
                while let Some(p) = a {
                    if select::matches_entry(e, self.tree, id, 0, Some(p)) {
                        return Some(p);
                    }
                    a = self.tree.get(p).parent;
                }
                None
            });
        // The element is inside its group, so hovering it hovers the group too.
        let with_group = |states: States, group: Option<u64>| {
            let wanted = if group.is_some() {
                states | HOVER
            } else {
                states
            };
            let extra: Vec<&Entry> = stateful
                .iter()
                .copied()
                .filter(|e| {
                    e.states & wanted != 0 && select::matches_entry(e, self.tree, id, states, group)
                })
                .collect();
            if extra.is_empty() {
                return None;
            }
            let all: Vec<&Entry> = base.iter().copied().chain(extra).collect();
            Some(self.memo_cascade(&all, &inline, parent))
        };
        let with_state = |states: States| with_group(states, group.filter(|_| states & HOVER != 0));
        let (hover, hover_paints) = match with_state(HOVER) {
            Some(c) => (Some(Box::new(c.style)), Some((c.fill, c.stroke))),
            None => (None, None),
        };
        let focus = with_state(FOCUS).map(|c| Box::new(c.style));
        let hover_focus = match hover.is_some() && focus.is_some() {
            true => with_state(HOVER | FOCUS).map(|c| Box::new(c.style)),
            false => None,
        };
        let focus_visible = match stateful.iter().any(|e| e.states & FOCUS_VISIBLE != 0) {
            true => with_state(FOCUS | FOCUS_VISIBLE).map(|c| Box::new(c.style)),
            false => None,
        };
        let computed = Computed {
            color: inherited.color,
            case: inherited.case,
            ws: inherited.ws,
            no_pointer: inherited.no_pointer,
            backdrop: inherited.backdrop,
            disp,
            hover,
            active: with_state(HOVER | ACTIVE).map(|c| Box::new(c.style)),
            focus,
            hover_focus,
            focus_visible,
            group: group.and_then(|g| Some((g, Box::new(with_group(0, Some(g))?.style)))),
            base: style,
            fit_block,
            transition,
            fixed,
            sticky,
            field_grows,
            app_region,
            z_index,
            fill,
            stroke,
            hover_paints,
            vars: inherited.vars.clone(),
            container: container.is_some(),
            content: None,
            counters,
            presentation,
            animation,
            image,
            list,
            table,
            text_shadow,
            border_sides,
            bg_image,
            bg_layers,
            order,
            placeholder: None,
            backdrop_box: None,
        };
        let mut computed = computed;
        // An SVG `opacity` attribute is a style below every CSS rule (presentation attributes are),
        // so a CSS transition animates it: MUI X Charts fades pie slices by attribute.
        if let Some(o) = node.props.attrs.get("opacity").and_then(|v| v.trim().parse().ok()) {
            let states = [&mut computed.hover, &mut computed.active, &mut computed.focus];
            let states = states.into_iter().flatten().map(|s| &mut **s);
            for s in std::iter::once(&mut computed.base).chain(states) {
                s.opacity.get_or_insert(o);
            }
        }
        let placeholder = &pseudo[Pseudo::Placeholder as usize];
        computed.placeholder = (!placeholder.is_empty())
            .then(|| self.memo_cascade(placeholder, &None, &inherited).inherited.color);
        let backdrop = &pseudo[Pseudo::Backdrop as usize];
        computed.backdrop_box = (node.tag == "dialog" && !backdrop.is_empty())
            .then(|| Rc::new(generated(self.memo_cascade(backdrop, &None, &inherited))));
        out.insert(id, Rc::new(computed));
        // Generated boxes inherit from the element, so they cascade after it.
        for (kind, entries) in [Pseudo::Before, Pseudo::After].into_iter().zip(&pseudo) {
            if entries.is_empty() {
                continue;
            }
            let c = self.memo_cascade(entries, &None, &inherited);
            if c.content.is_none() || c.disp == Disp::None {
                continue; // Without `content` there is no box.
            }
            out.insert(pseudo_id(id, kind), Rc::new(generated(c)));
        }
        inherited
    }

    /// `@container` conditions, each against the nearest ancestor container it names, at its size
    /// from the last layout; before its first layout, the window's size stands in.
    fn containers_match(&self, queries: &[ContainerQuery], parent: &Inherited) -> bool {
        queries.iter().all(|q| {
            let named = |(_, names): &&(u64, Vec<String>)| {
                q.name.as_ref().is_none_or(|n| names.contains(n))
            };
            // No such container: the condition is unknown, which doesn't match.
            let Some((id, _)) = parent.containers.iter().rev().find(named) else {
                return false;
            };
            let env = match self.sizes.get(id) {
                Some(&(width, height)) => Env {
                    width,
                    height,
                    ..*self.env
                },
                None => *self.env,
            };
            media_matches(std::slice::from_ref(&q.query), &env)
        })
    }

    fn memo_cascade(
        &self,
        matched: &[&Entry],
        inline: &Option<Rc<DeclarationBlock<'static>>>,
        parent: &Inherited,
    ) -> Cascaded {
        let key = MemoKey {
            entries: matched
                .iter()
                .map(|&e| std::ptr::from_ref(e) as usize)
                .collect(),
            inline: inline.as_ref().map_or(0, |b| Rc::as_ptr(b) as usize),
            vars: Rc::as_ptr(&parent.vars) as usize,
            color: hsla_bits(parent.color),
            case: parent.case,
            ws: parent.ws,
            no_pointer: parent.no_pointer,
            font_px: parent.font_px.to_bits(),
            ch: parent.ch.to_bits(),
            backdrop: hsla_bits(parent.backdrop),
            layout: parent.layout,
            grid_areas: parent.grid_areas.as_ref().map_or(0, |a| Rc::as_ptr(a) as usize),
        };
        if let Some(hit) = self.cache.cascades.borrow().get(&key) {
            return hit.clone();
        }
        let empty = DeclarationBlock::default();
        let cascaded = self.cascade(matched, inline.as_deref().unwrap_or(&empty), parent);
        self.cache
            .cascades
            .borrow_mut()
            .insert(key, cascaded.clone());
        cascaded
    }

    fn cascade(
        &self,
        matched: &[&Entry],
        inline: &DeclarationBlock<'static>,
        parent: &Inherited,
    ) -> Cascaded {
        // Sorted ascending, later wins: normal < inline < !important (earlier layers win) < inline !important.
        let mut decls: Vec<(CascadeKey, &Property<'static>)> = Vec::new();
        for e in matched {
            let r = &self.sheet.rules[e.rule];
            decls.extend(
                r.decls
                    .iter()
                    .map(|d| ((0, r.layer, e.specificity, r.order), d)),
            );
            decls.extend(
                r.important
                    .iter()
                    .map(|d| ((2, u32::MAX - r.layer, e.specificity, r.order), d)),
            );
        }
        decls.extend(inline.declarations.iter().map(|d| ((1, 0, 0, 0), d)));
        decls.extend(
            inline
                .important_declarations
                .iter()
                .map(|d| ((3, 0, 0, 0), d)),
        );
        decls.sort_by_key(|(k, _)| *k);

        // Custom properties first: every other value may reference them.
        let mut vars = parent.vars.clone();
        let mut local = HashMap::new();
        for (_, d) in &decls {
            let Property::Custom(c) = d else { continue };
            let name = c.name.as_ref();
            if !name.starts_with("--") {
                continue; // An unknown property, not a variable.
            }
            let value = value_text(d);
            let non_inherited = self.sheet.registered.get(name).is_some_and(|r| !r.inherits);
            match value.trim() {
                // `initial` is the guaranteed-invalid value: `var()` falls back to its fallback or @property initial.
                "initial" => {
                    local.remove(name);
                    if vars.contains_key(name) {
                        Rc::make_mut(&mut vars).remove(name);
                    }
                }
                "inherit" | "unset" => {}
                _ if non_inherited => {
                    local.insert(name.to_owned(), value);
                }
                _ => {
                    Rc::make_mut(&mut vars).insert(name.to_owned(), value);
                }
            }
        }
        let lookup = |name: &str| {
            local
                .get(name)
                .or_else(|| vars.get(name))
                .map(String::as_str)
                .or_else(|| self.sheet.registered.get(name)?.initial.as_deref())
        };

        let resolved = resolve(decls.iter().map(|(_, d)| *d), &lookup, parent.backdrop);

        let ctx = Ctx {
            case: parent.case,
            ws: parent.ws,
            no_pointer: parent.no_pointer,
            font_px: parent.font_px,
            ch: parent.ch,
            parent_font_px: parent.font_px,
            color: parent.color,
            parent_color: parent.color,
            vw: self.env.width,
            vh: self.env.height,
            dark: self.env.dark,
            backdrop: parent.backdrop,
        };
        let mut b = Builder::new(ctx, self.fonts);
        b.parent_areas = parent.grid_areas.clone();
        let rest: Vec<&Property> = resolved.iter().filter(|p| !b.apply_early(p)).collect();
        for p in rest {
            b.apply(p);
        }
        let ctx = b.ctx;
        let field_grows = b.field_grows;
        let (bg, gradient_mid, fit, transition, fixed, sticky, app_region) = (
            b.bg,
            b.gradient_mid,
            b.fit_width,
            b.transition(),
            b.fixed,
            b.sticky,
            b.app_region,
        );
        let container = b.contain_inline.then(|| b.container_names.clone());
        let (z_index, fill, stroke, animation) =
            (b.z_index, b.fill.take(), b.stroke.take(), b.animation());
        let content = b.content.clone();
        let counters = b.counters.take().map(Rc::new);
        let presentation = std::mem::take(&mut b.presentation);
        let grid_areas = b.grid_areas.take();
        let (image, order, list, table, text_shadow) = (b.image, b.order, b.list.clone(), b.table, b.text_shadow);
        let bg_layers = b.bg_layers().into();
        let border_sides = b.border_sides();
        let bg_image = b.bg_url.take().map(|url| (url, b.bg_place));
        let (mut style, disp) = b.finish();
        // Keyframes resolve against this element: its variables (tw-animate-css's
        // `--tw-enter-opacity`...) and its font size and color.
        let animation = animation.and_then(|mut a| {
            let frames = self.sheet.keyframes.get(&a.name)?;
            a.frames = frames
                .iter()
                .map(|(offset, block)| {
                    let props = resolve(block.declarations.iter(), &lookup, parent.backdrop);
                    let mut kb = Builder::new(ctx, self.fonts);
                    let rest: Vec<&Property> =
                        props.iter().filter(|p| !kb.apply_early(p)).collect();
                    for p in rest {
                        kb.apply(p);
                    }
                    (*offset, Look::of(&kb.finish().0))
                })
                .collect();
            Some(a)
        });
        let mut fit_block = false;
        if fit {
            match parent.layout {
                ParentLayout::FlexColumn { stretch: true } if style.align_self.is_none() => {
                    style.align_self = Some(AlignItems::Start)
                }
                ParentLayout::Grid { stretch: true } if style.justify_self.is_none() => {
                    style.justify_self = Some(AlignItems::Start)
                }
                ParentLayout::Flow => fit_block = !disp.is_inline_level(),
                _ => {}
            }
        }
        let backdrop = gradient_mid
            .or(bg)
            .map_or(parent.backdrop, |c| over(c, parent.backdrop));
        // gpui paints a box-shadow under the whole box, so a see-through box would show it inside.
        // Filling it with what is behind it looks the same and keeps the shadow outside, as in CSS.
        let see_through =
            style.background.is_none() || (gradient_mid.is_none() && bg.is_some_and(|c| c.a < 1.0));
        if see_through && style.box_shadow.as_ref().is_some_and(|s| !s.is_empty()) {
            style.background = Some(backdrop.into());
        }
        Cascaded {
            fixed,
            sticky,
            field_grows,
            app_region,
            z_index,
            fill,
            stroke,
            animation,
            style,
            disp,
            inherited: Inherited {
                vars,
                // Set per node after the (shared) cascade: it names the node itself.
                containers: Rc::default(),
                color: ctx.color,
                case: ctx.case,
                ws: ctx.ws,
                no_pointer: ctx.no_pointer,
                font_px: ctx.font_px,
                ch: ctx.ch,
                backdrop,
                layout: parent.layout,
                grid_areas,
            },
            fit_block,
            transition,
            container,
            content,
            counters,
            presentation,
            image,
            list,
            table,
            text_shadow,
            border_sides,
            bg_image,
            bg_layers,
            order,
        }
    }
}

/// A pseudo-element's box (`::before`, `::after`, `::backdrop`) from its cascade: it has no states.
fn generated(c: Cascaded) -> Computed {
    Computed {
        color: c.inherited.color,
        case: c.inherited.case,
        ws: c.inherited.ws,
        no_pointer: c.inherited.no_pointer,
        backdrop: c.inherited.backdrop,
        disp: c.disp,
        hover: None,
        active: None,
        focus: None,
        hover_focus: None,
        focus_visible: None,
        group: None,
        base: c.style,
        fit_block: c.fit_block,
        transition: c.transition,
        fixed: c.fixed,
        sticky: c.sticky,
        field_grows: false,
        app_region: c.app_region,
        z_index: c.z_index,
        fill: c.fill,
        stroke: c.stroke,
        hover_paints: None,
        vars: c.inherited.vars,
        container: false,
        content: c.content,
        counters: c.counters,
        presentation: c.presentation,
        animation: c.animation,
        image: c.image,
        list: c.list,
        table: c.table,
        text_shadow: c.text_shadow,
        border_sides: c.border_sides,
        bg_image: c.bg_image,
        bg_layers: c.bg_layers,
        order: c.order,
        placeholder: None,
        backdrop_box: None,
    }
}

/// The id of an element's `::before`/`::after` box: host-only, never sent by JS (whose ids are
/// small), so it can't clash. Its text child is `pseudo_id(..) | PSEUDO_TEXT`.
pub fn pseudo_id(id: u64, kind: Pseudo) -> u64 {
    id | PSEUDO
        | if kind == Pseudo::After {
            PSEUDO_AFTER
        } else {
            0
        }
}
pub const PSEUDO: u64 = 1 << 62;
const PSEUDO_AFTER: u64 = 1 << 61;
pub const PSEUDO_TEXT: u64 = 1 << 60;

/// `top` composited over an opaque `bottom`.
pub(super) fn over(top: Hsla, bottom: Hsla) -> Hsla {
    let (t, b) = (Rgba::from(top), Rgba::from(bottom));
    let mix = |t: f32, b: f32| t * top.a + b * (1.0 - top.a);
    Rgba {
        r: mix(t.r, b.r),
        g: mix(t.g, b.g),
        b: mix(t.b, b.b),
        a: 1.0,
    }
    .into()
}

fn parse_inline(text: &str) -> DeclarationBlock<'static> {
    match StyleAttribute::parse(text, ParserOptions::default()) {
        Ok(attr) => attr.declarations.into_owned(),
        Err(e) => {
            eprintln!("[gpuijs] invalid inline style {text:?}: {e}");
            DeclarationBlock::default()
        }
    }
}

/// Declarations in cascade order, `var()`s substituted and shorthands expanded to longhands.
/// `parent_bg` is what `background: inherit` takes.
fn resolve<'a>(
    decls: impl Iterator<Item = &'a Property<'static>>,
    lookup: &dyn Fn(&str) -> Option<&'a str>,
    parent_bg: Hsla,
) -> Vec<Property<'static>> {
    let mut resolved = Vec::new();
    for d in decls {
        match d {
            // Unknown to lightningcss, but how Tailwind's `line-clamp-*` limits lines, what lets
            // presses through an overlay, what makes a custom title bar draggable, and how tables
            // lay out.
            Property::Custom(c)
                if matches!(
                    c.name.as_ref(),
                    "-webkit-line-clamp"
                        | "line-clamp"
                        | "pointer-events"
                        | "app-region"
                        | "-webkit-app-region"
                        | "text-anchor"
                        | "dominant-baseline"
                        | "border-collapse"
                        | "table-layout"
                        | "field-sizing"
                ) =>
            {
                resolved.push(d.clone())
            }
            // Unknown to lightningcss, so not an unparsed `var()` either: Tailwind's `before:` and
            // `after:` give `content: var(--tw-content)`, and `tabular-nums` a list of `var()`s.
            Property::Custom(c)
                if matches!(
                    c.name.as_ref(),
                    "content"
                        | "font-variant-numeric"
                        | "font-feature-settings"
                        | "object-fit"
                        | "object-position"
                        | "outline-offset"
                        | "text-underline-offset"
                        | "text-wrap"
                        | "text-wrap-mode"
                        | "text-wrap-style"
                        | "counter-reset"
                        | "counter-set"
                        | "counter-increment"
                ) =>
            {
                if let Some(text) = substitute(&value_text(d), lookup, 0)
                    && let Ok(p) =
                        Property::parse_string(d.property_id(), &text, ParserOptions::default())
                {
                    resolved.push(p.into_owned());
                }
            }
            // Unknown to lightningcss and not drawn, but changing what a page looks like: reported.
            Property::Custom(c)
                if matches!(
                    c.name.as_ref(),
                    "float"
                        | "columns"
                        | "column-count"
                        | "column-width"
                        | "writing-mode"
                        | "zoom"
                        | "shape-outside"
                        | "initial-letter"
                        | "-webkit-text-stroke"
                        | "background-blend-mode"
                ) =>
            {
                super::warn_once(c.name.as_ref())
            }
            Property::Custom(_) => {}
            Property::Unparsed(u) => {
                let Some(text) = substitute(&value_text(d), lookup, 0) else {
                    continue;
                };
                // shortcut: CSS-wide keywords cancel earlier declarations, so the property falls back to
                // inheriting (inherited props) or its initial value; exact for preflight's `color: inherit`.
                if matches!(
                    text.trim(),
                    "inherit" | "initial" | "unset" | "revert" | "revert-layer"
                ) {
                    let mut ids = Vec::new();
                    leaf_ids(u.property_id.clone(), &mut ids);
                    resolved.retain(|p: &Property| !ids.contains(&p.property_id()));
                    // `background: inherit` (vaul's drawer `::after` extends the drawer so).
                    // shortcut: the opaque color behind the parent's children stands in for the
                    // parent's background color, which it is when that is opaque.
                    if text.trim() == "inherit"
                        && matches!(
                            u.property_id,
                            PropertyId::Background | PropertyId::BackgroundColor
                        )
                    {
                        let c = Rgba::from(parent_bg);
                        let byte = |v: f32| (v * 255.0).round() as u8;
                        let color = CssColor::RGBA(RGBA::new(byte(c.r), byte(c.g), byte(c.b), c.a));
                        resolved.push(Property::BackgroundColor(color));
                    }
                    // `height: inherit` (Base UI's progress indicator fills its track so).
                    // shortcut: 100% stands in for the parent's size; the same for a definite one.
                    if text.trim() == "inherit"
                        && matches!(
                            u.property_id,
                            PropertyId::Height | PropertyId::Width | PropertyId::BlockSize | PropertyId::InlineSize
                        )
                        && let Ok(p) =
                            Property::parse_string(u.property_id.clone(), "100%", ParserOptions::default())
                    {
                        resolved.push(p.into_owned());
                    }
                    continue;
                }
                // lightningcss leaves `box-shadow: none` unparsed; no shadows is an empty list to it
                // (MUI's contained buttons drop their hover shadow with one).
                if let PropertyId::BoxShadow(prefix) = &u.property_id
                    && text.trim() == "none"
                {
                    resolved.push(Property::BoxShadow(Default::default(), *prefix));
                    continue;
                }
                // `dense` alone means `row dense`, which lightningcss doesn't take.
                let text = match (&u.property_id, text.trim()) {
                    (PropertyId::GridAutoFlow, "dense") => "row dense".into(),
                    _ => text,
                };
                if u.property_id == PropertyId::TextShadow && text.trim() == "none" {
                    resolved.push(Property::TextShadow(Default::default()));
                    continue;
                }
                // Still unparsed after substitution means invalid or a CSS-wide keyword: the declaration is dropped.
                let parse = |text: &str| {
                    Property::parse_string(u.property_id.clone(), text, ParserOptions::default())
                        .ok()
                        .filter(|p| !matches!(p, Property::Unparsed(_)))
                        .map(|p| p.into_owned())
                };
                if let Some(p) = parse(&text) {
                    expand(p, &mut resolved);
                } else if let Some((text, oklab)) = without_interpolation(&text)
                    && let Some(p) = parse(&text)
                {
                    // Read by the gradient that follows it (see `Builder::background_image`).
                    if oklab {
                        resolved.push(Property::Custom(CustomProperty {
                            name: CustomPropertyName::from(CowArcStr::from(OKLAB_GRADIENT)),
                            value: TokenList(Vec::new()),
                        }));
                    }
                    expand(p, &mut resolved);
                }
            }
            p => expand(p.clone(), &mut resolved),
        }
    }
    resolved
}

/// `text` with gradients' interpolation color spaces removed (`linear-gradient(to top in oklab, …)`
/// becomes `linear-gradient(to top, …)`): lightningcss can't parse them, and Tailwind v4 writes one
/// into every gradient. `None` when there are none; else whether one was Oklab (or Oklch).
// shortcut: Oklch interpolates as Oklab, and other spaces as sRGB; only a slightly different midpoint.
fn without_interpolation(text: &str) -> Option<(String, bool)> {
    let mut oklab = false;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("gradient(") {
        let (head, args) = rest.split_at(i + "gradient(".len());
        out.push_str(head);
        // The interpolation is in the first argument, before any color stop.
        let first = args.find([',', ')']).unwrap_or(args.len());
        let line = &args[..first];
        let kept = match line
            .find(" in ")
            .map(|j| j + 1)
            .or(line.starts_with("in ").then_some(0))
        {
            Some(j) => {
                oklab |= line[j..].contains("oklab") || line[j..].contains("oklch");
                line[..j].trim_end()
            }
            None => line,
        };
        out.push_str(kept);
        rest = &args[first..];
        // Nothing left of the first argument (`in oklab,`): its comma goes too.
        if kept.trim().is_empty() && rest.starts_with(',') {
            rest = rest[1..].trim_start();
        }
    }
    out.push_str(rest);
    (out != text).then_some((out, oklab))
}

/// The marker property put before a background whose gradients interpolate in Oklab.
pub(super) const OKLAB_GRADIENT: &str = "-gpuijs-oklab-gradient";

/// Shorthands become longhands so the mapping only handles one spelling of each property.
fn expand(p: Property<'static>, out: &mut Vec<Property<'static>>) {
    match p.property_id().longhands() {
        Some(ids) if !ids.is_empty() => {
            for id in ids {
                // lightningcss doesn't hand out the thickness of `text-decoration`.
                let thickness = || match &p {
                    Property::TextDecoration(d, _) if id == PropertyId::TextDecorationThickness => {
                        Some(Property::TextDecorationThickness(d.thickness.clone()))
                    }
                    _ => None,
                };
                if let Some(l) = p.longhand(&id).or_else(thickness) {
                    expand(l, out);
                }
            }
        }
        _ => out.push(p),
    }
}

/// The longhands a property expands to, as `expand` would push them.
fn leaf_ids(id: PropertyId<'static>, out: &mut Vec<PropertyId<'static>>) {
    match id.longhands() {
        Some(ids) if !ids.is_empty() => ids.into_iter().for_each(|l| leaf_ids(l, out)),
        _ => out.push(id),
    }
}

/// Inlines `var(--x, fallback)` references; `None` when one is unresolvable (the declaration is invalid).
pub fn substitute<'a>(
    text: &str,
    lookup: &dyn Fn(&str) -> Option<&'a str>,
    depth: u8,
) -> Option<String> {
    if depth > 32 {
        return None; // A reference cycle.
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let body = &rest[start + 4..];
        let end = closing_paren(body)?;
        let inner = &body[..end];
        let (name, fallback) = match top_level_comma(inner) {
            Some(i) => (&inner[..i], Some(&inner[i + 1..])),
            None => (inner, None),
        };
        let value = lookup(name.trim()).or(fallback)?;
        rest = &body[end + 1..];
        // Substitution works on tokens, so a value never fuses with its neighbours: minified CSS
        // like `var(--d,.15s)var(--e,ease)` must become `.15s ease`, not `.15sease`.
        let joins = |c: char| !(c.is_whitespace() || matches!(c, '(' | ')' | ','));
        if out.ends_with(joins) {
            out.push(' ');
        }
        out.push_str(&substitute(value.trim(), lookup, depth + 1)?);
        if rest.starts_with(joins) {
            out.push(' ');
        }
    }
    out.push_str(rest);
    Some(out)
}

fn closing_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}
