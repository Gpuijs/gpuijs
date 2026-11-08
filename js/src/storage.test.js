import { expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { createStorage } from "./storage.js";

test("localStorage survives a relaunch through its file", () => {
  const file = path.join(mkdtempSync(path.join(tmpdir(), "gpuijs-storage-")), "app", "localStorage.json");
  const a = createStorage(file);
  a.setItem("theme", "dark");
  a.setItem("n", 1);
  a.removeItem("n");
  const b = createStorage(file);
  expect(b.getItem("theme")).toBe("dark");
  expect(b.getItem("n")).toBeNull();
  expect(b.length).toBe(1);
  b.clear();
  expect(createStorage(file).length).toBe(0);
});
