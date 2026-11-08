// Editable content and the DOM selection: `contenteditable` elements, `Range`, `Selection` and
// `MutationObserver`, as rich text editors (Lexical, ProseMirror/TipTap, Slate, Quill) use them.
// JS owns the document and the selection, as a browser's DOM does. The host draws the caret and
// the selected text, maps the pointer and vertical caret moves to (node, offset) points, and sends
// what is typed (src/editing.rs). Editing defaults (typing, Backspace, Enter, paste) run here,
// after `beforeinput`, which editors usually cancel to edit their own model instead.

// The renderer's node primitives and events (see `createEditing`).
let dom = null;

const isText = (n) => n != null && typeof n.text === "string" && n.type === undefined && n.nodeType !== 8;
const kidsOf = (n) => n?.kids ?? [];
const nodeLength = (n) => (isText(n) ? n.text.length : kidsOf(n).length);
const indexOf = (n) => kidsOf(n.parent).indexOf(n);
const domError = (message, name) => new DOMException(message, name);

function isInclusiveAncestor(a, n) {
  for (; n; n = n.parent) if (n === a) return true;
  return false;
}
function chain(n) {
  const out = [];
  for (; n; n = n.parent) out.push(n);
  return out.reverse();
}
// -1 if `a` comes before `b` in tree order (an ancestor comes before its descendants).
function treeOrder(a, b) {
  if (a === b) return 0;
  const ca = chain(a);
  const cb = chain(b);
  if (ca[0] !== cb[0]) return 1;
  let i = 0;
  while (i < ca.length && i < cb.length && ca[i] === cb[i]) i++;
  if (i === ca.length) return -1;
  if (i === cb.length) return 1;
  return indexOf(ca[i]) < indexOf(cb[i]) ? -1 : 1;
}
/** Where boundary point (an, ao) is relative to (bn, bo): -1, 0 or 1, by the DOM's rules. */
export function comparePoints(an, ao, bn, bo) {
  if (an === bn) return Math.sign(ao - bo);
  if (treeOrder(an, bn) > 0) return -comparePoints(bn, bo, an, ao);
  if (isInclusiveAncestor(an, bn)) {
    let child = bn;
    while (child.parent !== an) child = child.parent;
    if (indexOf(child) < ao) return 1;
  }
  return -1;
}
function commonAncestor(a, b) {
  for (let n = a; n; n = n.parent) if (isInclusiveAncestor(n, b)) return n;
  return null;
}
function* preorder(n) {
  yield n;
  for (const k of [...kidsOf(n)]) yield* preorder(k);
}
// The text nodes a range covers, as [node, start, end], in order.
function* textPieces(r) {
  const { startContainer: sc, startOffset: so, endContainer: ec, endOffset: eo } = r;
  if (sc === ec && isText(sc)) return void (yield [sc, so, eo]);
  for (const n of preorder(commonAncestor(sc, ec))) {
    if (!isText(n)) continue;
    if (n !== sc && comparePoints(n, 0, sc, so) < 0) continue;
    if (n !== ec && comparePoints(n, 0, ec, eo) >= 0) break;
    yield [n, n === sc ? so : 0, n === ec ? eo : n.text.length];
  }
}

// Node edits through the renderer, so React-made nodes take them too and the host hears of them.
const setData = (n, text) => dom.setText(n, text);
const insertAt = (parent, offset, child) => dom.insert(parent, child, kidsOf(parent)[offset] ?? null);
const removeNode = (n) => n.parent && dom.remove(n.parent, n);
function shallowClone(n) {
  if (isText(n)) return dom.createText(n.text);
  const copy = dom.createElement(n.type, n.svg);
  for (const [k, v] of Object.entries(n.props ?? {})) if (typeof v === "string" && k !== "children") copy.setAttribute(k === "className" ? "class" : k, v);
  return copy;
}

export class Range {
  static START_TO_START = 0;
  static START_TO_END = 1;
  static END_TO_END = 2;
  static END_TO_START = 3;
  constructor() {
    const root = dom?.root() ?? null;
    this.startContainer = this.endContainer = root;
    this.startOffset = this.endOffset = 0;
  }
  get collapsed() {
    return this.startContainer === this.endContainer && this.startOffset === this.endOffset;
  }
  get commonAncestorContainer() {
    return commonAncestor(this.startContainer, this.endContainer);
  }
  setStart(node, offset) {
    if (offset > nodeLength(node)) throw domError("offset is past the node's length", "IndexSizeError");
    this.startContainer = node;
    this.startOffset = offset;
    if (chain(node)[0] !== chain(this.endContainer)[0] || comparePoints(node, offset, this.endContainer, this.endOffset) > 0) {
      this.endContainer = node;
      this.endOffset = offset;
    }
    selection.touched(this);
  }
  setEnd(node, offset) {
    if (offset > nodeLength(node)) throw domError("offset is past the node's length", "IndexSizeError");
    this.endContainer = node;
    this.endOffset = offset;
    if (chain(node)[0] !== chain(this.startContainer)[0] || comparePoints(node, offset, this.startContainer, this.startOffset) < 0) {
      this.startContainer = node;
      this.startOffset = offset;
    }
    selection.touched(this);
  }
  setStartBefore(n) {
    this.setStart(n.parent, indexOf(n));
  }
  setStartAfter(n) {
    this.setStart(n.parent, indexOf(n) + 1);
  }
  setEndBefore(n) {
    this.setEnd(n.parent, indexOf(n));
  }
  setEndAfter(n) {
    this.setEnd(n.parent, indexOf(n) + 1);
  }
  collapse(toStart = false) {
    if (toStart) [this.endContainer, this.endOffset] = [this.startContainer, this.startOffset];
    else [this.startContainer, this.startOffset] = [this.endContainer, this.endOffset];
    selection.touched(this);
  }
  selectNode(n) {
    const i = indexOf(n);
    this.setStart(n.parent, i);
    this.setEnd(n.parent, i + 1);
  }
  selectNodeContents(n) {
    this.setStart(n, 0);
    this.setEnd(n, nodeLength(n));
  }
  cloneRange() {
    const r = new Range();
    Object.assign(r, { startContainer: this.startContainer, startOffset: this.startOffset, endContainer: this.endContainer, endOffset: this.endOffset });
    return r;
  }
  compareBoundaryPoints(how, other) {
    const [a, b] = {
      0: [["start", "start"]],
      1: [["end", "start"]],
      2: [["end", "end"]],
      3: [["start", "end"]],
    }[how][0];
    return comparePoints(this[a + "Container"], this[a + "Offset"], other[b + "Container"], other[b + "Offset"]);
  }
  comparePoint(node, offset) {
    if (comparePoints(node, offset, this.startContainer, this.startOffset) < 0) return -1;
    return comparePoints(node, offset, this.endContainer, this.endOffset) > 0 ? 1 : 0;
  }
  isPointInRange(node, offset) {
    return chain(node)[0] === chain(this.startContainer)[0] && this.comparePoint(node, offset) === 0;
  }
  intersectsNode(n) {
    if (!n.parent) return true;
    const i = indexOf(n);
    return comparePoints(n.parent, i, this.endContainer, this.endOffset) < 0 && comparePoints(n.parent, i + 1, this.startContainer, this.startOffset) > 0;
  }
  toString() {
    let s = "";
    for (const [n, a, b] of textPieces(this)) s += n.text.slice(a, b);
    return s;
  }
  // Top-level nodes wholly inside the range.
  contained() {
    const { startContainer: sc, startOffset: so, endContainer: ec, endOffset: eo } = this;
    const out = [];
    const walk = (n) =>
      kidsOf(n).forEach((k, i) => {
        if (comparePoints(n, i, sc, so) >= 0 && comparePoints(n, i + 1, ec, eo) <= 0) out.push(k);
        else if (isInclusiveAncestor(k, sc) || isInclusiveAncestor(k, ec)) walk(k);
      });
    walk(this.commonAncestorContainer);
    return out;
  }
  deleteContents() {
    if (this.collapsed) return;
    const { startContainer: sc, startOffset: so, endContainer: ec, endOffset: eo } = this;
    if (sc === ec && isText(sc)) {
      setData(sc, sc.text.slice(0, so) + sc.text.slice(eo));
      return this.setEnd(sc, so);
    }
    let [node, offset] = [sc, so];
    if (!isInclusiveAncestor(sc, ec)) {
      let ref = sc;
      while (ref.parent && !isInclusiveAncestor(ref.parent, ec)) ref = ref.parent;
      [node, offset] = [ref.parent, indexOf(ref) + 1];
    }
    const doomed = this.contained();
    if (isText(sc)) setData(sc, sc.text.slice(0, so));
    for (const n of doomed) removeNode(n);
    if (isText(ec)) setData(ec, ec.text.slice(eo));
    if (isText(sc)) [node, offset] = [sc, so];
    this.setStart(node, Math.min(offset, nodeLength(node)));
    this.collapse(true);
  }
  // A fragment of copies of what the range covers, its partly covered ancestors cut down to it.
  cloneContents() {
    const frag = dom.createFragment();
    const copy = (n, into) => {
      if (isText(n)) {
        const [a, b] = [n === this.startContainer ? this.startOffset : 0, n === this.endContainer ? this.endOffset : n.text.length];
        return void dom.insert(into, dom.createText(n.text.slice(a, b)), null);
      }
      const c = shallowClone(n);
      dom.insert(into, c, null);
      for (const k of kidsOf(n)) if (this.intersectsNode(k)) copy(k, c);
    };
    const top = this.commonAncestorContainer;
    if (isText(top)) copy(top, frag);
    else for (const k of kidsOf(top)) if (this.intersectsNode(k)) copy(k, frag);
    return frag;
  }
  // shortcut: copies then deletes, so the fragment's nodes aren't the originals; move them if an editor needs identity.
  extractContents() {
    const frag = this.cloneContents();
    this.deleteContents();
    return frag;
  }
  insertNode(n) {
    let { startContainer: parent, startOffset: offset } = this;
    const wasCollapsed = this.collapsed;
    if (isText(parent)) {
      const tail = dom.createText(parent.text.slice(offset));
      setData(parent, parent.text.slice(0, offset));
      offset = indexOf(parent) + 1;
      parent = parent.parent;
      insertAt(parent, offset, tail);
    }
    if (n.nodeType === 11) {
      const kids = [...kidsOf(n)];
      for (const [i, k] of kids.entries()) insertAt(parent, offset + i, k);
      if (wasCollapsed) this.setEnd(parent, offset + kids.length);
    } else {
      insertAt(parent, offset, n);
      if (wasCollapsed) this.setEnd(parent, offset + 1);
    }
  }
  surroundContents(n) {
    const contents = this.extractContents();
    for (const k of [...kidsOf(n)]) removeNode(k);
    for (const k of [...kidsOf(contents)]) dom.insert(n, k, null);
    this.insertNode(n);
    this.selectNode(n);
  }
  // shortcut: the box of the element holding the start, not the text's own lines; ask the host for line boxes when a toolbar must sit on the selected words.
  getBoundingClientRect() {
    const el = isText(this.startContainer) ? this.startContainer.parent : this.startContainer;
    return el?.getBoundingClientRect?.() ?? new DOMRect();
  }
  getClientRects() {
    return [this.getBoundingClientRect()];
  }
  createContextualFragment(markup) {
    const t = globalThis.document.createElement("template");
    t.innerHTML = markup;
    return t.content;
  }
  detach() {}
}

export class StaticRange {
  constructor({ startContainer, startOffset, endContainer, endOffset }) {
    Object.assign(this, { startContainer, startOffset, endContainer, endOffset });
  }
  get collapsed() {
    return this.startContainer === this.endContainer && this.startOffset === this.endOffset;
  }
}

class DOMRect {
  x = 0;
  y = 0;
  width = 0;
  height = 0;
  top = 0;
  left = 0;
  right = 0;
  bottom = 0;
}

/** The document's selection: one range and its direction. */
export class Selection {
  #range = null;
  #backward = false;
  #anchor() {
    const r = this.#range;
    return this.#backward ? [r.endContainer, r.endOffset] : [r.startContainer, r.startOffset];
  }
  #focus() {
    const r = this.#range;
    return this.#backward ? [r.startContainer, r.startOffset] : [r.endContainer, r.endOffset];
  }
  get rangeCount() {
    return this.#range ? 1 : 0;
  }
  get anchorNode() {
    return this.#range ? this.#anchor()[0] : null;
  }
  get anchorOffset() {
    return this.#range ? this.#anchor()[1] : 0;
  }
  get focusNode() {
    return this.#range ? this.#focus()[0] : null;
  }
  get focusOffset() {
    return this.#range ? this.#focus()[1] : 0;
  }
  get isCollapsed() {
    return !this.#range || this.#range.collapsed;
  }
  get type() {
    return !this.#range ? "None" : this.#range.collapsed ? "Caret" : "Range";
  }
  get direction() {
    return !this.#range || this.#range.collapsed ? "none" : this.#backward ? "backward" : "forward";
  }
  getRangeAt(i) {
    if (i !== 0 || !this.#range) throw domError("no range at that index", "IndexSizeError");
    return this.#range;
  }
  addRange(r) {
    if (this.#range) return;
    this.#set(r, false);
  }
  removeRange(r) {
    if (r === this.#range) this.#set(null, false);
  }
  removeAllRanges() {
    if (this.#range) this.#set(null, false);
  }
  empty() {
    this.removeAllRanges();
  }
  collapse(node, offset = 0) {
    if (!node) return this.removeAllRanges();
    const r = new Range();
    r.setStart(node, offset);
    r.setEnd(node, offset);
    this.#set(r, false);
  }
  setPosition(node, offset) {
    this.collapse(node, offset);
  }
  collapseToStart() {
    if (this.#range) this.collapse(this.#range.startContainer, this.#range.startOffset);
  }
  collapseToEnd() {
    if (this.#range) this.collapse(this.#range.endContainer, this.#range.endOffset);
  }
  extend(node, offset = 0) {
    if (!this.#range) throw domError("nothing is selected", "InvalidStateError");
    const [an, ao] = this.#anchor();
    this.setBaseAndExtent(an, ao, node, offset);
  }
  setBaseAndExtent(an, ao, fn, fo) {
    const r = new Range();
    const backward = comparePoints(fn, fo, an, ao) < 0;
    r.setStart(...(backward ? [fn, fo] : [an, ao]));
    r.setEnd(...(backward ? [an, ao] : [fn, fo]));
    this.#set(r, backward);
  }
  selectAllChildren(n) {
    this.setBaseAndExtent(n, 0, n, nodeLength(n));
  }
  containsNode(n, partly = false) {
    const r = this.#range;
    if (!r || !n.parent) return false;
    const i = indexOf(n);
    if (partly) return r.intersectsNode(n);
    return comparePoints(n.parent, i, r.startContainer, r.startOffset) >= 0 && comparePoints(n.parent, i + 1, r.endContainer, r.endOffset) <= 0;
  }
  deleteFromDocument() {
    this.#range?.deleteContents();
  }
  // Moves the focus (or the caret, for "move") as arrow keys do; lines are the caret's block.
  // shortcut: "line" granularity moves to the block's ends; the host knows wrapped lines.
  modify(alter, direction, granularity) {
    if (!this.#range) return;
    const host = editingHost(this.focusNode);
    const forward = direction === "forward" || direction === "right";
    const to = moved([this.focusNode, this.focusOffset], forward, granularity, host);
    if (!to) return;
    if (alter === "extend") this.extend(...to);
    else this.collapse(...to);
  }
  toString() {
    return this.#range?.toString() ?? "";
  }
  #set(r, backward) {
    this.#range = r;
    this.#backward = backward;
    changed();
  }
  /** A range of this selection's was set through its own methods. */
  touched(r) {
    if (r === this.#range) changed();
  }
  /** The live range, for the renderer's node edits to keep current. */
  get live() {
    return this.#range;
  }
}

/** The document's selection (`getSelection()`). */
export const selection = new Selection();
let selectionNotice = null;
// `selectionchange` fires on `document` in a later task, once for any number of changes.
function changed() {
  mirrorDirty = true;
  dom?.later();
  selectionNotice ??= setTimeout(() => {
    selectionNotice = null;
    globalThis.document?.dispatchEvent?.(new Event("selectionchange"));
  });
}

// --- Keeping the selection where it was as nodes change (the DOM's live range rules) ---
// shortcut: only the selection's range is live; other ranges keep their offsets.

function adjust(fix) {
  const r = selection.live;
  if (!r) return;
  const [s, e] = [fix(r.startContainer, r.startOffset), fix(r.endContainer, r.endOffset)];
  if (s) [r.startContainer, r.startOffset] = s;
  if (e) [r.endContainer, r.endOffset] = e;
  if (s || e) changed();
}

/** Before `child` leaves `parent`. */
function removing(parent, child) {
  if (observers.size) record("childList", parent, () => ({ removedNodes: [child], previousSibling: child.previousSibling ?? null, nextSibling: child.nextSibling ?? null }));
  const r = selection.live;
  if (!r) return;
  // Only a point after another child of `parent`, or inside `child`, moves. Reading the index
  // otherwise would settle `parent`'s batched child edits on every removal (a cleared list).
  const touches = (n, o) => (n === parent && o > 0) || isInclusiveAncestor(child, n);
  if (!touches(r.startContainer, r.startOffset) && !touches(r.endContainer, r.endOffset)) return;
  const i = indexOf(child);
  adjust((n, o) => (isInclusiveAncestor(child, n) ? [parent, i] : n === parent && o > i ? [n, o - 1] : null));
}
/** After `child` joined `parent`. */
function inserted(parent, child) {
  if (observers.size) record("childList", parent, () => ({ addedNodes: [child], previousSibling: child.previousSibling ?? null, nextSibling: child.nextSibling ?? null }));
  const r = selection.live;
  if (!r || (r.startContainer !== parent && r.endContainer !== parent)) return;
  const i = indexOf(child);
  adjust((n, o) => (n === parent && o > i ? [n, o + 1] : null));
}
/** After a text node's data went from `old` to its current text: points in it move as the DOM's
 * "replace data" moves them, for the part that changed. */
function textChanged(n, old) {
  if (observers.size) record("characterData", n, () => ({ oldValue: old }));
  const r = selection.live;
  if (!r || (r.startContainer !== n && r.endContainer !== n)) return;
  const now = n.text;
  let p = 0;
  while (p < old.length && p < now.length && old[p] === now[p]) p++;
  let s = 0;
  while (s < old.length - p && s < now.length - p && old[old.length - 1 - s] === now[now.length - 1 - s]) s++;
  const [count, added] = [old.length - p - s, now.length - p - s];
  adjust((node, o) => (node !== n ? null : o > p && o <= p + count ? [n, p] : o > p + count ? [n, o + added - count] : null));
}
function attributeChanged(n, name, old) {
  if (observers.size) record("attributes", n, () => ({ attributeName: name, oldValue: old }));
}

// --- MutationObserver ---

const observers = new Set();
let delivery = false;
function record(type, target, details) {
  for (const o of observers) o._consider(type, target, details);
}
function deliver() {
  delivery = false;
  for (const o of [...observers]) {
    const records = o.takeRecords();
    if (records.length) o._callback(records, o);
  }
}

export class MutationObserver {
  #targets = new Map();
  #records = [];
  constructor(callback) {
    if (typeof callback !== "function") throw new TypeError("MutationObserver needs a callback");
    this._callback = callback;
  }
  observe(target, options = {}) {
    const o = { ...options };
    if (o.attributeOldValue || o.attributeFilter) o.attributes ??= true;
    if (o.characterDataOldValue) o.characterData ??= true;
    if (!o.childList && !o.attributes && !o.characterData) throw new TypeError("observe() needs childList, attributes or characterData");
    this.#targets.set(target, o);
    observers.add(this);
  }
  disconnect() {
    this.#targets.clear();
    this.#records = [];
    observers.delete(this);
  }
  takeRecords() {
    const r = this.#records;
    this.#records = [];
    return r;
  }
  _consider(type, target, details) {
    for (let n = target; n; n = n.parent) {
      const o = this.#targets.get(n);
      if (!o || (n !== target && !o.subtree) || !o[type]) continue;
      const fields = details();
      if (type === "attributes" && o.attributeFilter && !o.attributeFilter.includes(fields.attributeName)) return;
      const keepOld = type === "attributes" ? o.attributeOldValue : type === "characterData" ? o.characterDataOldValue : false;
      this.#records.push({ type, target, addedNodes: [], removedNodes: [], previousSibling: null, nextSibling: null, attributeName: null, attributeNamespace: null, ...fields, oldValue: keepOld ? (fields.oldValue ?? null) : null });
      if (!delivery) {
        delivery = true;
        queueMicrotask(deliver);
      }
      return;
    }
  }
}

// --- Editing hosts ---

const EDITABLE = new Set(["", "true", "plaintext-only"]);
function editableAttr(n) {
  const v = n?.props?.contentEditable ?? n?.props?.contenteditable;
  return v == null ? null : String(v).toLowerCase();
}
/** Whether a node's content can be edited: inside an element with `contenteditable`. */
export function isContentEditable(n) {
  for (let x = isText(n) ? n.parent : n; x; x = x.parent) {
    const v = editableAttr(x);
    if (v === "false") return false;
    if (v != null && v !== "inherit") return EDITABLE.has(v);
  }
  return false;
}
/** The outermost editable element around `n`, which takes focus and the `beforeinput`/`input` events. */
export function editingHost(n) {
  let x = isText(n) ? n?.parent : n;
  if (!x || !isContentEditable(x)) return null;
  while (x.parent && isContentEditable(x.parent)) x = x.parent;
  return x;
}
const plainOnly = (host) => editableAttr(host) === "plaintext-only";

const BLOCKS = new Set("address article aside blockquote dd div dl dt figcaption figure footer form h1 h2 h3 h4 h5 h6 header li main nav ol p pre section table td th tr ul".split(" "));
function blockOf(n, host) {
  let x = isText(n) ? n.parent : n;
  while (x && x !== host && !BLOCKS.has(x.type)) x = x.parent;
  return x ?? host;
}
// The caret stops: text nodes and line breaks, in order.
function leaves(host) {
  return [...preorder(host)].filter((n) => isText(n) || n.type === "br");
}
const hasContent = (block) => leaves(block).some((n) => n.type === "br" || n.text.length);

const segmenters = {};
const segmenter = (granularity) => (segmenters[granularity] ??= new Intl.Segmenter(undefined, { granularity }));
function nextBoundary(text, offset, forward, granularity) {
  const breaks = [0];
  for (const { index, isWordLike } of segmenter(granularity).segment(text)) if (granularity !== "word" || isWordLike || index > 0) breaks.push(index);
  breaks.push(text.length);
  const unique = [...new Set(breaks)].sort((a, b) => a - b);
  if (granularity === "word") {
    // A word step lands at the next word's end going forward, its start going back.
    const words = [...segmenter("word").segment(text)].filter((s) => s.isWordLike);
    if (forward) return words.map((w) => w.index + w.segment.length).find((e) => e > offset) ?? null;
    return words.map((w) => w.index).findLast((s) => s < offset) ?? null;
  }
  return forward ? (unique.find((b) => b > offset) ?? null) : (unique.findLast((b) => b < offset) ?? null);
}

// A point as the caret's: in a text node, or before a line break, when there is one at or near it.
function leafPoint(node, offset) {
  if (isText(node)) return [node, offset];
  const kids = kidsOf(node);
  const at = kids[offset];
  if (at) return isText(at) || at.type === "br" ? [at, 0] : leafPoint(at, 0);
  const before = kids[offset - 1];
  if (!before) return [node, offset];
  if (isText(before)) return [before, before.text.length];
  if (before.type === "br") return [before, 0];
  return leafPoint(before, nodeLength(before));
}
// A (text node | <br>, offset) as a DOM point.
const domPoint = (leaf, offset) => (leaf.type === "br" ? [leaf.parent, indexOf(leaf) + offset] : [leaf, offset]);

/** Where the caret goes from `point`, one `granularity` step on: null at the host's end. */
function moved([node, offset], forward, granularity, host) {
  if (!host) return null;
  const [leaf, at] = leafPoint(node, offset);
  if (granularity === "line" || granularity === "lineboundary" || granularity === "paragraph" || granularity === "paragraphboundary") {
    const block = blockOf(leaf, host);
    return forward ? [block, nodeLength(block)] : [block, 0];
  }
  if (granularity === "documentboundary") return forward ? [host, nodeLength(host)] : [host, 0];
  const unit = granularity === "word" ? "word" : "grapheme";
  if (isText(leaf)) {
    const b = nextBoundary(leaf.text, at, forward, unit);
    if (b != null) return [leaf, b];
  }
  const all = leaves(host);
  let i = all.indexOf(leaf);
  if (i < 0) return null;
  // A line break is a step of its own: past it is the next line's start.
  if (leaf.type === "br" && forward) {
    const next = all[i + 1];
    return next ? (isText(next) ? [next, 0] : domPoint(next, 0)) : domPoint(leaf, 1);
  }
  const block = blockOf(leaf, host);
  for (i += forward ? 1 : -1; i >= 0 && i < all.length; i += forward ? 1 : -1) {
    const n = all[i];
    const crossed = blockOf(n, host) !== block;
    // Past a line break in the same block is the next line's start; else the caret stops before it.
    if (n.type === "br") return domPoint(n, forward && !crossed ? 1 : 0);
    if (!n.text.length) continue;
    // Into another block the step is the block boundary itself; within one it passes a character.
    if (crossed) return [n, forward ? 0 : n.text.length];
    const b = nextBoundary(n.text, forward ? 0 : n.text.length, forward, unit);
    return [n, b ?? (forward ? n.text.length : 0)];
  }
  return null;
}

// --- Editing actions ---

const inputEvent = (type, fields) => {
  const { cancelable = false, ranges = [], ...rest } = fields;
  const e = Object.assign(new Event(type, { bubbles: true, cancelable, composed: true }), { data: null, dataTransfer: null, isComposing: false, ...rest });
  e.getTargetRanges = () => ranges;
  return e;
};
const staticRange = (r) => (r ? new StaticRange(r) : null);

// `beforeinput` at the editing host; false if a listener cancelled it.
function beforeInput(host, inputType, fields = {}) {
  const r = selection.live;
  const e = inputEvent("beforeinput", { inputType, cancelable: !fields.isComposing, ranges: r ? [staticRange(r)] : [], ...fields });
  dom.propagate(e, host);
  return !e.defaultPrevented;
}
function input(host, inputType, fields = {}) {
  dom.propagate(inputEvent("input", { inputType, ...fields }), host);
  dom.fire(host, "input");
}
// `beforeinput`, the default if nobody cancelled it, then `input`.
function edit(host, inputType, fields, run) {
  if (!beforeInput(host, inputType, fields)) return;
  run();
  input(host, inputType, fields);
}

// The selection's text and markup removed, merging the blocks at its ends; the caret goes there.
function deleteSelection(host) {
  const r = selection.live;
  if (!r || r.collapsed) return;
  const startBlock = blockOf(r.startContainer, host);
  const endBlock = blockOf(r.endContainer, host);
  r.deleteContents();
  const [node, offset] = [r.startContainer, r.startOffset];
  if (startBlock !== endBlock && isInclusiveAncestor(host, endBlock) && isInclusiveAncestor(host, startBlock)) mergeBlocks(startBlock, endBlock);
  if (!hasContent(startBlock)) dom.insert(startBlock, dom.createElement("br"), null);
  selection.collapse(node.parent || node === host ? node : startBlock, Math.min(offset, nodeLength(node)));
}
// `into` takes what is left of `from`, which goes (with ancestors it leaves empty).
function mergeBlocks(into, from) {
  const last = kidsOf(into).at(-1);
  if (last?.type === "br") removeNode(last);
  for (const k of [...kidsOf(from)]) dom.insert(into, k, null);
  let gone = from;
  while (gone.parent && kidsOf(gone.parent).length === 1 && !isInclusiveAncestor(gone.parent, into)) gone = gone.parent;
  removeNode(gone);
}

function insertText(host, text) {
  deleteSelection(host);
  let [node, offset] = [selection.focusNode, selection.focusOffset];
  if (!node) return;
  // An empty block's line break only held it open; text takes its place.
  const block = blockOf(node, host);
  const last = leaves(block).at(-1);
  const placeholder = text && last?.type === "br" && !hasText(block) ? last : null;
  if (!isText(node)) {
    const kids = kidsOf(node);
    const [before, after] = [kids[offset - 1], kids[offset]];
    if (isText(before)) [node, offset] = [before, before.text.length];
    else if (isText(after)) [node, offset] = [after, 0];
    else {
      const t = dom.createText("");
      insertAt(node, offset, t);
      [node, offset] = [t, 0];
    }
  }
  setData(node, node.text.slice(0, offset) + text + node.text.slice(offset));
  if (placeholder) removeNode(placeholder);
  selection.collapse(node, offset + text.length);
}
const hasText = (block) => leaves(block).some((n) => isText(n) && n.text.length);

// Enter: the block splits at the caret, its inline ancestors with it.
function insertParagraph(host) {
  deleteSelection(host);
  const [node, offset] = [selection.focusNode, selection.focusOffset];
  const block = blockOf(node, host);
  if (block === host || plainOnly(host)) return insertLineBreak(host);
  let [cur, off, right] = [node, offset, null];
  if (isText(cur)) {
    const tail = cur.text.slice(off);
    setData(cur, cur.text.slice(0, off));
    right = tail ? dom.createText(tail) : null;
    [off, cur] = [indexOf(cur) + 1, cur.parent];
  }
  for (;;) {
    const clone = shallowClone(cur);
    if (right) dom.insert(clone, right, null);
    for (const k of kidsOf(cur).slice(off)) dom.insert(clone, k, null);
    if (cur === block) {
      insertAt(block.parent, indexOf(block) + 1, clone);
      for (const b of [block, clone]) if (!hasContent(b)) dom.insert(b, dom.createElement("br"), null);
      const [leaf, at] = leafPoint(clone, 0);
      return selection.collapse(...domPoint(leaf, at));
    }
    [right, off, cur] = [clone, indexOf(cur) + 1, cur.parent];
  }
}

// Shift+Enter: a `<br>`, and another after it at a block's end, which gives the new line its height.
function insertLineBreak(host) {
  deleteSelection(host);
  let [node, offset] = [selection.focusNode, selection.focusOffset];
  if (isText(node)) {
    if (offset < node.text.length) {
      const tail = dom.createText(node.text.slice(offset));
      setData(node, node.text.slice(0, offset));
      insertAt(node.parent, indexOf(node) + 1, tail);
    }
    [node, offset] = [node.parent, indexOf(node) + (offset > 0 ? 1 : 0)];
  }
  const br = dom.createElement("br");
  insertAt(node, offset, br);
  const block = blockOf(br, host);
  if (leaves(block).at(-1) === br) dom.insert(br.parent, dom.createElement("br"), kidsOf(br.parent)[indexOf(br) + 1] ?? null);
  selection.collapse(node, offset + 1);
}

// Backspace or Delete with nothing selected: the selection grows by one step first.
function deleteStep(host, forward, granularity) {
  if (selection.isCollapsed) {
    const to = moved([selection.focusNode, selection.focusOffset], forward, granularity, host);
    if (!to) return;
    selection.extend(...to);
  }
  deleteSelection(host);
}

const LINE = "lineboundary";
function deletion(host, inputType) {
  const [forward, granularity] = {
    deleteContentBackward: [false, "character"],
    deleteContentForward: [true, "character"],
    deleteWordBackward: [false, "word"],
    deleteWordForward: [true, "word"],
    deleteSoftLineBackward: [false, LINE],
    deleteSoftLineForward: [true, LINE],
  }[inputType];
  // The ranges it removes, for `getTargetRanges()`.
  if (!selection.isCollapsed) return edit(host, inputType, {}, () => deleteSelection(host));
  const to = moved([selection.focusNode, selection.focusOffset], forward, granularity, host);
  if (!to) return;
  const r = new Range();
  const [fn, fo] = [selection.focusNode, selection.focusOffset];
  if (forward) r.setStart(fn, fo), r.setEnd(...to);
  else r.setStart(...to), r.setEnd(fn, fo);
  edit(host, inputType, { ranges: [staticRange(r)] }, () => deleteStep(host, forward, granularity));
}

// --- Clipboard: the system's, through pbcopy/pbpaste; the app's own HTML kept for pasting back ---

class DataTransfer {
  #data = new Map();
  dropEffect = "none";
  effectAllowed = "all";
  files = [];
  items = [];
  get types() {
    return [...this.#data.keys()];
  }
  getData(type) {
    return this.#data.get(type === "text" ? "text/plain" : type) ?? "";
  }
  setData(type, value) {
    this.#data.set(type === "text" ? "text/plain" : type, String(value));
  }
  clearData(type) {
    if (type) this.#data.delete(type);
    else this.#data.clear();
  }
}
export { DataTransfer };

// What this app last put on the clipboard: its HTML comes back when the text there is still that.
let copied = null;
function readClipboard() {
  const data = new DataTransfer();
  const text = Bun.spawnSync(["pbpaste"]).stdout.toString();
  data.setData("text/plain", text);
  if (copied?.["text/plain"] === text) for (const [k, v] of Object.entries(copied)) data.setData(k, v);
  return data;
}
function writeClipboard(data) {
  const text = data.getData("text/plain");
  Bun.spawnSync(["pbcopy"], { stdin: Buffer.from(text) });
  copied = Object.fromEntries(data.types.map((t) => [t, data.getData(t)]));
}
function selectionHtml() {
  const frag = selection.live.cloneContents();
  return kidsOf(frag).map((k) => (isText(k) ? k.text.replace(/&/g, "&amp;").replace(/</g, "&lt;") : (k.outerHTML ?? ""))).join("");
}
function clipboardEvent(host, type, data) {
  const e = Object.assign(new Event(type, { bubbles: true, cancelable: true, composed: true }), { clipboardData: data });
  dom.propagate(e, host);
  return !e.defaultPrevented;
}
function copy(host, cut) {
  if (selection.isCollapsed) return;
  const data = new DataTransfer();
  if (clipboardEvent(host, cut ? "cut" : "copy", data)) {
    data.setData("text/plain", selection.toString());
    data.setData("text/html", selectionHtml());
    writeClipboard(data);
    if (cut) edit(host, "deleteByCut", {}, () => deleteSelection(host));
  } else if (data.types.length) {
    writeClipboard(data);
  }
}
function paste(host) {
  const data = readClipboard();
  if (!clipboardEvent(host, "paste", data)) return;
  // shortcut: pastes as text, line by line; parse `text/html` when pasted markup must keep its formatting.
  edit(host, "insertFromPaste", { dataTransfer: data }, () => {
    const lines = data.getData("text/plain").replace(/\r\n?/g, "\n").split("\n");
    lines.forEach((line, i) => {
      if (i) insertParagraph(host);
      if (line) insertText(host, line);
    });
  });
}

// --- Composition (IME) ---

let composing = null; // { node, start, length }
function compose(host, text, done) {
  if (!composing) {
    if (done) return edit(host, "insertText", { data: text }, () => insertText(host, text));
    dom.propagate(Object.assign(new Event("compositionstart", { bubbles: true }), { data: selection.toString() }), host);
    insertText(host, "");
    composing = { node: selection.focusNode, start: selection.focusOffset, length: 0 };
  }
  const { node, start, length } = composing;
  const fields = { data: text, isComposing: !done };
  beforeInput(host, "insertCompositionText", { ...fields, isComposing: true });
  if (isText(node)) {
    setData(node, node.text.slice(0, start) + text + node.text.slice(start + length));
    selection.collapse(node, start + text.length);
  }
  composing.length = text.length;
  if (!done) dom.propagate(Object.assign(new Event("compositionupdate", { bubbles: true }), { data: text }), host);
  input(host, "insertCompositionText", fields);
  if (done) {
    composing = null;
    dom.propagate(Object.assign(new Event("compositionend", { bubbles: true }), { data: text }), host);
  }
}

// --- Host messages: the caret drawn, and what the host reports ---

let mirrorDirty = true;
let mirrored = "";
function mirror() {
  const focused = dom.focused();
  const host = focused && isContentEditable(focused) ? editingHost(focused) : null;
  const r = selection.live;
  const msg = { t: "selection", host: host?.hid ?? null, caret: null, ranges: [] };
  if (host && r && isInclusiveAncestor(host, r.startContainer) && isInclusiveAncestor(host, r.endContainer)) {
    const [leaf, at] = leafPoint(selection.focusNode, selection.focusOffset);
    msg.caret = [leaf.hid, at];
    if (!r.collapsed) for (const [n, a, b] of textPieces(r)) if (b > a) msg.ranges.push([n.hid, a, b]);
  }
  return msg;
}

// A word, or the block, around a point (double and triple clicks).
function selectAround(host, node, offset, detail) {
  if (detail >= 3) {
    const block = blockOf(node, host);
    return selection.setBaseAndExtent(block, 0, block, nodeLength(block));
  }
  if (!isText(node)) return selection.collapse(node, offset);
  const word = [...segmenter("word").segment(node.text)].find((s) => s.index <= offset && offset <= s.index + s.segment.length && (s.isWordLike || s.index + s.segment.length > offset));
  if (!word) return selection.collapse(node, offset);
  selection.setBaseAndExtent(node, word.index, node, word.index + word.segment.length);
}

/**
 * Editing for the renderer: `primitives` are its node edits (`insert`, `remove`, `setText`,
 * `createText`, `createElement`, `createFragment`), `root()`, `instance(id)`, `focused()`,
 * `setFocus(node)`, `propagate(event, target)` (DOM listeners), `fire(target, type)` (React
 * handlers), `send(msg)` and `later()` (a flush at the end of the task).
 */
export function createEditing(primitives) {
  dom = primitives;
  return {
    selection,
    removing,
    inserted,
    textChanged,
    attributeChanged,
    /** Sends the host the selection to draw, when it changed. */
    sync() {
      if (!mirrorDirty) return;
      mirrorDirty = false;
      const msg = mirror();
      const text = JSON.stringify(msg);
      if (text !== mirrored) {
        mirrored = text;
        dom.send(msg);
      }
    },
    /** Focus moved: an editing host taking it without a caret inside gets one at its start. */
    focusChanged(node) {
      mirrorDirty = true;
      const host = node && isContentEditable(node) ? editingHost(node) : null;
      const r = selection.live;
      if (host && !(r && isInclusiveAncestor(host, r.startContainer))) {
        const [leaf, at] = leafPoint(host, 0);
        selection.collapse(...domPoint(leaf, at));
      }
    },
    /** A host event for an editing host: `caret` (a press or drag placed it), `textinput`, `composition`. */
    hostEvent(target, msg) {
      const host = editingHost(target) ?? target;
      if (msg.type === "caret") {
        const n = dom.instance(msg.node);
        if (!n) return;
        if (dom.focused() !== host && !isInclusiveAncestor(host, dom.focused())) dom.setFocus(host);
        const [node, offset] = domPoint(n, Math.min(msg.offset, nodeLength(n)));
        if ((msg.extend || msg.drag) && selection.rangeCount) selection.extend(node, offset);
        else if (msg.detail >= 2) selectAround(host, node, offset, msg.detail);
        else selection.collapse(node, offset);
        return;
      }
      if (editingHost(dom.focused()) !== host) return;
      if (msg.type === "composition") return compose(host, msg.text, false);
      // Return, Tab and other control characters arrive as keys.
      if (/^[\x00-\x1f\x7f]*$/.test(msg.text)) return;
      if (composing) return compose(host, msg.text, true);
      // The input method replacing text before the caret in its node (an accent picked for a letter).
      const node = selection.focusNode;
      if (msg.replace && isText(node)) selection.setBaseAndExtent(node, Math.min(msg.replace[0], node.text.length), node, Math.min(msg.replace[1], node.text.length));
      edit(host, "insertText", { data: msg.text }, () => insertText(host, msg.text));
    },
    /** A key the focused element's listeners didn't cancel: the editing it does by default. */
    keydown(msg) {
      const host = editingHost(dom.focused());
      if (!host || composing) return false;
      const { key, metaKey: meta, altKey: alt, shiftKey: shift, ctrlKey: ctrl } = msg;
      const command = meta || ctrl;
      const go = (forward, granularity) => {
        let to = msg.caretTo ? domPoint(dom.instance(msg.caretTo[0]) ?? selection.focusNode, msg.caretTo[1]) : null;
        // Without extending, a selection collapses to its side in the direction of travel.
        if (!to && !shift && !selection.isCollapsed && granularity === "character") {
          const r = selection.live;
          to = forward ? [r.endContainer, r.endOffset] : [r.startContainer, r.startOffset];
        }
        to ??= moved([selection.focusNode, selection.focusOffset], forward, granularity, host);
        if (!to) return;
        shift ? selection.extend(...to) : selection.collapse(...to);
      };
      switch (key) {
        case "Backspace":
          return deletion(host, command ? "deleteSoftLineBackward" : alt ? "deleteWordBackward" : "deleteContentBackward"), true;
        case "Delete":
          return deletion(host, alt ? "deleteWordForward" : "deleteContentForward"), true;
        case "Enter":
          if (shift || plainOnly(host)) edit(host, "insertLineBreak", {}, () => insertLineBreak(host));
          else edit(host, "insertParagraph", {}, () => insertParagraph(host));
          return true;
        case "ArrowLeft":
        case "ArrowRight":
          return go(key === "ArrowRight", meta ? LINE : alt ? "word" : "character"), true;
        case "ArrowUp":
        case "ArrowDown":
          return go(key === "ArrowDown", meta ? "documentboundary" : LINE), true;
        case "Home":
        case "End":
          return go(key === "End", LINE), true;
      }
      if (!command) return false;
      switch (key.toLowerCase()) {
        case "a":
          return selection.selectAllChildren(host), true;
        case "z":
          return beforeInput(host, shift ? "historyRedo" : "historyUndo"), true;
        case "y":
          return beforeInput(host, "historyRedo"), true;
        case "b":
          return beforeInput(host, "formatBold"), true;
        case "i":
          return beforeInput(host, "formatItalic"), true;
        case "u":
          return beforeInput(host, "formatUnderline"), true;
        case "c":
          return copy(host, false), true;
        case "x":
          return copy(host, true), true;
        case "v":
          return paste(host), true;
      }
      return false;
    },
  };
}
