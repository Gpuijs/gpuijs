// Next.js App Router stand-in. Each navigation builds the route's segment tree (root layout,
// nested layouts, parallel slots, page) once; async server components are awaited outside React and
// shown through Suspense, with the segment's loading.tsx as the fallback, as Next streams them.
// Navigations run in a transition, so the current page stays up until the next one is ready, and
// layouts keep their state because their element tree has the same shape.
// shortcut: no error.tsx boundaries, no route-level metadata; add them when a page needs them.
import React, { Suspense, createContext, startTransition, use, useContext, useEffect, useState } from 'react';
import type { ComponentType, ReactElement, ReactNode } from 'react';

type Url = { pathname: string; search: string };
const here = (): Url => ({ pathname: location.pathname, search: location.search });


export class Redirect extends Error {
  constructor(readonly url: string) {
    super(`redirect ${url}`);
  }
}
export class NotFound extends Error {}

let navigate: (url: string, replace?: boolean) => void = (url, replace) =>
  history[replace ? 'replaceState' : 'pushState'](null, '', url);
let refresh = () => {};
export const router = {
  push: (url: string) => navigate(url),
  replace: (url: string) => navigate(url, true),
  back: () => history.back(),
  forward: () => history.forward(),
  refresh: () => refresh(),
  prefetch: () => {}
};

const UrlContext = createContext<Url>(here());
export const useUrl = () => useContext(UrlContext);

type Props = Record<string, unknown>;
type Server = (props: any) => ReactNode | Promise<ReactNode>;

function Await({ value }: { value: Promise<ReactNode> }) {
  return use(value);
}

// A server component's element: rendered once, here, outside React.
export function server(C: Server, props: Props = {}, Loading?: ComponentType, notFound?: ComponentType) {
  const value = (async () => {
    try {
      return await C(props);
    } catch (e) {
      if (e instanceof Redirect) {
        queueMicrotask(() => navigate(e.url, true));
        return null;
      }
      if (e instanceof NotFound && notFound) return React.createElement(notFound);
      throw e;
    }
  })();
  return (
    <Suspense fallback={Loading ? <Loading /> : null}>
      <Await value={value} />
    </Suspense>
  );
}

// The root layout returns <html><head/><body/></html>; gpuijs already has the document, so its
// attributes go to the real <html> and <body> and only the body's children render.
export async function document(RootLayout: Server, children: ReactNode) {
  const html = (await RootLayout({ children })) as ReactElement<Props>;
  const { children: parts, suppressHydrationWarning: _, ...attrs } = html.props;
  for (const [k, v] of Object.entries(attrs)) globalThis.document.documentElement.setAttribute(k, String(v));
  const body = React.Children.toArray(parts as ReactNode).find(
    (e) => (e as ReactElement).type === 'body'
  ) as ReactElement<Props>;
  for (const c of String(body.props.className ?? '').split(/\s+/).filter(Boolean))
    globalThis.document.body.classList.add(c);
  return body.props.children as ReactNode;
}

export type Route = [pattern: string, build: (params: Record<string, string>, search: Props) => ReactNode];

// "/dashboard/product/:productId" and "/dashboard/profile/*" (a [[...catch-all]]).
function match(pattern: string, pathname: string) {
  const want = pattern.split('/');
  const got = pathname.replace(/\/$/, '').split('/');
  const params: Record<string, string> = {};
  for (let i = 0; i < want.length; i++) {
    if (want[i] === '*') return params;
    if (got[i] === undefined) return null;
    if (want[i].startsWith(':')) params[want[i].slice(1)] = decodeURIComponent(got[i]);
    else if (want[i] !== got[i]) return null;
  }
  return got.length === want.length ? params : null;
}

export function App({ routes, fallback }: { routes: Route[]; fallback: () => ReactNode }) {
  const build = (url: Url) => {
    // shortcut: a repeated query key keeps its last value; pass arrays when a page reads one.
    const search = Object.fromEntries(new URLSearchParams(url.search));
    for (const [pattern, page] of routes) {
      const params = match(pattern, url.pathname || '/');
      if (params) return page(params, search);
    }
    return fallback();
  };
  const [state, setState] = useState(() => ({ url: here(), tree: build(here()) }));

  useEffect(() => {
    const show = () => startTransition(() => setState({ url: here(), tree: build(here()) }));
    navigate = (url, replace) => {
      history[replace ? 'replaceState' : 'pushState'](null, '', url);
      show();
    };
    refresh = show;
    // nuqs updates the query with history.replaceState and tells its hooks itself; only the
    // pathname picks a route, so the tree is rebuilt on back/forward.
    addEventListener('popstate', show);
    return () => removeEventListener('popstate', show);
  }, []);

  return <UrlContext value={state.url}>{state.tree}</UrlContext>;
}
