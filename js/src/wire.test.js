import { expect, test } from "bun:test";
import { encodeCommit } from "./wire.js";

// The same bytes are decoded in src/protocol.rs's `decodes_the_binary_commit_wire_js_writes`.
const FIXTURE =
  "0b000000010200000003000000646976ff0f03000000612062010000007809000000636f6c6f723a7265640105000000636c69636b010006000000646174612d6b02000000c3a90100000076010000007005000000732e706e6703000000020300000002000000686903020000000000040300000002000000796f0501000000020000000602000000030000000400000007010000000200000008020000000300000009000a01050000000b020000000a0000002c010000";

test("a commit encodes to the bytes the host decodes", () => {
  const ops = [
    { el: [2, "div", { class: "a b", id: "x", style: "color:red", events: ["click"], attrs: { "data-k": "é" }, value: "v", placeholder: "p", disabled: true, src: "s.png", seq: 3, measure: true, modal: true }] },
    { txt: [3, "hi"] },
    { set: [2, {}] },
    { settext: [3, "yo"] },
    { append: [1, 2] },
    { insert: [2, 3, 4] },
    { remove: [1, 2] },
    { detach: [2, 3] },
    { focus: null },
    { hover: 5 },
    { scroll: [2, 10, 300] },
  ];
  expect(Buffer.from(encodeCommit(ops)).toString("hex")).toBe(FIXTURE);
});

test("room is left for a frame header, and a big commit grows the buffer", () => {
  const ops = Array.from({ length: 20000 }, (_, i) => ({ txt: [i + 2, "row " + i] }));
  const bytes = encodeCommit(ops, 5);
  expect(new DataView(bytes.buffer, bytes.byteOffset).getUint32(5, true)).toBe(20000);
  expect(() => encodeCommit([{ append: [2 ** 32, 1] }])).toThrow();
});
