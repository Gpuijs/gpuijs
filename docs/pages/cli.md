# CLI

The `gpuijs` command has two subcommands: `dev` runs an app, and `build` packages it.

```text
gpuijs dev <app-dir>
gpuijs build <app-dir> [--sign <identity>]
```

`<app-dir>` defaults to the current directory. The `@gpuijs/core` package installs it: an app calls `gpuijs` from its `package.json` scripts, or runs `bunx gpuijs`. In the gpuijs repository, call the CLI as `bun js/bin/gpuijs.js`.

## `gpuijs dev`

```sh
gpuijs dev examples/shadcn-demo
```

`dev` builds the app and launches it in four steps.

### 1. Bundle the JavaScript

`src/main.*` is bundled with `Bun.build` into `.gpuijs/app.js`:

- **Target `bun`, with the `browser` export condition.** Packages resolve to their browser builds, because the app is a client UI. Their "node" builds are often server renderers (react-resizable-panels' has no layout effects, for example).
- **CSS imports are dropped from the bundle.** Step 2 compiles them instead.
- **`*.svg?react` imports become components,** as with `vite-plugin-svgr` (see [SVG as components](assets.html#svg-files-as-components)).
- **Development builds by default.** Set `GPUIJS_PRODUCTION=1` to bundle production builds (`NODE_ENV=production` for React, Vue and Solid) and minify, as `build` does. Use it for benchmarks.

### 2. Compile the CSS

The CLI collects the app's stylesheets in the order a browser would apply them:

1. `src/app.css`, if it exists.
2. Every `.css` file the JavaScript imports, depth-first in module evaluation order, as Vite applies them. Files that `app.css` already `@import`s are skipped.

It writes `.gpuijs/entry.css` with one `@import` per sheet and runs the Tailwind v4 CLI on it, with the app's directory as the working directory. Tailwind scans the app's sources for class names, and plain CSS passes through unchanged. The result is `.gpuijs/app.css`.

### 3. Read the page settings

- The `class` attribute of `<body>` in `index.html`, if any, is passed to the app as `GPUIJS_BODY_CLASS`. The renderer adds those classes to `<body>`.
- The app name from `package.json` goes to the app as `GPUIJS_APP_NAME`.
- `public/` is passed as `GPUIJS_PUBLIC`.

### 4. Launch the host

The CLI starts the host binary with the bundle's path. The host opens a socket and two pipes, starts Bun on the bundle, waits for it to connect and opens the window the app asks for. When the window closes, both processes exit.

The host binary is `GPUIJS_HOST` if set, else `target/debug/gpuijs` in a gpuijs checkout, else the one in the `@gpuijs/darwin-arm64` or `@gpuijs/darwin-x64` package npm installed with `@gpuijs/core`. Stopping the CLI with Ctrl+C (or `SIGTERM`) closes the window too.

> [!NOTE]
> There's no hot reload yet. Restart `gpuijs dev` to see changes.

## `gpuijs build`

```sh
gpuijs build examples/shadcn-demo
gpuijs build examples/shadcn-demo --sign "Developer ID Application: Jane Doe (TEAMID)"
```

`build` runs the same bundling and CSS steps with production builds and minification. It then:

1. Takes the release host: `GPUIJS_HOST` if set, else the installed `@gpuijs/darwin-*` package's. In a gpuijs checkout it runs `cargo build --release` and takes `target/release/gpuijs`.
2. Creates `.gpuijs/<Name>.app`, with the host as its executable.
3. Compiles the bundle into a standalone Bun executable at `Contents/Resources/app`, so the user doesn't need Bun installed.
4. Copies `app.css`, `public/` and a small `gpuijs.json` (name and bundle id) into `Contents/Resources`.
5. Writes `Info.plist` from `package.json`.
6. With `--sign`, signs the sidecar and then the bundle with the hardened runtime and the JIT entitlements Bun needs.

The full story, notarization included, is in [Packaging for macOS](shipping.html).

## Exit codes and errors

| Situation | Result |
| --- | --- |
| Unknown command, or flags other than `--sign <identity>` | Prints usage, exits 1 |
| No `src/main.*` | `gpuijs: no src/main.{tsx,jsx,ts,js} in <dir>`, exits 1 |
| Bundling fails | Bun's build log, exits 1 |
| Tailwind fails | Tailwind's exit code |
| Host binary missing | `host binary not found at …; run cargo build or set GPUIJS_HOST`, exits 1 |
| No host package for this platform | `gpuijs: @gpuijs/<platform>-<arch> isn't installed; gpuijs runs on macOS (arm64, x64) only`, exits 1 |
| App exits | The host's exit code |

## Environment variables

The CLI reads `GPUIJS_HOST` and `GPUIJS_PRODUCTION`, and passes several variables on to the host and the app. [Environment variables](env-vars.html) lists them all.
