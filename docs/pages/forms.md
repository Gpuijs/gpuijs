# Forms and inputs

Text fields are native GPUI inputs that live in the host. Checkboxes, radios and buttons are styleable boxes whose behaviour gpuijs carries out in JavaScript, as a browser does.

## Text fields

```tsx
function Search() {
  const [query, setQuery] = useState("")
  return (
    <input
      className="h-9 w-64 rounded-md border px-3 focus-visible:ring-2"
      placeholder="Search…"
      value={query}
      onChange={(e) => setQuery(e.target.value)}
    />
  )
}
```

In Vue, `v-model` works on text fields, checkboxes, radios and `<select>`. In Solid, pair `value` with `onInput`.

Every `<input>` that isn't a checkbox or radio, and every `<textarea>`, becomes a native single-line text field.

### Typing never waits on JavaScript

The host owns the field's text. A keystroke updates the field and draws it at once, then reports `change` to JavaScript with the new value and an increasing sequence number. If JavaScript is busy rendering, typing still keeps up.

### Controlled and uncontrolled

Both work:

- **Controlled** (`value` + `onChange` in React, `v-model` in Vue, `value` + `onInput` in Solid): the host applies a `value` from the app only if it differs from what the field shows and isn't older than the last edit the field reported. A slow re-render can't roll back characters typed in the meantime or move the cursor.
- **Uncontrolled** (`defaultValue`, or neither): the field keeps what's typed. `ref.current.value` reads it.

Setting `el.value = "..."` from JavaScript (flatpickr does this to show the picked date) updates the field.

### What the field supports

| Feature | Support |
| --- | --- |
| Typing, IME composition, dead keys | Yes, through macOS text input |
| Emoji and symbols palette | Ctrl+Cmd+Space |
| Backspace, Delete | Yes |
| Left, Right, Home, End | Yes |
| Shift+Left, Shift+Right selection | Yes |
| Select all, copy, cut, paste | Cmd+A, Cmd+C, Cmd+X, Cmd+V. Pasted newlines become spaces. |
| Mouse | Click to place the cursor, Shift+click or drag to select |
| `placeholder` | Drawn in the text color at half opacity |
| `disabled` | Yes, and `:disabled` matches |
| `:placeholder-shown`, `:focus`, `:focus-visible` | Yes |

### Limitations

- **Single line only.** `<textarea>` is the same single-line field for now.
- **No masking.** `type="password"` isn't masked yet, and no input type gets special behaviour (`number`, `date`, `range`, `file` and `color` are plain text fields).
- **`::placeholder` rules don't apply.** The placeholder color comes from the field's `color`.
- `selectionStart`, `setSelectionRange()` and `select()` aren't available.

## Checkboxes and radios

```tsx
<label className="flex items-center gap-2">
  <input type="checkbox" checked={done} onChange={(e) => setDone(e.target.checked)} />
  Done
</label>
```

Checkboxes and radios are boxes you style with CSS. A click toggles them, as in a browser:

1. `onClick` runs.
2. Unless `preventDefault()` was called, the input toggles and `onChange` fires. Read the new state from `e.target.checked`.
3. A radio with a `name` unchecks the other **uncontrolled** radios in its group. Controlled radios update when React re-renders.

A controlled input (`checked={...}`) shows what React renders next, as react-dom restores it.

`:checked` matches checked inputs, so sibling selectors like `peer-checked:` in Tailwind work. That's also how "hidden native input plus styled dot" radio patterns work (an `sr-only` input inside a `<label>`).

## Labels

Clicking a `<label>`:

- With `htmlFor`, it acts on the element with that `id`.
- Otherwise it acts on the first `input`, `select`, `textarea` or `button` inside it.

A checkbox or radio gets clicked (and toggles). A text field gets focus. A button or link inside a label handles its own click.

## Select

```tsx
<select value={size} onChange={(e) => setSize(e.target.value)} className="rounded border px-2 py-1">
  <option value="s">Small</option>
  <option value="m">Medium</option>
  <optgroup label="Big">
    <option value="l">Large</option>
    <option value="xl">Extra large</option>
  </optgroup>
</select>
```

A native `<select>` shows the label of the chosen option. A click opens the option list as an overlay anchored below the select, kept inside the window and drawn above everything else. Picking an option fires `onChange`, with the option's `value` (or its label if it has none) in `e.target.value`. Clicking anywhere else closes the list.

Custom select components (Radix Select, MUI Select, Base UI Select, shadcn's Select) render their own lists and work too.

## Buttons and submitting

```tsx
<form onSubmit={(e) => { e.preventDefault(); save() }}>
  <input name="title" />
  <button>Save</button>            {/* type="submit" by default */}
  <button type="button" onClick={cancel}>Cancel</button>
</form>
```

A form is submitted when:

- a submit button is clicked (a `<button>` without `type`, or with `type="submit"`), unless the button is disabled; or
- Enter is pressed in a text field inside it.

The form is the button's `form` attribute (a form `id`), or its nearest `<form>` ancestor. Submitting runs React `onSubmit` handlers and nothing else. Vue's `@submit` and Solid's `onSubmit` don't fire yet: handle the button's click or the field's Enter key. There's no page to navigate, so calling `preventDefault()` is harmless but not needed.

`FormData` built from a form element doesn't work, because host nodes aren't DOM forms. Keep field values in state, or use a form library that does (react-hook-form with controlled fields, TanStack Form).

## Form libraries

Libraries that keep field values in framework state work: TanStack Form (the `next-shadcn-dashboard` example uses it), and react-hook-form through `Controller` or controlled inputs. Anything that reads values out of a DOM form (`new FormData(form)`, `form.elements`) doesn't.

zod validation works. The browser environment defines `FileList` and `HTMLFormElement` classes so zod's `instanceof` checks don't throw.
