// `gpuijs/vue`: a Vue app in the window. Import it before anything that loads Vue, which reads
// `document` once, as it loads.
import "./env.js";
import { createApp } from "vue";
export * from "./index.js";

/** Mounts a component, or an app from `createApp` with its plugins installed, into the window. */
export function render(app, props) {
  const vue = typeof app.mount === "function" ? app : createApp(app, props);
  vue.mount(document.body);
  return vue;
}
