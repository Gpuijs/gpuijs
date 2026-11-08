import { expect, test } from "bun:test";
import { parseSvg } from "./svgr.js";

test("svg markup parses to React-spelled element trees", () => {
  const markup = `<?xml version="1.0"?>
<!-- icon -->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" class="fill-current" style="opacity: .5; stroke-width:2">
  <path d="M5 10H15" stroke-linecap='round'/>
  <g fillRule="evenodd"><text>A &amp; B</text></g>
</svg>`;
  expect(parseSvg(markup)).toEqual([
    "svg",
    { xmlns: "http://www.w3.org/2000/svg", viewBox: "0 0 20 20", className: "fill-current", style: { opacity: ".5", strokeWidth: "2" } },
    [
      ["path", { d: "M5 10H15", "stroke-linecap": "round" }, []],
      ["g", { fillRule: "evenodd" }, [["text", {}, ["A & B"]]]],
    ],
  ]);
});
