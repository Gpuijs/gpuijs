# Wire protocol

This page documents the messages between the app process and the host. You only need it to work on gpuijs itself, or to debug the bridge. The channels are described in [Architecture](architecture.html#three-channels).

The authoritative definitions are `src/protocol.rs` (app → host), `js/src/wire.js` (the binary commit encoder) and the `dispatch` function in `js/src/renderer.js` (host → app).

## Framing (app → host)

Everything the app sends goes through the `GPUIJS_IN` pipe as frames:

```text
┌─────────┬──────────────────┬───────────────────────┐
│ kind u8 │ length u32 (LE)  │ body (length bytes)   │
└─────────┴──────────────────┴───────────────────────┘
kind 0: body is a JSON message
kind 1: body is a binary commit
```

The host stops reading if a frame claims more than 1 GiB, since the stream must be out of step by then.

## JSON messages (app → host)

Each is an object tagged by `t`.

| `t` | Fields | Meaning |
| --- | --- | --- |
| `window` | `options` | `new Window(options)`. Always the first message. The window opens with these. |
| `css` | `text` | Replaces the app's stylesheet (the compiled `app.css`) |
| `styles` | `sheets: string[]` | Replaces the runtime `<style>` sheets, in document order |
| `sync` | | The app is blocked until the host writes the next layout's rects to `GPUIJS_SYNC` |
| `settled` | | JavaScript has finished handling the input events it was sent |
| `bench` | `req`, `rect?` | Answers a benchmark query or settle request |
| `menu` | `items` | Sets the application menu |
| `dockmenu` | `items` | Sets the Dock menu |
| `popup` | `req`, `items` | Shows a context menu. The host answers `menu-closed` with `req`. |
| `win` | `op`, `arg` | A `Window` method: `setTitle`, `minimize`, `maximize`, `unmaximize`, `setFullScreen`, `close`, `focus`, `setSize` |
| `app` | `op` | An `app` method: `quit`, `hide`, `show`, `focus` |
| `dialog` | `req`, `kind`, `options` | `open`, `save` or `message`. The host answers `dialog` with `req`. |

A menu item is `{ id, label, type, role, accelerator, checked, enabled, submenu }`, with `id` being the item's `commandId`.

## Binary commits (app → host)

One React commit is one binary frame, so it applies atomically. A 10,000-row mount is megabytes of ops, and JSON took longer to write and parse than the host took to apply them.

Everything is little-endian. Ids are `u32`. A string is a `u32` byte length followed by UTF-8.

```text
commit  := count:u32  op*
op      := code:u8  fields
```

| Code | Op | Fields | Meaning |
| --- | --- | --- | --- |
| 1 | `el` | `id:u32` `tag:str` `props` | Create an element |
| 2 | `txt` | `id:u32` `text:str` | Create a text node |
| 3 | `set` | `id:u32` `props` | Replace an element's props |
| 4 | `settext` | `id:u32` `text:str` | Replace a text node's text |
| 5 | `append` | `parent:u32` `child:u32` | Append a child |
| 6 | `insert` | `parent:u32` `child:u32` `before:u32` | Insert a child before a sibling |
| 7 | `remove` | `parent:u32` `child:u32` | Remove and free a subtree |
| 8 | `detach` | `parent:u32` `child:u32` | Unlink without freeing (a library may attach the node again) |
| 9 | `focus` | `has:u8` `id:u32?` | JavaScript moved focus to a node, or cleared it |
| 10 | `hover` | `has:u8` `id:u32?` | The SVG shape under the pointer, found by JavaScript hit-testing, or none |

### Props

Props start with a `u16` mask of the fields present, and then those fields in bit order:

| Bit | Field | Encoding | Meaning |
| --- | --- | --- | --- |
| 0 | `class` | str | Class list |
| 1 | `id` | str | Element id |
| 2 | `style` | str | Inline style as CSS declaration text |
| 3 | `events` | `u8` count, then strs | Events the host should report: `click`, `mousedown`, `mouseup`, `mousemove`, `mouseenter`, `mouseleave`, `keydown`, `contextmenu`, `change` |
| 4 | `attrs` | `u16` count, then key/value str pairs | Attributes for selectors and the host (`role`, `type`, `href`, `data-*`, `aria-*`, SVG attributes, ...) |
| 5 | `value` | str | A field's value |
| 6 | `placeholder` | str | A field's placeholder |
| 7 | `disabled` | none (the bit alone) | Disabled |
| 8 | `src` | str | An image's resolved local path |
| 9 | `seq` | u32 | An input's last change sequence the app has seen |
| 10 | `measure` | none (the bit alone) | Report this node's rect after each layout |

## Host → app messages

The host writes newline-delimited JSON to `GPUIJS_SOCKET`.

### `metrics`

```json
{"t": "metrics", "regular": [0.27, 0.31, ...], "bold": [0.28, ...]}
```

Sent first. Advance widths of printable ASCII (space through `~`) in the system UI font, in ems, regular and bold. Rendering waits for this, and [`measureText`](api.html#measuretext) uses it.

### `event`

```json
{"t": "event", "id": 42, "type": "click", "clientX": 120, "clientY": 88, "target": 57}
```

All other messages are events with a node `id` (0 is `<html>`) and a `type`:

| `type` | Extra fields | Meaning |
| --- | --- | --- |
| `click`, `mousedown`, `mouseup`, `mousemove`, `mouseenter`, `mouseleave`, `contextmenu` | `clientX`, `clientY`, `target?` | Pointer events on a node that listens. `target` is the innermost box under the pointer. |
| `document` | `events`, `clientX`, `clientY`, `target?` | Pointer events no node reported, for `document` listeners |
| `keydown` | `key`, `metaKey`, `ctrlKey`, `shiftKey`, `altKey` | A key press. On `<html>` for the window, or on a text field for its own keys. |
| `change` | `value`, `seq?` | A text field edit (with its sequence number) or a `<select>` pick |
| `focus`, `blur` | | A native field gained or lost focus |
| `resize` | `rect` | A measured node's new rect: `{ x, y, width, height, padding, border }` |
| `visibilitychange` | `hidden` | The window was hidden or shown |
| `appearance` | `dark` | The system appearance changed |
| `window` | `name`, `focused`, `maximized`, `fullScreen`, `bounds` | A window event (`ready-to-show`, `focus`, `blur`, `resize`, `close`) with the window's state |
| `app` | `name` | An app event (`activate`) |
| `menu` | `item` | A menu item was chosen (its `commandId`) |
| `menu-closed` | `req` | A context menu closed |
| `dialog` | `req`, and the result | A dialog closed |
| `benchQuery`, `benchSettle` | `req`, `selector?`, `index?` | Benchmark driver requests |

### Sync answers

When the app sends `sync`, the host applies everything before it, lays out, and writes one line to `GPUIJS_SYNC`:

```json
{"rects": {"42": {"x": 0, "y": 64, "width": 320, "height": 48, "padding": [0, 0, 0, 0], "border": [0, 0, 1, 0]}}}
```

It holds the rect of every measured node that changed. The app applies them as `resize` events before returning from the geometry read that forced the layout.

## Input sequence numbers

Text fields live in the host. Each edit increments the field's `seq` and reports `change` with it. When the app sends props for an input, they carry the last `seq` it has seen, and the host applies a `value` only if it isn't older than the field's last edit. A controlled input therefore never rolls back keystrokes typed while React was busy.
