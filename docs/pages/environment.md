# Browser environment

A gpuijs app runs under Bun, not in a browser, so there's no `window` or `document`. Many libraries touch them anyway, often as soon as they load, and Vue and Solid build their UI through `document`. Every entry point (`@gpuijs/core/react`, `@gpuijs/core/vue`, `@gpuijs/core/solid`) provides just enough of both, backed by the native tree:

```tsx
import { render } from "@gpuijs/core/react" // first, before anything that might touch window or document
```

Import the entry point first in `src/main.*`. ES modules evaluate in import order, so the globals then exist before any library module runs. `@gpuijs/core/env` sets up the same globals on its own, without `render`.

> [!NOTE]
> These globals aren't a DOM. Elements are gpuijs host nodes (see [API reference: host nodes](api.html#host-nodes)), and the environment only fills in the globals around them. Each shim exists because a real library needs it, and the source names which one.

## `window`

`window` is `globalThis`, and it's an event target (`addEventListener`, `removeEventListener`, `dispatchEvent`).

| Member | Behaviour |
| --- | --- |
| `innerWidth`, `innerHeight` | The window's content size (1024×768 until the first layout) |
| `scrollX`, `scrollY`, `pageXOffset`, `pageYOffset` | How far `<body>` has scrolled within the window |
| `resize` event | Fires after each layout that changes the window size, including once right after the first |
| `appearance` event | Fires when the macOS appearance switches between light and dark |
| `popstate` event | Fires on `history.back()`, `forward()` and `go()` |
| `matchMedia(query)` | See below |
| `getComputedStyle(el)` | See below |
| `requestAnimationFrame`, `cancelAnimationFrame` | Paused while the window is hidden |
| `ResizeObserver` | Over host nodes. See [Layout](layout.html#resizeobserver). |
| `MutationObserver` | Reports child, text and attribute changes to host nodes in a microtask |
| `getSelection()`, `Range`, `StaticRange` | The caret and selection in editable content. See [Elements: editable content](elements.html#editable-content). |
| `DOMParser` | Parses markup into detached host nodes |
| `scrollTo()` | Does nothing |
| `localStorage` | Persistent. See below. |
| `location`, `history` | An in-memory history. See below. |
| `navigator.language` | `"en-US"` unless something set it |
| `Image` | An `<img>` element: loads its `src`, fires `load` or `error`, and reports `naturalWidth`/`naturalHeight` |
| `WebGLRenderingContext`, `WebGL2RenderingContext` | And the `WebGL*` object classes, for `instanceof` checks. See [Elements: canvas](elements.html#canvas). |

### `matchMedia`

| Query | `matches` |
| --- | --- |
| `(prefers-color-scheme: dark)` / `(prefers-color-scheme: light)` | Follows the macOS appearance, and listeners hear changes |
| `(hover: hover)`, `(pointer: fine)`, and `any-` forms | `true`, because a desktop pointer is a mouse |
| Anything else | `false` |

Lists support `addEventListener("change", ...)`, `removeEventListener` and the deprecated `addListener`/`removeListener` (MUI still calls them). Only `prefers-color-scheme` lists ever fire.

Width-based media queries in CSS respond to the window size. `matchMedia("(min-width: 768px)")` from JavaScript always reports `false`. Read `window.innerWidth` and listen for `resize` instead.

### `getComputedStyle`

The cascade runs in the host, so `getComputedStyle(el)` can't report every computed value. It answers with what layout libraries read:

- `width`, `height`: the measured size, laid out on the spot if needed (MUI X Charts sizes itself from these).
- `paddingTop`/`Right`/`Bottom`/`Left` and `border*Width`: from the measured box.
- `position`: the element's inline `position`, else `"static"`.
- `transform`: a `matrix(1, 0, 0, 1, x, y)` for an inline px translation (vaul reads drag offsets this way), else `"none"`.
- Everything else gets CSS initial values: `display: "block"`, `overflow: "visible"`, `direction: "ltr"`, `filter: "none"` and so on. `getPropertyValue()` returns `""`.

## `document`

`document` is an event target too.

| Member | Behaviour |
| --- | --- |
| `documentElement` | The `<html>` node. `classList.add`/`remove`/`contains` and `setAttribute`/`removeAttribute`/`getAttribute`/`hasAttribute` write through to the host, so `.dark` and `[data-theme]` selectors match. |
| `body` | The `<body>` node. Portals mount here. |
| `head` | Takes `<style>` elements. See [Styling](styling.html#css-in-js-and-runtime-styles). |
| `activeElement` | The focused host node, or `body` |
| `visibilityState`, `hidden` | `"hidden"` while the window is minimized, covered or on another Space. `visibilitychange` fires on `document`. |
| `createElement(tag)`, `createElementNS(ns, tag)` | Host nodes for library-built DOM. `createElement("style")` makes a style element. |
| `createTextNode`, `createDocumentFragment` | For library-built DOM |
| `createTreeWalker(root, show, filter)` | Walks elements under `root` with `nextNode()` (Radix FocusScope uses it) |
| `createEvent()` | A plain `Event` |
| `getElementById(id)` | Style elements in `head` by id, then elements under `body` |
| `querySelector`, `querySelectorAll` | Return `null` and `[]`. Query from `document.body` instead. |
| `getElementsByTagName` | Returns `[]` |
| `contains(node)` | Whether the node is in the window |
| `getRootNode()` | `document` |
| `defaultView` | `window` |
| `styleSheets` | `[]` |
| `cookie` | An in-memory cookie jar for this run. `max-age=0` deletes. |
| `getSelection()` | The window's selection |
| `createRange()` | A `Range` |
| `implementation.createHTMLDocument()` | A detached document for building nodes |

### Capture listeners

`document.addEventListener(type, fn, { capture: true })` (or `true`) registers a capture listener. Capture listeners run **before** the framework's handlers, as in a browser's capture phase. Radix Select relies on this to cancel the release of the press that opened it. See [Events: propagation order](events.html#propagation-order).

## `localStorage`

`localStorage` has the full `Storage` API (`getItem`, `setItem`, `removeItem`, `clear`, `key`, `length`), and values are stored as strings.

- **In development** it's in memory and starts empty each run.
- **In a packaged app** it persists in `~/Library/Application Support/<bundle id>/localStorage.json`. Each write is saved to a temporary file and renamed, so a crash never leaves a half-written store. An unreadable file is ignored with a warning.

`sessionStorage` and IndexedDB aren't provided.

## History and location

There's no address bar, so `history` is a list of entries in memory. `location` is a `URL` object on `http://localhost/`.

- `pushState(state, title, url)` and `replaceState(...)` update the list and `location`. As in the DOM, `url` is optional and resolves against the current URL.
- `back()`, `forward()` and `go(n)` move through the list and dispatch `popstate` on `window`.
- `history.length`, `history.state` and `scrollRestoration` are available.

Client-side routers built on the History API, like react-router, work on top of this. To open the app at a route during development, set `ROUTE`:

```sh
ROUTE=/settings/profile gpuijs dev .
```

## Classes for `instanceof` checks

Libraries test values with `instanceof`, so the environment defines these classes:

| Class | Behaviour |
| --- | --- |
| `Node`, `Element`, `HTMLElement` | `instanceof` is true for any host element. Their prototypes forward `querySelector`, `querySelectorAll`, `closest`, `matches`, `contains` and `getBoundingClientRect` to the node, for code that calls `Element.prototype.closest.call(el, ...)`. `Node` carries the `ELEMENT_NODE`, `TEXT_NODE` and `DOCUMENT_POSITION_*` constants. |
| `HTMLInputElement`, `HTMLSelectElement`, `HTMLTextAreaElement` | Empty classes with no-op `value`/`checked` setters, for Radix's hidden native inputs. Host nodes are never instances. |
| `FileList`, `HTMLFormElement`, `ShadowRoot` | Empty classes, so zod, Radix and MUI X can test against them |
| `DocumentFragment` | For Radix Select's detached item collection |
| `NodeFilter` | `SHOW_ELEMENT` and the `FILTER_*` constants |

## What isn't there

- Bun already provides `fetch`, `URL`, `TextEncoder`, `crypto`, timers, `EventTarget`, `Event`, `AbortController`, `structuredClone`, `WebSocket`, `Worker` and `BroadcastChannel`, so the environment leaves them alone.
- There's no `XMLHttpRequest` (axios falls back to `fetch` through its adapter), `IntersectionObserver`, `sessionStorage`, IndexedDB, `navigator.clipboard`, or `Notification`.
- Bun's `alert()`, `confirm()` and `prompt()` read from the terminal, not the window. Use [`dialog.showMessageBox`](dialogs.html#showmessagebox) instead.
