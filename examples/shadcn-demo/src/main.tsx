import { Window, setRootClass } from "gpuijs/react"

import { App } from "./App"

// A custom title bar: the window's content runs under the traffic lights; App draws the bar.
const win = new Window({
  width: 1024,
  height: 820,
  minWidth: 480,
  minHeight: 400,
  titleBarStyle: "hidden",
  trafficLightPosition: { x: 18, y: 18 },
})
win.render(<App setDark={(dark) => setRootClass(dark ? "dark" : "")} />)
