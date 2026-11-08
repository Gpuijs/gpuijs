// React host config that mirrors host instances into the Rust tree as op batches (one per commit).
import { createContext } from "react";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import Reconciler from "react-reconciler";
import { parseMarkup } from "./markup.js";
import { bbox, hitTest, rootBox } from "./svg-geometry.js";
import {
  ConcurrentRoot,
  ContinuousEventPriority,
  DefaultEventPriority,
  DiscreteEventPriority,
  NoEventPriority,
} from "react-reconciler/constants.js";
import { styleToCss } from "./style.js";
import { createEditing, isContentEditable } from "./editing.js";
import { DetailsNode, DialogNode, ImageNode, imageSize } from "./elements.js";
import { CanvasNode, MediaNode, mediaUpdate, skia } from "./media.js";
import { decodeImage } from "./webgl.js";
import { waitForHost } from "./transport.js";

// React prop -> wire event name. Functions stay in JS; the host only learns which events to report.
const EVENTS = {
  onClick: "click",
  onMouseDown: "mousedown",
  onPointerDown: "mousedown",
  onMouseUp: "mouseup",
  onPointerUp: "mouseup",
  onPointerEnter: "mouseenter",
  onPointerMove: "mousemove",
  onMouseMove: "mousemove",
  onPointerLeave: "mouseleave",
  onMouseEnter: "mouseenter",
  onMouseLeave: "mouseleave",
  onKeyDown: "keydown",
  onContextMenu: "contextmenu",
  onDoubleClick: "dblclick",
  onWheel: "wheel",
  onScroll: "scroll",
  onChange: "change",
  onInput: "change",
};
// React's onScroll doesn't bubble (React 17 on), as the DOM's `scroll` on elements doesn't.
const NON_BUBBLING_TYPES = ["mouseenter", "mouseleave", "scroll"];
// react-reconciler's fiber tag for host elements.
const HOST_COMPONENT = 5;
const NON_BUBBLING = new Set(NON_BUBBLING_TYPES);
const HANDLERS = {};
for (const [prop, type] of Object.entries(EVENTS)) (HANDLERS[type] ??= []).push(prop);
// Capture-phase props make the host report the event too.
for (const [prop, type] of Object.entries(EVENTS)) if (!NON_BUBBLING_TYPES.includes(type)) EVENTS[prop + "Capture"] = type;
// Key-ups come with each key-down (see `DOM_EVENTS.keydown`), so onKeyUp asks for key-downs.
EVENTS.onKeyUp = EVENTS.onKeyUpCapture = "keydown";
HANDLERS.keyup = ["onKeyUp"];
const EVENT_ENTRIES = Object.entries(EVENTS);
// Not reported by the host: synthesized here when a button or Enter submits a form.
HANDLERS.submit = ["onSubmit"];
// Editable content's `input` (see editing.js); a field's onInput comes with its `change`.
HANDLERS.input = ["onInput"];
// The DOM events each host mouse event becomes, in browser order, and whether they bubble.
// They reach listeners added with addEventListener on elements, then `document`.
const DOM_EVENTS = {
  mousedown: [["pointerdown", true], ["mousedown", true]],
  mouseup: [["pointerup", true], ["mouseup", true]],
  click: [["click", true]],
  mousemove: [["pointermove", true], ["mousemove", true]],
  mouseenter: [["pointerover", true], ["pointerenter", false], ["mouseover", true], ["mouseenter", false]],
  mouseleave: [["pointerout", true], ["pointerleave", false], ["mouseout", true], ["mouseleave", false]],
  dblclick: [["dblclick", true]],
  wheel: [["wheel", true]],
};
// DOM listener type -> the host event that produces it, so DOM-only listeners get reported too.
const WIRE_FOR_DOM = { keydown: "keydown" };
for (const [wire, types] of Object.entries(DOM_EVENTS)) for (const [type] of types) WIRE_FOR_DOM[type] = wire;
// Fields report these unasked, so they need no wiring. Typing is `input` (a <select> adds `change`):
// Vue's v-model and Solid's onInput listen for it.
// shortcut: keyup follows keydown at once; ask the host for real key-ups if timing matters.
WIRE_FOR_DOM.keyup = "keydown";
// The host reports every scroll container's offset as it changes: no wiring.
Object.assign(DOM_EVENTS, {
  scroll: [["scroll", false]],
  keydown: [["keydown", true], ["keyup", true]],
  change: [["input", true]],
  focus: [["focus", false], ["focusin", true]],
  blur: [["blur", false], ["focusout", true]],
});
// Media events, which the host reports (see `mediaEvent`); none bubble.
for (const prop of "onDurationChange onLoadedMetadata onLoadedData onCanPlay onCanPlayThrough onPlay onPlaying onPause onTimeUpdate onSeeked onEnded onError onVolumeChange onRateChange".split(" ")) {
  const type = prop.slice(2).toLowerCase();
  HANDLERS[type] = [prop];
  NON_BUBBLING.add(type);
}
for (const type of ["toggle", "cancel", "close", "load"]) {
  HANDLERS[type] = ["on" + type[0].toUpperCase() + type.slice(1)];
  NON_BUBBLING.add(type);
}
// React's onFocus/onBlur bubble (they are focusin/focusout).
HANDLERS.focus = ["onFocus"];
HANDLERS.blur = ["onBlur"];
// React prop -> attribute, for attributes selectors or the host read.
const ATTRS = { role: "role", type: "type", name: "name", href: "href", title: "title", htmlFor: "for", colSpan: "colspan", rowSpan: "rowspan", start: "start", min: "min", max: "max", step: "step", low: "low", high: "high", optimum: "optimum", rows: "rows", cols: "cols", size: "size", required: "required", readOnly: "readonly", width: "width", height: "height", autoPlay: "autoplay", autoplay: "autoplay", loop: "loop", muted: "muted", controls: "controls" };
const noop = () => {};

// SVG subtrees are serialized back to markup by the host, so their elements carry every attribute.
const SVG_TAGS = new Set(
  "svg g path circle ellipse line polyline polygon rect text tspan defs linearGradient radialGradient stop clipPath mask use symbol pattern title desc foreignObject".split(" "),
);
const SVG_SKIP = new Set(["children", "className", "style", "key", "ref", "dangerouslySetInnerHTML", "suppressHydrationWarning"]);
const SVG_CAMEL = new Set(
  "viewBox preserveAspectRatio gradientUnits gradientTransform patternUnits patternContentUnits patternTransform clipPathUnits maskUnits maskContentUnits markerWidth markerHeight refX refY textLength lengthAdjust spreadMethod stdDeviation pathLength startOffset".split(" "),
);
/** React prop name -> SVG attribute name, following react-dom (`strokeWidth` -> `stroke-width`). */
function svgAttr(k) {
  if (SVG_CAMEL.has(k)) return k;
  if (k === "xlinkHref") return "href";
  return k.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase());
}

// Child edits wait in `edits` until `kids` is read, then apply in one pass, over a linked list
// when there are many: a cleared list's rows leave one at a time and a keyed reorder moves
// hundreds of rows before one sibling, which splicing makes quadratic (a DOM's children are linked).
function settleKids(n) {
  const edits = n.edits;
  n.edits = null;
  const kids = n._kids;
  if (edits.length <= 8) {
    for (const [kind, c, before] of edits) {
      const at = kids.indexOf(c);
      if (at >= 0) kids.splice(at, 1);
      if (kind === "append") kids.push(c);
      // A sibling that isn't there puts it first, as `splice(-1 -> 0)` always did.
      else if (kind === "insert") kids.splice(Math.max(0, kids.indexOf(before)), 0, c);
    }
    return;
  }
  const next = new Map();
  const prev = new Map();
  let head = kids[0] ?? null;
  let tail = kids.at(-1) ?? null;
  for (let i = 0; i < kids.length; i++) {
    next.set(kids[i], kids[i + 1] ?? null);
    prev.set(kids[i], kids[i - 1] ?? null);
  }
  const unlink = (c) => {
    if (!next.has(c)) return;
    const p = prev.get(c);
    const q = next.get(c);
    p ? next.set(p, q) : (head = q);
    q ? prev.set(q, p) : (tail = p);
    next.delete(c);
    prev.delete(c);
  };
  const link = (c, q) => {
    const p = q ? prev.get(q) : tail;
    next.set(c, q);
    prev.set(c, p);
    p ? next.set(p, c) : (head = c);
    q ? prev.set(q, c) : (tail = c);
  };
  for (const [kind, c, before] of edits) {
    unlink(c);
    if (kind === "append") link(c, null);
    else if (kind === "insert") link(c, next.has(before) ? before : head);
  }
  n._kids = [];
  for (let c = head; c; c = next.get(c)) n._kids.push(c);
}

const isRemote = (src) => /^https?:\/\//i.test(src);
// The host only reads local files, so remote images are downloaded once, keyed by URL.
// The app's cache directory once packaged (`gpuijs build`), else the system's temp one.
const remoteDir = process.env.GPUIJS_CACHE_DIR ?? path.join(tmpdir(), "gpuijs-assets");
const cached = (url) => path.join(remoteDir, createHash("sha1").update(url).digest("hex"));
const downloads = new Map();

/** Downloads a remote `src` into the asset cache; resolves to the local file, or null when it fails. */
export function fetchAsset(url) {
  const file = cached(url);
  if (existsSync(file)) return Promise.resolve(file);
  if (!downloads.has(url)) {
    const download = fetch(url)
      .then(async (res) => {
        if (!res.ok) return null;
        mkdirSync(remoteDir, { recursive: true });
        // Written aside and renamed, so a half-written file is never taken for a cached one.
        const part = `${file}.${process.pid}.part`;
        writeFileSync(part, Buffer.from(await res.arrayBuffer()));
        renameSync(part, file);
        return file;
      })
      .catch(() => null);
    downloads.set(url, download);
    // Failures are retried the next time something asks.
    download.then((f) => f ?? downloads.delete(url));
  }
  return downloads.get(url);
}

/**
 * `/images/a.png` -> the file under the app's public/ dir when it exists; a remote URL -> its
 * downloaded copy once `fetchAsset` has it; other values pass through.
 */
export function assetPath(src) {
  if (isRemote(src)) {
    const file = cached(src);
    return existsSync(file) ? file : src;
  }
  // Resolved against the page's URL, as a browser does: `./images/a.png` on `/profile` is `/images/a.png`.
  const url = new URL(src, globalThis.location?.href ?? "http://localhost/");
  const pub = process.env.GPUIJS_PUBLIC && path.join(process.env.GPUIJS_PUBLIC, decodeURIComponent(url.pathname));
  return pub && url.protocol.startsWith("http") && existsSync(pub) ? pub : src;
}

const isCapture = (options) => options === true || options?.capture === true;
// A string passed to `append`/`before`/... becomes a text node, as in the DOM.
const asNode = (at, n) => (typeof n === "object" && n ? n : at.dom.createText(String(n)));
// `before`/`after`/`replaceWith`, which elements and text nodes share.
const CHILD_NODE = {
  before(...nodes) {
    for (const n of nodes) this.parent?.insertBefore(asNode(this, n), this);
  },
  after(...nodes) {
    const next = this.nextSibling;
    for (const n of nodes) this.parent?.insertBefore(asNode(this, n), next);
  },
  replaceWith(...nodes) {
    const parent = this.parent;
    if (!parent) return;
    const next = this.nextSibling;
    this.remove();
    for (const n of nodes) parent.insertBefore(asNode(this, n), next);
  },
};

const STREAMED = new Set(["video", "audio", "source"]);
const VOID_TAGS = new Set("area base br col embed hr img input link meta source track wbr".split(" "));
const ATTR_NAMES = { className: "class", htmlFor: "for", tabIndex: "tabindex" };
const SKIP_ATTRS = new Set(["children", "key", "ref", "dangerouslySetInnerHTML", "suppressHydrationWarning", "suppressContentEditableWarning"]);
const escapeText = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\u00a0/g, "&nbsp;");
// A node as markup, for `innerHTML`/`outerHTML` reads. React's text nodes are plain `{ text }`s.
function serialize(n) {
  if (n.nodeType === 8) return `<!--${n.comment}-->`;
  if (!n.type) return escapeText(n.text ?? "");
  if (n.nodeType === 11) return n.childNodes.map(serialize).join("");
  let out = `<${n.type}`;
  for (const [k, v] of Object.entries(n.props ?? {})) {
    if (k === "style" || SKIP_ATTRS.has(k) || v == null || typeof v === "function" || typeof v === "object") continue;
    if (v === false && !/^(data|aria)-/.test(k)) continue;
    out += ` ${ATTR_NAMES[k] ?? ATTRS[k] ?? k}="${String(v === true ? "" : v).replace(/&/g, "&amp;").replace(/"/g, "&quot;")}"`;
  }
  const css = styleToCss({ ...(typeof n.props?.style === "object" ? n.props.style : {}), ...n.styleWrites });
  if (css) out += ` style="${css.split(";").map((d) => d.replace(":", ": ")).join("; ")};"`;
  out += ">";
  if (VOID_TAGS.has(n.type)) return out;
  const kids = n.type === "template" ? n.content.childNodes : n.childNodes;
  return out + kids.map(serialize).join("") + `</${n.type}>`;
}

// Bun clears an event's stop flags when a native dispatch ends, and gpuijs dispatches one event
// through several targets (window, document, host nodes): the flags are kept here instead.
// 1: stopPropagation, 2: stopImmediatePropagation.
const STOPPED = Symbol("stopped");
export function trackStops(event) {
  if (event[STOPPED] !== undefined) return event;
  event[STOPPED] = 0;
  const { stopPropagation, stopImmediatePropagation } = event;
  return Object.defineProperties(event, {
    stopPropagation: { value: () => ((event[STOPPED] ||= 1), stopPropagation.call(event)), configurable: true },
    stopImmediatePropagation: { value: () => ((event[STOPPED] = 2), stopImmediatePropagation.call(event)), configurable: true },
    cancelBubble: { get: () => event[STOPPED] > 0, set: (v) => v && event.stopPropagation(), configurable: true },
  });
}
const at = (event, currentTarget, eventPhase) => Object.defineProperties(event, { currentTarget: { value: currentTarget, configurable: true }, eventPhase: { value: eventPhase, configurable: true } });
// `node`'s listeners for one phase, unless propagation was stopped.
function invoke(node, event, capture, phase) {
  const list = node.listeners?.[event.type];
  if (!list?.length || event[STOPPED]) return;
  at(event, node, phase);
  for (const l of [...list]) {
    if (event[STOPPED] === 2) return;
    if (l.capture !== capture || !node.listeners[event.type].includes(l)) continue;
    if (l.once) node.removeEventListener(event.type, l.listener, l.capture);
    typeof l.listener === "function" ? l.listener.call(node, event) : l.listener.handleEvent(event);
  }
}
// `window`'s and `document`'s capture listeners, which hear an event before anything else.
function captureOutside(event) {
  trackStops(event);
  for (const o of [globalThis, globalThis.document]) if (o?.dispatchCapture && !event[STOPPED]) at(event, o, 1), o.dispatchCapture(event);
}
// The DOM's dispatch: capture from `window` down to the target, the target, then bubbling back up.
// `outerCaptured`: `window`'s and `document`'s capture listeners have heard it already (host events
// run them before React's handlers).
function propagate(event, target, outerCaptured = false) {
  trackStops(event);
  const path = [];
  for (let n = target; n; n = n.parent) path.push(n);
  const outer = path.at(-1).type === "html" ? [globalThis.document, globalThis].filter(Boolean) : [];
  Object.defineProperty(event, "target", { value: target, configurable: true });
  if (!Object.hasOwn(event, "composedPath")) Object.defineProperty(event, "composedPath", { value: () => [...path, ...outer], configurable: true });
  if (!outerCaptured && outer.length) captureOutside(event);
  for (let i = path.length - 1; i > 0; i--) invoke(path[i], event, true, 1);
  invoke(target, event, true, 2);
  invoke(target, event, false, 2);
  if (!event.bubbles) return at(event, null, 0);
  for (let i = 1; i < path.length; i++) invoke(path[i], event, false, 3);
  for (const o of outer) if (!event[STOPPED]) at(event, o, 3), (o.dispatchBubble ?? o.dispatchEvent)?.call(o, event);
  at(event, null, 0);
}

// What a ref to a host element exposes. Libraries like Radix call DOM methods on refs in effects;
// gpuijs has no DOM, so these answer from the tree where they can and do nothing otherwise.
const HostNode = {
  nodeType: 1,
  get tagName() {
    // As in a browser, only HTML names are uppercased (ApexCharts walks up to `nodeName === "svg"`).
    return this.svg || SVG_TAGS.has(this.type) ? this.type : this.type.toUpperCase();
  },
  get nodeName() {
    return this.tagName;
  },
  get parentElement() {
    return this.parent;
  },
  // A checkbox's or radio's state: what a click set (see `toggle`), else what React rendered.
  get checked() {
    return this._checked ?? Boolean(this.props.checked ?? this.props.defaultChecked);
  },
  set checked(v) {
    this._checked = Boolean(v);
  },
  // An input's text. A script writing it (flatpickr shows the picked dates) shows it in the host.
  get value() {
    return this._value;
  },
  set value(v) {
    this._value = this.written = String(v);
    this.dom.touch(this);
  },
  get parentNode() {
    return this.parent;
  },
  get ownerDocument() {
    return globalThis.document;
  },
  // Connected means under <html>; the root of a connected node is the document.
  get isConnected() {
    let n = this;
    while (n.parent) n = n.parent;
    return n.type === "html";
  },
  getRootNode() {
    let n = this;
    while (n.parent) n = n.parent;
    return n.type === "html" ? globalThis.document : n;
  },
  // Real listeners: libraries dispatch their own events on nodes (Radix "pointer down outside").
  addEventListener(type, listener, options) {
    const signal = options?.signal;
    if (signal?.aborted) return;
    const capture = isCapture(options);
    const list = ((this.listeners ??= {})[type] ??= []);
    if (listener && !list.some((l) => l.listener === listener && l.capture === capture)) {
      list.push({ listener, capture, once: options?.once });
      signal?.addEventListener("abort", () => this.removeEventListener(type, listener, options), { once: true });
    }
    // The host only reports events something listens for.
    if (WIRE_FOR_DOM[type]) this.relisten?.();
  },
  removeEventListener(type, listener, options) {
    const list = this.listeners?.[type];
    const capture = isCapture(options);
    if (list) this.listeners[type] = list.filter((l) => l.listener !== listener || l.capture !== capture);
  },
  dispatchEvent(event) {
    propagate(event, this);
    return !event.defaultPrevented;
  },
  focus() {
    this.setFocus(this);
  },
  blur() {
    if (this.isFocused()) this.setFocus(null);
  },
  click() {
    this.dispatchHost?.({ id: this.hid, type: "click" });
  },
  // Web Animations: the host runs CSS animations itself, so none are scriptable here; Base UI then
  // treats a popup's exit animation as finished and unmounts it at once.
  // shortcut: no exit animation for libraries that wait on `animation.finished`; report the host's when one needs it.
  getAnimations: () => [],
  // `el.animate()` (kbar's palette) finishes at once: with `fill: forwards` its end state stays on
  // the element, as when a browser's animation has run.
  // shortcut: no tweening; run keyframes in the host when a library's motion matters.
  animate(keyframes, options) {
    const frames = Array.isArray(keyframes) ? keyframes : [];
    const end = options?.direction === "reverse" ? frames[0] : frames.at(-1);
    if (end && (options?.fill === "forwards" || options?.fill === "both")) {
      for (const [k, v] of Object.entries(end)) if (!["offset", "easing", "composite"].includes(k)) this.style[k] = v;
    }
    const animation = {
      finished: Promise.resolve(),
      playState: "finished",
      effect: { getTiming: () => ({ duration: 0, iterations: 1 }) },
      cancel: noop,
      finish: noop,
      play: noop,
      pause: noop,
      reverse: noop,
      addEventListener: (type, f) => type === "finish" && queueMicrotask(() => f({ type })),
      removeEventListener: noop,
      onfinish: null,
    };
    queueMicrotask(() => animation.onfinish?.({ type: "finish" }));
    animation.finished = animation.finished.then(() => animation);
    return animation;
  },
  contains(other) {
    for (let n = other; n; n = n.parent) if (n === this) return true;
    return false;
  },
  // `Node.DOCUMENT_POSITION_*` flags for where `other` is (MUI orders a menu's items by it).
  compareDocumentPosition(other) {
    if (other === this) return 0;
    const chain = (n) => {
      const out = [];
      for (; n; n = n.parent) out.unshift(n);
      return out;
    };
    const [a, b] = [chain(this), chain(other)];
    if (a[0] !== b[0]) return 1 | 32; // disconnected, implementation-specific
    let i = 0;
    while (a[i] === b[i]) i++;
    if (i === a.length) return 16 | 4; // inside this
    if (i === b.length) return 8 | 2; // contains this
    const kids = a[i - 1].kids;
    return kids.indexOf(b[i]) < kids.indexOf(a[i]) ? 2 : 4;
  },
  // Children, with the edits since the last read applied (see `settleKids`).
  get kids() {
    if (this.edits) settleKids(this);
    return this._kids;
  },
  set kids(v) {
    this._kids = v;
    this.edits = null;
  },
  get childNodes() {
    return this.kids ?? [];
  },
  hasChildNodes() {
    return this.childNodes.length > 0;
  },
  get childElementCount() {
    return this.children.length;
  },
  get firstElementChild() {
    return this.children[0] ?? null;
  },
  get lastElementChild() {
    return this.children.at(-1) ?? null;
  },
  get localName() {
    return this.type;
  },
  // Reflected attributes, as the DOM's IDL properties are.
  get id() {
    return this.getAttribute("id") ?? "";
  },
  set id(v) {
    this.setAttribute("id", v);
  },
  get hidden() {
    return this.hasAttribute("hidden");
  },
  set hidden(v) {
    this.toggleAttribute("hidden", Boolean(v));
  },
  // No shadow DOM here; Lexical walks `activeElement.shadowRoot` until it's null.
  shadowRoot: null,
  get isContentEditable() {
    return isContentEditable(this);
  },
  get contentEditable() {
    const v = this.props.contentEditable ?? this.props.contenteditable;
    return v == null ? "inherit" : String(v);
  },
  set contentEditable(v) {
    this.setAttribute("contenteditable", String(v));
  },
  get tabIndex() {
    const v = parseInt(this.getAttribute("tabindex"));
    return Number.isNaN(v) ? (INTERACTIVE.has(this.type) ? 0 : -1) : v;
  },
  set tabIndex(v) {
    this.setAttribute("tabindex", v);
  },
  get innerText() {
    return this.textContent;
  },
  set innerText(text) {
    this.textContent = text;
  },
  get children() {
    return (this.kids ?? []).filter((k) => k.type);
  },
  get firstChild() {
    return this.kids?.[0] ?? null;
  },
  get lastChild() {
    return this.kids?.at(-1) ?? null;
  },
  get nextSibling() {
    const kin = this.parent?.kids ?? [];
    return kin[kin.indexOf(this) + 1] ?? null;
  },
  get previousSibling() {
    const kin = this.parent?.kids ?? [];
    return kin[kin.indexOf(this) - 1] ?? null;
  },
  get textContent() {
    return (this.kids ?? []).map((k) => k.text ?? k.textContent).join("");
  },
  set textContent(text) {
    if (!this.imperative) return;
    for (const k of [...(this.kids ?? [])]) this.removeChild(k);
    if (text != null && text !== "") this.appendChild(this.dom.createText(String(text)));
  },
  get innerHTML() {
    return (this.type === "template" ? this.content : this).childNodes.map(serialize).join("");
  },
  get outerHTML() {
    return serialize(this);
  },
  set outerHTML(markup) {
    const t = this.dom.createElement("template");
    t.innerHTML = markup;
    this.replaceWith(...t.content.childNodes);
  },
  insertAdjacentHTML(where, markup) {
    const t = this.dom.createElement(this.svg ? "g" : "template", this.svg);
    t.innerHTML = markup;
    const frag = this.dom.createFragment();
    frag.append(...(t.type === "template" ? t.content : t).childNodes);
    this.insertAdjacentElement(where, frag);
  },
  insertAdjacentText(where, text) {
    this.insertAdjacentElement(where, this.dom.createText(text));
  },
  set innerHTML(markup) {
    // A <template>'s markup goes to its `content`, exactly: Solid and Vue clone from it and walk it.
    const template = this.type === "template";
    const into = template ? this.content : this;
    // On a React node this only clears what libraries added (jsVectorMap empties its container
    // before drawing again); React's own children stay.
    for (const k of [...(into.kids ?? [])]) into.removeChild(k);
    if (!this.imperative) return;
    const build = (n, svg) => {
      if (typeof n === "string") return this.dom.createText(n);
      const [tag, attrs, kids] = n;
      if (tag === "#comment") return this.dom.createComment(kids[0]);
      const el = this.dom.createElement(tag, svg || tag.toLowerCase() === "svg");
      for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
      for (const k of kids) el.appendChild(build(k, el.svg));
      return el;
    };
    for (const n of parseMarkup(String(markup ?? ""), template)) into.appendChild(build(n, this.svg));
  },
  get content() {
    return this.type === "template" ? (this._content ??= this.dom.createFragment()) : undefined;
  },
  get className() {
    return this.getAttribute("class") ?? "";
  },
  set className(value) {
    this.setAttribute("class", value);
  },
  getElementsByTagName(tag) {
    return this.querySelectorAll(tag);
  },
  getElementsByClassName(name) {
    const out = [];
    const walk = (n) => {
      for (const k of n.children) {
        if (String(k.props.className ?? "").split(/\s+/).includes(name)) out.push(k);
        walk(k);
      }
    };
    walk(this);
    return out;
  },
  // Imperative DOM edits move nodes libraries built themselves (`document.createElement*`:
  // ApexCharts' SVG, flatpickr's calendar). React's own nodes stay where React puts them, except
  // into or out of such DOM (flatpickr wraps its <input>), and helper objects that aren't host nodes (Radix focus guards) are accepted and ignored.
  insertAdjacentElement(where, el) {
    if (where === "afterbegin") return this.insertBefore(el, this.firstChild);
    if (where === "beforebegin") return this.parent?.insertBefore(el, this);
    if (where === "afterend") return this.parent?.insertBefore(el, this.nextSibling);
    return this.appendChild(el);
  },
  appendChild(el) {
    return this.insertBefore(el, null);
  },
  insertBefore(el, before) {
    if (el?.nodeType === 11) for (const k of [...el.childNodes]) this.insertBefore(k, before);
    else if (el && el !== before && (el.imperative || (el.fiber && (this.imperative || el.parent?.imperative)))) this.dom.insert(this, el, before);
    return el;
  },
  removeChild(el) {
    if (el?.imperative && el.parent === this) this.dom.remove(this, el);
    return el;
  },
  replaceChild(next, old) {
    this.insertBefore(next, old);
    this.removeChild(old);
    return old;
  },
  // MUI X charts measures axis labels by swapping <text>s into a hidden <svg>.
  replaceChildren(...nodes) {
    for (const k of [...(this.kids ?? [])]) this.removeChild(k);
    for (const n of nodes) this.appendChild(n);
  },
  remove() {
    this.parent?.removeChild(this);
  },
  append(...nodes) {
    for (const n of nodes) this.appendChild(asNode(this, n));
  },
  prepend(...nodes) {
    const first = this.firstChild;
    for (const n of nodes) this.insertBefore(asNode(this, n), first);
  },
  ...CHILD_NODE,
  cloneNode(deep = false) {
    if (!this.imperative) return this;
    const copy = this.dom.createElement(this.type, this.svg);
    for (const [k, v] of Object.entries(this.props)) if (typeof v === "string") copy.setAttribute(k === "className" ? "class" : k, v);
    copy.style.cssText = this.style.cssText;
    if (deep) for (const k of this.kids ?? []) copy.appendChild(k.cloneNode(true));
    return copy;
  },
  matches(selector) {
    return parseSelector(selector).some((complex) => matchComplex(this, complex));
  },
  closest(selector) {
    for (let n = this; n?.type; n = n.parent) if (n.matches(selector)) return n;
    return null;
  },
  querySelector(selector) {
    return this.querySelectorAll(selector)[0] ?? null;
  },
  querySelectorAll(selector) {
    const list = parseSelector(selector);
    const out = [];
    const walk = (n) => {
      for (const k of n.kids ?? []) {
        if (!k.type) continue;
        if (list.some((complex) => matchComplex(k, complex))) out.push(k);
        walk(k);
      }
    };
    walk(this);
    return out;
  },
  getAttribute(name) {
    const prop = { class: "className", for: "htmlFor", colspan: "colSpan", tabindex: "tabIndex" }[name] ?? name;
    const v = this.props?.[prop];
    // react-dom drops `false` attributes, except data-*/aria-* which it writes as "false".
    const absent = v === false && !/^(data|aria)-/.test(prop);
    return v == null || absent || typeof v === "object" || typeof v === "function" ? null : String(v);
  },
  hasAttribute(name) {
    return this.getAttribute(name) != null;
  },
  toggleAttribute(name, force) {
    const on = force ?? !this.hasAttribute(name);
    if (on !== this.hasAttribute(name)) on ? this.setAttribute(name, "") : this.removeAttribute(name);
    return on;
  },
  // Live, as in the DOM: `el.dataset.placement = "left"` sets `data-placement` (ApexCharts' tooltip
  // arrow is styled by it).
  get dataset() {
    const out = {};
    for (const [k, v] of Object.entries(this.props ?? {})) {
      if (k.startsWith("data-") && v != null) out[k.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())] = String(v);
    }
    const attr = (key) => "data-" + String(key).replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
    return new Proxy(out, {
      set: (_, key, value) => (this.setAttribute(attr(key), value), true),
      deleteProperty: (_, key) => (this.removeAttribute(attr(key)), true),
    });
  },
  // Attributes become props (SVG ones verbatim). On a React node one stays until React renders a
  // new value for it, as in the DOM: MUI X Charts animates its bars by writing `height`.
  setAttribute(name, value) {
    if (name === "style") this.style.cssText = value;
    else {
      const prop = { class: "className", for: "htmlFor" }[name] ?? name;
      const old = this.props[prop];
      this.props = { ...this.props, [prop]: String(value) };
      this.dom.attributeChanged(this, name, old);
      if (!this.imperative) (this.attrWrites ??= new Set()).add(prop);
      this.dom.touch(this);
    }
  },
  setAttributeNS(_ns, name, value) {
    this.setAttribute(name, value);
  },
  removeAttribute(name) {
    if (name === "style") this.style.cssText = "";
    const { [{ class: "className", for: "htmlFor" }[name] ?? name]: old, ...rest } = this.props;
    this.props = rest;
    if (old !== undefined) this.dom.attributeChanged(this, name, old);
    this.dom.touch(this);
  },
  get namespaceURI() {
    return this.svg || SVG_TAGS.has(this.type) ? "http://www.w3.org/2000/svg" : "http://www.w3.org/1999/xhtml";
  },
  // SVG geometry from attributes (ApexCharts sizes labels with a hidden <text>'s box).
  getBBox() {
    return domRect(bbox(this));
  },
  getComputedTextLength() {
    return bbox(this).width;
  },
  // Sizes are known for nodes a ResizeObserver watches or JS has measured before (the host reports
  // them as they change). The first read of any other node lays the window out on the spot, as a
  // browser does: Radix Select places its list from nodes it has just mounted.
  // Pending style writes are laid out first too, as a browser's forced reflow does: MUI's Collapse
  // sets a px height and reads a size before setting 0, which then transitions from the px.
  laidOut() {
    if (!this.rect || this.dom?.stylesPending()) this.measureNow?.();
    return this.rect ?? {};
  },
  getBoundingClientRect() {
    // Inside an <svg> (drawn as one image) a box comes from its attributes, placed in the svg's.
    const svg = this.type !== "svg" && this.closestSvg();
    if (svg) {
      const at = svg.getBoundingClientRect();
      const [k, ox, oy] = viewBoxScale(svg, at);
      const b = rootBox(this) ?? {};
      return domRect({ x: at.x + (b.x - ox) * k, y: at.y + (b.y - oy) * k, width: b.width * k, height: b.height * k });
    }
    return domRect(this.rect && !this.dom?.stylesPending() ? this.rect : this.laidOut());
  },
  closestSvg() {
    for (let n = this.parent; n?.type; n = n.parent) if (n.type === "svg") return n;
    return null;
  },
  getClientRects() {
    return this.rect ? [domRect(this.rect)] : [];
  },
  get offsetWidth() {
    return this.laidOut().width ?? 0;
  },
  get offsetHeight() {
    return this.laidOut().height ?? 0;
  },
  // shortcut: the parent stands in for the offset parent (the nearest positioned ancestor).
  // None for a fixed box, as in the DOM: Popper.js places a fixed tooltip against the window then.
  get offsetParent() {
    if ((this.styleWrites?.position || this.props.style?.position) === "fixed") return null;
    return this.parent ?? null;
  },
  // Its left and top border widths.
  get clientLeft() {
    return this.laidOut().border?.[3] ?? 0;
  },
  get clientTop() {
    return this.laidOut().border?.[0] ?? 0;
  },
  get offsetTop() {
    return (this.laidOut().y ?? 0) - (this.parent?.rect?.y ?? 0);
  },
  get offsetLeft() {
    return (this.laidOut().x ?? 0) - (this.parent?.rect?.x ?? 0);
  },
  // The padding box: the border box less its borders.
  get clientWidth() {
    const { width = 0, border: [, r = 0, , l = 0] = [] } = this.laidOut();
    return width - l - r;
  },
  get clientHeight() {
    const { height = 0, border: [t = 0, , b = 0] = [] } = this.laidOut();
    return height - t - b;
  },
  get scrollWidth() {
    const r = this.laidOut();
    const rights = this.children.map((k) => {
      const c = k.laidOut();
      return (c.x ?? 0) + (c.width ?? 0) - (r.x ?? 0) + this.scrollLeft;
    });
    return Math.max(r.width ?? 0, ...rights);
  },
  // The content's extent, not the box's: a collapsed (height 0, overflow hidden) panel still reports what it holds.
  // shortcut: ignores the box's bottom padding and its children's bottom margins; add them when a layout needs them.
  get scrollHeight() {
    const r = this.laidOut();
    const bottoms = this.children.map((k) => {
      const c = k.laidOut();
      return (c.y ?? 0) + (c.height ?? 0) - (r.y ?? 0) + this.scrollTop;
    });
    return Math.max(r.height ?? 0, ...bottoms);
  },
  // Scroll offsets, as the host last reported them or JS set them (`scrolled`: [left, top]).
  get scrollTop() {
    return this.scrolled?.[1] ?? 0;
  },
  set scrollTop(top) {
    this.scrollTo(this.scrollLeft, top);
  },
  get scrollLeft() {
    return this.scrolled?.[0] ?? 0;
  },
  set scrollLeft(left) {
    this.scrollTo(left, this.scrollTop);
  },
  scrollTo(x, y) {
    const to = typeof x === "object" && x ? x : { left: x, top: y };
    const r = this.laidOut();
    const max = [this.scrollWidth - (r.width ?? 0), this.scrollHeight - (r.height ?? 0)];
    const clamp = (v, i) => Math.round(Math.max(0, Math.min(Number.isFinite(+v) ? +v : (this.scrolled?.[i] ?? 0), max[i])));
    const next = [clamp(to.left, 0), clamp(to.top, 1)];
    // In place: `document.scrollingElement` inherits the root's.
    if (this.scrolled) this.scrolled.splice(0, 2, ...next);
    else this.scrolled = next;
    this.dom.scroll(this, next);
  },
  scroll(x, y) {
    this.scrollTo(x, y);
  },
  scrollBy(x, y) {
    const by = typeof x === "object" && x ? x : { left: x, top: y };
    this.scrollTo(this.scrollLeft + (+by.left || 0), this.scrollTop + (+by.top || 0));
  },
  // Scrolls the nearest scroll container (one the host reported) so the box shows.
  // shortcut: that container only, not every one above it; `smooth` jumps.
  scrollIntoView(arg = true) {
    const { block = arg === false ? "end" : "start", inline = "nearest" } = typeof arg === "object" && arg ? arg : {};
    let box = this.parent;
    while (box && !box.scrolled) box = box.parent;
    if (!box) return;
    const r = this.getBoundingClientRect();
    const b = box.getBoundingClientRect();
    const place = (pos, start, size, viewStart, viewSize, where) => {
      const end = start + size;
      const viewEnd = viewStart + viewSize;
      if (where === "start") return pos + start - viewStart;
      if (where === "end") return pos + end - viewEnd;
      if (where === "center") return pos + start + size / 2 - (viewStart + viewSize / 2);
      return start < viewStart ? pos + start - viewStart : end > viewEnd ? pos + Math.min(end - viewEnd, start - viewStart) : pos;
    };
    box.scrollTo(place(box.scrollLeft, r.left, r.width, b.left, b.width, inline), place(box.scrollTop, r.top, r.height, b.top, b.height, block));
  },
};

// `el.onclick = f`: one listener per type, replaced by the next assignment; `false` returned cancels.
for (const type of "click dblclick auxclick contextmenu mousedown mouseup mousemove mouseover mouseout mouseenter mouseleave pointerdown pointerup pointermove pointerover pointerout pointerenter pointerleave pointercancel keydown keyup keypress input change submit reset focus blur focusin focusout wheel scroll load error toggle durationchange loadedmetadata loadeddata canplay canplaythrough play playing pause timeupdate seeked ended volumechange ratechange".split(" ")) {
  Object.defineProperty(HostNode, "on" + type, {
    get() {
      return this.handlers?.[type]?.f ?? null;
    },
    set(f) {
      const handlers = (this.handlers ??= {});
      if (handlers[type]) this.removeEventListener(type, handlers[type].listener);
      handlers[type] = typeof f === "function" ? { f, listener: (e) => f.call(this, e) === false && e.preventDefault() } : null;
      if (handlers[type]) this.addEventListener(type, handlers[type].listener);
    },
  });
}

// A small selector engine for what libraries query host nodes with (cmdk, Radix, ApexCharts): tag,
// #id, .class, [attr], [attr="v"], :not(...), :first/last/only-child, escaped names (`data\\:x`),
// the descendant, child (>) and sibling (+ ~) combinators, and comma lists.
// shortcut: no other pseudo-classes; add them when a library needs one.
const selectorCache = new Map();
function parseSelector(selector) {
  const text = String(selector); // as the DOM does: closest(undefined) looks for <undefined>
  let list = selectorCache.get(text);
  if (!list) {
    list = splitTop(text, ",").map(parseComplex);
    selectorCache.set(text, list);
  }
  return list;
}
// Walks `text` outside brackets, parentheses, quotes and escapes, calling `at(i, c)` per character.
function scanTop(text, at) {
  let depth = 0, quote = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "\\") i++;
    else if (quote) quote = c === quote ? null : quote;
    else if (c === '"' || c === "'") quote = c;
    else if (c === "[" || c === "(") depth++;
    else if (c === "]" || c === ")") depth--;
    else if (depth === 0) at(i, c);
  }
}
// Splits on `sep` at the top level.
function splitTop(text, sep) {
  const parts = [];
  let start = 0;
  scanTop(text, (i, c) => {
    if (c !== sep) return;
    parts.push(text.slice(start, i));
    start = i + 1;
  });
  parts.push(text.slice(start));
  return parts;
}
// `a > b c` -> compounds left to right, each with the combinator joining it to the one before.
function parseComplex(text) {
  const steps = [];
  let start = 0;
  let comb = null;
  const cut = (end, next) => {
    const part = text.slice(start, end).trim();
    if (part) {
      steps.push({ test: parseCompound(part), comb: comb ?? " " });
      comb = null;
    }
    if (next !== " " || !part) comb = next === " " ? comb : next;
    else comb = " ";
  };
  scanTop(text, (i, c) => {
    if (c === " " || c === ">" || c === "+" || c === "~") {
      cut(i, c);
      start = i + 1;
    }
  });
  cut(text.length, null);
  if (steps.length) steps[0].comb = null;
  return steps;
}
const unescape = (s) => s?.replace(/\\(.)/g, "$1");
const IDENT = String.raw`(?:[\w-]|\\.)+`;
const COMPOUND = new RegExp(
  String.raw`^(\*|[a-zA-Z][\w-]*)|#(${IDENT})|\.(${IDENT})|\[\s*(${IDENT})\s*(?:=\s*(?:"([^"]*)"|'([^']*)'|([^\]\s]*)))?\s*\]|:not\(((?:[^()]|\([^()]*\))*)\)|:(first|last|only)-child\b`,
  "y",
);
function parseCompound(text) {
  const tests = [];
  const re = new RegExp(COMPOUND);
  for (let m; re.lastIndex < text.length && (m = re.exec(text)); ) {
    const [, tag, id, cls, attr, v1, v2, v3, not, child] = m.map((v, i) => (i >= 2 && i <= 4 ? unescape(v) : v));
    if (tag) tag !== "*" && tests.push((n) => n.type.toLowerCase() === tag.toLowerCase());
    else if (id) tests.push((n) => n.getAttribute("id") === id);
    else if (cls) tests.push((n) => String(n.props.className ?? "").split(/\s+/).includes(cls));
    else if (attr) {
      const want = v1 ?? v2 ?? v3;
      tests.push((n) => {
        const v = n.getAttribute(attr);
        return want === undefined ? v != null : v === want;
      });
    } else if (not !== undefined) {
      const inner = parseSelector(not);
      tests.push((n) => !inner.some((c) => matchComplex(n, c)));
    } else if (child) {
      // Among the parent's elements, as in the DOM (text nodes don't count).
      tests.push((n) => {
        const kin = n.parent?.children ?? [n];
        return (child !== "last" ? kin[0] === n : true) && (child !== "first" ? kin.at(-1) === n : true);
      });
    }
  }
  if (re.lastIndex !== text.length) tests.push(() => false); // unsupported syntax matches nothing
  return (n) => tests.every((t) => t(n));
}
// The last compound matches `node`, the others the nodes their combinators lead to. As in the
// DOM, `el.querySelectorAll` checks ancestors above `el` too.
function matchComplex(node, steps, i = steps.length - 1) {
  if (!steps[i]?.test(node)) return false;
  if (i === 0) return true;
  const inScope = (n) => !!n?.type;
  const before = (n) => {
    const kin = n.parent?.children ?? [];
    return kin.slice(0, kin.indexOf(n)).reverse();
  };
  switch (steps[i].comb) {
    case ">":
      return inScope(node.parent) && matchComplex(node.parent, steps, i - 1);
    case "+":
      return !!before(node)[0] && matchComplex(before(node)[0], steps, i - 1);
    case "~":
      return before(node).some((s) => matchComplex(s, steps, i - 1));
    default:
      for (let p = node.parent; inScope(p); p = p.parent) if (matchComplex(p, steps, i - 1)) return true;
      return false;
  }
}

const PRESSES = new Set(["mousedown", "mouseup", "click"]);
// Host events that aren't input; the rest get a `settled` once handled (`NOT_INPUT` in src/render.rs).
const NOT_INPUT = new Set(["frame", "resize", "scroll", "benchQuery", "benchSettle", "visibilitychange", "appearance", "menu", "menu-closed", "dialog", "window", "app", "media"]);
// Host-drawn text fields, which report their own keys.
const FIELDS = new Set(["input", "textarea", "select"]);
const INTERACTIVE = new Set(["a", "button", "input", "select", "textarea"]);
const LABELABLE = (n) => n.type === "input" || n.type === "select" || n.type === "textarea";
const checkable = (n) => n.type === "input" && (n.props.type === "checkbox" || n.props.type === "radio");
const POINTER = new Set(["mousemove", "mousedown", "mouseup", "click", "mouseenter", "mouseleave"]);
const HOVER = new Set(["mouseover", "mouseout", "pointerover", "pointerout"]);
const subtree = (n) => [n, ...(n.kids ?? []).filter((k) => k.type).flatMap(subtree)];
function outermostSvg(n) {
  let svg = null;
  for (let e = n; e?.type; e = e.parent) if (e.type === "svg") svg = e;
  return svg;
}

const REFLECTED = ["type", "value", "placeholder", "title", "name", "href", "src", "checked", "disabled"];
const BOOLEAN = new Set(["checked", "disabled"]);
// A boolean prop or attribute is on: React's `true`, or a script's `disabled=""`.
const on = (v) => v === "" || Boolean(v);

// A text node a library made (`document.createTextNode`, `el.textContent = ...`).
const TextNode = {
  nodeType: 3,
  nodeName: "#text",
  imperative: true,
  get data() {
    return this.text;
  },
  set data(text) {
    this.dom.setText(this, String(text));
  },
  get textContent() {
    return this.text;
  },
  set textContent(text) {
    this.data = text;
  },
  get nodeValue() {
    return this.text;
  },
  set nodeValue(text) {
    this.data = text;
  },
  get parentNode() {
    return this.parent;
  },
  get nextSibling() {
    const kin = this.parent?.kids ?? [];
    return kin[kin.indexOf(this) + 1] ?? null;
  },
  get previousSibling() {
    const kin = this.parent?.kids ?? [];
    return kin[kin.indexOf(this) - 1] ?? null;
  },
  get parentElement() {
    return this.parent;
  },
  get ownerDocument() {
    return globalThis.document;
  },
  get length() {
    return this.data.length;
  },
  hasChildNodes: () => false,
  childNodes: [],
  remove() {
    this.parent?.removeChild(this);
  },
  get isConnected() {
    return this.parent?.isConnected ?? false;
  },
  getRootNode() {
    return this.parent?.getRootNode() ?? this;
  },
  ...CHILD_NODE,
  cloneNode() {
    return this.dom.createText(this.text);
  },
};

// A comment (Vue's and Solid's anchors): kept among its parent's children in JS, never sent to the
// host, so an anchor can't become an empty box that a flex `gap` spaces.
const CommentNode = Object.create(
  TextNode,
  Object.getOwnPropertyDescriptors({
    nodeType: 8,
    nodeName: "#comment",
    text: "",
    get data() {
      return this.comment;
    },
    set data(text) {
      this.comment = String(text);
    },
    get textContent() {
      return this.comment;
    },
    set textContent(text) {
      this.data = text;
    },
    get nodeValue() {
      return this.comment;
    },
    set nodeValue(text) {
      this.data = text;
    },
    cloneNode() {
      return this.dom.createComment(this.comment);
    },
  }),
);

// CSS px per user unit of an <svg> drawn in `rect`, and its viewBox origin (none: 1, 0, 0).
// shortcut: uniform scale by width; preserveAspectRatio isn't applied.
function viewBoxScale(svg, rect) {
  const v = String(svg.props?.viewBox ?? "").split(/[\s,]+/).map(Number);
  return v.length === 4 && v[2] > 0 && rect.width ? [rect.width / v[2], v[0], v[1]] : [1, 0, 0];
}

const INPUTS = new Set(["input", "textarea", "select"]);
// Elements whose DOM interface has a `type` property a script might set.
const TYPED = new Set(["input", "button", "select", "textarea", "a", "ol", "li", "link", "style", "script", "source", "embed", "object", "menu"]);

/** Whether two prop objects wire the same: handlers only by presence, `style` by its entries. */
export function sameProps(a, b) {
  let keys = 0;
  for (const k in a) {
    keys++;
    const x = a[k];
    const y = b[k];
    if (x === y || k === "children" || (typeof x === "function" && typeof y === "function")) continue;
    if (k === "style" && x && y && typeof x === "object" && typeof y === "object" && sameEntries(x, y)) continue;
    return false;
  }
  for (const _ in b) keys--;
  return keys === 0;
}

/** Whether two `wire` results are equal: flat fields, `events` (a list) and `attrs` (strings). */
function sameWire(a, b) {
  if (!b) return false;
  let keys = 0;
  for (const k in a) {
    keys++;
    const x = a[k];
    const y = b[k];
    if (x === y) continue;
    if (k === "events" && y && x.length === y.length && x.every((e, i) => e === y[i])) continue;
    if (k === "attrs" && y && sameEntries(x, y)) continue;
    return false;
  }
  for (const _ in b) keys--;
  return keys === 0;
}

function sameEntries(a, b) {
  let keys = 0;
  for (const k in a) {
    keys++;
    if (a[k] !== b[k]) return false;
  }
  for (const _ in b) keys--;
  return keys === 0;
}

function domRect({ x = 0, y = 0, width = 0, height = 0 } = {}) {
  return { x, y, left: x, top: y, right: x + width, bottom: y + height, width, height, toJSON: () => ({ x, y, width, height }) };
}

/** `sheets` (from `createHead`) lets React-rendered `<style>` elements style the window. */
// A data: URL's bytes, base64 or percent-encoded.
function dataBytes(url) {
  const m = url.match(/^data:([^,]*?)(;base64)?,(.*)$/s);
  if (!m) return null;
  return m[2] ? Buffer.from(m[3], "base64") : Buffer.from(decodeURIComponent(m[3]));
}
// An <img>'s file, read now: null while a remote one isn't downloaded, or if it isn't there.
function imageBytes(n) {
  const src = String(n.props.src ?? "");
  if (src.startsWith("data:")) return dataBytes(src);
  const file = assetPath(src);
  if (isRemote(file)) return null;
  try {
    return readFileSync(file);
  } catch {
    return null;
  }
}
// The size in the header, else (SVG, AVIF...) by decoding; null if it can't be decoded.
function sizeOf(bytes) {
  const size = imageSize(bytes);
  if (size) return size;
  try {
    const { width, height } = decodeImage(bytes);
    return width ? { width, height } : null;
  } catch {
    return null;
  }
}

export function createRenderer(send, sheets) {
  let nextId = 2;
  let ops = [];
  let priority = NoEventPriority;
  // The element with focus, as `document.activeElement` reports it (null means <body>).
  let focused = null;
  // Pressed mouse buttons, as DOM pointer events' `buttons` (drag code ends a drag on a move with none).
  // shortcut: primary button only, tracked from presses JS saw; ask the host if right/middle drags matter.
  let buttons = 0;
  // The node holding pointer capture (`setPointerCapture`), until the release.
  let captured = null;
  const instances = new Map();
  // Nodes whose `style` JS wrote since the last flush.
  const restyled = new Set();
  // Canvases drawn on in this task.
  const painted = new Set();
  // Library-built nodes (see HostNode's DOM methods): created and edited outside React commits,
  // so their ops go out at the end of the task, or before a layout sync.
  let flushQueued = false;
  const later = () => {
    if (flushQueued) return;
    flushQueued = true;
    queueMicrotask(() => {
      flushQueued = false;
      flushStyleWrites();
    });
  };
  const dom = {
    stylesPending: () => restyled.size > 0,
    // The host gets a canvas's pixels once, after the task that drew on it.
    paint(n) {
      if (!painted.size) queueMicrotask(sendPixels);
      painted.add(n);
    },
    // `play()`, `pause()`, `currentTime = s`... after the commits that made the element.
    media(n, op, value = 0) {
      flush();
      send({ t: "media", id: n.hid, op, value });
    },
    // A details opened or closed: the others in its `name` group close, and `toggle` fires once
    // in a later task, with the state it left and the one it ends in.
    toggled(n) {
      if (n.wasOpen == null) {
        n.wasOpen = !n.open;
        setTimeout(() => {
          const [oldState, newState] = [n.wasOpen, n.open].map((o) => (o ? "open" : "closed"));
          n.wasOpen = null;
          if (oldState === newState) return;
          fire(n, "toggle");
          propagate(Object.assign(new Event("toggle"), { oldState, newState }), n);
        });
      }
      const name = n.getAttribute("name");
      if (n.open && name) for (const o of instances.values()) if (o !== n && o.type === "details" && o.open && o.getAttribute("name") === name) o.open = false;
    },
    // A dialog took focus to its `autofocus` or first focusable element, else itself; closing gives
    // it back.
    dialogOpened(n) {
      n.restoreFocus = focused;
      const inside = subtree(n).slice(1);
      const focusable = (k) => (INTERACTIVE.has(k.type) && !on(k.props.disabled)) || k.tabIndex >= 0;
      (inside.find((k) => k.props.autoFocus || k.hasAttribute("autofocus")) ?? inside.find(focusable) ?? n).focus();
    },
    dialogClosed(n) {
      const back = n.restoreFocus;
      n.restoreFocus = null;
      if (back && instances.has(back.hid)) back.focus();
      setTimeout(() => dom.dialogEvent(n, "close"));
    },
    // `cancel` or `close` to React and DOM listeners; false when one cancelled it.
    dialogEvent(n, type) {
      const event = new Event(type, { cancelable: type === "cancel" });
      const react = fire(n, type);
      propagate(event, n);
      return !react.defaultPrevented && !event.defaultPrevented;
    },
    // An <img>'s pixels now, from a local file or a data: URL; a remote one needs `loadImage`.
    // An <img>'s pixels, for a texture or a canvas, when `load` didn't decode them.
    // shortcut: Skia fills the pixels in a later task, so an image in the page drawn right away
    // draws empty; decode on load for attached images too if an app textures one from the page.
    decodeImage(n) {
      const bytes = imageBytes(n);
      if (!bytes) return;
      try {
        n._image = decodeImage(bytes);
      } catch {
        n._image = null;
      }
    },
    // Its size, now, from the file's header.
    probeImage(n) {
      const bytes = imageBytes(n);
      if (bytes) n._size = sizeOf(bytes);
    },
    // An <img> loading as in a browser: `load` or `error` once its size is known, and a promise for
    // `decode()`. A detached one (`new Image()`, Three's and Pixi's loaders) is made to be drawn,
    // and Skia only decodes asynchronously: its pixels are decoded before `load` too. One in the
    // page is drawn by the host, so only its header is read.
    loadImage(n) {
      const src = String(n.props.src ?? "");
      if (n.loading?.src === src) return n.loading.done;
      const file = isRemote(src) ? fetchAsset(src) : Promise.resolve(assetPath(src));
      const bytes = src.startsWith("data:") ? Promise.resolve(dataBytes(src)) : file.then((f) => (f ? readFile(f) : Promise.reject()));
      const done = bytes
        .then(async (b) => {
          const size = b ? sizeOf(b) : null;
          if (size && !n.parent) n._image = await skia().loadImage(b);
          return size;
        })
        .then(
          (size) => ((n._size = size), size ? "load" : "error"),
          () => ((n._size = null), "error"),
        )
        .then((type) => {
          if (String(n.props.src ?? "") !== src) return;
          fire(n, type);
          propagate(new Event(type), n);
          if (type === "error") throw new DOMException(`The image at ${src} can't be decoded.`, "EncodingError");
        });
      n.loading = { src, done };
      return done;
    },
    // A media element's state changed (the host's report, or its own call): its events, which don't bubble.
    mediaEvent(n, msg) {
      for (const type of mediaUpdate(n.media, msg)) {
        fire(n, type);
        propagate(new Event(type), n);
      }
    },
    insert(parent, child, before) {
      before?.parent === parent ? insert(parent, child, before) : append(parent, child);
      // A drawing JS hit-tests needs its place on screen; and its listeners may come along.
      if (child.type === "svg" && !outermostSvg(parent)) {
        child.measured = true;
        for (let n = parent; n?.type; n = n.parent) {
          n.drawing = true;
          restyled.add(n);
        }
      }
      const svg = outermostSvg(child);
      if (svg) restyled.add(svg);
      later();
    },
    remove(parent, child) {
      // Detached, not freed: libraries attach removed nodes again (ApexCharts' measuring text).
      // shortcut: never freed in the host either; free them once something reports them garbage.
      detach(child);
      child.parent = null;
      if (owned(parent) && live(parent) && live(child)) ops.push({ detach: [parent.hid, child.hid] });
      later();
    },
    touch(n) {
      restyled.add(n);
      later();
    },
    scroll(n, [x, y]) {
      if (!instances.has(n.hid)) return;
      ops.push({ scroll: [n.hid, x, y] });
      later();
    },
    setText(n, text) {
      const old = n.text;
      n.text = text;
      editing.textChanged(n, old);
      if (instances.has(n.hid)) ops.push({ settext: [n.hid, text] });
      later();
    },
    createElement(tag, svg = false) {
      const type = svg ? tag : tag.toLowerCase();
      const inst = node({ hid: nextId++, type, props: {}, fiber: null, parent: null, seq: 0, imperative: true, svg });
      // DOM properties reflect attributes (`input.type = "number"`); `type` stays the tag.
      for (const prop of REFLECTED) {
        Object.defineProperty(inst, prop, {
          // Boolean ones are the attribute's presence: `checked = true` writes `checked=""`.
          get: () => (prop === "type" ? type : BOOLEAN.has(prop) ? inst.hasAttribute(prop) : inst.props[prop]),
          set: (v) => (typeof v === "boolean" ? (v ? inst.setAttribute(prop, "") : inst.removeAttribute(prop)) : inst.setAttribute(prop, v)),
        });
      }
      instances.set(inst.hid, inst);
      inst.wired = wire(inst);
      ops.push({ el: [inst.hid, type, inst.wired] });
      later();
      return inst;
    },
    createText(text) {
      const inst = Object.assign(Object.create(TextNode), { dom, hid: nextId++, text: String(text), parent: null });
      instances.set(inst.hid, inst);
      ops.push({ txt: [inst.hid, inst.text] });
      later();
      return inst;
    },
    createComment(data) {
      return Object.assign(Object.create(CommentNode), { dom, comment: String(data ?? ""), parent: null });
    },
    createFragment() {
      // Holds children until they move into a real parent; it never reaches the host.
      return Object.assign(Object.create(HostNode), { dom, nodeType: 11, type: "#document-fragment", props: {}, kids: [], parent: null, imperative: true });
    },
    attributeChanged: (n, name, old) => editing.attributeChanged(n, name, old),
  };
  const editing = createEditing({
    insert: (parent, child, before) => dom.insert(parent, child, before),
    remove: (parent, child) => dom.remove(parent, child),
    setText: (n, text) => dom.setText(n, text),
    createText: (text) => dom.createText(text),
    createElement: (tag, svg) => dom.createElement(tag, svg),
    createFragment: () => dom.createFragment(),
    root: () => html,
    instance: (id) => instances.get(id),
    focused: () => focused,
    setFocus: (n) => setFocus(n),
    propagate: (event, target) => propagate(event, target),
    fire: (target, type) => fire(target, type),
    send: (msg) => send(msg),
    later,
  });

  // What all of this renderer's nodes share, so a node holds only its own fields. `style` and
  // `classList` are made on first use: few nodes touch them, and a big mount makes tens of thousands.
  const Shared = Object.create(
    HostNode,
    Object.getOwnPropertyDescriptors({
      dom,
      dispatchHost: dispatch,
      relisten() {
        if (!instances.has(this.hid)) return;
        emitProps(this);
        const svg = outermostSvg(this);
        if (svg && svg !== this) emitProps(svg);
        flush();
      },
      // Ancestors too: layout code measures a node's containers alongside it (a drag handle and its group).
      measure() {
        for (let n = this; n && !n.rect && !n.measured; n = n.parent) {
          n.measured = true;
          if (instances.has(n.hid)) emitProps(n);
        }
        flush();
      },
      measureNow() {
        this.measure();
        syncLayout();
      },
      setPointerCapture() {
        captured = this;
      },
      releasePointerCapture() {
        if (captured === this) captured = null;
      },
      hasPointerCapture() {
        return captured === this;
      },
      setFocus,
      isFocused() {
        return focused === this;
      },
      get style() {
        return (this._style ??= writableStyle(this));
      },
      set style(v) {
        this._style = v;
      },
      get classList() {
        return (this._classList ??= writableClassList(this));
      },
      set classList(v) {
        this._classList = v;
      },
    }),
  );
  // Solid's delegated handlers (`el.$$click = fn`; Solid itself listens on `document`) make the host
  // report the element's events, as a listener on it does: else a click finds no target but <body>.
  for (const type of Object.keys(WIRE_FOR_DOM)) {
    Object.defineProperty(Shared, "$$" + type, {
      get() {
        return this.delegated?.[type];
      },
      set(fn) {
        (this.delegated ??= {})[type] = fn;
        this.dom.touch(this);
      },
    });
  }
  const kinds = { canvas: CanvasNode, video: MediaNode, audio: MediaNode, details: DetailsNode, dialog: DialogNode, img: ImageNode };
  for (const [type, api] of Object.entries(kinds)) kinds[type] = Object.create(Shared, Object.getOwnPropertyDescriptors(api));
  const node = (fields) => Object.assign(Object.create(kinds[fields.type] ?? Shared), fields);
  // `el.classList.add("vaul-dragging")`: imperative class edits apply over React's `className`
  // until React sets a new one, as in a browser.
  function writableClassList(n) {
    const edits = (n.classEdits = new Map()); // class -> added (true) or removed (false)
    const has = (c) => edits.get(c) ?? String(n.props?.className ?? "").split(/\s+/).includes(c);
    const edit = (c, on) => {
      if (has(c) === on) return;
      // A library's own node has no React className to edit over: the class attribute is it.
      if (n.imperative) {
        const list = String(n.props.className ?? "").split(/\s+/).filter(Boolean);
        return n.setAttribute("class", (on ? [...list, c] : list.filter((x) => x !== c)).join(" "));
      }
      edits.set(c, on);
      if (!restyled.size) queueMicrotask(flushStyleWrites);
      restyled.add(n);
    };
    return {
      add: (...cs) => cs.forEach((c) => edit(c, true)),
      remove: (...cs) => cs.forEach((c) => edit(c, false)),
      toggle: (c, force) => (edit(c, force ?? !has(c)), has(c)),
      replace: (a, b) => has(a) && (edit(a, false), edit(b, true), true),
      contains: has,
    };
  }
  function className(inst) {
    const classes = new Set(String(inst.props.className ?? "").split(/\s+/).filter(Boolean));
    for (const [c, on] of inst.classEdits ?? []) on ? classes.add(c) : classes.delete(c);
    return [...classes].join(" ");
  }
  // `el.style.left = "10px"`: imperative writes style the element over React's `style` prop, as in
  // a browser (Radix Select positions its list this way). They reach the host with the next flush.
  function writableStyle(n) {
    const values = (n.styleWrites = {});
    const camel = (name) => (name.startsWith("--") ? name : name.replace(/-([a-z])/g, (_, c) => c.toUpperCase()));
    // Emptying a property React also sets clears React's value too, as in the DOM, where both are
    // the one inline style (Popper.js empties `left` when it places a tooltip by `right`).
    const write = (key, value, masks = true) => {
      if (value != null && value !== "") values[key] = String(value);
      else if (masks && n.props?.style?.[key] != null) values[key] = "";
      else delete values[key];
      if (!restyled.size) queueMicrotask(flushStyleWrites);
      restyled.add(n);
    };
    const methods = {
      setProperty: (name, value) => write(camel(name), value),
      removeProperty: (name) => write(camel(name), null),
      getPropertyValue: (name) => values[camel(name)] ?? "",
    };
    // `cssText` is all of it: vaul saves it and assigns it back to undo its writes.
    const setCssText = (text) => {
      for (const key of Object.keys(values)) write(key, null, false);
      for (const decl of String(text ?? "").split(";")) {
        const at = decl.indexOf(":");
        if (at > 0) write(camel(decl.slice(0, at).trim()), decl.slice(at + 1).trim());
      }
    };
    // `style["font-size"]` is `style.fontSize`, as in the DOM (MUI X Charts styles the text it
    // measures so).
    const prop = (key) => (typeof key === "string" ? camel(key) : key);
    return new Proxy(values, {
      get: (target, key) => (key === "cssText" ? styleToCss(target) : (methods[key] ?? target[prop(key)] ?? "")),
      set: (_, key, value) => (key === "cssText" ? setCssText(value) : write(prop(key), value), true),
    });
  }
  function flushStyleWrites() {
    for (const n of restyled) if (instances.has(n.hid)) emitProps(n);
    restyled.clear();
    flush();
  }
  // Blocks until the host has laid out everything sent so far, then takes every measured rect: a
  // browser's forced layout. Outside the host (tests) there is nothing to wait for.
  function syncLayout() {
    if (!process.env.GPUIJS_SYNC) return;
    sheets?.flush();
    flushStyleWrites();
    send({ t: "sync" });
    const { rects } = waitForHost();
    for (const [id, rect] of Object.entries(rects)) dispatch({ type: "resize", id: Number(id), rect, sync: true });
  }
  const html = node({ hid: 0, type: "html", props: {}, parent: null, scrolled: [0, 0] });
  const body = node({ hid: 1, type: "body", props: {}, parent: html });
  instances.set(0, html).set(1, body);
  // `<body class>` from the app's index.html, which `gpuijs dev` passes on.
  for (const c of (process.env.GPUIJS_BODY_CLASS ?? "").split(/\s+/).filter(Boolean)) body.classList.add(c);

  function wire(inst) {
    const { props } = inst;
    // Most nodes have no class edits, style or listeners: those steps are skipped for them.
    const out = { class: inst.classEdits?.size ? className(inst) : String(props.className ?? ""), events: [], attrs: {} };
    if (props.id != null) out.id = String(props.id);
    let style = inst.styleWrites ? styleToCss({ ...props.style, ...inst.styleWrites }) : props.style ? styleToCss(props.style) : "";
    if (inst.suspended) style += ";display:none";
    if (style) out.style = style;
    for (const [prop, type] of EVENT_ENTRIES) {
      if (typeof props[prop] === "function" && !out.events.includes(type)) out.events.push(type);
    }
    // An <svg> is one image to the host: it reports what its elements listen for, and JS finds
    // the element under the pointer (see `svgTarget`).
    const listening = inst.type === "svg" ? subtree(inst) : [inst];
    for (const n of listening) {
      if (!n.listeners && !n.delegated) continue;
      const handled = Object.entries(n.listeners ?? {}).filter(([, list]) => list.length);
      for (const [domType, fn] of Object.entries(n.delegated ?? {})) if (fn) handled.push([domType]);
      for (const [domType] of handled) {
        // Crossing into a drawing's elements fires over/out, which the host can't see: it
        // reports the moves over the drawing instead.
        const drawing = inst.type === "svg" || inst.drawing;
        const types = HOVER.has(domType) && drawing ? [WIRE_FOR_DOM[domType], "mousemove", "mouseleave"] : [WIRE_FOR_DOM[domType]];
        for (const type of types) if (type && !out.events.includes(type)) out.events.push(type);
      }
    }
    // Submit buttons need their clicks reported even without a handler: submitting is the default action.
    if (inst.type === "button" && (props.type ?? "submit") === "submit" && !out.events.includes("click")) out.events.push("click");
    const svg = inst.svg || SVG_TAGS.has(inst.type);
    for (const [k, v] of Object.entries(props)) {
      if (v == null || typeof v === "function" || typeof v === "object") continue;
      // A false boolean attribute (`required={false}`) is absent, as react-dom leaves it.
      if (Object.hasOwn(ATTRS, k)) {
        if (v !== false) out.attrs[ATTRS[k]] = String(v);
      }
      // data-*, aria-* and other custom attributes (cmdk's `cmdk-item`), as react-dom passes them through.
      else if (k.includes("-")) out.attrs[k] = String(v);
      else if (svg && !SVG_SKIP.has(k)) out.attrs[svgAttr(k)] = String(v);
    }
    const value = inst.written ?? props.value ?? props.defaultValue;
    if (value != null) out.value = String(value);
    if (props.placeholder != null) out.placeholder = String(props.placeholder);
    if (on(props.disabled)) {
      out.disabled = true;
      out.attrs.disabled = "";
    }
    // Boolean attribute: present only when true (the UA sheet hides `[hidden]`).
    if (props.hidden != null && props.hidden !== false) out.attrs.hidden = "";
    if (props.open === true || props.open === "") out.attrs.open = "";
    // `false` too, which makes a node inside editable content uneditable.
    const editable = props.contentEditable ?? props.contenteditable;
    if (editable != null) out.attrs.contenteditable = String(editable);
    // The host matches `:checked` on it (`.radio:checked ~ .box`).
    if (inst.type === "input" && inst.checked) out.attrs.checked = "";
    // An <img> with a new source loads it for its listeners (`new Image()` preloaders, Three's ImageLoader).
    if (inst.type === "img" && inst.wiredSrc !== props.src) {
      inst.wiredSrc = props.src;
      inst._image = inst._size = undefined;
      const heard = inst.listeners?.load?.length || inst.listeners?.error?.length || props.onLoad || props.onError;
      if (props.src != null && heard) dom.loadImage(inst).catch(noop);
    }
    if (props.src != null) {
      out.src = assetPath(String(props.src));
      // Not downloaded yet: show it once it is. Media streams from the URL instead.
      if (isRemote(out.src) && !STREAMED.has(inst.type)) fetchAsset(out.src).then((file) => file && inst.relisten());
    }
    // Its size, however it was set (`canvas.width = 600` too): the host's box before CSS sizes it.
    if (inst.type === "canvas") Object.assign(out.attrs, { width: String(inst.width), height: String(inst.height) });
    if (inst.type === "input") out.seq = inst.seq;
    if (inst.isModal) out.modal = true;
    // Pointer-tracking code reads the element's rect on every move (Recharts' tooltip).
    if (inst.observers?.size || inst.measured || out.events.includes("mousemove")) out.measure = true;
    // The host defaults what is left out; a 10k-row mount sends megabytes of these.
    if (!out.events.length) delete out.events;
    if (!out.class) delete out.class;
    for (const _ in out.attrs) return out;
    delete out.attrs;
    return out;
  }

  // A React-rendered <style> (shadcn's ChartStyle) styles the window like one in document.head,
  // while it is mounted.
  function syncStyle(inst) {
    if (inst.type !== "style" || !sheets) return;
    const { dangerouslySetInnerHTML: html, children } = inst.props;
    inst.sheet ??= sheets.createStyle();
    inst.sheet.textContent = html?.__html ?? (typeof children === "string" ? children : "");
  }

  function emitProps(inst) {
    // Deleted by React and freed in the host: a library may still write to it (FullCalendar's
    // resize handling after the calendar unmounts).
    if (!instances.has(inst.hid)) return;
    const next = wire(inst);
    if (!sameWire(next, inst.wired)) {
      inst.wired = next;
      ops.push({ set: [inst.hid, next] });
    }
  }

  // A portal may target a container gpuijs doesn't own (Radix Select collects items in a detached
  // DocumentFragment); its children are tracked here but never shown.
  const owned = (parent) => parent.hid != null;
  // Known to the host: created and not freed (see `forget`).
  const live = (n) => instances.has(n.hid);
  // Queued on the parent, applied when its `kids` is next read (see `settleKids`).
  function edit(parent, kind, child, before) {
    parent._kids ??= [];
    (parent.edits ??= []).push([kind, child, before]);
  }
  function detach(child) {
    if (!child.parent) return;
    editing.removing(child.parent, child);
    edit(child.parent, "remove", child);
  }
  function append(parent, child) {
    if (child.sheet) sheets.head.appendChild(child.sheet);
    detach(child);
    child.parent = parent;
    edit(parent, "append", child);
    editing.inserted(parent, child);
    if (owned(parent) && live(parent) && live(child)) ops.push({ append: [parent.hid, child.hid] });
  }
  function insert(parent, child, before) {
    if (child.sheet) sheets.head.appendChild(child.sheet);
    detach(child);
    child.parent = parent;
    edit(parent, "insert", child, before);
    editing.inserted(parent, child);
    if (!owned(parent) || !live(parent) || !live(child)) return;
    if (live(before)) return void ops.push({ insert: [parent.hid, child.hid, before.hid] });
    // Before a comment (the host doesn't have it): before the next node it has, else last.
    const kin = parent.kids;
    const next = kin.slice(kin.indexOf(child) + 1).find(live);
    ops.push(next ? { insert: [parent.hid, child.hid, next.hid] } : { append: [parent.hid, child.hid] });
  }
  function remove(parent, child) {
    detach(child);
    child.parent = null;
    if (!owned(parent) || !live(parent) || !live(child)) return;
    ops.push({ remove: [parent.hid, child.hid] });
    forget(child);
  }
  // The host frees a removed node's whole subtree at once, library-built nodes in it included, so
  // they're forgotten here too: a later write to one (a library tidying up its own nodes as it
  // unmounts) then sends nothing naming a node the host no longer has.
  function forget(n) {
    instances.delete(n.hid);
    for (const k of n.kids ?? []) forget(k);
  }
  function sendPixels() {
    flush();
    for (const n of painted) {
      if (!instances.has(n.hid)) continue;
      const { width, height, data } = n.pixels() ?? {};
      if (width && height) send({ t: "pixels", id: n.hid, width, height, data });
    }
    painted.clear();
  }
  function flush() {
    if (ops.length) {
      send({ t: "commit", ops });
      ops = [];
    }
    editing.sync();
  }

  const reconciler = Reconciler({
    rendererPackageName: "gpuijs",
    rendererVersion: "0.1.0",
    supportsMutation: true,
    supportsPersistence: false,
    supportsHydration: false,
    isPrimaryRenderer: true,
    supportsMicrotasks: true,
    scheduleMicrotask: queueMicrotask,
    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,
    noTimeout: -1,
    getRootHostContext: () => ({}),
    getChildHostContext: (ctx) => ctx,
    getPublicInstance: (inst) => inst,
    prepareForCommit: () => null,
    resetAfterCommit: flush,
    preparePortalMount() {},

    createInstance(type, props, _root, _context, fiber) {
      const inst = node({ hid: nextId++, type, props, fiber, parent: null, seq: 0, _value: props.value ?? props.defaultValue });
      // `type` is the tag; `input.type = "text"` (flatpickr) writes an attribute React owns.
      // Only elements with a DOM `type` get the read-only accessor; it slows every other node.
      if (TYPED.has(type)) Object.defineProperty(inst, "type", { get: () => type, set() {}, enumerable: true });
      instances.set(inst.hid, inst);
      inst.wired = wire(inst);
      ops.push({ el: [inst.hid, type, inst.wired] });
      syncStyle(inst);
      return inst;
    },
    createTextInstance(text) {
      const inst = { hid: nextId++, text, parent: null };
      instances.set(inst.hid, inst);
      ops.push({ txt: [inst.hid, text] });
      return inst;
    },
    appendInitialChild: append,
    appendChild: append,
    appendChildToContainer: append,
    insertBefore: insert,
    insertInContainerBefore: insert,
    removeChild: remove,
    removeChildFromContainer: remove,
    finalizeInitialChildren: () => false,
    shouldSetTextContent: () => false,
    commitUpdate(inst, type, oldProps, newProps) {
      if (oldProps.className !== newProps.className) inst.classEdits?.clear();
      // A new value from React replaces one a script wrote.
      if (oldProps.value !== newProps.value) inst.written = undefined;
      // So does one for an attribute a script wrote; the others keep the script's value.
      if (inst.attrWrites) {
        const kept = {};
        for (const k of inst.attrWrites) {
          if (oldProps[k] === newProps[k]) kept[k] = inst.props[k];
          else inst.attrWrites.delete(k);
        }
        newProps = { ...newProps, ...kept };
      }
      inst.props = newProps;
      // React calls this for every element of a re-rendered component, mostly with equal props.
      // Inputs always re-emit: their props carry the change `seq` JS has seen.
      if (!INPUTS.has(type) && sameProps(oldProps, newProps)) return syncStyle(inst);
      emitProps(inst);
      syncStyle(inst);
    },
    commitTextUpdate(inst, old, text) {
      inst.text = text;
      editing.textChanged(inst, old);
      ops.push({ settext: [inst.hid, text] });
    },
    resetTextContent() {},
    clearContainer() {},
    hideInstance(inst) {
      inst.suspended = true;
      emitProps(inst);
    },
    unhideInstance(inst) {
      inst.suspended = false;
      emitProps(inst);
    },
    hideTextInstance(inst) {
      ops.push({ settext: [inst.hid, ""] });
    },
    unhideTextInstance(inst, text) {
      ops.push({ settext: [inst.hid, text] });
    },
    detachDeletedInstance(inst) {
      if (inst.sheet) sheets.head.removeChild(inst.sheet);
      instances.delete(inst.hid);
      if (focused === inst) focused = null;
    },
    getInstanceFromNode: () => null,
    beforeActiveInstanceBlur() {},
    afterActiveInstanceBlur() {},
    prepareScopeUpdate() {},
    getInstanceFromScope: () => null,
    setCurrentUpdatePriority(p) {
      priority = p;
    },
    getCurrentUpdatePriority: () => priority,
    resolveUpdatePriority: () => priority || DefaultEventPriority,
    maySuspendCommit: () => false,
    maySuspendCommitOnUpdate: () => false,
    maySuspendCommitInSyncRender: () => false,
    preloadInstance: () => true,
    startSuspendingCommit() {},
    suspendInstance() {},
    // Called on every transition commit (react-router navigations); there are no view transitions.
    suspendOnActiveViewTransition() {},
    waitForCommitToBeReady: () => null,
    NotPendingTransition: null,
    HostTransitionContext: createContext(null),
    resetFormInstance() {},
    requestPostPaintCallback() {},
    shouldAttemptEagerTransition: () => false,
    trackSchedulerEvent() {},
    resolveEventType: () => null,
    resolveEventTimeStamp: () => -1.1,
  });

  const logError = (err) => console.error(err);
  const root = reconciler.createContainer(body, ConcurrentRoot, null, false, null, "", logError, logError, logError, null);

  // shortcut: contentRect is the border box (no padding/border info in JS); fine for unpadded chart and scroll containers.
  class ResizeObserver {
    #callback;
    #targets = new Set();
    constructor(callback) {
      this.#callback = callback;
    }
    observe(target) {
      if (!(target?.hid >= 0) || this.#targets.has(target)) return;
      this.#targets.add(target);
      (target.observers ??= new Set()).add(this);
      emitProps(target);
      flush();
      // Like the DOM, an observation starts with the current size.
      if (target.rect) queueMicrotask(() => this.#targets.has(target) && this.notify(target));
    }
    unobserve(target) {
      if (!this.#targets.delete(target)) return;
      target.observers.delete(this);
      if (instances.has(target.hid)) {
        emitProps(target);
        flush();
      }
    }
    disconnect() {
      for (const t of [...this.#targets]) this.unobserve(t);
    }
    notify(target) {
      const rect = domRect(target.rect);
      const box = [{ inlineSize: rect.width, blockSize: rect.height }];
      this.#callback([{ target, contentRect: rect, borderBoxSize: box, contentBoxSize: box }], this);
    }
  }

  // Focus from JS (`el.focus()`): React handlers run now; the host focuses the native input or
  // shows `:focus` styles.
  function setFocus(node) {
    if (focused === node) return;
    const prev = focused;
    focused = node;
    const p = priority;
    priority = DiscreteEventPriority;
    try {
      if (prev && instances.has(prev.hid)) fire(prev, "blur");
      if (node) fire(node, "focus");
    } finally {
      priority = p;
    }
    editing.focusChanged(node);
    ops.push({ focus: node?.hid ?? null });
    flush();
  }

  // `requestAnimationFrame`: every callback asked for runs as the display's next frame begins (the
  // host says when), with one timestamp, as in a browser; what they change is drawn in the frame
  // after. Timed by JS alone, they drifted against the display and an
  // animation's frames reached the screen unevenly. None run while the window is hidden
  // (minimised, covered, another Space), as browsers pause it in background tabs; they run once
  // it shows again.
  let hidden = false;
  let dark = false;
  const frameCallbacks = new Map();
  let nextFrameId = 1;
  let frameAsked = false;
  function scheduleFrame() {
    if (frameAsked || hidden || !frameCallbacks.size) return;
    frameAsked = true;
    send({ t: "frame" });
  }
  function runFrame() {
    frameAsked = false;
    if (hidden) return;
    const now = performance.now();
    const callbacks = [...frameCallbacks.values()];
    frameCallbacks.clear();
    for (const callback of callbacks) {
      // As in a browser, one throwing doesn't stop the rest.
      try {
        callback(now);
      } catch (e) {
        queueMicrotask(() => {
          throw e;
        });
      }
    }
  }
  // The host draws an input's frame as soon as JS is done with it (see `Msg::Settled`): after
  // React's work for the event (microtasks) and anything it set off for "now" (0 ms timers).
  let settling = false;
  function settleSoon() {
    if (settling) return;
    settling = true;
    setTimeout(() => {
      settling = false;
      send({ t: "settled" });
    }, 0);
  }
  function setHidden(next) {
    if (hidden === next) return;
    hidden = next;
    globalThis.document?.dispatchEvent?.(new Event("visibilitychange"));
    scheduleFrame();
  }

  // Host event -> React handlers, bubbling through instance parents like react-dom.
  // Pointer capture: moves and the release go to the capturing node wherever the pointer is, and
  // the release ends it (vaul captures on press and ends its drag on the release).
  function dispatch(msg) {
    if (!NOT_INPUT.has(msg.type)) settleSoon();
    if (msg.type === "visibilitychange") return setHidden(msg.hidden);
    if (msg.type === "frame") return runFrame();
    // The system's light/dark appearance: `window` hears it for `matchMedia("(prefers-color-scheme)")`.
    if (msg.type === "appearance") {
      dark = msg.dark;
      return globalThis.dispatchEvent?.(new Event("appearance"));
    }
    // A benchmark driver (the host's `bench`): where an element is, and when an injected input's
    // events have been handled, their commits (sent in order before this) included.
    if (msg.type === "benchQuery") {
      const el = body.querySelectorAll(msg.selector)[msg.index ?? 0];
      const r = el?.getBoundingClientRect();
      const rect = r ? { x: r.x, y: r.y, width: r.width, height: r.height, value: el.value ?? null } : null;
      return send({ t: "bench", req: msg.req, rect });
    }
    if (msg.type === "benchSettle") return setTimeout(() => send({ t: "bench", req: msg.req }), 0);
    if (captured && !instances.has(captured.hid)) captured = null;
    if (!captured) return dispatchTo(msg);
    if (msg.type === "mousemove" || msg.type === "mouseup") {
      dispatchTo({ ...msg, id: captured.hid, target: null });
    } else if (msg.type === "document" && msg.events.some((e) => e === "pointermove" || e === "pointerup")) {
      const up = msg.events.includes("pointerup");
      dispatchTo({ ...msg, type: up ? "mouseup" : "mousemove", id: captured.hid, target: null });
      const rest = msg.events.filter((e) => !["pointermove", "mousemove", "pointerup", "mouseup"].includes(e));
      if (rest.length) dispatchTo({ ...msg, events: rest });
      if (!up) return;
    } else {
      return dispatchTo(msg);
    }
    if (msg.type !== "mousemove") captured = null;
  }

  function dispatchTo(msg) {
    let target = instances.get(msg.id);
    if (!target) return;
    // The host reports a press from the innermost listener; the DOM target is the innermost box
    // under the pointer, inside it (FullCalendar finds its event with `ev.target.closest()`).
    if (PRESSES.has(msg.type) && msg.target != null) target = instances.get(msg.target) ?? target;
    if (msg.type === "media") return dom.mediaEvent(target, msg);
    if (msg.type === "caret" || msg.type === "textinput" || msg.type === "composition") return editing.hostEvent(target, msg);
    if (msg.type === "resize") {
      const old = target.rect;
      target.rect = msg.rect;
      if (old?.width !== msg.rect.width || old?.height !== msg.rect.height) {
        // Observers hear about sizes a sync took after the code that asked has run, as in a browser.
        const notify = () => target.observers?.forEach((o) => o.notify(target));
        msg.sync ? queueMicrotask(notify) : notify();
      }
      return;
    }
    if (POINTER.has(msg.type) && msg.clientX != null) {
      const inner = svgTarget(target, msg.clientX, msg.clientY);
      if (msg.type !== "mousedown" && msg.type !== "mouseup" && msg.type !== "click") hoverSvg(msg.type === "mouseleave" ? null : inner, msg);
      if (inner && msg.type !== "mouseenter" && msg.type !== "mouseleave") target = inner;
    }
    if (msg.type === "scroll") {
      const at = [msg.scrollLeft, msg.scrollTop];
      target.scrolled ? target.scrolled.splice(0, 2, ...at) : (target.scrolled = at);
      // The first report only says the box scrolls; the page's scroll is `document`'s, heard by `window`.
      if (msg.init) return;
      if (target === html) return void globalThis.document?.dispatchEvent?.(new Event("scroll", { bubbles: true }));
    }
    if (msg.type === "change") {
      target.seq = msg.seq;
      // The host already shows what was typed; only library-built inputs keep it as an attribute,
      // and a range, which the host draws from the value it is sent.
      if (target.imperative || target.props?.type === "range") target.value = msg.value;
      else target._value = msg.value;
    }
    // A native input gained or lost focus; skip echoes of focus JS already applied.
    if (msg.type === "focus") {
      if (focused === target) return;
      if (focused && instances.has(focused.hid)) fire(focused, "blur");
      focused = target;
      editing.focusChanged(target);
    }
    if (msg.type === "blur") {
      if (focused !== target) return;
      focused = null;
      editing.focusChanged(null);
    }
    const prev = priority;
    // Like react-dom: presses and keys are discrete; pointer movement is continuous.
    const moving = ["mouseenter", "mouseleave", "mousemove"].includes(msg.type) || msg.events?.includes("pointermove");
    priority = moving ? ContinuousEventPriority : DiscreteEventPriority;
    try {
      if (msg.type === "document") {
        // Pointer events no element reported bubble from the innermost box the host saw under the
        // pointer (else <body>) to `document`.
        const from = (msg.target != null && instances.get(msg.target)) || body;
        for (const type of msg.events) {
          const event = domEvent(type, from, msg, true);
          if (type === "click" && !event.defaultPrevented) activate(from);
        }
        return;
      }
      if (target === html) {
        // Window-level keys go to the app's `document` listeners (Escape to dismiss, shortcuts),
        // from the element JS focused, as a browser targets the active element: MUI's menus close
        // on Escape and move on arrow keys in their own onKeyDown. Fields report their own keys.
        const { t, id, type, ...init } = msg;
        const dom = Object.assign(new Event(type, { bubbles: true, cancelable: true }), init);
        const el = focused && instances.has(focused.hid) && !FIELDS.has(focused.type) ? focused : null;
        if (!el) {
          propagate(dom, body);
          return void keyDefault(msg, dom.defaultPrevented);
        }
        captureOutside(dom);
        const event = fire(el, type, msg, dom.defaultPrevented);
        if (event.defaultPrevented) dom.preventDefault();
        if (!event.isPropagationStopped()) bubbleDomEvent(dom, el);
        // The key's release, at once (see `DOM_EVENTS.keydown`).
        if (type === "keydown") {
          const up = Object.assign(new Event("keyup", { bubbles: true, cancelable: true }), init);
          captureOutside(up);
          if (!fire(el, "keyup", msg).isPropagationStopped()) bubbleDomEvent(up, el);
        }
        return void keyDefault(msg, dom.defaultPrevented);
      }
      // Browser order: `document`'s capture listeners, React's handlers, then DOM listeners bubbling
      // up to `document`, all before any effect React runs for it (Radix registers outside-press
      // listeners after opening). Cancelling in capture cancels for React too: Radix Select ignores
      // the release of the press that opened it that way.
      const types = msg.type === "change" && target.type === "select" ? [...DOM_EVENTS.change, ["change", true]] : DOM_EVENTS[msg.type];
      const dom = (types ?? []).map(([type, bubbles]) => newDomEvent(type, target, msg, bubbles));
      for (const e of dom) captureOutside(e);
      const event = fire(target, msg.type, msg, dom.some((e) => e.defaultPrevented));
      if (msg.type === "keydown") fire(target, "keyup", msg);
      for (const e of dom) bubbleDomEvent(e, target);
      if (!event.defaultPrevented) submitFrom(target, msg);
      if (msg.type === "keydown") keyDefault(msg, event.defaultPrevented || dom.some((e) => e.defaultPrevented));
      if (msg.type === "click" && !event.defaultPrevented && !dom.some((e) => e.defaultPrevented)) activate(target);
    } finally {
      priority = prev;
    }
  }

  // The element under the pointer inside an <svg> at or below `node`: the host draws the svg as
  // one image and only knows its box, so the element is found by geometry. Null off any drawing.
  function svgTarget(node, x, y) {
    const svgs = node.type === "svg" ? [node] : subtree(node).filter((n) => n.type === "svg" && !outermostSvg(n.parent));
    for (const svg of svgs.reverse()) {
      // The pointer can reach a drawing that nothing measured yet (the listener is its container):
      // its box is asked for and reported by the next frame. Laying out on the spot would block
      // every move, as drawings mount under the pointer (a chart tooltip's marks) or stay hidden.
      if (!svg.rect) svg.measure();
      const r = svg.rect ?? {};
      if (r.width == null || x < r.x || y < r.y || x > r.x + r.width || y > r.y + r.height) continue;
      const [k, ox, oy] = viewBoxScale(svg, r);
      return hitTest(svg, (x - r.x) / k + ox, (y - r.y) / k + oy) ?? svg;
    }
    return null;
  }
  // Moving between elements of a drawing fires what a browser would: out/leave on the one left,
  // over/enter on the one entered (ApexCharts' and jsVectorMap's tooltips listen for these).
  let svgHover = null;
  function hoverSvg(next, msg) {
    if (next === svgHover) return;
    const prev = svgHover;
    svgHover = next;
    // The host matches `:hover` on it (TailAdmin's map regions are filled from CSS).
    ops.push({ hover: next?.hid ?? null });
    flush();
    const chain = (n) => {
      const out = [];
      for (let e = n; e?.type; e = e.parent) {
        out.push(e);
        if (e.type === "svg" && !outermostSvg(e.parent)) break;
      }
      return out;
    };
    const [from, to] = [chain(prev), chain(next)];
    // React's onPointerEnter/Leave too (MUI X Charts highlights a pie slice so); the outermost
    // <svg> is a box the host reports itself.
    const react = (n, type) => (n.type !== "svg" || outermostSvg(n.parent)) && fire(n, type, msg);
    if (prev) for (const type of ["pointerout", "mouseout"]) domEvent(type, prev, msg, true);
    for (const n of from.filter((n) => !to.includes(n))) {
      for (const type of ["pointerleave", "mouseleave"]) domEvent(type, n, msg, false);
      react(n, "mouseleave");
    }
    if (next) for (const type of ["pointerover", "mouseover"]) domEvent(type, next, msg, true);
    for (const n of to.filter((n) => !from.includes(n)).reverse()) {
      for (const type of ["pointerenter", "mouseenter"]) domEvent(type, n, msg, false);
      react(n, "mouseenter");
    }
  }

  // A DOM event at `target`: its listeners, then (if it bubbles) its ancestors', `document`'s and `window`'s.
  function domEvent(type, target, msg, bubbles) {
    const event = newDomEvent(type, target, msg, bubbles);
    propagate(event, target);
    return event;
  }
  function newDomEvent(type, target, msg, bubbles) {
    const pressed = { pointerdown: 1, mousedown: 1, pointerup: 0, mouseup: 0 }[type];
    if (pressed != null) buttons = pressed;
    const { clientX = 0, clientY = 0 } = msg;
    // Page coordinates are <body>'s, which scrolls with the page: what an absolutely positioned
    // child of <body> is placed by (jsVectorMap's tooltip follows `pageY`). Measured from the
    // first pointer event on.
    if (!body.measured) body.measure();
    const [pageX, pageY] = [clientX - (body.rect?.x ?? 0), clientY - (body.rect?.y ?? 0)];
    const { key, ctrlKey = false, metaKey = false, shiftKey = false, altKey = false } = msg;
    const init = { button: 0, buttons, isPrimary: true, pointerType: "mouse", clientX, clientY, pageX, pageY, key, ctrlKey, metaKey, shiftKey, altKey };
    const event = Object.assign(new Event(type, { bubbles, cancelable: true }), init);
    Object.defineProperty(event, "target", { value: target, configurable: true });
    // The DOM path, not the internal EventTarget a listener ran on: FullCalendar reads the target as
    // `composedPath()[0]`.
    const path = [];
    for (let n = target; n; n = n.parent) path.push(n);
    Object.defineProperty(event, "composedPath", { value: () => [...path, globalThis.document, globalThis].filter(Boolean) });
    return event;
  }
  // The target's and its ancestors' listeners, then (if it bubbles) `document`'s and `window`'s.
  // A `document` that keeps capture listeners apart (`dispatchCapture`) ran those already.
  function bubbleDomEvent(event, target) {
    propagate(event, target, true);
  }

  // `target` and the host nodes above it in the component tree, innermost first: as in react-dom, a
  // portal's events bubble to the component that rendered it (a Select list in a Drawer), not to
  // the node it is mounted in. Nodes React didn't create follow the host tree.
  function reactPath(target) {
    const path = [];
    if (!target.fiber) {
      for (let node = target; node; node = node.parent) path.push(node);
      return path;
    }
    // A detached fiber's stateNode is null (React clears it on deletion): no node to fire on.
    for (let f = target.fiber; f; f = f.return) if (f.tag === HOST_COMPONENT && f.stateNode) path.push(f.stateNode);
    return path;
  }

  // Runs React handlers for `type` from `target` up through its ancestors, like react-dom.
  function fire(target, type, msg = {}, prevented = false) {
    let stopped = false;
    const { key, clientX = 0, clientY = 0 } = msg;
    const event = {
      type,
      key,
      clientX,
      clientY,
      pageX: clientX,
      pageY: clientY,
      target,
      currentTarget: target,
      // Primary-button press with no modifiers: what routers' links and Radix triggers check for.
      button: 0,
      pointerType: "mouse",
      ctrlKey: msg.ctrlKey ?? false,
      metaKey: msg.metaKey ?? false,
      shiftKey: msg.shiftKey ?? false,
      altKey: msg.altKey ?? false,
      defaultPrevented: prevented,
      preventDefault() {
        event.defaultPrevented = true;
      },
      isDefaultPrevented: () => event.defaultPrevented,
      stopPropagation() {
        stopped = true;
      },
      isPropagationStopped: () => stopped,
      persist: noop,
    };
    event.nativeEvent = event;
    // Capture phase (`onPointerDownCapture`...) from the root down, then bubbling back up.
    const path = NON_BUBBLING.has(type) ? [target] : reactPath(target);
    for (const node of NON_BUBBLING.has(type) ? [] : [...path].reverse()) {
      if (stopped) break;
      event.currentTarget = node;
      for (const prop of HANDLERS[type] ?? []) node.props?.[prop + "Capture"]?.(event);
    }
    for (const node of path) {
      if (stopped) break;
      event.currentTarget = node;
      for (const prop of HANDLERS[type] ?? []) node.props?.[prop]?.(event);
    }
    return event;
  }

  // A click's default actions for checkable inputs: one toggles itself, and a label clicks its
  // control (a radio hidden `sr-only` behind a styled dot) or focuses a text field.
  function activate(target) {
    for (let n = target; n?.type; n = n.parent) {
      if (checkable(n)) return toggle(n);
      // A details' summary opens and closes it.
      if (n.type === "summary" && n.parent?.type === "details" && n.parent.kids?.find((k) => k.type) === n) {
        n.parent.open = !n.parent.open;
        return flush();
      }
      if (n.type === "label") {
        const id = n.props.htmlFor;
        const control = id != null ? [...instances.values()].find((i) => i.props?.id === id) : subtree(n).find(LABELABLE);
        if (control && checkable(control)) {
          fire(control, "click");
          toggle(control);
        } else control?.focus();
        return;
      }
      // A button or link inside a label is what was clicked.
      if (INTERACTIVE.has(n.type)) return;
    }
  }
  function toggle(input) {
    const radio = input.props.type === "radio";
    if (on(input.props.disabled) || (radio && input.checked)) return;
    // Uncontrolled radios in the group give up their check; React re-renders controlled ones.
    if (radio && input.props.name != null)
      for (const n of instances.values())
        if (n !== input && checkable(n) && n.props.name === input.props.name && n.checked && n.props.checked == null) {
          n.checked = false;
          emitProps(n);
        }
    input.checked = !input.checked;
    fire(input, "change");
    // DOM listeners hear it too (Vue's checkbox v-model listens for `change`).
    for (const type of ["input", "change"]) domEvent(type, input, {}, true);
    // A controlled input shows what React renders next, as react-dom restores it.
    if (input.props.checked != null) delete input._checked;
    emitProps(input);
    flush();
  }

  // A key's default action once its listeners let it: Escape closes the topmost modal dialog.
  function keyDefault(msg, prevented) {
    if (prevented || editing.keydown(msg) || msg.key !== "Escape") return;
    [...instances.values()].findLast((n) => n.type === "dialog" && n.isModal)?.requestClose();
  }

  // The browser's default actions that submit a form: clicking a submit button, or Enter in a text field.
  function submitFrom(target, msg) {
    let source = null;
    if (msg.type === "click") {
      for (let n = target; n && !source; n = n.parent) if (n.type === "button") source = n;
      if (source && ((source.props.type ?? "submit") !== "submit" || source.props.disabled)) return;
    } else if (msg.type === "keydown" && msg.key === "Enter" && target.type === "input") {
      source = target;
    }
    if (!source) return;
    const formId = source.props.form;
    let form = null;
    if (formId) form = [...instances.values()].find((i) => i.type === "form" && i.props.id === formId);
    else for (let n = source.parent; n && !form; n = n.parent) if (n.type === "form") form = n;
    if (!form) return;
    const submitted = fire(form, "submit");
    // `<form method="dialog">` closes its dialog instead, returning the submit button's value.
    if (submitted.defaultPrevented || String(form.props.method ?? "").toLowerCase() !== "dialog") return;
    let dialog = form.parent;
    while (dialog && dialog.type !== "dialog") dialog = dialog.parent;
    dialog?.close(source.type === "button" ? (source.getAttribute("value") ?? undefined) : undefined);
  }

  return {
    render(element) {
      reconciler.updateContainer(element, root, null, null);
    },
    /** The `<body>` host node, for `document.body` (portals mount into it). */
    body,
    ResizeObserver,
    /** `document.activeElement`: the focused host node, or <body>. */
    activeElement: () => focused ?? body,
    setRootClass(className) {
      html.props = { ...html.props, className };
      emitProps(html);
      flush();
    },
    /** An attribute on <html> (MUI's `data-mui-color-scheme`); `null` removes it. */
    setRootAttribute(name, value) {
      const { [name]: _, ...rest } = html.props;
      html.props = value == null ? rest : { ...rest, [name]: String(value) };
      emitProps(html);
      flush();
    },
    /** Whether the system appearance is dark, as `(prefers-color-scheme: dark)` matches. */
    prefersDark: () => dark,
    dispatch,
    requestAnimationFrame(callback) {
      const id = nextFrameId++;
      frameCallbacks.set(id, callback);
      scheduleFrame();
      return id;
    },
    cancelAnimationFrame: (id) => frameCallbacks.delete(id),
    /** `document.visibilityState`: "hidden" while the window is minimised, covered or on another Space. */
    visibilityState: () => (hidden ? "hidden" : "visible"),
    /** `document.createElement(NS)`, `createTextNode`, `createComment`, `createDocumentFragment` for library-built DOM. */
    createElement: (tag, svg) => dom.createElement(tag, svg),
    createTextNode: (text) => dom.createText(text),
    createComment: (data) => dom.createComment(data),
    createDocumentFragment: () => dom.createFragment(),
  };
}
