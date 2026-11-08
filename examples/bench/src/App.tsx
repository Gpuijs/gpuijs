import { useLayoutEffect, useRef, useState } from "react"

// Each action's time runs from the click to a layout read of a fresh node after the list. A new node
// has no cached rect, so in gpuijs the read waits until the host has restyled and laid out the commit.
type Row = { id: number; label: string }

let nextId = 0
const rows = (n: number) => Array.from({ length: n }, () => ({ id: nextId, label: `row ${nextId++}` }))
// `BENCH_AUTO=1 bun run dev` (or `?auto` in a browser) clicks every button in turn, then exits.
const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env
const inBrowser = typeof location !== "undefined"
const auto = inBrowser ? location.search.includes("auto") : !!env?.BENCH_AUTO

export function App() {
  const [list, setList] = useState<Row[]>([])
  const [spin, setSpin] = useState(false)
  const [run, setRun] = useState<{ name: string; at: number; n: number } | null>(null)
  const [log, setLog] = useState<string[]>([])
  const probe = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    if (!run || !probe.current) return
    probe.current.offsetHeight
    const line = `${run.name}: ${(performance.now() - run.at).toFixed(1)} ms`
    console.log(line)
    setLog((l) => [line, ...l].slice(0, 8))
    setRun(null)
    if (auto) setTimeout(() => (script.shift() ?? (() => inBrowser || globalThis.process!.exit(0)))(), 300)
  }, [run])

  const act = (name: string, next: (l: Row[]) => Row[]) => () => {
    setRun((r) => ({ name, at: performance.now(), n: (r?.n ?? 0) + 1 }))
    setList(next)
  }

  const mount = act("mount 10k", () => rows(10_000))
  const updateAll = act("update all", (l) => l.map((r) => ({ ...r, label: r.label + "!" })))
  const updateTenth = act("update every 10th", (l) => l.map((r, i) => (i % 10 ? r : { ...r, label: r.label + "!" })))
  const swap = act("swap 2", (l) => (l.length > 998 ? [l[0], l[998], ...l.slice(2, 998), l[1], ...l.slice(999)] : l))
  const clear = act("clear", () => [])
  const script = useRef(Array(10).fill(0).flatMap(() => [updateAll, updateTenth, swap, clear, mount])).current
  useLayoutEffect(() => {
    if (auto) setTimeout(mount, 500)
  }, [])

  return (
    <div>
      <div className="bar">
        <button onClick={mount}>mount 10k</button>
        <button onClick={updateAll}>update all</button>
        <button onClick={updateTenth}>update 1/10</button>
        <button onClick={swap}>swap</button>
        <button onClick={clear}>clear</button>
        <button onClick={() => setSpin((s) => !s)}>spinner</button>
        {spin && <div className="spin" />}
      </div>
      <div className="bar">{log.join(" · ")}</div>
      <div className="list">
        {list.map((r) => (
          <div className={r.id % 2 ? "row odd" : "row"} key={r.id}>
            <span>{r.id}</span>
            <span>{r.label}</span>
          </div>
        ))}
      </div>
      <div ref={probe} key={run?.n ?? 0} />
    </div>
  )
}
