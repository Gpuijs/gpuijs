// gpuijs/vue comes first: it sets up `document` before Vue loads.
import { Window, render } from "gpuijs/vue"
import "./app.css"
import App from "./App.vue"

new Window({ width: 480, height: 560 })
render(App)
