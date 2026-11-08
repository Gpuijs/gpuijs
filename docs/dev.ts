// `bun run dev`: builds the docs, serves docs/dist, and rebuilds when anything in docs/ changes.
// shortcut: no live reload; refresh the browser after a rebuild.
import { watch } from "node:fs"
import path from "node:path"

const here = import.meta.dir
const dist = path.join(here, "dist")
// A subprocess, so a failed build (a broken link exits 1) doesn't stop the server.
const build = () => Bun.spawnSync([process.execPath, path.join(here, "build.ts")], { stdout: "inherit", stderr: "inherit" })
build()

let timer: Timer | undefined
watch(here, { recursive: true }, (_, file) => {
  if (!file || file.startsWith("dist")) return
  clearTimeout(timer)
  timer = setTimeout(build, 100)
})

const server = Bun.serve({
  port: Number(process.env.PORT ?? 4321),
  async fetch(req) {
    const name = decodeURIComponent(new URL(req.url).pathname).replace(/^\/+/, "") || "index.html"
    const file = Bun.file(path.join(dist, path.normalize(name)))
    if (!path.join(dist, path.normalize(name)).startsWith(dist) || !(await file.exists())) return new Response("Not found", { status: 404 })
    return new Response(file)
  },
})
console.log(`docs: serving on ${server.url}`)
