# Quick start

This page takes you from an empty directory to a running app, and then to a packaged `.app`.

## Requirements

| Tool | Why |
| --- | --- |
| **macOS** (Apple silicon or Intel) | The host draws with Metal and uses AppKit for windows, menus and dialogs. |
| **[Bun](https://bun.sh)** (1.2 or later; gpuijs is developed on 1.4) | Runs the app, bundles it and compiles its CSS. |

The native host comes prebuilt from npm, so you don't need Rust. To build gpuijs itself, see [From source](#from-source).

## Create an app

An app is a directory with a `package.json` and a `src/main.tsx` (or `.jsx`, `.ts`, `.js`).

```text
hello/
├── package.json
└── src/
    ├── main.tsx
    └── app.css
```

Make the directory and install gpuijs with your framework (`react`, `vue` or `solid-js`):

```sh
mkdir hello && cd hello
bun init -y
bun add gpuijs react
```

`bun add gpuijs` also installs the host for your Mac, `gpuijs-darwin-arm64` or `gpuijs-darwin-x64`. Add `dev` and `build` scripts to `package.json`:

```json
{
  "name": "hello",
  "private": true,
  "type": "module",
  "scripts": { "dev": "gpuijs dev .", "build": "gpuijs build ." },
  "dependencies": {
    "gpuijs": "^0.1.0",
    "react": "^19.3.0"
  }
}
```

`src/app.css` pulls in Tailwind:

```css
@import "tailwindcss";
```

`src/main.tsx` imports the framework's entry point first and renders the app. With React:

```tsx
import { render } from "gpuijs/react"
import { useState } from "react"

function App() {
  const [name, setName] = useState("")
  return (
    <div className="flex flex-col gap-3 p-8">
      <h1 className="text-3xl font-bold tracking-tight">Hello{name && `, ${name}`}</h1>
      <input
        className="rounded-md border border-zinc-300 px-3 py-2"
        placeholder="Your name"
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
    </div>
  )
}

render(<App />)
```

With Vue, the entry is `src/main.ts` and the component lives in a `.vue` file:

```ts
import { render } from "gpuijs/vue"
import App from "./App.vue"

render(App)
```

```vue
<!-- src/App.vue -->
<script setup>
import { ref } from "vue"
const name = ref("")
</script>

<template>
  <div class="flex flex-col gap-3 p-8">
    <h1 class="text-3xl font-bold tracking-tight">Hello{{ name && `, ${name}` }}</h1>
    <input v-model="name" class="rounded-md border border-zinc-300 px-3 py-2" placeholder="Your name" />
  </div>
</template>
```

With Solid (which also needs `babel-preset-solid`, see [Frameworks: Solid](frameworks.html#solid)):

```tsx
import { render } from "gpuijs/solid"
import { createSignal } from "solid-js"

function App() {
  const [name, setName] = createSignal("")
  return (
    <div class="flex flex-col gap-3 p-8">
      <h1 class="text-3xl font-bold tracking-tight">Hello{name() && `, ${name()}`}</h1>
      <input
        class="rounded-md border border-zinc-300 px-3 py-2"
        placeholder="Your name"
        value={name()}
        onInput={(e) => setName(e.currentTarget.value)}
      />
    </div>
  )
}

render(() => <App />)
```

Run it:

```sh
bun run dev
```

A window opens with your app. `bunx gpuijs dev .` does the same without a script.

> [!TIP]
> `src/app.css` is picked up automatically, even when nothing imports it. CSS you import from JavaScript (`import "./button.css"`) is compiled too, in import order. See [Styling](styling.html#how-css-reaches-the-window).

### Why the entry point comes first

Libraries, and Vue itself, reach for `window`, `document`, `matchMedia`, `ResizeObserver` and friends as soon as they load. The entry point (`gpuijs/react`, `gpuijs/vue` or `gpuijs/solid`) defines those globals, backed by gpuijs. Import it **before anything else**, so it runs before any library module. [Browser environment](environment.html) covers what it provides.

## Open a window yourself

`render` opens a default 1024×768 window. To choose the size, title bar or minimum size, create the `Window` first. It comes from the same entry point:

```tsx
import { Window, render } from "gpuijs/react" // or gpuijs/vue, gpuijs/solid
import { App } from "./App"

new Window({
  width: 1200,
  height: 800,
  minWidth: 640,
  minHeight: 480,
  titleBarStyle: "hidden",
  trafficLightPosition: { x: 16, y: 18 },
})
render(<App />)
```

Next, see [Window](window.html), [Menus](menus.html) and [Custom title bars](title-bar.html).

## Ship it

```sh
bun run build
open .gpuijs/hello.app
```

[Packaging for macOS](shipping.html) covers app names, bundle ids, signing and notarization.

## From source

To work on gpuijs itself or run the examples, build the host from a checkout. You also need **[Rust](https://rustup.rs)** (2024 edition, stable) and the **Xcode Command Line Tools** (`xcode-select --install`) for the Metal shader compiler.

gpuijs is a Bun workspace (the `js` package plus the examples) next to a Cargo crate (the host). From the root of the checkout:

```sh
bun install     # JS dependencies for the package and every example
cargo build     # the debug host, at target/debug/gpuijs
```

The first `cargo build` compiles GPUI and takes a few minutes. Later builds are fast. Dependencies build with optimizations even in debug builds, because layout, text and SVG work would otherwise make every frame several times slower.

Run an example:

```sh
bun js/bin/gpuijs.js dev examples/shadcn-demo
```

A window opens with a shadcn/ui page. Each example also has a `dev` script, so `cd examples/shadcn-demo && bun run dev` works too. In a checkout the CLI uses the host in `target/`, not the npm one. An app under `examples/` depends on `"gpuijs": "workspace:*"`, and the workspace links it.

## The examples

| Example | What it shows |
| --- | --- |
| `examples/shadcn-demo` | shadcn/ui components on Tailwind v4, a custom title bar and a dark mode toggle. The smallest example and the best place to start. |
| `examples/mui-dashboard` | Material UI's dashboard template, unmodified: Emotion CSS-in-JS, MUI X Charts, Data Grid, Date Pickers and Tree View. |
| `examples/next-shadcn-dashboard` | A large Next.js shadcn admin dashboard, run as a desktop app: Base UI, TanStack Table/Query/Form, dnd-kit, cmdk, embla and more. |
| `examples/vue-demo` | A todo list as a Vue 3 single-file component, with Tailwind and a scoped style. |
| `examples/solid-demo` | The same todo list in Solid. |
| `examples/bench` | A benchmark page and `interactions.ts`, which measures input-to-pixel latency against Chrome. See [Performance](performance.html). |

Most examples also have a `web` script (`bun run web`) that serves the same app in a browser, for side-by-side comparison.
