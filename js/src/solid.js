// `gpuijs/solid`: a Solid app in the window. Import it first, before anything that loads Solid.
import "./env.js";
import { render as mount } from "solid-js/web";
export * from "./index.js";

/** Renders `() => <App />` into the window; returns a function that unmounts it. */
export function render(code) {
  return mount(code, document.body);
}
