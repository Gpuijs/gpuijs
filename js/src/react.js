// `@gpuijs/core/react`: a React app in the window, with the `window`/`document` React libraries expect.
// Import it first, before those libraries load.
import "./env.js";
import { Window } from "./native.js";
export * from "./index.js";

/** Renders into the app's `Window`, opening a default one if the app made none. */
export function render(element) {
  (Window.getAllWindows()[0] ?? new Window()).render(element);
}
