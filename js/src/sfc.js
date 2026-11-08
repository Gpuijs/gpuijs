// `.vue` single-file components, compiled as @vitejs/plugin-vue does, with the app's own
// `vue/compiler-sfc`. Each <style> is written out as a CSS file the module imports, so it joins the
// app's CSS (and its Tailwind pass) like any imported stylesheet.
// shortcut: plain CSS <style> only (no scss/less) and no src="" blocks; add them when an app needs one.
import { writeFileSync } from "node:fs";
import path from "node:path";

/** The JS module for one `.vue` file, and whether it is TypeScript. Styles go to `cssDir`. */
export function vueModule(source, filename, compiler, cssDir, isProd = false) {
  const { descriptor, errors } = compiler.parse(source, { filename });
  if (errors.length) throw errors[0];
  const id = Bun.hash(filename).toString(36).slice(0, 8);
  const scoped = descriptor.styles.some((s) => s.scoped);
  let code = "const _sfc_main = {};";
  let bindingMetadata;
  if (descriptor.script || descriptor.scriptSetup) {
    // <script setup> renders from setup (`inlineTemplate`); a plain <script> gets `render` below.
    const script = compiler.compileScript(descriptor, { id, isProd, inlineTemplate: !!descriptor.scriptSetup, genDefaultAs: "_sfc_main" });
    code = script.content;
    bindingMetadata = script.bindings;
  }
  if (descriptor.template && !descriptor.scriptSetup) {
    const template = compiler.compileTemplate({
      source: descriptor.template.content,
      filename,
      id,
      scoped,
      isProd,
      compilerOptions: { bindingMetadata },
    });
    if (template.errors.length) throw template.errors[0];
    code += `\n${template.code.replace(/\bexport function render\b/, "function render")}\n_sfc_main.render = render;`;
  }
  if (scoped) code += `\n_sfc_main.__scopeId = "data-v-${id}";`;
  code += "\nexport default _sfc_main;";
  const css = descriptor.styles.map((style, i) => {
    const out = compiler.compileStyle({ source: style.content, filename, id: `data-v-${id}`, scoped: style.scoped, isProd });
    if (out.errors.length) throw out.errors[0];
    const file = path.join(cssDir, `${path.basename(filename, ".vue")}-${id}-${i}.css`);
    writeFileSync(file, out.code);
    return `import ${JSON.stringify(file)};\n`;
  });
  const lang = descriptor.scriptSetup?.lang ?? descriptor.script?.lang;
  return { code: css.join("") + code, ts: lang === "ts" || lang === "tsx" };
}
