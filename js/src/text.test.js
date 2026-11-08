import { expect, test } from "bun:test";
import { measureText, setMetrics } from "./text.js";

test("text measures from the host's advances, at the style's size and weight", () => {
  const regular = Array(95).fill(0.5);
  const bold = Array(95).fill(0.6);
  setMetrics({ regular, bold });
  expect(measureText("Apr 9", { fontSize: "12px" })).toEqual({ width: 30, height: 14 });
  expect(measureText("ab", { fontSize: 10, fontWeight: 700 }).width).toBe(12);
  // Defaults to a 16px body font; characters outside ASCII use an average advance.
  expect(measureText("é").width).toBeCloseTo(8.8);
});
