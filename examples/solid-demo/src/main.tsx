// gpuijs/solid comes first: it sets up `document` before Solid loads.
import { Window, render } from "gpuijs/solid"
import "./app.css"
import { App } from "./App"

new Window({ width: 480, height: 560 })
render(() => <App />)
