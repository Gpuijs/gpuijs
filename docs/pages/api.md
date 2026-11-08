# API reference

Everything the `gpuijs` package exports. Apps import from their framework's entry point, which re-exports everything below:

| Import | What it is |
| --- | --- |
| `gpuijs/react` | The browser environment, React's `render`, and everything below |
| `gpuijs/vue` | The browser environment, Vue's `render`, and everything below |
| `gpuijs/solid` | The browser environment, Solid's `render`, and everything below |
| `gpuijs` | Everything below, without `render` or the browser environment |
| `gpuijs/env` | Only the browser environment: a side-effect module that defines browser globals. See [Browser environment](environment.html). |

```ts
import {
  render, body, activeElement, head, createStyleElement,
  createElement, createTextNode, createDocumentFragment,
  ResizeObserver, requestAnimationFrame, cancelAnimationFrame, visibilityState,
  setRootClass, setRootAttribute, prefersDark,
  measureText, assetPath, fetchAsset,
  Window, Menu, MenuItem, app, dialog,
} from "gpuijs/react" // or gpuijs/vue, gpuijs/solid
```

Most of the document-like exports are what the browser environment uses to build `document` and `window`. Apps can use the globals instead.

## Rendering

### `render`

```ts
// gpuijs/react
render(element: React.ReactNode): void
// gpuijs/vue
render(app: Component | App, props?: object): App
// gpuijs/solid
render(code: () => JSX.Element): () => void
```

Renders into the app's window, opening a default `Window` if the app made none. React's `render` updates the root when called again. Vue's takes a component, or an app from `createApp` with its plugins installed, and returns the mounted app. Solid's returns a function that unmounts. See [Rendering](rendering.html) and [Frameworks](frameworks.html).

### `body`

```ts
body(): HostNode
```

The `<body>` host node. In React, portal into it with `createPortal(children, body())`. `document.body` returns the same node.

### `activeElement`

```ts
activeElement(): HostNode
```

The focused host node, or `<body>` when nothing is focused. `document.activeElement` returns the same.

## Root element

### `setRootClass`

```ts
setRootClass(className: string): void
```

Sets the `class` attribute of the root `<html>` node, replacing what was there. Use it for class-based dark mode: `setRootClass(dark ? "dark" : "")`.

### `setRootAttribute`

```ts
setRootAttribute(name: string, value: string | null): void
```

Sets an attribute on `<html>`, or removes it when `value` is `null`. For example, `setRootAttribute("data-theme", "dark")`.

### `prefersDark`

```ts
prefersDark(): boolean
```

Whether the macOS appearance is dark. `window` hears an `appearance` event when it changes. `(prefers-color-scheme: dark)` media queries follow the same setting.

## Styles

### `head`

```ts
head(): HeadNode
```

`document.head`. `<style>` elements appended to it style the window after the app's CSS. It supports `appendChild`, `insertBefore` (which appends), `prepend`, `removeChild` and `children`.

### `createStyleElement`

```ts
createStyleElement(): StyleNode
```

`document.createElement("style")`. Set its CSS through `textContent`, `innerHTML`, `appendChild(textNode)` or `sheet.insertRule()`. It applies while it's in `head`. See [Styling: CSS-in-JS](styling.html#css-in-js-and-runtime-styles).

## Library-built DOM

### `createElement`

```ts
createElement(tag: string, svg?: boolean): HostNode
```

`document.createElement(tag)`, or `document.createElementNS(svgNamespace, tag)` when `svg` is true. It returns a host node outside React's control, which shows once appended to a node in the window. See [Rendering: library-built DOM](rendering.html#library-built-dom).

### `createTextNode`

```ts
createTextNode(text: string): HostNode
```

`document.createTextNode`.

### `createDocumentFragment`

```ts
createDocumentFragment(): Fragment
```

`document.createDocumentFragment`. Appending a fragment moves its children.

## Observing and scheduling

### `ResizeObserver`

```ts
new ResizeObserver(callback: (entries, observer) => void)
```

`ResizeObserver` over host nodes, with `observe`, `unobserve` and `disconnect`. Entries carry `target`, `contentRect`, `borderBoxSize` and `contentBoxSize`, all measuring the **border box**. See [Layout: ResizeObserver](layout.html#resizeobserver).

### `requestAnimationFrame`

```ts
requestAnimationFrame(callback: (time: number) => void): number
```

Runs `callback` about one display frame (16 ms) later. All callbacks requested for a frame run together with the same timestamp, and one throwing doesn't stop the rest. Callbacks don't run while the window is hidden. They run once it shows again.

### `cancelAnimationFrame`

```ts
cancelAnimationFrame(id: number): void
```

### `visibilityState`

```ts
visibilityState(): "visible" | "hidden"
```

`"hidden"` while the window is minimized, covered or on another Space. `document` hears `visibilitychange` when it changes.

## Text and assets

### `measureText`

```ts
measureText(text: string, style?: { fontSize?, fontWeight?, letterSpacing? }): { width: number, height: number }
```

The size of one line of `text` in px, synchronously. `fontSize` defaults to 16. A `fontWeight` of `"bold"` or 600 and above uses bold advances. `height` is `round(fontSize × 1.2)`. It always uses the system UI font, and characters outside ASCII use an average advance. See [Layout: measuring text](layout.html#measuring-text).

### `assetPath`

```ts
assetPath(src: string): string
```

Resolves an image URL the way `<img src>` does. A root-relative or relative URL becomes the file under `public/` (if it exists). A remote URL becomes its cached download (once fetched). Anything else passes through unchanged.

### `fetchAsset`

```ts
fetchAsset(url: string): Promise<string | null>
```

Downloads a remote URL into the asset cache. It resolves to the local file path, or `null` if the download fails. Concurrent calls share one download, and failures are retried on the next call. See [Images: remote images](assets.html#remote-images).

## Shell

| Export | Reference |
| --- | --- |
| `Window` | [Window](window.html) |
| `Menu`, `MenuItem` | [Menus](menus.html) |
| `app` | [Dialogs and app: app](dialogs.html#app) |
| `dialog` | [Dialogs and app: dialog](dialogs.html#dialog) |

## Host nodes

A ref to a host element, `document.body`, and nodes from `createElement` are host nodes. They implement this subset of the DOM:

### Identity and tree

| Member | Notes |
| --- | --- |
| `nodeType` | `1` for elements, `3` for text |
| `tagName`, `nodeName` | Uppercase for HTML (`"DIV"`), as written for SVG (`"svg"`, `"linearGradient"`) |
| `namespaceURI` | XHTML or SVG |
| `id`, `className` | Read and write |
| `parentNode`, `parentElement` | |
| `childNodes`, `children` | `children` leaves out text nodes |
| `firstChild`, `lastChild`, `nextSibling`, `previousSibling` | |
| `ownerDocument` | `document` |
| `isConnected` | Whether it's under `<html>` |
| `getRootNode()` | `document` when connected |
| `contains(other)` | |
| `compareDocumentPosition(other)` | `Node.DOCUMENT_POSITION_*` flags |
| `textContent` | Read anywhere. Writable on library-built nodes. |
| `innerHTML` | Writable (parsed into nodes). Reads return what was last written. |

### Queries

`querySelector`, `querySelectorAll`, `closest`, `matches`, `getElementsByTagName` and `getElementsByClassName`. The selector engine supports tag, `#id`, `.class`, `[attr]`, `[attr="v"]`, `:not(...)`, `:first-child`, `:last-child`, `:only-child`, escaped names (`data\:x`), the descendant, `>`, `+` and `~` combinators, and comma lists.

These selectors run in JavaScript over host nodes, not against the stylesheet. They're separate from the host's CSS selector support, which is [much broader](css.html#selectors).

### Attributes

| Member | Notes |
| --- | --- |
| `getAttribute(name)`, `hasAttribute(name)` | Read from the node's props, which are kept under React's names: `class`, `for`, `colspan` and `tabindex` map to `className`, `htmlFor` and so on. |
| `setAttribute(name, value)`, `setAttributeNS` | On a React node, the value stays until React renders a new one for that prop |
| `removeAttribute(name)` | |
| `dataset` | Live: `el.dataset.state = "open"` sets `data-state` |
| `style` | `el.style.foo = ...`, `setProperty`, `removeProperty`, `cssText` |

### Geometry

`getBoundingClientRect()`, `getClientRects()`, `offsetWidth`, `offsetHeight`, `offsetTop`, `offsetLeft`, `offsetParent`, `clientWidth`, `clientHeight`, `clientTop`, `clientLeft`, `scrollWidth` and `scrollHeight`. SVG elements also have `getBBox()` and `getComputedTextLength()`. Reads may force a layout. See [Layout: reading sizes](layout.html#reading-sizes-from-javascript).

`scrollTop` and `scrollLeft` are always 0. `scrollTo()` and `scrollIntoView()` do nothing.

### Events, focus and forms

| Member | Notes |
| --- | --- |
| `addEventListener(type, listener, options)` | `once` and `signal` are supported. Listening for pointer or key events makes the host report them for this node. |
| `removeEventListener(type, listener)` | |
| `dispatchEvent(event)` | Runs listeners on the node and, if `event.bubbles`, its ancestors |
| `click()` | Dispatches a click as if the user clicked |
| `setPointerCapture()`, `releasePointerCapture()`, `hasPointerCapture()` | Route pointer moves and the release to this node until the release |
| `focus()`, `blur()` | Runs `onFocus`/`onBlur`, and focuses the native field for inputs |
| `value` | A text field's current text. Writing it updates the field. |
| `checked` | A checkbox's or radio's state |
| `getAnimations()` | Always `[]`, so exit animations that wait on Web Animations finish at once |

### Editing (library-built nodes)

`appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `replaceChildren`, `insertAdjacentElement`, `remove` and `cloneNode`. On React-owned nodes these only move library-built nodes. See [Rendering: library-built DOM](rendering.html#library-built-dom).
