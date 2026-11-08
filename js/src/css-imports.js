import path from "node:path";

/**
 * The stylesheets JS imports (`import "./x.css"`), from a Bun.build metafile, in the order a browser
 * applies them: depth-first through each module's imports, as ES modules evaluate.
 */
export function importedCss(inputs, entry) {
  const css = [];
  const seen = new Set();
  const visit = (file) => {
    if (seen.has(file)) return;
    seen.add(file);
    if (file.endsWith(".css")) css.push(path.resolve(file));
    else for (const i of inputs[file]?.imports ?? []) visit(i.path);
  };
  visit(path.relative(process.cwd(), entry));
  return css;
}

/** The files a stylesheet `@import`s directly, resolved like a bundler does. */
export function cssImportsOf(file, text) {
  const dir = path.dirname(file);
  const out = new Set();
  for (const [, spec] of text.matchAll(/@import\s+["']([^"']+)["']/g)) {
    if (spec.startsWith(".")) out.add(path.resolve(dir, spec));
    else {
      try {
        out.add(Bun.resolveSync(spec, dir));
      } catch {}
    }
  }
  return out;
}
