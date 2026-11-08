# Custom title bars

As in Electron, a custom title bar is made with a hidden title bar and CSS: `app-region: drag` marks what drags the window, and `app-region: no-drag` marks what doesn't.

```ts
import { Window } from "gpuijs"

const win = new Window({
  width: 1100,
  height: 760,
  titleBarStyle: "hidden",
  trafficLightPosition: { x: 16, y: 18 },
})
```

Then, in whichever framework renders it, a bar like this:

```html
<header class="flex h-12 items-center gap-2 border-b pl-20 pr-3 [app-region:drag]">
  <span class="text-sm font-medium">Notes</span>
  <div class="ml-auto flex gap-1 [app-region:no-drag]">
    <button class="rounded px-2 py-1 hover:bg-zinc-100">Share</button>
    <button class="rounded px-2 py-1 hover:bg-zinc-100">⋯</button>
  </div>
</header>
```

## Step by step

1. **Hide the title bar.** Pass `titleBarStyle: "hidden"` (or `"hiddenInset"`) to `new Window()`. The content then fills the whole window, under the traffic lights.
2. **Place the traffic lights.** Use `trafficLightPosition: { x, y }` to line them up with your bar. `hiddenInset` moves them to (20, 20).
3. **Leave room for them.** Pad the start of your bar (about 72–80px with the default position) so content doesn't sit under the buttons.
4. **Mark the drag region.** Put `app-region: drag` on the bar.
5. **Exclude controls.** Put `app-region: no-drag` on buttons, inputs and anything else clickable inside the bar.

## How it behaves

- The **innermost** box with an `app-region` under a press decides. A `no-drag` button inside a `drag` bar gets the click.
- Dragging a `drag` region moves the window.
- Double-clicking a `drag` region does what the system title bar does (zoom or minimize, per the user's macOS setting).

## Spelling it

All three spellings work:

```css
.titlebar { app-region: drag; }
.titlebar { -webkit-app-region: drag; }
```

```tsx
<header className="[app-region:drag]">
```

The [`shadcn-demo`](quick-start.html#the-examples) example draws its own title bar this way.
