# Styling

gpuijs styles your app with real CSS. The host parses stylesheets with [lightningcss](https://lightningcss.dev) and runs a full cascade (specificity, source order, `!important`, cascade layers, inheritance and custom properties) over the native tree. Tailwind v4, plain CSS, CSS modules compiled to classes, and CSS-in-JS libraries that inject `<style>` tags all work.

## How CSS reaches the window

The host's stylesheet is built from four sources, in cascade order:

1. **The UA stylesheet.** Browser defaults, in the lowest layer. See [Elements](elements.html#boxes).
2. **The app's stylesheet.** At build time the CLI gathers `src/app.css`, every `.css` file your JavaScript imports and the `<style>` blocks of `.vue` files, in import order, and compiles them with Tailwind into `.gpuijs/app.css`. See [CLI: compile the CSS](cli.html#2-compile-the-css).
3. **Runtime `<style>` elements.** CSS that libraries add to `document.head`, and `<style>` elements the app renders, come after the app's CSS in document order.
4. **Inline styles.** The `style` prop and `el.style.*` writes, which beat everything except `!important`.

```tsx
import "./globals.css"       // compiled at build time, Tailwind included
import "some-lib/styles.css" // also compiled at build time, in import order
```

## Tailwind CSS

Tailwind v4 is built in. Start your stylesheet with:

```css
@import "tailwindcss";
```

Tailwind's CLI scans your app's sources for class names, so there's no `content` config. Everything compiles to standard CSS that gpuijs understands: preflight, `@layer`, `@property`, custom properties, `color-mix()`, `oklch()`, media and container queries, `:has()`, `:is()`/`:where()`, and nesting.

Variants work as in the browser: `hover:`, `active:`, `focus:`, `focus-visible:`, `focus-within:`, `disabled:`, `checked:`, `group-hover:`, `peer-checked:`, `has-[...]:`, `dark:`, `sm:`/`md:`/`lg:`, `@container` variants (`@md:`), `data-[state=open]:`, `aria-[...]:`, `rtl:`/`ltr:`, `placeholder-shown:`, `before:`/`after:`, `first:`/`last:`/`odd:`/`even:`, `*:` and arbitrary values like `[app-region:drag]`.

Plugins that produce plain CSS work too. `tw-animate-css` is used by shadcn's enter and exit animations.

### shadcn/ui

shadcn/ui components work as generated. Its theme (CSS variables in `:root` and `.dark`, `@theme inline` mappings, `oklch()` colors) is plain CSS. Copy the components into `src/components/ui`, set up the `@/` alias (see [Project structure](project.html#path-aliases)), and import your `globals.css`.

## Dark mode

### Class-based (`dark:` variant)

shadcn and Tailwind's `dark:` variant can key off a `dark` class on `<html>`. Set it with [`setRootClass`](api.html#setrootclass), or through `document.documentElement.classList` when you use `gpuijs/env`:

```tsx
import { setRootClass } from "gpuijs"

setRootClass(dark ? "dark" : "")

// or, as in a browser
document.documentElement.classList.toggle("dark", dark)
```

`setRootClass` replaces the whole `class` attribute of `<html>`. `classList.add`/`remove` keep the other classes.

### Attribute-based

MUI's CSS variables mode and many theme libraries write an attribute on `<html>` (`data-mui-color-scheme="dark"`, `data-theme="dark"`). `document.documentElement.setAttribute()` and [`setRootAttribute`](api.html#setrootattribute) both work, and selectors on the attribute match.

### Following the system

`@media (prefers-color-scheme: dark)` matches the macOS appearance and updates live when it changes. So do `light-dark()` colors and `matchMedia("(prefers-color-scheme: dark)")`, whose listeners hear changes. In code, [`prefersDark()`](api.html#prefersdark) reads it, and `window` hears an `appearance` event when it changes.

```ts
const sync = () => setRootClass(prefersDark() ? "dark" : "")
sync()
window.addEventListener("appearance", sync)
```

## Inline styles

In React, the `style` prop works as in react-dom: camelCase names, custom properties (`"--progress": "40%"`), and numbers become `px` except for unitless properties.

```tsx
<div style={{ width: 240, opacity: 0.8, "--accent": "#f97316" }} />
// width:240px;opacity:0.8;--accent:#f97316
```

> [!NOTE]
> gpuijs treats a shorter list of properties as unitless than react-dom does: `aspectRatio`, `flex`, `flexGrow`, `flexShrink`, `fontWeight`, `lineHeight`, `opacity`, `order` and `zIndex`. Pass strings for others (`style={{ scale: "1.1" }}`).

Vue's `:style` and Solid's `style` write through `el.style`, which works too. Writes through a ref also work: `el.style.height = "120px"`, `el.style.setProperty("--x", "4px")` and `el.style.cssText = "..."`. They're batched and sent to the host at the end of the task, or right away when JavaScript reads a size (a forced layout, as in a browser).

## CSS-in-JS and runtime styles

Libraries that inject CSS at runtime write `<style>` elements into `document.head`. Those reach the host:

- `document.createElement("style")` creates a style element.
- `appendChild`, `insertBefore`, `prepend` and `removeChild` on `document.head` add, reorder and remove sheets.
- `textContent`, `innerHTML`, `appendChild(textNode)` and the element's text node's `data` set the CSS.
- `sheet.insertRule()`, `deleteRule()` and `cssRules` work.

Edits within one task reach the host as one update. A sheet prepended to `head` loses ties to the app's CSS, which is what ApexCharts relies on to let the app override its defaults.

Known to work: **Emotion** (MUI's default engine, used by the `mui-dashboard` example), **goober**, **sonner**, **react-resizable-panels**, **react-remove-scroll**, and ApexCharts' injected styles.

> [!TIP]
> Emotion and similar libraries generate a stream of distinct CSS for as long as the app runs. The host keeps owned copies of only the rules it needs, so memory stays flat.

## Transitions and animations

`transition` and `@keyframes` animations run in the host, natively, without JavaScript per frame. They interpolate:

- `color`, `background-color` and `border-color`
- `opacity`
- `box-shadow`
- `transform`, `translate`, `rotate` and `scale`
- `width`, `height`, `top`, `right`, `bottom`, `left` and `inset`, when both ends are in px (a sidebar's `transition-[left,right,width]`)

Other properties jump to their end value. All of `animation-*` is supported, including `steps()` and `cubic-bezier()` easing, iteration counts, direction, fill mode and play state. Keyframes resolve `var()` through each animating element's own variables, which is how `tw-animate-css` parameterizes its enter and exit animations.

`prefers-reduced-motion: no-preference` always matches.

## When CSS isn't supported

Unsupported CSS doesn't break anything. The declaration or rule is skipped, and the host prints a warning the first time it sees each feature:

```text
[gpuijs] unsupported css: @font-face
[gpuijs] unsupported css: display: Ruby
```

Watch the terminal running `gpuijs dev` for these. [CSS support](css.html) is the full reference.
