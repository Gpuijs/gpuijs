import { useEffect, useRef, useState } from "react"
import { Chart, registerables } from "chart.js"

Chart.register(...registerables)

export function App() {
  return (
    <div className="page">
      <CanvasCard />
      <VideoCard />
      <AudioCard />
    </div>
  )
}

// A 2D canvas drawn by hand every frame, and a Chart.js chart.
function CanvasCard() {
  const spinner = useRef<HTMLCanvasElement>(null)
  const chartCanvas = useRef<HTMLCanvasElement>(null)
  const chart = useRef<Chart | null>(null)

  useEffect(() => {
    const ctx = spinner.current!.getContext("2d")!
    let frame = 0
    const draw = (t: number) => {
      ctx.clearRect(0, 0, 200, 200)
      ctx.save()
      ctx.translate(100, 100)
      ctx.rotate(t / 600)
      const gradient = ctx.createLinearGradient(-60, 0, 60, 0)
      gradient.addColorStop(0, "#ff6b6b")
      gradient.addColorStop(1, "#4d96ff")
      ctx.fillStyle = gradient
      ctx.fillRect(-60, -20, 120, 40)
      ctx.restore()
      ctx.fillStyle = "#1f2328"
      ctx.font = "14px system-ui"
      ctx.fillText(`t = ${(t / 1000).toFixed(1)}s`, 10, 190)
      frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(frame)
  }, [])

  useEffect(() => {
    chart.current = new Chart(chartCanvas.current!, {
      type: "bar",
      data: { labels: ["Mon", "Tue", "Wed", "Thu", "Fri"], datasets: [{ label: "Visits", data: [12, 19, 7, 15, 10], backgroundColor: "#4d96ff" }] },
      options: { responsive: false },
    })
    return () => chart.current?.destroy()
  }, [])

  const shuffle = () => {
    const c = chart.current!
    c.data.datasets[0].data = c.data.labels!.map(() => Math.round(Math.random() * 20))
    c.update()
  }

  return (
    <section className="card">
      <h2>Canvas</h2>
      <div className="row">
        <canvas ref={spinner} width={200} height={200} />
        <div>
          <canvas ref={chartCanvas} className="chart" width={420} height={210} />
          <div className="bar">
            <button onClick={shuffle}>Shuffle data</button>
          </div>
        </div>
      </div>
    </section>
  )
}

function useLog() {
  const [log, setLog] = useState<string[]>([])
  return [log, (line: string) => setLog((l) => [line, ...l].slice(0, 6))] as const
}

// Native controls, plus the same element driven from JS, with the events it fires.
function VideoCard() {
  const video = useRef<HTMLVideoElement>(null)
  const [time, setTime] = useState(0)
  const [log, add] = useLog()
  return (
    <section className="card">
      <h2>Video</h2>
      <div className="row">
        <video
          ref={video}
          src="/clip.mp4"
          width={480}
          controls
          autoPlay
          muted
          loop
          onLoadedMetadata={(e) => add(`loadedmetadata ${e.currentTarget.duration.toFixed(2)}s ${e.currentTarget.videoWidth}×${e.currentTarget.videoHeight}`)}
          onPlay={() => add("play")}
          onPause={() => add("pause")}
          onSeeked={() => add("seeked")}
          onTimeUpdate={(e) => setTime(e.currentTarget.currentTime)}
        />
        <div className="log">
          {log.map((l, i) => (
            <div key={i}>{l}</div>
          ))}
        </div>
      </div>
      <div className="bar">
        <button onClick={() => video.current!.play()}>play()</button>
        <button onClick={() => video.current!.pause()}>pause()</button>
        <button onClick={() => (video.current!.currentTime = 3)}>currentTime = 3</button>
        <button onClick={() => (video.current!.muted = !video.current!.muted)}>toggle muted</button>
        <button onClick={() => (video.current!.playbackRate = video.current!.playbackRate === 1 ? 2 : 1)}>1× / 2×</button>
        <span>{time.toFixed(2)}s</span>
      </div>
    </section>
  )
}

// An <audio controls> bar, and a sound played with `new Audio()` outside the page.
function AudioCard() {
  const [log, add] = useLog()
  const beep = () => {
    const a = new Audio("/beep.wav")
    a.addEventListener("ended", () => add("beep ended"))
    a.play()
  }
  return (
    <section className="card">
      <h2>Audio</h2>
      <audio src="/tone.m4a" controls onPlay={() => add("tone play")} onEnded={() => add("tone ended")} />
      <div className="bar">
        <button onClick={beep}>new Audio("/beep.wav").play()</button>
      </div>
      <div className="log">
        {log.map((l, i) => (
          <div key={i}>{l}</div>
        ))}
      </div>
    </section>
  )
}
