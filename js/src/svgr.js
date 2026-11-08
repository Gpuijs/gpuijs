// `import { ReactComponent } from "./x.svg?react"`, as vite-plugin-svgr compiles it with svgr's
// `icon: true`: a component drawing the file's markup at 1em, its props spread on the root <svg>.

import { parseMarkup } from "./markup.js";

const camel = (s) => s.trim().replace(/-([a-z])/g, (_, c) => c.toUpperCase());

/** SVG markup -> its root element as `[tag, props, children]`, props spelled as React takes them. */
export function parseSvg(markup) {
  const react = (node) => {
    if (typeof node === "string") return node;
    const [tag, attrs, kids] = node;
    const props = {};
    for (const [name, value] of Object.entries(attrs)) {
      if (name === "class") props.className = value;
      else if (name === "style") {
        props.style = Object.fromEntries(value.split(";").filter((d) => d.includes(":")).map((d) => d.split(":").map(camel)));
      } else props[name] = value;
    }
    return [tag, props, kids.map(react)];
  };
  return react(parseMarkup(markup).find((n) => typeof n !== "string"));
}

/** The JS module for one `.svg?react` import; `react` is the path the app resolves "react" to. */
export function svgModule(markup, react) {
  const [tag, props, kids] = parseSvg(markup);
  const tree = [tag, { ...props, width: "1em", height: "1em" }, kids];
  return `import { createElement as h } from ${JSON.stringify(react)};
const draw = ([t, a, k], props) => h(t, props ? { ...a, ...props } : a, ...k.map((c) => (typeof c === "string" ? c : draw(c))));
const tree = ${JSON.stringify(tree)};
export const ReactComponent = (props) => draw(tree, props);
`;
}
