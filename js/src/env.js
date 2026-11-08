// `import "gpuijs/env"` first thing in an app: just enough `window`/`document` for React libraries
// (Radix, floating-ui, MUI, Recharts, ApexCharts, react-router, sonner, axios) to run outside a
// browser. Elements themselves are gpuijs host nodes; this file only fills in the globals around them.
// shortcut: a fixed desktop pointer media query and no real focus; replace as gpuijs grows window APIs.
import path from "node:path";
import {
  ResizeObserver,
  Window,
  activeElement,
  body,
  cancelAnimationFrame,
  createComment,
  createDocumentFragment,
  createElement,
  createStyleElement,
  createTextNode,
  head,
  prefersDark,
  requestAnimationFrame,
  setRootAttribute,
  setRootClass,
  trackStops,
  visibilityState,
} from "./index.js";
import { createStorage } from "./storage.js";
import * as webgl from "./webgl.js";
import { DataTransfer, MutationObserver, Range, Selection, StaticRange, selection } from "./editing.js";

const g = globalThis;
// `document` and `window` are event targets: Radix listens on and dispatches to them.
const target = (props) => Object.defineProperties(new EventTarget(), Object.getOwnPropertyDescriptors(props));

// <html>: the root host node, observed so its size (the viewport) is known. `window` hears a
// `resize` each time it changes, the first too: it arrives after mount, when apps read
// `innerWidth` (TailAdmin's sidebar decides mobile vs desktop then, and on resize).
let htmlNode;
const html = () => {
  if (!htmlNode) {
    htmlNode = body().parent;
    new ResizeObserver(() => g.dispatchEvent(new Event("resize"))).observe(htmlNode);
  }
  return htmlNode;
};
// Classes on <html> (theme, font) go to the host's root node.
const classes = new Set();
const setClasses = () => setRootClass([...classes].join(" "));
const classList = {
  add: (...c) => (c.forEach((x) => classes.add(x)), setClasses()),
  remove: (...c) => (c.forEach((x) => classes.delete(x)), setClasses()),
  contains: (c) => classes.has(c),
  forEach: (f) => [...classes].forEach(f),
};
// Other attributes on <html> (MUI's `data-mui-color-scheme`) go there too.
const rootAttrs = new Map();
let documentElement;

// `window`'s and `document`'s capture listeners are kept apart so gpuijs runs them before any element
// hears an event, as a browser's capture phase does (Radix Select cancels the release of the press
// that opened it).
function phased(obj, base = obj) {
  const bubble = { add: base.addEventListener.bind(base), remove: base.removeEventListener.bind(base), fire: base.dispatchEvent.bind(base) };
  const capture = new EventTarget();
  const inCapture = (o) => o === true || (typeof o === "object" && o?.capture === true);
  return Object.assign(obj, {
    addEventListener: (type, l, o) => (inCapture(o) ? capture.addEventListener(type, l, o) : bubble.add(type, l, o)),
    removeEventListener: (type, l, o) => (inCapture(o) ? capture.removeEventListener(type, l, o) : bubble.remove(type, l, o)),
    dispatchCapture: (e) => capture.dispatchEvent(e),
    dispatchBubble: (e) => bubble.fire(e),
    dispatchEvent: (e) => {
      trackStops(e);
      capture.dispatchEvent(e);
      return e.cancelBubble ? !e.defaultPrevented : bubble.fire(e);
    },
  });
}

g.window = g.self = globalThis;
phased(g, new EventTarget());
Object.defineProperty(g, "innerWidth", { get: () => html().rect?.width ?? 1024 });
Object.defineProperty(g, "innerHeight", { get: () => html().rect?.height ?? 768 });
// How far the page is scrolled (Popper.js adds these to a tooltip's viewport): <html> is the
// window's scroll container.
for (const [names, axis] of [[["scrollX", "pageXOffset"], "scrollLeft"], [["scrollY", "pageYOffset"], "scrollTop"]]) {
  for (const name of names) Object.defineProperty(g, name, { get: () => html()[axis] });
}
for (const m of ["scrollTo", "scroll", "scrollBy"]) g[m] = (...a) => html()[m](...a);
g.ResizeObserver = ResizeObserver;
// Real ones over host nodes, for editors that watch their DOM (editing.js).
Object.assign(g, { MutationObserver, Range, Selection, StaticRange, DataTransfer });
// addListener/removeListener: the deprecated pair MUI's color-scheme provider still calls.
// A desktop's pointer is a mouse, which hovers: MUI X Charts places its tooltip for one.
// `prefers-color-scheme` follows the system appearance, and its lists hear when that changes.
const mouse = /^(@media\s*)?\((any-)?(pointer:\s*fine|hover:\s*hover)\)$/;
const scheme = /^(@media\s*)?\(prefers-color-scheme:\s*(dark|light)\)$/;
g.matchMedia = (media) => {
  const want = media.trim().match(scheme)?.[2];
  const listeners = new Map();
  const list = {
    media,
    get matches() {
      return want ? (want === "dark") === prefersDark() : mouse.test(media.trim());
    },
    addEventListener(_type, f) {
      if (!want || listeners.has(f)) return;
      const heard = () => f({ media, matches: list.matches });
      listeners.set(f, heard);
      g.addEventListener("appearance", heard);
    },
    removeEventListener(_type, f) {
      g.removeEventListener("appearance", listeners.get(f));
      listeners.delete(f);
    },
    addListener: (f) => list.addEventListener("change", f),
    removeListener: (f) => list.removeEventListener("change", f),
  };
  return list;
};
// The cascade lives in the host, so this answers with CSS initial values, plus the size, padding and
// border a host node knows once measured (`rect`) — what floating-ui and Radix Select position with.
const edges = (prefix, suffix, px = []) =>
  Object.fromEntries(["Top", "Right", "Bottom", "Left"].map((side, i) => [prefix + side + suffix, `${px[i] ?? 0}px`]));
// A translation as a browser computes it, `matrix(1, 0, 0, 1, x, y)`: vaul reads how far a drawer
// was dragged from it. shortcut: px translations only (vaul's); any other transform reads 'none'.
const matrix = (transform = "") => {
  const m = transform.trim().match(/^translate(3d|X|Y)?\(([^)]*)\)$/);
  const px = (m?.[2] ?? "").split(",").map((v) => v.trim());
  if (!m || !px.every((v) => v === "0" || v.endsWith("px"))) return "none";
  const [a = 0, b = 0] = px.map(parseFloat);
  const [x, y] = m[1] === "Y" ? [0, a] : [a, m[1] === "X" ? 0 : b];
  return `matrix(1, 0, 0, 1, ${x}, ${y})`;
};
g.getComputedStyle = (el) => ({
  ...edges("padding", "", el?.rect?.padding),
  ...edges("border", "Width", el?.rect?.border),
  // shortcut: an inline `position` only (Popper.js checks for a fixed tooltip); stylesheets' read 'static'.
  position: el?.style?.position || el?.props?.style?.position || "static",
  ...{ display: "block", direction: "ltr", willChange: "auto", contain: "none" },
  ...{ overflow: "visible", overflowX: "visible", overflowY: "visible", containerType: "normal" },
  ...{ translate: "none", scale: "none", rotate: "none", perspective: "none" },
  transform: matrix(el?.style?.transform || el?.props?.style?.transform),
  ...{ filter: "none", backdropFilter: "none", animationName: "none" },
  // The host runs transitions and animations itself; to scripts (Base UI's Collapsible) none run.
  ...{ animationDuration: "0s", transitionDuration: "0s", transitionProperty: "none" },
  // shortcut: inline sizes only (Base UI's select reads its popup's max height); stylesheets' read 'none'.
  maxHeight: el?.style?.maxHeight || el?.props?.style?.maxHeight || "none",
  maxWidth: el?.style?.maxWidth || el?.props?.style?.maxWidth || "none",
  // Lazy, and laid out on the spot like `offsetWidth`: MUI X Charts sizes itself from these on mount.
  get width() {
    const w = el?.laidOut?.().width ?? el?.rect?.width;
    return w == null ? "auto" : `${w}px`;
  },
  get height() {
    const h = el?.laidOut?.().height ?? el?.rect?.height;
    return h == null ? "auto" : `${h}px`;
  },
  getPropertyValue: () => "",
});
// The caret and selection in editable content (editing.js).
g.getSelection = () => selection;
// Paused while the window is hidden, as in a browser's background tab.
g.requestAnimationFrame = requestAnimationFrame;
g.cancelAnimationFrame = cancelAnimationFrame;
// Idle callbacks run in a later task with a frame's worth of time to spare.
g.requestIdleCallback = (callback, { timeout } = {}) => setTimeout(() => callback({ didTimeout: false, timeRemaining: () => 16 }), Math.min(timeout ?? 1, 1));
g.cancelIdleCallback = (id) => clearTimeout(id);
// shortcut: macOS's Retina scale; report the window's real scale factor if a non-Retina display matters.
g.devicePixelRatio ??= 2;
g.screen ??= {
  get width() {
    return g.innerWidth;
  },
  get height() {
    return g.innerHeight;
  },
  get availWidth() {
    return g.innerWidth;
  },
  get availHeight() {
    return g.innerHeight;
  },
  colorDepth: 24,
  pixelDepth: 24,
};
g.CSS ??= {
  escape: (s) => String(s).replace(/[^\w-]/g, "\\$&").replace(/^(-?)(\d)/, (_, dash, d) => `${dash}\\3${d} `),
  // shortcut: claims support for everything; test against the host's cascade if a library branches on it.
  supports: () => true,
};
// Visibility from the host's layout: an observed element intersects when its box overlaps the
// window. Entries come when it is first observed and each time its box changes.
// shortcut: the window is the only root and margins/thresholds are ignored; scrolling alone doesn't report.
g.IntersectionObserver = class {
  #observer;
  constructor(callback, options = {}) {
    this.root = options.root ?? null;
    this.rootMargin = options.rootMargin ?? "0px";
    this.thresholds = [options.threshold ?? 0].flat();
    this.#observer = new ResizeObserver((entries) => callback(entries.map((e) => this.#entry(e.target)), this));
  }
  #entry(target) {
    const r = target.getBoundingClientRect();
    const [w, h] = [g.innerWidth, g.innerHeight];
    const x = Math.max(0, Math.min(r.right, w) - Math.max(r.left, 0));
    const y = Math.max(0, Math.min(r.bottom, h) - Math.max(r.top, 0));
    const area = r.width * r.height;
    const isIntersecting = x > 0 && y > 0;
    const root = { x: 0, y: 0, top: 0, left: 0, right: w, bottom: h, width: w, height: h };
    const left = Math.max(r.left, 0), top = Math.max(r.top, 0);
    const rect = { x: left, y: top, left, top, right: left + x, bottom: top + y, width: x, height: y };
    return { target, time: performance.now(), isIntersecting, intersectionRatio: area ? (x * y) / area : 0, boundingClientRect: r, intersectionRect: rect, rootBounds: root };
  }
  observe(target) {
    this.#observer.observe(target);
  }
  unobserve(target) {
    this.#observer.unobserve(target);
  }
  disconnect() {
    this.#observer.disconnect();
  }
  takeRecords = () => [];
};

// Host nodes count as elements, so libraries (floating-ui) treat them as real ones; text and
// comment nodes, fragments and `document` are nodes.
const isHostElement = (v) => v?.nodeType === 1;
const nodeTest = {
  Node: (v) => typeof v?.nodeType === "number",
  Element: isHostElement,
  HTMLElement: isHostElement,
  CharacterData: (v) => v?.nodeType === 3 || v?.nodeType === 8,
  Text: (v) => v?.nodeType === 3,
  Comment: (v) => v?.nodeType === 8,
};
for (const [name, test] of Object.entries(nodeTest)) {
  g[name] = class {
    static [Symbol.hasInstance] = test;
  };
  // Called on host nodes through the prototype: `Element.prototype.querySelector.call(el, s)` (jsVectorMap).
  for (const m of ["querySelector", "querySelectorAll", "closest", "matches", "contains", "getBoundingClientRect"]) {
    g[name].prototype[m] = function (...a) {
      return this[m](...a);
    };
  }
}
// `instanceof SVGElement` (dnd-kit) holds for host nodes in the SVG namespace.
g.SVGElement = class {
  static [Symbol.hasInstance] = (v) => isHostElement(v) && v.namespaceURI === "http://www.w3.org/2000/svg";
};
// `el instanceof HTMLButtonElement` (framer-motion skips presses on disabled buttons), and the
// other element interfaces scripts test a target with, by tag.
const TAG_CLASSES = {
  HTMLButtonElement: "button",
  HTMLAnchorElement: "a",
  HTMLDivElement: "div",
  HTMLSpanElement: "span",
  HTMLLabelElement: "label",
  HTMLImageElement: "img",
  HTMLCanvasElement: "canvas",
  HTMLOptionElement: "option",
  HTMLTableElement: "table",
  HTMLTableRowElement: "tr",
  HTMLTableCellElement: /^t[dh]$/,
  HTMLUListElement: "ul",
  HTMLOListElement: "ol",
  HTMLLIElement: "li",
  HTMLParagraphElement: "p",
  HTMLHeadingElement: /^h[1-6]$/,
  HTMLTemplateElement: "template",
  HTMLDetailsElement: "details",
  HTMLDialogElement: "dialog",
  HTMLStyleElement: "style",
  HTMLVideoElement: "video",
  HTMLAudioElement: "audio",
  HTMLMediaElement: /^(audio|video)$/,
  SVGSVGElement: "svg",
};
const tagIs = (v, tag) => isHostElement(v) && (typeof tag === "string" ? v.localName === tag : tag.test(v.localName));
for (const [name, tag] of Object.entries(TAG_CLASSES)) {
  g[name] = class {
    static [Symbol.hasInstance] = (v) => tagIs(v, tag);
  };
}
// The host's events are plain `Event`s; each subclass claims them by type (Base UI's focus manager
// asks `event instanceof win.KeyboardEvent` as a popup closes), and constructs as an `Event` with
// its init fields.
const eventClass = (types) =>
  class extends Event {
    constructor(type, init = {}) {
      super(type, init);
      const { bubbles: _, cancelable: __, composed: ___, ...fields } = init;
      Object.assign(this, fields);
    }
    static [Symbol.hasInstance](v) {
      return Function.prototype[Symbol.hasInstance].call(this, v) || (v instanceof Event && types.test(v.type));
    }
  };
const pointerTypes = /^(pointer|mouse|click$|dblclick$|auxclick$|contextmenu$|wheel$|drag|drop$)/;
for (const [name, types] of [
  ["UIEvent", /^(pointer|mouse|click$|dblclick$|auxclick$|contextmenu$|wheel$|drag|drop$|key|focus|blur$|input$|beforeinput$|touch)/],
  ["MouseEvent", pointerTypes],
  ["PointerEvent", /^pointer/],
  ["WheelEvent", /^wheel$/],
  ["DragEvent", /^(drag|drop$)/],
  ["KeyboardEvent", /^key/],
  ["FocusEvent", /^(focus|blur$)/],
  ["InputEvent", /^(input|beforeinput)$/],
  ["ClipboardEvent", /^(copy|cut|paste)$/],
  ["CompositionEvent", /^composition/],
  ["TouchEvent", /^touch/],
]) {
  g[name] ??= eventClass(types);
}
// Lexical edits through `beforeinput` where it can, which it learns from this method being there.
g.InputEvent.prototype.getTargetRanges ??= () => [];
// Markup parsed into detached host nodes (TipTap reads its HTML content through one).
g.DOMParser = class DOMParser {
  parseFromString(markup, _type) {
    const text = String(markup);
    const body = createElement("body");
    body.innerHTML = text.match(/<body[^>]*>([\s\S]*)<\/body>/i)?.[1] ?? text.replace(/<\/?(html|head|body)[^>]*>/gi, "");
    return { body, documentElement: body, querySelector: (s) => body.querySelector(s), querySelectorAll: (s) => body.querySelectorAll(s) };
  }
};
// `node instanceof window.Document` (dnd-kit) is true of `document` only.
g.Document = class {
  static [Symbol.hasInstance] = (v) => v != null && v === g.document;
};
// Used for `instanceof` checks (zod, Radix form association, MUI X charts' gestures) and by Radix's
// hidden native inputs, which borrow the prototype's `value`/`checked` setters; host nodes are never instances.
// The setters are no-ops: Radix sets a hidden input through them, then dispatches the event itself.
for (const name of ["FileList", "ShadowRoot"]) g[name] ??= class {};
for (const [name, tag] of [["HTMLInputElement", "input"], ["HTMLSelectElement", "select"], ["HTMLTextAreaElement", "textarea"], ["HTMLFormElement", "form"]]) {
  g[name] = class {
    set value(_) {}
    set checked(_) {}
    static [Symbol.hasInstance] = (v) => tagIs(v, tag);
  };
}
// Radix Select collects its items in a detached fragment.
g.DocumentFragment = class {
  nodeType = 11;
  static [Symbol.hasInstance] = (v) => v?.nodeType === 11;
};
// `new FormData(form)`: the form's named, enabled controls, as a browser submits them.
{
  const Native = g.FormData;
  g.FormData = class extends Native {
    constructor(form, ...rest) {
      super(...(form?.nodeType === 1 ? [] : [form, ...rest].filter((a) => a !== undefined)));
      if (form?.nodeType !== 1) return;
      for (const el of form.querySelectorAll("input, select, textarea")) {
        if (!el.getAttribute("name") || el.hasAttribute("disabled")) continue;
        const type = el.getAttribute("type");
        if ((type === "checkbox" || type === "radio") && !el.checked) continue;
        if (type === "submit" || type === "button" || type === "reset") continue;
        this.append(el.getAttribute("name"), el.value ?? ((type === "checkbox" || type === "radio") && "on") ?? "");
      }
    }
  };
}
// `new Image()`: an <img> outside the page, which loads all the same (Radix Avatar preloads with
// one, Three.js's ImageLoader decodes textures through one).
g.Image = function Image(width, height) {
  const image = g.document.createElement("img");
  if (width != null) image.setAttribute("width", width);
  if (height != null) image.setAttribute("height", height);
  return image;
};

// WebGL's interfaces: Three.js tells WebGL2 from WebGL1 with `instanceof WebGL2RenderingContext`.
for (const name of ["WebGLRenderingContext", "WebGL2RenderingContext", "WebGLBuffer", "WebGLFramebuffer", "WebGLProgram", "WebGLQuery", "WebGLRenderbuffer", "WebGLSampler", "WebGLShader", "WebGLSync", "WebGLTexture", "WebGLTransformFeedback", "WebGLVertexArrayObject", "WebGLUniformLocation", "WebGLActiveInfo", "WebGLShaderPrecisionFormat"]) {
  g[name] = webgl[name];
}

// `new Audio(url)`: an <audio> outside the page, which plays all the same.
g.Audio = function Audio(src) {
  const audio = g.document.createElement("audio");
  if (src != null) audio.src = src;
  return audio;
};

// `Node`'s constants: MUI orders a menu's items by `compareDocumentPosition`.
Object.assign(g.Node, {
  ELEMENT_NODE: 1,
  TEXT_NODE: 3,
  COMMENT_NODE: 8,
  DOCUMENT_NODE: 9,
  DOCUMENT_FRAGMENT_NODE: 11,
  DOCUMENT_POSITION_DISCONNECTED: 1,
  DOCUMENT_POSITION_PRECEDING: 2,
  DOCUMENT_POSITION_FOLLOWING: 4,
  DOCUMENT_POSITION_CONTAINS: 8,
  DOCUMENT_POSITION_CONTAINED_BY: 16,
});

// Radix FocusScope walks a container's elements to find what can take focus.
g.NodeFilter = { SHOW_ELEMENT: 1, FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3 };
function createTreeWalker(root, _show, filter) {
  const order = [];
  const visit = (n) => {
    for (const k of n.kids ?? []) {
      if (!k.type) continue;
      const verdict = filter?.acceptNode(k) ?? 1;
      if (verdict === 1) order.push(k);
      if (verdict !== 2) visit(k);
    }
  };
  visit(root);
  let i = -1;
  return {
    get currentNode() {
      return order[i] ?? root;
    },
    nextNode: () => order[++i] ?? null,
  };
}

// Everything in the document a selector matches: <html>, <body> and what is under it.
const queryAll = (selector) => [html(), body()].filter((n) => n.matches(selector)).concat(body().querySelectorAll(selector));
const createRange = () => new Range();
// "loading" while the app's modules run, then DOMContentLoaded and load, as a page's scripts see it.
let readyState = "loading";
let title = "";

const cookies = new Map();
g.document = target({
  nodeType: 9,
  nodeName: "#document",
  get readyState() {
    return readyState;
  },
  // The window's title.
  get title() {
    return Window.getAllWindows()[0]?.title ?? title;
  },
  set title(text) {
    title = String(text);
    Window.getAllWindows()[0]?.setTitle(title);
  },
  get location() {
    return g.location;
  },
  set location(url) {
    g.location.href = url;
  },
  get scrollingElement() {
    return g.document.documentElement;
  },
  createRange,
  get documentElement() {
    // The real root node, with `classList` and attribute writes routed as above.
    documentElement ??= Object.assign(Object.create(html()), {
      classList,
      setAttribute: (k, v) => (rootAttrs.set(k, String(v)), setRootAttribute(k, String(v))),
      removeAttribute: (k) => (rootAttrs.delete(k), setRootAttribute(k, null)),
      getAttribute: (k) => rootAttrs.get(k) ?? null,
      hasAttribute: (k) => rootAttrs.has(k),
    });
    return documentElement;
  },
  // Portals mount here.
  get body() {
    return body();
  },
  // Injected <style>s (sonner, react-resizable-panels' drag cursor) style the window after app.css.
  get head() {
    return head();
  },
  // react-router finds its window here.
  defaultView: g,
  // Library-built DOM (ApexCharts' SVG, flatpickr's calendar, Recharts' measuring <span>) is made of
  // host nodes, rendered once attached to one React rendered. <style> styles the window.
  createElement: (tag) => (tag.toLowerCase() === "style" ? createStyleElement() : createElement(tag)),
  createElementNS: (ns, tag) =>
    ns === "http://www.w3.org/2000/svg" ? createElement(tag, true) : g.document.createElement(tag),
  createTextNode: (text) => createTextNode(text),
  createDocumentFragment: () => createDocumentFragment(),
  createComment: (data) => createComment(data),
  // Solid imports templates that hold custom elements.
  importNode: (node, deep) => node.cloneNode(deep),
  createEvent: () => new Event(""),
  createTreeWalker,
  styleSheets: [],
  getSelection: () => selection,
  get activeElement() {
    return activeElement();
  },
  // "hidden" while the window is minimised, covered or on another Space; `visibilitychange` fires.
  get visibilityState() {
    return visibilityState();
  },
  get hidden() {
    return visibilityState() === "hidden";
  },
  // The document is its own root, as in a browser: MUI X charts listen for pointer events there.
  getRootNode: () => g.document,
  querySelector: (selector) => queryAll(selector)[0] ?? null,
  querySelectorAll: queryAll,
  contains: (node) => html().contains(node),
  // Injected <style>s (ApexCharts checks for its own before adding it) and elements by id.
  getElementById: (id) => head().children.find((s) => s.id === id) ?? body().querySelector(`[id="${id}"]`),
  // `<head>` is outside the body's tree: TipTap appends its styles to `getElementsByTagName("head")[0]`.
  getElementsByTagName: (tag) => (String(tag).toLowerCase() === "head" ? [g.document.head] : queryAll(tag === "*" ? "*" : CSS.escape(tag))),
  // A detached document (TipTap serializes HTML in one): document's factories, a body of its own.
  implementation: {
    createHTMLDocument: () => {
      const body = createElement("body");
      return Object.create(g.document, { body: { value: body }, documentElement: { value: body } });
    },
    hasFeature: () => true,
  },
  getElementsByClassName: (names) => queryAll(names.trim().split(/\s+/).map((c) => "." + CSS.escape(c)).join("")),
  getElementsByName: (name) => queryAll(`[name="${name}"]`),
  get cookie() {
    return [...cookies].map(([k, v]) => `${k}=${v}`).join("; ");
  },
  set cookie(line) {
    const [pair, ...attrs] = line.split(";");
    const [k, v] = pair.split("=");
    if (attrs.some((a) => a.trim() === "max-age=0")) cookies.delete(k.trim());
    else cookies.set(k.trim(), v);
  },
});

phased(g.document);
// An event dispatched on `document` is captured by `window` first and, if it bubbles, reaches it after.
{
  const doc = g.document;
  doc.dispatchEvent = (e) => {
    trackStops(e);
    if (!Object.hasOwn(e, "target")) Object.defineProperty(e, "target", { value: doc, configurable: true });
    const steps = [[g, g.dispatchCapture], [doc, doc.dispatchCapture], [doc, doc.dispatchBubble], ...(e.bubbles ? [[g, g.dispatchBubble]] : [])];
    for (const [currentTarget, fire] of steps) {
      if (e.cancelBubble) break;
      Object.defineProperty(e, "currentTarget", { value: currentTarget, configurable: true });
      fire(e);
    }
    return !e.defaultPrevented;
  };
}

setTimeout(() => {
  readyState = "interactive";
  g.document.dispatchEvent(new Event("readystatechange"));
  g.document.dispatchEvent(new Event("DOMContentLoaded", { bubbles: true }));
  readyState = "complete";
  g.document.dispatchEvent(new Event("readystatechange"));
  g.dispatchEvent(new Event("load"));
});

g.localStorage = createStorage(process.env.GPUIJS_DATA_DIR && path.join(process.env.GPUIJS_DATA_DIR, "localStorage.json"));
g.navigator ??= {};
g.navigator.language ??= "en-US";

// gpuijs has no address bar: history is a list of entries; ROUTE=/calendar opens a page directly.
const entries = [{ state: null, url: process.env.ROUTE ?? "/" }];
let index = 0;
// As in the DOM, the URL is optional and resolves against the current one.
g.location = new URL("http://localhost/");
const go = (url = g.location.href) => (g.location = new URL(url, g.location)).href;
go(entries[0].url);
g.history = {
  get length() {
    return entries.length;
  },
  get state() {
    return entries[index].state;
  },
  scrollRestoration: "auto",
  pushState(state, _, url) {
    entries.splice(++index, Infinity, { state, url: go(url) });
  },
  replaceState(state, _, url) {
    entries[index] = { state, url: go(url) };
  },
  go(delta = 0) {
    index = Math.max(0, Math.min(entries.length - 1, index + delta));
    go(entries[index].url);
    g.dispatchEvent(Object.assign(new Event("popstate"), { state: entries[index].state }));
  },
  back: () => g.history.go(-1),
  forward: () => g.history.go(1),
};
