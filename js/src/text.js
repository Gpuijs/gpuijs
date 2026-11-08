// Synchronous text measurement, for code that sizes text through the DOM (Recharts' axis labels
// measure a hidden <span>). Widths come from the UI font's advances, which the host sends first.
let metrics = null;

export function setMetrics(m) {
  metrics = m;
}

/** `{ width, height }` of one line of `text` in px, styled like an element's `style` object. */
// shortcut: always the UI font (any family), regular or bold; outside ASCII an average advance.
export function measureText(text, { fontSize, fontWeight, letterSpacing } = {}) {
  const px = parseFloat(fontSize) || 16;
  const bold = fontWeight === "bold" || Number(fontWeight) >= 600;
  const table = bold ? metrics?.bold : metrics?.regular;
  let ems = 0;
  let count = 0;
  for (const ch of String(text ?? "")) {
    ems += table?.[ch.codePointAt(0) - 32] ?? 0.55;
    count++;
  }
  return { width: ems * px + count * (parseFloat(letterSpacing) || 0), height: Math.round(px * 1.2) };
}
