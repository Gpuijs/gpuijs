import { expect, test } from "bun:test";
import { createElement as h } from "react";
import { MutationObserver, Range, selection } from "./editing.js";
import { createRenderer } from "./renderer.js";

const tick = () => new Promise((r) => setTimeout(r, 20));

// An editor's root as Lexical and ProseMirror make one: a React element with `contentEditable`,
// filled by the library. `paragraphs` become <p>s of one text node each.
async function editor(...paragraphs) {
  const sent = [];
  let root;
  const r = createRenderer((msg) => msg.t !== "settled" && sent.push(msg));
  r.render(h("div", { contentEditable: true, suppressContentEditableWarning: true, ref: (n) => (root = n) }));
  await tick();
  for (const text of paragraphs) {
    const p = root.dom.createElement("p");
    p.appendChild(root.dom.createText(text));
    root.appendChild(p);
  }
  const heard = [];
  for (const type of ["beforeinput", "input"]) root.addEventListener(type, (e) => heard.push(`${type} ${e.inputType} ${e.data ?? ""}`.trim()));
  const event = (type, fields) => r.dispatch({ t: "event", id: root.hid, type, ...fields });
  const key = (key, mods = {}) => r.dispatch({ t: "event", id: 0, type: "keydown", key, ...mods });
  const html = () => root.innerHTML;
  return { r, sent, root, heard, event, key, html, text: (i) => root.childNodes[i].firstChild };
}

test("a press in editable content focuses it and places the caret; typing inserts there", async () => {
  const { root, heard, event, html, text } = await editor("ab");
  event("caret", { node: text(0).hid, offset: 1 });
  expect(root.isFocused()).toBe(true);
  expect([selection.focusNode, selection.focusOffset]).toEqual([text(0), 1]);
  event("textinput", { text: "x" });
  expect(html()).toBe("<p>axb</p>");
  expect(selection.focusOffset).toBe(2);
  expect(heard).toEqual(["beforeinput insertText x", "input insertText x"]);
  // Return arrives as a key, not as text.
  event("textinput", { text: "\r" });
  expect(html()).toBe("<p>axb</p>");
});

test("Enter splits the block, Backspace at its start joins it back", async () => {
  const { root, event, key, html, text } = await editor("abc");
  event("caret", { node: text(0).hid, offset: 1 });
  key("Enter");
  expect(html()).toBe("<p>a</p><p>bc</p>");
  expect([selection.focusNode, selection.focusOffset]).toEqual([text(1), 0]);
  key("Backspace");
  expect(html()).toBe("<p>abc</p>");
  expect(selection.focusOffset).toBe(1);
  // Shift+Enter breaks the line; at a block's end a second <br> holds the new line open.
  // The joined block holds both text nodes.
  event("caret", { node: root.firstChild.lastChild.hid, offset: 2 });
  key("Enter", { shiftKey: true });
  expect(html()).toBe("<p>abc<br><br></p>");
});

test("an emptied block keeps a <br> for its line, which the next text replaces", async () => {
  const { event, key, html, text } = await editor("a", "b");
  event("caret", { node: text(1).hid, offset: 1 });
  key("Backspace");
  expect(html()).toBe("<p>a</p><p><br></p>");
  event("textinput", { text: "c" });
  expect(html()).toBe("<p>a</p><p>c</p>");
  // Option+Backspace takes the word; a selection goes as one.
  key("ArrowUp", { caretTo: [text(0).hid, 1] });
  expect(selection.focusNode).toBe(text(0));
  selection.setBaseAndExtent(text(0), 0, text(1), 1);
  expect(selection.toString()).toBe("ac");
  key("Delete");
  expect(html()).toBe("<p><br></p>");
});

test("a cancelled beforeinput leaves the content to the editor", async () => {
  const { root, event, key, html, text } = await editor("ab");
  root.addEventListener("beforeinput", (e) => e.preventDefault());
  event("caret", { node: text(0).hid, offset: 2 });
  event("textinput", { text: "x" });
  key("Backspace");
  key("Enter");
  expect(html()).toBe("<p>ab</p>");
});

test("IME composition replaces its own text until committed", async () => {
  const { root, event, html, text } = await editor("a");
  const phases = [];
  for (const type of ["compositionstart", "compositionupdate", "compositionend"]) root.addEventListener(type, (e) => phases.push(`${type} ${e.data}`));
  event("caret", { node: text(0).hid, offset: 1 });
  event("composition", { text: "k" });
  event("composition", { text: "か" });
  expect(html()).toBe("<p>aか</p>");
  event("textinput", { text: "化" });
  expect(html()).toBe("<p>a化</p>");
  expect(phases).toEqual(["compositionstart ", "compositionupdate k", "compositionupdate か", "compositionend 化"]);
});

test("MutationObserver reports text and child changes in a microtask", async () => {
  const { root, event, key, text } = await editor("ab");
  const records = [];
  new MutationObserver((list) => records.push(...list)).observe(root, { subtree: true, childList: true, characterData: true, characterDataOldValue: true });
  event("caret", { node: text(0).hid, offset: 2 });
  event("textinput", { text: "c" });
  key("Enter");
  await Promise.resolve();
  expect(records[0]).toMatchObject({ type: "characterData", target: text(0), oldValue: "ab" });
  expect(records.some((r) => r.type === "childList" && r.target === root && r.addedNodes[0]?.type === "p")).toBe(true);
});

test("Range: text across nodes, deleting, comparing, and the selection kept live", async () => {
  const { root, text } = await editor("hello", "world");
  const r = new Range();
  r.setStart(text(0), 3);
  r.setEnd(text(1), 2);
  expect(r.toString()).toBe("lowo");
  expect(r.commonAncestorContainer).toBe(root);
  const r2 = r.cloneRange();
  r2.collapse(true);
  expect(r.compareBoundaryPoints(Range.START_TO_START, r2)).toBe(0);
  expect(r.comparePoint(text(1), 4)).toBe(1);
  r.deleteContents();
  expect(root.innerHTML).toBe("<p>hel</p><p>rld</p>");
  // A node inserted before the selection's point in the same parent moves the point along.
  selection.collapse(root, 1);
  root.insertBefore(root.dom.createElement("hr"), root.firstChild);
  expect(selection.focusOffset).toBe(2);
});

test("the host gets the caret and selected text of the focused editing host", async () => {
  const { sent, root, event, text } = await editor("hello");
  event("caret", { node: text(0).hid, offset: 1 });
  event("caret", { node: text(0).hid, offset: 4, extend: true });
  await tick();
  expect(sent.filter((m) => m.t === "selection").at(-1)).toEqual({ t: "selection", host: root.hid, caret: [text(0).hid, 4], ranges: [[text(0).hid, 1, 4]] });
  root.blur();
  await tick();
  expect(sent.at(-1)).toMatchObject({ t: "selection", host: null, caret: null });
});
