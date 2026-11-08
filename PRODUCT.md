# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack
Plain static HTML and CSS (user's choice: no React or other framework), hosted at gpuijs.dev.

## Users
Web developers who build UIs in React, Vue or Solid and want to ship a native macOS desktop app without Electron's bundled Chromium or a WebView. They are evaluating gpuijs against Electron and Tauri: they want to know whether their existing components, CSS (Tailwind included) and libraries carry over, and how to try it.

Secondary: contributors who would work on the Rust host (GPUI, the CSS cascade, layout).

## Product Purpose
gpuijs runs an ordinary React, Vue or Solid app as a native macOS app. The app runs under Bun; a Rust host built on GPUI (the UI framework behind the Zed editor) mirrors the node tree, runs the CSS cascade with the app's stylesheet, lays it out (flexbox, grid, block) and draws it with Metal. Success for a visitor: they understand it is not a browser and not a WebView, believe their code carries over, and start the quick start.

## Positioning
No browser engine and no WebView: the app's own CSS is cascaded and laid out by a native host and drawn on the GPU, while the app keeps its stock framework runtime and its libraries (Radix, shadcn/ui, MUI, Recharts…). Electron ships Chromium; Tauri ships the system WebView; gpuijs ships neither.

## Operating Context
Developers evaluate by reading code samples, running `bun js/bin/gpuijs.js dev examples/shadcn-demo`, and comparing against the same app in Chrome (the examples have `web` scripts and compare tooling). Apps are a directory with `src/main.*`; `gpuijs dev` runs them, `gpuijs build` packages a signed `.app`. The shell API mirrors Electron's (`Window`, `Menu`, `app`, `dialog`).

## Capabilities and Constraints
- React 19 (own reconciler, concurrent root), Vue 3 and Solid through their stock DOM runtimes; entry points `gpuijs/react`, `gpuijs/vue`, `gpuijs/solid`.
- Real CSS: selectors, specificity, cascade layers, custom properties, calc(), media and container queries, :has(), transitions, keyframes. Tailwind v4 compiled by the CLI.
- Native text fields (typing never waits on JavaScript), native menus, dialogs, custom title bars via `app-region`.
- Constraints: macOS only for now; one window per app; early software. Not a full DOM (no iframes).
- Not published to npm yet; install is from a checkout.
- Hosting: the landing page lives at gpuijs.dev, the docs at docs.gpuijs.dev.
- Open: public repository URL (the user will provide it).

## Brand Commitments
Name is lowercase "gpuijs". The docs use an orange accent (#c2410c light, #fb8a4c dark), the system UI font and a conic-gradient square mark. The landing page (site/index.html) uses the docs theme and stays simple: the user declined bespoke visual directions.

## Evidence on Hand
- Example apps: examples/shadcn-demo, mui-dashboard (MUI's dashboard template, unmodified), next-shadcn-dashboard (a large shadcn admin dashboard), vue-demo, solid-demo, css-lab, bench.
- Screenshot: examples/next-shadcn-dashboard/public/shadcn-dashboard.png; css-lab compare captures (gpuijs vs Chrome) under examples/css-lab/.gpuijs/compare/.
- No benchmark figures, user counts, testimonials or press. Do not invent any.

## Product Principles
- Your app, unchanged: lead with the code the developer already writes.
- Not a browser, and honest about it: state the limits (macOS only, one window, early).
- Native where it matters: text input, menus, dialogs, title bars.
- Framework-neutral: React, Vue and Solid are peers.
