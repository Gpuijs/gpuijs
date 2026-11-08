# Elements

You use ordinary HTML tags, in JSX or Vue templates. Most become CSS boxes, styled and laid out by the host. A few have native behaviour. This page covers how each kind of element renders.

## Boxes

`div`, `section`, `main`, `header`, `nav`, `ul`, `li`, `p`, `h1`–`h6` and the other structural tags are boxes. The built-in UA stylesheet gives them browser defaults:

- Structural and text-block tags are `display: block`. Any other tag is `inline`, as CSS's initial value says.
- `input`, `button`, `select`, `textarea`, `img` and `svg` are `inline-block`.
- `html` sets `font-family: system-ui, sans-serif`, `font-size: 16px`, `line-height: 1.5` and `color: #000`. `body` has an 8px margin.
- `p`, `ul`, `ol`, `blockquote`, `figure` and `pre` get `1em` vertical margins. Headings get their usual sizes and bold weight.
- `a` is blue, underlined and has `cursor: pointer`. `strong`, `b` and `th` are bold, and `em` and `i` are italic. `code` and `pre` use a monospace font.
- `[hidden]`, `head`, `title`, `meta`, `link`, `base`, `template`, `noscript`, `style` and `script` are `display: none`.

The UA stylesheet sits in the lowest cascade layer, so any rule of yours (Tailwind's preflight included) overrides it.

## Text

Text children are drawn by GPUI's text system, with system fonts.

**Paragraphs.** A run of text and plain inline elements (`span`, `strong`, `em`, `a`, `code` and so on) becomes one wrapped paragraph. Inline elements inside it can change `color`, `font-weight`, `font-style` and `text-decoration` (underline, line-through), and those changes become styled runs within the paragraph.

> [!WARNING]
> Changing `font-size` or `font-family` on inline text **inside** a paragraph isn't supported yet. The run keeps the paragraph's font, and gpuijs logs a warning once. Put differently sized text in its own block, or use flex: `<p className="flex items-baseline gap-1">`.

**Inline boxes.** An inline element that's more than styled text (it has a click handler, a background or `:hover` styles, or contains an input or image) makes its line a wrapping row of boxes. Inline boxes don't break across lines.

**White space** collapses as in CSS. `white-space: pre`, `pre-wrap`, `pre-line`, `nowrap` and `break-spaces` are honoured, along with `text-overflow: ellipsis`, `-webkit-line-clamp`, `overflow-wrap`, `word-break`, `letter-spacing`, `text-transform` and `text-align`.

**Fonts** come from the system. `font-family` lists are matched against installed font names. `-apple-system`, `BlinkMacSystemFont` and `system-ui` map to the system UI font (San Francisco). `@font-face` isn't supported yet: the rule is skipped and a warning is logged.

## Images

```tsx
<img src="/avatar.png" alt="" className="size-10 rounded-full object-cover" />
```

`<img>` draws PNG, JPEG, WebP, GIF, BMP, TIFF and SVG files. `src` can be a path under `public/`, an absolute file path or an `http(s)` URL (downloaded once and cached). `object-fit` and `object-position` are supported, and rounded corners clip the image. See [Images, SVG and assets](assets.html).

## SVG

Inline `<svg>` subtrees are serialized back to SVG markup and rasterized at the window's scale, so they stay sharp. Icons (lucide, tabler, heroicons, MUI icons) and charts (Recharts, MUI X Charts, ApexCharts) render this way. CSS reaches into SVG: `fill`, `stroke`, `currentColor`, `stroke-width`, `stroke-dasharray`, `opacity` and the other presentation properties, `var()` in attributes, and `:hover` on individual shapes.

Pointer events inside an SVG are hit-tested by geometry in JavaScript. `onPointerEnter` on a `<path>` works, and so do `mouseover` listeners on chart marks. See [SVG](assets.html#inline-svg).

## Canvas

`<canvas>` has a working 2D context: `getContext("2d")` draws with Skia ([@napi-rs/canvas](https://github.com/Brooooooklyn/canvas)) in the JS process, and the window shows the result after each task that drew. Text, gradients, `getImageData`, `toDataURL` and `toBlob` work, so Chart.js draws. `width` and `height` set its pixel size and, unless CSS sizes it, its size on screen. A canvas created only to `measureText` costs nothing to show.

`getContext("webgl2")` and `getContext("webgl")` return WebGL 2 and WebGL 1 contexts, run on macOS's OpenGL 4.1, so Three.js and PixiJS draw. The usual WebGL 1 extensions (vertex array objects, instancing, float textures, depth textures, draw buffers, anisotropic filtering) are there, as is `EXT_color_buffer_float` on WebGL 2. Textures take typed arrays, `ImageData`, other canvases, and `<img>` and `new Image()` elements, which load from `public/` or a URL and fire `load` as in a browser. Only images created for drawing (not rendered in the page) have their pixels decoded: an `<img>` your components render reads just its file's header for `naturalWidth`, so it can't be used as a texture.

Each frame the canvas drew is read back from the GPU and sent to the window, so a large canvas redrawn every frame costs more than in a browser. Shaders are translated from GLSL ES to desktop GLSL; compile errors name lines of the translated source.

`drawImage` on a 2D context takes other canvases and @napi-rs/canvas images, but not `<img>` elements.

## Video and audio

`<video>` and `<audio>` play through AVFoundation. Set `src` on the element or on its first `<source>`. Local files, `public/` URLs and `http(s)` URLs all work; remote ones stream. AVFoundation plays MP4/MOV (H.264, HEVC), MP3, AAC, WAV and AIFF, but not WebM or Ogg. `canPlayType` answers accordingly.

`play()`, `pause()`, `currentTime`, `duration`, `paused`, `ended`, `volume`, `muted`, `loop`, `autoplay` and `playbackRate` work, and so do the media events (`onPlay`, `onPause`, `onTimeUpdate`, `onLoadedMetadata`, `onEnded`...). `new Audio(url)` plays without being in the page.

A video is letterboxed in its box (`object-fit: contain`) and is 300×150 until its own size is known. `controls` shows a minimal bar with play/pause, the time and progress. It has no seeking, volume or fullscreen. An `<audio>` without `controls` isn't displayed.

## Form controls

| Element | How it renders |
| --- | --- |
| `<input>` (text-like types) | A native single-line text field. Typing is handled natively and reported with `onChange`. |
| `<input type="checkbox">`, `<input type="radio">` | A styleable box. A click toggles it and fires `onChange`, as in a browser. `:checked` matches. |
| `<textarea>` | The same native field as `<input>`, so it's single-line for now. |
| `<select>` + `<option>` / `<optgroup>` | Shows the chosen option. A click opens an option list over the window, and picking one fires `onChange`. |
| `<button>` | A box. Submit buttons submit their form. |
| `<label>` | Clicking it toggles its checkbox or radio, or focuses its text field. |
| `<form>` | `onSubmit` fires for a submit button click, or Enter in a text field. |

[Forms and inputs](forms.html) has the details.

## Details

`<details>` shows only its `<summary>` until it's open. Clicking the summary opens or closes it. `details.open` reads and sets it, and `toggle` (React's `onToggle`) fires once in a later task, with `oldState` and `newState`. Details sharing a `name` form an accordion: opening one closes the others.

## Dialog

`<dialog>` is hidden until it's open. `show()` opens it in place, centered across its container. `showModal()` opens it in the top layer: centered in the window, above every z-index, over a `::backdrop` that keeps the pointer from the page. Style the backdrop with `dialog::backdrop` or Tailwind's `backdrop:` variant.

Opening moves focus to its `autofocus` element, else its first focusable one, and closing gives focus back. Escape fires `cancel` (React's `onCancel`); unless that's cancelled, the dialog closes. `close(value)` sets `returnValue` and fires `close` (`onClose`) in a later task, and `requestClose()` goes through `cancel` first. A submit button in a `<form method="dialog">` closes the dialog with the button's `value`. A click on the backdrop is a click on the dialog itself, so `event.target === dialog` detects a click outside its box. `:modal` and `:open` match.

Modal dialogs stack in document order, not in the order they were opened. Tab doesn't move focus anywhere in gpuijs, so there is no focus trap to need.

## Editable content

An element with `contentEditable` (or `contenteditable="true"`/`"plaintext-only"`) is edited as in a browser, so rich text editors work: Lexical, TipTap/ProseMirror, and a plain editable `div`. A press places the caret and focuses the element. A drag or Shift+press selects; a double press selects a word and a triple press the block.

Typing, Backspace, Delete (with Option for a word, ⌘ for the line), Enter (a new block; Shift+Enter a `<br>`), the arrow keys, Home/End, ⌘A, and copy, cut and paste each fire a `beforeinput` with the browser's `inputType`. If no listener cancels it, the edit is made, then `input` fires (React's `onInput`). ⌘Z, ⌘B, ⌘I and ⌘U only fire `beforeinput` (`historyUndo`, `formatBold`...), as editors handle those themselves. Input methods (Japanese, Chinese, accents) fire composition events.

The DOM's `Selection`, `Range`, `selectionchange` and `MutationObserver` are real, and work for nodes a library made as well as React's. The window draws the caret and the selected text.

- The clipboard is the system's for plain text. HTML copied in the app is pasted back as HTML, but text pasted from elsewhere arrives as `text/plain` only, and the default paste inserts plain text.
- `range.getBoundingClientRect()` returns the box of the element containing the range's start, not the text's own lines. A toolbar that floats over the selected words sits over its block instead.
- The caret doesn't blink. Text in an inline element with padding, a border or a background (an inline `<code>` chip) can't take the caret.

## Tables

`<table>`, `<thead>`, `<tbody>`, `<tfoot>`, `<tr>`, `<th>`, `<td>` and `<caption>` lay out like a browser table. The table becomes a grid with one track per column, and columns size to their content. `colspan` works. `rowspan` isn't supported, and row-level `:hover` styles don't apply.

## Generated content

`::before` and `::after` with a string `content` generate a box, styled from the element's rules. Counters, `attr()` and image content generate nothing. Other pseudo-elements (`::placeholder`, `::selection`, `::marker`, `::-webkit-scrollbar`) have no box to style and are skipped.

## Links

`<a>` is styled like a link but doesn't navigate anywhere. There's no address bar. Routers that handle clicks themselves, like react-router's `<Link>`, work, because they call `history.pushState` (see [Browser environment: history](environment.html#history-and-location)). An `<a href>` with no click handler does nothing when clicked.

## `<style>`

A `<style>` element the app renders (shadcn's `ChartStyle` renders one) styles the window while it's mounted, after the app's CSS. `<style>` elements libraries append to `document.head` work the same way. See [Styling: CSS-in-JS](styling.html#css-in-js-and-runtime-styles).

## Head elements

Components can render `<title>`, `<meta>` and `<link>` anywhere (React 19 encourages it). gpuijs keeps them in the tree but doesn't display them, and they have no effect. Set the window's title with [`win.setTitle()`](window.html#methods).

## Not supported

These elements have no native behaviour. They render as empty boxes, or as plain containers for their children:

- `<iframe>`
- Native drag and drop (`draggable`, `onDrag*`). Pointer-event-based libraries like dnd-kit work.
