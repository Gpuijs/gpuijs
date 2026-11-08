import { expect, test } from "bun:test";

// env.js replaces globals, so each check runs in its own Bun process.
const run = (code) => {
  const env = new URL("./env.js", import.meta.url).pathname;
  const p = Bun.spawnSync(["bun", "-e", `await import(${JSON.stringify(env)}); ${code}`]);
  if (p.exitCode !== 0) throw new Error(p.stderr.toString());
  return JSON.parse(p.stdout.toString());
};

test("the host's plain events are instances of the event class their type belongs to", () => {
  const out = run(`
    const key = new Event("keydown"), click = new Event("click"), focus = new Event("focusin");
    const made = new KeyboardEvent("keydown", { key: "a", bubbles: true });
    console.log(JSON.stringify([
      key instanceof KeyboardEvent, key instanceof MouseEvent,
      click instanceof MouseEvent, click instanceof PointerEvent,
      focus instanceof FocusEvent, focus instanceof UIEvent,
      made instanceof KeyboardEvent, made instanceof Event, made.key, made.bubbles,
      window.KeyboardEvent === KeyboardEvent,
    ]));
  `);
  expect(out).toEqual([true, false, true, false, true, true, true, true, "a", true, true]);
});
