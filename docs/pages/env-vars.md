# Environment variables

gpuijs is configured by convention. Environment variables cover the rest: overrides for development and benchmarking, and the plumbing between the CLI, the host and the app.

## Variables you set

| Variable | Read by | Effect |
| --- | --- | --- |
| `GPUIJS_HOST` | CLI | The host binary to run (`dev`) or package (`build`), instead of the installed `gpuijs-darwin-*` one (in a gpuijs checkout, `target/debug/gpuijs` or a fresh `cargo build --release`) |
| `GPUIJS_PRODUCTION` | CLI | With `dev`: bundle production builds of the framework and libraries, and minify, as `build` does. Use it for benchmarks. |
| `GPUIJS_WINDOW_SIZE` | Host | `WIDTHxHEIGHT`, like `1440x900`. Opens the window at that size, overriding `new Window()`'s `width` and `height`. |
| `GPUIJS_PROFILE` | Host | Prints a per-frame timing breakdown to stderr. See [Performance](performance.html#frame-profiling). |
| `GPUIJS_BENCH_SOCKET` | Host | A Unix socket path where the host listens for a benchmark driver. See [Performance](performance.html#benchmark-driver). |
| `ROUTE` | App | The initial URL path, like `/settings`, for apps with a client-side router |

```sh
GPUIJS_WINDOW_SIZE=1440x900 GPUIJS_PROFILE=1 ROUTE=/calendar bun js/bin/gpuijs.js dev examples/my-app
```

## Variables gpuijs sets

You don't set these. They're listed to explain what's going on, and for debugging.

| Variable | Set by | Meaning |
| --- | --- | --- |
| `GPUIJS_CSS` | CLI, or the packaged host | The compiled stylesheet the app sends to the host at startup |
| `GPUIJS_PUBLIC` | CLI, or the packaged host | The `public/` directory that root-relative asset URLs resolve against |
| `GPUIJS_APP_NAME` | CLI, or the packaged host | The app's name, for `app.getName()` and menu role labels |
| `GPUIJS_BODY_CLASS` | CLI | Classes from `index.html`'s `<body class>`, added to `<body>`. A packaged app has them compiled into its bundle. |
| `GPUIJS_BUN` | CLI | The Bun executable the host uses to run the bundle in development |
| `GPUIJS_SOCKET` | Host | The Unix socket the app connects to for host-to-app messages |
| `GPUIJS_IN` | Host | The named pipe the app writes its messages to |
| `GPUIJS_SYNC` | Host | The named pipe the host answers forced layouts on |
| `GPUIJS_DATA_DIR` | Packaged host | `~/Library/Application Support/<id>`, where `localStorage` is kept |
| `GPUIJS_CACHE_DIR` | Packaged host | `~/Library/Caches/<id>`, where remote images are cached |

Running a bundle directly with `bun .gpuijs/app.js` fails with `GPUIJS_SOCKET is not set; launch this bundle through the gpuijs host`. The host has to start the app. See [Architecture](architecture.html#two-processes).
