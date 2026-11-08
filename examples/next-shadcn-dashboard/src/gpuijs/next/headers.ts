// Next.js stand-in: no request, so cookies come from document.cookie, where the app's client code
// writes them (active_theme, sidebar_state).
export async function cookies() {
  const jar = new Map(document.cookie.split('; ').filter(Boolean).map((c) => c.split('=') as [string, string]));
  return { get: (name: string) => (jar.has(name) ? { name, value: jar.get(name)! } : undefined) };
}
