# Library compatibility

gpuijs runs libraries unchanged when they stick to their framework and the DOM subset that [host nodes](api.html#host-nodes) and the [browser environment](environment.html) provide. Each library below has been run in gpuijs: in an example app, in the test suite, or while building compatibility shims for it. gpuijs carries specific fixes for most of them.

Most of them are React libraries, because the large examples are React apps. Framework-agnostic ones (floating-ui, ApexCharts, Chart.js, flatpickr, axios) rely only on host nodes and the browser environment, which Vue and Solid apps get too. So far they have been tested from React apps. See [Frameworks](frameworks.html).

Import your framework's entry point (`@gpuijs/core/react`, `@gpuijs/core/vue` or `@gpuijs/core/solid`) first when you use any of these.

## Component libraries

| Library | Status | Notes |
| --- | --- | --- |
| **shadcn/ui** | Works | Used by `shadcn-demo` and `next-shadcn-dashboard`. Theme variables, `tw-animate-css` enter and exit animations, `ChartStyle`. |
| **Radix UI** | Works | Dialog, Popover, Select, Tooltip, Avatar and the other primitives shadcn/ui builds on. Focus scope, dismissal on outside presses, collision-aware positioning and hidden native inputs are all handled. |
| **Base UI** | Works | Used by `next-shadcn-dashboard`. Exit animations end at once, because `getAnimations()` returns `[]`. |
| **Material UI** (`@mui/material`) | Works | The `mui-dashboard` example runs MUI's dashboard template unmodified: Emotion styles, menus, selects, collapses, color schemes. |
| **MUI X Charts** | Works | Line, bar and pie charts, axis label measuring, tooltips, highlight on hover |
| **MUI X Data Grid** | Works | In `mui-dashboard` |
| **MUI X Date Pickers** | Works | In `mui-dashboard` |
| **MUI X Tree View** | Works | In `mui-dashboard` |
| **cmdk** | Works | Arrow and Enter key navigation from the input, attribute selectors on `cmdk-*` |
| **vaul** | Works | Drawer drags read their offset through `getComputedStyle().transform` |
| **sonner** | Works | Injected styles, stacked toasts |

## Charts and visualization

| Library | Status | Notes |
| --- | --- | --- |
| **Recharts** | Works | Text measured through [`measureText`](api.html#measuretext)-backed hidden spans, tooltips that follow the pointer |
| **ApexCharts** / react-apexcharts | Works | Builds its SVG with `createElementNS`. Label sizing through `getBBox()`, injected default styles, `innerHTML` tooltips. |
| **jsVectorMap** | Works | Region hover through SVG hit-testing, CSS `:hover` fills, page-coordinate tooltips |
| **MUI X Charts** | Works | See above |
| **Three.js** | Works | WebGL 2, textures from `TextureLoader`. See [Elements: canvas](elements.html#canvas). |
| **PixiJS** v8 | Works | WebGL renderer, `Graphics`, `Text`, `Ticker` |

## Positioning and overlays

| Library | Status | Notes |
| --- | --- | --- |
| **floating-ui** | Works | Host nodes pass its element checks. Geometry reads force a layout when needed. |
| **Popper.js** | Works | Fixed-position tooltips are placed against the window |
| **react-remove-scroll** | Works | Its injected styles apply |
| **react-resizable-panels** | Works | Use its browser build, which the CLI picks by default. Its drag cursor style is injected at runtime. |

## Routing, data and forms

| Library | Status | Notes |
| --- | --- | --- |
| **react-router** | Works | On the in-memory `history`. `ROUTE=/path` opens a route directly. |
| **TanStack Query** | Works | In `next-shadcn-dashboard` |
| **TanStack Table** | Works | In `next-shadcn-dashboard` |
| **TanStack Form** | Works | In `next-shadcn-dashboard` |
| **zod** | Works | `instanceof FileList` checks resolve |
| **axios** | Works | Through its `fetch` adapter |
| **react-helmet-async** | Partly | Renders. Setting `<html>`/`<body>` attributes through it does nothing. |

## Interaction and animation

| Library | Status | Notes |
| --- | --- | --- |
| **dnd-kit** | Works | Pointer-event sensors. Used by `next-shadcn-dashboard`. |
| **Lexical** | Works | Rich text through `beforeinput`. See [Elements: editable content](elements.html#editable-content). |
| **TipTap** / ProseMirror | Works | StarterKit; reads typing through `MutationObserver` |
| **embla-carousel** | Works | Used by `next-shadcn-dashboard` |
| **react-spring** | Works | Per-frame style writes through `el.style` |
| **tw-animate-css** | Works | Keyframes read each element's own `--tw-enter-*` variables |

## Calendars and pickers

| Library | Status | Notes |
| --- | --- | --- |
| **flatpickr** | Works | Builds its calendar with `createElement`, wraps its input, writes `input.value` |
| **FullCalendar** | Works | Event targets through `composedPath()` |
| **MUI X Date Pickers** | Works | See above |

## CSS-in-JS

| Library | Status | Notes |
| --- | --- | --- |
| **Emotion** | Works | MUI's styling engine, and `@emotion/styled` directly |
| **goober** | Works | Finds its sheet again through the element id global |
| **Tailwind CSS v4** | Built in | See [Styling](styling.html#tailwind-css) |

Libraries not listed here may well work. Try them, and watch the terminal for errors and `unsupported css` warnings.

## What tends not to work

Libraries that need these don't work yet:

- **Scroll position**: virtualizers that compute their window from `scrollTop` (react-window, react-virtuoso, TanStack Virtual). gpuijs culls long lists natively, so render the list directly. See [Layout: long lists](layout.html#long-lists).
- **`IntersectionObserver`**: infinite-scroll triggers and lazy-loading wrappers.
- **Full DOM queries**: `document.querySelector` across the page returns `null`. Query from `document.body` instead.
- **iframes**.

## Making a library work

When a library fails, the error usually names the missing global or method. Most fixes are small additions to `js/src/env.js` (globals) or the `HostNode` object in `js/src/renderer.js` (element methods). The existing shims show the pattern: each says which library needs it and why. See [Contributing](contributing.html).
