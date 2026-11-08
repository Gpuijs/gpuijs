// The docs' interactions: theme, mobile navigation, search, code highlighting and copy buttons,
// and the "On this page" highlight.
const root = document.documentElement

document.querySelector(".theme-button").addEventListener("click", () => {
  const dark = root.dataset.theme ? root.dataset.theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches
  root.dataset.theme = dark ? "light" : "dark"
  try {
    localStorage.setItem("theme", root.dataset.theme)
  } catch {}
})

const menu = document.querySelector(".menu-button")
menu.addEventListener("click", () => {
  const open = document.body.classList.toggle("nav-open")
  menu.setAttribute("aria-expanded", String(open))
})

// Search over page titles and headings (search.js, written by the build).
const input = document.querySelector(".search input")
const results = document.querySelector(".results")
let active = 0
function show() {
  const words = input.value.toLowerCase().split(/\s+/).filter(Boolean)
  const hits = words.length
    ? window.SEARCH.filter((e) => words.every((w) => `${e.t} ${e.s}`.toLowerCase().includes(w)))
        .sort((a, b) => Number(!a.t.toLowerCase().includes(words[0])) - Number(!b.t.toLowerCase().includes(words[0])))
        .slice(0, 12)
    : []
  active = 0
  results.hidden = !words.length
  results.replaceChildren(
    ...(hits.length
      ? hits.map((e, i) => {
          const li = document.createElement("li")
          li.setAttribute("role", "option")
          li.setAttribute("aria-selected", String(i === active))
          const a = document.createElement("a")
          a.href = e.u
          a.innerHTML = `<strong></strong><span></span>`
          a.firstChild.textContent = e.t
          a.lastChild.textContent = e.s
          li.append(a)
          return li
        })
      : [Object.assign(document.createElement("li"), { className: "empty", textContent: "No results" })]),
  )
}
input.addEventListener("input", show)
input.addEventListener("keydown", (e) => {
  const items = [...results.querySelectorAll("[role=option]")]
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault()
    active = (active + (e.key === "ArrowDown" ? 1 : -1) + items.length) % Math.max(items.length, 1)
    items.forEach((li, i) => li.setAttribute("aria-selected", String(i === active)))
  } else if (e.key === "Enter" && items[active]) {
    location.href = items[active].querySelector("a").href
  } else if (e.key === "Escape") {
    input.value = ""
    show()
    input.blur()
  }
})
input.addEventListener("blur", () => setTimeout(() => (results.hidden = true), 150))
input.addEventListener("focus", () => input.value && show())
document.addEventListener("keydown", (e) => {
  const typing = /^(INPUT|TEXTAREA)$/.test(document.activeElement?.tagName)
  if (!typing && (e.key === "/" || (e.key === "k" && (e.metaKey || e.ctrlKey)))) {
    e.preventDefault()
    input.focus()
  }
})

// Code: highlighted once highlight.js loads (offline, blocks stay plain), each with a copy button.
for (const pre of document.querySelectorAll("article pre")) {
  const code = pre.querySelector("code")
  if (window.hljs && code?.className) window.hljs.highlightElement(code)
  const button = document.createElement("button")
  button.className = "copy"
  button.textContent = "Copy"
  button.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(code?.innerText ?? pre.innerText)
      button.textContent = "Copied"
    } catch {
      button.textContent = "Failed"
    }
    setTimeout(() => (button.textContent = "Copy"), 1500)
  })
  pre.append(button)
}

// The section being read, marked in "On this page".
const links = new Map([...document.querySelectorAll(".toc a")].map((a) => [a.hash.slice(1), a]))
if (links.size) {
  const headings = [...links.keys()].map((id) => document.getElementById(id)).filter(Boolean)
  const mark = () => {
    let current = headings[0]
    for (const h of headings) if (h.getBoundingClientRect().top < 120) current = h
    for (const a of links.values()) a.classList.toggle("active", a === links.get(current?.id))
  }
  addEventListener("scroll", mark, { passive: true })
  mark()
}
