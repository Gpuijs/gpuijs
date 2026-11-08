// `<canvas>`, `<video>` and `<audio>` elements' DOM API, on top of the renderer's host nodes.
// A canvas draws with @napi-rs/canvas (Skia) in this process, or with OpenGL for WebGL (webgl.js),
// and the host shows its pixels after each task that drew. Media plays in the host (AVPlayer, src/media.rs), which reports progress
// back as `media` events (the renderer's `mediaEvent`).
import { readdirSync } from "node:fs";
import path from "node:path";
import { createWebGLContext } from "./webgl.js";

// Context methods that don't draw; the rest send the canvas's pixels after the task.
const READS = /^(get|is|measure|create)/;
const dimension = (n, k, initial) => {
  const v = n.dims?.[k] ?? Number.parseInt(n.props[k]);
  return Number.isInteger(v) && v >= 0 ? v : initial;
};

export const CanvasNode = {
  get width() {
    return dimension(this, "width", 300);
  },
  set width(v) {
    this.resize("width", v);
  },
  get height() {
    return dimension(this, "height", 150);
  },
  set height(v) {
    this.resize("height", v);
  },
  // Setting either clears the canvas, as in the DOM.
  resize(k, v) {
    (this.dims ??= {})[k] = Math.max(0, Math.floor(Number(v)) || 0);
    // The new size goes to the host before the cleared pixels do.
    this.dom.touch(this);
    if (this.backing) {
      this.backing[k] = this[k];
      this.dom.paint(this);
    }
    if (this.gl) {
      this.gl._resizeDrawingBuffer();
      this.dom.paint(this);
    }
  },
  // A canvas keeps the kind of context it got first; asked for another, it answers null, as in the DOM.
  getContext(kind, attributes) {
    if (kind === "2d") return this.gl ? null : (this.context ??= context2d(this));
    if (this.backing) return null;
    try {
      this.gl ??= createWebGLContext(this, kind, attributes);
    } catch (e) {
      console.warn(e.message);
      return null;
    }
    return this.gl && (kind === "webgl2") === (this.gl._version === 2) ? this.gl : null;
  },
  /** What the host shows: straight-alpha RGBA rows, top first; null before any context. */
  pixels() {
    if (this.gl) return this.gl._snapshot();
    if (!this.backing) return null;
    const { width, height } = this.backing;
    return { width, height, data: this.backing.getContext("2d").getImageData(0, 0, width, height).data };
  },
  // A Skia canvas holding what it shows, to encode.
  encodable() {
    if (!this.gl) return this.getContext("2d") && this.backing;
    const { width, height, data } = this.gl._snapshot();
    const canvas = skia().createCanvas(width, height);
    const ctx = canvas.getContext("2d");
    const image = ctx.createImageData(width, height);
    image.data.set(data);
    ctx.putImageData(image, 0, 0);
    return canvas;
  },
  toDataURL(type, quality) {
    return this.encodable().toDataURL(type, quality);
  },
  toBlob(callback, type = "image/png", quality) {
    const blob = new Blob([this.encodable().toBuffer(type, quality)], { type });
    setTimeout(() => callback(blob));
  },
};

// A packaged app's Skia binding sits beside its executable (`gpuijs build`), outside the bundle.
export function skia() {
  const dir = path.dirname(process.execPath);
  const addon = readdirSync(dir).find((f) => /^skia\..*\.node$/.test(f));
  if (addon) process.env.NAPI_RS_NATIVE_LIBRARY_PATH ??= path.join(dir, addon);
  return require("@napi-rs/canvas");
}

function context2d(node) {
  const { createCanvas } = skia();
  const backing = (node.backing = createCanvas(node.width, node.height));
  const ctx = backing.getContext("2d");
  // `drawImage(canvas)` and `createPattern(canvas)` take another canvas element's pixels.
  // shortcut: canvases and @napi-rs/canvas images only, not <img> elements; load those when a library draws one.
  const unwrap = (a) => a?.backing ?? a;
  const methods = new Map();
  return new Proxy(ctx, {
    get(target, key) {
      if (key === "canvas") return node;
      const v = target[key];
      if (typeof v !== "function") return v;
      let method = methods.get(key);
      if (!method) {
        const draws = typeof key === "string" && !READS.test(key);
        method = (...args) => {
          if (draws) {
            // React re-rendered it at a new `width`/`height`: drawing starts on a canvas that size.
            if (backing.width !== node.width) backing.width = node.width;
            if (backing.height !== node.height) backing.height = node.height;
            node.dom.paint(node);
          }
          return v.apply(target, args.map(unwrap));
        };
        methods.set(key, method);
      }
      return method;
    },
    set(target, key, value) {
      target[key] = value;
      return true;
    },
  });
}

// Formats AVFoundation plays.
const PLAYABLE = /^(audio|video)\/(mp4|mpeg|mp3|aac|wav|x-wav|x-m4a|m4a|quicktime|x-aiff|aiff)\b/i;

export const MediaNode = {
  get media() {
    return (this._media ??= { paused: true, ended: false, currentTime: 0, duration: NaN, volume: 1, playbackRate: 1, readyState: 0, videoWidth: 0, videoHeight: 0, error: null });
  },
  get paused() {
    return this.media.paused;
  },
  get ended() {
    return this.media.ended;
  },
  get duration() {
    return this.media.duration;
  },
  get readyState() {
    return this.media.readyState;
  },
  get videoWidth() {
    return this.media.videoWidth;
  },
  get videoHeight() {
    return this.media.videoHeight;
  },
  get error() {
    return this.media.error;
  },
  get currentSrc() {
    return this.props.src ?? this.kids?.find((k) => k.type === "source")?.props.src ?? "";
  },
  play() {
    if (this.media.paused) {
      this.dom.media(this, "play");
      this.dom.mediaEvent(this, { event: "play" });
    }
    return Promise.resolve();
  },
  pause() {
    if (this.media.paused) return;
    this.dom.media(this, "pause");
    this.dom.mediaEvent(this, { event: "pause" });
  },
  load() {},
  canPlayType: (type) => (PLAYABLE.test(String(type)) ? "maybe" : ""),
  get currentTime() {
    return this.media.currentTime;
  },
  set currentTime(v) {
    this.dom.media(this, "seek", Number(v) || 0);
    this.dom.mediaEvent(this, { event: "seeked", currentTime: Number(v) || 0 });
  },
  get volume() {
    return this.media.volume;
  },
  set volume(v) {
    this.media.volume = Math.min(1, Math.max(0, Number(v)));
    this.dom.media(this, "volume", this.media.volume);
    this.dom.mediaEvent(this, { event: "volumechange" });
  },
  get playbackRate() {
    return this.media.playbackRate;
  },
  set playbackRate(v) {
    this.media.playbackRate = Number(v);
    this.dom.media(this, "rate", this.media.playbackRate);
    this.dom.mediaEvent(this, { event: "ratechange" });
  },
  // The host reads these as attributes.
  get muted() {
    return this.hasAttribute("muted");
  },
  set muted(v) {
    this.toggleAttribute("muted", Boolean(v));
    this.dom.mediaEvent(this, { event: "volumechange" });
  },
  get loop() {
    return this.hasAttribute("loop");
  },
  set loop(v) {
    this.toggleAttribute("loop", Boolean(v));
  },
  get autoplay() {
    return this.hasAttribute("autoplay");
  },
  set autoplay(v) {
    this.toggleAttribute("autoplay", Boolean(v));
  },
  get controls() {
    return this.hasAttribute("controls");
  },
  set controls(v) {
    this.toggleAttribute("controls", Boolean(v));
  },
};

// What each host report (or the element's own call) changes, then the DOM events it fires, in order.
const MEDIA_EVENTS = {
  loadedmetadata: ["durationchange", "loadedmetadata", "loadeddata", "canplay", "canplaythrough"],
  play: ["play", "playing"],
  pause: ["pause"],
  timeupdate: ["timeupdate"],
  seeked: ["seeked", "timeupdate"],
  ended: ["timeupdate", "pause", "ended"],
  error: ["error"],
  volumechange: ["volumechange"],
  ratechange: ["ratechange"],
};

/** Updates a media element's state from a report; returns the DOM events to fire. */
export function mediaUpdate(media, msg) {
  if (msg.currentTime != null) media.currentTime = msg.currentTime;
  if (msg.event === "loadedmetadata") Object.assign(media, { duration: msg.duration ?? Infinity, videoWidth: msg.videoWidth, videoHeight: msg.videoHeight, readyState: 4 });
  if (msg.event === "play") Object.assign(media, { paused: false, ended: false });
  if (msg.event === "pause") media.paused = true;
  if (msg.event === "ended") Object.assign(media, { paused: true, ended: true });
  if (msg.event === "error") media.error = { code: 4, message: "the host can't play this source" };
  return MEDIA_EVENTS[msg.event] ?? [];
}
