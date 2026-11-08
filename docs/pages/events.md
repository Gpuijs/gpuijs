# Events

Event handlers work as they do in the browser, with capture and bubbling phases, `stopPropagation()` and `preventDefault()`. React event props behave as in react-dom. Vue (`@click`, `v-model`) and Solid (`onClick`, `on:click`) handlers are DOM listeners on host nodes, covered under [DOM listeners](#dom-listeners). The host only reports the events your app listens for, so a node without handlers costs nothing per pointer move.

## React event props

| Prop | Fires when |
| --- | --- |
| `onClick` | The primary button is pressed and released on the element |
| `onMouseDown`, `onPointerDown` | The primary button is pressed |
| `onMouseUp`, `onPointerUp` | The primary button is released |
| `onMouseMove`, `onPointerMove` | The pointer moves over the element |
| `onMouseEnter`, `onPointerEnter` | The pointer enters the element (doesn't bubble) |
| `onMouseLeave`, `onPointerLeave` | The pointer leaves the element (doesn't bubble) |
| `onContextMenu` | The right mouse button is pressed |
| `onKeyDown` | A key is pressed while the element, or something inside it, has focus |
| `onChange`, `onInput` | A text field's value changes, a checkbox or radio toggles, or a `<select>` option is picked |
| `onFocus`, `onBlur` | The element or a descendant gains or loses focus (these bubble, as React's do) |
| `onSubmit` | A form is submitted by a submit button or Enter in a text field |

Every bubbling event also has a capture form: `onClickCapture`, `onPointerDownCapture`, `onKeyDownCapture` and so on.

The `Mouse` and `Pointer` names map to the same host events. The pointer is always a mouse (`pointerType: "mouse"`, `isPrimary: true`).

### Not supported yet

`onScroll`, `onWheel`, `onKeyUp`, `onKeyPress`, `onDoubleClick`, `onDrag*`, `onDrop`, `onTouch*`, `onCopy`/`onPaste` and `onAnimationEnd`/`onTransitionEnd` never fire. Scrolling itself is native and works. Its position just isn't reported to JavaScript.

## The event object

React handlers get a lightweight synthetic event:

| Field | Value |
| --- | --- |
| `type` | The event name (`click`, `keydown`, ...) |
| `target`, `currentTarget` | Host nodes |
| `clientX`, `clientY` | Pointer position in window coordinates |
| `pageX`, `pageY` | The same as `clientX`/`clientY` on React events |
| `key` | For `keydown`: the DOM `KeyboardEvent.key` (`"Enter"`, `"Escape"`, `"a"`, ...) |
| `ctrlKey`, `metaKey`, `shiftKey`, `altKey` | Modifier state |
| `button` | Always `0` |
| `pointerType` | Always `"mouse"` |
| `defaultPrevented`, `preventDefault()`, `isDefaultPrevented()` | Cancel the default action (see below) |
| `stopPropagation()`, `isPropagationStopped()` | Stop the event in its current phase |
| `nativeEvent` | The event itself |
| `persist()` | Does nothing, as in React 17+ |

For `onChange`, read the new value from `e.target.value` or `e.target.checked`.

## Propagation order

For a click, gpuijs follows the browser's order:

1. `document` listeners added with `{ capture: true }`.
2. React capture handlers (`onClickCapture`), from the root down to the target.
3. React bubble handlers (`onClick`), from the target up to the root.
4. DOM listeners (Vue and Solid handlers, and `addEventListener`) on the target and its ancestors, then on `document`, then on `window`.
5. The default action, unless something called `preventDefault()`.

All of this runs before any effect that React schedules for the event. This matters for libraries like Radix, which register "click outside" listeners in an effect after opening.

React events bubble through the **component** tree. A portal's events bubble to the component that rendered the portal.

## DOM listeners

`addEventListener` works on host nodes, `document` and `window`, and it's how Vue and Solid attach their handlers. For each host event, gpuijs dispatches the matching DOM events in browser order:

| Host event | DOM events dispatched |
| --- | --- |
| press | `pointerdown`, `mousedown` |
| release | `pointerup`, `mouseup` |
| click | `click` |
| move | `pointermove`, `mousemove` |
| enter | `pointerover`, `pointerenter`, `mouseover`, `mouseenter` |
| leave | `pointerout`, `pointerleave`, `mouseout`, `mouseleave` |
| key press | `keydown`, then `keyup` right after |
| text edit | `input` (plus `change` for a `<select>`) |
| checkbox or radio toggle | `input`, `change` |
| focus, blur | `focus` and `focusin`, `blur` and `focusout` |

DOM events have `clientX`/`clientY`, `pageX`/`pageY` (relative to `<body>`, so they follow page scrolling), `button`, `buttons`, `pointerType`, `isPrimary` and a `composedPath()` that runs from the target up to `document` and `window`.

Adding a DOM listener for one of the pointer types (or `keydown`) on a host node tells the host to start reporting that event for the node, just as a React prop would. Fields report edits and focus changes without being asked. Form submission (a submit button, or Enter in a text field) reaches React's `onSubmit` only; Vue and Solid apps handle the button's click or the field's Enter key instead.

### Pointer capture

`el.setPointerCapture()`, `releasePointerCapture()` and `hasPointerCapture()` work. While a node holds capture, pointer moves and the release go to it wherever the pointer is, and the release ends the capture. Drag handles, sliders and drawers (vaul) rely on this.

### Events on `document`

Pointer presses, releases, clicks and moves that no element reported still reach `document` and `window`. They're dispatched from the innermost box under the pointer (or `<body>`), so "click outside" detection works.

## Keyboard

Every key press in the window is reported as a `keydown`, whether or not anything is focused:

- If an element JavaScript focused (`el.focus()`) has focus, the event goes to it first and bubbles through its `onKeyDown` handlers, then to `document` and `window`. This is how menus handle Escape and arrow keys.
- Otherwise it goes straight to `document` and `window` listeners. Use this for app-wide shortcuts:

```tsx
useEffect(() => {
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "k" && e.metaKey) setPaletteOpen(true)
  }
  document.addEventListener("keydown", onKey)
  return () => document.removeEventListener("keydown", onKey)
}, [])
```

A focused **text field** keeps most keys to itself (typing, Backspace, Left/Right, Cmd+A/C/V/X, Home/End). It reports `Enter`, `Escape`, `Tab`, `ArrowUp` and `ArrowDown` to its `onKeyDown`, and those bubble. That's enough for cmdk-style list navigation.

Named keys use DOM names: `Escape`, `Enter`, `Tab`, `Backspace`, `Delete`, `" "` (space), `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp` and `PageDown`. Other keys report the character typed.

> [!TIP]
> For shortcuts that should show in the menu bar, use a menu item's `accelerator` instead. See [Menus](menus.html#accelerators).

## Focus

Text fields take focus when clicked. Any host node can be focused from JavaScript with `el.focus()`, and `document.activeElement` (or [`activeElement()`](api.html#activeelement)) reports it. When focus changes, `onBlur` fires on the old element and `onFocus` on the new one.

The host matches `:focus`, `:focus-within` and `:focus-visible`. `:focus-visible` matches for text fields, and for other elements after keyboard use, as browsers decide.

> [!NOTE]
> Tab doesn't move focus between elements yet, and clicking a button doesn't focus it.

## Default actions

After the handlers run, unless one called `preventDefault()`:

- **Checkbox or radio clicked:** it toggles and fires `onChange`. A radio unchecks the uncontrolled radios in its `name` group.
- **Label clicked:** it clicks its control (`htmlFor`, or the checkbox, radio or input inside it), or focuses a text field.
- **Submit button clicked**, or **Enter in a text field:** the button's form (its `form` attribute, or its nearest `<form>`) gets `onSubmit`. Buttons default to `type="submit"`, as in HTML. Disabled buttons don't submit.

See [Forms and inputs](forms.html).

## Hover and active styles

`:hover` and `:active` are handled by the host without any JavaScript. Hover styles apply natively as the pointer moves, so they're instant even while JavaScript is busy. `group-hover:` and `.group:hover .x` work. `pointer-events: none` lets presses and hover pass through to what's underneath.

## Cursors

The `cursor` property sets the mouse cursor: `pointer`, `text`, `grab`, `grabbing`, `not-allowed`, `crosshair`, the resize cursors and more. See [CSS support](css.html#interaction).
