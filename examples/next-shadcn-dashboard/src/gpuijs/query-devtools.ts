// @tanstack/react-query-devtools stand-in: the panel is a Solid.js app building its own DOM, which
// gpuijs (a React renderer) doesn't host. It is a development aid, absent from production builds.
export const ReactQueryDevtools = (_: Record<string, unknown>) => null;
