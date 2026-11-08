import { expect, test } from "bun:test";
import { Menu, MenuItem, Window, attachHost, dialog, hostEvent, wire } from "./native.js";

const sent = [];
let started = false;
attachHost({
  start: () => (started = true),
  started: () => started,
  send: (msg) => sent.push(msg),
  render: () => {},
});
const tick = () => new Promise((r) => setTimeout(r, 0));

test("templates become the host's items: roles filled in, menu roles expanded, hidden items left out", () => {
  const menu = Menu.buildFromTemplate([
    { role: "appMenu" },
    { label: "File", submenu: [{ label: "Open…", accelerator: "CmdOrCtrl+O" }, { type: "separator" }, { label: "Secret", visible: false }] },
    { role: "editMenu" },
  ]);
  const items = wire(menu, new Map());
  expect(items.map((i) => i.label)).toEqual(["gpuijs", "File", "Edit"]);
  expect(items[0].submenu.map((i) => i.role ?? i.type)).toEqual(
    ["about", "separator", "services", "separator", "hide", "hideOthers", "unhide", "separator", "quit"],
  );
  expect(items[0].submenu.at(-1)).toMatchObject({ label: "Quit gpuijs", accelerator: "CmdOrCtrl+Q", enabled: true });
  expect(items[1].submenu.map((i) => i.label)).toEqual(["Open…", ""]);
  expect(items[1].submenu[0]).toMatchObject({ type: "normal", accelerator: "CmdOrCtrl+O", submenu: [] });
  expect(items[2].submenu.find((i) => i.role === "selectAll").label).toBe("Select All");
});

test("clicks run `click`; checkboxes toggle and radio items take their group's check", async () => {
  new Window({ titleBarStyle: "hidden" });
  const clicks = [];
  const menu = Menu.buildFromTemplate([
    {
      label: "View",
      submenu: [
        { label: "Grid", type: "checkbox", click: (item) => clicks.push(item.checked) },
        { type: "separator" },
        { label: "Small", type: "radio", checked: true },
        { label: "Large", type: "radio" },
      ],
    },
  ]);
  Menu.setApplicationMenu(menu);
  await tick();
  const [view] = sent.findLast((m) => m.t === "menu").items;
  const [grid, , small, large] = view.submenu;

  hostEvent({ t: "event", type: "menu", item: grid.id });
  expect(clicks).toEqual([true]);
  hostEvent({ t: "event", type: "menu", item: large.id });
  const items = menu.items[0].submenu.items;
  expect([items[2].checked, items[3].checked]).toEqual([false, true]);
  // The change goes back to the host, as Electron's live items do.
  await tick();
  const resent = sent.findLast((m) => m.t === "menu").items[0].submenu;
  expect(resent.map((i) => i.checked)).toEqual([true, false, false, true]);
  expect(small.id).toBe(resent[2].id);
});

test("dialogs resolve with the host's answer; window state follows its events", async () => {
  const win = Window.getAllWindows()[0];
  const answer = dialog.showOpenDialog(win, { properties: ["openFile", "multiSelections"] });
  const req = sent.at(-1);
  expect(req).toMatchObject({ t: "dialog", kind: "open", options: { properties: ["openFile", "multiSelections"] } });
  hostEvent({ t: "event", type: "dialog", id: 0, req: req.req, canceled: false, filePaths: ["/tmp/a"] });
  expect(await answer).toEqual({ canceled: false, filePaths: ["/tmp/a"] });

  const seen = [];
  for (const name of ["focus", "maximize", "enter-full-screen"]) win.on(name, () => seen.push(name));
  const bounds = { x: 0, y: 0, width: 800, height: 600 };
  hostEvent({ t: "event", type: "window", name: "focus", focused: true, maximized: false, fullScreen: false, bounds });
  hostEvent({ t: "event", type: "window", name: "resize", focused: true, maximized: true, fullScreen: true, bounds });
  expect(seen).toEqual(["focus", "maximize", "enter-full-screen"]);
  expect([win.isFocused(), win.isMaximized(), win.getSize()]).toEqual([true, true, [800, 600]]);
  expect(() => new Window()).toThrow();
});
