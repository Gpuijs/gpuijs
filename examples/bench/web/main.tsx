// The same App in react-dom, for browser/Electron baseline numbers.
import { createRoot } from "react-dom/client"

import { App } from "../src/App"

createRoot(document.getElementById("root")!).render(<App />)
