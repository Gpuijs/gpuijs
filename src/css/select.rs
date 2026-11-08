//! Lets lightningcss's selector engine match against our tree.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;

use lightningcss::rules::CssRule;
use lightningcss::selector::{Direction, PseudoClass, PseudoElement, Selector};
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::ident::Ident;
use lightningcss::values::string::{CSSString, CowArcStr};
use parcel_selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use parcel_selectors::context::{MatchingContext, MatchingMode, QuirksMode};
use parcel_selectors::matching::{ElementSelectorFlags, matches_selector};
use static_self::IntoOwned;
use parcel_selectors::parser::SelectorImpl;
use parcel_selectors::{Element, OpaqueElement};

/// lightningcss keeps its `SelectorImpl` type private; recover it from the public `Selector` alias.
pub trait ImplOf {
    type Impl;
}
impl<'i, I: SelectorImpl<'i>> ImplOf for parcel_selectors::parser::Selector<'i, I> {
    type Impl = I;
}
pub type Selectors = <Selector<'static> as ImplOf>::Impl;

use super::{ACTIVE, FOCUS, FOCUS_VISIBLE, HOVER, States};
use crate::tree::{HTML, IdMap, IdSet, Node, Tree};

/// The matcher panics on these instead of failing to match, so such selectors are never indexed.
pub fn unsupported(selector: &Selector) -> Option<&'static str> {
    use parcel_selectors::parser::{Component, NthType};
    selector.iter_raw_match_order().find_map(|c| match c {
        Component::Has(_) => Some(":has()"),
        Component::NthOf(_) => Some(":nth-child(… of …)"),
        Component::Nth(d) if matches!(d.ty, NthType::Col | NthType::LastCol) => Some(":nth-col()"),
        Component::Nesting => Some("& outside a nested rule"),
        Component::Is(list)
        | Component::Where(list)
        | Component::Any(_, list)
        | Component::Negation(list) => list.iter().find_map(unsupported),
        Component::Slotted(s) | Component::Host(Some(s)) => unsupported(s),
        _ => None,
    })
}

/// Whether matching reads an element's siblings or its place among them (`+`, `~`, `:nth-child()`,
/// `:first-child`...), so adding, removing or moving a sibling can change it.
pub fn reads_siblings(selector: &Selector) -> bool {
    use parcel_selectors::parser::{Combinator, Component};
    selector.iter_raw_match_order().any(|c| match c {
        Component::Combinator(Combinator::NextSibling | Combinator::LaterSibling)
        | Component::Nth(_)
        | Component::NthOf(_) => true,
        Component::Is(list)
        | Component::Where(list)
        | Component::Any(_, list)
        | Component::Negation(list) => list.iter().any(reads_siblings),
        _ => false,
    })
}

/// The states a selector needs of the element's ancestors or siblings rather than of the element:
/// `.group:hover .x` and Tailwind's `.x:is(:where(.group):hover *)` have hover; `.x:hover` none.
pub fn ancestor_states(selector: &Selector) -> States {
    use parcel_selectors::parser::Component;
    let mut past_subject = false;
    selector.iter_raw_match_order().fold(0, |states, c| {
        past_subject |= matches!(c, Component::Combinator(_));
        states
            | match c {
                _ if past_subject => states_in_component(c),
                Component::Is(list) | Component::Where(list) | Component::Any(_, list) => {
                    list.iter().fold(0, |s, sel| s | ancestor_states(sel))
                }
                _ => 0,
            }
    })
}

/// `states_in` for one component.
fn states_in_component(c: &parcel_selectors::parser::Component<Selectors>) -> States {
    use parcel_selectors::parser::Component;
    match c {
        Component::NonTSPseudoClass(pc) => state_of(pc),
        Component::Is(list) | Component::Where(list) | Component::Any(_, list) => {
            list.iter().fold(0, |s, sel| s | states_in(sel))
        }
        _ => 0,
    }
}

/// The states anywhere in a selector, ancestors' too (`.group:hover .x`); `:not()` is left out, as a
/// negated state matches without simulating it.
pub fn states_in(selector: &Selector) -> States {
    use parcel_selectors::parser::Component;
    selector.iter_raw_match_order().fold(0, |states, c| {
        states
            | match c {
                Component::NonTSPseudoClass(pc) => state_of(pc),
                Component::Is(list) | Component::Where(list) | Component::Any(_, list) => {
                    list.iter().fold(0, |s, sel| s | states_in(sel))
                }
                _ => 0,
            }
    })
}

/// The state pseudo-class bits a pseudo-class needs.
pub fn state_of(pc: &PseudoClass) -> States {
    match pc {
        PseudoClass::Hover => HOVER,
        PseudoClass::Active => ACTIVE,
        PseudoClass::Focus | PseudoClass::FocusWithin => FOCUS,
        PseudoClass::FocusVisible => FOCUS_VISIBLE,
        _ => 0,
    }
}

#[derive(Clone, Copy)]
pub struct El<'a> {
    tree: &'a Tree,
    id: u64,
    /// The element whose hover/active/focus is simulated; only it ever matches those states.
    subject: u64,
    states: States,
    /// An ancestor simulated as hovered too, for `.group:hover .x` (Tailwind's `group-hover:`).
    group: Option<u64>,
}

impl fmt::Debug for El<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "El({})", self.id)
    }
}

impl<'a> El<'a> {
    fn node(&self) -> &'a Node {
        self.tree.get(self.id)
    }

    fn at(&self, id: u64) -> Self {
        El { id, ..*self }
    }

    fn sibling(&self, step: isize) -> Option<Self> {
        let parent = self.tree.get(self.node().parent?);
        let mut i = parent.children.iter().position(|&c| c == self.id)? as isize;
        loop {
            i += step;
            let &id = parent.children.get(usize::try_from(i).ok()?)?;
            if !self.tree.get(id).is_text() {
                return Some(self.at(id));
            }
        }
    }

    /// The nearest `dir="ltr"`/`dir="rtl"` up the tree; `dir="auto"` reads as the default, ltr.
    fn direction(&self) -> Direction {
        let mut el = Some(*self);
        while let Some(e) = el {
            match e.attr("dir") {
                Some(d) if d.eq_ignore_ascii_case("rtl") => return Direction::Rtl,
                Some(d) if d.eq_ignore_ascii_case("ltr") => return Direction::Ltr,
                _ => el = e.node().parent.map(|p| e.at(p)),
            }
        }
        Direction::Ltr
    }

    /// A text field, or an element in `contenteditable` content.
    fn writable(&self) -> bool {
        let node = self.node();
        if matches!(node.tag.as_str(), "input" | "textarea") {
            return !node.props.disabled && !node.props.attrs.contains_key("readonly");
        }
        if self.tree.editable.is_empty() {
            return false;
        }
        let mut el = Some(*self);
        while let Some(e) = el {
            match e.attr("contenteditable").map(str::to_ascii_lowercase).as_deref() {
                Some("false") => return false,
                Some("inherit") | None => el = e.node().parent.map(|p| e.at(p)),
                Some(v) => return matches!(v, "" | "true" | "plaintext-only"),
            }
        }
        false
    }

    fn attr(&self, name: &str) -> Option<&'a str> {
        let props = &self.node().props;
        match name {
            "id" => props.id.as_deref(),
            "class" => Some(props.class.as_str()),
            _ => props.attrs.get(name).map(String::as_str),
        }
    }
}

/// How a `:has()` argument relates to the subject: `:has(x)`, `:has(> x)`, `:has(+ x)`, `:has(~ x)`,
/// or `Scoped` for an argument with more combinators (`:has(> x ~ y)`): its selector then starts
/// with `:scope`, matched against the subject, and anything after the subject may be its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Relation {
    Descendant,
    Child,
    Next,
    Later,
    Scoped,
}

/// Every group (one `:has()`) needs at least one related element matching one of its selectors.
pub type Has = Vec<Vec<(Relation, Selector<'static>)>>;

pub fn matches_entry(
    e: &super::Entry,
    tree: &Tree,
    id: u64,
    states: States,
    group: Option<u64>,
) -> bool {
    // A pseudo-element's rules match its originating element, the pseudo part set aside.
    let mode = match e.pseudo {
        Some(_) => MatchingMode::ForStatelessPseudoElement,
        None => MatchingMode::Normal,
    };
    matches_in(mode, &e.selector, tree, id, states, group)
        && e.has.iter().all(|group| {
            group.iter().any(|(rel, sel)| {
                let scope = (*rel == Relation::Scoped).then_some(id);
                related(tree, id, *rel)
                    .into_iter()
                    .any(|r| matches_scoped(sel, tree, r, scope))
            })
        })
}

fn related(tree: &Tree, id: u64, rel: Relation) -> Vec<u64> {
    let elements = |ids: &[u64]| -> Vec<u64> {
        ids.iter()
            .copied()
            .filter(|&c| !tree.get(c).is_text())
            .collect()
    };
    let descendants = |ids: Vec<u64>| {
        let mut out = Vec::new();
        let mut stack = ids;
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(elements(&tree.get(n).children));
        }
        out
    };
    match rel {
        Relation::Child => elements(&tree.get(id).children),
        Relation::Descendant => descendants(elements(&tree.get(id).children)),
        Relation::Scoped => {
            let mut roots = elements(&tree.get(id).children);
            roots.extend(related(tree, id, Relation::Later));
            descendants(roots)
        }
        Relation::Next | Relation::Later => {
            let Some(parent) = tree.get(id).parent else {
                return Vec::new();
            };
            let siblings = elements(&tree.get(parent).children);
            let after = siblings
                .iter()
                .position(|&s| s == id)
                .map_or(&[][..], |i| &siblings[i + 1..]);
            match rel {
                Relation::Next => after.iter().take(1).copied().collect(),
                _ => after.to_vec(),
            }
        }
    }
}

thread_local! {
    /// `:has()` argument lists that `rewrite_has` turned into `:-gpuijs-has-N`, by N, and N by
    /// their text (a restyled sheet reuses its numbers).
    static HAS_GROUPS: RefCell<(Vec<Vec<(Relation, Selector<'static>)>>, HashMap<String, usize>)> =
        RefCell::default();
    /// Elements styled by a `:-gpuijs-has-N` evaluated on another element, by that element: when
    /// something under it changes, they restyle too.
    static HAS_DEPENDENTS: RefCell<IdMap<IdSet>> = RefCell::default();
}

const HAS_PLACEHOLDER: &str = "-gpuijs-has-";

/// `selector` with each `:has(...)` replaced by a placeholder pseudo-class the matcher evaluates
/// itself, for `:has()` the engine can't take: on an ancestor compound (`.a:has(> b) > input`) or
/// inside `:is()`/`:not()` (Tailwind's `group-has-*`). `None` when an argument doesn't parse.
pub fn rewrite_has(selector: &Selector<'static>) -> Option<Selector<'static>> {
    let text: Vec<char> = selector
        .to_css_string(PrinterOptions::default())
        .ok()?
        .chars()
        .collect();
    let mut out = String::new();
    let mut i = 0;
    while i < text.len() {
        if text[i] == '\\' {
            out.extend(&text[i..(i + 2).min(text.len())]);
            i += 2;
            continue;
        }
        if text[i..].starts_with(&[':', 'h', 'a', 's', '(']) {
            let end = i + 5 + closing(&text[i + 5..])?;
            let args: String = text[i + 5..end].iter().collect();
            let n = HAS_GROUPS.with(|g| {
                let (groups, numbers) = &mut *g.borrow_mut();
                if let Some(&n) = numbers.get(&args) {
                    return Some(n);
                }
                groups.push(relative_list(&args)?);
                numbers.insert(args, groups.len() - 1);
                Some(groups.len() - 1)
            })?;
            out.push_str(&format!(":{HAS_PLACEHOLDER}{n}"));
            i = end + 1;
            continue;
        }
        out.push(text[i]);
        i += 1;
    }
    parse_selector(&out)
}

/// Whether some rewritten `:has()` reads the siblings of the element it is evaluated on.
pub fn has_reads_siblings() -> bool {
    HAS_GROUPS.with(|g| {
        g.borrow().0.iter().flatten().any(|(rel, _)| *rel != Relation::Descendant && *rel != Relation::Child)
    })
}

/// The elements whose style read a rewritten `:has()` on `anchor`.
pub fn has_dependents(anchor: u64) -> Vec<u64> {
    HAS_DEPENDENTS.with(|d| d.borrow().get(&anchor).map(|s| s.iter().copied().collect()).unwrap_or_default())
}

/// Whether `id` has a related element matching group `n` (one `:has()`), as `matches_entry` checks
/// a subject's groups.
fn matches_has_group(tree: &Tree, id: u64, n: usize) -> bool {
    HAS_GROUPS.with(|g| {
        let groups = g.borrow();
        groups.0.get(n).is_some_and(|group| {
            group.iter().any(|(rel, sel)| {
                let scope = (*rel == Relation::Scoped).then_some(id);
                related(tree, id, *rel)
                    .into_iter()
                    .any(|r| matches_scoped(sel, tree, r, scope))
            })
        })
    })
}

/// Splits `subject:has(rel, ...)` into the subject and its `:has()` groups. `None` when a `:has()` is
/// not on the subject compound or is nested in another pseudo-class.
pub fn split_has(selector: &Selector<'static>) -> Option<(Selector<'static>, Has)> {
    let text: Vec<char> = selector
        .to_css_string(PrinterOptions::default())
        .ok()?
        .chars()
        .collect();
    let (mut base, mut has) = (String::new(), Has::new());
    let (mut i, mut depth, mut seen_has) = (0, 0i32, false);
    while i < text.len() {
        let c = text[i];
        if c == '\\' {
            base.extend(&text[i..(i + 2).min(text.len())]);
            i += 2;
            continue;
        }
        if text[i..].starts_with(&[':', 'h', 'a', 's', '(']) {
            if depth > 0 {
                return None;
            }
            let end = i + 5 + closing(&text[i + 5..])?;
            has.push(relative_list(&text[i + 5..end].iter().collect::<String>())?);
            seen_has = true;
            i = end + 1;
            continue;
        }
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ' ' | '>' | '+' | '~' if depth == 0 && seen_has => return None,
            _ => {}
        }
        base.push(c);
        i += 1;
    }
    if base.is_empty() || base.ends_with([' ', '>', '+', '~']) {
        base.push('*');
    }
    Some((parse_selector(&base)?, has))
}

fn relative_list(args: &str) -> Option<Vec<(Relation, Selector<'static>)>> {
    split_top_level(args, ',')
        .into_iter()
        .map(|arg| {
            let arg = arg.trim();
            let (rel, rest) = match arg.chars().next()? {
                '>' => (Relation::Child, &arg[1..]),
                '+' => (Relation::Next, &arg[1..]),
                '~' => (Relation::Later, &arg[1..]),
                _ => (Relation::Descendant, arg),
            };
            let rest = rest.trim();
            if [' ', '>', '+', '~']
                .iter()
                .any(|&c| split_top_level(rest, c).len() > 1)
            {
                let lead = if rel == Relation::Descendant { " " } else { "" };
                return Some((Relation::Scoped, parse_selector(&format!(":scope{lead}{arg}"))?));
            }
            Some((rel, parse_selector(rest)?))
        })
        .collect()
}

/// Index of the `)` closing an already opened parenthesis.
fn closing(text: &[char]) -> Option<usize> {
    let mut depth = 1;
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            '\\' => i += 1,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let (mut parts, mut start, mut depth, mut escaped) = (Vec::new(), 0, 0i32, false);
    for (i, c) in s.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts.retain(|p| !p.trim().is_empty());
    parts
}

fn parse_selector(text: &str) -> Option<Selector<'static>> {
    let css = format!("{text}{{}}");
    let sheet = StyleSheet::parse(&css, ParserOptions::default()).ok()?;
    match sheet.rules.0.into_iter().next()? {
        CssRule::Style(rule) => rule.selectors.0.into_iter().next().map(IntoOwned::into_owned),
        _ => None,
    }
}

pub fn matches(selector: &Selector, tree: &Tree, id: u64, states: States) -> bool {
    matches_in(MatchingMode::Normal, selector, tree, id, states, None)
}

/// `matches`, with `:scope` meaning the `scope` element.
fn matches_scoped(selector: &Selector, tree: &Tree, id: u64, scope: Option<u64>) -> bool {
    let Some(scope) = scope else {
        return matches(selector, tree, id, 0);
    };
    let mut ctx = MatchingContext::new(MatchingMode::Normal, None, None, QuirksMode::NoQuirks);
    ctx.scope_element = Some(OpaqueElement::new(tree.get(scope)));
    let el = El {
        tree,
        id,
        subject: id,
        states: 0,
        group: None,
    };
    matches_selector(selector, 0, None, &el, &mut ctx, &mut |_, _| {})
}

fn matches_in(
    mode: MatchingMode,
    selector: &Selector,
    tree: &Tree,
    id: u64,
    states: States,
    group: Option<u64>,
) -> bool {
    let mut ctx = MatchingContext::new(mode, None, None, QuirksMode::NoQuirks);
    let el = El {
        tree,
        id,
        subject: id,
        states,
        group,
    };
    matches_selector(selector, 0, None, &el, &mut ctx, &mut |_, _| {})
}

impl<'a, 'i> Element<'i> for El<'a> {
    type Impl = Selectors;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(self.node())
    }

    fn parent_element(&self) -> Option<Self> {
        self.node().parent.map(|p| self.at(p))
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.sibling(-1)
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.sibling(1)
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(&self, name: &Ident<'i>) -> bool {
        self.node().tag == name.0.as_ref()
    }

    fn has_namespace(&self, _ns: &CowArcStr<'i>) -> bool {
        true
    }

    fn is_same_type(&self, other: &Self) -> bool {
        self.node().tag == other.node().tag
    }

    fn attr_matches(
        &self,
        _ns: &NamespaceConstraint<&CowArcStr<'i>>,
        local_name: &Ident<'i>,
        operation: &AttrSelectorOperation<&CSSString<'i>>,
    ) -> bool {
        self.attr(local_name.0.as_ref())
            .is_some_and(|v| operation.eval_str(v))
    }

    fn match_non_ts_pseudo_class<F>(
        &self,
        pc: &PseudoClass<'i>,
        _context: &mut MatchingContext<'_, 'i, Selectors>,
        _flags_setter: &mut F,
    ) -> bool
    where
        F: FnMut(&Self, ElementSelectorFlags),
    {
        let node = self.node();
        let state = state_of(pc);
        if state != 0 {
            return (self.id == self.subject && self.states & state != 0)
                || (state == HOVER && self.group == Some(self.id));
        }
        let form = matches!(
            node.tag.as_str(),
            "input" | "button" | "select" | "textarea"
        );
        match pc {
            PseudoClass::Disabled => form && node.props.disabled,
            PseudoClass::Checked => node.tag == "input" && node.props.attrs.contains_key("checked"),
            PseudoClass::Enabled => form && !node.props.disabled,
            PseudoClass::Required => form && node.props.attrs.contains_key("required"),
            PseudoClass::Optional => form && !node.props.attrs.contains_key("required"),
            // Text fields and editable content are writable.
            PseudoClass::ReadWrite(_) => self.writable(),
            PseudoClass::ReadOnly(_) => !self.writable(),
            PseudoClass::PlaceholderShown(_) => {
                node.props.placeholder.is_some()
                    && node.props.value.as_deref().unwrap_or("").is_empty()
            }
            PseudoClass::Link | PseudoClass::AnyLink(_) => self.is_link(),
            PseudoClass::Modal => node.tag == "dialog" && node.props.modal,
            PseudoClass::Open => {
                matches!(node.tag.as_str(), "details" | "dialog") && node.props.attrs.contains_key("open")
            }
            PseudoClass::Dir { direction } => self.direction() == *direction,
            PseudoClass::Custom { name } => match name.strip_prefix(HAS_PLACEHOLDER).and_then(|n| n.parse().ok()) {
                Some(n) => {
                    if self.id != self.subject {
                        HAS_DEPENDENTS.with(|d| d.borrow_mut().entry(self.id).or_default().insert(self.subject));
                    }
                    matches_has_group(self.tree, self.id, n)
                }
                None => false,
            },
            _ => false,
        }
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement<'i>,
        _context: &mut MatchingContext<'_, 'i, Selectors>,
    ) -> bool {
        false
    }

    fn is_link(&self) -> bool {
        self.node().tag == "a" && self.attr("href").is_some()
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &Ident<'i>, _case: CaseSensitivity) -> bool {
        self.node().props.id.as_deref() == Some(id.0.as_ref())
    }

    fn has_class(&self, name: &Ident<'i>, _case: CaseSensitivity) -> bool {
        self.node().classes.iter().any(|c| c == name.0.as_ref())
    }

    fn imported_part(&self, _name: &Ident<'i>) -> Option<Ident<'i>> {
        None
    }

    fn is_part(&self, _name: &Ident<'i>) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.node()
            .children
            .iter()
            .all(|&c| self.tree.get(c).text.as_deref() == Some(""))
    }

    fn is_root(&self) -> bool {
        self.id == HTML
    }
}
