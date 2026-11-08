# Dialogs and app

`dialog` shows native open panels, save panels and alerts. `app` controls the application. Both follow Electron's API.

## `dialog`

Every dialog function returns a Promise. As in Electron, the first argument (the parent `Window`) is optional: `dialog.showOpenDialog(win, options)` and `dialog.showOpenDialog(options)` both work.

### `showOpenDialog`

```tsx
import { dialog } from "@gpuijs/core"

const { canceled, filePaths } = await dialog.showOpenDialog({
  properties: ["openFile", "multiSelections"],
  buttonLabel: "Import",
})
if (!canceled) importFiles(filePaths)
```

| Option | Description |
| --- | --- |
| `properties` | An array of `"openFile"`, `"openDirectory"` and `"multiSelections"`. With neither `openFile` nor `openDirectory`, files can be chosen. |
| `buttonLabel` | The confirm button's label |

It resolves to `{ canceled: boolean, filePaths: string[] }`.

### `showSaveDialog`

```tsx
const { canceled, filePath } = await dialog.showSaveDialog({ defaultPath: "Untitled.md" })
if (!canceled) await Bun.write(filePath, text)
```

| Option | Description |
| --- | --- |
| `defaultPath` | The suggested path. A bare file name starts in the home folder, as in Electron. |

It resolves to `{ canceled: boolean, filePath: string }`.

### `showMessageBox`

```tsx
const { response } = await dialog.showMessageBox(win, {
  type: "warning",
  message: "Delete this note?",
  detail: "You can't undo this.",
  buttons: ["Delete", "Cancel"],
})
if (response === 0) deleteNote()
```

| Option | Description |
| --- | --- |
| `type` | `"info"` (the default), `"warning"` or `"error"` |
| `message` | The main text |
| `detail` | Secondary text |
| `buttons` | Button labels. Defaults to `["OK"]`. |

It resolves to `{ response: number, checkboxChecked: false }`, where `response` is the index of the button clicked.

### Reading and writing files

The app runs under Bun, so after a dialog you can use Bun's and Node's file APIs directly:

```tsx
const { filePaths } = await dialog.showOpenDialog({ properties: ["openFile"] })
const text = await Bun.file(filePaths[0]).text()
```

Node APIs (`node:fs`, `node:path`, `node:child_process`), `fetch`, `Bun.file` and `Bun.write` are all available in the app process. There's no sandbox between your UI code and the system.

## `app`

```tsx
import { app } from "@gpuijs/core"

app.on("activate", () => win.focus()) // the Dock icon was clicked
console.log(app.getName())
```

| Member | Description |
| --- | --- |
| `app.name`, `app.getName()` | The app's name. See [Project structure](project.html#package-json). |
| `app.quit()` | Quits the app |
| `app.hide()` | Hides the app (⌘H) |
| `app.show()`, `app.focus()` | Activates the app and brings it to the front |
| `app.isReady()` | Always `true` |
| `app.whenReady()` | A resolved Promise, so Electron-style startup code works unchanged |
| `app.dock.setMenu(menu)`, `app.dock.getMenu()` | The Dock menu. See [Menus](menus.html#dock-menu). |

### Events

`app` is a Node `EventEmitter`.

| Event | When |
| --- | --- |
| `activate` | The Dock icon was clicked while the app was running |
| `browser-window-focus` | The window gained focus. Listeners get `(event, window)`. |
| `browser-window-blur` | The window lost focus. Listeners get `(event, window)`. |
