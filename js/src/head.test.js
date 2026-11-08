import { expect, test } from "bun:test";
import { createHead } from "./head.js";

const tick = () => new Promise((r) => setTimeout(r, 0));

test("<style>s in head reach the host as one update per task, until removed", async () => {
  const sent = [];
  const { head, createStyle } = createHead((msg) => sent.push(msg));

  // react-resizable-panels: append, then swap a single rule per cursor change.
  const cursor = createStyle();
  expect(cursor.sheet).toBeNull();
  head.appendChild(cursor);
  const i = cursor.sheet.insertRule("*{cursor: ew-resize !important;}");
  // react-style-singleton / sonner: a text node appended to the element.
  const lib = createStyle();
  lib.appendChild({ data: ".a{color:red}" });
  head.appendChild(lib);
  await tick();
  expect(sent).toEqual([{ t: "styles", sheets: ["\n*{cursor: ew-resize !important;}", ".a{color:red}"] }]);

  cursor.sheet.removeRule(i);
  cursor.sheet.insertRule("*{cursor: w-resize !important;}");
  await tick();
  expect(sent.at(-1).sheets[0]).toBe("\n*{cursor: w-resize !important;}");

  head.removeChild(cursor);
  lib.remove();
  await tick();
  expect(sent.at(-1)).toEqual({ t: "styles", sheets: [] });
  expect(sent).toHaveLength(3);
});

test("goober: its sheet is found again by id and written through its text node", async () => {
  const sent = [];
  const { head, createStyle } = createHead((msg) => sent.push(msg));
  const sheet = () => (globalThis._goober || Object.assign(head.appendChild(createStyle()), { innerHTML: " ", id: "_goober" })).firstChild;
  sheet().data += ".a{}";
  sheet().data += ".b{}";
  await tick();
  expect(head.children).toHaveLength(1);
  expect(sent.at(-1).sheets).toEqual([" .a{}.b{}"]);
  delete globalThis._goober;
});

test("a prepended <style> goes first (ApexCharts' defaults, under the app's sheets)", async () => {
  const sent = [];
  const { head, createStyle } = createHead((msg) => sent.push(msg));
  const app = createStyle();
  app.textContent = ".a{}";
  head.appendChild(app);
  const lib = createStyle();
  lib.textContent = ".b{}";
  head.prepend(lib);
  await tick();
  expect(sent.at(-1).sheets).toEqual([".b{}", ".a{}"]);
  expect(head.children).toEqual([lib, app]);
});

test("flush sends queued style edits now, and the queued send then has nothing to add", async () => {
  const sent = [];
  const { head, createStyle, flush } = createHead((msg) => sent.push(msg));
  const style = createStyle();
  head.appendChild(style);
  style.sheet.insertRule(".menu{position:fixed}");
  flush();
  expect(sent).toEqual([{ t: "styles", sheets: [".menu{position:fixed}"].map((r) => "\n" + r) }]);
  await tick();
  expect(sent.length).toBe(1);
});
