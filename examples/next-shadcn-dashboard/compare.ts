// Renders each page in gpuijs and in Chrome at the same viewport and scale, runs the same clicks in
// both, and saves the two screenshots side by side for comparing by eye:
//
//   bun compare.ts [name-filter]   → .gpuijs/compare/<scenario>.{gpuijs,web}.png and index.html
//                                    (and <scenario>.<side>.log when the page threw)
//
// gpuijs takes input through the host's bench socket (src/bench.rs), Chrome (headless) through the
// DevTools protocol. Both follow the system's light/dark appearance.
// shortcut: screenshots only, no pixel diff; add one when the pages get close enough for it to mean something.
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

type Step = { click: string; index?: number } | { wheel: number };
type Scenario = { name: string; route: string; steps?: Step[] };
type Rect = { x: number; y: number; width: number; height: number };
type Target = {
  rect(selector: string, index?: number): Promise<Rect | null>;
  send(input: { kind: 'click' | 'wheel'; x: number; y: number; dy?: number }): Promise<void>;
  /** Brings the window to the front, where it draws (Chrome is headless). */
  front(): Promise<void>;
  screenshot(file: string): Promise<void>;
  /** Uncaught errors the page reported so far, and the whole log they came in. */
  errors(): string[];
  log(): string;
  close(): Promise<void>;
};

const [width, height] = [1440, 900];
const titleBar = 32; // the macOS title bar over a gpuijs window, in CSS px
const settle = 4500; // the overview's slowest parallel route waits 3s
const app = import.meta.dir;
const out = path.join(app, '.gpuijs/compare');
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const dark = Bun.spawnSync(['defaults', 'read', '-g', 'AppleInterfaceStyle']).stdout.toString().trim() === 'Dark';

const pages = [
  'overview', 'product', 'product/new', 'product/1', 'users', 'kanban', 'chat', 'ai-chat', 'forms',
  'forms/basic', 'forms/advanced', 'forms/multi-step', 'forms/sheet-form', 'react-query', 'elements/icons',
  'notifications', 'profile', 'workspaces', 'workspaces/team', 'billing', 'exclusive'
];
const scenarios: Scenario[] = [
  ...pages.map((p) => ({ name: p.replace(/\//g, '-'), route: `/dashboard/${p}` })),
  { name: 'sign-in', route: '/auth/sign-in' },
  { name: 'not-found', route: '/nowhere' },
  { name: 'sidebar-open', route: '/dashboard/overview', steps: [{ click: 'header [data-slot="sidebar-trigger"]' }] },
  { name: 'kbar', route: '/dashboard/overview', steps: [{ click: 'header [data-slot="button"]', index: 1 }] },
  { name: 'theme-select', route: '/dashboard/overview', steps: [{ click: 'header [data-slot="select-trigger"]' }] },
  { name: 'notifications-popover', route: '/dashboard/overview', steps: [{ click: 'header [data-slot="popover-trigger"]' }] },
  { name: 'user-menu', route: '/dashboard/overview', steps: [{ click: '[data-slot="dropdown-menu-trigger"]' }] },
  { name: 'product-next-page', route: '/dashboard/product', steps: [{ click: '[aria-label="Go to next page"]' }] },
  { name: 'overview-scrolled', route: '/dashboard/overview', steps: [{ wheel: 600 }] }
];

// ---- gpuijs: the host's bench socket ----

async function gpuijs(route: string): Promise<Target> {
  const dir = mkdtempSync(path.join(tmpdir(), 'gpuijs-compare-'));
  const socketPath = path.join(dir, 'bench.sock');
  const env = { ...process.env, GPUIJS_BENCH_SOCKET: socketPath, GPUIJS_WINDOW_SIZE: `${width}x${height}`, ROUTE: route };
  const cli = spawn('bun', [path.join(app, '../../js/bin/gpuijs.js'), 'dev', app], { env, stdio: ['ignore', 'ignore', 'pipe'] });
  let log = '';
  cli.stderr!.on('data', (d) => (log += d));
  const waiting = new Map<number, (reply: any) => void>();
  let buffer = '';
  let socket: Awaited<ReturnType<typeof Bun.connect>> | undefined;
  for (let i = 0; i < 300 && !socket; i++) {
    await sleep(100);
    socket = await Bun.connect({
      unix: socketPath,
      socket: {
        data(_, data) {
          buffer += data.toString();
          for (let nl; (nl = buffer.indexOf('\n')) >= 0; buffer = buffer.slice(nl + 1)) {
            const reply = JSON.parse(buffer.slice(0, nl));
            waiting.get(reply.id)?.(reply);
            waiting.delete(reply.id);
          }
        },
        // The app crashed: fail what waits on it instead of hanging.
        close() {
          for (const resolve of waiting.values()) resolve({ error: 'the app exited' });
          waiting.clear();
        }
      }
    }).catch(() => undefined);
  }
  if (!socket) throw new Error('gpuijs: the bench socket never opened');
  const host = Bun.spawnSync(['pgrep', '-P', String(cli.pid)]).stdout.toString().trim().split('\n')[0];
  let nextId = 1;
  const call = (command: object) =>
    new Promise<any>((resolve) => {
      const id = nextId++;
      waiting.set(id, resolve);
      socket!.write(JSON.stringify({ id, ...command }) + '\n');
    });
  return {
    rect: async (selector, index = 0) => (await call({ cmd: 'query', selector, index })).rect,
    async send(input) {
      const reply = await call({ cmd: 'input', input });
      if (reply.error) throw new Error(`gpuijs: ${reply.error}`);
    },
    // A window behind others is occluded, and an occluded window draws nothing.
    async front() {
      Bun.spawnSync(['osascript', '-l', 'JavaScript', '-e', `ObjC.import("AppKit"); $.NSRunningApplication.runningApplicationWithProcessIdentifier(${host}).activateWithOptions(3)`]);
      await sleep(500);
    },
    errors: () => log.match(/^\w*Error: .*$/gm) ?? [],
    log: () => log,
    async screenshot(file) {
      // The viewport: the window's bounds below its title bar.
      const bounds = Bun.spawnSync([
        'osascript', '-l', 'JavaScript', '-e',
        `ObjC.import("CoreGraphics"); const w = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0))).find((w) => w.kCGWindowOwnerPID === ${host} && w.kCGWindowLayer === 0); w ? [w.kCGWindowBounds.X, w.kCGWindowBounds.Y + ${titleBar}, ${width}, ${height}].join(",") : ""`
      ]).stdout.toString().trim();
      if (!bounds) throw new Error('gpuijs: no window on screen');
      Bun.spawnSync(['screencapture', '-x', '-R', bounds, file]);
      // The capture is in the display's color space; Chrome's screenshots are sRGB.
      Bun.spawnSync(['sips', '--matchTo', '/System/Library/ColorSync/Profiles/sRGB Profile.icc', file], { stdout: 'ignore' });
    },
    async close() {
      socket!.end();
      cli.kill();
      await new Promise((r) => cli.once('exit', r));
      rmSync(dir, { recursive: true, force: true });
    }
  };
}

// ---- Chrome: headless, over the DevTools protocol ----

const chromePath = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

async function chrome(url: string): Promise<Target> {
  const profile = mkdtempSync(path.join(tmpdir(), 'gpuijs-compare-chrome-'));
  const port = 9300 + Math.floor(Math.random() * 600);
  const proc: ChildProcess = spawn(
    chromePath,
    ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, '--no-first-run', '--hide-scrollbars', 'about:blank'],
    { stdio: 'ignore' }
  );
  let ws: string | undefined;
  for (let i = 0; i < 100 && !ws; i++) {
    await sleep(100);
    const list = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).catch(() => []);
    ws = list.find((p: any) => p.type === 'page')?.webSocketDebuggerUrl;
  }
  if (!ws) throw new Error('chrome: no page to attach to');
  const cdp = await devtools(ws);
  // gpuijs draws at the display's scale (2x on a Retina Mac).
  await cdp('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 2, mobile: false });
  await cdp('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: dark ? 'dark' : 'light' }] });
  const errors: string[] = [];
  await cdp('Runtime.enable');
  cdp.on('Runtime.exceptionThrown', (p: any) => errors.push(p.exceptionDetails.exception?.description?.split('\n')[0] ?? p.exceptionDetails.text));
  await cdp('Page.navigate', { url });
  const evaluate = async (expression: string) => {
    const { result, exceptionDetails } = await cdp('Runtime.evaluate', { expression, returnByValue: true });
    if (exceptionDetails) throw new Error(`chrome: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`);
    return result.value;
  };
  return {
    rect: (selector, index = 0) =>
      evaluate(`(() => {
        const r = document.querySelectorAll(${JSON.stringify(selector)})[${index}]?.getBoundingClientRect()
        return r ? { x: r.x, y: r.y, width: r.width, height: r.height } : null
      })()`),
    async send({ kind, x, y, dy = 0 }) {
      if (kind === 'wheel') return void (await cdp('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY: dy }));
      await cdp('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      await cdp('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
      await cdp('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
    },
    front: async () => {},
    errors: () => errors,
    log: () => errors.join('\n'),
    async screenshot(file) {
      const { data } = await cdp('Page.captureScreenshot', { format: 'png' });
      await Bun.write(file, Buffer.from(data, 'base64'));
    },
    async close() {
      cdp.close();
      proc.kill();
      await new Promise((r) => proc.once('exit', r));
      rmSync(profile, { recursive: true, force: true });
    }
  };
}

async function devtools(url: string) {
  const socket = new WebSocket(url);
  await new Promise((r) => socket.addEventListener('open', r, { once: true }));
  const waiting = new Map<number, (reply: any) => void>();
  const listeners = new Map<string, (params: any) => void>();
  socket.addEventListener('message', (m) => {
    const message = JSON.parse(String(m.data));
    if (message.id) waiting.get(message.id)?.(message);
    else listeners.get(message.method)?.(message.params);
  });
  let nextId = 1;
  return Object.assign(
    (method: string, params: object = {}) =>
      new Promise<any>((resolve, reject) => {
        const id = nextId++;
        waiting.set(id, (reply) => (reply.error ? reject(new Error(`${method}: ${reply.error.message}`)) : resolve(reply.result)));
        socket.send(JSON.stringify({ id, method, params }));
      }),
    { close: () => socket.close(), on: (method: string, f: (params: any) => void) => listeners.set(method, f) }
  );
}

// ---- running ----

async function capture(target: Target, scenario: Scenario, file: string) {
  try {
    await target.front();
    await sleep(settle);
    for (const step of scenario.steps ?? []) {
      if ('wheel' in step) await target.send({ kind: 'wheel', x: width / 2, y: height / 2, dy: step.wheel });
      else {
        const r = await target.rect(step.click, step.index);
        if (!r) throw new Error(`nothing matches ${step.click} [${step.index ?? 0}]`);
        await target.send({ kind: 'click', x: r.x + r.width / 2, y: r.y + r.height / 2 });
      }
      await sleep(800);
    }
    await target.screenshot(file);
    const errors = target.errors();
    if (!errors.length) return null;
    await Bun.write(file.replace(/\.png$/, '.log'), target.log());
    return `threw ${[...new Set(errors)].join('; ')} (log: ${path.basename(file.replace(/\.png$/, '.log'))})`;
  } catch (e) {
    return (e as Error).message;
  } finally {
    await target.close();
  }
}

const filter = process.argv[2];
const picked = scenarios.filter((s) => !filter || s.name.includes(filter));
mkdirSync(out, { recursive: true });
const port = 4300 + Math.floor(Math.random() * 600);
const web = spawn('bun', [path.join(app, 'web/serve.ts')], { cwd: app, env: { ...process.env, PORT: String(port) }, stdio: 'ignore' });
const base = `http://localhost:${port}`;
for (let i = 0; i < 100 && !(await fetch(base).then((r) => r.ok).catch(() => false)); i++) await sleep(100);

const failures: string[] = [];
for (const s of picked) {
  for (const [side, open] of [['gpuijs', () => gpuijs(s.route)], ['web', () => chrome(base + s.route)]] as const) {
    const error = await open().then((t) => capture(t, s, path.join(out, `${s.name}.${side}.png`)), (e: Error) => e.message);
    console.log(`${s.name} ${side}: ${error ?? 'ok'}`);
    if (error) failures.push(`${s.name} ${side}: ${error}`);
  }
}
web.kill();

await Bun.write(
  path.join(out, 'index.html'),
  `<!doctype html><meta charset="utf-8"><title>gpuijs vs web</title>
<style>body{font:14px system-ui;margin:16px;background:#888}figure{margin:0 0 24px}div{display:flex;gap:8px}img{width:50%;min-width:0}</style>
${picked.map((s) => `<figure><figcaption>${s.name} — ${s.route} · gpuijs | web</figcaption><div><img src="${s.name}.gpuijs.png"><img src="${s.name}.web.png"></div></figure>`).join('\n')}`
);
console.log(`\n${picked.length} scenarios, ${failures.length} failures → ${path.relative(process.cwd(), out)}/index.html`);
for (const f of failures) console.log(`  ${f}`);
