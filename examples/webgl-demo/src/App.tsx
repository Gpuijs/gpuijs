import { useEffect, useRef } from "react"
import * as THREE from "three"
import { Application, Graphics, Text } from "pixi.js"

export function App() {
  return (
    <div className="page">
      <ThreeCard />
      <PixiCard />
    </div>
  )
}

// Three.js on WebGL2: a lit cube with a texture from public/, spinning.
function ThreeCard() {
  const canvas = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const renderer = new THREE.WebGLRenderer({ canvas: canvas.current!, antialias: true })
    renderer.setPixelRatio(devicePixelRatio)
    renderer.setSize(320, 240, false)
    const scene = new THREE.Scene()
    scene.background = new THREE.Color(0xf6f8fa)
    const camera = new THREE.PerspectiveCamera(50, 320 / 240, 0.1, 100)
    camera.position.z = 3
    const texture = new THREE.TextureLoader().load("/checker.png")
    texture.colorSpace = THREE.SRGBColorSpace
    const cube = new THREE.Mesh(new THREE.BoxGeometry(1.2, 1.2, 1.2), new THREE.MeshStandardMaterial({ map: texture }))
    const light = new THREE.DirectionalLight(0xffffff, 2)
    light.position.set(2, 3, 4)
    scene.add(cube, light, new THREE.AmbientLight(0xffffff, 1))
    renderer.setAnimationLoop((t) => {
      cube.rotation.set(t / 2000, t / 1500, 0)
      renderer.render(scene, camera)
    })
    return () => {
      renderer.setAnimationLoop(null)
      renderer.dispose()
    }
  }, [])

  return (
    <div className="card">
      <h2>Three.js</h2>
      <p>WebGL2, a texture, lights</p>
      <canvas ref={canvas} />
    </div>
  )
}

// PixiJS v8 on WebGL: shapes and text, moved by its ticker.
function PixiCard() {
  const canvas = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const app = new Application()
    let ready = false
    app
      .init({ canvas: canvas.current!, width: 320, height: 240, background: "#1099bb", preference: "webgl", resolution: devicePixelRatio, autoDensity: true })
      .then(() => {
        ready = true
        const box = new Graphics().rect(-40, -40, 80, 80).fill(0xff3300)
        box.position.set(100, 110)
        const ball = new Graphics().circle(0, 0, 30).fill(0xffffff)
        const label = new Text({ text: "PixiJS", style: { fill: 0xffffff, fontSize: 24 } })
        label.position.set(16, 196)
        app.stage.addChild(box, ball, label)
        app.ticker.add((ticker) => {
          box.rotation += 0.02 * ticker.deltaTime
          ball.position.set(220 + Math.cos(ticker.lastTime / 500) * 50, 110 + Math.sin(ticker.lastTime / 500) * 50)
        })
      })
    return () => {
      if (ready) app.destroy()
    }
  }, [])

  return (
    <div className="card">
      <h2>PixiJS</h2>
      <p>WebGL, Graphics, Text, Ticker</p>
      <canvas ref={canvas} />
    </div>
  )
}
