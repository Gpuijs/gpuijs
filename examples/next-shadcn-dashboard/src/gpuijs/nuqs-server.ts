// nuqs/server, with a searchParamsCache that works outside a server: nuqs scopes it to a request
// with React.cache, which client React lacks. Here the page each navigation renders parses it last.
import { createLoader } from '../../node_modules/nuqs/dist/server.js';

export * from '../../node_modules/nuqs/dist/server.js';

export function createSearchParamsCache<P extends Parameters<typeof createLoader>[0]>(parsers: P) {
  const load = createLoader(parsers);
  let parsed = load({}) as ReturnType<typeof load>;
  return {
    parse: (searchParams: Parameters<typeof load>[0]) => (parsed = load(searchParams)),
    get: <K extends keyof typeof parsed>(key: K) => parsed[key],
    all: () => parsed
  };
}
