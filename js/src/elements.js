// `<details>`, `<dialog>` and `<img>` elements' DOM API, on top of the renderer's host nodes. What
// needs the renderer (firing events, focus, files) goes through `this.dom`.

export const DetailsNode = {
  get open() {
    return this.hasAttribute("open");
  },
  set open(v) {
    if (Boolean(v) === this.open) return;
    this.toggleAttribute("open", Boolean(v));
    this.dom.toggled(this);
  },
};

// `show()` opens it in place; `showModal()` in the top layer, over a backdrop that keeps the page
// from the pointer, closed by Escape. `close` fires in a later task, as in a browser.
export const DialogNode = {
  get open() {
    return this.hasAttribute("open");
  },
  set open(v) {
    if (!v) this.isModal = false;
    this.toggleAttribute("open", Boolean(v));
  },
  get returnValue() {
    return this._returnValue ?? "";
  },
  set returnValue(v) {
    this._returnValue = String(v);
  },
  show() {
    if (this.open && this.isModal) throw new DOMException("The dialog is already open as a modal dialog.", "InvalidStateError");
    if (this.open) return;
    this.open = true;
    this.dom.dialogOpened(this);
  },
  showModal() {
    if (this.open && !this.isModal) throw new DOMException("The dialog is already open as a non-modal dialog.", "InvalidStateError");
    if (this.open) return;
    this.isModal = true;
    this.open = true;
    this.dom.dialogOpened(this);
  },
  close(returnValue) {
    if (!this.open) return;
    if (returnValue !== undefined) this.returnValue = returnValue;
    this.open = false;
    this.dom.dialogClosed(this);
  },
  // `cancel` first, which a listener may cancel.
  requestClose(returnValue) {
    if (this.open && this.dom.dialogEvent(this, "cancel")) this.close(returnValue);
  },
};

// `<img>` and `new Image()`. The host draws an image from its file on its own; JS decodes it only
// when something asks: load listeners, `naturalWidth`, `decode()`, a WebGL texture.
// An <img> learns its size from the file's header. Its pixels are decoded only for one that is
// to be drawn (a WebGL texture, a canvas): the host shows the images in the page itself.
export const ImageNode = {
  get complete() {
    return !this.props.src || this._size !== undefined;
  },
  get naturalWidth() {
    return this.size()?.width ?? 0;
  },
  get naturalHeight() {
    return this.size()?.height ?? 0;
  },
  /** { width, height } now; null if it can't be read (a remote one not downloaded yet, a broken one). */
  size() {
    if (this._size === undefined) this.dom.probeImage(this);
    return this._size ?? null;
  },
  // Its `width`/`height` attribute, else its natural size (Three.js sizes textures by these).
  // shortcut: not the laid-out size of a CSS-sized image; measure it when a library needs that.
  get width() {
    return Number.parseInt(this.getAttribute("width")) || this.naturalWidth;
  },
  set width(v) {
    this.setAttribute("width", v);
  },
  get height() {
    return Number.parseInt(this.getAttribute("height")) || this.naturalHeight;
  },
  set height(v) {
    this.setAttribute("height", v);
  },
  /** The decoded Skia image, now; null if it can't be (a remote one not downloaded yet). */
  decoded() {
    if (this._image === undefined) this.dom.decodeImage(this);
    return this._image ?? null;
  },
  decode() {
    return this.dom.loadImage(this);
  },
};

// shortcut: EXIF orientation is ignored, so a rotated JPEG reports its stored width and height;
// read the orientation tag if a library lays out phone photos by naturalWidth.
/** An image file's pixel size from its header (PNG, JPEG, GIF, WebP, BMP); null for other formats. */
export function imageSize(b) {
  const u16 = (i) => (b[i] << 8) | b[i + 1];
  const le16 = (i) => b[i] | (b[i + 1] << 8);
  const le24 = (i) => b[i] | (b[i + 1] << 8) | (b[i + 2] << 16);
  const ascii = (i, n) => String.fromCharCode(...b.subarray(i, i + n));
  if (b.length >= 24 && b[0] === 0x89 && ascii(1, 3) === "PNG") return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
  if (b.length >= 10 && ascii(0, 4) === "GIF8") return { width: le16(6), height: le16(8) };
  if (b.length >= 26 && ascii(0, 2) === "BM") return { width: b.readInt32LE(18), height: Math.abs(b.readInt32LE(22)) };
  if (b.length >= 30 && ascii(0, 4) === "RIFF" && ascii(8, 4) === "WEBP") {
    const chunk = ascii(12, 4);
    if (chunk === "VP8 ") return { width: le16(26) & 0x3fff, height: le16(28) & 0x3fff };
    if (chunk === "VP8L") return { width: 1 + (((b[22] & 0x3f) << 8) | b[21]), height: 1 + (((b[24] & 0xf) << 10) | (b[23] << 2) | ((b[22] & 0xc0) >> 6)) };
    if (chunk === "VP8X") return { width: 1 + le24(24), height: 1 + le24(27) };
    return null;
  }
  if (b[0] === 0xff && b[1] === 0xd8) {
    // Segments up to the frame header (SOF0-15, except DHT, JPG and DAC).
    for (let i = 2; i + 9 < b.length; ) {
      if (b[i] !== 0xff) return null;
      const marker = b[i + 1];
      if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) return { width: u16(i + 7), height: u16(i + 5) };
      i += 2 + u16(i + 2);
    }
  }
  return null;
}
