//! Retained node tree mirrored from React host instances.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use crate::protocol::{Op, Props};

/// Hashes a node id with one multiply (as FxHash does): ids are small counters, and SipHash, the
/// default, showed up in profiles of big trees.
#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

pub type IdMap<V> = HashMap<u64, V, BuildHasherDefault<IdHasher>>;
pub type IdSet = HashSet<u64, BuildHasherDefault<IdHasher>>;

pub const HTML: u64 = 0;
pub const BODY: u64 = 1;

#[derive(Debug, Default)]
pub struct Node {
    /// Lowercase tag name; empty for text nodes.
    pub tag: String,
    /// `Some` for text nodes.
    pub text: Option<String>,
    pub props: Props,
    pub classes: Vec<String>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    /// Removed, and leaving `nodes` at the next `settle`.
    pub freed: bool,
}

impl Node {
    fn element(tag: &str, props: Props) -> Self {
        let mut node = Node {
            tag: tag.to_ascii_lowercase(),
            ..Default::default()
        };
        node.set_props(props);
        node
    }

    fn set_props(&mut self, props: Props) {
        self.classes = props.class.split_whitespace().map(str::to_owned).collect();
        self.props = props;
    }

    pub fn is_text(&self) -> bool {
        self.text.is_some()
    }
}

pub struct Tree {
    pub nodes: IdMap<Node>,
    /// Children of parents a batch edits many times, until `settle` (see `Chain`).
    chains: IdMap<Chain>,
    /// Child edits per parent since the last `settle`.
    edits: IdMap<u32>,
    /// Removed nodes, to be dropped when nothing waits on the host (`take_garbage`): a cleared
    /// list frees tens of thousands.
    garbage: Vec<Box<dyn Any>>,
    /// Ids of the nodes removed since the last `take_freed`.
    freed: Vec<u64>,
    /// Removed nodes still in `nodes`, until the next `settle`.
    unswept: Vec<u64>,
    /// Elements with a `contenteditable` attribute, freed ones too (see `Root::editable`).
    pub editable: IdSet,
}

/// A parent with this many child edits in a batch has them made on a `Chain`.
const CHAIN_AFTER: u32 = 16;

/// A parent's children as a linked list while a batch edits many of them: a removal or move is
/// O(1) here, as in a DOM, where `Vec` shifts the rest (a cleared list, a keyed reorder moving
/// hundreds of rows).
#[derive(Default)]
struct Chain {
    head: Option<u64>,
    tail: Option<u64>,
    /// Each child's (previous, next).
    links: IdMap<(Option<u64>, Option<u64>)>,
}

impl Chain {
    fn new(children: &[u64]) -> Self {
        let mut chain = Chain::default();
        for &c in children {
            chain.link(c, None);
        }
        chain
    }

    fn unlink(&mut self, c: u64) {
        let Some((p, n)) = self.links.remove(&c) else {
            return;
        };
        match p {
            Some(p) => self.links.get_mut(&p).unwrap().1 = n,
            None => self.head = n,
        }
        match n {
            Some(n) => self.links.get_mut(&n).unwrap().0 = p,
            None => self.tail = p,
        }
    }

    /// Before `before`, or last when it isn't a child.
    fn link(&mut self, c: u64, before: Option<u64>) {
        let n = before.filter(|b| self.links.contains_key(b));
        let p = match n {
            Some(n) => self.links[&n].0,
            None => self.tail,
        };
        self.links.insert(c, (p, n));
        match p {
            Some(p) => self.links.get_mut(&p).unwrap().1 = Some(c),
            None => self.head = Some(c),
        }
        match n {
            Some(n) => self.links.get_mut(&n).unwrap().0 = Some(c),
            None => self.tail = Some(c),
        }
    }

    fn into_vec(self) -> Vec<u64> {
        std::iter::successors(self.head, |c| self.links[c].1).collect()
    }
}

impl Default for Tree {
    fn default() -> Self {
        let mut nodes = IdMap::default();
        nodes.insert(HTML, Node::element("html", Props::default()));
        nodes.insert(BODY, Node::element("body", Props::default()));
        let mut tree = Tree {
            nodes,
            chains: IdMap::default(),
            edits: IdMap::default(),
            garbage: Vec::new(),
            freed: Vec::new(),
            unswept: Vec::new(),
            editable: IdSet::default(),
        };
        tree.attach(HTML, BODY, None);
        tree
    }
}

impl Tree {
    fn mark_editable(&mut self, id: u64, props: &Props) {
        match props.attrs.contains_key("contenteditable") {
            true => self.editable.insert(id),
            false => self.editable.remove(&id),
        };
    }

    pub fn get(&self, id: u64) -> &Node {
        &self.nodes[&id]
    }

    #[cfg(test)]
    pub fn apply(&mut self, op: Op) {
        self.apply_deferred(op);
        self.settle();
    }

    /// `apply`, leaving the `children` of parents edited many times stale until `settle`.
    pub fn apply_deferred(&mut self, op: Op) {
        match op {
            Op::El(id, tag, props) => {
                self.mark_editable(id, &props);
                self.nodes.insert(id, Node::element(&tag, props));
            }
            Op::Txt(id, text) => {
                self.nodes.insert(
                    id,
                    Node {
                        text: Some(text),
                        ..Default::default()
                    },
                );
            }
            Op::Set(id, props) => {
                self.mark_editable(id, &props);
                self.with(id, "set", |n| n.set_props(props))
            }
            Op::Settext(id, text) => self.with(id, "settext", |n| n.text = Some(text)),
            Op::Append(parent, child) => self.attach(parent, child, None),
            Op::Insert(parent, child, before) => self.attach(parent, child, Some(before)),
            Op::Remove(parent, child) => {
                self.detach(parent, child);
                self.free(child);
            }
            Op::Detach(parent, child) => self.detach(parent, child),
            // Focus is UI state, handled by the renderer.
            Op::Focus(_) | Op::Hover(_) | Op::Scroll(..) => {}
        }
    }

    pub fn take_garbage(&mut self) -> Vec<Box<dyn Any>> {
        std::mem::take(&mut self.garbage)
    }

    pub fn take_freed(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.freed)
    }

    fn with(&mut self, id: u64, op: &str, f: impl FnOnce(&mut Node)) {
        match self.nodes.get_mut(&id) {
            Some(node) => f(node),
            None => eprintln!("[gpuijs] {op} on unknown node {id}"),
        }
    }

    /// Brings every parent's `children` up to date; call after a batch of `apply_deferred`.
    pub fn settle(&mut self) {
        for (parent, chain) in std::mem::take(&mut self.chains) {
            if let Some(p) = self.nodes.get_mut(&parent) {
                p.children = chain.into_vec();
            }
        }
        self.edits.clear();
        self.sweep();
    }

    /// Moves removed nodes out of `nodes`, into `garbage`. When most go (a cleared list), the rest
    /// move to a fresh map instead, so the ones going aren't copied out one by one.
    fn sweep(&mut self) {
        let ids = std::mem::take(&mut self.unswept);
        if ids.len() * 2 > self.nodes.len() {
            let mut old = std::mem::take(&mut self.nodes);
            let kept: Vec<u64> = old
                .iter()
                .filter(|(_, n)| !n.freed)
                .map(|(&id, _)| id)
                .collect();
            // Sized as before: the next big mount needn't grow it again.
            self.nodes.reserve(old.capacity());
            for id in kept {
                self.nodes.insert(id, old.remove(&id).unwrap());
            }
            self.garbage.push(Box::new(old));
        } else {
            let gone: Vec<Node> = ids.iter().filter_map(|id| self.nodes.remove(id)).collect();
            self.garbage.push(Box::new(gone));
        }
    }

    fn settle_parent(&mut self, parent: u64) {
        if let Some(chain) = self.chains.remove(&parent)
            && let Some(p) = self.nodes.get_mut(&parent)
        {
            p.children = chain.into_vec();
        }
    }

    /// The chain `parent`'s child edits go to, once it has had enough of them.
    fn chain(&mut self, parent: u64) -> Option<&mut Chain> {
        let edits = self.edits.entry(parent).or_default();
        *edits += 1;
        if *edits < CHAIN_AFTER && !self.chains.contains_key(&parent) {
            return None;
        }
        let children = &self.nodes.get(&parent)?.children;
        Some(
            self.chains
                .entry(parent)
                .or_insert_with(|| Chain::new(children)),
        )
    }

    /// Appends or inserts `child`, moving it if it is already attached somewhere.
    fn attach(&mut self, parent: u64, child: u64, before: Option<u64>) {
        let alive = |id| self.nodes.get(&id).is_some_and(|n| !n.freed);
        if !alive(parent) || !alive(child) {
            eprintln!("[gpuijs] attach of unknown node {child} to {parent}");
            return;
        }
        if let Some(old) = self.nodes[&child].parent {
            self.detach(old, child);
        }
        if let Some(chain) = self.chain(parent) {
            chain.link(child, before);
        } else {
            let siblings = &mut self.nodes.get_mut(&parent).unwrap().children;
            let at = before
                .and_then(|b| siblings.iter().position(|&c| c == b))
                .unwrap_or(siblings.len());
            siblings.insert(at, child);
        }
        self.nodes.get_mut(&child).unwrap().parent = Some(parent);
    }

    fn detach(&mut self, parent: u64, child: u64) {
        if let Some(chain) = self.chain(parent) {
            chain.unlink(child);
        } else if let Some(p) = self.nodes.get_mut(&parent)
            && let Some(at) = p.children.iter().position(|&c| c == child)
        {
            p.children.remove(at);
        }
        if let Some(c) = self.nodes.get_mut(&child) {
            c.parent = None;
        }
    }

    /// Marks a subtree removed; `settle` takes it out of `nodes`.
    fn free(&mut self, id: u64) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            // Its `children` must be current: a stale one may name children moved elsewhere.
            self.settle_parent(n);
            let Some(node) = self.nodes.get_mut(&n).filter(|node| !node.freed) else {
                continue;
            };
            node.freed = true;
            node.parent = None;
            stack.extend(&node.children);
            self.freed.push(n);
            self.unswept.push(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(id: u64) -> Op {
        Op::El(id, "DIV".into(), Props::default())
    }

    #[test]
    fn append_insert_move_remove() {
        let mut t = Tree::default();
        for op in [
            el(2),
            el(3),
            el(4),
            Op::Txt(5, "hi".into()),
            Op::Append(BODY, 2),
            Op::Append(BODY, 3),
            Op::Insert(BODY, 4, 3),
            Op::Append(2, 5),
        ] {
            t.apply(op);
        }
        t.settle();
        assert_eq!(t.get(BODY).children, [2, 4, 3]);
        assert_eq!(t.get(2).tag, "div");

        // Re-inserting an attached node moves it.
        t.apply(Op::Insert(BODY, 3, 2));
        t.settle();
        assert_eq!(t.get(BODY).children, [3, 2, 4]);

        // Detaching keeps the node for a later attach; removing frees the whole subtree.
        t.apply(Op::Detach(BODY, 2));
        t.settle();
        assert_eq!(t.get(BODY).children, [3, 4]);
        t.apply(Op::Append(BODY, 2));
        assert_eq!(t.get(BODY).children, [3, 4, 2]);
        assert_eq!(t.get(2).children, [5]);
        t.apply(Op::Remove(BODY, 2));
        t.settle();
        assert_eq!(t.get(BODY).children, [3, 4]);
        assert!(!t.nodes.contains_key(&2) && !t.nodes.contains_key(&5));
    }

    #[test]
    fn many_child_edits_in_a_batch_settle_in_order() {
        let mut t = Tree::default();
        for id in 2..42 {
            t.apply_deferred(el(id));
            t.apply_deferred(Op::Append(BODY, id));
        }
        for id in 2..32 {
            t.apply_deferred(Op::Remove(BODY, id));
        }
        t.apply_deferred(Op::Detach(BODY, 32));
        t.apply_deferred(Op::Append(BODY, 32));
        t.apply_deferred(Op::Insert(BODY, 41, 33));
        t.apply_deferred(Op::Insert(BODY, 40, 99));
        t.settle();
        assert_eq!(
            t.get(BODY).children,
            [41, 33, 34, 35, 36, 37, 38, 39, 32, 40]
        );
        assert!(!t.nodes.contains_key(&2));
    }

    #[test]
    fn set_props_splits_classes() {
        let mut t = Tree::default();
        t.apply(el(2));
        t.apply(Op::Set(
            2,
            Props {
                class: " a  b ".into(),
                ..Default::default()
            },
        ));
        assert_eq!(t.get(2).classes, ["a", "b"]);
    }
}
