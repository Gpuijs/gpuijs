# Window

The shell API follows [Electron](https://www.electronjs.org/docs/latest/api/browser-window)'s, with `Window` in place of `BrowserWindow`. If you know Electron, you already know it. The host carries out each call with native AppKit windows through GPUI.

```tsx
import { Window, render } from "@gpuijs/core/react" // or gpuijs/vue, gpuijs/solid

const win = new Window({
  width: 1200,
  height: 800,
  minWidth: 720,
  minHeight: 480,
  title: "Notes",
  titleBarStyle: "hiddenInset",
})

win.on("focus", () => console.log("focused"))
render(<App />) // renders into win
```

> [!WARNING]
> An app has **one window** for now. A second `new Window()` throws `gpuijs: an app has one Window for now`.

## Creating the window

Create the `Window` **before** rendering or touching `document`. Anything that needs the window first (`render`, `document.body`, a `ResizeObserver`) opens a default one, and a `new Window(options)` after that throws:

```text
gpuijs: create the Window before rendering or touching the document
```

Importing the entry point doesn't open the window, so `import { Window } from "@gpuijs/core/vue"` (or `react`, `solid`) before `new Window()` is fine.

If the app never creates one, `render` opens a default window: 1024×768, centered, resizable, with a standard title bar titled with the app's name.

## Options

| Option | Type | Default | Description |
| --- | --- | --- | --- |
| `width` | number | `1024` | Content width in points |
| `height` | number | `768` | Content height in points |
| `minWidth` | number | none | Minimum width while resizing |
| `minHeight` | number | none | Minimum height while resizing |
| `title` | string | app name | The title shown in the title bar and Window menu |
| `resizable` | boolean | `true` | Whether the user can resize the window |
| `minimizable` | boolean | `true` | Whether the window can be minimized |
| `titleBarStyle` | `"default"` \| `"hidden"` \| `"hiddenInset"` | `"default"` | `hidden` removes the title bar and runs content under the traffic lights. `hiddenInset` does the same with the traffic lights moved in to (20, 20). |
| `trafficLightPosition` | `{ x, y }` | none | Where the traffic lights sit with a hidden title bar |

The window opens centered on the main display. `GPUIJS_WINDOW_SIZE=WxH` overrides `width` and `height`, which is useful for screenshots and benchmarks.

## Methods

| Method | Description |
| --- | --- |
| `render(element)` | React only: renders a React element into the window, like the `render` export (Electron's `loadURL`, for gpuijs). Vue and Solid apps use their entry point's `render`. |
| `setTitle(title)` | Sets the title |
| `getTitle()` | The current title |
| `minimize()` | Minimizes the window to the Dock |
| `maximize()` | Zooms the window, if it isn't already maximized |
| `unmaximize()` | Un-zooms the window, if it's maximized |
| `isMaximized()` | Whether it's maximized |
| `setFullScreen(flag)` | Enters or leaves full screen |
| `isFullScreen()` | Whether it's in full screen |
| `focus()` | Brings the window to the front and focuses it |
| `isFocused()` | Whether it's the key window |
| `close()` | Closes the window, which quits the app |
| `getBounds()` | `{ x, y, width, height }` |
| `getSize()` | `[width, height]` |
| `setSize(width, height)` | Resizes the content area |

State queries (`isMaximized`, `isFullScreen`, `isFocused`, `getBounds`, `getSize`) answer from the state the host last reported, so they're synchronous and cheap. They're current as of the last window event.

### Static methods

| Method | Description |
| --- | --- |
| `Window.getAllWindows()` | `[win]`, or `[]` before a window exists |
| `Window.getFocusedWindow()` | The window if it's focused, else `null` |
| `Window.fromId(id)` | The window for `id` (always `1`), else `null` |

`win.id` is always `1`.

## Events

`Window` is a Node `EventEmitter`. Use `on`, `once` and `off`.

| Event | When |
| --- | --- |
| `ready-to-show` | The window has opened |
| `focus` | The window became key |
| `blur` | The window stopped being key |
| `resize` | The window's size or position changed |
| `maximize`, `unmaximize` | The maximized state changed |
| `enter-full-screen`, `leave-full-screen` | Full screen was entered or left |
| `close` | The window is closing. The listener gets an event with a no-op `preventDefault()`, because closing can't be cancelled yet. |
| `closed` | Right after `close` |

```tsx
win.on("resize", () => {
  const { width, height } = win.getBounds()
  saveWindowSize(width, height)
})
```

Focus and blur also emit `browser-window-focus` and `browser-window-blur` on [`app`](dialogs.html#app).

## Window lifetime

Closing the window quits the app. The host quits when its window closes, which closes the socket to the JavaScript process, which then exits. Likewise, if the JavaScript process exits or crashes, the host quits.

## Visibility

When the window is minimized, fully covered or on another Space, `document.visibilityState` becomes `"hidden"` and `document` hears `visibilitychange`. `requestAnimationFrame` callbacks pause until the window shows again, as browsers pause background tabs. See [`visibilityState()`](api.html#visibilitystate).

## Related

- [Custom title bars](title-bar.html): drawing your own title bar with `titleBarStyle: "hidden"` and `app-region: drag`.
- [Menus](menus.html): the menu bar, Dock menu and context menus.
- [Dialogs and app](dialogs.html): open and save panels, message boxes, and the `app` object.
