# Architecture

gpuijs splits an app between a JavaScript process that runs the UI framework (React, Vue or Solid) and a Rust process that owns everything visual. This page follows the pieces from launch to pixels.

```text
┌──────────────────────────── app process (Bun) ──────────────────────────┐
│  your React, Vue or Solid app                                           │
│      │                                                                  │
│  React reconciler + gpuijs host config  (js/src/renderer.js)            │
│      │ commits as ops            ▲ events, rects, metrics               │
│  transport  (js/src/transport.js, wire.js)                              │
└──────┼───────────────────────────┼──────────────────────────────────────┘
       │ pipe GPUIJS_IN            │ socket GPUIJS_SOCKET
       │ (binary + JSON frames)    │ (NDJSON)      pipe GPUIJS_SYNC: layout answers
┌──────▼───────────────────────────┴──────────── host process (Rust) ─────┐
│  protocol (src/protocol.rs) ─► tree (src/tree.rs)                       │
│                                   │                                     │
│  stylesheet + cascade (src/css/) ─► computed styles                     │
│                                   │                                     │
│  render (src/render.rs) ─► GPUI elements ─► Taffy layout ─► Metal       │
│  input.rs (text fields) · native.rs (window, menus, dialogs) · svg.rs   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Two processes

**The host** (`src/main.rs`) is the process you launch. In development the CLI runs `gpuijs .gpuijs/app.js`, with the host from the installed `@gpuijs/darwin-*` package (`target/debug/gpuijs` in a gpuijs checkout). In a packaged app the host is the bundle's executable and finds the compiled sidecar in `Contents/Resources`.

At startup, the host:

1. Binds a Unix socket and creates two named pipes in the temp directory, all named after its PID.
2. Spawns the app under Bun, passing their paths as `GPUIJS_SOCKET`, `GPUIJS_IN` and `GPUIJS_SYNC`.
3. Starts reading the input pipe on a thread, because the app writes its first messages (the stylesheet) as soon as it starts.
4. Waits for the app to connect, failing fast if the app exits first.
5. Waits for the app's first message, which is always `window` with the options of `new Window(...)` (or the defaults), and opens the GPUI window with them.

**The app** connects to the socket, sends `window`, then its stylesheet, and waits for the host's font metrics before its first render.

Their lifetimes are linked. Closing the window quits the host, which closes the socket, and the app exits when the socket closes. If the app exits, the host's message stream ends, and the host quits too.

## Three channels

| Channel | Direction | Format | Why it's separate |
| --- | --- | --- | --- |
| `GPUIJS_IN` (named pipe) | app → host | Frames: a 1-byte kind and a 4-byte length, then a JSON message (kind 0) or a binary commit (kind 1) | Written with **blocking** writes, so every message is out before the app blocks for a layout answer. A socket write could sit in Bun's event loop. |
| `GPUIJS_SOCKET` (Unix socket) | host → app | Newline-delimited JSON | Events, measured rects, window state and dialog results, read on the app's event loop |
| `GPUIJS_SYNC` (named pipe) | host → app | One JSON line per request | Answers to forced layouts, which the app reads with a **blocking** read |

stdout and stderr stay free for your `console.log`. [Wire protocol](protocol.html) documents every message.

## The app side

### The renderer

`js/src/renderer.js` is a host config for `react-reconciler`, React's renderer-building package, with a concurrent root. Host instances are plain JavaScript objects (host nodes) that keep their props, children and a fiber reference. They have no DOM behind them.

As React commits, the renderer records **ops**: `el` (create an element), `txt` (create a text node), `set` (new props), `settext`, `append`, `insert`, `remove` and `detach`. At the end of each commit, it encodes the batch in binary (`js/src/wire.js`) and writes it to the pipe. Imperative edits (`setAttribute`, `el.style`, `innerHTML`) join the same batching. That's how Vue and Solid apps run: their DOM runtimes create and edit host nodes through `document`, without React rendering anything, and their edits are sent once per task.

Props are turned into what the host needs:

- `className` → `class`; `style` → CSS declaration text; attributes for selectors (`id`, `role`, `type`, `data-*`, `aria-*`, ...).
- **Event names**, not functions. Handlers stay in JavaScript, and the host learns which events to report for each node.
- `value`, `placeholder`, `disabled` and `checked` for form controls, with a sequence number for inputs.
- `src`, resolved to a local file (see [assets](assets.html)).
- A `measure` flag on nodes whose rects JavaScript wants after each layout (observed, measured before, or tracking the pointer).

### Events

The host reports events with the node id, type and coordinates. The renderer turns each into a browser-shaped dispatch: `document` capture listeners, React's capture and bubble handlers along the fiber path (so portals bubble to their owners), DOM listeners up the host tree to `document` and `window`, and then the default action (toggling a checkbox, clicking a label's control, submitting a form). Pointer events inside an `<svg>` are hit-tested by geometry in `js/src/svg-geometry.js`.

### Settling

After JavaScript has handled an input event (React's work, microtasks, and 0 ms timers it started), it sends `settled`. The host waits for this to draw the frame for the input, so it draws the result rather than an intermediate state.

### Forced layout

When code reads a geometry property of a node without a fresh rect, the renderer flushes pending ops, sends `sync`, and blocks on a synchronous read of the `GPUIJS_SYNC` pipe. The host applies everything, lays out, and writes every measured rect as one JSON line. This is a browser's forced reflow, done across processes.

## The host side

### Tree

`src/tree.rs` keeps the mirror of the React tree: nodes with a tag, props and children, or text. Commits are applied atomically. A `Changes` record tracks what was attached, changed or removed, for incremental restyling.

### CSS

`src/css/` is the style engine:

- `mod.rs` parses author CSS with lightningcss, lowering nesting and media range syntax so the cascade only sees flat rules. It flattens `@media`, `@supports`, `@container` and `@layer` into an indexed rule list (by id, class, tag and universal), and collects `@keyframes` and `@property`.
- `select.rs` lets lightningcss's selector engine match against the tree. It handles `:hover`/`:active`/`:focus` as state bits (so GPUI can apply them natively), group hover, `:has()` and sibling-sensitive selectors.
- `cascade.rs` runs the cascade: matching, specificity, layers, `!important`, inheritance, custom properties and `var()` substitution. It memoizes results by (matched rules, inline style, inherited input), so repeated rows share one computation, and it restyles only what a commit changed.
- `props.rs` maps each computed declaration onto a GPUI `StyleRefinement`, plus gpuijs's own extras (display mode, text transform, white-space, app region, SVG paints, transitions).

A UA stylesheet (`src/ua.css`) sits in the lowest layer.

### Render

`src/render.rs` rebuilds GPUI elements from the tree and computed styles every frame. GPUI is an immediate-mode framework, so this is its normal model. It handles:

- block flow with inline line boxes, and paragraphs with styled text runs;
- flex and grid items (via GPUI's Taffy integration), tables as grids, and `position: fixed` layers above the scrolling page;
- state styles for hover, active and focus, applied by GPUI without a round trip;
- transitions and keyframe animations (`src/transition.rs`), requesting animation frames while any run;
- native text fields (`src/input.rs`), `<select>` lists, images, and SVG rasterization (`src/svg.rs`);
- culling of long lists (`src/cull.rs`);
- reporting events, measured rects and window state back to JavaScript.

### Native shell

`src/native.rs` carries out the Electron-style API: window options and operations, the application and Dock menus (with accelerators as GPUI key bindings), context menus, and dialogs. Results go back to JavaScript as events tagged with the request id.

### Vendored GPUI and Taffy

gpuijs builds against patched copies of GPUI and Taffy in `vendor/`. The patches add CSS features GPUI doesn't have: grid track lists, transforms, group opacity and filters, backdrop blur, inset shadows, multi-stop and radial gradients, letter spacing, `object-fit`, and window drag regions. They also carry layout fixes for Taffy. [Contributing](contributing.html#vendored-dependencies) explains how they're marked.
