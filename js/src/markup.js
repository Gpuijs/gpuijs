// A small HTML/SVG markup parser, for SVG files imported as components and for `innerHTML` writes
// (ApexCharts' tooltips, flatpickr's arrows).
// shortcut: whitespace-only text between tags is dropped; fine for widgets, not for prose.

const TOKEN = /<!--[\s\S]*?-->|<[?!][^>]*>|<\/([^\s>]+)\s*>|<([^\s/>]+)((?:[^>"']|"[^"]*"|'[^']*')*?)(\/?)>|([^<]+)/g;
const ATTR = /([^\s=/]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
const VOID = new Set("area base br col embed hr img input link meta source track wbr".split(" "));
const ENTITY = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };
const decode = (s) =>
  s.replace(/&(#x[\da-f]+|#\d+|\w+);/gi, (m, e) =>
    e[0] === "#" ? String.fromCodePoint(parseInt(e.slice(e[1] === "x" ? 2 : 1), e[1] === "x" ? 16 : 10)) : (ENTITY[e] ?? m),
  );

/**
 * Markup -> its top-level nodes: text as strings, elements as `[tag, attrs, children]`. `exact` keeps
 * whitespace-only text and comments (`["#comment", {}, [text]]`), as a <template> must: Solid walks it.
 */
export function parseMarkup(markup, exact = false) {
  const root = [null, {}, []];
  const open = [root];
  for (const [token, close, tag, attrs, selfClosing, text] of String(markup).matchAll(TOKEN)) {
    if (text !== undefined) {
      if (exact || text.trim()) open.at(-1)[2].push(decode(text));
    } else if (exact && token.startsWith("<!") && !/^<!doctype/i.test(token)) {
      // `<!>` (Solid's insertion marker) is an empty comment, as browsers parse it.
      open.at(-1)[2].push(["#comment", {}, [token.startsWith("<!--") ? token.slice(4, -3) : token.slice(2, -1)]]);
    } else if (tag) {
      const out = {};
      for (const [, name, dq, sq, bare] of attrs.matchAll(ATTR)) out[name] = decode(dq ?? sq ?? bare ?? "");
      const el = [tag, out, []];
      open.at(-1)[2].push(el);
      if (!selfClosing && !VOID.has(tag.toLowerCase())) open.push(el);
    } else if (close) {
      // Close up to the matching element, so a stray end tag can't unbalance the rest.
      const at = open.findLastIndex((el, i) => i > 0 && el[0].toLowerCase() === close.toLowerCase());
      if (at > 0) open.length = at;
    }
  }
  return root[2];
}
