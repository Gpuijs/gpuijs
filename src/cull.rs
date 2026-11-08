//! Off-screen culling for long vertical lists. A child of such a list that was laid out at the last
//! frame, hasn't changed since and is outside the visible part of the list is not built: a run of
//! them becomes one spacer with their measured extent, so layout around them is the same. When a
//! spacer turns out to be visible after all (the list scrolled or content above it moved), the
//! frame is drawn again with what that layout measured, before it is presented.
//!
//! Children not measured yet (new ones) and changed ones are left out too while off screen, as with
//! `content-visibility: auto`: new ones with the height their measured siblings have, changed ones
//! with the size they had. Each is measured for real once it comes near the view.

use crate::tree::{IdMap, IdSet, Tree};

/// Lists with fewer children are always built in full: culling them saves too little.
pub const MIN_KIDS: usize = 32;

/// The height of an unmeasured child in a list where nothing is measured yet. The frame is drawn
/// again with the first measured ones (see `Runs::guessed`).
const GUESS: f32 = 20.0;

/// The children to leave out of a list's build, as runs of neighbours.
#[derive(Debug, Default, PartialEq)]
pub struct Runs {
    /// Each run's index in the children, and the children in it.
    pub runs: Vec<(usize, Vec<u64>)>,
    /// Some were left out with a guessed height: draw again once the built ones are measured.
    pub guessed: bool,
}

/// A list as last laid out, in its own coordinates (y grows down from its top edge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct List {
    /// The draw that measured it (see `Cull::frame`).
    pub frame: u64,
    /// Window y of the list's top edge.
    pub origin: f32,
    /// The part of it that was visible (inside every clip and the window).
    pub visible: (f32, f32),
    pub width: f32,
}

/// A child of a list as last laid out: its border box, in the list's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kid {
    pub top: f32,
    pub height: f32,
    /// The list's width then; at another width the child may wrap differently.
    pub list_width: f32,
    /// The child laid out just before it (0 for none), so a run is only measured across
    /// children that were neighbours then too.
    pub prev: u64,
    /// Never laid out: `top` and `height` are the list's estimate, made again each draw.
    pub estimated: bool,
}

impl Kid {
    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

#[derive(Default)]
pub struct Cull {
    /// Increments with every draw.
    pub frame: u64,
    pub lists: IdMap<List>,
    pub kids: IdMap<Kid>,
}

impl Cull {
    /// These nodes or something inside them changed: they and their ancestors must be laid out
    /// again before they can be culled, except a list's children, which keep the size they had as
    /// an estimate while they stay off screen, unless they can't be left out now (`cullable`).
    /// Shared ancestors are walked once.
    pub fn invalidate(
        &mut self,
        tree: &Tree,
        ids: impl IntoIterator<Item = u64>,
        cullable: &dyn Fn(u64) -> bool,
    ) {
        let mut seen = IdSet::default();
        for id in ids {
            let mut at = Some(id);
            while let Some(n) = at.filter(|n| seen.insert(*n)) {
                at = tree.nodes.get(&n).and_then(|node| node.parent);
                if !(at.is_some_and(|p| self.lists.contains_key(&p)) && cullable(n)) {
                    self.kids.remove(&n);
                }
            }
        }
    }

    /// The children of `list` (in order) to leave out this draw, given what may not be culled
    /// (`keep`) and which unmeasured children may stand in with an estimate (`estimable`: in-flow
    /// boxes). A list not laid out at the last draw is taken to show `(0, view)` of itself.
    pub fn runs(
        &mut self,
        list: u64,
        kids: &[u64],
        keep: &dyn Fn(u64) -> bool,
        estimable: &dyn Fn(u64) -> bool,
        view: f32,
    ) -> Runs {
        let (visible, width) = match self.lists.get(&list).filter(|l| l.frame + 1 == self.frame) {
            Some(l) => (l.visible, Some(l.width)),
            None => ((0.0, view), None),
        };
        // Built ahead of the visible part by its own height each way, so scrolling rarely meets a spacer.
        let margin = (visible.1 - visible.0).max(200.0);
        let (top, bottom) = (visible.0 - margin, visible.1 + margin);
        let measured = |k: &&Kid| !k.estimated && Some(k.list_width) == width;
        // Unmeasured children take their measured siblings' typical height and spacing.
        let (mut heights, mut gaps) = (Vec::new(), Vec::new());
        let mut last: Option<(u64, Kid)> = None;
        for &id in kids {
            let k = self.kids.get(&id).filter(measured).copied();
            if let Some(k) = k {
                heights.push(k.height);
                if let Some((_, pk)) = last.filter(|(p, _)| k.prev == *p) {
                    gaps.push(k.top - pk.bottom());
                }
            }
            last = k.map(|k| (id, k));
        }
        let guess = heights.is_empty();
        let height = if guess { GUESS } else { median(&mut heights) };
        let gap = if gaps.is_empty() {
            0.0
        } else {
            median(&mut gaps)
        };

        let mut out = Runs::default();
        let (mut prev, mut prev_measured, mut open) = (0, false, false);
        // Where the next child starts, as far as is known.
        let mut y = 0.0;
        for (i, &id) in kids.iter().enumerate() {
            let estimate = Kid {
                top: y,
                height,
                list_width: width.unwrap_or(f32::NAN),
                prev,
                estimated: true,
            };
            let k = match self.kids.get_mut(&id) {
                Some(k) if measured(&&*k) => *k,
                // Estimated before, so estimable then; a change since went through `invalidate`.
                Some(k) if k.estimated => {
                    *k = estimate;
                    estimate
                }
                _ if estimable(id) => {
                    self.kids.insert(id, estimate);
                    estimate
                }
                _ => {
                    (prev, prev_measured, open) = (id, false, false);
                    continue;
                }
            };
            let (culled, joins) = match k.estimated {
                // Only below the view: above it, a wrong estimate would move what shows.
                true => (height > 0.0 && k.top > bottom, true),
                false => (
                    k.height > 0.0 && (k.bottom() < top || k.top > bottom),
                    (prev_measured && k.prev == prev) || (k.top - y).abs() < 0.5,
                ),
            };
            if culled && !keep(id) {
                if open && joins {
                    out.runs.last_mut().unwrap().1.push(id);
                } else {
                    out.runs.push((i, vec![id]));
                }
                out.guessed |= guess && k.estimated;
                open = true;
            } else {
                open = false;
            }
            (prev, prev_measured) = (id, !k.estimated);
            y = k.bottom() + gap;
        }
        out
    }

    /// The extent of a run: (top, height).
    pub fn extent(&self, run: &[u64]) -> (f32, f32) {
        let first = self.kids[&run[0]];
        let last = self.kids[run.last().unwrap()];
        (first.top, last.bottom() - first.top)
    }

    /// A run's spacer was laid out at `top`: content above it moved, and its children with it.
    pub fn moved(&mut self, run: &[u64], top: f32) {
        let Some(first) = self.kids.get(&run[0]) else {
            return;
        };
        let delta = top - first.top;
        if delta.abs() < 0.01 {
            return;
        }
        for id in run {
            if let Some(k) = self.kids.get_mut(id) {
                k.top += delta;
            }
        }
    }
}

fn median(values: &mut [f32]) -> f32 {
    let mid = values.len() / 2;
    *values.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Ancestors-or-self of `ids`: nodes that must stay built (what is hovered or focused).
pub fn with_ancestors(tree: &Tree, ids: impl IntoIterator<Item = u64>) -> IdSet {
    let mut out = IdSet::default();
    for id in ids {
        let mut at = Some(id);
        while let Some(n) = at.filter(|n| out.insert(*n)) {
            at = tree.nodes.get(&n).and_then(|node| node.parent);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cull(kids: &[(u64, f32, f32, u64)]) -> Cull {
        let mut c = Cull {
            frame: 2,
            ..Default::default()
        };
        c.lists.insert(
            1,
            List {
                frame: 1,
                origin: 0.0,
                visible: (0.0, 100.0),
                width: 50.0,
            },
        );
        for &(id, top, height, prev) in kids {
            let k = Kid {
                top,
                height,
                list_width: 50.0,
                prev,
                estimated: false,
            };
            c.kids.insert(id, k);
        }
        c
    }

    /// Runs with nothing estimated: only measured children are left out.
    fn runs(c: &mut Cull, kids: &[u64], keep: &dyn Fn(u64) -> bool) -> Vec<Vec<u64>> {
        ids(c.runs(1, kids, keep, &|_| false, 100.0))
    }

    fn ids(runs: Runs) -> Vec<Vec<u64>> {
        runs.runs.into_iter().map(|(_, run)| run).collect()
    }

    #[test]
    fn runs_of_far_children_between_built_ones() {
        // Rows of 100px in a 100px view with a 200px margin: 2..=5 are built, 6..=9 culled.
        let rows: Vec<(u64, f32, f32, u64)> = (2..10)
            .map(|id| (id, (id as f32 - 2.0) * 100.0, 100.0, id - 1))
            .collect();
        let mut c = cull(&rows);
        let kids: Vec<u64> = (2..10).collect();
        assert_eq!(runs(&mut c, &kids, &|_| false), vec![vec![6, 7, 8, 9]]);
        assert_eq!(c.extent(&[6, 7, 8, 9]), (400.0, 400.0));
        // A kept (hovered) row splits the run, and so does a new one, which is built to be measured.
        assert_eq!(
            runs(&mut c, &kids, &|id| id == 7),
            vec![vec![6], vec![8, 9]]
        );
        let mut kids2 = kids.clone();
        kids2.insert(6, 42);
        assert_eq!(
            runs(&mut c, &kids2, &|_| false),
            vec![vec![6, 7], vec![8, 9]]
        );
    }

    #[test]
    fn a_run_breaks_where_neighbours_changed() {
        let rows = [
            (5, 400.0, 100.0, 4),
            (6, 500.0, 100.0, 5),
            (7, 600.0, 100.0, 6),
        ];
        let mut c = cull(&rows);
        // 5 was removed: 6 now follows 4, so the gap 5 left isn't measured into a run.
        assert_eq!(runs(&mut c, &[4, 6, 7], &|_| false), vec![vec![6, 7]]);
        // Swapped neighbours start new runs.
        assert_eq!(
            runs(&mut c, &[5, 7, 6], &|_| false),
            vec![vec![5], vec![7], vec![6]]
        );
    }

    #[test]
    fn no_culling_without_a_layout_from_the_last_draw_or_at_another_width() {
        let rows = [(5, 400.0, 100.0, 4)];
        let mut c = cull(&rows);
        c.frame = 3;
        assert!(runs(&mut c, &[5], &|_| false).is_empty());
        c.frame = 2;
        c.lists.get_mut(&1).unwrap().width = 60.0;
        assert!(runs(&mut c, &[5], &|_| false).is_empty());
    }

    #[test]
    fn new_children_below_the_view_take_their_siblings_height() {
        // 2..=5 measured at 100px each; 6..=9 are new, estimated at 100px from 400 on.
        let rows: Vec<(u64, f32, f32, u64)> = (2..6)
            .map(|id| (id, (id as f32 - 2.0) * 100.0, 100.0, id - 1))
            .collect();
        let mut c = cull(&rows);
        let kids: Vec<u64> = (2..10).collect();
        let out = c.runs(1, &kids, &|_| false, &|_| true, 100.0);
        assert!(!out.guessed);
        assert_eq!(out.runs, vec![(4, vec![6, 7, 8, 9])]);
        assert_eq!(c.extent(&[6, 7, 8, 9]), (400.0, 400.0));
        assert!(c.kids[&9].estimated);
        // Not estimable (an absolute box): built, and it ends the run.
        let out = c.runs(
            1,
            &[2, 3, 4, 5, 6, 7, 42, 8, 9],
            &|_| false,
            &|id| id != 42,
            100.0,
        );
        assert_eq!(ids(out), vec![vec![6, 7], vec![8, 9]]);
    }

    #[test]
    fn a_list_drawn_for_the_first_time_guesses() {
        let mut c = Cull {
            frame: 2,
            ..Default::default()
        };
        let kids: Vec<u64> = (2..100).collect();
        // A 100px view and 200px margin: rows that start past 300px at 20px each are left out.
        let out = c.runs(1, &kids, &|_| false, &|_| true, 100.0);
        assert!(out.guessed);
        assert_eq!(ids(out), vec![(18..100).collect::<Vec<u64>>()]);
    }

    #[test]
    fn changed_children_of_a_list_keep_their_size() {
        let mut c = cull(&[(5, 400.0, 100.0, 4)]);
        let mut tree = Tree::default();
        for op in [
            crate::protocol::Op::El(1, "div".into(), Default::default()),
            crate::protocol::Op::El(5, "div".into(), Default::default()),
            crate::protocol::Op::Append(1, 5),
        ] {
            tree.apply(op);
        }
        c.invalidate(&tree, [5], &|_| true);
        assert!(c.kids.contains_key(&5));
        c.invalidate(&tree, [5], &|_| false);
        assert!(!c.kids.contains_key(&5));
    }

    #[test]
    fn moved_runs_shift_their_children() {
        let mut c = cull(&[(5, 400.0, 100.0, 4), (6, 500.0, 100.0, 5)]);
        c.moved(&[5, 6], 350.0);
        assert_eq!((c.kids[&5].top, c.kids[&6].top), (350.0, 450.0));
    }
}
