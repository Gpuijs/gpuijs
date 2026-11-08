# Project structure

A gpuijs app is a plain directory. The CLI finds everything by convention, so there is no config file.

## Layout

```text
my-app/
├── package.json        name, version, and the packaged app's name and bundle id
├── index.html          optional: only its <body class="..."> is read
├── public/             optional: files served at root-relative URLs (/images/logo.png)
├── src/
│   ├── main.tsx        the entry: main.tsx, main.jsx, main.ts or main.js, in that order
│   ├── app.css         optional: a stylesheet entry nothing has to import
│   └── ...
└── .gpuijs/            build output, made by the CLI (add it to .gitignore)
```

### `src/main.*`

The entry module. The CLI looks for `src/main.tsx`, `src/main.jsx`, `src/main.ts` and then `src/main.js`, and uses the first it finds. The entry should:

1. Import its framework's entry point (`@gpuijs/core/react`, `@gpuijs/core/vue` or `@gpuijs/core/solid`) first.
2. Optionally create a `Window` and set up menus.
3. Call the entry point's `render` with the root component. See [Frameworks](frameworks.html).

### `src/app.css`

If it exists, `src/app.css` is compiled first, before any CSS imported from JavaScript. This suits apps whose global CSS isn't imported from code, such as Next.js apps that import it from a layout gpuijs never runs. Sheets that `app.css` already `@import`s aren't added a second time.

### `public/`

Root-relative URLs resolve against `public/`, as a dev server would serve them. `<img src="/avatars/ada.png">` loads `public/avatars/ada.png`. Relative URLs resolve against the current route (`./a.png` on `/profile` is `/a.png`). See [Images, SVG and assets](assets.html).

### `index.html`

gpuijs never loads `index.html` as a page. It reads one thing from it: the `class` attribute of `<body>`. Vite templates often put global classes there (`<body class="bg-gray-50 dark:bg-gray-900">`), and gpuijs applies them to its `<body>` node. Everything else in the file is ignored.

### `package.json`

The `name` and `version` fields, plus an optional `gpuijs` block, name the app:

```json
{
  "name": "my-app",
  "version": "1.2.0",
  "gpuijs": { "name": "My App", "id": "com.example.myapp" }
}
```

| Field | Used for | Default |
| --- | --- | --- |
| `gpuijs.name` | `app.getName()`, the menu bar's app menu, the `.app` name and the log file name | `productName`, then `name`, then the directory name |
| `gpuijs.id` | The bundle identifier, which also names the app's data and cache directories | `dev.gpuijs.<name>` |
| `version` | `CFBundleVersion` and `CFBundleShortVersionString` | `0.0.0` |

The window's size and look don't belong here. They come from `new Window(...)` in code.

## Dependencies

A gpuijs app depends on `@gpuijs/core` and its framework: `react` (19.3 or later), `vue` (3.5) or `solid-js` (1.9). Solid apps also need `babel-preset-solid` as a dev dependency. [Frameworks](frameworks.html) has the details for each.

```json
{
  "dependencies": {
    "@gpuijs/core": "^0.1.0",
    "vue": "^3.5"
  }
}
```

The `@gpuijs/core` package brings Tailwind v4 (`tailwindcss` and `@tailwindcss/cli`). If your CSS says `@import "tailwindcss"` and the import doesn't resolve from your app's directory, add `tailwindcss` to your app's own dependencies.

### Inside or outside the repository

An app anywhere depends on `@gpuijs/core` from npm, and npm installs the prebuilt host beside it.

To run against a gpuijs checkout instead, put the app inside `examples/` with `"@gpuijs/core": "workspace:*"`: the Bun workspace links `@gpuijs/core` for you, and the CLI finds the debug host at `target/debug/gpuijs`. An app elsewhere can depend on the checkout by path (`"@gpuijs/core": "link:gpuijs"` after `bun link` in `js/`, or `"file:../gpuijs/js"`). The CLI looks for the host relative to its own file, at `<checkout>/target/debug/gpuijs`. To use a host built somewhere else, set `GPUIJS_HOST`:

```sh
GPUIJS_HOST=/path/to/gpuijs bunx gpuijs dev .
```

## TypeScript

The CLI bundles TypeScript and JSX with Bun, so no `tsc` step is needed. For React, use the automatic JSX runtime in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true
  }
}
```

Solid apps set `"jsx": "preserve"` and `"jsxImportSource": "solid-js"` instead. Vue apps need no JSX setting.

The `@gpuijs/core` package is plain JavaScript with JSDoc comments, so it has no bundled type declarations yet.

## Path aliases

Bun's bundler reads `paths` from `tsconfig.json`, so shadcn-style `@/components/...` imports work:

```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": { "@/*": ["./src/*"] }
  }
}
```

## Build output

Everything the CLI generates goes in `<app>/.gpuijs/`:

| File | What it is |
| --- | --- |
| `app.js` | The bundled app, run by the host under Bun |
| `entry.css` | The generated Tailwind entry: one `@import` per stylesheet |
| `app.css` | The compiled stylesheet the host loads |
| `<Name>.app` | The packaged app, after `gpuijs build` |
