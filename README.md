# gpuijs

React, Vue and Solid apps rendered natively by [GPUI](https://gpui.rs). The app runs under Bun on gpuijs's host nodes; a Rust host lays it out with the app's CSS (Tailwind included) and draws it on the GPU. macOS only for now.

## Install

```sh
bun add @gpuijs/core react   # or vue, solid-js; the prebuilt macOS host comes with it
bunx gpuijs dev .      # runs src/main.tsx in a native window
```

## Docs

The full documentation is in [`docs/pages`](docs/pages). Build it as a site with `bun run docs` and open `docs/dist/index.html`.

## Develop gpuijs

```sh
bun install
cargo build
bun js/bin/gpuijs.js dev examples/shadcn-demo
```

An app is a directory with `src/main.{tsx,jsx,ts,js}`. It imports its framework's entry point first, which sets up the `window`/`document` libraries expect and exports `render`:

```ts
import { render } from "@gpuijs/core/react" // render(<App />)
import { render } from "@gpuijs/core/vue"   // render(App) or render(createApp(App).use(router))
import { render } from "@gpuijs/core/solid" // render(() => <App />)
```

CSS imported from JS (and `src/app.css`) is compiled with Tailwind. Files in `public/` serve root-relative URLs (`/images/a.png`).

`gpuijs dev` compiles `.vue` files with the app's `vue/compiler-sfc`, and Solid JSX with the app's `babel-preset-solid` when it depends on `solid-js`. See `examples/shadcn-demo`, `examples/vue-demo`, `examples/solid-demo` and [docs/pages/frameworks.md](docs/pages/frameworks.md).

## Window, menus and title bar

The shell API follows Electron's, with `Window` in place of `BrowserWindow`. Every entry point exports it, and plain `@gpuijs/core` exports it alone:

```tsx
import { Menu, Window, app, dialog } from "@gpuijs/core/react" // or gpuijs/vue, gpuijs/solid

const win = new Window({ width: 1200, height: 800, titleBarStyle: "hidden", trafficLightPosition: { x: 16, y: 18 } })
win.on("focus", () => {})
render(<App />) // into win; without a `new Window`, render opens a default one

Menu.setApplicationMenu(
  Menu.buildFromTemplate([
    { role: "appMenu" },
    { label: "File", submenu: [{ label: "Open…", accelerator: "CmdOrCtrl+O", click: open }] },
    { role: "editMenu" },
    { label: "View", submenu: [{ label: "Sidebar", type: "checkbox", checked: true, click: (item) => {} }] },
    { role: "windowMenu" },
  ]),
)
app.dock.setMenu(Menu.buildFromTemplate([{ label: "New Window", click: () => {} }]))

async function open() {
  const { canceled, filePaths } = await dialog.showOpenDialog(win, { properties: ["openFile"] })
}
// on a contextmenu event: Menu.buildFromTemplate([...]).popup()
```

- **`Window`:** `setTitle`, `minimize`, `maximize`, `unmaximize`, `isMaximized`, `setFullScreen`, `close`, `focus`, `getBounds`, `setSize`. Events: `ready-to-show`, `focus`, `blur`, `resize`, `maximize`, `enter-full-screen`, `close`.
- **`MenuItem`:** `label`, `type` (`normal`, `separator`, `submenu`, `checkbox`, `radio`), `role`, `accelerator`, `enabled`, `visible`, `checked`, `click`.
- **`dialog`:** `showOpenDialog`, `showSaveDialog`, `showMessageBox`.

A custom title bar is made with CSS, as in Electron: `app-region: drag` (or `-webkit-app-region`, Tailwind `[app-region:drag]`) on the bar, and `no-drag` on the buttons inside it.

There is one window per app for now. `menu.popup()` always opens at the pointer.

## Ship a macOS app

```sh
bun js/bin/gpuijs.js build examples/shadcn-demo                       # unsigned, for this machine
bun js/bin/gpuijs.js build examples/shadcn-demo --sign "Developer ID Application: …"
```

This writes `<app-dir>/.gpuijs/<name>.app`. The app's executable is the release host, and `Contents/Resources` holds the app compiled into a standalone Bun executable, its CSS and `public/`. Settings come from the app's `package.json`; the window's size and look come from its `new Window(...)`:

```json
{ "name": "my-app", "version": "1.0.0", "gpuijs": { "name": "My App", "id": "com.example.myapp" } }
```

At runtime the app keeps `localStorage` in `~/Library/Application Support/<id>`, downloaded images in `~/Library/Caches/<id>`, and its output (errors included) in `~/Library/Logs/<name>.log`.

`--sign` signs with the hardened runtime and the JIT entitlements Bun needs (`js/bin/entitlements.plist`). To distribute, notarize the signed app:

```sh
ditto -c -k --keepParent "My App.app" app.zip
xcrun notarytool submit app.zip --keychain-profile <profile> --wait
xcrun stapler staple "My App.app"
```

## Test

```sh
cargo test
cd js && bun test
```
