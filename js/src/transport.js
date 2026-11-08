// Host -> JS: NDJSON over the Unix socket in GPUIJS_SOCKET. JS -> host: frames (commits in
// binary, see wire.js; the rest JSON) written with blocking writes to the pipe in GPUIJS_IN, so
// every message is out before JS blocks waiting for a layout answer on the pipe in GPUIJS_SYNC
// (see `waitForHost`). stdout stays free for console.log.
import net from "node:net";
import { openSync, readSync, writeSync } from "node:fs";
import { encodeCommit } from "./wire.js";

let answers = null;

export function connect(onMessage) {
  const path = process.env.GPUIJS_SOCKET;
  if (!path) throw new Error("gpuijs: GPUIJS_SOCKET is not set; launch this bundle through the gpuijs host");

  const socket = net.connect(path);
  socket.setEncoding("utf8");
  let buf = "";
  socket.on("data", (chunk) => {
    buf += chunk;
    let i;
    while ((i = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, i);
      buf = buf.slice(i + 1);
      if (line) onMessage(JSON.parse(line));
    }
  });
  // The host owns our lifetime: when its window closes, the socket closes.
  socket.on("close", () => process.exit(0));
  socket.on("error", (err) => {
    console.error("[gpuijs] socket error:", err);
    process.exit(1);
  });
  const out = openSync(process.env.GPUIJS_IN, "w");
  return (msg) => {
    const bytes = msg.t === "commit" ? encodeCommit(msg.ops, HEAD) : msg.t === "pixels" ? pixels(msg) : json(msg);
    // Frame: a u8 kind (0 JSON, 1 binary commit, 2 canvas pixels) and the u32 length of what follows.
    bytes[0] = msg.t === "commit" ? 1 : msg.t === "pixels" ? 2 : 0;
    new DataView(bytes.buffer, bytes.byteOffset).setUint32(1, bytes.length - HEAD, true);
    for (let at = 0; at < bytes.length; ) at += writeSync(out, bytes, at);
  };
}

const HEAD = 5;
function json(msg) {
  const body = Buffer.from(JSON.stringify(msg));
  const frame = Buffer.allocUnsafe(HEAD + body.length);
  body.copy(frame, HEAD);
  return frame;
}

// A canvas's id, width and height as u32s, then its RGBA rows (`decode_pixels` in src/protocol.rs).
function pixels({ id, width, height, data }) {
  const frame = Buffer.allocUnsafe(HEAD + 12 + data.length);
  frame.writeUInt32LE(id, HEAD);
  frame.writeUInt32LE(width, HEAD + 4);
  frame.writeUInt32LE(height, HEAD + 8);
  frame.set(data, HEAD + 12);
  return frame;
}

/** Blocks until the host writes one line to the answer pipe; parsed. */
export function waitForHost() {
  answers ??= openSync(process.env.GPUIJS_SYNC, "r");
  const chunks = [];
  const chunk = Buffer.alloc(64 * 1024);
  for (;;) {
    const n = readSync(answers, chunk, 0, chunk.length, null);
    const end = chunk.subarray(0, n).indexOf(10);
    // The host answers once per request, so a line never runs into the next one.
    chunks.push(Buffer.from(chunk.subarray(0, end < 0 ? n : end)));
    if (end >= 0 || n === 0) return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  }
}
