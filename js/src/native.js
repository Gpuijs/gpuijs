// Electron's app shell API, for the one native window gpuijs draws into: `Window` (Electron's
// `BrowserWindow`), `Menu`, `MenuItem`, `app` and `dialog`, with Electron's names and shapes.
// The host carries them out (src/native.rs); what it reports back arrives through `hostEvent`.
import { EventEmitter } from "node:events";

// Set by index.js: opens the window, renders into it, and sends to the host.
let host = null;
export function attachHost(h) {
  host = h;
}

const appName = () => process.env.GPUIJS_APP_NAME ?? "gpuijs";

// --- Window

let current = null;
// What the host last reported: `isMaximized()`, `getBounds()`... answer from it.
let state = { focused: false, maximized: false, fullScreen: false, bounds: { x: 0, y: 0, width: 0, height: 0 } };

export class Window extends EventEmitter {
  /**
   * Opens the window with Electron's `BrowserWindow` options: `width`, `height`, `minWidth`,
   * `minHeight`, `title`, `resizable`, `minimizable`, `titleBarStyle` ("default" | "hidden" |
   * "hiddenInset") and `trafficLightPosition` ({ x, y }).
   */
  constructor(options = {}) {
    super();
    if (current) throw new Error("gpuijs: an app has one Window for now");
    // Something reached the host first (a render, or DOM in an earlier task) and opened a default one.
    if (host.started() && Object.keys(options).length) {
      throw new Error("gpuijs: create the Window before rendering or touching the document");
    }
    current = this;
    this.id = 1;
    this.title = options.title ?? appName();
    host.start(options);
    refreshMenus();
  }

  static getAllWindows() {
    return current ? [current] : [];
  }

  static getFocusedWindow() {
    return current && state.focused ? current : null;
  }

  static fromId(id) {
    return id === 1 ? current : null;
  }

  /** Renders a React element into the window (Electron's `loadURL`, for gpuijs). */
  render(element) {
    host.render(element);
  }

  setTitle(title) {
    this.title = String(title);
    call("setTitle", this.title);
  }
  getTitle() {
    return this.title;
  }
  minimize() {
    call("minimize");
  }
  maximize() {
    call("maximize");
  }
  unmaximize() {
    call("unmaximize");
  }
  isMaximized() {
    return state.maximized;
  }
  setFullScreen(flag) {
    call("setFullScreen", !!flag);
  }
  isFullScreen() {
    return state.fullScreen;
  }
  close() {
    call("close");
  }
  focus() {
    call("focus");
  }
  isFocused() {
    return state.focused;
  }
  getBounds() {
    return { ...state.bounds };
  }
  getSize() {
    return [state.bounds.width, state.bounds.height];
  }
  setSize(width, height) {
    call("setSize", [width, height]);
  }
}

const call = (op, arg = null) => host.send({ t: "win", op, arg });

// The host reports a window event with the window's state; changes in that state are Electron's
// other events.
function windowEvent({ t, id, type, name, ...next }) {
  const prev = state;
  state = next;
  if (!current) return;
  if (name === "close") {
    current.emit("close", { preventDefault() {} });
    current.emit("closed");
    return;
  }
  current.emit(name);
  if (next.maximized !== prev.maximized) current.emit(next.maximized ? "maximize" : "unmaximize");
  if (next.fullScreen !== prev.fullScreen) current.emit(next.fullScreen ? "enter-full-screen" : "leave-full-screen");
  if (name === "focus") app.emit("browser-window-focus", {}, current);
  if (name === "blur") app.emit("browser-window-blur", {}, current);
}

// --- Menu

// Electron's default label and accelerator for each item role (macOS), by lowercased role.
const ROLES = {
  undo: ["undo", "Undo", "CmdOrCtrl+Z"],
  redo: ["redo", "Redo", "Shift+CmdOrCtrl+Z"],
  cut: ["cut", "Cut", "CmdOrCtrl+X"],
  copy: ["copy", "Copy", "CmdOrCtrl+C"],
  paste: ["paste", "Paste", "CmdOrCtrl+V"],
  selectall: ["selectAll", "Select All", "CmdOrCtrl+A"],
  about: ["about", () => `About ${appName()}`],
  hide: ["hide", () => `Hide ${appName()}`, "Command+H"],
  hideothers: ["hideOthers", "Hide Others", "Command+Alt+H"],
  unhide: ["unhide", "Show All"],
  quit: ["quit", () => `Quit ${appName()}`, "CmdOrCtrl+Q"],
  minimize: ["minimize", "Minimize", "CmdOrCtrl+M"],
  zoom: ["zoom", "Zoom"],
  close: ["close", "Close Window", "CmdOrCtrl+W"],
  togglefullscreen: ["togglefullscreen", "Toggle Full Screen", "Ctrl+Command+F"],
  front: ["front", "Bring All to Front"],
  services: ["services", "Services"],
};

// Electron's menu roles: a whole submenu, as macOS shows it.
const MENU_ROLES = {
  appmenu: () => [
    appName(),
    ["about", "separator", "services", "separator", "hide", "hideOthers", "unhide", "separator", "quit"],
  ],
  filemenu: () => ["File", ["close"]],
  editmenu: () => ["Edit", ["undo", "redo", "separator", "cut", "copy", "paste", "selectAll"]],
  viewmenu: () => ["View", ["togglefullscreen"]],
  windowmenu: () => ["Window", ["minimize", "zoom", "separator", "front"]],
};
const roleItem = (role) => (role === "separator" ? { type: "separator" } : { role });

let nextId = 1;

export class MenuItem {
  constructor(options = {}) {
    const key = options.role?.toLowerCase();
    const role = ROLES[key];
    const menuRole = MENU_ROLES[key]?.();
    const label = role?.[1] ?? menuRole?.[0];
    let submenu = options.submenu ?? (menuRole && menuRole[1].map(roleItem));
    if (Array.isArray(submenu)) submenu = Menu.buildFromTemplate(submenu);
    this.commandId = nextId++;
    this.id = options.id;
    this.role = role?.[0] ?? options.role;
    this.type = options.type ?? (submenu ? "submenu" : "normal");
    this.label = options.label ?? (typeof label === "function" ? label() : label) ?? "";
    this.sublabel = options.sublabel ?? "";
    this.accelerator = options.accelerator ?? role?.[2];
    this.submenu = submenu;
    this.click = options.click ?? (() => {});
    this.menu = null;
    this._enabled = options.enabled ?? true;
    this._visible = options.visible ?? true;
    this._checked = options.checked ?? false;
  }

  // Changing these shows at once, as in Electron.
  get enabled() {
    return this._enabled;
  }
  set enabled(v) {
    this._enabled = !!v;
    refreshMenus();
  }
  get visible() {
    return this._visible;
  }
  set visible(v) {
    this._visible = !!v;
    refreshMenus();
  }
  get checked() {
    return this._checked;
  }
  set checked(v) {
    this._checked = !!v;
    refreshMenus();
  }
}

export class Menu extends EventEmitter {
  constructor() {
    super();
    this.items = [];
  }

  static buildFromTemplate(template) {
    const menu = new Menu();
    for (const t of template) menu.append(t instanceof MenuItem ? t : new MenuItem(t));
    return menu;
  }

  static setApplicationMenu(menu) {
    applicationMenu = menu;
    refreshMenus();
  }

  static getApplicationMenu() {
    return applicationMenu;
  }

  append(item) {
    this.insert(this.items.length, item);
  }

  insert(pos, item) {
    item.menu = this;
    this.items.splice(pos, 0, item);
    refreshMenus();
  }

  getMenuItemById(id) {
    for (const item of this.items) {
      if (item.id === id) return item;
      const found = item.submenu?.getMenuItemById(id);
      if (found) return found;
    }
    return null;
  }

  /**
   * Shows the menu as a context menu; `callback` runs once it closes.
   * shortcut: it opens at the pointer; `x`/`y` are ignored.
   */
  popup(options = {}) {
    const items = wire(this, popupItems);
    this.emit("menu-will-show", {});
    request({ t: "popup", items }).then(() => {
      popupItems.clear();
      this.emit("menu-will-close", {});
      options.callback?.();
    });
  }

  closePopup() {}
}

let applicationMenu = null;
let dockMenu = null;
// Items the host may report clicked, by commandId: the application, dock and open context menus'.
const menuItems = new Map();
const dockItems = new Map();
const popupItems = new Map();

/** A menu as the host takes it (src/native.rs `Item`); its items are registered in `into`. */
export function wire(menu, into) {
  return menu.items
    .filter((item) => item.visible)
    .map((item) => {
      into.set(item.commandId, item);
      return {
        id: item.commandId,
        label: item.label,
        type: item.type,
        role: item.role ?? null,
        accelerator: item.accelerator ?? null,
        checked: item.checked,
        enabled: item.enabled,
        submenu: item.submenu ? wire(item.submenu, into) : [],
      };
    });
}

// Sent once per task however many items changed, and only once the window is open.
let refreshing = false;
function refreshMenus() {
  if (refreshing || !host?.started()) return;
  refreshing = true;
  queueMicrotask(() => {
    refreshing = false;
    menuItems.clear();
    dockItems.clear();
    if (applicationMenu) host.send({ t: "menu", items: wire(applicationMenu, menuItems) });
    if (dockMenu) host.send({ t: "dockmenu", items: wire(dockMenu, dockItems) });
  });
}

// A click: checkboxes toggle and radio items take their group's check, as in Electron, then `click`.
function clicked(id) {
  const item = menuItems.get(id) ?? dockItems.get(id) ?? popupItems.get(id);
  if (!item) return;
  if (item.type === "checkbox") item.checked = !item.checked;
  if (item.type === "radio") {
    // A radio group is the run of adjacent radio items around it.
    const siblings = item.menu?.items ?? [item];
    let start = siblings.indexOf(item);
    let end = start;
    while (start > 0 && siblings[start - 1].type === "radio") start--;
    while (end < siblings.length - 1 && siblings[end + 1].type === "radio") end++;
    for (let i = start; i <= end; i++) siblings[i].checked = siblings[i] === item;
  }
  item.click(item, current, { triggeredByAccelerator: false });
}

// --- app

export const app = Object.assign(new EventEmitter(), {
  get name() {
    return appName();
  },
  getName: appName,
  isReady: () => true,
  whenReady: () => Promise.resolve(),
  quit: () => host.send({ t: "app", op: "quit" }),
  hide: () => host.send({ t: "app", op: "hide" }),
  show: () => host.send({ t: "app", op: "show" }),
  focus: () => host.send({ t: "app", op: "focus" }),
  dock: {
    setMenu(menu) {
      dockMenu = menu;
      refreshMenus();
    },
    getMenu: () => dockMenu,
  },
});

// --- dialog

// `dialog.*(window?, options)`: the window argument is optional, as in Electron.
const dialogCall = (kind) => (win, options) =>
  request({ t: "dialog", kind, options: (win instanceof Window || win == null ? options : win) ?? {} });

export const dialog = {
  /** `{ properties: ["openFile" | "openDirectory" | "multiSelections"], buttonLabel }` → `{ canceled, filePaths }`. */
  showOpenDialog: dialogCall("open"),
  /** `{ defaultPath }` → `{ canceled, filePath }`. */
  showSaveDialog: dialogCall("save"),
  /** `{ type, message, detail, buttons }` → `{ response, checkboxChecked }`. */
  showMessageBox: dialogCall("message"),
};

// --- host replies

const pending = new Map();
let nextReq = 1;
function request(msg) {
  const req = nextReq++;
  return new Promise((resolve) => {
    pending.set(req, resolve);
    host.send({ ...msg, req });
  });
}

/** A host event for the shell; false if it is the page's (input and the like). */
export function hostEvent(msg) {
  switch (msg.type) {
    case "menu":
      clicked(msg.item);
      return true;
    case "menu-closed":
    case "dialog": {
      const { t, id, type, req, ...result } = msg;
      pending.get(req)?.(result);
      pending.delete(req);
      return true;
    }
    case "window":
      windowEvent(msg);
      return true;
    case "app":
      app.emit(msg.name, {}, false);
      return true;
  }
  return false;
}
