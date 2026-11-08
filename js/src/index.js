import { readFileSync } from "node:fs";
import { createHead } from "./head.js";
import { attachHost, hostEvent } from "./native.js";
import { createRenderer } from "./renderer.js";
import { setMetrics } from "./text.js";

export { Menu, MenuItem, Window, app, dialog } from "./native.js";
export { assetPath, fetchAsset, trackStops } from "./renderer.js";
export { measureText } from "./text.js";
import { connect } from "./transport.js";

let renderer;
let styles;
let send;
// The host's font metrics come first; rendering waits for them so measuring text works.
let ready = false;
let pending = null;

// The window opens with the options of the app's `new Window(options)`, or a default one the first
// time something reaches the host without one. Building DOM doesn't open it: Vue makes a <template>
// as it loads, before the app's `new Window` runs, and that only goes out at the end of the task.
function open(windowOptions = {}) {
  if (send) return;
  instance();
  send = connect((msg) => {
    if (msg.t === "event" && !hostEvent(msg)) renderer.dispatch(msg);
    if (msg.t === "metrics") {
      setMetrics(msg);
      ready = true;
      if (pending) renderer.render(pending);
      pending = null;
    }
  });
  send({ t: "window", options: windowOptions });
  // The CLI compiles the app's CSS (Tailwind included) and points us at the result.
  if (process.env.GPUIJS_CSS) send({ t: "css", text: readFileSync(process.env.GPUIJS_CSS, "utf8") });
}

function toHost(msg) {
  open();
  send(msg);
}

function instance() {
  if (renderer) return renderer;
  styles = createHead(toHost);
  renderer = createRenderer(toHost, styles);
  return renderer;
}

attachHost({
  start: open,
  started: () => !!send,
  send: toHost,
  render(element) {
    const r = instance();
    open();
    if (ready) r.render(element);
    else pending = element;
  },
});

/** The `<body>` node; portals (`createPortal(el, body())`) render into the window through it. */
export function body() {
  return instance().body;
}

/** The focused host node, or `<body>`, as `document.activeElement` reports it. */
export function activeElement() {
  return instance().activeElement();
}

/** `ResizeObserver` over host nodes: the host reports an observed node's bounds after layout. */
export class ResizeObserver {
  constructor(callback) {
    return new (instance().ResizeObserver)(callback);
  }
}

/**
 * `requestAnimationFrame`: runs `callback` about one display frame later, and not while the window
 * is hidden (minimised, covered, on another Space), as browsers pause it in background tabs.
 */
export function requestAnimationFrame(callback) {
  return instance().requestAnimationFrame(callback);
}

/** `cancelAnimationFrame`. */
export function cancelAnimationFrame(id) {
  instance().cancelAnimationFrame(id);
}

/**
 * `document.visibilityState`: "hidden" while the window is minimised, covered or on another Space.
 * `document` hears `visibilitychange` when it changes.
 */
export function visibilityState() {
  return instance().visibilityState();
}

/** Sets `class` on the root `<html>` node, e.g. `setRootClass("dark")`. */
export function setRootClass(className) {
  instance().setRootClass(className);
}

/** Sets (or, with `null`, removes) an attribute on the root `<html>` node. */
export function setRootAttribute(name, value) {
  instance().setRootAttribute(name, value);
}

/** Whether the system appearance is dark; `window` hears `appearance` when it changes. */
export function prefersDark() {
  return instance().prefersDark();
}

/** `document.head`: <style> elements appended to it style the window, after the app's CSS. */
export function head() {
  instance();
  return styles.head;
}

/** `document.createElement("style")`, for libraries that inject CSS at runtime. */
export function createStyleElement() {
  instance();
  return styles.createStyle();
}

/**
 * `document.createElement`/`createElementNS` for DOM a library builds itself (ApexCharts' SVG,
 * flatpickr's calendar): a host node React doesn't own, shown once attached to one that is.
 */
export function createElement(tag, svg = false) {
  return instance().createElement(tag, svg);
}

/** `document.createTextNode`, for library-built DOM. */
export function createTextNode(text) {
  return instance().createTextNode(text);
}

/** `document.createComment`, for Vue's and Solid's anchors. */
export function createComment(data) {
  return instance().createComment(data);
}

/** `document.createDocumentFragment`, for library-built DOM. */
export function createDocumentFragment() {
  return instance().createDocumentFragment();
}
