import { expect, test } from "bun:test";
import { apply, bbox, hitTest, parseTransform, pathPolylines, rootBox } from "./svg-geometry.js";

// Host-node-like elements: type, props, kids, parent.
const el = (type, props = {}, kids = []) => {
  const n = { type, props, kids };
  kids.forEach((k) => (k.parent = n));
  return n;
};

test("transforms compose left to right, as nested groups do", () => {
  const m = parseTransform("translate(10, 20) scale(2)");
  expect(apply(m, 1, 1)).toEqual([12, 22]);
  const [x, y] = apply(parseTransform("rotate(90 5 5)"), 10, 5);
  expect([Math.round(x), Math.round(y)]).toEqual([5, 10]);
});

test("paths flatten lines, relative moves and arcs onto their outline", () => {
  const [square] = pathPolylines("m 0 0 h 10 v 10 H 0 z");
  expect(square).toEqual([[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]);
  const [half] = pathPolylines("M 0 0 A 5 5 0 0 1 10 0");
  const top = Math.min(...half.map((p) => p[1]));
  expect(Math.round(top)).toBe(-5); // the half circle bulges up (sweep 1, y down)
  expect(half.at(-1).map((v) => Math.round(v) + 0)).toEqual([10, 0]);
});

test("hits go to the topmost filled shape through group transforms; boxes follow them", () => {
  const back = el("rect", { width: "100", height: "100", class: "bg" });
  const bar = el("path", { d: "M10 10 H20 V50 H10 Z" });
  const crosshair = el("rect", { x: "10", width: "10", height: "100", "pointer-events": "none" });
  const group = el("g", { transform: "translate(30, 0)" }, [bar, crosshair]);
  const svg = el("svg", {}, [back, group]);
  expect(hitTest(svg, 45, 30)).toBe(bar);
  expect(hitTest(svg, 45, 80)).toBe(back);
  expect(hitTest(svg, 200, 30)).toBeNull();
  expect(bbox(group)).toEqual({ x: 10, y: 0, width: 10, height: 100 }); // getBBox: own transform excluded
  expect(rootBox(bar)).toEqual({ x: 40, y: 10, width: 10, height: 40 });
});
