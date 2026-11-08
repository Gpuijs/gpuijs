# Menus

`Menu` and `MenuItem` follow Electron's API. Build menus from templates, set the application menu and the Dock menu, and pop up native context menus.

```tsx
import { Menu, app } from "gpuijs"

Menu.setApplicationMenu(
  Menu.buildFromTemplate([
    { role: "appMenu" },
    {
      label: "File",
      submenu: [
        { label: "New Note", accelerator: "CmdOrCtrl+N", click: () => createNote() },
        { label: "Open…", accelerator: "CmdOrCtrl+O", click: () => openFile() },
        { type: "separator" },
        { role: "close" },
      ],
    },
    { role: "editMenu" },
    {
      label: "View",
      submenu: [
        { id: "sidebar", label: "Show Sidebar", type: "checkbox", checked: true, click: (item) => setSidebar(item.checked) },
        { role: "togglefullscreen" },
      ],
    },
    { role: "windowMenu" },
  ]),
)
```

## Application menu

`Menu.setApplicationMenu(menu)` sets the macOS menu bar. On macOS the first top-level menu is the app menu, whatever its label, so start with `{ role: "appMenu" }` or a submenu of your own.

`Menu.getApplicationMenu()` returns the menu you set, or `null`.

The menu bar is updated once per task, however many items changed, and only once the window is open. You can call `setApplicationMenu` before `new Window()`.

## Menu items

A template entry is a plain object with `MenuItem` options, or a `MenuItem` instance.

| Option | Type | Description |
| --- | --- | --- |
| `label` | string | The item's text |
| `type` | `"normal"` \| `"separator"` \| `"submenu"` \| `"checkbox"` \| `"radio"` | Defaults to `"submenu"` with a `submenu`, else `"normal"` |
| `click` | `(item, window, event) => void` | Runs when the item is chosen. `event` is `{ triggeredByAccelerator: false }`. |
| `accelerator` | string | A keyboard shortcut, like `"CmdOrCtrl+Shift+S"`. See below. |
| `role` | string | A predefined action. See [Roles](#roles). |
| `submenu` | template array or `Menu` | A nested menu |
| `enabled` | boolean | Defaults to `true`. Disabled items are greyed out. |
| `visible` | boolean | Defaults to `true`. Hidden items are left out. |
| `checked` | boolean | For `checkbox` and `radio` items |
| `id` | string | For `menu.getMenuItemById(id)` |
| `sublabel` | string | Kept on the item, but not shown |

### Changing items

`enabled`, `visible` and `checked` can be changed after the menu is set, and the change shows at once, as in Electron:

```tsx
const menu = Menu.getApplicationMenu()
menu.getMenuItemById("sidebar").checked = false
menu.getMenuItemById("save").enabled = hasChanges
```

### Checkboxes and radio groups

Choosing a `checkbox` item toggles `checked` before `click` runs. Choosing a `radio` item checks it and unchecks the other radio items in its group. A group is the run of adjacent radio items around it.

```tsx
{
  label: "Sort By",
  submenu: [
    { label: "Name", type: "radio", checked: true, click: () => sort("name") },
    { label: "Date", type: "radio", click: () => sort("date") },
    { label: "Size", type: "radio", click: () => sort("size") },
  ],
}
```

## Accelerators

Accelerators use Electron's syntax: modifiers and a key, joined by `+`.

| Modifier | Meaning on macOS |
| --- | --- |
| `Command`, `Cmd`, `CommandOrControl`, `CmdOrCtrl`, `Super`, `Meta` | ⌘ |
| `Control`, `Ctrl` | ⌃ |
| `Alt`, `Option`, `AltGr` | ⌥ |
| `Shift` | ⇧ |

Keys can be letters, digits, punctuation (`,`, `.`, `/`, ...), `F1`–`F24`, `Plus`, `Space`, `Tab`, `Backspace`, `Delete`, `Enter`/`Return`, `Esc`/`Escape`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp` and `PageDown`. Accelerators with other keys (media keys) or modifiers (`Hyper`) are ignored.

An accelerator works while the window is focused, even when the menu isn't open, and shows next to the item in the menu.

## Roles

A `role` gives an item Electron's default label and accelerator, and the action is carried out natively. Write roles in any case (`selectAll` or `selectall`).

| Role | Label | Accelerator |
| --- | --- | --- |
| `undo` | Undo | ⌘Z |
| `redo` | Redo | ⇧⌘Z |
| `cut` | Cut | ⌘X |
| `copy` | Copy | ⌘C |
| `paste` | Paste | ⌘V |
| `selectAll` | Select All | ⌘A |
| `about` | About *App* | |
| `hide` | Hide *App* | ⌘H |
| `hideOthers` | Hide Others | ⌥⌘H |
| `unhide` | Show All | |
| `quit` | Quit *App* | ⌘Q |
| `minimize` | Minimize | ⌘M |
| `zoom` | Zoom | |
| `close` | Close Window | ⌘W |
| `togglefullscreen` | Toggle Full Screen | ⌃⌘F |
| `front` | Bring All to Front | |
| `services` | Services | |

You can override a role's `label` or `accelerator`.

### Menu roles

These roles expand to a whole standard submenu:

| Role | Menu |
| --- | --- |
| `appMenu` | *App*: About, Services, Hide, Hide Others, Show All, Quit |
| `fileMenu` | File: Close Window |
| `editMenu` | Edit: Undo, Redo, Cut, Copy, Paste, Select All |
| `viewMenu` | View: Toggle Full Screen |
| `windowMenu` | Window: Minimize, Zoom, Bring All to Front |

*App* is the app's name. See [Project structure: package.json](project.html#package-json).

## Dock menu

```tsx
app.dock.setMenu(
  Menu.buildFromTemplate([
    { label: "New Note", click: () => createNote() },
    { label: "Quick Capture", click: () => capture() },
  ]),
)
```

`app.dock.setMenu(menu)` sets the menu shown when the Dock icon is right-clicked. `app.dock.getMenu()` returns it.

## Context menus

Build the menu and pop it up from a `contextmenu` handler:

```ts
const showNoteMenu = (note) =>
  Menu.buildFromTemplate([
    { label: "Rename", click: () => rename(note) },
    { label: "Duplicate", click: () => duplicate(note) },
    { type: "separator" },
    { label: "Delete", click: () => remove(note) },
  ]).popup({ callback: () => console.log("menu closed") })
```

```tsx
<div onContextMenu={() => showNoteMenu(note)}>{note.title}</div>      // React, Solid
<div @contextmenu="showNoteMenu(note)">{{ note.title }}</div>        <!-- Vue -->
```

`menu.popup(options?)` shows the menu as a native context menu. `options.callback` runs once it closes. The menu emits `menu-will-show` when it opens and `menu-will-close` when it closes.

> [!NOTE]
> The context menu always opens at the pointer. The `x`, `y` and `window` options are ignored. `menu.closePopup()` does nothing.

## `Menu` API

| Member | Description |
| --- | --- |
| `Menu.buildFromTemplate(template)` | A menu from template objects or `MenuItem`s |
| `Menu.setApplicationMenu(menu)` | Sets the menu bar |
| `Menu.getApplicationMenu()` | The menu bar's menu, or `null` |
| `new Menu()` | An empty menu |
| `menu.items` | Its `MenuItem`s |
| `menu.append(item)` | Adds an item at the end |
| `menu.insert(pos, item)` | Inserts an item at `pos` |
| `menu.getMenuItemById(id)` | Finds an item by `id`, searching submenus |
| `menu.popup(options)` | Shows it as a context menu |

## `MenuItem` API

`new MenuItem(options)` takes the [options above](#menu-items). An item has `label`, `type`, `role`, `accelerator`, `submenu`, `click`, `id`, `sublabel`, `enabled`, `visible`, `checked` and `menu` (the menu it's in).
