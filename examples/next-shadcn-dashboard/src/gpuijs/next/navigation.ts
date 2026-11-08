// Next.js stand-in: navigation goes through the router in ../router.
import { NotFound, Redirect, router, useUrl } from '../router';

export const useRouter = () => router;
export const usePathname = () => useUrl().pathname;
export function redirect(url: string): never {
  throw new Redirect(url);
}
export function notFound(): never {
  throw new NotFound();
}
