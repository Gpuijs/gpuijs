# Frameworks

gpuijs runs React, Vue 3 and Solid apps on the same host nodes. React renders through gpuijs's own renderer. Vue and Solid use their stock DOM runtimes (`createApp().mount`, `solid-js/web`) unchanged. `gpuijs dev` and `gpuijs build` compile components for all three.

Each framework has its own entry point. It sets up `window` and `document` (see [Browser environment](environment.html)) and exports `render`, plus the native shell API (`Window`, `Menu`, `app`, `dialog`…):

| Import | `render` |
| --- | --- |
| `@gpuijs/core/react` | `render(<App />)`. Calling it again updates the root. |
| `@gpuijs/core/vue` | `render(App)`, or `render(createApp(App).use(router))` to install plugins first. Returns the Vue app. |
| `@gpuijs/core/solid` | `render(() => <App />)`. Returns a function that unmounts it. |

Plain `@gpuijs/core` exports the shared API without `render` or the browser environment, for modules that only need `Window`, `Menu` or `dialog`.

Import the entry point **first** in `src/main.*`, before anything that loads the framework or its libraries. Vue reads `document` once, as it loads.

## React

```sh
bun add @gpuijs/core react
```

```tsx
// src/main.tsx
import { Window, render } from "@gpuijs/core/react"
import "./app.css"
import { App } from "./App"

new Window({ width: 900, height: 600 })
render(<App />)
```

React 19.3 or later is required. `react-dom` isn't needed to render, though libraries that import it (for `createPortal` or `flushSync`) still need it installed. Use `"jsx": "react-jsx"` in `tsconfig.json`. See [Rendering](rendering.html) for the React root, and `examples/shadcn-demo`.

## Vue

```sh
bun add @gpuijs/core vue
```

```ts
// src/main.ts
import { Window, render } from "@gpuijs/core/vue"
import "./app.css"
import App from "./App.vue"

new Window({ width: 900, height: 600 })
render(App)
```

`.vue` single-file components compile with the app's own `vue/compiler-sfc`, which comes with `vue`. `<script setup>`, TypeScript (`lang="ts"`) and scoped styles work. Each `<style>` joins the app's CSS. See `examples/vue-demo`.

## Solid

```sh
bun add @gpuijs/core solid-js
bun add -d babel-preset-solid @babel/core @babel/preset-typescript
```

```tsx
// src/main.tsx
import { Window, render } from "@gpuijs/core/solid"
import "./app.css"
import { App } from "./App"

new Window({ width: 900, height: 600 })
render(() => <App />)
```

When the app's `package.json` depends on `solid-js`, the CLI compiles the app's `.jsx` and `.tsx` files with `babel-preset-solid`, as vite-plugin-solid does. Set `"jsx": "preserve"` and `"jsxImportSource": "solid-js"` in `tsconfig.json` for the editor. See `examples/solid-demo`.

## Limits

- SFC `<style>` blocks are plain CSS: no `lang="scss"` or `lang="less"`, and no `src=""`.
- The host reports key-downs only. `keyup` fires right after each `keydown`, so `@keyup.enter` works, but the two arrive together.
- Text fields report Enter, Escape, Tab and the arrow keys as `keydown`. Typed characters arrive as `input` events.
- A Vue or Solid bundle still includes React, because the host-node layer lives in gpuijs's React renderer.
- Svelte isn't supported. Svelte 5 needs real DOM classes (`Node.prototype`, `Text`, `Comment`) that host nodes don't have.
