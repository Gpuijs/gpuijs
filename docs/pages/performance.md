# Performance and profiling

gpuijs is built to keep input-to-pixel latency low. Much of what's slow in a browser happens natively in the host, and JavaScript only does the framework's work. This page covers how that works, how to measure it, and how to keep your app fast.

## What's fast by design

- **Hover, active and focus styles** are matched natively. Moving the pointer over a list restyles it without waking JavaScript.
- **Transitions and animations** run in the host. No JavaScript runs per frame for CSS animations.
- **Text fields** draw keystrokes natively, before JavaScript hears about them.
- **Scrolling** is native, and long lists are culled to what's visible. See [Layout: long lists](layout.html#long-lists).
- **Restyles are incremental.** A commit restyles the changed nodes (and, when the stylesheet uses `:has()` or sibling selectors, the nodes those can affect). Cascade results are memoized: nodes with the same matched rules, inline style and inherited values share one result, so a 10,000-row table computes a handful of distinct styles.
- **Commits are binary.** Each batch of changes (a React commit, or a task's worth of Vue or Solid updates) is encoded as compact binary ops, not JSON. See [Wire protocol](protocol.html).
- **Events are opt-in.** The host reports an event for a node only if something listens for it.
- **Frames are drawn when JavaScript settles.** After an input, the host waits for JavaScript to finish the framework's work and anything it scheduled for "now" (0 ms timers), then draws one frame with the result, instead of drawing intermediate states.

## Writing fast gpuijs apps

- **Benchmark production builds.** Development builds (of React especially) are several times slower. Use `GPUIJS_PRODUCTION=1 gpuijs dev` or a packaged app.
- **Prefer CSS to JavaScript for visual state.** `hover:` beats `onMouseEnter` + state, and a CSS transition beats a per-frame JavaScript animation.
- **Avoid `onMouseMove` on many nodes.** Each listening node is measured after every layout, because pointer-tracking code reads rects on every move.
- **Don't interleave style writes and geometry reads.** Each read after a write forces a round trip to the host for layout. Batch the reads, then the writes.
- **Don't virtualize long lists.** gpuijs culls off-screen children natively. Render the list.
- **Keep keys stable.** Keyed reorders are cheap, while remounts rebuild host nodes.

## Frame profiling

Set `GPUIJS_PROFILE` to print a timing breakdown for every frame to stderr:

```sh
GPUIJS_PROFILE=1 GPUIJS_PRODUCTION=1 bun js/bin/gpuijs.js dev examples/mui-dashboard
```

```text
[gpuijs profile] draw 6.84 ms: restyle 1.20, build 2.31, layout 2.02, prepaint 0.31, paint 0.60, other 0.40
[gpuijs profile] present 0.62 ms: scene 0.20, drawable wait 0.11, encode 0.25, other 0.06
[gpuijs profile] gpu 2.15 ms
```

- A **draw** line covers a frame's work on the main thread: `restyle` (the cascade over changed nodes), `build` (styles to GPUI elements), `layout`, `prepaint` and `paint`.
- A **present** line covers handing the scene to the GPU: `scene`, `drawable wait` and `encode`.
- A **gpu** line is printed when the GPU finishes a frame.
- Spans with a trailing `*`, like `apply*` (applying commits to the tree), ran between frames and aren't counted in the line's total.

The numbers above are illustrative, and a span only appears when the frame did that work. `other` is time not covered by a named span.

## Benchmark driver

With `GPUIJS_BENCH_SOCKET=/tmp/bench.sock`, the host listens on that Unix socket for a benchmark driver. The driver sends JSON lines, and gets one reply line per command with the same `id`.

**Find an element:**

```json
{"id": 1, "cmd": "query", "selector": ".menu-item", "index": 2}
```

```json
{"id": 1, "rect": {"x": 12, "y": 140, "width": 220, "height": 36, "value": null}}
```

The reply is `"rect": null` when nothing matches. `value` is a text field's contents.

**Inject an input and time it:**

```json
{"id": 2, "cmd": "input", "input": {"kind": "click", "x": 40, "y": 158}}
```

```json
{"id": 2, "ms": 4.2}
```

`ms` runs from injecting the input until the GPU finished the first frame drawn after JavaScript handled it, so it covers the whole input-to-pixels path. Inputs enter where macOS input enters GPUI. Input kinds:

| `kind` | Fields | Notes |
| --- | --- | --- |
| `move` | `x`, `y` | Pointer move |
| `click` | `x`, `y` | Press and release, timed from the release |
| `wheel` | `x`, `y`, `dy` | Scroll. Positive `dy` scrolls down. |
| `key` | `text` | One key: its key event, then its text into the focused field |

Coordinates are window coordinates in CSS pixels.

## The `bench` example

`examples/bench` has two benchmarks:

- **`bun run dev`** opens a page with list operations (create, update, swap and clear thousands of rows). Each action is timed from the click to a layout read of a fresh node after the list, so it includes the host's restyle and layout. `BENCH_AUTO=1 bun run dev` clicks every button in turn and exits. `bun run web` runs the same page in a browser.
- **`bun run interactions`** (`interactions.ts`) measures input-to-pixel latency for real interactions on a large dashboard (hovering a sidebar, typing in a modal, scrolling, navigating), in gpuijs through the bench socket and in Chrome through the DevTools protocol, at 1440×900:

```sh
bun examples/bench/interactions.ts [--only gpuijs|chrome] [--scenario hover] [--dev] [--rounds 3]
```

Keep both windows uncovered and the mouse still while it runs, because a hidden window draws nothing. `interactions.ts` expects the TailAdmin dashboard checked out at `examples/tailadmin`, which isn't part of the repository.
