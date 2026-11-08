// `document.head` for libraries that inject <style> elements (react-resizable-panels' drag cursor,
// sonner, react-remove-scroll): their CSS reaches the host, after the app's.
const noop = () => {};

export function createHead(send) {
  let styles = new Set();
  let queued = false;
  // Edits within one task (insert a rule, remove the old one) reach the host as one update, or
  // sooner when a forced layout needs them (MUI measures a menu as it opens, in its first CSS).
  const flush = () => {
    if (!queued) return;
    queued = false;
    send({ t: "styles", sheets: [...styles].map((s) => [s.text, ...s.rules].join("\n")) });
  };
  const changed = () => {
    if (queued) return;
    queued = true;
    queueMicrotask(flush);
  };

  const head = {
    nodeType: 1,
    tagName: "HEAD",
    nodeName: "HEAD",
    appendChild(el) {
      if (el?.tagName === "STYLE") {
        styles.delete(el); // re-appending moves it last
        styles.add(el);
        el.parentNode = head;
        changed();
      }
      return el;
    },
    insertBefore(el) {
      return head.appendChild(el);
    },
    // Earlier sheets lose ties: ApexCharts puts its defaults first so the app's CSS wins.
    prepend(el) {
      if (el?.tagName === "STYLE") {
        styles = new Set([el, ...[...styles].filter((s) => s !== el)]);
        el.parentNode = head;
        changed();
      }
    },
    get children() {
      return [...styles];
    },
    contains(el) {
      return el === head || styles.has(el);
    },
    removeChild(el) {
      if (styles.delete(el)) {
        el.parentNode = null;
        changed();
      }
      return el;
    },
  };

  // shortcut: rules added with insertRule follow the element's text whatever their index; fine for
  // libraries that keep a single rule (react-resizable-panels) or only use text.
  function createStyle() {
    const style = {
      nodeType: 1,
      tagName: "STYLE",
      nodeName: "STYLE",
      parentNode: null,
      text: "",
      rules: [],
      get textContent() {
        return style.text;
      },
      set textContent(value) {
        style.text = String(value ?? "");
        if (style.parentNode) changed();
      },
      get innerHTML() {
        return style.text;
      },
      set innerHTML(value) {
        style.textContent = value;
      },
      appendChild(node) {
        style.textContent = style.text + (node?.data ?? node?.textContent ?? "");
        return node;
      },
      // goober writes its CSS through the element's text node.
      firstChild: {
        get data() {
          return style.text;
        },
        set data(value) {
          style.textContent = value;
        },
      },
      // goober finds its sheet again as `window._goober`, the global a browser makes for an element id.
      get id() {
        return style._id ?? "";
      },
      set id(value) {
        style._id = value;
        if (value) globalThis[value] ??= style;
      },
      remove() {
        style.parentNode?.removeChild(style);
      },
      setAttribute: noop,
      removeAttribute: noop,
      getAttribute: () => null,
      // As in the DOM, a style element has a sheet only while it is in the document.
      get sheet() {
        return style.parentNode ? sheet : null;
      },
    };
    const sheet = {
      get cssRules() {
        return style.rules.map((cssText) => ({ cssText }));
      },
      insertRule(rule, index = 0) {
        style.rules.splice(index, 0, rule);
        changed();
        return index;
      },
      deleteRule(index) {
        style.rules.splice(index, 1);
        changed();
      },
      removeRule(index = 0) {
        sheet.deleteRule(index);
      },
    };
    return style;
  }

  return { head, createStyle, flush };
}
