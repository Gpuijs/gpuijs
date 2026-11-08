import { useRef, useState } from "react"

export function App() {
  return (
    <div className="flex flex-col gap-6 p-6 text-sm text-neutral-900">
      <DetailsCard />
      <DialogCard />
    </div>
  )
}

// Native disclosure: one alone, and an accordion of details sharing a `name`.
function DetailsCard() {
  const [log, setLog] = useState<string[]>([])
  const note = (line: string) => setLog((l) => [line, ...l].slice(0, 4))
  return (
    <section className="flex flex-col gap-3 rounded-lg border border-neutral-300 bg-white p-4">
      <h2 className="text-base font-semibold">Details</h2>
      <details onToggle={(e) => note(`alone: ${e.currentTarget.open ? "open" : "closed"}`)}>
        <summary>What is gpuijs?</summary>
        <p className="mt-2 text-neutral-600">React apps rendered natively by GPUI.</p>
      </details>
      {["Shipping", "Returns", "Warranty"].map((title) => (
        <details key={title} name="faq" className="rounded border border-neutral-200 px-3 py-2" onToggle={(e) => e.currentTarget.open && note(`faq: ${title}`)}>
          <summary className="font-medium">{title}</summary>
          <p className="mt-2 text-neutral-600">Everything about {title.toLowerCase()}. Opening another section closes this one.</p>
        </details>
      ))}
      <div className="font-mono text-xs text-neutral-500">{log.join(" · ")}</div>
    </section>
  )
}

// A non-modal dialog, and a modal one with a styled backdrop and `<form method="dialog">`.
function DialogCard() {
  const modal = useRef<HTMLDialogElement>(null)
  const panel = useRef<HTMLDialogElement>(null)
  const [result, setResult] = useState("")
  return (
    <section className="flex flex-col gap-3 rounded-lg border border-neutral-300 bg-white p-4">
      <h2 className="text-base font-semibold">Dialog</h2>
      <div className="flex gap-2">
        <button className="rounded bg-neutral-900 px-3 py-1.5 text-white" onClick={() => modal.current!.showModal()}>
          showModal()
        </button>
        <button className="rounded border border-neutral-300 px-3 py-1.5" onClick={() => (panel.current!.open ? panel.current!.close() : panel.current!.show())}>
          show() / close()
        </button>
        <span className="self-center font-mono text-xs text-neutral-500">returnValue: {result || "—"}</span>
      </div>
      <dialog ref={panel} className="relative mt-2 rounded border border-neutral-300 p-3">
        A non-modal dialog, in place. The page stays usable.
      </dialog>
      <dialog
        ref={modal}
        className="w-80 rounded-xl border-0 p-5 shadow-xl backdrop:bg-black/50"
        onClose={() => setResult(modal.current!.returnValue)}
        // Clicks on the backdrop are the dialog's own: close on a click outside its box.
        onClick={(e) => e.target === modal.current && modal.current!.close("backdrop")}
      >
        <form method="dialog" className="flex flex-col gap-3">
          <h3 className="text-base font-semibold">Delete project?</h3>
          <p className="text-neutral-600">Escape, a click outside, or a button closes this.</p>
          <input className="rounded border border-neutral-300 px-2 py-1" placeholder="Focused when opened" />
          <div className="flex justify-end gap-2">
            <button value="cancel" className="rounded border border-neutral-300 px-3 py-1.5">Cancel</button>
            <button value="delete" className="rounded bg-red-600 px-3 py-1.5 text-white">Delete</button>
          </div>
        </form>
      </dialog>
    </section>
  )
}
