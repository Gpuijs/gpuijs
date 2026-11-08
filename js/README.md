# gpuijs

React, Vue and Solid apps rendered natively by [GPUI](https://gpui.rs). The app runs under Bun; a Rust host lays it out with the app's CSS (Tailwind included) and draws it on the GPU. macOS only for now.

```sh
bun add gpuijs react
bunx gpuijs dev .      # run the app in this directory (src/main.tsx)
bunx gpuijs build .    # package it as a macOS .app in .gpuijs/
```

```tsx
import { render } from "gpuijs/react" // or gpuijs/vue, gpuijs/solid
render(<App />)
```

Documentation: [docs.gpuijs.dev](https://docs.gpuijs.dev)
