// Input-to-pixels latency on real pages: TailAdmin in gpuijs and in Chrome (a visible window), at a
// 1440×900 viewport, production builds (`--dev` for development ones). Each sample runs from an
// input entering the app to the frame showing its result being drawn:
// - gpuijs: injected where macOS input enters gpui; until the GPU finished the first frame drawn
//   after JS handled it (the host's `bench`).
// - Chrome: injected with DevTools' `Input.dispatch*`, the browser's own input path; until the GPU
//   process swapped the first frame with its result (`LatchToSwapEnd` in a trace: Chrome's own
//   `EventLatency` for the input, else the first frame committing main-thread work begun after
//   the input was handled). That is when the frame goes to the system compositor, a little before
//   its GPU work is done, so it is slightly generous to Chrome. The in-page "next paint" (second
//   `requestAnimationFrame` after the event) is shown alongside: it fires as the frame after the
//   one with the result begins, so it runs up to a frame late.
//
//   bun examples/bench/interactions.ts [--only gpuijs|chrome] [--scenario hover] [--dev] [--rounds 3]
//
// Keep both windows uncovered, the screen unlocked and the mouse still while it runs: a hidden
// window draws nothing (it stops with an error then), and real input would mix in. The display is
// kept awake for the run.
import { spawn, spawnSync, type ChildProcess } from "node:child_process"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

type Input =
  | { kind: "move" | "click"; selector: string; index?: number }
  | { kind: "wheel"; dy: number }
  | { kind: "key"; text: string }
type Rect = { x: number; y: number; width: number; height: number; value: string | null }
type Scenario = {
  name: string
  route: string
  setup: Input[]
  steps: Input[]
  /** After the steps: why the inputs evidently didn't do their job, or null. */
  check?: (target: Target) => Promise<string | null>
}
type Target = {
  rect(selector: string, index?: number): Promise<Rect | null>
  /** Injects an input at window coordinates and returns its latency in ms (`raf`: Chrome's next paint). */
  send(input: { kind: string; x?: number; y?: number; dy?: number; text?: string }): Promise<{ ms: number | null; raf?: number | null }>
  close(): Promise<void>
}

const args = process.argv.slice(2)
const flag = (name: string) => args.includes(name)
const option = (name: string) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined)
const dev = flag("--dev")
const rounds = Number(option("--rounds") ?? 3)
const only = args.includes("--only") ? option("--only") : null
const picked = args.includes("--scenario") ? option("--scenario") : null
const [width, height] = [1440, 900]
const root = path.resolve(import.meta.dir, "../..")
const app = path.join(root, "examples/tailadmin")
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
// The display asleep hides every window; keep it awake until this process ends.
spawn("caffeinate", ["-d", "-i", "-w", `${process.pid}`], { stdio: "ignore" }).unref()

const menuItems = Array.from({ length: 8 }, (_, index) => ({ kind: "move" as const, selector: "aside .menu-item", index }))
const repeat = <T,>(items: T[], n: number) => Array.from({ length: n }, () => items).flat()
const typed = "the quick brown fox jumps"
const scenarios: Scenario[] = [
  // Hover styles on the sidebar's items, as the pointer passes over them.
  { name: "hover sidebar", route: "/", setup: [], steps: repeat(menuItems, rounds * 2) },
  // A controlled field (React state per key) in the calendar's modal, over its blurred backdrop.
  {
    name: "type in modal",
    route: "/calendar",
    setup: [
      { kind: "click", selector: "button.bg-brand-500\\!" },
      { kind: "click", selector: "#event-title" },
    ],
    steps: repeat([...typed].map((text) => ({ kind: "key" as const, text })), rounds),
    async check(target) {
      const value = (await target.rect("#event-title"))?.value
      return value === typed.repeat(rounds) ? null : `the field holds ${JSON.stringify(value)}`
    },
  },
  // The dashboard (cards, charts, a map, a table) scrolled down by wheel steps, then back up.
  {
    name: "scroll dashboard",
    route: "/",
    setup: [],
    steps: repeat([...Array(10).fill({ kind: "wheel", dy: 120 }), ...Array(10).fill({ kind: "wheel", dy: -120 })], rounds),
    async check(target) {
      const before = (await target.rect(".col-span-12"))?.y ?? 0
      await run(target, { kind: "wheel", dy: 300 })
      await sleep(300)
      const after = (await target.rect(".col-span-12"))?.y ?? 0
      return after < before - 100 ? null : `the page didn't scroll (content at ${before} → ${after})`
    },
  },
  // Client-side navigation between two real pages, until the new page's first frame.
  {
    name: "navigate pages",
    route: "/",
    setup: [],
    steps: repeat(
      [
        { kind: "click", selector: 'aside a[href="/calendar"]' },
        { kind: "click", selector: 'aside a[href="/profile"]' },
      ],
      rounds * 10,
    ),
    // It ends on the profile page.
    check: async (target) => ((await target.rect('img[alt="user"]')) ? null : "the profile page isn't showing"),
  },
]

/** Resolves an input's target to window coordinates, then sends it. */
async function run(target: Target, input: Input): ReturnType<Target["send"]> {
  if (input.kind === "wheel") return target.send({ kind: "wheel", x: width / 2, y: height / 2, dy: input.dy })
  if (input.kind === "key") return target.send(input)
  const rect = await target.rect(input.selector, input.index)
  if (!rect) throw new Error(`nothing matches ${input.selector} [${input.index ?? 0}]`)
  return target.send({ kind: input.kind, x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 })
}

// ---- gpuijs: the host's bench socket (src/bench.rs) ----

async function gpuijs(route: string): Promise<Target> {
  const dir = mkdtempSync(path.join(tmpdir(), "gpuijs-bench-"))
  const socketPath = path.join(dir, "bench.sock")
  const env = { ...process.env, GPUIJS_BENCH_SOCKET: socketPath, GPUIJS_WINDOW_SIZE: `${width}x${height}`, ROUTE: route }
  if (!dev) env.GPUIJS_PRODUCTION = "1"
  const proc = spawn("bun", [path.join(root, "js/bin/gpuijs.js"), "dev", app], { env, stdio: ["ignore", "ignore", "inherit"] })
  const waiting = new Map<number, (reply: any) => void>()
  let buffer = ""
  let socket: Awaited<ReturnType<typeof Bun.connect>> | undefined
  for (let i = 0; i < 300 && !socket; i++) {
    await sleep(100)
    socket = await Bun.connect({
      unix: socketPath,
      socket: {
        data(_, data) {
          buffer += data.toString()
          let nl
          while ((nl = buffer.indexOf("\n")) >= 0) {
            const reply = JSON.parse(buffer.slice(0, nl))
            buffer = buffer.slice(nl + 1)
            waiting.get(reply.id)?.(reply)
            waiting.delete(reply.id)
          }
        },
      },
    }).catch(() => undefined)
  }
  if (!socket) throw new Error("gpuijs: the bench socket never opened")
  let nextId = 1
  const call = (command: object) =>
    new Promise<any>((resolve) => {
      const id = nextId++
      waiting.set(id, resolve)
      socket!.write(JSON.stringify({ id, ...command }) + "\n")
    })
  return {
    rect: async (selector, index = 0) => (await call({ cmd: "query", selector, index })).rect,
    async send(input) {
      const reply = await call({ cmd: "input", input })
      if (reply.error) throw new Error(`gpuijs: ${reply.error}`)
      return { ms: reply.ms }
    },
    async close() {
      socket!.end()
      proc.kill()
      await new Promise((r) => proc.once("exit", r))
      rmSync(dir, { recursive: true, force: true })
    },
  }
}

// ---- Chrome: DevTools protocol over a WebSocket ----

const chromePath = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
// Marks the next event of a type, and resolves with the time from its creation to the second
// animation frame after it.
const harness = `({
  arm(type) {
    this.result = new Promise((resolve) => addEventListener(type, (e) => {
      const start = e.timeStamp
      requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now() - start)))
    }, { capture: true, once: true, passive: true }))
  },
})`
const domEvent: Record<string, string> = { move: "pointermove", click: "click", wheel: "wheel", key: "keydown" }

async function chrome(url: string, chromeRoute: string): Promise<Target> {
  const profile = mkdtempSync(path.join(tmpdir(), "gpuijs-bench-chrome-"))
  const port = 9300 + Math.floor(Math.random() * 600)
  const proc: ChildProcess = spawn(
    chromePath,
    [
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profile}`,
      `--app=${url}${chromeRoute}`,
      `--window-size=${width},${height + 28}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      // A visible window, measured as one: no throttling if it loses focus.
      "--disable-backgrounding-occluded-windows",
      "--disable-renderer-backgrounding",
      "--disable-background-timer-throttling",
    ],
    { stdio: "ignore" },
  )
  let ws: string | undefined
  for (let i = 0; i < 100 && !ws; i++) {
    await sleep(100)
    const pages = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).catch(() => [])
    ws = pages.find((p: any) => p.type === "page")?.webSocketDebuggerUrl
  }
  if (!ws) throw new Error("chrome: no page to attach to")
  const page = await devtools(ws)
  const cdp = page.cdp
  // Traces are browser-wide (the GPU process's swaps included), so they go to the browser target.
  const browserWs = (await fetch(`http://127.0.0.1:${port}/json/version`).then((r) => r.json())).webSocketDebuggerUrl
  const browser = await devtools(browserWs)
  const evaluate = async (expression: string, awaitPromise = false) => {
    const { result, exceptionDetails } = await cdp("Runtime.evaluate", { expression, awaitPromise, returnByValue: true })
    if (exceptionDetails) throw new Error(`chrome: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`)
    return result.value
  }
  // The viewport, not the window, is 1440×900: grow the window by what the frame takes.
  await sleep(500)
  const { windowId, bounds } = await cdp("Browser.getWindowForTarget")
  const inner = await evaluate("[innerWidth, innerHeight]")
  await cdp("Browser.setWindowBounds", {
    windowId,
    bounds: { width: bounds.width + width - inner[0], height: bounds.height + height - inner[1] },
  })
  return {
    rect: (selector, index = 0) =>
      evaluate(`(() => {
        const el = document.querySelectorAll(${JSON.stringify(selector)})[${index}]
        const r = el?.getBoundingClientRect()
        return r ? { x: r.x, y: r.y, width: r.width, height: r.height, value: el.value ?? null } : null
      })()`),
    async send(input) {
      if ((await evaluate("document.visibilityState")) !== "visible") throw new Error("chrome: the window is hidden")
      await evaluate(`(window.__bench ??= ${harness}).arm(${JSON.stringify(domEvent[input.kind])})`)
      const events: any[] = []
      const collect = browser.on("Tracing.dataCollected", (p) => events.push(...p.value))
      // The categories with `EventLatency` and the frame pipeline only, to add little to Chrome's work.
      await browser.cdp("Tracing.start", { transferMode: "ReportEvents", traceConfig: { includedCategories: ["benchmark", "input"] } })
      // Renderer and GPU processes start recording a moment after the browser.
      await sleep(100)
      const { x = 0, y = 0 } = input
      if (input.kind === "move") await cdp("Input.dispatchMouseEvent", { type: "mouseMoved", x, y })
      if (input.kind === "click") {
        await cdp("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 })
        await cdp("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 })
      }
      if (input.kind === "wheel") await cdp("Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY: input.dy })
      if (input.kind === "key") {
        const text = input.text!
        await cdp("Input.dispatchKeyEvent", { type: "keyDown", key: text, text, unmodifiedText: text })
        await cdp("Input.dispatchKeyEvent", { type: "keyUp", key: text })
      }
      // No frames come if the window got hidden meanwhile.
      const raf = await evaluate("Promise.race([__bench.result, new Promise((r) => setTimeout(() => r(null), 3000))])", true)
      // The swap is recorded when its presentation is reported, a frame or two later.
      await sleep(150)
      const complete = new Promise((r) => browser.once("Tracing.tracingComplete", r))
      await browser.cdp("Tracing.end")
      await complete
      collect()
      return { ms: swapLatency(events, input.kind), raf }
    },
    async close() {
      page.close()
      browser.close()
      proc.kill()
      await new Promise((r) => proc.once("exit", r))
      rmSync(profile, { recursive: true, force: true })
    },
  }
}

/** A DevTools protocol connection: commands, and listeners for its events. */
async function devtools(url: string) {
  const socket = new WebSocket(url)
  await new Promise((r) => socket.addEventListener("open", r, { once: true }))
  const waiting = new Map<number, (reply: any) => void>()
  const listeners = new Set<{ method: string; fn: (params: any) => void }>()
  socket.addEventListener("message", (m) => {
    const message = JSON.parse(String(m.data))
    if (message.id) waiting.get(message.id)?.(message)
    else for (const l of listeners) if (l.method === message.method) l.fn(message.params)
  })
  let nextId = 1
  const on = (method: string, fn: (params: any) => void) => {
    const l = { method, fn }
    listeners.add(l)
    return () => listeners.delete(l)
  }
  return {
    cdp: (method: string, params: object = {}) =>
      new Promise<any>((resolve, reject) => {
        const id = nextId++
        waiting.set(id, (reply) => (reply.error ? reject(new Error(`${method}: ${reply.error.message}`)) : resolve(reply.result)))
        socket.send(JSON.stringify({ id, method, params }))
      }),
    on,
    once: (method: string, fn: (params: any) => void) => {
      const off = on(method, (p) => (off(), fn(p)))
    },
    close: () => socket.close(),
  }
}

// The `EventLatency` an injected input starts from: a click is timed from its release.
const latencyType: Record<string, string> = { move: "MOUSE_MOVED_EVENT", click: "MOUSE_RELEASED", wheel: "MOUSE_WHEEL", key: "KEY_PRESSED" }

/**
 * From a trace of one input: ms from its creation to the GPU process's swap of the first frame
 * with its result. Chrome's `EventLatency` follows the input to that frame when it caused one (a
 * wheel's scroll goes on as gesture events of the same start); a mouse move's ends once handled,
 * when Chrome finds nothing it changed, so then it is the first frame whose main-thread work
 * committed after the input was handled (mouse moves are handled within a frame's main-thread
 * work). Null when neither is in the trace.
 */
function swapLatency(events: any[], kind: string): number | null {
  // Slices nest under an `EventLatency` or `PipelineReporter` root. Ids repeat (and are local to
  // a process), so a slice belongs to the open root with its id on its thread.
  type Tree = { name: string; pid: number; type?: string; start: number; stages: Map<string, { start: number; end?: number }> }
  const trees: Tree[] = []
  const open = new Map<string, Tree>()
  for (const e of events.filter((e) => e.ph === "b" || e.ph === "e").sort((a, b) => a.ts - b.ts)) {
    const key = `${e.pid}:${e.tid}:${e.id2?.local ?? e.id2?.global ?? e.id}`
    if (e.ph === "b" && (e.name === "EventLatency" || e.name === "PipelineReporter")) {
      const tree = { name: e.name, pid: e.pid, type: e.args?.event_latency?.event_type, start: e.ts, stages: new Map() }
      open.set(key, tree)
      trees.push(tree)
      continue
    }
    const tree = open.get(key)
    if (!tree) continue
    if (e.ph === "b") tree.stages.set(e.name, { start: e.ts })
    else if (e.name === tree.name) open.delete(key)
    else {
      const stage = tree.stages.get(e.name)
      if (stage) stage.end = e.ts
    }
  }
  const swap = (t: Tree) => t.stages.get("LatchToSwapEnd")?.end
  const input = trees.find((t) => t.name === "EventLatency" && t.type === latencyType[kind])
  if (!input) return null
  const ms = (end: number) => (end - input.start) / 1000
  const followed = trees.filter((t) => t.name === "EventLatency" && t.start >= input.start && swap(t))
  if (followed.length) return ms(Math.min(...followed.map((t) => swap(t)!)))
  const handled = input.stages.get("RendererMainProcessing")?.end
  if (handled == null) return null
  const frames = trees.filter(
    (t) => t.name === "PipelineReporter" && t.pid === input.pid && swap(t) && (t.stages.get("SendBeginMainFrameToCommit")?.end ?? 0) > handled,
  )
  return frames.length ? ms(Math.min(...frames.map((t) => swap(t)!))) : null
}

/** Serves TailAdmin's web build (or its dev server with `--dev`); returns its URL and a stop. */
async function serveWeb(): Promise<{ url: string; stop: () => void }> {
  const vite = path.join(app, "node_modules/.bin/vite")
  const port = 4300 + Math.floor(Math.random() * 600)
  let out: string | undefined
  if (!dev) {
    out = mkdtempSync(path.join(tmpdir(), "gpuijs-bench-web-"))
    const built = spawnSync(vite, ["build", "--outDir", out, "--logLevel", "error"], { cwd: app, stdio: "inherit" })
    if (built.status) throw new Error("vite build failed")
  }
  const serveArgs = dev ? ["--port", `${port}`, "--strictPort"] : ["preview", "--outDir", out!, "--port", `${port}`, "--strictPort"]
  const server = spawn(vite, serveArgs, { cwd: app, stdio: "ignore" })
  const url = `http://localhost:${port}`
  for (let i = 0; i < 100; i++) {
    if (await fetch(url).then((r) => r.ok).catch(() => false)) break
    await sleep(100)
  }
  return {
    url,
    stop() {
      server.kill()
      if (out) rmSync(out, { recursive: true, force: true })
    },
  }
}

// ---- running and reporting ----

const failures: string[] = []

/** Opens the app at the scenario's page, waits for it, runs the setup, then times every step. */
async function measure(
  open: (route: string) => Promise<Target>,
  scenario: Scenario,
  name: string,
): Promise<{ times: number[]; raf: number[] }> {
  const target = await open(scenario.route)
  try {
    for (let i = 0; i < 300 && !(await target.rect("aside .menu-item")); i++) await sleep(100)
    // Charts, fonts and images settle.
    await sleep(2500)
    for (const input of scenario.setup) {
      await run(target, input)
      await sleep(500)
    }
    const [times, raf]: number[][] = [[], []]
    for (const input of scenario.steps) {
      const sample = await run(target, input)
      if (sample.ms != null) times.push(sample.ms)
      if (sample.raf != null) raf.push(sample.raf)
      // Transitions started by the last input finish before the next.
      await sleep(input.kind === "click" ? 400 : 60)
    }
    const problem = await scenario.check?.(target)
    if (problem) failures.push(`${name}, ${scenario.name}: ${problem}`)
    if (times.length < scenario.steps.length) failures.push(`${name}, ${scenario.name}: ${scenario.steps.length - times.length} inputs untimed`)
    return { times, raf }
  } finally {
    await target.close()
  }
}

const stats = (times: number[]) => {
  const sorted = [...times].sort((a, b) => a - b)
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))]
  return { n: sorted.length, median: at(0.5), p95: at(0.95) }
}

/** A scenario that stops with an error (a window got hidden) is reported; the others still run. */
async function attempt<T>(f: () => Promise<T>, name: string): Promise<T | null> {
  try {
    return await f()
  } catch (e) {
    failures.push(`${name}: ${(e as Error).message}`)
    return null
  }
}

const chosen = scenarios.filter((s) => !picked || s.name.includes(picked))
const results: Record<string, Record<string, number[]>> = {}
if (only !== "gpuijs") {
  const web = await serveWeb()
  try {
    for (const s of chosen) {
      console.error(`chrome: ${s.name}`)
      const r = await attempt(() => measure((route) => chrome(web.url, route), s, "chrome"), `chrome, ${s.name}`)
      if (r) Object.assign((results[s.name] ??= {}), { chrome: r.times, chromeRaf: r.raf })
    }
  } finally {
    web.stop()
  }
}
if (only !== "chrome") {
  for (const s of chosen) {
    console.error(`gpuijs: ${s.name}`)
    const r = await attempt(() => measure(gpuijs, s, "gpuijs"), `gpuijs, ${s.name}`)
    if (r) (results[s.name] ??= {}).gpuijs = r.times
  }
}

const cell = (times?: number[]) => {
  if (!times?.length) return "—".padStart(22)
  const { n, median, p95 } = stats(times)
  return `${median.toFixed(1)} / ${p95.toFixed(1)} ms (${n})`.padStart(22)
}
console.log(`\nInput to pixels, ${dev ? "development" : "production"} builds, ${width}×${height}: median / p95 (samples)\n`)
console.log(`${"scenario".padEnd(18)}${"Chrome (next paint)".padStart(22)}${"Chrome".padStart(22)}${"gpuijs".padStart(22)}${"gpuijs vs Chrome".padStart(18)}`)
for (const [name, r] of Object.entries(results)) {
  const ratio = r.chrome?.length && r.gpuijs?.length ? stats(r.gpuijs).median / stats(r.chrome).median : null
  const verdict = ratio == null ? "" : ratio < 1 ? `${(1 / ratio).toFixed(2)}x faster` : `${ratio.toFixed(2)}x slower`
  console.log(`${name.padEnd(18)}${cell(r.chromeRaf)}${cell(r.chrome)}${cell(r.gpuijs)}${verdict.padStart(18)}`)
}
if (failures.length) console.log(`\nChecks that failed (those numbers don't mean what they say):\n${failures.map((f) => `- ${f}`).join("\n")}`)
