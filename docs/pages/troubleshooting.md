# Troubleshooting

## Startup

### `host binary not found at …/target/debug/gpuijs`

In a gpuijs checkout, the native host hasn't been built: run `cargo build` at the repository root, or set `GPUIJS_HOST` to a host binary.

### `gpuijs: gpuijs-darwin-arm64 isn't installed`

The prebuilt host package didn't install alongside `gpuijs`. Reinstall with `bun install` (or `npm install`), and don't pass `--no-optional` or `--omit=optional`. On Linux or Windows this message is expected: gpuijs runs on macOS only for now.

### `gpuijs: no src/main.{tsx,jsx,ts,js} in <dir>`

The app directory needs an entry at `src/main.tsx`, `src/main.jsx`, `src/main.ts` or `src/main.js`. Check that you passed the app's directory, not its `src/`.

### `GPUIJS_SOCKET is not set; launch this bundle through the gpuijs host`

You ran the bundle (`.gpuijs/app.js`) with Bun directly. The host has to start it. Use `gpuijs dev <app-dir>`.

### `js process exited before connecting`

The app crashed while loading, before rendering anything. The error is printed above this line. A common cause is a library touching `window` or `document` at import time because the framework's entry point (`gpuijs/react`, `gpuijs/vue` or `gpuijs/solid`) isn't imported first.

### `ReferenceError: window is not defined` (or `document`, `matchMedia`, …)

Make the framework's entry point (`gpuijs/react`, `gpuijs/vue` or `gpuijs/solid`) the **first** import of `src/main.*`; it sets up the browser globals. Imports run in order, so the globals have to exist before any library module is evaluated.

### `gpuijs: create the Window before rendering or touching the document`

Something opened the default window (a `render` call, `document.body`, a `ResizeObserver`) before `new Window(options)`. Create the window first, at the top of `src/main.*`.

### `gpuijs: an app has one Window for now`

Only one `Window` can be created. Use in-app panels or modals instead of a second window.

## Rendering

### Nothing shows, or the window stays blank

- Check the terminal for errors. Errors thrown while rendering are logged there.
- Make sure something calls the entry point's `render` with your root component.
- If your root uses `h-full` or `h-screen`, check that its ancestors have a height. `<html>` fills the window, but `<body>` sizes to its content unless you give it a height (`html, body { height: 100% }` or `h-full` on both).

### Styles are missing

- Check that your CSS is imported from JavaScript or lives at `src/app.css`. Other `.css` files aren't compiled.
- For Tailwind, the stylesheet needs `@import "tailwindcss"`. Class names built at runtime from string fragments (`"bg-" + color`) aren't found by Tailwind's scanner. Write full class names.
- Look for `[gpuijs] unsupported css: …` warnings, which name what was skipped. See [CSS support](css.html).

### Text with mixed sizes looks wrong

Inline text can't change `font-size` or `font-family` inside a paragraph yet (look for a warning about it). Move the differently sized text into its own block, or make the parent `flex items-baseline`.

### Web fonts don't load

`@font-face` isn't supported yet. Install the font on the system, or use the system font stack.

### An element with `position: sticky` scrolls away

`sticky` behaves like `relative` for now.

### A virtualized list only shows its first rows

Scroll offsets aren't reported to JavaScript, so virtualizers never see the list scroll. Render the full list. gpuijs culls off-screen rows natively. See [Layout: long lists](layout.html#long-lists).

### An image doesn't show

- Root-relative URLs (`/logo.png`) must exist under `public/`.
- Remote images appear once downloaded. Check that the URL is reachable.
- CSS `url()` loads local files only, not remote or `data:` URLs.
- Check the format: PNG, JPEG, WebP, GIF, BMP, TIFF or SVG.

## Interaction

### A click handler doesn't fire

- Check for `pointer-events: none` on the element or an ancestor. It inherits.
- An element under a `position: fixed` overlay doesn't get clicks. Make sure modals and their backdrops unmount when closed.
- Inside a custom title bar, buttons need `app-region: no-drag`.

### `onScroll`, `onWheel`, `onKeyUp` or `onDoubleClick` never fire

These events aren't supported yet. See [Events](events.html#not-supported-yet).

### Keyboard shortcuts don't work while typing

A focused text field keeps most keys to itself. App-wide shortcuts are best as menu item [accelerators](menus.html#accelerators), which work everywhere.

### Tab doesn't move between fields

Keyboard focus navigation isn't implemented yet.

### A password field shows its text

`type="password"` isn't masked yet.

## Packaged apps

### The app opens and closes at once, or shows nothing

Read `~/Library/Logs/<Name>.log`. It has the host's and the app's output, errors included.

### `localStorage` is empty every time in development

That's expected. In development `localStorage` lives in memory. Only packaged apps persist it. See [Browser environment](environment.html#localstorage).

### macOS says the app is damaged or can't be opened

Unsigned or un-notarized apps downloaded from the internet are quarantined by Gatekeeper. Sign and notarize the app (see [Packaging](shipping.html#notarizing)). For local testing, `xattr -dr com.apple.quarantine "My App.app"` removes the quarantine flag.

## Getting more information

- `GPUIJS_PROFILE=1` prints per-frame timings. See [Performance](performance.html#frame-profiling).
- Host messages are prefixed `[gpuijs]`. App messages are whatever your code logs, since `console.log` goes to the terminal.
- To compare against a browser, most examples have `bun run web`, which serves the same app in a browser.
