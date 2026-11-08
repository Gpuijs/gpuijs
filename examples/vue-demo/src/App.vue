<script setup lang="ts">
import { computed, ref } from "vue"

type Todo = { id: number; text: string; done: boolean }

let next = 3
const todos = ref<Todo[]>([
  { id: 1, text: "Render Vue with GPUI", done: true },
  { id: 2, text: "Try a scoped style", done: false },
])
const draft = ref("")
const left = computed(() => todos.value.filter((t) => !t.done).length)

function add() {
  const text = draft.value.trim()
  if (!text) return
  todos.value.push({ id: next++, text, done: false })
  draft.value = ""
}

function remove(todo: Todo) {
  todos.value.splice(todos.value.indexOf(todo), 1)
}
</script>

<template>
  <main class="mx-auto flex max-w-md flex-col gap-3 p-6">
    <h1 class="title">Todos</h1>
    <input
      v-model="draft"
      placeholder="What needs doing?"
      class="rounded-md border border-gray-300 px-3 py-2"
      @keyup.enter="add"
    />
    <ul class="flex flex-col gap-2">
      <li v-for="todo in todos" :key="todo.id" class="flex items-center gap-2">
        <input v-model="todo.done" type="checkbox" />
        <span class="flex-1" :class="{ 'text-gray-400 line-through': todo.done }">{{ todo.text }}</span>
        <button class="text-sm text-red-600" @click="remove(todo)">Remove</button>
      </li>
    </ul>
    <p v-if="todos.length" class="text-sm text-gray-500">{{ left }} left</p>
    <p v-else class="text-sm text-gray-500">Nothing to do.</p>
  </main>
</template>

<style scoped>
.title {
  font-size: 24px;
  font-weight: 700;
  color: #4f46e5;
}
</style>
