// The same App rendered by react-dom, as a visual reference for the gpui window.
import { createRoot } from "react-dom/client"

import { App } from "../src/App"

createRoot(document.getElementById("root")!).render(
  <App setDark={(dark) => document.documentElement.classList.toggle("dark", dark)} />,
)
