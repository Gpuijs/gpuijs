import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import path from "node:path";

/** `localStorage`, kept in `file` (JSON) across launches; in memory only without one. */
export function createStorage(file) {
  let store = new Map();
  try {
    if (file && existsSync(file)) store = new Map(Object.entries(JSON.parse(readFileSync(file, "utf8"))));
  } catch (err) {
    console.error(`[gpuijs] ignoring unreadable ${file}:`, err);
  }
  const save = () => {
    if (!file) return;
    mkdirSync(path.dirname(file), { recursive: true });
    // Written aside and renamed, so a crash mid-write never leaves a half-written store.
    const part = `${file}.${process.pid}.part`;
    writeFileSync(part, JSON.stringify(Object.fromEntries(store)));
    renameSync(part, file);
  };
  return {
    get length() {
      return store.size;
    },
    key: (i) => [...store.keys()][i] ?? null,
    getItem: (k) => store.get(String(k)) ?? null,
    setItem: (k, v) => (store.set(String(k), String(v)), save()),
    removeItem: (k) => store.delete(String(k)) && save(),
    clear: () => (store.clear(), save()),
  };
}
