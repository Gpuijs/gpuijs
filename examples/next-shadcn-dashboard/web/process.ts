// Next.js inlines process.env at build time; in the browser there is no process. First import.
(globalThis as { process?: unknown }).process ??= { env: {} };
