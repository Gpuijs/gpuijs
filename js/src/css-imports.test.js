import { expect, test } from "bun:test";
import path from "node:path";
import { cssImportsOf, importedCss } from "./css-imports.js";

test("CSS imported from JS comes out in evaluation order, each file once", () => {
  const imp = (...paths) => ({ imports: paths.map((p) => ({ path: p })) });
  const inputs = {
    "src/main.tsx": imp("lib/a.css", "src/App.tsx", "src/index.css"),
    "src/App.tsx": imp("src/Calendar.tsx", "lib/a.css"),
    "src/Calendar.tsx": imp("cal/skeleton.css", "cal/theme.css", "src/App.tsx"),
  };
  expect(importedCss(inputs, path.resolve("src/main.tsx"))).toEqual(
    ["lib/a.css", "cal/skeleton.css", "cal/theme.css", "src/index.css"].map((f) => path.resolve(f)),
  );
});

test("a stylesheet's relative @imports resolve against it", () => {
  const file = path.resolve("app/src/app.css");
  expect([...cssImportsOf(file, `@import './styles/index.css';\n@import "../app/globals.css";`)]).toEqual([
    path.resolve("app/src/styles/index.css"),
    path.resolve("app/app/globals.css"),
  ]);
});
