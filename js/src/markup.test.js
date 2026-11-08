import { expect, test } from "bun:test";
import { parseMarkup } from "./markup.js";

test("HTML markup keeps void elements, bare attributes and entities straight", () => {
  expect(parseMarkup(`<div class=a disabled><img src="x.png"><span>1&nbsp;&#38;&#x41;</span></div>tail`)).toEqual([
    ["div", { class: "a", disabled: "" }, [["img", { src: "x.png" }, []], ["span", {}, ["1 &A"]]]],
    "tail",
  ]);
});

test("exact markup, as a <template> holds it, keeps whitespace and comments", () => {
  const markup = `<p><b>a</b> <!><!--x--></p>`;
  expect(parseMarkup(markup)).toEqual([["p", {}, [["b", {}, ["a"]]]]]);
  expect(parseMarkup(markup, true)).toEqual([["p", {}, [["b", {}, ["a"]], " ", ["#comment", {}, [""]], ["#comment", {}, ["x"]]]]]);
});
