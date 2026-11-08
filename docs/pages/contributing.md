# Contributing

This page is a map of the repository and the conventions it follows.

## Repository layout

```text
gpuijs/
├── Cargo.toml              the host crate, with vendored-patch notes
├── package.json            the Bun workspace: js/ and examples/*
├── src/                    the native host (Rust)
│   ├── main.rs             process setup, channels, window, packaged-app launch
│   ├── protocol.rs         app → host messages and the binary commit decoder
│   ├── tree.rs             the mirrored element tree
│   ├── css/
│   │   ├── mod.rs          stylesheet parsing, at-rules, rule index, media evaluation
│   │   ├── select.rs       selector matching against the tree
│   │   ├── cascade.rs      the cascade, inheritance, var(), memoization, incremental restyle
│   │   ├── props.rs        computed declarations → GPUI styles
│   │   └── tests.rs        CSS tests
│   ├── ua.css              the UA stylesheet
│   ├── render.rs           tree + styles → GPUI elements, events, layout reporting
│   ├── input.rs            native text fields
│   ├── native.rs           window options, menus, dialogs
│   ├── svg.rs              inline SVG → markup for rasterizing
│   ├── transition.rs       transitions and keyframe animations
│   ├── cull.rs             off-screen culling of long lists
│   └── bench.rs            the benchmark driver socket
├── js/                     the gpuijs npm package
│   ├── bin/gpuijs.js       the CLI (dev, build)
│   ├── bin/entitlements.plist
│   └── src/
│       ├── index.js        public exports
│       ├── env.js          @gpuijs/core/env: browser globals
│       ├── renderer.js     the React host config, host nodes, event dispatch
│       ├── native.js       Window, Menu, MenuItem, app, dialog
│       ├── transport.js    socket and pipes
│       ├── wire.js         binary commit encoder
│       ├── head.js         document.head and runtime <style> elements
│       ├── svg-geometry.js SVG bounding boxes and hit-testing
│       ├── markup.js       small HTML/SVG parser (innerHTML, svg?react)
│       ├── svgr.js         *.svg?react components
│       ├── css-imports.js  CSS import order from the bundle's metafile
│       ├── storage.js      localStorage
│       ├── style.js        React style objects → CSS text
│       └── text.js         measureText
├── vendor/
│   ├── gpui/               patched GPUI 0.2.2
│   └── taffy/              patched Taffy 0.9.0
├── tests/                  integration tests (taffy patch guards, fixtures)
├── examples/               example apps
└── docs/                   this site
```

## Building and testing

```sh
bun install
cargo build              # debug host
cargo test               # Rust tests: CSS, cascade, cull, transitions, native, taffy patches
cd js && bun test        # JS tests: renderer, wire, head, storage, svg, markup, ...
```

Run an example to check a change end to end:

```sh
bun js/bin/gpuijs.js dev examples/shadcn-demo
```

## Conventions

The code follows a few habits worth keeping.

- **Comments say why, and name the library that needs it.** Most compatibility code exists because a specific library does something specific (`// Radix Select collects its items in a detached fragment.`). Keep that, because it tells the next person whether the code is still needed.
- **Known limits are marked `shortcut:`.** When code knowingly handles only part of a feature, a comment says what's missing and when to do better: `// shortcut: px translations only (vaul's); any other transform reads 'none'.` Searching for `shortcut:` lists the known gaps.
- **Unsupported CSS warns once.** Use `warn_once` in `src/css/` for anything skipped, so developers can see what was dropped.
- **Match the browser.** When in doubt, do what a browser does, in the browser's order, and say which behaviour you're matching.
- **Tests sit beside the code.** JS tests are `*.test.js` next to their module, and Rust tests are in `#[cfg(test)]` modules or `src/css/tests.rs`.

## Vendored dependencies

GPUI and Taffy are vendored in `vendor/` and patched through `[patch.crates-io]` in `Cargo.toml`. Every change is marked in the source with a `gpuijs patch` comment, and `Cargo.toml` summarizes each patch and the files it touches. When you change a vendored crate:

1. Mark the change with a `gpuijs patch:` comment explaining it.
2. Add or update its line in the comment above `[patch.crates-io]` in `Cargo.toml`.
3. For Taffy layout fixes, add a guard test in `tests/taffy_patch.rs`.

Several GPUI patches change Metal shaders (`platform/mac/shaders.metal`). Linux's `shaders.wgsl` hasn't been updated to match, which is one reason gpuijs is macOS only.

Dependencies build with `opt-level = 3` even in debug builds. CSS parsing, layout, text and SVG are otherwise too slow to iterate on, while gpuijs's own crate stays at `opt-level = 1` for quick rebuilds.

## Adding library support

Most "library X doesn't work" problems are a missing global or DOM method.

1. Run the library in an example and read the error. It usually names what's missing (`document.foo is not a function`).
2. Add the smallest thing that makes it work: a global in `js/src/env.js`, or a method on `HostNode` in `js/src/renderer.js`.
3. Write the comment: which library needs it, and what it does with it.
4. Add a test in `js/src/renderer.test.js` (or the relevant `*.test.js`).

For CSS a library needs, add the property in `src/css/props.rs` and a test in `src/css/tests.rs`. Tests there build a small tree, run the cascade and check the computed GPUI style.

## Working on the docs

The docs are Markdown in `docs/pages/`, built into a static site by `docs/build.ts`:

```sh
bun docs/build.ts        # writes docs/dist/
open docs/dist/index.html
```

- The sidebar order and page titles are in the `NAV` list at the top of `docs/build.ts`. A new page needs an entry there.
- Each page starts with `# Title`, which the build replaces with the title from `NAV`.
- `> [!NOTE]`, `> [!TIP]` and `> [!WARNING]` blockquotes become callouts.
- Link between pages as `page.html` or `page.html#heading-id`. The build fails on a link to a missing page or heading.
- Code blocks are highlighted by highlight.js in the browser. Tag them with a language (`tsx`, `ts`, `css`, `sh`, `json`, `rust`, `text`).
