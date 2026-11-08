# Introduction

gpuijs runs React, Vue and Solid apps as native macOS apps, drawn on the GPU by [GPUI](https://gpui.rs), the UI framework behind the Zed editor. You write an ordinary app with ordinary CSS (Tailwind included). gpuijs lays it out and paints it natively. There is no browser engine and no WebView.

Each framework has its own entry point. Import it first in `src/main.*` and hand it your root component:

```tsx
// React
import { render } from "gpuijs/react"
import "./app.css"
import { App } from "./App"

render(<App />)
```

```ts
// Vue
import { render } from "gpuijs/vue"
import "./app.css"
import App from "./App.vue"

render(App)
```

```tsx
// Solid
import { render } from "gpuijs/solid"
import "./app.css"
import { App } from "./App"

render(() => <App />)
```

The components themselves are unchanged: React hooks, Vue single-file components, Solid signals. See [Frameworks](frameworks.html).

## How it works

A gpuijs app is two processes:

- **The app** runs under [Bun](https://bun.sh). Your framework renders into gpuijs **host nodes**: React through gpuijs's own renderer (built on `react-reconciler`), Vue and Solid through their stock DOM runtimes. There is no real DOM. Each batch of changes becomes a list of small operations: create a node, set its props, append it.
- **The host** is a Rust program built on GPUI. It keeps a mirror of the tree, runs the CSS cascade over it with the app's stylesheet, lays it out with flexbox, grid and block layout, and draws it with Metal. It also owns the window, text input, menus and dialogs.

Events go the other way. The host hit-tests clicks, pointer movement and keys, and reports only the events the app listens for. The app then runs the framework's handlers and DOM listeners with browser-like capture and bubbling.

Read [Architecture](architecture.html) for the full picture.

## What you get

- **Your framework, unchanged.** React 19 (hooks, context, portals, Suspense and concurrent rendering, through React's own reconciler), Vue 3 (single-file components, `<script setup>`, plugins) and Solid.
- **Real CSS.** Selectors, specificity, cascade layers, custom properties, `calc()`, media and container queries, `:has()`, transitions and keyframe animations. Tailwind v4 is compiled for you. The [CSS support](css.html) page lists every property.
- **Native widgets where it matters.** Text fields are native GPUI inputs, so typing never waits on JavaScript. `<select>` opens a native-feeling list.
- **An Electron-style shell API.** `Window`, `Menu`, `MenuItem`, `app` and `dialog` use Electron's names and shapes. See [Window](window.html).
- **Browser globals for libraries.** Every entry point provides enough of `window` and `document` for libraries like Radix, shadcn/ui, MUI, Recharts, ApexCharts, react-router and floating-ui. See [Library compatibility](libraries.html).
- **One-command packaging.** `gpuijs build` produces a signed `.app` you can notarize. See [Packaging for macOS](shipping.html).

## Status

gpuijs is early software.

> [!WARNING]
> gpuijs is **macOS only** for now. Several features (backdrop blur, CSS filters, group opacity, inset shadows, gradients) use Metal shaders in a patched copy of GPUI. An app has **one window**.

It is not a browser. Your components run unchanged, but it renders to a native tree, not a DOM. Code that expects a full DOM API (`document.querySelector` across the whole page, `<iframe>`) won't work. [Troubleshooting](troubleshooting.html) lists the known gaps.

## Where to next

- [Quick start](quick-start.html): install gpuijs and run your first app.
- [Project structure](project.html): what a gpuijs app looks like on disk.
- [Styling](styling.html): Tailwind, plain CSS and CSS-in-JS.
- [API reference](api.html): every export of `gpuijs`.
