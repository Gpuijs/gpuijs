// Commits as bytes for the host (`decode_commit` in src/protocol.rs reads them): a 10k-row mount
// is megabytes of ops, which JSON took longer to write and parse than the host to apply.
//
// Little-endian. A commit is a u32 op count, then each op as a u8 code and its fields. Ids are
// u32; strings a u32 byte length and UTF-8. Props are a u16 mask of the fields present, then
// those fields in mask-bit order (`disabled`, `measure` and `modal` are bits only).

const OPS = { el: 1, txt: 2, set: 3, settext: 4, append: 5, insert: 6, remove: 7, detach: 8, focus: 9, hover: 10, scroll: 11 };
const CLASS = 1 << 0;
const ID = 1 << 1;
const STYLE = 1 << 2;
const EVENTS = 1 << 3;
const ATTRS = 1 << 4;
const VALUE = 1 << 5;
const PLACEHOLDER = 1 << 6;
const DISABLED = 1 << 7;
const SRC = 1 << 8;
const SEQ = 1 << 9;
const MEASURE = 1 << 10;
const MODAL = 1 << 11;

const text = new TextEncoder();
let buf = new Uint8Array(1 << 16);
let view = new DataView(buf.buffer);
let at = 0;

function room(n) {
  if (at + n <= buf.length) return;
  let size = buf.length * 2;
  while (size < at + n) size *= 2;
  const grown = new Uint8Array(size);
  grown.set(buf.subarray(0, at));
  buf = grown;
  view = new DataView(buf.buffer);
}
function u8(n) {
  room(1);
  buf[at++] = n;
}
function u16(n) {
  room(2);
  view.setUint16(at, n, true);
  at += 2;
}
function u32(n) {
  if (!(n >= 0 && n <= 0xffffffff)) throw new Error(`gpuijs: ${n} doesn't fit the wire's u32`);
  room(4);
  view.setUint32(at, n, true);
  at += 4;
}
function str(s) {
  // UTF-8 takes at most 3 bytes per UTF-16 unit.
  room(4 + s.length * 3);
  const start = at + 4;
  // ASCII is copied here: most strings are short class names and text, where calling out to the
  // encoder (and making a view for it) cost more than the copy.
  let i = 0;
  for (; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c > 0x7f) break;
    buf[start + i] = c;
  }
  const written = i < s.length ? i + text.encodeInto(s.slice(i), buf.subarray(start + i)).written : i;
  view.setUint32(at, written, true);
  at = start + written;
}
function props(p) {
  let mask = 0;
  if (p.class) mask |= CLASS;
  if (p.id != null) mask |= ID;
  if (p.style != null) mask |= STYLE;
  if (p.events?.length) mask |= EVENTS;
  const attrs = p.attrs && Object.entries(p.attrs);
  if (attrs?.length) mask |= ATTRS;
  if (p.value != null) mask |= VALUE;
  if (p.placeholder != null) mask |= PLACEHOLDER;
  if (p.disabled) mask |= DISABLED;
  if (p.src != null) mask |= SRC;
  if (p.seq != null) mask |= SEQ;
  if (p.measure) mask |= MEASURE;
  if (p.modal) mask |= MODAL;
  u16(mask);
  if (mask & CLASS) str(p.class);
  if (mask & ID) str(p.id);
  if (mask & STYLE) str(p.style);
  if (mask & EVENTS) {
    u8(p.events.length);
    for (const e of p.events) str(e);
  }
  if (mask & ATTRS) {
    u16(attrs.length);
    for (const [k, v] of attrs) {
      str(k);
      str(v);
    }
  }
  if (mask & VALUE) str(p.value);
  if (mask & PLACEHOLDER) str(p.placeholder);
  if (mask & SRC) str(p.src);
  if (mask & SEQ) u32(p.seq);
}
function maybe(id) {
  u8(id == null ? 0 : 1);
  if (id != null) u32(id);
}

/** The ops of a commit as bytes, after `head` bytes left for the caller (a frame header). The
 * result is a view of a buffer the next call reuses. */
export function encodeCommit(ops, head = 0) {
  at = 0;
  room(head);
  at = head;
  u32(ops.length);
  for (const op of ops) {
    let kind;
    for (kind in op) break;
    const args = op[kind];
    if (!OPS[kind]) throw new Error(`gpuijs: no wire code for op ${kind}`);
    u8(OPS[kind]);
    switch (kind) {
      case "el":
        u32(args[0]);
        str(args[1]);
        props(args[2]);
        break;
      case "set":
        u32(args[0]);
        props(args[1]);
        break;
      case "txt":
      case "settext":
        u32(args[0]);
        str(args[1]);
        break;
      case "insert":
      case "scroll":
        u32(args[0]);
        u32(args[1]);
        u32(args[2]);
        break;
      case "focus":
      case "hover":
        maybe(args);
        break;
      default:
        u32(args[0]);
        u32(args[1]);
    }
  }
  return buf.subarray(0, at);
}
