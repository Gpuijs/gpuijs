# CSS support

The host parses CSS with [lightningcss](https://lightningcss.dev), matches selectors against the native tree, and maps computed values onto GPUI styles. This page lists what's supported. Anything not listed is skipped, and the host prints `[gpuijs] unsupported css: …` the first time it meets it.

## The cascade

| Feature | Support |
| --- | --- |
| Specificity and source order | Yes |
| `!important` | Yes, including against inline styles |
| Inline styles (`style` prop, `el.style`) | Yes |
| Cascade layers (`@layer`, statements and blocks, nested) | Yes. Unlayered rules beat every layer, and the UA stylesheet sits in the lowest. |
| Inheritance | Yes: `color`, the font properties, `line-height`, `letter-spacing`, `text-align`, `white-space`, `text-transform`, `pointer-events` and custom properties inherit |
| `inherit`, `initial`, `unset`, `revert` | Yes |
| Custom properties and `var()` with fallbacks | Yes, including nested fallbacks. Cycles resolve to the fallback. |
| `@property` | Yes: `inherits` and `initial-value` are honoured |
| CSS nesting (`&`) | Yes, lowered at parse time |
| `@import` | Not at runtime. The CLI bundles imports before the host sees the CSS. |

## At-rules

| Rule | Support |
| --- | --- |
| `@media` | `width`, `height`, `min-*`/`max-*` and range syntax (`(width >= 768px)`), `prefers-color-scheme` (follows macOS, live), `prefers-reduced-motion` (`no-preference` matches), `hover: hover`, `pointer: fine` (and the `any-` forms), `color`. `and`, `or`, `not` and `only` work, and `em`/`rem` resolve against 16px. |
| `@container` | Size conditions against the nearest container (or the nearest one with that name). The window size stands in before the first layout. |
| `@supports` | Evaluated: a declaration is supported when lightningcss can parse it into a typed value |
| `@layer` | Yes |
| `@keyframes` | Yes. The last definition of a name wins. |
| `@property` | Yes |
| `@font-face` | Not yet (skipped with a warning). Fonts come from the system. |
| `@page`, `@counter-style`, `@namespace` | No |

## Selectors

| Selector | Support |
| --- | --- |
| Type, `.class`, `#id`, `*` | Yes |
| Attribute selectors: `[a]`, `[a=v]`, `[a~=v]`, `[a\|=v]`, `[a^=v]`, `[a$=v]`, `[a*=v]`, `i`/`s` flags | Yes. See [which attributes the host sees](#attributes-in-selectors). |
| Combinators: descendant, `>`, `+`, `~` | Yes |
| `:is()`, `:where()`, `:not()` | Yes |
| `:has()` | On the subject: `:has(x)`, `:has(> x)`, `:has(+ x)`, `:has(~ x)` and longer relative selectors. Not nested inside another pseudo-class. |
| `:nth-child()`, `:nth-last-child()`, `:nth-of-type()`, `:nth-last-of-type()`, `:first-child`, `:last-child`, `:only-child`, `:first-of-type`, `:last-of-type`, `:only-of-type` | Yes. `:nth-child(… of S)` isn't supported. |
| `:root`, `:empty` | Yes |
| `:hover`, `:active` | Yes, applied natively without JavaScript. `:hover` on an ancestor works (`.group:hover .x`). |
| `:focus`, `:focus-within`, `:focus-visible` | Yes |
| `:disabled`, `:enabled` | On `input`, `button`, `select` and `textarea` |
| `:checked` | On checkboxes and radios |
| `:placeholder-shown` | Yes |
| `:link`, `:any-link` | `<a>` with an `href` |
| `:dir()` | Always `ltr` for now |
| `:visited`, `:target`, `:invalid`, `:required` and other states | Never match |
| `::before`, `::after` | Yes, with string `content` |
| Other pseudo-elements (`::placeholder`, `::selection`, `::marker`, `::-webkit-scrollbar`, `::backdrop`) | Skipped |

Selectors that the matcher can't evaluate are skipped whole, not misapplied: `:nth-col()`, `:nth-child(… of …)` and `&` outside a nested rule.

### Attributes in selectors

The host sees these attributes: `id`, `class`, `role`, `type`, `name`, `href`, `title`, `for`, `colspan`, `disabled`, `hidden`, `checked`, and every attribute whose name contains a hyphen (`data-*`, `aria-*`, and custom ones like cmdk's `cmdk-item`). Inside SVG, every attribute is passed through.

Others, like `value`, `placeholder`, `alt` and `dir`, aren't available to attribute selectors.

## Values

| Kind | Support |
| --- | --- |
| Lengths | `px`, `rem` (16px), `em`, `%`, `ch`, `vw`, `vh`, `vmin`, `vmax`, `svh`/`lvh`/`dvh` and friends (all the window size), `pt`, `in`, `cm`, `mm` |
| Math | `calc()` (lengths mixed with percentages), `min()`, `max()`, `clamp()`, `abs()` |
| Colors | Named colors, hex, `rgb()`, `hsl()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()`, `color-mix()`, `currentColor`, `transparent`, `light-dark()` (follows the macOS appearance) |
| Angles and times | `deg`, `rad`, `grad`, `turn`, `s`, `ms` |
| Easing | `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`, `cubic-bezier()`, `steps()` |

## Properties

### Box model and sizing

| Property | Notes |
| --- | --- |
| `width`, `height`, `min-*`, `max-*` (and logical `inline-size`, `block-size`, ...) | `auto`, lengths and `stretch`. `min-content`, `max-content` and `fit-content` widths size to content. |
| `margin-*`, `padding-*` (and logical forms) | Including `auto` margins |
| `border-*-width`, `border-*-style`, `border-*-color` | `solid`. `dashed` and `dotted` draw dashed. `double`, `groove` and the rest draw solid. A side with `none` or `hidden` isn't drawn. |
| `border-*-radius` (and logical forms) | Yes |
| `box-sizing` | Always `border-box` |
| `aspect-ratio` | Yes |
| `overflow`, `overflow-x`, `overflow-y` | `visible`, `hidden`, `clip`, `scroll`, `auto`. Clipping one axis clips both. |
| `visibility` | `visible`, and `hidden` (`collapse` acts as `hidden`) |

### Layout

| Property | Notes |
| --- | --- |
| `display` | `block`, `inline`, `inline-block`, `flex`, `inline-flex`, `grid`, `inline-grid`, `flow-root`, `none`, `contents`, table values, `-webkit-box`. See [Layout](layout.html#layout-modes). |
| `position` | `static`, `relative`, `absolute`, `fixed`. `sticky` acts as `relative`. |
| `top`, `right`, `bottom`, `left`, `inset` (and logical forms) | Yes |
| `z-index` | Yes |
| `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`, `flex`, `flex-flow` | Yes |
| `align-items`, `align-self`, `align-content`, `justify-content`, `justify-items`, `justify-self`, `place-*` | Yes |
| `gap`, `row-gap`, `column-gap` | Yes |
| `order` | Yes |
| `grid-template-columns`, `grid-template-rows`, `grid-template-areas`, `grid-template` | Track sizes, `fr`, `minmax()`, `repeat()` with `auto-fill`/`auto-fit`, named lines and areas |
| `grid-auto-columns`, `grid-auto-rows`, `grid-auto-flow` | Including `dense` |
| `grid-column`, `grid-row`, `grid-area` and their longhands | Lines, spans and area names |
| `container-type`, `container-name`, `container` | Yes |

### Text and fonts

| Property | Notes |
| --- | --- |
| `color` | Yes |
| `font-family` | Installed fonts. `system-ui`, `-apple-system` and `BlinkMacSystemFont` map to the system UI font. |
| `font-size`, `font-weight`, `font-style` | Yes |
| `line-height` | Numbers, lengths and percentages |
| `text-align` | `left`, `right`, `center`, `start`, `end`. `justify` aligns left. |
| `text-decoration-line`, `-color`, `-style`, `-thickness` | `underline` and `line-through`, with `wavy` drawn wavy. Dotted, dashed and double lines draw solid. |
| `text-transform` | `uppercase`, `lowercase`, `capitalize` |
| `text-overflow` | `ellipsis` and `clip` |
| `white-space` | `normal`, `nowrap`, `pre`, `pre-wrap`, `pre-line`, `break-spaces` |
| `overflow-wrap`, `word-wrap`, `word-break` | `overflow-wrap: break-word` and `anywhere`, `word-break: break-all` and `break-word` |
| `letter-spacing` | Yes |
| `-webkit-line-clamp`, `line-clamp` | Yes, with `display: -webkit-box` and `-webkit-box-orient: vertical` |
| `font-variant-numeric` | `tabular-nums`, `proportional-nums`, `lining-nums`, `oldstyle-nums`, `diagonal-fractions`, `stacked-fractions`, `ordinal`, `slashed-zero`, as OpenType features |
| `font-feature-settings` | Yes |
| `content` | A string, on `::before` and `::after` |

### Backgrounds and borders

| Property | Notes |
| --- | --- |
| `background-color` | Yes |
| `background-image` | `url()` of a local file, `linear-gradient()` with up to four stops (any angle or corner direction), and `radial-gradient()` drawn from the center at `farthest-corner` size |
| `background-size` | `cover`, `contain`, `100% 100%`, `auto` |
| `background-position` | Keywords, lengths, percentages |
| `box-shadow` | Multiple shadows, `inset` shadows, spread, blur. Animatable. |
| `outline`, `outline-width`, `outline-style`, `outline-color`, `outline-offset` | Drawn as a ring outside the border |

Conic and repeating gradients aren't drawn. Radial gradients at other positions or sizes draw at the center, at farthest-corner size. `background-repeat`, `background-clip`, `background-origin` and `background-attachment` are accepted and ignored.

### Effects

| Property | Notes |
| --- | --- |
| `opacity` | Group opacity. A translucent box with children is composited once, as in a browser. |
| `filter` | `blur()`, `brightness()`, `contrast()`, `grayscale()`, `hue-rotate()`, `invert()`, `opacity()`, `saturate()`, `sepia()`, composed into one color matrix. `drop-shadow()` and `url()` aren't supported. macOS only. |
| `backdrop-filter` | `blur()`: what's behind the box is blurred under its background. Other functions are skipped. macOS only. |
| `clip-path` | `inset(50%)` or more hides the box (Tailwind's `sr-only`). Other shapes aren't applied. |
| `mix-blend-mode`, `mask` | No |

### Transforms

| Property | Notes |
| --- | --- |
| `transform` | `translate*()`, `rotate()`/`rotateZ()`, `scale*()` and lists of them. Translations in `px`, `%` or mixed `calc()`. `translate3d()` and `scale3d()` keep their 2D part, and `translateZ()`/`perspective()` are ignored. `rotateX()`/`rotateY()`, `skew()` and `matrix()` aren't supported. |
| `translate`, `rotate`, `scale` | Yes, composed with `transform` as CSS composes them |
| `transform-origin` | Keywords, lengths and percentages |

A list composes as translate, rotate, scale, then any translate that came after a rotation (moving along the rotated axes). Rotations and scales in one list are each summed into one.

### Transitions and animations

| Property | Notes |
| --- | --- |
| `transition`, `transition-property`, `-duration`, `-delay`, `-timing-function` | Animates colors, `opacity`, `box-shadow`, transforms, and px `width`/`height`/insets. See [Styling](styling.html#transitions-and-animations). |
| `animation`, `animation-name`, `-duration`, `-delay`, `-timing-function`, `-iteration-count`, `-direction`, `-fill-mode`, `-play-state` | Yes |
| `animation-timeline`, `animation-range-*`, `animation-composition` | Accepted. Animations run on time. |

### Interaction

| Property | Notes |
| --- | --- |
| `cursor` | `auto`, `default`, `none`, `pointer`, `text`, `vertical-text`, `crosshair`, `copy`, `alias`, `context-menu`, `no-drop`, `not-allowed`, `grab`, `grabbing`, `col-resize`, `row-resize`, and the `n`/`e`/`s`/`w`/`ne`/`nw`/`se`/`sw`/`ew`/`ns`/`nesw`/`nwse-resize` cursors |
| `pointer-events` | `none` (inherited) lets presses and hover pass through. `auto` restores them. |
| `app-region`, `-webkit-app-region` | `drag` and `no-drag`, for [custom title bars](title-bar.html) |

### Images

| Property | Notes |
| --- | --- |
| `object-fit` | `fill`, `contain`, `cover`, `none`, `scale-down` |
| `object-position` | Yes |

### SVG

`fill`, `stroke`, `fill-opacity`, `fill-rule`, `stroke-opacity`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-dasharray`, `stroke-dashoffset`, `shape-rendering`, `text-anchor` and `dominant-baseline` apply to SVG elements, winning over their attributes. See [Images, SVG and assets](assets.html#css-inside-svg).

### Accepted and ignored

These parse without a warning but change nothing. Most have no effect in gpuijs, or are always effectively on:

`box-sizing`, `appearance`, `user-select`, `resize`, `vertical-align`, `text-indent`, `text-align-last`, `text-size-adjust`, `text-rendering`, `tab-size`, `list-style-*`, `color-scheme`, `print-color-adjust`, `background-repeat`, `background-clip`, `background-origin`, `background-attachment`, `word-spacing: 0`, `border-spacing: 0`, `font-stretch: normal`, `font-variant-caps: normal`, and the old `-webkit-box-*` and `-ms-flex-*` spellings that Emotion writes next to standard flexbox.

## Known gaps

The main things missing, in rough order of how often apps hit them:

1. `@font-face` web fonts.
2. `position: sticky`.
3. Changing font size or family within a paragraph's inline text.
4. Multi-line `<textarea>`.
5. `rowspan` in tables.
6. `skew()`, `matrix()` and real 3D transforms.
7. Conic and repeating gradients, and gradients with more than four stops.
8. `mix-blend-mode`, `mask` and arbitrary `clip-path` shapes.
9. `::placeholder`, `::selection` and scrollbar styling.
