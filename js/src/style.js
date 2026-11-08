// React style object -> CSS declaration text, following react-dom's px rule for numbers.
const UNITLESS = new Set([
  "aspectRatio",
  "flex",
  "flexGrow",
  "flexShrink",
  "fontWeight",
  "lineHeight",
  "opacity",
  "order",
  "zIndex",
]);

// Property names repeat across every node of a list; convert each once.
const names = new Map();
function cssName(k) {
  let name = names.get(k);
  if (name === undefined) names.set(k, (name = k.startsWith("--") ? k : k.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase())));
  return name;
}

export function styleToCss(style) {
  let css = "";
  for (const k in style) {
    const v = style[k];
    if (v == null || v === "" || typeof v === "boolean") continue;
    if (css) css += ";";
    css += cssName(k) + ":" + (typeof v === "number" && v !== 0 && !UNITLESS.has(k) ? v + "px" : v);
  }
  return css;
}
