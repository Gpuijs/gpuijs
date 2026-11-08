# Images, SVG and assets

The host reads images from local files. gpuijs resolves the URLs your app uses (root-relative paths, remote URLs) to files before the host sees them.

## Where images come from

| `src` | Resolves to |
| --- | --- |
| `/images/logo.png` | `public/images/logo.png`, when it exists |
| `./logo.png`, `logo.png` | Resolved against the current route first (`./logo.png` on `/settings` is `/logo.png`), then looked up in `public/` |
| `https://example.com/a.jpg` | Downloaded once into the asset cache, then read from there |
| `/Users/me/Pictures/a.png` | An absolute file path that isn't under `public/` is read directly |

Supported formats are PNG, JPEG, WebP, GIF, BMP, TIFF and SVG.

### `public/`

Put static files in `public/`. In development the CLI points the app at it. `gpuijs build` copies it into the app bundle (`Contents/Resources/public`), so the same URLs work in the packaged app.

### Remote images

```tsx
<img src="https://avatars.githubusercontent.com/u/1?v=4" className="size-8 rounded-full" />
```

Remote images are downloaded on first use and cached by URL (a SHA-1 of it):

- in development, in `$TMPDIR/gpuijs-assets`;
- in a packaged app, in `~/Library/Caches/<bundle id>`.

Files are written to a temporary name and renamed, so a half-written download is never used. A failed download is retried the next time something asks for that URL. Until the file is there, the image draws nothing.

`new Image()` fires `load` once the file exists (downloading remote ones) and `error` otherwise. So Radix Avatar's fallback shows while an avatar loads or when it fails.

## `<img>`

```tsx
<img src="/cover.jpg" alt="" className="h-48 w-full rounded-xl object-cover object-top" />
```

- `object-fit`: `fill` (the default), `contain`, `cover`, `none` and `scale-down`.
- `object-position`: keywords, lengths and percentages.
- Rounded corners clip the image, and an image inside an `overflow: hidden` box with rounded corners takes the box's corners, as avatars need.
- `opacity`, `filter` and transforms apply.

## Background images

```css
.hero {
  background-image: url("/hero.jpg");
  background-size: cover;
  background-position: center;
}
```

`background-image: url(...)` takes a local file: a root-relative URL is looked up in `public/`, and anything else is tried as a file path. Remote and `data:` URLs aren't loaded in CSS (a warning is logged). Use `<img>` for remote images. `background-size` (`cover`, `contain`, `100% 100%` or `auto`) and `background-position` place it. Gradients are covered in [CSS support: backgrounds](css.html#backgrounds-and-borders).

## Inline SVG

```tsx
<svg viewBox="0 0 24 24" className="size-5 text-orange-500" fill="none" stroke="currentColor" strokeWidth={2}>
  <path d="M5 12h14M12 5l7 7-7 7" />
</svg>
```

An inline `<svg>` subtree is serialized back to SVG markup and rasterized at the window's scale factor, so it stays sharp on Retina displays. Re-rendering an unchanged SVG reuses the cached image.

### CSS inside SVG

The SVG is drawn as one image, but CSS still applies to its elements:

- `color` sets `currentColor`.
- `fill`, `stroke`, `fill-opacity`, `stroke-opacity`, `stroke-width`, `stroke-linecap`, `stroke-linejoin`, `stroke-dasharray`, `stroke-dashoffset`, `fill-rule`, `shape-rendering` and `text-anchor` from CSS win over the element's attributes, as in a browser.
- `opacity`, `display: none` and `visibility: hidden` hide or fade elements.
- `var(--x)` works in attributes (`stop-color="var(--chart-1)"`).
- `:hover` on an individual shape works. JavaScript finds the shape under the pointer (see below) and tells the host to match it.
- `transform` and `transform-origin` on SVG elements: rotation and scale turn about the element's center.

In React, camelCase SVG props become SVG attributes as react-dom writes them (`strokeWidth` becomes `stroke-width`, and `viewBox` stays as is).

### Events inside SVG

The host only knows the `<svg>`'s box, so JavaScript hit-tests pointer positions against the SVG's shapes: `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `path` and `text`, with their transforms and the viewBox applied. The shape under the pointer becomes the event's `target`. Moving between shapes fires `pointerover`/`pointerout`, `pointerenter`/`pointerleave` (and the `mouse*` versions), plus React's `onPointerEnter`/`onPointerLeave` on the shapes. Chart tooltips (Recharts, MUI X Charts, ApexCharts, jsVectorMap) rely on this.

`getBBox()`, `getComputedTextLength()` and `getBoundingClientRect()` on SVG elements are computed from attributes. ApexCharts sizes its labels this way.

### Limitations

- `preserveAspectRatio` isn't applied when mapping pointer positions. The SVG is assumed to scale uniformly by width.
- `<foreignObject>` content isn't drawn.
- SMIL animation (`<animate>`) doesn't run. Animate with CSS or framework state instead.

## SVG files as components

Vite React apps import SVG files as React components through `vite-plugin-svgr`. The gpuijs CLI supports the same import:

```tsx
import { ReactComponent as Logo } from "./logo.svg?react"

<Logo className="size-6 text-zinc-500" />
```

The component draws the file's markup at `1em × 1em` (as svgr's `icon: true` does), with your props spread on the root `<svg>`. Only relative `?react` specifiers are supported. An aliased path like `@/icons/x.svg?react` isn't resolved yet.

## Asset helpers

Two exports help code that handles image URLs itself:

```ts
import { assetPath, fetchAsset } from "@gpuijs/core"

assetPath("/images/a.png")                // "/abs/path/to/app/public/images/a.png", if it exists
await fetchAsset("https://example.com/a.png") // local file path, or null on failure
```

See [API reference](api.html#assetpath).
