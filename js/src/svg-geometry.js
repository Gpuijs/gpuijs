// SVG geometry from element attributes: what the host draws as one image, measured and
// hit-tested here (ApexCharts' bars and labels, jsVectorMap's regions).
import { measureText } from "./text.js";

const IDENTITY = [1, 0, 0, 1, 0, 0];

/** `a` then `b`: the matrix that applies `b` inside `a`'s space, as nested transforms do. */
export function multiply([a, b, c, d, e, f], [A, B, C, D, E, F]) {
  return [a * A + c * B, b * A + d * B, a * C + c * D, b * C + d * D, a * E + c * F + e, b * E + d * F + f];
}

export function apply([a, b, c, d, e, f], x, y) {
  return [a * x + c * y + e, b * x + d * y + f];
}

export function invert([a, b, c, d, e, f]) {
  const det = a * d - b * c || 1;
  return [d / det, -b / det, -c / det, a / det, (c * f - d * e) / det, (b * e - a * f) / det];
}

/** A `transform` attribute as a matrix. */
export function parseTransform(text) {
  let m = IDENTITY;
  for (const [, fn, args] of String(text ?? "").matchAll(/(\w+)\s*\(([^)]*)\)/g)) {
    const v = args.split(/[\s,]+/).filter(Boolean).map(Number);
    const rad = ((v[0] ?? 0) * Math.PI) / 180;
    const step = {
      matrix: () => v,
      translate: () => [1, 0, 0, 1, v[0] ?? 0, v[1] ?? 0],
      scale: () => [v[0] ?? 1, 0, 0, v[1] ?? v[0] ?? 1, 0, 0],
      rotate: () => {
        const [cx = 0, cy = 0] = v.slice(1);
        const r = [Math.cos(rad), Math.sin(rad), -Math.sin(rad), Math.cos(rad), 0, 0];
        return multiply(multiply([1, 0, 0, 1, cx, cy], r), [1, 0, 0, 1, -cx, -cy]);
      },
      skewX: () => [1, 0, Math.tan(rad), 1, 0, 0],
      skewY: () => [1, Math.tan(rad), 0, 1, 0, 0],
    }[fn];
    if (step) m = multiply(m, step());
  }
  return m;
}

/** Path data as polylines, one per subpath, with curves and arcs flattened. */
export function pathPolylines(d) {
  const lines = [];
  let line = null;
  let [x, y, sx, sy] = [0, 0, 0, 0];
  let [cx, cy] = [0, 0]; // last control point, for S/T
  let prev = "";
  const to = (px, py) => {
    line.push([px, py]);
    [x, y] = [px, py];
  };
  const curve = (pts) => {
    // Bezier of any degree, evaluated at a few steps (enough to hit-test and bound).
    for (let i = 1; i <= 12; i++) {
      let p = pts.map((q) => [...q]);
      const t = i / 12;
      while (p.length > 1) p = p.slice(1).map((q, j) => [p[j][0] + (q[0] - p[j][0]) * t, p[j][1] + (q[1] - p[j][1]) * t]);
      to(...p[0]);
    }
  };
  const tokens = String(d ?? "").match(/[a-zA-Z]|-?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?/gi) ?? [];
  let i = 0;
  let cmd = "";
  const num = () => Number(tokens[i++]);
  while (i < tokens.length) {
    if (/[a-zA-Z]/.test(tokens[i])) cmd = tokens[i++];
    const rel = cmd === cmd.toLowerCase();
    const [ox, oy] = rel ? [x, y] : [0, 0];
    switch (cmd.toUpperCase()) {
      case "M":
        line = [];
        lines.push(line);
        to(ox + num(), oy + num());
        [sx, sy] = [x, y];
        cmd = rel ? "l" : "L"; // further pairs are lines
        break;
      case "L":
        to(ox + num(), oy + num());
        break;
      case "H":
        to(ox + num(), y);
        break;
      case "V":
        to(x, oy + num());
        break;
      case "C": {
        const p = [ox + num(), oy + num(), ox + num(), oy + num(), ox + num(), oy + num()];
        [cx, cy] = [p[2], p[3]];
        curve([[x, y], [p[0], p[1]], [p[2], p[3]], [p[4], p[5]]]);
        break;
      }
      case "S": {
        const [rx, ry] = /[CS]/i.test(prev) ? [2 * x - cx, 2 * y - cy] : [x, y];
        const p = [ox + num(), oy + num(), ox + num(), oy + num()];
        [cx, cy] = [p[0], p[1]];
        curve([[x, y], [rx, ry], [p[0], p[1]], [p[2], p[3]]]);
        break;
      }
      case "Q": {
        const p = [ox + num(), oy + num(), ox + num(), oy + num()];
        [cx, cy] = [p[0], p[1]];
        curve([[x, y], [p[0], p[1]], [p[2], p[3]]]);
        break;
      }
      case "T": {
        [cx, cy] = /[QT]/i.test(prev) ? [2 * x - cx, 2 * y - cy] : [x, y];
        curve([[x, y], [cx, cy], [ox + num(), oy + num()]]);
        break;
      }
      case "A": {
        const [rx, ry, rot, large, sweep] = [num(), num(), num(), num(), num()];
        arc(x, y, rx, ry, rot, large, sweep, ox + num(), oy + num()).forEach((p) => to(...p));
        break;
      }
      case "Z":
        to(sx, sy);
        break;
      default:
        i++; // unknown command: skip a token rather than loop forever
    }
    prev = cmd;
  }
  return lines;
}

// An elliptical arc as points, from SVG's endpoint form (spec appendix B.2.4).
function arc(x1, y1, rx, ry, deg, large, sweep, x2, y2) {
  if (!rx || !ry) return [[x2, y2]];
  const phi = (deg * Math.PI) / 180;
  const [cos, sin] = [Math.cos(phi), Math.sin(phi)];
  const dx = (x1 - x2) / 2;
  const dy = (y1 - y2) / 2;
  const x = cos * dx + sin * dy;
  const y = -sin * dx + cos * dy;
  [rx, ry] = [Math.abs(rx), Math.abs(ry)];
  const fit = (x * x) / (rx * rx) + (y * y) / (ry * ry);
  if (fit > 1) [rx, ry] = [rx * Math.sqrt(fit), ry * Math.sqrt(fit)];
  const num = rx * rx * ry * ry - rx * rx * y * y - ry * ry * x * x;
  const k = (large === sweep ? -1 : 1) * Math.sqrt(Math.max(0, num / (rx * rx * y * y + ry * ry * x * x)));
  const [ccx, ccy] = [(k * rx * y) / ry, (-k * ry * x) / rx];
  const [cx, cy] = [cos * ccx - sin * ccy + (x1 + x2) / 2, sin * ccx + cos * ccy + (y1 + y2) / 2];
  const angle = (ux, uy, vx, vy) => Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
  const start = angle(1, 0, (x - ccx) / rx, (y - ccy) / ry);
  let delta = angle((x - ccx) / rx, (y - ccy) / ry, (-x - ccx) / rx, (-y - ccy) / ry);
  if (!sweep && delta > 0) delta -= 2 * Math.PI;
  if (sweep && delta < 0) delta += 2 * Math.PI;
  const steps = Math.max(4, Math.ceil(Math.abs(delta) / (Math.PI / 16)));
  return Array.from({ length: steps }, (_, i) => {
    const t = start + (delta * (i + 1)) / steps;
    const [px, py] = [rx * Math.cos(t), ry * Math.sin(t)];
    return [cos * px - sin * py + cx, sin * px + cos * py + cy];
  });
}

const attr = (n, k) => parseFloat(n.props?.[k]) || 0;
const elements = (n) => (n.kids ?? []).filter((k) => k.type);
const ownTransform = (n) => parseTransform(n.props?.transform);

function boundsOf(points) {
  if (!points.length) return null;
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y };
}

const corners = ({ x, y, width, height }) => [[x, y], [x + width, y], [x, y + height], [x + width, y + height]];

/**
 * An element's box in its own user space (`getBBox()`): text by its measured line, shapes by
 * their coordinates, groups by their children's boxes through the children's transforms.
 * shortcut: tspan offsets (dx/dy) and text on a path are left out.
 */
export function bbox(n) {
  switch (n.type) {
    case "text":
    case "tspan": {
      const font = { fontSize: n.props["font-size"] ?? n.styleWrites?.fontSize, fontWeight: n.props["font-weight"] ?? n.styleWrites?.fontWeight };
      const { width, height } = measureText(n.textContent, font);
      const anchor = n.props["text-anchor"];
      const x = attr(n, "x") - (anchor === "middle" ? width / 2 : anchor === "end" ? width : 0);
      return { x, y: attr(n, "y") - height * 0.8, width, height };
    }
    case "rect":
    case "image":
    case "foreignObject":
      return { x: attr(n, "x"), y: attr(n, "y"), width: attr(n, "width"), height: attr(n, "height") };
    case "circle":
      return { x: attr(n, "cx") - attr(n, "r"), y: attr(n, "cy") - attr(n, "r"), width: 2 * attr(n, "r"), height: 2 * attr(n, "r") };
    case "ellipse":
      return { x: attr(n, "cx") - attr(n, "rx"), y: attr(n, "cy") - attr(n, "ry"), width: 2 * attr(n, "rx"), height: 2 * attr(n, "ry") };
    case "line":
      return boundsOf([[attr(n, "x1"), attr(n, "y1")], [attr(n, "x2"), attr(n, "y2")]]);
    case "path":
      return boundsOf(pathPolylines(n.props.d).flat()) ?? { x: 0, y: 0, width: 0, height: 0 };
    case "polygon":
    case "polyline":
      return boundsOf(points(n)) ?? { x: 0, y: 0, width: 0, height: 0 };
    case "svg":
      if (n.props.width && !String(n.props.width).endsWith("%")) return { x: attr(n, "x"), y: attr(n, "y"), width: attr(n, "width"), height: attr(n, "height") };
  }
  const pts = elements(n).flatMap((k) => {
    const b = bbox(k);
    return b.width || b.height ? corners(b).map(([x, y]) => apply(ownTransform(k), x, y)) : [];
  });
  return boundsOf(pts) ?? { x: 0, y: 0, width: 0, height: 0 };
}

const points = (n) => {
  const v = String(n.props.points ?? "").split(/[\s,]+/).filter(Boolean).map(Number);
  return Array.from({ length: v.length >> 1 }, (_, i) => [v[2 * i], v[2 * i + 1]]);
};

/** From `n`'s user space to its root <svg>'s: the transforms of `n` and its ancestors below it. */
export function toRoot(n) {
  let m = IDENTITY;
  for (let e = n; e?.type && e.type !== "svg"; e = e.parent) m = multiply(ownTransform(e), m);
  return m;
}

/** `n`'s box in its root <svg>'s user space, transforms included (`getBoundingClientRect`). */
export function rootBox(n) {
  return boundsOf(corners(bbox(n)).map(([x, y]) => apply(toRoot(n), x, y)));
}

// Even-odd fill: a point is inside when a ray from it crosses the outline an odd number of times.
function insidePolylines(lines, x, y) {
  let inside = false;
  for (const pts of lines) {
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const [xi, yi] = pts[i];
      const [xj, yj] = pts[j];
      if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
    }
  }
  return inside;
}

function contains(n, x, y) {
  switch (n.type) {
    case "path":
      return insidePolylines(pathPolylines(n.props.d), x, y);
    case "polygon":
      return insidePolylines([points(n)], x, y);
    case "circle":
    case "ellipse": {
      const [rx, ry] = n.type === "circle" ? [attr(n, "r"), attr(n, "r")] : [attr(n, "rx"), attr(n, "ry")];
      return rx > 0 && ry > 0 && ((x - attr(n, "cx")) / rx) ** 2 + ((y - attr(n, "cy")) / ry) ** 2 <= 1;
    }
    case "rect":
    case "image":
    case "foreignObject":
    case "text":
    case "tspan": {
      const b = bbox(n);
      return x >= b.x && x <= b.x + b.width && y >= b.y && y <= b.y + b.height;
    }
    default:
      return false;
  }
}

const passive = (n) => n.props?.["pointer-events"] === "none" || n.styleWrites?.pointerEvents === "none" || n.props?.visibility === "hidden";

/**
 * The topmost element of `svg` at (x, y) in its user space, as the browser targets a pointer
 * event: later siblings paint over earlier ones; groups only pass hits through to children.
 * shortcut: fills only (no stroke-only hits), even-odd for every shape.
 */
export function hitTest(svg, x, y) {
  const visit = (n, px, py) => {
    if (passive(n) || n.props?.display === "none") return null;
    const [lx, ly] = apply(invert(ownTransform(n)), px, py);
    const kids = elements(n);
    for (let i = kids.length - 1; i >= 0; i--) {
      const hit = visit(kids[i], lx, ly);
      if (hit) return hit;
    }
    return contains(n, lx, ly) ? n : null;
  };
  const kids = elements(svg);
  for (let i = kids.length - 1; i >= 0; i--) {
    const hit = visit(kids[i], x, y);
    if (hit) return hit;
  }
  return null;
}
