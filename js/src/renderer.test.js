import { expect, test } from "bun:test";
import { createElement as h, startTransition, useState } from "react";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHead } from "./head.js";
import { assetPath, createRenderer, fetchAsset, sameProps } from "./renderer.js";
import { skia } from "./media.js";

const tick = () => new Promise((r) => setTimeout(r, 20));

// `sent` holds what the tests here look at: commits and replies, not `settled` (tested below).
function setup(element) {
  const sent = [];
  const r = createRenderer((msg) => msg.t !== "settled" && sent.push(msg));
  r.render(element);
  return { r, sent };
}

test("after input, JS says it's settled once, after the commit it caused", async () => {
  const sent = [];
  function App() {
    const [n, set] = useState(0);
    return h("button", { onClick: () => set((x) => x + 1) }, `n=${n}`);
  }
  const r = createRenderer((msg) => sent.push(msg));
  r.render(h(App));
  await tick();
  const button = sent[0].ops.find((op) => op.el?.[1] === "button").el[0];
  sent.length = 0;
  r.dispatch({ t: "event", id: button, type: "click" });
  r.dispatch({ t: "event", id: button, type: "click" });
  r.dispatch({ t: "event", id: button, type: "resize", rect: { x: 0, y: 0, width: 1, height: 1 } });
  await new Promise((resolve) => setTimeout(resolve, 5));
  expect(sent.map((m) => m.t)).toEqual(["commit", "commit", "settled"]);
});

test("initial commit creates, attaches and wires props", async () => {
  const { sent } = setup(
    h("div", { className: "p-4", style: { padding: 8, opacity: 0.5 }, "data-slot": "card", onClick() {} }, "hi"),
  );
  await tick();
  expect(sent).toHaveLength(1);
  const { ops } = sent[0];
  const [div, tag, props] = ops.find((op) => op.el).el;
  const [txt] = ops.find((op) => op.txt).txt;
  expect(tag).toBe("div");
  expect(props).toMatchObject({
    class: "p-4",
    style: "padding:8px;opacity:0.5",
    events: ["click"],
    attrs: { "data-slot": "card" },
  });
  expect(ops.filter((op) => op.append)).toEqual([{ append: [div, txt] }, { append: [1, div] }]);
});

test("children removed one by one leave their parent, and may come back", async () => {
  let set;
  function List() {
    const [ids, setIds] = useState(() => Array.from({ length: 1000 }, (_, i) => i));
    set = setIds;
    return h("div", { className: "list" }, ids.map((i) => h("div", { key: i }, i)));
  }
  const { r, sent } = setup(h(List));
  await tick();
  const list = r.body.kids[0];
  set([]);
  await tick();
  expect(list.kids).toEqual([]);
  expect(sent.at(-1).ops.filter((op) => op.remove)).toHaveLength(1000);
  set([7, 3]);
  await tick();
  expect(list.kids.map((k) => k.kids[0].text)).toEqual(["7", "3"]);
  expect(list.firstChild.nextSibling).toBe(list.lastChild);
  // A keyed swap that has React move every row between the two.
  const ids = Array.from({ length: 100 }, (_, i) => i);
  set(ids);
  await tick();
  const swapped = [0, 98, ...ids.slice(2, 98), 1, 99];
  set(swapped);
  await tick();
  expect(list.kids.map((k) => Number(k.kids[0].text))).toEqual(swapped);
});

test("props wire the same when only handlers, children or style objects are new", () => {
  const a = { className: "row", style: { padding: 8 }, onClick() {}, children: "a" };
  expect(sameProps(a, { className: "row", style: { padding: 8 }, onClick() {}, children: "b" })).toBe(true);
  expect(sameProps(a, { ...a, className: "row on" })).toBe(false);
  expect(sameProps(a, { ...a, style: { padding: 9 } })).toBe(false);
  expect(sameProps(a, { ...a, onClick: undefined })).toBe(false);
  expect(sameProps(a, { ...a, "data-state": "open" })).toBe(false);
});

test("a re-render with equal props sends nothing; a changed prop is sent", async () => {
  let set;
  function App() {
    const [n, setN] = useState(0);
    set = setN;
    return h("div", { className: n > 1 ? "b" : "a", onClick: () => n }, String(n > 0));
  }
  const { sent } = setup(h(App));
  await tick();
  sent.length = 0;
  set(1);
  await tick();
  expect(sent.flatMap((m) => m.ops).filter((op) => op.set)).toEqual([]);
  set(2);
  await tick();
  expect(sent.flatMap((m) => m.ops).filter((op) => op.set)).toHaveLength(1);
});

test("click bubbles to ancestors until stopPropagation", async () => {
  const calls = [];
  function App() {
    const [n, setN] = useState(0);
    return h(
      "div",
      { onClick: () => calls.push("outer") },
      h("div", { onClick: () => calls.push("middle") }, h("span", { onClick: (e) => (calls.push("inner"), setN(n + 1)) }, `n=${n}`)),
      h("button", { onClick: (e) => (calls.push("button"), e.stopPropagation()) }),
    );
  }
  const { r, sent } = setup(h(App));
  await tick();
  const idOf = (tag) => sent[0].ops.find((op) => op.el?.[1] === tag).el[0];
  const [span, button] = [idOf("span"), idOf("button")];

  r.dispatch({ t: "event", id: span, type: "click" });
  await tick();
  expect(calls).toEqual(["inner", "middle", "outer"]);
  const spanText = sent[0].ops.find((op) => op.txt?.[1] === "n=0").txt[0];
  expect(sent.at(-1).ops).toContainEqual({ settext: [spanText, "n=1"] });

  calls.length = 0;
  r.dispatch({ t: "event", id: button, type: "click" });
  expect(calls).toEqual(["button"]);
});

test("change events update target.value and the input seq", async () => {
  let seen;
  const { r, sent } = setup(h("input", { value: "a", onChange: (e) => (seen = e.target.value) }));
  await tick();
  const id = sent[0].ops[0].el[0];
  r.dispatch({ t: "event", id, type: "change", value: "ab", seq: 1 });
  expect(seen).toBe("ab");
});

test("a script writing a React input's value shows it in the host", async () => {
  let input;
  const { r, sent } = setup(h("input", { ref: (n) => (input = n), placeholder: "pick" }));
  await tick();
  input.value = "Oct 17 to Oct 23";
  await tick();
  const sets = () => sent.flatMap((m) => m.ops ?? []).filter((op) => op.set?.[0] === input.hid);
  expect(sets().at(-1).set[1].value).toBe("Oct 17 to Oct 23");
  expect(input.value).toBe("Oct 17 to Oct 23");
});

test("svg elements carry their attributes in SVG spelling", async () => {
  const { sent } = setup(
    h("svg", { viewBox: "0 0 24 24", strokeWidth: 2, className: "size-4", fill: "none" }, h("path", { d: "M1 1L2 2", strokeLinecap: "round" })),
  );
  await tick();
  const els = sent[0].ops.filter((op) => op.el).map((op) => op.el);
  expect(els.find(([, t]) => t === "svg")[2].attrs).toEqual({ viewBox: "0 0 24 24", "stroke-width": "2", fill: "none" });
  expect(els.find(([, t]) => t === "path")[2].attrs).toEqual({ d: "M1 1L2 2", "stroke-linecap": "round" });
});

test("React-rendered <style> elements style the window while mounted", async () => {
  const sent = [];
  const send = (msg) => sent.push(msg);
  const r = createRenderer(send, createHead(send));
  const css = (text) => h("style", { dangerouslySetInnerHTML: { __html: text } });
  r.render(h("div", null, css("a{}")));
  await tick();
  r.render(h("div", null, css("b{}")));
  await tick();
  r.render(h("div"));
  await tick();
  expect(sent.filter((m) => m.t === "styles").map((m) => m.sheets)).toEqual([["a{}"], ["b{}"], []]);
});

test("ResizeObserver asks the host to measure and reports host sizes", async () => {
  let node;
  const { r, sent } = setup(h("div", { ref: (n) => (node = n) }));
  await tick();
  const seen = [];
  const ro = new r.ResizeObserver((entries) => seen.push(entries[0].contentRect.width));
  ro.observe(node);
  expect(sent.at(-1).ops).toEqual([{ set: [node.hid, expect.objectContaining({ measure: true })] }]);
  r.dispatch({ t: "event", type: "resize", id: node.hid, rect: { x: 1, y: 2, width: 30, height: 40 } });
  r.dispatch({ t: "event", type: "resize", id: node.hid, rect: { x: 5, y: 2, width: 30, height: 40 } });
  expect(seen).toEqual([30]); // a move alone is not a resize
  expect(node.getBoundingClientRect()).toMatchObject({ left: 5, top: 2, right: 35, bottom: 42 });
  // The host's rect is the border box; client sizes leave the borders out.
  r.dispatch({ t: "event", type: "resize", id: node.hid, rect: { x: 5, y: 2, width: 30, height: 40, border: [1, 2, 3, 4] } });
  expect([node.offsetWidth, node.clientWidth, node.clientHeight]).toEqual([30, 24, 36]);
  ro.disconnect();
  expect(sent.at(-1).ops[0].set[1].measure).toBeUndefined();
});

test("submit buttons and Enter submit their form", async () => {
  const submits = [];
  let button, input, outside;
  const { r } = setup(
    h(
      "div",
      null,
      h(
        "form",
        { id: "f", onSubmit: (e) => (e.preventDefault(), submits.push("form")) },
        h("input", { ref: (n) => (input = n) }),
        h("button", { ref: (n) => (button = n) }, h("span", null, "Save")),
        h("button", { type: "button", onClick: () => submits.push("plain") }),
      ),
      h("button", { type: "submit", form: "f", ref: (n) => (outside = n) }),
    ),
  );
  await tick();
  r.dispatch({ t: "event", type: "click", id: button.kids[0].hid });
  r.dispatch({ t: "event", type: "click", id: outside.hid });
  r.dispatch({ t: "event", type: "keydown", key: "Enter", id: input.hid });
  r.dispatch({ t: "event", type: "keydown", key: "a", id: input.hid });
  r.dispatch({ t: "event", type: "click", id: button.parent.kids[2].hid });
  expect(submits).toEqual(["form", "form", "form", "plain"]);
});

test("submit buttons without handlers still ask the host for clicks", async () => {
  const { sent } = setup(
    h("div", null, h("button", { type: "submit", form: "f" }), h("button", null), h("button", { type: "button" })),
  );
  await tick();
  const events = sent[0].ops.filter((op) => op.el?.[1] === "button").map((op) => op.el[2].events ?? []);
  expect(events).toEqual([["click"], ["click"], []]);
});

test("focus() moves the active element, runs onFocus/onBlur and tells the host", async () => {
  const seen = [];
  let a, b;
  const { r, sent } = setup(
    h(
      "div",
      { onFocus: (e) => seen.push(`focus ${e.target.props.id}`), onBlur: (e) => seen.push(`blur ${e.target.props.id}`) },
      h("button", { id: "a", ref: (n) => (a = n) }),
      h("input", { id: "b", ref: (n) => (b = n) }),
    ),
  );
  await tick();
  expect(r.activeElement()).toBe(r.body);
  a.focus();
  b.focus();
  expect(r.activeElement()).toBe(b);
  expect(sent.at(-1).ops).toEqual([{ focus: b.hid }]);
  // The host echoes focus it applied; that must not fire twice. Then the user clicks away.
  r.dispatch({ t: "event", type: "focus", id: b.hid });
  r.dispatch({ t: "event", type: "blur", id: b.hid });
  expect(seen).toEqual(["focus a", "blur a", "focus b", "blur b"]);
  expect(r.activeElement()).toBe(r.body);
});

test("host nodes answer querySelectorAll, matches and closest", async () => {
  let root, item;
  setup(
    h(
      "div",
      { "cmdk-root": "", ref: (n) => (root = n) },
      h("div", { "cmdk-group": "", className: "g x" }, h("div", { "cmdk-item": "", "aria-disabled": false, id: "one", ref: (n) => (item = n) })),
      h("div", { "cmdk-item": "", "aria-disabled": true }),
      h("span", { "data-value": "a b" }),
    ),
  );
  await tick();
  expect(root.querySelectorAll('[cmdk-item=""]:not([aria-disabled="true"])')).toEqual([item]);
  expect(root.querySelectorAll("[cmdk-group] [cmdk-item]")).toEqual([item]);
  expect(root.querySelector("div.g.x > div")).toBe(item);
  expect(root.querySelector("[cmdk-root] > [cmdk-item]")).toBe(root.children[1]); // not the nested one
  expect(root.querySelector("[cmdk-group] + [cmdk-item]")).toBe(root.children[1]);
  expect(root.querySelector("[cmdk-group] ~ span")?.type).toBe("span");
  expect(root.querySelector("[cmdk-group]>*")).toBe(item);
  expect(root.querySelector("[data\\:missing] > *")).toBeNull();
  expect(root.querySelector('span[data-value="a b"], #nope')?.type).toBe("span");
  expect(item.matches("#one")).toBe(true);
  expect(item.closest("[cmdk-root]")).toBe(root);
});

test("capture handlers run first, node listeners work, and document hears presses", async () => {
  const order = [];
  let inner;
  const doc = new EventTarget();
  globalThis.document = doc;
  doc.addEventListener("pointerdown", (e) => order.push(`document ${e.target.props?.id}`));
  const { r, sent } = setup(
    h(
      "div",
      { onPointerDownCapture: () => order.push("outer capture"), onPointerDown: () => order.push("outer") },
      h("div", { id: "in", ref: (n) => (inner = n), onPointerDown: () => order.push("inner") }),
    ),
  );
  await tick();
  expect(sent[0].ops.find((op) => op.el)?.el[2].events).toContain("mousedown");
  inner.addEventListener("custom", (e) => order.push(`custom ${e.detail}`), { once: true });
  r.dispatch({ t: "event", type: "mousedown", id: inner.hid });
  inner.dispatchEvent(new CustomEvent("custom", { detail: 1 }));
  inner.dispatchEvent(new CustomEvent("custom", { detail: 2 }));
  // A press no element reported reaches document with <body> as its target.
  r.dispatch({ t: "event", type: "document", events: ["pointerdown"], id: 0 });
  expect(order).toEqual(["outer capture", "inner", "outer", "document in", "custom 1", "document undefined"]);
  delete globalThis.document;
});

test("page coordinates follow the scrolled page, as <body>'s rect reports it", async () => {
  let box;
  const { r } = setup(h("div", { ref: (n) => (box = n) }));
  await tick();
  const seen = [];
  box.addEventListener("mousemove", (e) => seen.push([e.clientY, e.pageY]));
  r.dispatch({ t: "event", type: "resize", id: 1, rect: { x: 0, y: -500, width: 800, height: 3000 } });
  r.dispatch({ t: "event", type: "mousemove", id: box.hid, clientX: 10, clientY: 40 });
  expect(seen).toEqual([[40, 540]]);
});

test("innerHTML on a React node clears what a library drew there, keeping React's children", async () => {
  let box;
  const { r } = setup(h("div", { ref: (n) => (box = n) }, h("span")));
  await tick();
  box.appendChild(r.createElement("svg", true));
  box.innerHTML = "";
  expect(box.childNodes.map((n) => n.nodeName)).toEqual(["SPAN"]);
});

test("nodes under a removed one are forgotten: writes to them after send nothing", async () => {
  let box;
  function App({ show }) {
    return show ? h("section", null, h("div", { ref: (n) => (box ??= n) })) : null;
  }
  const { r, sent } = setup(h(App, { show: true }));
  await tick();
  // A library's own node inside React's subtree.
  const drawn = r.createElement("span");
  box.appendChild(drawn);
  await tick();
  r.render(h(App, { show: false }));
  await tick();
  sent.length = 0;
  drawn.textContent = "late";
  drawn.style.color = "red";
  drawn.appendChild(r.createElement("i"));
  box.removeChild(drawn);
  await tick();
  const named = new Set(sent.flatMap((m) => m.ops ?? []).flatMap((op) => Object.values(op)[0]).filter((v) => typeof v === "number"));
  expect(named.has(drawn.hid) || named.has(box.hid)).toBe(false);
});

test("a press no element reported targets the innermost box the host saw under the pointer", async () => {
  let menu, item;
  const { r } = setup(h("div", { ref: (n) => (menu = n) }, h("span", { ref: (n) => (item = n) }, "Language")));
  await tick();
  const targets = [];
  globalThis.document = { dispatchCapture() {}, dispatchEvent: (e) => targets.push(e.target) };
  r.dispatch({ t: "event", type: "document", events: ["mousedown"], id: 0, target: item.hid });
  delete globalThis.document;
  // A click-outside check (`menu.contains(event.target)`) sees the press inside the menu.
  expect(targets[0]).toBe(item);
  expect(menu.contains(targets[0])).toBe(true);
});

test("onMouseMove hears every move with coordinates, and its element is measured", async () => {
  const seen = [];
  let chart;
  const { r, sent } = setup(h("div", { ref: (n) => (chart = n), onMouseMove: (e) => seen.push(e.clientX) }));
  await tick();
  expect(sent[0].ops.find((op) => op.el)?.el[2]).toMatchObject({ events: ["mousemove"], measure: true });
  r.dispatch({ t: "event", type: "mousemove", id: chart.hid, clientX: 3 });
  r.dispatch({ t: "event", type: "mousemove", id: chart.hid, clientX: 7 });
  expect(seen).toEqual([3, 7]);
});

test("DOM listeners on elements get pointer events with coordinates, and make the host report them", async () => {
  const seen = [];
  let outer, inner;
  const { r, sent } = setup(h("div", { ref: (n) => (outer = n) }, h("span", { ref: (n) => (inner = n) })));
  await tick();
  inner.addEventListener("pointerleave", (e) => seen.push(`leave ${e.clientX},${e.clientY} ${e.currentTarget === inner}`));
  outer.addEventListener("pointerout", (e) => seen.push(`out@outer target=${e.target === inner}`));
  outer.addEventListener("pointerleave", () => seen.push("leave@outer (must not bubble)"));
  const wired = (id) => sent.flatMap((m) => m.ops).filter((op) => op.set?.[0] === id).at(-1).set[1].events;
  expect(wired(inner.hid)).toEqual(["mouseleave"]);
  expect(wired(outer.hid)).toEqual(["mouseleave"]);
  r.dispatch({ t: "event", type: "mouseleave", id: inner.hid, clientX: 12, clientY: 34 });
  expect(seen).toEqual(["out@outer target=true", "leave 12,34 true"]);
});

test("what drag libraries use: lazy rects, dataset, abortable listeners, pressed buttons and window", async () => {
  let group, handle;
  const { r, sent } = setup(
    h("div", { ref: (n) => (group = n), "data-panel-group-id": "g" }, h("div", { ref: (n) => (handle = n) })),
  );
  await tick();
  expect(group.dataset).toEqual({ panelGroupId: "g" });

  // A first read is zero and asks the host to measure the node and its ancestors.
  expect(handle.getBoundingClientRect().width).toBe(0);
  const measured = sent.at(-1).ops.filter((op) => op.set?.[1].measure).map((op) => op.set[0]);
  expect(measured).toEqual(expect.arrayContaining([handle.hid, group.hid]));
  r.dispatch({ t: "event", type: "resize", id: group.hid, rect: { x: 0, y: 0, width: 300, height: 40 } });
  expect(group.getBoundingClientRect().width).toBe(300);

  const seen = [];
  const ctl = new AbortController();
  r.body.addEventListener("pointermove", (e) => seen.push(`move ${e.buttons} ${e.isPrimary}`), { signal: ctl.signal });
  const win = new EventTarget();
  const dispatchEvent = globalThis.dispatchEvent;
  globalThis.dispatchEvent = win.dispatchEvent.bind(win);
  win.addEventListener("pointerup", (e) => seen.push(`window up ${e.buttons}`));
  r.dispatch({ t: "event", type: "mousedown", id: handle.hid });
  r.dispatch({ t: "event", type: "document", events: ["pointermove"], id: 0 });
  r.dispatch({ t: "event", type: "mouseup", id: handle.hid });
  ctl.abort();
  r.dispatch({ t: "event", type: "document", events: ["pointermove"], id: 0 });
  globalThis.dispatchEvent = dispatchEvent;
  expect(seen).toEqual(["move 1 true", "window up 0"]);
});

test("remote image sources are downloaded and handed to the host as local files", async () => {
  const png = Buffer.from("89504e470d0a1a0a", "hex");
  const server = Bun.serve({ port: 0, fetch: (req) => (req.url.endsWith("/missing") ? new Response("no", { status: 404 }) : new Response(png)) });
  const url = `http://localhost:${server.port}/portrait.png?${Math.random()}`;
  try {
    const { sent } = setup(h("img", { src: url }));
    await tick();
    const first = sent[0].ops.find((op) => op.el).el[2];
    expect(first.src).toBe(url);
    expect(await fetchAsset(url)).not.toBeNull();
    await tick();
    const local = sent.at(-1).ops.find((op) => op.set).set[1].src;
    expect(local).not.toBe(url);
    expect(readFileSync(local)).toEqual(png);
    expect(assetPath(url)).toBe(local);
    expect(await fetchAsset(`http://localhost:${server.port}/missing`)).toBeNull();
  } finally {
    server.stop(true);
  }
});

test("imperative style writes reach the host over React's style, and offsets come from rects", async () => {
  let outer, inner;
  const { r, sent } = setup(
    h("div", { ref: (n) => (outer = n), style: { position: "fixed", left: 1 } }, h("div", { ref: (n) => (inner = n) })),
  );
  await tick();
  outer.style.left = "12px";
  outer.style.setProperty("min-width", "40px");
  expect(outer.style.minWidth).toBe("40px");
  await tick();
  const set = sent.flatMap((m) => m.ops ?? []).findLast((op) => op.set?.[0] === outer.hid);
  expect(set.set[1].style).toBe("position:fixed;left:12px;min-width:40px");
  r.dispatch({ t: "event", type: "resize", id: outer.hid, rect: { x: 10, y: 20, width: 100, height: 50 } });
  r.dispatch({ t: "event", type: "resize", id: inner.hid, rect: { x: 14, y: 26, width: 50, height: 10 } });
  expect([inner.offsetTop, inner.offsetLeft, inner.offsetParent]).toEqual([6, 4, outer]);
});

test("document capture listeners run before React and can cancel its event", async () => {
  const order = [];
  const capture = new EventTarget();
  const bubble = new EventTarget();
  capture.addEventListener("pointerup", (e) => (order.push("capture"), e.preventDefault()));
  bubble.addEventListener("pointerup", (e) => order.push(`bubble ${e.defaultPrevented}`));
  globalThis.document = { dispatchCapture: (e) => capture.dispatchEvent(e), dispatchBubble: (e) => bubble.dispatchEvent(e) };
  let item;
  const { r } = setup(h("div", { ref: (n) => (item = n), onPointerUp: (e) => order.push(`react ${e.defaultPrevented}`) }));
  await tick();
  r.dispatch({ t: "event", type: "mouseup", id: item.hid });
  expect(order).toEqual(["capture", "react true", "bubble true"]);
  delete globalThis.document;
});

test("a document listener finds the pressed element first in composedPath()", async () => {
  const doc = new EventTarget();
  let path;
  doc.addEventListener("pointerdown", (e) => (path = e.composedPath()));
  globalThis.document = { dispatchCapture: () => {}, dispatchBubble: (e) => doc.dispatchEvent(e) };
  let outer, inner;
  const { r } = setup(h("div", { ref: (n) => (outer = n) }, h("span", { ref: (n) => (inner = n) })));
  await tick();
  r.dispatch({ t: "event", type: "mousedown", id: inner.hid });
  expect(path.slice(0, 2)).toEqual([inner, outer]);
  expect(inner.closest(undefined)).toBeNull();
  expect(outer.querySelector("span:last-child")).toBe(inner);
  expect(outer.querySelector("span:first-child:last-child")).toBe(inner);
  expect(outer.querySelector(":only-child:not(span)")).toBeNull();
  delete globalThis.document;
});

test("style.cssText reads and replaces the imperative style, as vaul saves and restores it", async () => {
  let el;
  const { sent } = setup(h("div", { ref: (n) => (el = n) }));
  await tick();
  const saved = el.style.cssText;
  Object.assign(el.style, { transform: "translate3d(10px, 0, 0)", transition: "none" });
  expect(el.style.cssText).toBe("transform:translate3d(10px, 0, 0);transition:none");
  await tick();
  const last = () => sent.flatMap((m) => m.ops ?? []).findLast((op) => op.set?.[0] === el.hid).set[1];
  expect(last().style).toBe("transform:translate3d(10px, 0, 0);transition:none");
  el.style.cssText = saved;
  expect(el.style.transform).toBe("");
  await tick();
  expect(last().style).toBeUndefined();
});

test("classList edits apply over className until React sets a new one", async () => {
  let el;
  const { r, sent } = setup(h("div", { ref: (n) => (el = n), className: "a b" }));
  await tick();
  el.classList.add("vaul-dragging");
  el.classList.remove("a");
  expect([el.classList.contains("vaul-dragging"), el.classList.contains("a")]).toEqual([true, false]);
  await tick();
  const last = () => sent.flatMap((m) => m.ops ?? []).findLast((op) => op.set?.[0] === el.hid).set[1];
  expect(last().class).toBe("b vaul-dragging");
  r.render(h("div", { ref: (n) => (el = n), className: "c" }));
  await tick();
  expect(last().class).toBe("c");
});

test("React handlers bubble out of a portal to the component that rendered it", async () => {
  const seen = [];
  let item;
  const sent = [];
  const r = createRenderer((msg) => sent.push(msg));
  // What react-dom's createPortal returns, mounted into <body>.
  const portal = (children) => ({ $$typeof: Symbol.for("react.portal"), key: null, children, containerInfo: r.body, implementation: null });
  r.render(
    h(
      "div",
      { onPointerUp: () => seen.push("drawer"), onPointerUpCapture: () => seen.push("drawer capture") },
      h("main", null, portal(h("div", { ref: (n) => (item = n), onPointerUp: () => seen.push("item") }))),
    ),
  );
  await tick();
  expect(item.parent).toBe(r.body);
  r.dispatch({ t: "event", type: "mouseup", id: item.hid });
  expect(seen).toEqual(["drawer capture", "item", "drawer"]);
});

test("pointer capture sends moves and the release to the capturing node until the release", async () => {
  const seen = [];
  let handle;
  const { r } = setup(
    h(
      "div",
      { onPointerMove: () => seen.push("drawer move"), onPointerUp: () => seen.push("drawer up") },
      h("button", { ref: (n) => (handle = n), onPointerDown: (e) => e.currentTarget.setPointerCapture(1) }),
    ),
  );
  await tick();
  r.dispatch({ t: "event", type: "mousedown", id: handle.hid });
  expect(handle.hasPointerCapture(1)).toBe(true);
  // Moves and the release elsewhere (reported to `document`) reach the capturing node.
  r.dispatch({ t: "event", type: "document", events: ["pointermove", "mousemove"], id: 0 });
  r.dispatch({ t: "event", type: "document", events: ["pointerup", "mouseup", "click"], id: 0 });
  expect(seen).toEqual(["drawer move", "drawer up"]);
  expect(handle.hasPointerCapture(1)).toBe(false);
  r.dispatch({ t: "event", type: "document", events: ["pointermove"], id: 0 });
  expect(seen).toHaveLength(2);
});

test("updates inside a transition commit (react-router navigates in one)", async () => {
  let set;
  function App() {
    const [page, setPage] = useState("home");
    set = setPage;
    return h("span", null, page);
  }
  const { sent } = setup(h(App));
  await tick();
  startTransition(() => set("calendar"));
  await tick();
  const text = sent[0].ops.find((op) => op.txt?.[1] === "home").txt[0];
  expect(sent.at(-1).ops).toContainEqual({ settext: [text, "calendar"] });
});

test("a React input moves into a library's wrapper and back out, keeping its tag", async () => {
  let box, input;
  const { r, sent } = setup(h("div", { ref: (n) => (box = n) }, h("input", { ref: (n) => (input = n) })));
  await tick();
  input.type = "text";
  const wrapper = r.createElement("div");
  box.insertBefore(wrapper, input);
  wrapper.appendChild(input);
  await tick();
  expect(input.parentNode).toBe(wrapper);
  expect(input.nodeName).toBe("INPUT");
  expect(sent.flatMap((m) => m.ops ?? [])).toContainEqual({ append: [wrapper.hid, input.hid] });
  box.insertBefore(input, wrapper);
  box.removeChild(wrapper);
  expect(box.childNodes).toEqual([input]);
});

test("replaceChildren swaps a library node's children for the given ones", () => {
  const { r } = setup(h("div"));
  const svg = r.createElement("svg", true);
  const [a, b, c] = ["text", "text", "text"].map((t) => r.createElement(t, true));
  svg.appendChild(a);
  svg.replaceChildren(b, c);
  expect(svg.children).toEqual([b, c]);
  expect(a.parentNode).toBeNull();
});

test("svg elements keep their tag's case in nodeName, html ones are uppercased", () => {
  const { r } = setup(h("div"));
  expect(r.createElement("feColorMatrix", true).nodeName).toBe("feColorMatrix");
  expect(r.createElement("svg", true).nodeName).toBe("svg");
  expect(r.createElement("div").nodeName).toBe("DIV");
});

test("library-built DOM reaches the host once attached, and detaches without being freed", async () => {
  let box;
  const { r, sent } = setup(h("div", { ref: (n) => (box = n) }));
  await tick();
  const svg = r.createElement("svg", true);
  const text = r.createElement("text", true);
  text.setAttribute("font-size", "10px");
  text.setAttribute("stroke-width", "2");
  text.textContent = "abc";
  svg.appendChild(text);
  const frag = r.createDocumentFragment();
  frag.appendChild(svg);
  box.appendChild(frag);
  await tick();
  const ops = sent.flatMap((m) => m.ops ?? []);
  expect(ops).toContainEqual({ append: [box.hid, svg.hid] });
  expect(ops.findLast((op) => op.set?.[0] === text.hid).set[1].attrs).toEqual({ "font-size": "10px", "stroke-width": "2" });
  expect(box.querySelector("text")).toBe(text);
  expect(text.getBBox().width).toBeGreaterThan(0);
  box.removeChild(svg);
  await tick();
  expect(sent.flatMap((m) => m.ops ?? [])).toContainEqual({ detach: [box.hid, svg.hid] });
  box.innerHTML = "<b>x</b>"; // React's node: left to React
  expect(box.childNodes).toEqual([]);
  svg.innerHTML = '<g class="a"><rect width="4" height="3"/></g>';
  expect(svg.getBBox()).toMatchObject({ width: 4, height: 3 });
  expect(svg.firstChild.className).toBe("a");
  svg.firstChild.classList.add("b"); // ApexCharts tags its nodes this way, then queries them
  expect(svg.querySelector(".a.b")).toBe(svg.firstChild);
  svg.firstChild.setAttribute("data:realIndex", "0"); // ApexCharts' attribute names, escaped in queries
  expect(svg.querySelector(".a[data\\:realIndex='0'] > rect")).toBe(svg.firstChild.firstChild);
  svg.dataset.tipPlacement = "left";
  expect([svg.getAttribute("data-tip-placement"), svg.dataset.tipPlacement]).toEqual(["left", "left"]);
  delete svg.dataset.tipPlacement;
  expect(svg.hasAttribute("data-tip-placement")).toBe(false);
  svg.id = "chart"; // jsVectorMap names its groups this way
  expect([svg.id, svg.getAttribute("id")]).toEqual(["chart", "chart"]);
});

test("pointer events over a library-built <svg> reach the element under the pointer", async () => {
  let box;
  const { r, sent } = setup(h("div", { ref: (n) => (box = n) }));
  await tick();
  const svg = r.createElement("svg", true);
  svg.innerHTML = '<g transform="translate(10,0)"><rect class="bar" width="10" height="10"/></g><rect class="dot" x="40" width="5" height="5"/>';
  box.appendChild(svg);
  const seen = [];
  // jsVectorMap delegates from its container: `e.target.matches(".jvm-element")`.
  box.addEventListener("mouseover", (e) => seen.push(`over ${e.target.getAttribute("class")}`));
  box.addEventListener("mouseout", (e) => seen.push(`out ${e.target.getAttribute("class")}`));
  box.addEventListener("mousemove", (e) => seen.push(`move ${e.target.type}`));
  await tick();
  const wired = (id) => sent.flatMap((m) => m.ops).filter((op) => op.set?.[0] === id).at(-1).set[1];
  expect(wired(box.hid).events).toContain("mousemove");
  expect(wired(svg.hid).measure).toBe(true);
  r.dispatch({ t: "event", type: "resize", id: svg.hid, rect: { x: 100, y: 100, width: 50, height: 20 } });
  const move = (x, y) => r.dispatch({ t: "event", type: "mousemove", id: box.hid, clientX: x, clientY: y });
  move(115, 105);
  move(116, 106);
  move(142, 102);
  expect(seen).toEqual(["over bar", "move rect", "move rect", "out bar", "over dot", "move rect"]);
  // The host hears which shape is hovered, for its `:hover` rules.
  const hovers = sent.flatMap((m) => m.ops ?? []).filter((op) => "hover" in op).map((op) => op.hover);
  expect(hovers).toEqual([svg.querySelector(".bar").hid, svg.querySelector(".dot").hid]);
  expect(svg.querySelector(".bar").getBoundingClientRect()).toMatchObject({ x: 110, y: 100, width: 10 });
});

test("the pointer over a container measures an <svg> nothing measured yet", async () => {
  // MUI X Charts listens on the chart's container div; its pie slices use onPointerEnter.
  let box, svg;
  const entered = [];
  const { r, sent } = setup(
    h("div", { ref: (n) => (box = n), onMouseMove() {} }, h("svg", { ref: (n) => (svg = n), viewBox: "0 0 50 20" }, h("rect", { width: 10, height: 10, onPointerEnter: () => entered.push("slice") }))),
  );
  await tick();
  const wired = (id) => sent.flatMap((m) => m.ops).filter((op) => op.set?.[0] === id || op.el?.[0] === id).at(-1);
  expect((wired(svg.hid).set ?? wired(svg.hid).el)[1].measure).toBeUndefined();
  const move = () => r.dispatch({ t: "event", type: "mousemove", id: box.hid, clientX: 105, clientY: 105 });
  move();
  await tick();
  expect(wired(svg.hid).set[1].measure).toBe(true);
  r.dispatch({ t: "event", type: "resize", id: svg.hid, rect: { x: 100, y: 100, width: 50, height: 20 } });
  move();
  expect(entered).toEqual(["slice"]);
});

test("a press bubbles from the innermost box under the pointer, not the listener", async () => {
  // FullCalendar delegates event clicks: one container listener, `ev.target.closest(".fc-event")`.
  let calendar, title;
  const { r } = setup(
    h("div", { ref: (n) => (calendar = n) }, h("div", { className: "fc-event" }, h("span", { ref: (n) => (title = n) }, "Meeting"))),
  );
  await tick();
  const hits = [];
  calendar.addEventListener("click", (e) => hits.push(e.target.closest(".fc-event")));
  r.dispatch({ t: "event", type: "click", id: calendar.hid, target: title.hid });
  expect(hits).toEqual([title.parent]);
});

test("clicking a label checks its radio, and a checkbox toggles itself", async () => {
  let level, checked;
  function Form() {
    const [l, setL] = useState("Primary");
    const [c, setC] = useState(false);
    [level, checked] = [l, c];
    return h(
      "form",
      null,
      ["Danger", "Primary"].map((key) =>
        h(
          "label",
          { key, htmlFor: `modal${key}` },
          h("span", null, h("input", { id: `modal${key}`, type: "radio", name: "level", className: "sr-only", checked: l === key, onChange: () => setL(key) }), h("span", { className: "box" })),
          key,
        ),
      ),
      h("label", null, h("input", { type: "checkbox", checked: c, onChange: (e) => setC(e.target.checked) })),
    );
  }
  const { r } = setup(h(Form));
  await tick();
  const labels = r.body.kids[0].kids;
  const box = labels[0].kids[0].kids[1];
  const radio = labels[0].kids[0].kids[0];
  const checkbox = labels[2].kids[0];
  // A press on the dot no element listens to reaches `document`, from the dot.
  r.dispatch({ t: "event", type: "document", events: ["pointerup", "mouseup", "click"], id: 0, target: box.hid });
  await tick();
  expect(level).toBe("Danger");
  expect(radio.checked).toBe(true);
  expect(radio.wired.attrs.checked).toBe("");
  expect(labels[1].kids[0].kids[0].checked).toBe(false);
  r.dispatch({ t: "event", type: "document", events: ["click"], id: 0, target: checkbox.hid });
  await tick();
  expect(checked).toBe(true);
  r.dispatch({ t: "event", type: "document", events: ["click"], id: 0, target: checkbox.hid });
  await tick();
  expect(checked).toBe(false);
  // A checked radio stays checked.
  r.dispatch({ t: "event", type: "document", events: ["click"], id: 0, target: radio.hid });
  await tick();
  expect(level).toBe("Danger");
});

test("relative image URLs resolve against the page's URL into public/, as a browser does", () => {
  const pub = process.env.GPUIJS_PUBLIC;
  process.env.GPUIJS_PUBLIC = mkdtempSync(join(tmpdir(), "gpuijs-public-"));
  mkdirSync(`${process.env.GPUIJS_PUBLIC}/images/user`, { recursive: true });
  writeFileSync(`${process.env.GPUIJS_PUBLIC}/images/user/owner.png`, "");
  const location = globalThis.location;
  globalThis.location = new URL("http://localhost/profile");
  try {
    const owner = `${process.env.GPUIJS_PUBLIC}/images/user/owner.png`;
    expect(assetPath("./images/user/owner.png")).toBe(owner);
    expect(assetPath("/images/user/owner.png")).toBe(owner);
    expect(assetPath("./missing.png")).toBe("./missing.png");
  } finally {
    globalThis.location = location;
    rmSync(process.env.GPUIJS_PUBLIC, { recursive: true });
    if (pub === undefined) delete process.env.GPUIJS_PUBLIC;
    else process.env.GPUIJS_PUBLIC = pub;
  }
});

test("requestAnimationFrame runs when the host starts a frame, and pauses while the window is hidden", async () => {
  const { r, sent } = setup(h("div"));
  await tick();
  const seen = [];
  const asked = () => sent.filter((m) => m.t === "frame").length;
  const frame = () => r.dispatch({ t: "event", type: "frame", id: 0 });
  globalThis.document = { dispatchEvent: (e) => seen.push(`${e.type} ${r.visibilityState()}`) };
  try {
    const times = [];
    r.requestAnimationFrame((t) => times.push(t));
    r.requestAnimationFrame((t) => times.push(t));
    r.cancelAnimationFrame(r.requestAnimationFrame(() => times.push("cancelled")));
    // One request to the host for all of them; they wait for its frame.
    expect(asked()).toBe(1);
    await tick();
    expect(times).toHaveLength(0);
    frame();
    // One frame for every callback, at one timestamp.
    expect(times).toHaveLength(2);
    expect(times[0]).toBe(times[1]);

    r.dispatch({ t: "event", type: "visibilitychange", id: 0, hidden: true });
    expect(r.visibilityState()).toBe("hidden");
    r.requestAnimationFrame(() => times.push("after"));
    expect(asked()).toBe(1);
    frame();
    expect(times).toHaveLength(2);

    r.dispatch({ t: "event", type: "visibilitychange", id: 0, hidden: false });
    expect(asked()).toBe(2);
    frame();
    expect(times.at(-1)).toBe("after");
    expect(seen).toEqual(["visibilitychange hidden", "visibilitychange visible"]);
  } finally {
    delete globalThis.document;
  }
});

test("a benchmark driver finds elements and hears when an input has been handled", async () => {
  const sent = [];
  const r = createRenderer((msg) => sent.push(msg));
  r.render(h("div", null, h("a", { href: "/calendar", style: { width: 40 } }, "Calendar")));
  await tick();
  r.dispatch({ t: "event", type: "benchQuery", id: 0, req: 1, selector: "a[href='/calendar']" });
  r.dispatch({ t: "event", type: "benchQuery", id: 0, req: 2, selector: ".missing" });
  r.dispatch({ t: "event", type: "benchSettle", id: 0, req: 3 });
  const replies = () => sent.filter((m) => m.t === "bench");
  expect(replies().map((m) => m.req)).toEqual([1, 2]);
  expect(replies()[0].rect).toMatchObject({ x: expect.any(Number), width: expect.any(Number) });
  expect(replies()[1].rect).toBeNull();
  // Answered after the event's work, on a later task.
  await tick();
  expect(replies().at(-1)).toEqual({ t: "bench", req: 3 });
});

test("a library writing to an element React deleted sends the host nothing", async () => {
  let el, hide;
  function App() {
    const [shown, setShown] = useState(true);
    hide = () => setShown(false);
    return shown ? h("div", { ref: (n) => n && (el = n) }) : null;
  }
  const { sent } = setup(h(App));
  await tick();
  hide();
  await tick();
  const before = sent.length;
  // FullCalendar resizing its grid after the calendar unmounted.
  el.style.height = "10px";
  el.setAttribute("data-x", "1");
  await tick();
  expect(sent.slice(before).flatMap((m) => m.ops ?? [])).toEqual([]);
});

test("compareDocumentPosition places a node before, after, inside or around another", async () => {
  let root, a, b, inner;
  setup(h("div", { ref: (n) => (root = n) }, h("div", { ref: (n) => (a = n) }, h("span", { ref: (n) => (inner = n) })), h("div", { ref: (n) => (b = n) })));
  await tick();
  expect(a.compareDocumentPosition(b)).toBe(4);
  expect(b.compareDocumentPosition(a)).toBe(2);
  expect(inner.compareDocumentPosition(b)).toBe(4);
  expect(a.compareDocumentPosition(inner)).toBe(16 | 4);
  expect(inner.compareDocumentPosition(root)).toBe(8 | 2);
  expect(a.compareDocumentPosition(a)).toBe(0);
});

test("getAttribute reads React's camelCased tabIndex as the tabindex attribute", async () => {
  let el;
  setup(h("li", { tabIndex: -1, ref: (n) => (el = n) }));
  await tick();
  // MUI's Select ignores clicks on items without one.
  expect(el.hasAttribute("tabindex")).toBe(true);
  expect(el.getAttribute("tabindex")).toBe("-1");
});

test("a window key reaches the focused element's handlers, then document", async () => {
  const heard = [];
  const doc = new EventTarget();
  globalThis.document = doc;
  doc.addEventListener("keydown", (e) => heard.push(`document ${e.key} ${e.target?.props?.id}`));
  let menu;
  const { r } = setup(
    h("div", { onKeyDown: (e) => heard.push(`outer ${e.key} ${e.shiftKey}`) },
      h("ul", { id: "menu", tabIndex: -1, ref: (n) => (menu = n), onKeyDown: (e) => heard.push(`menu ${e.key}`) })),
  );
  await tick();
  r.dispatch({ t: "event", type: "keydown", id: 0, key: "a" });
  menu.focus();
  r.dispatch({ t: "event", type: "keydown", id: 0, key: "Escape", shiftKey: true });
  expect(heard).toEqual(["document a undefined", "menu Escape", "outer Escape true", "document Escape menu"]);
  delete globalThis.document;
});

test("a style written by its CSS name reads back by its property name", async () => {
  let el;
  setup(h("div", { ref: (n) => (el = n) }));
  await tick();
  el.style["font-size"] = "12px";
  expect(el.style.fontSize).toBe("12px");
  expect(el.style.getPropertyValue("font-size")).toBe("12px");
  expect(el.style.cssText).toContain("font-size");
});

test("an attribute a script wrote on a React node stays until React renders a new value for it", async () => {
  let rect, render;
  function App() {
    const [height, setHeight] = useState(0);
    const [n, setN] = useState(0);
    render = { h: setHeight, other: () => setN((x) => x + 1) };
    return h("svg", null, h("rect", { ref: (r) => r && (rect = r), height, "data-n": n }));
  }
  setup(h(App));
  await tick();
  rect.setAttribute("height", "50"); // MUI X Charts' bar animation
  expect(rect.props.height).toBe("50");
  render.other();
  await tick();
  expect(rect.props.height).toBe("50");
  render.h(10);
  await tick();
  expect(rect.props.height).toBe(10);
});

test("emptying a style property clears React's inline value for it too", async () => {
  let el;
  const { sent } = setup(h("div", { ref: (n) => (el = n), style: { position: "fixed", left: 0, top: 0 } }));
  await tick();
  el.style.left = ""; // Popper.js placing a tooltip by `right` instead
  el.style.right = "0";
  await tick();
  const style = sent.flatMap((m) => m.ops ?? []).filter((op) => op.set).at(-1).set[1].style;
  expect(style).toContain("right:0");
  expect(style).not.toContain("left");
});

test("<html> keeps its class and attributes apart, and hears the system appearance", async () => {
  const { r, sent } = setup(h("div"));
  await tick();
  sent.length = 0;
  r.setRootClass("dark");
  r.setRootAttribute("data-mui-color-scheme", "dark");
  const html = () => sent.flatMap((m) => m.ops ?? []).filter((op) => op.set?.[0] === 0).at(-1).set[1];
  expect(html()).toMatchObject({ class: "dark", attrs: { "data-mui-color-scheme": "dark" } });
  r.setRootAttribute("data-mui-color-scheme", null);
  expect(html().class).toBe("dark");
  expect(html().attrs?.["data-mui-color-scheme"]).toBeUndefined();

  expect(r.prefersDark()).toBe(false);
  let heard = 0;
  globalThis.addEventListener?.("appearance", () => heard++);
  r.dispatch({ t: "event", id: 0, type: "appearance", dark: true });
  expect(r.prefersDark()).toBe(true);
  if (globalThis.addEventListener) expect(heard).toBe(1);
});

test("el.animate() finishes at once, leaving a forwards fill's end state; toggleAttribute", async () => {
  const { r } = setup(h("div", { id: "palette" }));
  await tick();
  const el = r.body.kids[0];
  let finished = false;
  const a = el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, fill: "forwards" });
  a.onfinish = () => (finished = true);
  expect(await a.finished).toBe(a);
  expect(finished).toBe(true);
  expect(el.style.opacity).toBe("1");
  el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, direction: "reverse", fill: "forwards" });
  expect(el.style.opacity).toBe("0");

  expect(el.toggleAttribute("data-autoscrolling")).toBe(true);
  expect(el.getAttribute("data-autoscrolling")).toBe("");
  expect(el.toggleAttribute("data-autoscrolling", true)).toBe(true);
  expect(el.toggleAttribute("data-autoscrolling")).toBe(false);
  expect(el.hasAttribute("data-autoscrolling")).toBe(false);
});

// Vue and Solid build their DOM themselves: comments anchor what they insert, and they listen with
// addEventListener (often on `document`).
const opsOf = (sent) => sent.flatMap((m) => m.ops ?? []);

test("a comment anchor stays in JS; what goes before it lands before the next node the host has", async () => {
  let box;
  const { r, sent } = setup(h("div", { ref: (n) => (box = n) }));
  await tick();
  const [a, anchor, b] = [r.createElement("i"), r.createComment("if"), r.createElement("b")];
  for (const n of [a, anchor, b]) box.appendChild(n);
  await tick();
  sent.length = 0;
  const x = r.createElement("u");
  box.insertBefore(x, anchor);
  box.removeChild(b);
  const y = r.createElement("s");
  box.insertBefore(y, anchor);
  await tick();
  const ops = opsOf(sent);
  expect(ops.some((op) => Object.values(op)[0]?.includes?.(anchor.hid))).toBe(false);
  expect(ops.filter((op) => op.insert || op.append)).toEqual([{ insert: [box.hid, x.hid, b.hid] }, { append: [box.hid, y.hid] }]);
  expect(box.childNodes.map((n) => n.nodeName)).toEqual(["I", "U", "S", "#comment"]);
  expect(anchor.nextSibling).toBeNull();
  expect(box.textContent).toBe("");
});

test("a <template> holds its markup in `content`, exactly, to clone from", () => {
  const r = createRenderer(() => {});
  const t = r.createElement("template");
  t.innerHTML = "<p><b>a</b> <!></p>";
  expect(t.childNodes).toEqual([]);
  const p = t.content.firstChild.cloneNode(true);
  expect(p.childNodes.map((n) => n.nodeName)).toEqual(["B", "#text", "#comment"]);
  expect(p.firstChild.nextSibling.data).toBe(" ");
});

test("DOM listeners hear typing, focus, keys and toggles; boolean props read back", async () => {
  const doc = new EventTarget();
  globalThis.document = { dispatchCapture: () => {}, dispatchBubble: (e) => doc.dispatchEvent(e) };
  let box;
  const { r } = setup(h("div", { ref: (n) => (box = n) }));
  await tick();
  const [field, check] = [r.createElement("input"), r.createElement("input")];
  check.type = "checkbox";
  box.appendChild(field);
  box.appendChild(check);
  const heard = [];
  field.addEventListener("input", (e) => heard.push(`input:${e.target.value}`));
  check.addEventListener("change", () => heard.push(`change:${check.checked}`));
  for (const type of ["focusin", "keydown", "keyup"]) doc.addEventListener(type, (e) => heard.push(`${type}:${e.key ?? ""}`));
  // Solid's delegated handler retargets the event as it walks up.
  doc.addEventListener("keydown", (e) => Object.defineProperty(e, "target", { configurable: true, value: box }));
  r.dispatch({ t: "event", type: "focus", id: field.hid });
  r.dispatch({ t: "event", type: "change", id: field.hid, value: "hi", seq: 1 });
  r.dispatch({ t: "event", type: "keydown", id: field.hid, key: "Enter" });
  r.dispatch({ t: "event", type: "click", id: check.hid });
  expect(heard).toEqual(["focusin:", "input:hi", "keydown:Enter", "keyup:Enter", "change:true"]);
  check.disabled = true;
  expect([check.checked, check.disabled, field.disabled]).toEqual([true, true, false]);
  delete globalThis.document;
});

test("double-clicks, wheels, key-ups and scrolls reach React; scrollTo tells the host", async () => {
  const seen = [];
  const { r, sent } = setup(
    h("div", {
      onDoubleClick: () => seen.push("dbl"),
      onWheel: (e) => seen.push(`wheel ${e.type}`),
      onKeyUp: (e) => seen.push(`up ${e.key}`),
      onScroll: () => seen.push("scroll"),
    }),
  );
  await tick();
  const [id, , wired] = sent[0].ops.find((op) => op.el?.[1] === "div").el;
  expect(wired.events.sort()).toEqual(["dblclick", "keydown", "scroll", "wheel"]);
  for (const msg of [{ type: "dblclick" }, { type: "wheel", deltaY: 3 }, { type: "keydown", key: "a" }, { type: "scroll", scrollLeft: 0, scrollTop: 0, init: true }, { type: "scroll", scrollLeft: 0, scrollTop: 40 }])
    r.dispatch({ t: "event", id, ...msg });
  expect(seen).toEqual(["dbl", "wheel wheel", "up a", "scroll"]);
  const node = r.body.kids[0];
  expect(node.scrollTop).toBe(40);
  r.dispatch({ t: "event", type: "resize", id, rect: { x: 0, y: 0, width: 100, height: 50 } });
  node.measured = true;
  node.scrollHeight; // no children: nothing to scroll to
  sent.length = 0;
  node.scrollTop = 500;
  await tick();
  expect(node.scrollTop).toBe(0);
  expect(sent.flatMap((m) => m.ops ?? [])).toEqual([{ scroll: [id, 0, 0] }]);
});

test("Solid's delegated handlers (`el.$$click`) make the host report the element's events", async () => {
  const { r, sent } = setup(h("div"));
  await tick();
  const button = r.createElement("button");
  button.type = "button";
  r.body.appendChild(button);
  button.$$click = () => {};
  await tick();
  const last = opsOf(sent).filter((op) => (op.el ?? op.set)?.[0] === button.hid).at(-1);
  expect((last.el?.[2] ?? last.set[1]).events).toEqual(["click"]);
  expect(typeof button.$$click).toBe("function");
});

test("a canvas drawn on sends its pixels once after the task, at its size", async () => {
  let canvas;
  const { sent } = setup(h("canvas", { width: 2, height: 1, ref: (n) => (canvas = n) }));
  await tick();
  expect(sent[0].ops.find((op) => op.el?.[1] === "canvas").el[2].attrs).toEqual({ width: "2", height: "1" });
  const ctx = canvas.getContext("2d");
  expect(ctx.canvas).toBe(canvas);
  expect(canvas.getContext("webgl")).toBeNull();
  ctx.fillStyle = "rgba(255, 0, 0, 0.5)";
  ctx.fillRect(0, 0, 1, 1);
  ctx.fillRect(0, 0, 1, 1);
  ctx.measureText("not drawing");
  await tick();
  const pixels = sent.filter((m) => m.t === "pixels");
  expect(pixels.length).toBe(1);
  expect({ ...pixels[0], data: [...pixels[0].data] }).toEqual({ t: "pixels", id: canvas.hid, width: 2, height: 1, data: [255, 0, 0, 192, 0, 0, 0, 0] });
  // Resizing clears it and tells the host the new size.
  canvas.width = 3;
  await tick();
  expect(sent.at(-1).data.length).toBe(3 * 4);
  expect(sent.findLast((m) => m.t === "commit").ops.at(-1).set[1].attrs.width).toBe("3");
});

test("media calls go to the host after the element's commit, and its reports update state and fire events", async () => {
  let video;
  const heard = [];
  const { r, sent } = setup(
    h("video", { src: "https://example.com/a.mp4", loop: true, ref: (n) => (video = n), onPlay: () => heard.push("onPlay"), onLoadedMetadata: () => heard.push("onLoadedMetadata") }),
  );
  await tick();
  // Streamed by the host, not downloaded first.
  expect(sent[0].ops.find((op) => op.el?.[1] === "video").el[2]).toMatchObject({ src: "https://example.com/a.mp4", attrs: { loop: "true" } });
  video.addEventListener("ended", () => heard.push("ended"));
  expect(video.paused).toBe(true);
  await video.play();
  expect(sent.at(-1)).toEqual({ t: "media", id: video.hid, op: "play", value: 0 });
  expect(video.paused).toBe(false);
  r.dispatch({ type: "media", id: video.hid, event: "loadedmetadata", duration: 12, videoWidth: 640, videoHeight: 360 });
  r.dispatch({ type: "media", id: video.hid, event: "ended", currentTime: 12 });
  expect([video.duration, video.videoWidth, video.currentTime, video.ended, video.paused]).toEqual([12, 640, 12, true, true]);
  expect(heard).toEqual(["onPlay", "onLoadedMetadata", "ended"]);
  expect(sent.filter((m) => m.t === "settled")).toEqual([]);
});

test("a summary click toggles its details, closes its name group, and fires toggle once later", async () => {
  const toggles = [];
  const refs = {};
  const { r, sent } = setup(
    h("div", null,
      h("details", { name: "faq", ref: (n) => (refs.a = n), onToggle: (e) => toggles.push(["a", e.type]) }, h("summary", { ref: (n) => (refs.sa = n) }, "A"), "a body"),
      h("details", { name: "faq", open: true, ref: (n) => (refs.b = n) }, h("summary", null, "B"), "b body"),
    ),
  );
  await tick();
  refs.b.addEventListener("toggle", (e) => toggles.push(["b", e.oldState, e.newState]));
  // An unhandled click comes as a `document` event from the box under the pointer.
  r.dispatch({ type: "document", id: 0, target: refs.sa.hid, events: ["click"] });
  expect([refs.a.open, refs.b.open]).toEqual([true, false]);
  await null;
  expect(sent.at(-1).ops.find((op) => op.set?.[0] === refs.a.hid).set[1].attrs.open).toBe("");
  expect(toggles).toEqual([]);
  await tick();
  expect(toggles).toEqual([["a", "toggle"], ["b", "open", "closed"]]);
  // Opened and closed again before the task: nothing changed, no event.
  refs.a.open = false;
  refs.a.open = true;
  await tick();
  expect(toggles.length).toBe(2);
});

test("showModal opens a dialog in the top layer; Escape cancels and closes it, and focus comes back", async () => {
  const heard = [];
  const refs = {};
  const { r, sent } = setup(
    h("div", null,
      h("button", { ref: (n) => (refs.opener = n) }, "Open"),
      h("dialog", { ref: (n) => (refs.d = n), onCancel: () => heard.push("onCancel"), onClose: () => heard.push("onClose") },
        h("form", { method: "dialog" }, h("input", { ref: (n) => (refs.field = n) }), h("button", { value: "ok", ref: (n) => (refs.ok = n) }, "OK")),
      ),
    ),
  );
  await tick();
  refs.opener.focus();
  refs.d.showModal();
  await null;
  const wired = sent.flatMap((m) => m.ops ?? []).findLast((op) => op.set?.[0] === refs.d.hid).set[1];
  expect([wired.modal, wired.attrs.open, r.activeElement()]).toEqual([true, "", refs.field]);
  expect(() => refs.d.show()).toThrow();
  // Escape from the focused field: its keys come from the field.
  r.dispatch({ type: "keydown", id: refs.field.hid, key: "Escape" });
  expect([refs.d.open, r.activeElement()]).toEqual([false, refs.opener]);
  await tick();
  expect(heard).toEqual(["onCancel", "onClose"]);
  // A submit button in `<form method="dialog">` closes it with its value.
  refs.d.showModal();
  r.dispatch({ type: "click", id: refs.ok.hid, target: refs.ok.hid });
  expect([refs.d.open, refs.d.returnValue]).toEqual([false, "ok"]);
});

test("an <img> learns its size from the file's header, and decodes pixels only when drawn", async () => {
  const { imageSize } = await import("./elements.js");
  const canvas = skia().createCanvas(3, 2);
  canvas.getContext("2d").fillRect(0, 0, 3, 2);
  for (const type of ["image/png", "image/jpeg", "image/webp"]) expect(imageSize(canvas.toBuffer(type))).toEqual({ width: 3, height: 2 });
  expect(imageSize(Buffer.from("GIF89a\x03\x00\x02\x00", "latin1"))).toEqual({ width: 3, height: 2 });
  expect(imageSize(Buffer.from("<svg/>"))).toBeNull();

  const dir = mkdtempSync(join(tmpdir(), "gpuijs-img-"));
  const file = join(dir, "a.png");
  writeFileSync(file, canvas.toBuffer("image/png"));
  try {
    // One in the page: its size, no pixels.
    let img;
    const loaded = [];
    setup(h("img", { src: file, ref: (n) => (img = n), onLoad: () => loaded.push(img.naturalWidth) }));
    await tick();
    expect(loaded).toEqual([3]);
    expect([img.complete, img.width, img.height]).toEqual([true, 3, 2]);
    expect(img._image).toBeUndefined();
    // A detached one, as a texture loader makes: drawable once loaded.
    const loader = img.dom.createElement("img");
    const pixels = new Promise((resolve) =>
      loader.addEventListener("load", () => {
        const out = skia().createCanvas(3, 2).getContext("2d");
        out.drawImage(loader.decoded(), 0, 0);
        resolve([...out.getImageData(0, 0, 1, 1).data]);
      }),
    );
    loader.setAttribute("src", file);
    expect(await pixels).toEqual([0, 0, 0, 255]);
  } finally {
    rmSync(dir, { recursive: true });
  }
});
