//! Author stylesheets: flattened into an indexed rule list that the cascade matches against the tree.

pub mod cascade;
pub mod props;
pub mod select;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{Result, anyhow};
use lightningcss::declaration::DeclarationBlock;
use lightningcss::media_query::{
    MediaCondition, MediaFeature, MediaFeatureComparison, MediaFeatureValue, MediaList, MediaQuery,
    MediaType, Operator, Qualifier, QueryFeature,
};
use lightningcss::properties::Property;
use lightningcss::rules::keyframes::{KeyframeSelector, KeyframesName};
use lightningcss::rules::supports::SupportsCondition;
use lightningcss::rules::{CssRule, CssRuleList};
use lightningcss::selector::{Component, PseudoElement, Selector};
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::{Features, Targets};
use lightningcss::traits::ToCss;
use lightningcss::values::length::LengthValue;
use lightningcss::values::string::CowArcStr;
use static_self::IntoOwned;

/// Pseudo-class states a selector depends on; matched separately so gpui can apply them natively.
pub type States = u8;
pub const HOVER: States = 1;
pub const ACTIVE: States = 2;
pub const FOCUS: States = 4;
/// `:focus-visible`: focused, and the browser would show it (keyboard use, a text field).
pub const FOCUS_VISIBLE: States = 8;

#[derive(Clone)]
pub struct Rule {
    pub decls: Vec<Property<'static>>,
    pub important: Vec<Property<'static>>,
    /// Cascade layer rank; unlayered rules get `u32::MAX` so they beat every layer.
    pub layer: u32,
    pub order: u32,
    /// All of these must match (nested `@media` blocks).
    pub media: Vec<MediaList<'static>>,
    /// And all of these (nested `@container` blocks).
    pub containers: Vec<ContainerQuery>,
}

/// An `@container` condition, read as a media query that the container's size stands in the
/// viewport for. It applies to the nearest ancestor container with `name` (any, without one).
#[derive(Clone)]
pub struct ContainerQuery {
    pub name: Option<String>,
    pub query: MediaList<'static>,
}

#[derive(Clone)]
pub struct Entry {
    pub selector: Selector<'static>,
    pub specificity: u32,
    pub rule: usize,
    pub states: States,
    /// Those of `states` on an ancestor (`group-hover:`), see `select::ancestor_states`.
    pub ancestor_states: States,
    /// `:has()` conditions on the subject, checked outside the selector engine.
    pub has: select::Has,
    /// Styles this generated box of the matching element rather than the element.
    pub pseudo: Option<Pseudo>,
}

/// A box CSS generates inside an element, before or after its children.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pseudo {
    Before,
    After,
    /// Not a box: only its `color` is used, by the text field.
    Placeholder,
    /// What a modal `<dialog>` covers the page with.
    Backdrop,
}

#[derive(Clone)]
pub struct Registered {
    pub inherits: bool,
    pub initial: Option<String>,
}

/// What `@media` queries are evaluated against.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Env {
    pub width: f32,
    pub height: f32,
    pub dark: bool,
}

#[derive(Default, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub entries: Vec<Entry>,
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    universal: Vec<usize>,
    pub registered: HashMap<String, Registered>,
    layers: Vec<String>,
    /// Some rule uses `:has()`, so a change can restyle the changed node's ancestors.
    pub has_has: bool,
    /// Some rule reads siblings (`select::reads_siblings`), so a change can restyle a node's siblings.
    pub reads_siblings: bool,
    /// `@keyframes` by name: (offset in 0..=1, declarations), sorted by offset.
    pub keyframes: HashMap<String, Vec<(f32, DeclarationBlock<'static>)>>,
}

const UA_CSS: &str = include_str!("../ua.css");

impl Stylesheet {
    /// The UA defaults followed by the author CSS, as one cascade (UA rules sit in the lowest layer).
    pub fn parse(author: &str) -> Result<Stylesheet> {
        let mut sheet = Stylesheet::default();
        sheet.layer_rank("-gpuijs-ua");
        sheet.add(UA_CSS, Some("-gpuijs-ua".into()))?;
        sheet.add(author, None)?;
        Ok(sheet)
    }

    /// This cascade plus a `<style>` a library added at runtime, which comes after it in source order.
    pub fn with_injected(&self, text: &str) -> Result<Stylesheet> {
        let mut sheet = self.clone();
        sheet.add(text, None)?;
        Ok(sheet)
    }

    fn add(&mut self, text: &str, layer: Option<String>) -> Result<()> {
        // Rules keep owned copies of what they need, so the text is freed with the parse: libraries
        // like Emotion inject a stream of distinct CSS for as long as the app runs.
        let flat = flatten(text)?;
        let parsed = StyleSheet::parse(&flat, parser_options()).map_err(|e| anyhow!("css: {e}"))?;
        self.collect(&parsed.rules, &layer, &[], &[]);
        Ok(())
    }

    fn layer_rank(&mut self, name: &str) -> u32 {
        let pos = self
            .layers
            .iter()
            .position(|l| l == name)
            .unwrap_or_else(|| {
                self.layers.push(name.to_owned());
                self.layers.len() - 1
            });
        pos as u32
    }

    fn collect(
        &mut self,
        rules: &CssRuleList<'_>,
        layer: &Option<String>,
        media: &[MediaList<'static>],
        containers: &[ContainerQuery],
    ) {
        for rule in &rules.0 {
            match rule {
                CssRule::Style(style) => {
                    if !style.rules.0.is_empty() {
                        warn_once("nested rules left after flattening");
                    }
                    let rule = self.rules.len();
                    let layer = layer.as_deref().map_or(u32::MAX, |l| self.layer_rank(l));
                    self.rules.push(Rule {
                        decls: style.declarations.declarations.clone().into_owned(),
                        important: style.declarations.important_declarations.clone().into_owned(),
                        layer,
                        order: rule as u32,
                        media: media.to_vec(),
                        containers: containers.to_vec(),
                    });
                    for selector in style.selectors.0.iter() {
                        self.index(selector.clone().into_owned(), rule);
                    }
                }
                CssRule::Media(m) => {
                    let mut media = media.to_vec();
                    media.push(m.query.clone().into_owned());
                    self.collect(&m.rules, layer, &media, containers);
                }
                CssRule::Supports(s) => {
                    if supports(&s.condition) {
                        self.collect(&s.rules, layer, media, containers);
                    }
                }
                CssRule::LayerStatement(s) => {
                    for name in &s.names {
                        self.layer_rank(&join_layer(layer, Some(&name.0[..])));
                    }
                }
                CssRule::LayerBlock(b) => {
                    let name = match &b.name {
                        Some(n) => join_layer(layer, Some(&n.0[..])),
                        None => format!(
                            "{}.<anon{}>",
                            layer.as_deref().unwrap_or(""),
                            self.rules.len()
                        ),
                    };
                    self.layer_rank(&name);
                    self.collect(&b.rules, &Some(name), media, containers);
                }
                CssRule::Property(p) => {
                    let initial = p
                        .initial_value
                        .as_ref()
                        .and_then(|v| v.to_css_string(PrinterOptions::default()).ok());
                    self.registered.insert(
                        p.name.0.to_string(),
                        Registered {
                            inherits: p.inherits,
                            initial,
                        },
                    );
                }
                CssRule::Container(c) => {
                    let text = c
                        .condition
                        .as_ref()
                        .map(|c| c.to_css_string(PrinterOptions::default()));
                    if let Some(query) = text.and_then(|t| t.ok()).and_then(|t| media_list(&t)) {
                        let mut containers = containers.to_vec();
                        containers.push(ContainerQuery {
                            name: c.name.as_ref().map(|n| n.0.0.to_string()),
                            query,
                        });
                        self.collect(&c.rules, layer, media, &containers);
                    }
                }
                CssRule::Import(_) => warn_once("@import (bundle CSS before passing it in)"),
                CssRule::Keyframes(k) => {
                    let name = match &k.name {
                        KeyframesName::Ident(i) => i.0.to_string(),
                        KeyframesName::Custom(s) => s.to_string(),
                    };
                    let mut frames = Vec::new();
                    for frame in &k.keyframes {
                        for selector in &frame.selectors {
                            let offset = match selector {
                                KeyframeSelector::From => 0.0,
                                KeyframeSelector::To => 1.0,
                                KeyframeSelector::Percentage(p) => p.0,
                                KeyframeSelector::TimelineRangePercentage(_) => continue,
                            };
                            frames.push((offset, frame.declarations.clone().into_owned()));
                        }
                    }
                    frames.sort_by(|a, b| a.0.total_cmp(&b.0));
                    // The last definition of a name wins, as in CSS.
                    self.keyframes.insert(name, frames);
                }
                CssRule::FontFace(_) => warn_once("@font-face"),
                _ => {}
            }
        }
    }

    fn index(&mut self, selector: Selector<'static>, rule: usize) {
        let pseudo = match selector.iter_raw_match_order().find_map(|c| match c {
            Component::PseudoElement(pe) => Some(pe),
            _ => None,
        }) {
            None => None,
            Some(PseudoElement::Before) => Some(Pseudo::Before),
            Some(PseudoElement::After) => Some(Pseudo::After),
            Some(PseudoElement::Placeholder(_)) => Some(Pseudo::Placeholder),
            Some(PseudoElement::Backdrop(_)) => Some(Pseudo::Backdrop),
            Some(_) => return, // ::selection, ::marker... have no box to style here.
        };
        // :has() keeps the specificity of the full selector, as in CSS.
        let specificity = selector.specificity();
        let mut rewritten_has = false;
        let (selector, has) = match select::unsupported(&selector) {
            Some(":has()") => match select::split_has(&selector) {
                Some(split) => split,
                // Elsewhere in the selector, each `:has()` becomes a pseudo-class matched by hand.
                None => match select::rewrite_has(&selector) {
                    Some(rewritten) => {
                        rewritten_has = true;
                        (rewritten, Vec::new())
                    }
                    None => {
                        warn_once("selector :has() with an argument that doesn't parse");
                        return;
                    }
                },
            },
            _ => (selector, Vec::new()),
        };
        if let Some(what) = select::unsupported(&selector) {
            warn_once(&format!("selector {what}"));
            return;
        }
        let states = select::states_in(&selector);
        let ancestor_states = select::ancestor_states(&selector);
        let (mut id, mut class, mut tag) = (None, None, None);
        // `iter()` walks the rightmost compound selector first; that is the one the element itself
        // matches, unless it is a pseudo-element's (`.a::after`): then the next one is.
        let mut parts = selector.iter();
        if pseudo.is_some() {
            for _ in &mut parts {}
            parts.next_sequence();
        }
        for c in parts {
            match c {
                Component::ID(i) => id = Some(i.0.to_string()),
                Component::Class(c) => class = Some(c.0.to_string()),
                Component::LocalName(n) => tag = Some(n.lower_name.0.to_string()),
                _ => {}
            }
        }
        let entry = self.entries.len();
        self.has_has |= !has.is_empty() || rewritten_has;
        self.reads_siblings |=
            select::reads_siblings(&selector) || (rewritten_has && select::has_reads_siblings());
        self.entries.push(Entry {
            specificity,
            selector,
            rule,
            states,
            ancestor_states,
            has,
            pseudo,
        });
        match (id, class, tag) {
            (Some(k), _, _) => self.by_id.entry(k).or_default().push(entry),
            (_, Some(k), _) => self.by_class.entry(k).or_default().push(entry),
            (_, _, Some(k)) => self.by_tag.entry(k).or_default().push(entry),
            _ => self.universal.push(entry),
        }
    }

    /// Entries whose rightmost compound could match an element with these keys.
    pub fn candidates<'a>(
        &'a self,
        tag: &str,
        id: Option<&str>,
        classes: &[String],
    ) -> impl Iterator<Item = &'a Entry> {
        let by_id = id.and_then(|i| self.by_id.get(i));
        let by_tag = self.by_tag.get(tag);
        let by_class = classes.iter().filter_map(|c| self.by_class.get(c.as_str()));
        by_id
            .into_iter()
            .chain(by_tag)
            .chain(by_class)
            .chain(std::iter::once(&self.universal))
            .flatten()
            .map(|&i| &self.entries[i])
    }
}

fn parser_options<'i>() -> ParserOptions<'i> {
    ParserOptions {
        error_recovery: true,
        ..Default::default()
    }
}

/// Lowers nesting and media range syntax so the cascade only ever sees flat rules.
fn flatten(text: &str) -> Result<String> {
    let text = &dense_flow(text);
    let mut sheet = StyleSheet::parse(text, parser_options()).map_err(|e| anyhow!("css: {e}"))?;
    let targets = Targets {
        include: Features::Nesting | Features::MediaRangeSyntax | Features::MediaIntervalSyntax,
        ..Default::default()
    };
    sheet
        .minify(MinifyOptions {
            targets,
            ..Default::default()
        })
        .map_err(|e| anyhow!("css: {e}"))?;
    let out = sheet
        .to_css(PrinterOptions {
            targets,
            ..Default::default()
        })
        .map_err(|e| anyhow!("css: {e}"))?;
    Ok(out.code)
}

/// `grid-auto-flow: dense` spelled `row dense`, as it means: lightningcss rejects `dense` alone
/// (Tailwind's `grid-flow-dense`).
fn dense_flow(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("grid-auto-flow:") {
        let (head, tail) = rest.split_at(at + "grid-auto-flow:".len());
        out.push_str(head);
        let value = tail.trim_start();
        let alone = value
            .strip_prefix("dense")
            .is_some_and(|after| after.trim_start().starts_with([';', '}', '!']) || after.trim().is_empty());
        if alone {
            out.push_str(" row");
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn join_layer(parent: &Option<String>, name: Option<&[CowArcStr<'_>]>) -> String {
    let own = name
        .map(|n| n.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join("."))
        .unwrap_or_default();
    match parent {
        Some(p) => format!("{p}.{own}"),
        None => own,
    }
}

/// A feature is supported when lightningcss can parse it into a typed value.
fn supports(c: &SupportsCondition) -> bool {
    match c {
        SupportsCondition::Not(c) => !supports(c),
        SupportsCondition::And(v) => v.iter().all(supports),
        SupportsCondition::Or(v) => v.iter().any(supports),
        SupportsCondition::Declaration { property_id, value } => {
            match Property::parse_string(property_id.clone(), value, ParserOptions::default()) {
                // A `var()` is valid for any known property (Tailwind's `supports-backdrop-filter:`
                // asks `(backdrop-filter: var(--tw))`).
                Ok(Property::Unparsed(_)) => value.contains("var("),
                Ok(Property::Custom(_)) | Err(_) => false,
                Ok(_) => true,
            }
        }
        SupportsCondition::Selector(_) => true,
        SupportsCondition::Unknown(_) => false,
    }
}

/// A container condition such as `(width >= 42rem)` read as the equivalent media query.
fn media_list(condition: &str) -> Option<MediaList<'static>> {
    let css = format!("@media {condition} {{}}");
    let sheet = StyleSheet::parse(&css, ParserOptions::default()).ok()?;
    match sheet.rules.0.first()? {
        CssRule::Media(m) => Some(m.query.clone().into_owned()),
        _ => None,
    }
}

pub fn media_matches(lists: &[MediaList], env: &Env) -> bool {
    lists.iter().all(|l| {
        l.media_queries.is_empty() || l.media_queries.iter().any(|q| query_matches(q, env))
    })
}

fn query_matches(q: &MediaQuery, env: &Env) -> bool {
    let type_ok = matches!(q.media_type, MediaType::All | MediaType::Screen);
    let ok = type_ok
        && q.condition
            .as_ref()
            .is_none_or(|c| condition_matches(c, env));
    if q.qualifier == Some(Qualifier::Not) {
        !ok
    } else {
        ok
    }
}

fn condition_matches(c: &MediaCondition, env: &Env) -> bool {
    match c {
        MediaCondition::Feature(f) => feature_matches(f, env),
        MediaCondition::Not(c) => !condition_matches(c, env),
        MediaCondition::Operation {
            conditions,
            operator,
        } => match operator {
            Operator::And => conditions.iter().all(|c| condition_matches(c, env)),
            Operator::Or => conditions.iter().any(|c| condition_matches(c, env)),
        },
        MediaCondition::Unknown(_) => false,
    }
}

fn feature_matches(f: &MediaFeature, env: &Env) -> bool {
    let px = |v: &MediaFeatureValue| match v {
        MediaFeatureValue::Length(l) => length_px(l),
        _ => None,
    };
    let dimension = |name: &str| match name {
        "width" => Some(env.width),
        "height" => Some(env.height),
        _ => None,
    };
    match f {
        QueryFeature::Plain { name, value } => {
            let name = css_text(name);
            match (name.as_str(), value) {
                ("min-width", v) => px(v).is_some_and(|p| env.width >= p),
                ("max-width", v) => px(v).is_some_and(|p| env.width <= p),
                ("min-height", v) => px(v).is_some_and(|p| env.height >= p),
                ("max-height", v) => px(v).is_some_and(|p| env.height <= p),
                ("hover" | "any-hover", MediaFeatureValue::Ident(i)) => i.0.as_ref() == "hover",
                ("pointer" | "any-pointer", MediaFeatureValue::Ident(i)) => i.0.as_ref() == "fine",
                ("prefers-color-scheme", MediaFeatureValue::Ident(i)) => {
                    (i.0.as_ref() == "dark") == env.dark
                }
                ("prefers-reduced-motion", MediaFeatureValue::Ident(i)) => {
                    i.0.as_ref() == "no-preference"
                }
                (n, v) => dimension(n).zip(px(v)).is_some_and(|(d, p)| d == p),
            }
        }
        QueryFeature::Boolean { name } => matches!(
            css_text(name).as_str(),
            "hover" | "any-hover" | "pointer" | "any-pointer" | "color"
        ),
        QueryFeature::Range {
            name,
            operator,
            value,
        } => dimension(&css_text(name))
            .zip(px(value))
            .is_some_and(|(d, p)| compare(d, *operator, p)),
        QueryFeature::Interval {
            name,
            start,
            start_operator,
            end,
            end_operator,
        } => dimension(&css_text(name))
            .zip(px(start))
            .zip(px(end))
            .is_some_and(|((d, s), e)| {
                compare(s, *start_operator, d) && compare(d, *end_operator, e)
            }),
    }
}

fn compare(a: f32, op: MediaFeatureComparison, b: f32) -> bool {
    match op {
        MediaFeatureComparison::Equal => a == b,
        MediaFeatureComparison::GreaterThan => a > b,
        MediaFeatureComparison::GreaterThanEqual => a >= b,
        MediaFeatureComparison::LessThan => a < b,
        MediaFeatureComparison::LessThanEqual => a <= b,
    }
}

/// Media queries resolve `em`/`rem` against the initial 16px font size.
fn length_px(l: &lightningcss::values::length::Length) -> Option<f32> {
    match l {
        lightningcss::values::length::Length::Value(v) => match v {
            LengthValue::Rem(n) | LengthValue::Em(n) => Some(n * 16.0),
            v => v.to_px(),
        },
        _ => None,
    }
}

fn css_text<T: ToCss>(value: &T) -> String {
    value
        .to_css_string(PrinterOptions::default())
        .unwrap_or_default()
}

/// The value text of a declaration, e.g. `var(--a, 1px)` for `padding: var(--a, 1px)`.
pub fn value_text(p: &Property) -> String {
    p.value_to_css_string(PrinterOptions::default())
        .unwrap_or_default()
}

/// Logs each unsupported CSS feature once, so developers can see what was dropped.
pub fn warn_once(what: &str) {
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let mut seen = SEEN.lock().unwrap();
    if seen
        .get_or_insert_with(HashSet::new)
        .insert(what.to_owned())
    {
        eprintln!("[gpuijs] unsupported css: {what}");
    }
}
