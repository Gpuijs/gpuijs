//! Editable content (`contenteditable`). JS owns the text and the selection
//! (js/src/editing.js); the host draws the caret and the selected text over the paragraphs that
//! show them, maps the pointer and vertical caret moves to (node, offset) points, and takes typed
//! and composed text through gpui's input handler (`Root`'s `EntityInputHandler`). Offsets are
//! UTF-16 code units, as DOM offsets count.

use std::ops::Range;
use std::rc::Rc;

use gpui::{Bounds, Hsla, Pixels, Point, TextLayout, Window, fill, point, px, rgb, size};
use serde::Deserialize;

/// The selection in the focused editing host, as JS last sent it: the caret (the selection's
/// focus) as a point in a text node or before a `<br>`, and the selected text as
/// (text node, start, end). `host` is `None` while no editing host has focus.
#[derive(Deserialize, Debug, Default, Clone, PartialEq)]
pub struct Selection {
    #[serde(default)]
    pub host: Option<u64>,
    #[serde(default)]
    pub caret: Option<(u64, usize)>,
    #[serde(default)]
    pub ranges: Vec<(u64, usize, usize)>,
}

/// Where one node's text landed in a paragraph: `bytes[i]` is the paragraph byte at the node's
/// UTF-16 offset `i` (collapsed whitespace shares one). A `<br>` has one, before its line break.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub node: u64,
    pub bytes: Vec<usize>,
}

impl Segment {
    fn byte(&self, offset: usize) -> usize {
        self.bytes[offset.min(self.bytes.len() - 1)]
    }
}

/// A paragraph inside an editing host, as laid out in the last frame.
pub struct Para {
    pub host: u64,
    pub layout: TextLayout,
    pub segments: Rc<Vec<Segment>>,
}

/// The paragraph byte of a (node, offset) point, if the node shows in it.
pub fn byte_at(segments: &[Segment], node: u64, offset: usize) -> Option<usize> {
    segments.iter().find(|s| s.node == node).map(|s| s.byte(offset))
}

/// The (node, offset) point at a paragraph byte. At a boundary it's the end of the node before,
/// so typing there continues that text (its bold, its link), as in a browser.
pub fn point_at(segments: &[Segment], byte: usize) -> Option<(u64, usize)> {
    let s = segments
        .iter()
        .find(|s| s.bytes.last().is_some_and(|&end| byte <= end))
        .or_else(|| segments.last())?;
    let offset = s.bytes.iter().position(|&b| b >= byte).unwrap_or(s.bytes.len() - 1);
    Some((s.node, offset))
}

/// What one paragraph shows of the selection.
#[derive(Clone, Default)]
pub struct Marks {
    pub caret: Option<usize>,
    pub ranges: Vec<Range<usize>>,
    pub caret_color: Option<Hsla>,
}

impl Selection {
    /// The caret and selected bytes in a paragraph of `host`; empty for the others.
    pub fn marks(&self, host: u64, segments: &[Segment]) -> Marks {
        if self.host != Some(host) {
            return Marks::default();
        }
        let ranges = self
            .ranges
            .iter()
            .filter_map(|&(node, a, b)| {
                let s = segments.iter().find(|s| s.node == node)?;
                Some(s.byte(a)..s.byte(b)).filter(|r| !r.is_empty())
            })
            .collect();
        let caret = self.caret.and_then(|(node, offset)| byte_at(segments, node, offset));
        Marks { caret, ranges, caret_color: None }
    }
}

/// macOS's highlight for selected text in the focused window.
const SELECTION: u32 = 0xb4d5fe;

/// Paints a paragraph's marks: called before the text paints, so they sit under it.
// shortcut: the caret doesn't blink; blink it on a timer if a steady caret reads as a glitch.
pub fn paint(layout: &TextLayout, marks: &Marks, window: &mut Window) {
    if !layout.is_placed() || (marks.caret.is_none() && marks.ranges.is_empty()) {
        return;
    }
    let bounds = layout.bounds();
    let line = layout.line_height();
    let len = layout.len();
    for r in &marks.ranges {
        let (Some(a), Some(b)) = (
            layout.position_for_index(r.start.min(len)),
            layout.position_for_index(r.end.min(len)),
        ) else {
            continue;
        };
        // One band per line, to the right edge on all but the last.
        let (mut x, mut y) = (a.x, a.y);
        while y + px(0.5) < b.y {
            window.paint_quad(fill(Bounds::from_corners(point(x, y), point(bounds.right(), y + line)), rgb(SELECTION)));
            (x, y) = (bounds.left(), y + line);
        }
        window.paint_quad(fill(Bounds::from_corners(point(x, y), point(b.x, y + line)), rgb(SELECTION)));
    }
    if let Some(at) = marks.caret.and_then(|i| layout.position_for_index(i.min(len))) {
        let color = marks.caret_color.unwrap_or(gpui::black());
        window.paint_quad(fill(Bounds::new(at, size(px(1.5), line)), color));
    }
}

/// The caret's box in window coordinates: where the IME's candidate window goes.
pub fn caret_bounds(paras: &[Para], caret: (u64, usize)) -> Option<Bounds<Pixels>> {
    let (para, byte) = find(paras, caret)?;
    let at = para.layout.position_for_index(byte.min(para.layout.len()))?;
    Some(Bounds::new(at, size(px(1.), para.layout.line_height())))
}

fn find(paras: &[Para], (node, offset): (u64, usize)) -> Option<(&Para, usize)> {
    paras
        .iter()
        .filter(|p| p.layout.is_placed())
        .find_map(|p| Some((p, byte_at(&p.segments, node, offset)?)))
}

// How far `at` is from a paragraph: rows first, so the one beside the pointer wins over one below.
fn distance(bounds: Bounds<Pixels>, at: Point<Pixels>) -> f32 {
    let dy = (bounds.top() - at.y).max(at.y - bounds.bottom()).max(px(0.));
    let dx = (bounds.left() - at.x).max(at.x - bounds.right()).max(px(0.));
    f32::from(dy) * 1000. + f32::from(dx)
}

fn index_at(layout: &TextLayout, at: Point<Pixels>) -> usize {
    let b = layout.bounds();
    let inside = point(at.x.clamp(b.left(), b.right()), at.y.clamp(b.top(), b.bottom() - px(1.)));
    layout.closest_index_for_position(inside).unwrap_or_else(|i| i)
}

/// The point under the pointer in `host`'s paragraphs, or in the one nearest it.
pub fn hit(paras: &[Para], host: u64, at: Point<Pixels>) -> Option<(u64, usize)> {
    let para = paras
        .iter()
        .filter(|p| p.host == host && p.layout.is_placed())
        .min_by(|a, b| distance(a.layout.bounds(), at).total_cmp(&distance(b.layout.bounds(), at)))?;
    point_at(&para.segments, index_at(&para.layout, at))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineMove {
    Up,
    Down,
    Start,
    End,
}

/// Where the caret goes a line up or down (keeping its x), or to its line's start or end.
// shortcut: up and down keep the x the caret has now, not the one it started from; remember the
// first x across moves if caret travel through short lines matters.
pub fn line_target(paras: &[Para], host: u64, caret: (u64, usize), mv: LineMove) -> Option<(u64, usize)> {
    let (para, byte) = find(paras, caret)?;
    let layout = &para.layout;
    let at = layout.position_for_index(byte.min(layout.len()))?;
    let (line, bounds) = (layout.line_height(), layout.bounds());
    let mid = at.y + line / 2.;
    let index = match mv {
        LineMove::Start => index_at(layout, point(bounds.left() - px(1.), mid)),
        LineMove::End => index_at(layout, point(bounds.right() + px(1.), mid)),
        LineMove::Up | LineMove::Down => {
            let y = if mv == LineMove::Up { mid - line } else { mid + line };
            if y > bounds.top() && y < bounds.bottom() {
                index_at(layout, point(at.x, y))
            } else {
                // Off this paragraph: the nearest one of the host's that way, at its closest line.
                let up = mv == LineMove::Up;
                let next = paras
                    .iter()
                    .filter(|p| p.host == host && p.layout.is_placed())
                    .map(|p| (p, p.layout.bounds()))
                    .filter(|(_, b)| if up { b.bottom() <= bounds.top() + px(0.5) } else { b.top() >= bounds.bottom() - px(0.5) })
                    .min_by(|(_, a), (_, b)| {
                        let gap = |b: &Bounds<Pixels>| if up { bounds.top() - b.bottom() } else { b.top() - bounds.bottom() };
                        f32::from(gap(a)).total_cmp(&f32::from(gap(b)))
                    });
                return match next {
                    Some((p, b)) => {
                        let y = if up { b.bottom() - line / 2. } else { b.top() + line / 2. };
                        point_at(&p.segments, index_at(&p.layout, point(at.x, y)))
                    }
                    // Past the first or last line: its start or end, as browsers go.
                    None => point_at(&para.segments, if up { 0 } else { layout.len() }),
                };
            }
        }
    };
    point_at(&para.segments, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(node: u64, bytes: &[usize]) -> Segment {
        Segment { node, bytes: bytes.to_vec() }
    }

    #[test]
    fn points_map_to_bytes_and_back() {
        // "ab" then "cd" (bold, say), then a line break before "e".
        let segments = [seg(1, &[0, 1, 2]), seg(2, &[2, 3, 4]), seg(3, &[4]), seg(4, &[5, 6])];
        assert_eq!(byte_at(&segments, 2, 1), Some(3));
        assert_eq!(byte_at(&segments, 2, 9), Some(4), "past the end is the end");
        assert_eq!(byte_at(&segments, 9, 0), None);
        // A boundary belongs to the text before it.
        assert_eq!(point_at(&segments, 2), Some((1, 2)));
        assert_eq!(point_at(&segments, 3), Some((2, 1)));
        assert_eq!(point_at(&segments, 6), Some((4, 1)));
        assert_eq!(point_at(&segments, 99), Some((4, 1)));
    }

    #[test]
    fn marks_are_the_focused_hosts_only() {
        let segments = [seg(1, &[0, 1, 2, 3])];
        let sel = Selection { host: Some(7), caret: Some((1, 3)), ranges: vec![(1, 1, 3), (9, 0, 1)] };
        let marks = sel.marks(7, &segments);
        assert_eq!((marks.caret, marks.ranges), (Some(3), vec![1..3]));
        assert!(sel.marks(8, &segments).caret.is_none());
    }
}
