# Rendering

Your framework renders into gpuijs **host nodes**, not DOM nodes. React does it through gpuijs's own React renderer. Vue and Solid do it through their stock DOM runtimes, which call `document.createElement` and friends on host nodes. Components, state and reactivity work exactly as they do in the browser. Only the host side differs: nodes live in the native host's tree.

## Rendering into the window

Each entry point exports a `render` that renders into the app's window and opens a default one if the app hasn't created one:

```tsx
import { render } from "@gpuijs/core/react"
render(<App />)

import { render } from "@gpuijs/core/vue"
render(App) // or render(createApp(App).use(router))

import { render } from "@gpuijs/core/solid"
render(() => <App />)
```

To choose the window's size and look, create a `Window` before calling `render`. [Frameworks](frameworks.html) covers each entry point.

With React, call `render` again with a new element to update the root, as with `root.render()` in `react-dom`. `win.render(element)` on the `Window` does the same.

### When rendering starts

The host first sends its font metrics, which [`measureText`](api.html#measuretext) needs. React rendering waits for them: a `render` call made before they arrive is held and runs once they do, so you don't need to wait for anything yourself.

## The root

The app renders into `<body>`, which sits inside `<html>`. Both exist from the start.

- `<html>` fills the window and is the scrolling viewport. Classes and attributes set on it with [`setRootClass`](api.html#setrootclass) and [`setRootAttribute`](api.html#setrootattribute) (or through `document.documentElement`) are what `.dark &` and `[data-theme]` selectors match.
- `<body>` gets the UA stylesheet's 8px margin, as in a browser. Reset it in your CSS (Tailwind's preflight does).

## React: concurrent rendering

The React root is a **concurrent root**, as `createRoot` makes in `react-dom`. Transitions, `useDeferredValue` and Suspense behave as they do in the browser.

React events get react-dom's priorities:

| Events | Priority |
| --- | --- |
| Clicks, presses, key presses, focus changes, `change` | Discrete (synchronous) |
| Pointer movement, enter and leave | Continuous |
| Everything else | Default |

## React: portals

Libraries render menus, dialogs and tooltips into `document.body` with `createPortal`. In gpuijs, `document.body` is the `<body>` host node, so that works. [`body()`](api.html#body) returns the same node:

```tsx
import { createPortal } from "react-dom"
import { body } from "@gpuijs/core/react"

function Modal({ children }) {
  return createPortal(<div className="fixed inset-0 grid place-items-center bg-black/40">{children}</div>, body())
}
```

As in react-dom, events in a portal bubble to the component that rendered it, not to the node it's mounted in. A `<Select>` list portaled out of a drawer still bubbles its clicks to the drawer.

## Host nodes and refs

A ref to an element (`<div ref={ref}>` in React or Solid, `ref="el"` in Vue) gives you a gpuijs **host node**, not a DOM element. It implements the parts of the DOM API that frameworks and libraries call:

- Tree: `parentNode`, `parentElement`, `children`, `childNodes`, `firstChild`, `lastChild`, `nextSibling`, `previousSibling`, `contains()`, `compareDocumentPosition()`, `getRootNode()`, `isConnected`, `ownerDocument`.
- Queries: `querySelector()`, `querySelectorAll()`, `closest()`, `matches()`, `getElementsByTagName()`, `getElementsByClassName()`.
- Attributes: `getAttribute()`, `setAttribute()`, `removeAttribute()`, `hasAttribute()`, `dataset`, `className`, `id`, `style`.
- Geometry: `getBoundingClientRect()`, `getClientRects()`, `offsetWidth`/`Height`/`Top`/`Left`/`Parent`, `clientWidth`/`Height`/`Top`/`Left`, `scrollWidth`/`Height`. For SVG: `getBBox()` and `getComputedTextLength()`.
- Events: `addEventListener()`, `removeEventListener()` (with `once` and `signal`), `dispatchEvent()`, `click()`.
- Focus: `focus()`, `blur()`.
- Form state: `value`, `checked`.

The full list is in [API reference: host nodes](api.html#host-nodes). Geometry reads can force a layout. See [Layout and measuring](layout.html#reading-sizes-from-javascript).

> [!NOTE]
> `scrollTop` and `scrollLeft` always read 0, and `scrollTo()` and `scrollIntoView()` do nothing. Scrolling is native and its offsets aren't reported to JavaScript yet.

## Library-built DOM

Some libraries build DOM themselves, and so do Vue and Solid: ApexCharts draws its SVG with `document.createElementNS`, flatpickr builds its calendar with `document.createElement`, and tooltips get written with `innerHTML`. gpuijs supports this. `document.createElement`, `createElementNS`, `createTextNode` and `createDocumentFragment` make host nodes outside React's control. They show up once appended to a node that is in the window.

On these nodes, `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `replaceChildren`, `insertAdjacentElement`, `innerHTML`, `textContent`, `cloneNode` and `el.style.*` all work, and each change reaches the host.

In a React app, React's own nodes stay where React puts them. `appendChild` on a React-rendered node only accepts library-built nodes (and moving a React node into or out of library-built DOM, which flatpickr does with its `<input>`). Writing `innerHTML` on a React node clears only what libraries added.

## How updates reach the host

Each React commit becomes one batch of operations sent to the host, and the host applies a batch atomically. DOM edits (Vue and Solid updates, or a library setting `el.style.height`, `setAttribute`, `innerHTML`) are batched within the current task and sent together.

The host restyles only what changed, lays out, and draws the next frame. [Architecture](architecture.html) has the details.
