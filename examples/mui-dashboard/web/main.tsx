// The same dashboard rendered by react-dom, as a visual reference for the gpui window.
import { createRoot } from "react-dom/client"

import Dashboard from "../src/dashboard/Dashboard"

createRoot(document.getElementById("root")!).render(<Dashboard />)
