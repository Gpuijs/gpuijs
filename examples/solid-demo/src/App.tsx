import { For, Show, createSignal } from "solid-js"

type Todo = { id: number; text: string; done: boolean }

export function App() {
  let next = 3
  const [todos, setTodos] = createSignal<Todo[]>([
    { id: 1, text: "Render Solid with GPUI", done: true },
    { id: 2, text: "Toggle one", done: false },
  ])
  const [draft, setDraft] = createSignal("")
  const left = () => todos().filter((t) => !t.done).length

  const add = () => {
    const text = draft().trim()
    if (!text) return
    setTodos([...todos(), { id: next++, text, done: false }])
    setDraft("")
  }
  const toggle = (todo: Todo) => setTodos(todos().map((t) => (t === todo ? { ...t, done: !t.done } : t)))
  const remove = (todo: Todo) => setTodos(todos().filter((t) => t !== todo))

  return (
    <main class="mx-auto flex max-w-md flex-col gap-3 p-6">
      <h1 class="text-2xl font-bold text-indigo-600">Todos</h1>
      <input
        value={draft()}
        placeholder="What needs doing?"
        class="rounded-md border border-gray-300 px-3 py-2"
        onInput={(e) => setDraft(e.currentTarget.value)}
        onKeyDown={(e) => e.key === "Enter" && add()}
      />
      <ul class="flex flex-col gap-2">
        <For each={todos()}>
          {(todo) => (
            <li class="flex items-center gap-2">
              <input type="checkbox" checked={todo.done} onChange={() => toggle(todo)} />
              <span class="flex-1" classList={{ "text-gray-400 line-through": todo.done }}>
                {todo.text}
              </span>
              <button class="text-sm text-red-600" onClick={() => remove(todo)}>
                Remove
              </button>
            </li>
          )}
        </For>
      </ul>
      <Show when={todos().length} fallback={<p class="text-sm text-gray-500">Nothing to do.</p>}>
        <p class="text-sm text-gray-500">{left()} left</p>
      </Show>
    </main>
  )
}
