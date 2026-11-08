#!/usr/bin/env bun
// `gpuijs dev <app-dir>`: bundle src/main.*, compile the CSS it imports (Tailwind included), launch the native host.
// `gpuijs build <app-dir> [--sign <identity>]`: the same, packaged as a macOS app in <app-dir>/.gpuijs.
import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import { cssImportsOf, importedCss } from "../src/css-imports.js";
import { vueModule } from "../src/sfc.js";
import { svgModule } from "../src/svgr.js";

const [command, dir = ".", ...flags] = process.argv.slice(2);
const sign = flags[0] === "--sign" ? flags[1] : undefined;
if ((command !== "dev" && command !== "build") || (flags.length && !sign)) {
  console.error("usage: gpuijs dev <app-dir>\n       gpuijs build <app-dir> [--sign <identity>]");
  process.exit(1);
}
const build = command === "build";

const root = path.resolve(dir);
const out = path.join(root, ".gpuijs");
mkdirSync(out, { recursive: true });
const here = import.meta.dir;
const repo = path.join(here, "../..");
const env = { ...process.env, GPUIJS_BUN: process.execPath };
const pkg = existsSync(path.join(root, "package.json")) ? await Bun.file(path.join(root, "package.json")).json() : {};
// `app.getName()`, as Electron takes it from package.json.
const appName = pkg.gpuijs?.name ?? pkg.productName ?? pkg.name ?? path.basename(root);
env.GPUIJS_APP_NAME = appName;

// The page's own `<body class>` (Vite apps put `dark:bg-gray-900` there); the renderer applies it.
const page = path.join(root, "index.html");
const bodyClass = existsSync(page) && (await Bun.file(page).text()).match(/<body[^>]*\sclass="([^"]*)"/)?.[1];
if (bodyClass) env.GPUIJS_BODY_CLASS = bodyClass;

// 1. JS: the bundle drops CSS imports; step 2 compiles them.
const entry = ["main.tsx", "main.jsx", "main.ts", "main.js"].map((f) => path.join(root, "src", f)).find(existsSync);
if (!entry) {
  console.error(`gpuijs: no src/main.{tsx,jsx,ts,js} in ${root}`);
  process.exit(1);
}
const production = Boolean(build || process.env.GPUIJS_PRODUCTION);
const result = await Bun.build({
  entrypoints: [entry],
  outdir: out,
  naming: "app.js",
  target: "bun",
  // The app is a client UI, so packages resolve to their browser builds; "node" ones are server
  // renderers (react-resizable-panels' has no layout effects).
  conditions: ["browser"],
  define: {
    // Vue's compile-time flags, which its bundler builds expect to be defined.
    __VUE_OPTIONS_API__: "true",
    __VUE_PROD_DEVTOOLS__: "false",
    __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: "false",
    // React's production build, as an app ships (and `GPUIJS_PRODUCTION=1` benchmarks compare). A
    // packaged app gets no environment from this CLI, so it carries the body class in the bundle.
    ...(production && { "process.env.NODE_ENV": '"production"' }),
    ...(build && bodyClass && { "process.env.GPUIJS_BODY_CLASS": JSON.stringify(bodyClass) }),
  },
  minify: production,
  metafile: true,
  plugins: [
    {
      // Vue single-file components, with the app's own compiler (`vue` ships it).
      name: "vue",
      setup(b) {
        const cssDir = path.join(out, "vue");
        b.onLoad({ filter: /\.vue$/ }, async (a) => {
          mkdirSync(cssDir, { recursive: true });
          const compiler = await import(Bun.resolveSync("vue/compiler-sfc", root));
          const { code, ts } = vueModule(await Bun.file(a.path).text(), a.path, compiler, cssDir, production);
          return { contents: code, loader: ts ? "ts" : "js" };
        });
      },
    },
    // Solid's JSX compiles to DOM code, with babel-preset-solid from the app, as vite-plugin-solid does.
    ...(dependsOn("solid-js") ? [solidJsx()] : []),
    { name: "drop-css", setup: (b) => b.onLoad({ filter: /\.css$/ }, () => ({ contents: "", loader: "js" })) },
    {
      // Vite apps import SVGs as components through vite-plugin-svgr.
      // shortcut: relative `?react` specifiers only; resolve aliased ones when an app uses them.
      name: "svg-react",
      setup(b) {
        b.onResolve({ filter: /\.svg\?react$/ }, (a) => ({
          path: path.resolve(path.dirname(a.importer), a.path.slice(0, -"?react".length)),
          namespace: "svg-react",
        }));
        b.onLoad({ filter: /.*/, namespace: "svg-react" }, async (a) => ({
          contents: svgModule(await Bun.file(a.path).text(), Bun.resolveSync("react", path.dirname(a.path))),
          loader: "js",
        }));
      },
    },
  ],
});
if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}

// 2. CSS: what the JS imports, in import order, as Vite applies it, after src/app.css when there is one
// (an entry for apps whose CSS isn't imported from JS, as in Next.js layouts). The Tailwind CLI scans
// the app's sources for class names; plain CSS passes through it unchanged.
const appCss = path.join(root, "src/app.css");
const own = existsSync(appCss) ? [appCss] : [];
const covered = own.length ? cssImportsOf(appCss, await Bun.file(appCss).text()) : new Set();
const sheets = [...own, ...importedCss(result.metafile.inputs, entry).filter((f) => !covered.has(f))];
if (sheets.length) {
  const input = path.join(out, "entry.css");
  // Tailwind scans from the entry's directory; the bundle there isn't the app's source.
  const imports = sheets.map((f) => `@import ${JSON.stringify(f)};\n`).join("");
  await Bun.write(input, `${imports}@source not ".";\n`);
  const cli = path.join(path.dirname(Bun.resolveSync("@tailwindcss/cli/package.json", here)), "dist/index.mjs");
  const outCss = path.join(out, "app.css");
  const tw = Bun.spawnSync([process.execPath, cli, "-i", input, "-o", outCss], { cwd: root, stdout: "inherit", stderr: "inherit" });
  if (tw.exitCode) process.exit(tw.exitCode);
  env.GPUIJS_CSS = outCss;
}

// Root-relative asset URLs ("/images/x.png") resolve against public/, as a dev server serves them.
env.GPUIJS_PUBLIC = path.join(root, "public");

if (build) {
  await packageApp();
  process.exit(0);
}

// 3. The native host spawns the bundle back under Bun and connects to it.
const host = findHost("debug");
const proc = Bun.spawn([host, path.join(out, "app.js")], { stdio: ["inherit", "inherit", "inherit"], env });
// Stopping the CLI (a benchmark closing its app) closes the window too.
for (const signal of ["SIGTERM", "SIGINT"]) process.on(signal, () => proc.kill(signal));
process.exit(await proc.exited);

// GPUIJS_HOST, else the repo's cargo build in a checkout, else the @gpuijs/<platform>-<arch>
// package npm installed beside this one.
function findHost(profile) {
  let host = process.env.GPUIJS_HOST;
  if (!host && existsSync(path.join(repo, "Cargo.toml"))) host = path.join(repo, "target", profile, "gpuijs");
  if (!host) {
    const name = `@gpuijs/${process.platform}-${process.arch}`;
    try {
      host = path.join(path.dirname(Bun.resolveSync(`${name}/package.json`, here)), "gpuijs");
    } catch {
      console.error(`gpuijs: ${name} isn't installed; gpuijs runs on macOS (arm64, x64) only`);
      process.exit(1);
    }
  }
  if (!existsSync(host)) {
    console.error(`gpuijs: host binary not found at ${host}; run \`cargo build\` or set GPUIJS_HOST`);
    process.exit(1);
  }
  return host;
}

function dependsOn(name) {
  return Boolean(pkg.dependencies?.[name] ?? pkg.devDependencies?.[name]);
}

function solidJsx() {
  return {
    name: "solid-jsx",
    setup(b) {
      // The app's own .jsx/.tsx; packages ship theirs compiled.
      b.onLoad({ filter: /^(?!.*\/node_modules\/).*\.[jt]sx$/ }, async (a) => {
        const from = (name) => Bun.resolveSync(name, root);
        const babel = await import(from("@babel/core"));
        const presets = [[from("babel-preset-solid"), { generate: "dom" }]];
        if (a.path.endsWith(".tsx")) presets.push([from("@babel/preset-typescript"), { isTSX: true, allExtensions: true }]);
        const source = await Bun.file(a.path).text();
        const { code } = await babel.transformAsync(source, { filename: a.path, presets, babelrc: false, configFile: false });
        return { contents: code, loader: "js" };
      });
    },
  };
}

// 3. `build`: <Name>.app with the release host as its executable and, in Resources, the bundle
// compiled into a standalone Bun executable, the CSS, public/ and what the host reads at launch
// (gpuijs.json; see `packaged` in src/main.rs). Name, id and version come from package.json:
// `{ "name", "version", "gpuijs": { "name", "id" } }`; the window's options from the app's `new Window`.
async function packageApp() {
  const name = appName;
  const id = pkg.gpuijs?.id ?? `dev.gpuijs.${pkg.name}`;
  if (!name || /[/:]/.test(name) || !/^[A-Za-z0-9.-]+$/.test(id)) {
    console.error(`gpuijs: package.json needs a name without / or : and a gpuijs.id of letters, digits, . and - (got ${name}, ${id})`);
    process.exit(1);
  }
  const run = (cmd, opts) => {
    if (Bun.spawnSync(cmd, { stdout: "inherit", stderr: "inherit", ...opts }).exitCode) {
      console.error(`gpuijs: \`${cmd.join(" ")}\` failed`);
      process.exit(1);
    }
  };

  if (!process.env.GPUIJS_HOST && existsSync(path.join(repo, "Cargo.toml"))) run(["cargo", "build", "--release"], { cwd: repo });
  const host = findHost("release");
  const app = path.join(out, `${name}.app`);
  const resources = path.join(app, "Contents/Resources");
  rmSync(app, { recursive: true, force: true });
  mkdirSync(path.join(app, "Contents/MacOS"), { recursive: true });
  mkdirSync(resources, { recursive: true });
  cpSync(host, path.join(app, "Contents/MacOS", name));
  run([process.execPath, "build", "--compile", path.join(out, "app.js"), "--outfile", path.join(resources, "app")]);
  // Native addons the bundle loads (Skia, for <canvas>) aren't embedded by this second compile: they
  // ship beside it, and js/src/media.js finds Skia's there.
  const addons = result.outputs.filter((o) => o.path.endsWith(".node")).map((o) => path.join(resources, path.basename(o.path)));
  for (const addon of addons) cpSync(path.join(out, path.basename(addon)), addon);
  if (env.GPUIJS_CSS) cpSync(env.GPUIJS_CSS, path.join(resources, "app.css"));
  if (existsSync(env.GPUIJS_PUBLIC)) cpSync(env.GPUIJS_PUBLIC, path.join(resources, "public"), { recursive: true });
  await Bun.write(path.join(resources, "gpuijs.json"), JSON.stringify({ name, id }));
  const xml = (v) => String(v).replace(/&/g, "&amp;").replace(/</g, "&lt;");
  const plist = {
    CFBundleName: name,
    CFBundleDisplayName: name,
    CFBundleExecutable: name,
    CFBundleIdentifier: id,
    CFBundleVersion: pkg.version ?? "0.0.0",
    CFBundleShortVersionString: pkg.version ?? "0.0.0",
    CFBundlePackageType: "APPL",
  };
  await Bun.write(
    path.join(app, "Contents/Info.plist"),
    `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
${Object.entries(plist).map(([k, v]) => `  <key>${k}</key><string>${xml(v)}</string>`).join("\n")}
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
`,
  );

  // Hardened runtime, as notarization requires; Bun's JIT needs the entitlements. The sidecar and
  // its addons are signed before the bundle that seals them.
  if (sign) {
    const entitlements = path.join(here, "entitlements.plist");
    for (const target of [...addons, path.join(resources, "app"), app]) {
      run(["codesign", "--force", "--timestamp", "--options", "runtime", "--entitlements", entitlements, "--sign", sign, target]);
    }
  }
  console.log(`gpuijs: built ${path.relative(process.cwd(), app)}`);
}
