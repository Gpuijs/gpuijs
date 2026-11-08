// `bun docs/build.ts`: renders docs/pages/*.md into a static site in docs/dist (open
// dist/index.html directly, no server needed). Markdown goes through Bun's built-in renderer;
// headings get anchors, `> [!NOTE]` blockquotes become callouts, and every internal link is
// checked, so a renamed page or heading fails the build.
import { cpSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";

// Sidebar order: [section, [[page, title], ...]]. A page's file is pages/<page>.md.
const NAV: [string, [string, string][]][] = [
  [
    "Getting started",
    [
      ["index", "Introduction"],
      ["quick-start", "Quick start"],
      ["frameworks", "Frameworks"],
      ["project", "Project structure"],
      ["cli", "CLI"],
    ],
  ],
  [
    "Building apps",
    [
      ["rendering", "Rendering"],
      ["elements", "Elements"],
      ["events", "Events"],
      ["forms", "Forms and inputs"],
      ["styling", "Styling"],
      ["layout", "Layout and measuring"],
      ["assets", "Images, SVG and assets"],
    ],
  ],
  [
    "Native shell",
    [
      ["window", "Window"],
      ["menus", "Menus"],
      ["dialogs", "Dialogs and app"],
      ["title-bar", "Custom title bars"],
    ],
  ],
  [
    "Reference",
    [
      ["api", "API reference"],
      ["css", "CSS support"],
      ["environment", "Browser environment"],
      ["libraries", "Library compatibility"],
      ["env-vars", "Environment variables"],
    ],
  ],
  [
    "Shipping",
    [
      ["shipping", "Packaging for macOS"],
      ["performance", "Performance and profiling"],
      ["troubleshooting", "Troubleshooting"],
    ],
  ],
  [
    "Internals",
    [
      ["architecture", "Architecture"],
      ["protocol", "Wire protocol"],
      ["contributing", "Contributing"],
    ],
  ],
];

const here = import.meta.dir;
const out = path.join(here, "dist");
const pages = NAV.flatMap(([section, items]) =>
  items.map(([slug, title]) => ({ slug, title, section })),
);

const slugify = (s: string) =>
  s
    .replace(/<[^>]+>/g, "")
    .toLowerCase()
    .replace(/&[a-z]+;/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
const strip = (s: string) =>
  s
    .replace(/<[^>]+>/g, "")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");

type Heading = { level: number; id: string; text: string };
const rendered = new Map<
  string,
  { html: string; headings: Heading[]; lead: string }
>();

for (const page of pages) {
  const md = await Bun.file(path.join(here, "pages", `${page.slug}.md`)).text();
  let html = Bun.markdown.html(md.replace(/^# .*\n/, ""));
  const headings: Heading[] = [];
  const used = new Set<string>();
  html = html.replace(/<h([234])>([\s\S]*?)<\/h\1>/g, (_, level, inner) => {
    let id = slugify(inner);
    for (let n = 2; used.has(id); n++) id = `${slugify(inner)}-${n}`;
    used.add(id);
    headings.push({ level: Number(level), id, text: strip(inner) });
    return `<h${level} id="${id}"><a class="anchor" href="#${id}" aria-label="Link to this section">#</a>${inner}</h${level}>`;
  });
  html = html
    .replace(
      /<blockquote>\n<p>\[!(NOTE|TIP|WARNING)\]\n?([\s\S]*?)<\/blockquote>/g,
      (_, kind, body) =>
        `<div class="callout ${kind.toLowerCase()}"><p class="callout-title">${{ NOTE: "Note", TIP: "Tip", WARNING: "Warning" }[kind as "NOTE"]}</p><p>${body}</div>`,
    )
    // highlight.js has no TSX grammar; its TypeScript one reads JSX.
    .replace(/class="language-(tsx|jsx|ts)"/g, 'class="language-typescript"')
    .replace(/class="language-(sh|shell|zsh)"/g, 'class="language-bash"')
    .replace(/<table>/g, '<div class="table-wrap"><table>')
    .replace(/<\/table>/g, "</table></div>");
  const lead = strip(html.match(/<p>([\s\S]*?)<\/p>/)?.[1] ?? "").slice(0, 200);
  rendered.set(page.slug, { html, headings, lead });
}

// Every internal link must reach a page, and an anchor on it.
const broken: string[] = [];
for (const page of pages) {
  for (const [, href] of rendered
    .get(page.slug)!
    .html.matchAll(/href="([^"]+)"/g)) {
    if (/^(https?:|mailto:)/.test(href)) continue;
    const [file, anchor] = href.split("#");
    const target = file
      ? rendered.get(file.replace(/\.html$/, ""))
      : rendered.get(page.slug);
    if (
      !target ||
      (file && !file.endsWith(".html")) ||
      (anchor && !target.headings.some((h) => h.id === anchor))
    )
      broken.push(`${page.slug}.md: ${href}`);
  }
}
if (broken.length) {
  console.error(`docs: broken links:\n  ${broken.join("\n  ")}`);
  process.exit(1);
}

const sidebar = (current: string) =>
  NAV.map(
    ([section, items]) =>
      `<div class="nav-section"><p class="nav-title">${section}</p><ul>${items
        .map(
          ([slug, title]) =>
            `<li><a href="${slug}.html"${slug === current ? ' aria-current="page"' : ""}>${title}</a></li>`,
        )
        .join("")}</ul></div>`,
  ).join("");

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const search: { t: string; s: string; u: string }[] = [];

pages.forEach((page, i) => {
  const { html, headings, lead } = rendered.get(page.slug)!;
  const toc = headings.filter((h) => h.level < 4);
  search.push({ t: page.title, s: page.section, u: `${page.slug}.html` });
  for (const h of toc)
    search.push({ t: h.text, s: page.title, u: `${page.slug}.html#${h.id}` });
  const [prev, next] = [pages[i - 1], pages[i + 1]];
  const pager = [
    prev
      ? `<a class="pager-prev" href="${prev.slug}.html"><span>Previous</span>${prev.title}</a>`
      : "<span></span>",
    next
      ? `<a class="pager-next" href="${next.slug}.html"><span>Next</span>${next.title}</a>`
      : "<span></span>",
  ].join("");
  Bun.write(
    path.join(out, `${page.slug}.html`),
    `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${page.slug === "index" ? "gpuijs docs" : `${esc(page.title)} · gpuijs`}</title>
<meta name="description" content="${esc(lead)}">
<script>try{const t=localStorage.getItem("theme");if(t)document.documentElement.dataset.theme=t}catch{}</script>
<link rel="stylesheet" href="style.css">
<script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js" defer></script>
<script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/languages/rust.min.js" defer></script>
<script src="search.js" defer></script>
<script src="site.js" defer></script>
</head>
<body>
<header class="topbar">
  <button class="menu-button" aria-label="Open navigation" aria-expanded="false">☰</button>
  <a class="brand" href="index.html"><span class="logo" aria-hidden="true"></span>gpuijs</a>
  <div class="search">
    <input type="search" placeholder="Search the docs" aria-label="Search the docs" autocomplete="off">
    <kbd>/</kbd>
    <ul class="results" role="listbox" hidden></ul>
  </div>
  <a class="github" href="https://github.com/Gpuijs/gpuijs" aria-label="gpuijs on GitHub"><svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.37A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z"/></svg><span class="stars" hidden><svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M8 .6l2.3 4.7 5.1.7-3.7 3.6.9 5.1L8 12.3l-4.6 2.4.9-5.1L.6 6l5.1-.7z"/></svg><b></b></span></a>
  <button class="theme-button" aria-label="Toggle dark mode">◐</button>
</header>
<div class="layout">
  <nav class="sidebar" aria-label="Documentation">${sidebar(page.slug)}</nav>
  <main>
    <article>
      <p class="eyebrow">${page.section}</p>
      <h1>${page.title}</h1>
      ${html}
    </article>
    <nav class="pager" aria-label="Pages">${pager}</nav>
  </main>
  <aside class="toc">${toc.length > 1 ? `<p class="nav-title">On this page</p><ul>${toc.map((h) => `<li class="l${h.level}"><a href="#${h.id}">${esc(h.text)}</a></li>`).join("")}</ul>` : ""}</aside>
</div>
</body>
</html>
`,
  );
});

await Bun.write(
  path.join(out, "search.js"),
  `window.SEARCH=${JSON.stringify(search)};\n`,
);
for (const f of ["style.css", "site.js"])
  cpSync(path.join(here, f), path.join(out, f));
console.log(
  `docs: built ${pages.length} pages into ${path.relative(process.cwd(), out)}`,
);
