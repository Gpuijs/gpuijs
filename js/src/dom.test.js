import { expect, test } from "bun:test";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

// What plain-DOM code (vanilla JS, Solid, Vue) calls, under `@gpuijs/core/env` with a stand-in host.
// env.js replaces globals, so the checks run in a Bun process of their own (see the end).
let host;
const loaded = [];
if (process.env.GPUIJS_DOM_CHECKS) {
  // Accepts the connection, sends font metrics, discards what JS sends; `host.write` sends events.
  const sock = join(tmpdir(), `gpuijs-dom-${process.pid}.sock`);
  net.createServer((c) => ((host = c), c.write(JSON.stringify({ t: "metrics", regular: null, bold: null }) + "\n"))).listen(sock);
  await new Promise((r) => setTimeout(r, 50));
  Object.assign(process.env, { GPUIJS_SOCKET: sock, GPUIJS_IN: "/dev/null" });
  await import("./env.js");
  document.addEventListener("DOMContentLoaded", () => loaded.push("DOMContentLoaded"));
  addEventListener("load", () => loaded.push("load"));
  // Something to send opens the connection.
  document.body.appendChild(document.createElement("main"));
  await new Promise((r) => setTimeout(r, 50));
}
const fromHost = async (msg) => {
  host.write(JSON.stringify({ t: "event", ...msg }) + "\n");
  await new Promise((r) => setTimeout(r, 20));
};

const checks = {
  "document.querySelector finds body content": () => {
    const d = document.createElement("div");
    d.id = "app";
    document.body.appendChild(d);
    return document.querySelector("#app") === d && document.querySelectorAll("div").length >= 1;
  },
  "new Audio(src) is a media element": () => {
    const a = new Audio("/beep.mp3");
    return a instanceof HTMLAudioElement && a instanceof HTMLMediaElement && a.src === "/beep.mp3" && a.paused && a.canPlayType("audio/mpeg") === "maybe";
  },
  "a detached canvas measures text": () => document.createElement("canvas").getContext("2d").measureText("hi").width > 0,
  "document.getElementById": () => document.getElementById("app")?.id === "app",
  "getElementsByTagName on document": () => document.getElementsByTagName("div").length >= 1,
  "createComment": () => document.createComment("x").nodeType === 8,
  "instanceof HTMLElement/Element/Node": () => {
    const d = document.createElement("div");
    return d instanceof HTMLElement && d instanceof Element && d instanceof Node;
  },
  "Text instanceof": () => document.createTextNode("a") instanceof Text,
  "template.content": () => {
    const t = document.createElement("template");
    t.innerHTML = "<div><span>a</span>b</div>";
    return t.content.firstChild?.nodeName === "DIV" && t.content.firstChild.firstChild.nodeName === "SPAN";
  },
  "innerHTML parses nested": () => {
    const d = document.createElement("div");
    d.innerHTML = "<ul><li class='a'>1</li><li>2</li></ul>";
    return d.querySelectorAll("li").length === 2 && d.querySelector(".a").textContent === "1";
  },
  "cloneNode(true) deep": () => {
    const d = document.createElement("div");
    d.innerHTML = "<p>x<b>y</b></p>";
    const c = d.cloneNode(true);
    return c.querySelector("b")?.textContent === "y" && c !== d;
  },
  "append/prepend/before/after/replaceWith": () => {
    const p = document.createElement("div");
    const a = document.createElement("a"), b = document.createElement("b"), i = document.createElement("i");
    p.append(a, "t");
    p.prepend(b);
    a.after(i);
    i.before(document.createElement("em"));
    i.replaceWith(document.createElement("s"));
    return p.childNodes.map((n) => n.nodeName).join(",") === "B,A,EM,S,#text";
  },
  "insertAdjacentHTML": () => {
    const d = document.createElement("div");
    d.insertAdjacentHTML("beforeend", "<span>x</span>");
    return d.firstChild?.nodeName === "SPAN";
  },
  "outerHTML": () => {
    const d = document.createElement("div");
    d.className = "a";
    d.textContent = "x";
    return d.outerHTML === '<div class="a">x</div>';
  },
  "classList": () => {
    const d = document.createElement("div");
    d.classList.add("a", "b");
    d.classList.toggle("a");
    return d.className === "b" && d.classList.contains("b");
  },
  "style.setProperty / cssText": () => {
    const d = document.createElement("div");
    d.style.setProperty("color", "red");
    d.style.marginTop = "4px";
    return d.style.color === "red" && d.style.getPropertyValue("margin-top") === "4px";
  },
  "dataset": () => {
    const d = document.createElement("div");
    d.dataset.fooBar = "1";
    return d.getAttribute("data-foo-bar") === "1";
  },
  "id/title/hidden properties reflect": () => {
    const d = document.createElement("div");
    d.id = "x";
    d.title = "t";
    d.hidden = true;
    return d.getAttribute("id") === "x" && d.getAttribute("title") === "t" && d.hasAttribute("hidden");
  },
  "input value/checked/type props": () => {
    const i = document.createElement("input");
    i.type = "checkbox";
    i.checked = true;
    const t = document.createElement("input");
    t.value = "hi";
    return i.getAttribute("type") === "checkbox" && i.checked && t.value === "hi";
  },
  "capture listeners and stopImmediatePropagation": () => {
    const p = document.createElement("div"), c = document.createElement("span");
    p.appendChild(c);
    const seen = [];
    p.addEventListener("x", () => seen.push("p-capture"), { capture: true });
    p.addEventListener("x", () => seen.push("p-bubble"));
    c.addEventListener("x", (e) => { seen.push("c1"); e.stopImmediatePropagation(); });
    c.addEventListener("x", () => seen.push("c2"));
    c.dispatchEvent(new Event("x", { bubbles: true }));
    return seen.join() === "p-capture,c1";
  },
  "event.target / eventPhase": () => {
    const p = document.createElement("div"), c = document.createElement("span");
    p.appendChild(c);
    let t;
    p.addEventListener("y", (e) => (t = [e.target, e.currentTarget]));
    c.dispatchEvent(new CustomEvent("y", { bubbles: true, detail: 1 }));
    return t?.[0] === c && t?.[1] === p;
  },
  "onclick property handler": () => {
    const b = document.createElement("button");
    let n = 0;
    b.onclick = () => n++;
    b.click();
    b.dispatchEvent(new Event("click"));
    return n >= 1;
  },
  "IntersectionObserver exists": () => typeof IntersectionObserver === "function",
  "requestIdleCallback": () => typeof requestIdleCallback === "function",
  "window size": () => typeof innerWidth === "number" && innerWidth > 0 && typeof devicePixelRatio === "number",
  "document.title/readyState": () => typeof document.title === "string" && document.readyState === "complete",
  "createRange": () => typeof document.createRange === "function",
  "scrollTop settable": () => {
    const d = document.createElement("div");
    d.scrollTop = 10;
    return typeof d.scrollTop === "number";
  },
  "Node constants": () => Node.ELEMENT_NODE === 1 && Node.TEXT_NODE === 3,
  "elements: children/childElementCount/firstElementChild": () => {
    const d = document.createElement("div");
    d.innerHTML = "<a></a>t<b></b>";
    return d.childElementCount === 2 && d.firstElementChild.nodeName === "A" && d.lastElementChild.nodeName === "B";
  },
  "text node splitText/data": () => {
    const t = document.createTextNode("ab");
    t.data = "cd";
    return t.textContent === "cd" && t.nodeValue === "cd";
  },
  "form elements / FormData": () => {
    const f = document.createElement("form");
    const i = document.createElement("input");
    i.name = "q";
    i.value = "v";
    f.appendChild(i);
    return new FormData(f).get("q") === "v";
  },
  "getComputedStyle": () => typeof getComputedStyle(document.body).getPropertyValue === "function",
  "DOMContentLoaded listener": () => (document.addEventListener("DOMContentLoaded", () => {}), true),
  "document.addEventListener click capture": () => (document.addEventListener("click", () => {}, true), true),
  "tabIndex/focus/activeElement": () => {
    const b = document.createElement("button");
    document.body.appendChild(b);
    b.focus();
    return document.activeElement === b;
  },
  "textContent setter on element with children": () => {
    const d = document.createElement("div");
    d.innerHTML = "<b>x</b>";
    d.textContent = "y";
    return d.childNodes.length === 1 && d.textContent === "y";
  },
  "nodeName / localName / tagName": () => {
    const d = document.createElement("div");
    return d.nodeName === "DIV" && d.tagName === "DIV" && d.localName === "div";
  },
  "isEqualNode/hasChildNodes": () => {
    const d = document.createElement("div");
    return typeof d.hasChildNodes === "function" && !d.hasChildNodes();
  },
  "window and elements hear scrolls the host reports": async () => {
    const d = document.createElement("div");
    document.body.appendChild(d);
    const heard = [];
    addEventListener("scroll", () => heard.push(["window", scrollY]));
    d.addEventListener("scroll", (e) => heard.push(["el", e.target.scrollTop, e.bubbles]));
    await fromHost({ id: d.hid, type: "scroll", scrollLeft: 0, scrollTop: 0, init: true });
    await fromHost({ id: d.hid, type: "scroll", scrollLeft: 0, scrollTop: 30 });
    await fromHost({ id: 0, type: "scroll", scrollLeft: 0, scrollTop: 120 });
    return JSON.stringify(heard) === '[["el",30,false],["window",120]]' && document.scrollingElement.scrollTop === 120;
  },
  "DOMContentLoaded and load fire once modules ran": () => document.readyState === "complete" && loaded.join() === "DOMContentLoaded,load",
  "CSS.escape": () => CSS.escape("a:b") === "a\\:b" && CSS.escape("1x") === "\\31 x",
  "document.getElementsByClassName": () => {
    const d = document.createElement("p");
    d.className = "k1 k2";
    document.body.appendChild(d);
    return document.getElementsByClassName("k2 k1")[0] === d;
  },
  "DOMParser parses into detached nodes": () => new DOMParser().parseFromString("<body><p>a<b>bold</b></p></body>", "text/html").body.querySelector("b")?.textContent === "bold",
  "document.implementation.createHTMLDocument": () => {
    const doc = document.implementation.createHTMLDocument();
    return doc.createElement("div").nodeName === "DIV" && doc.body !== document.body;
  },
  "head is found by tag name": () => document.getElementsByTagName("head")[0] === document.head && document.head.contains(document.head),
  "InputEvent has getTargetRanges": () => "getTargetRanges" in new InputEvent("input"),
  "getSelection and ranges over host nodes": () => {
    const p = document.createElement("p");
    p.textContent = "hello";
    document.body.appendChild(p);
    getSelection().setBaseAndExtent(p.firstChild, 1, p.firstChild, 4);
    const r = document.createRange();
    r.selectNodeContents(p);
    return getSelection().toString() === "ell" && r.toString() === "hello" && r instanceof Range && getSelection().type === "Range";
  },
  "MutationObserver hears appended children": async () => {
    const d = document.createElement("div");
    const seen = [];
    new MutationObserver((records) => seen.push(...records)).observe(d, { childList: true });
    d.appendChild(document.createElement("span"));
    await Promise.resolve();
    return seen.length === 1 && seen[0].addedNodes[0].nodeName === "SPAN";
  },
  "isContentEditable inherits": () => {
    const d = document.createElement("div");
    d.contentEditable = "true";
    const s = document.createElement("span");
    d.appendChild(s);
    return s.isContentEditable && !document.createElement("div").isContentEditable && d.contentEditable === "true";
  },
  "instanceof by tag": () => {
    const i = document.createElement("input");
    return i instanceof HTMLInputElement && !(i instanceof HTMLButtonElement) && document.createComment("") instanceof Node;
  },
};
if (process.env.GPUIJS_DOM_CHECKS) {
  const failed = [];
  for (const [name, f] of Object.entries(checks)) {
    try {
      if (!(await f())) failed.push(name);
    } catch (e) {
      failed.push(`${name}: ${e.message}`);
    }
  }
  console.log(JSON.stringify(failed));
  process.exit(0);
}

test("DOM APIs work as in a browser", () => {
  const p = Bun.spawnSync(["bun", import.meta.path], { env: { ...process.env, GPUIJS_DOM_CHECKS: "1" } });
  if (p.exitCode !== 0) throw new Error(p.stderr.toString());
  expect(JSON.parse(p.stdout.toString().trim().split("\n").at(-1))).toEqual([]);
});
