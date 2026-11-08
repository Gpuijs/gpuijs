# Layout and measuring

The host lays out the tree with [Taffy](https://github.com/DioxusLabs/taffy), through GPUI. Taffy implements CSS flexbox, grid and block layout. gpuijs adds inline text flow, tables and positioning on top, and carries a few fixes to Taffy (see [Contributing](contributing.html#vendored-dependencies)).

## Layout modes

| `display` | Support |
| --- | --- |
| `block`, `flow-root` | Block layout with margins, including auto margins |
| `inline`, `inline-block` | Inline content shares line boxes. Text and plain inline elements form wrapped paragraphs. See [Elements: text](elements.html#text). |
| `flex`, `inline-flex` | Full flexbox: direction, wrap, grow, shrink, basis, alignment, `gap`, `order` |
| `grid`, `inline-grid` | Track lists with `fr`, `minmax()`, `repeat()`, `auto-fill`/`auto-fit`, named lines and areas, `grid-auto-*`, `grid-auto-flow` (`dense` too), `justify-items`/`justify-self` |
| `none` | Removed from layout |
| `contents` | The box disappears and its children take its place |
| `table` and its parts | Laid out from the tags, as a grid with one track per column. See [Elements: tables](elements.html#tables). |
| `-webkit-box` | The line-clamp idiom: a block of clamped text |

Other values (`ruby`, `list-item` markers) fall back to `block`, with a warning.

## Positioning

| `position` | Behaviour |
| --- | --- |
| `static` | Normal flow. Insets are ignored. |
| `relative` | Normal flow, offset by insets |
| `absolute` | Out of flow, placed against the containing block |
| `fixed` | Placed against the window, in a layer above the scrolling page. It doesn't scroll with the page. |
| `sticky` | Treated as `relative`. It doesn't stick yet. |

`z-index` orders positioned boxes as CSS does. A box with `z-index` inside a fixed layer stacks within that layer.

## Sizes and units

Lengths: `px`, `rem`, `em`, `%`, `ch` (the width of the font's "0"), `vw`, `vh`, `vmin`, `vmax`, and the small, large and dynamic viewport units (`svh`, `lvh`, `dvh`, ...), which all equal the window size. `calc()`, `min()`, `max()` and `clamp()` work, including mixes of percentages and lengths.

Sizing keywords: `auto` and `stretch`. A `min-content`, `max-content` or `fit-content` width sizes the box to its content and stops it stretching, which approximates all three. `aspect-ratio` works.

Boxes are always sized as `box-sizing: border-box` (what Tailwind's preflight sets anyway). `content-box` isn't supported.

`rem` is always 16px, the initial `font-size`.

## Scrolling

`overflow: auto` and `overflow: scroll` make a box scroll natively, with the trackpad and momentum, and its content clipped. `overflow: hidden` and `clip` clip without scrolling. Clipping one axis clips both.

`<html>` is the viewport and scrolls the page, so a page taller than the window scrolls. `position: fixed` boxes stay put.

> [!NOTE]
> Scroll offsets aren't reported to JavaScript yet. `scrollTop` reads 0, `onScroll` never fires, and `scrollTo()`/`scrollIntoView()` do nothing. Virtualized lists that compute their window from `scrollTop` (react-window, TanStack Virtual) only render their first page. Long lists don't need virtualizing, though (see the next section).

## Long lists

The host culls long vertical lists by itself. In a list with 32 or more children, children that are off screen, already measured and unchanged since the last frame aren't built. A run of them becomes one spacer with their measured height, so the layout around them doesn't change. New or changed children off screen are left out too, at their sibling's height or their old size, until they come near the view, much like `content-visibility: auto`. If a spacer turns out to be visible, the frame is redrawn with the real children before it's shown.

So you can render 10,000 rows directly and scrolling stays smooth.

## Reading sizes from JavaScript

Libraries measure elements constantly: popovers anchor to triggers, charts size to their containers, and collapsibles animate to their content's height. gpuijs supports this the way a browser does.

**Measured nodes.** The first time JavaScript reads a node's geometry (`getBoundingClientRect()`, `offsetWidth`, `clientHeight`, ...), the node and its ancestors become *measured*. From then on, the host reports their rects after each layout that changes them.

**Forced layout.** If a node hasn't been laid out yet, or style writes are pending, a geometry read blocks JavaScript until the host has applied everything sent so far, laid out the window and answered with fresh rects. That's a browser's forced synchronous layout. It's what lets Radix Select place a list it just mounted, and MUI's Collapse read a height before animating it.

```tsx
const ref = useRef<HTMLDivElement>(null)
useLayoutEffect(() => {
  const { width, height } = ref.current!.getBoundingClientRect() // laid out on the spot if needed
}, [])
```

Forced layouts are fast, but they're a round trip to the host. Avoid reading geometry in a loop that also writes styles.

### What geometry reads return

| API | Returns |
| --- | --- |
| `getBoundingClientRect()` | The border box in window coordinates. Inside an `<svg>`, computed from attributes and the viewBox. |
| `offsetWidth`, `offsetHeight`, `clientWidth`, `clientHeight`, `scrollWidth` | The border box size |
| `scrollHeight` | The larger of the box's height and its children's extent |
| `offsetTop`, `offsetLeft` | Position relative to the parent |
| `offsetParent` | The parent, or `null` for a fixed box |
| `clientTop`, `clientLeft` | The top and left border widths |
| `getComputedStyle(el)` | See [Browser environment](environment.html#getcomputedstyle) |

## `ResizeObserver`

```tsx
useEffect(() => {
  const ro = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width))
  ro.observe(ref.current!)
  return () => ro.disconnect()
}, [])
```

`ResizeObserver` is a global, or import it from `@gpuijs/core`. The host reports an observed node's size after each layout that changes it, and an observation starts with the current size, as in the DOM. Entries have `target`, `contentRect`, `borderBoxSize` and `contentBoxSize`.

> [!NOTE]
> `contentRect` is the border box, not the content box: padding and borders are included. This is fine for unpadded chart and scroll containers, which is the usual use.

## The viewport

`window.innerWidth` and `innerHeight` are the window's content size, and `window` hears `resize` whenever it changes, including once just after the first layout. Media queries use the same size: `@media (min-width: 768px)` and Tailwind's `md:` respond as the window resizes.

Set `GPUIJS_WINDOW_SIZE=1440x900` to open the window at a fixed size for screenshots and benchmarks.

## Container queries

`container-type: inline-size` (or `size`) and `container-name` make a box a container, and `@container` rules match against its size. Named and unnamed containers work, and so do Tailwind's `@container` and `@md:` variants. Before the first layout, container queries use the window size.

## Measuring text

[`measureText(text, style)`](api.html#measuretext) returns the width and height of one line of text, synchronously, for code that sizes text before rendering it (chart axis labels, for example):

```ts
import { measureText } from "@gpuijs/core"

measureText("Revenue", { fontSize: 12, fontWeight: 600 }) // { width: 47.4, height: 14 }
```

It uses the system UI font's advance widths, sent by the host at startup, in regular or bold. Characters outside ASCII use an average width. It ignores `fontFamily`.
