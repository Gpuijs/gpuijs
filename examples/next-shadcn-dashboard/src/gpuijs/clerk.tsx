// @clerk/nextjs stand-in: always signed in as one demo user, in one organization on the Pro plan.
// Clerk's hosted widgets (sign-in, profiles, pricing) need its servers, so they render as a box
// naming the widget.
import type { ReactNode } from 'react';
import { router } from './router';

const user = {
  id: 'user_demo',
  fullName: 'Demo User',
  firstName: 'Demo',
  lastName: 'User',
  imageUrl: '',
  emailAddresses: [{ emailAddress: 'demo@example.com' }],
  primaryEmailAddress: { emailAddress: 'demo@example.com' }
};
const organization = { id: 'org_demo', name: 'Acme Inc', slug: 'acme', hasImage: false, imageUrl: '' };
const membership = {
  id: 'mem_demo',
  role: 'org:admin',
  permissions: ['org:teams:manage', 'org:sys_memberships:manage', 'org:sys_billing:manage'],
  organization
};
const signOut = async (opts?: { redirectUrl?: string }) => router.push(opts?.redirectUrl ?? '/auth/sign-in');

export const useUser = () => ({ isLoaded: true, isSignedIn: true, user });
export const useAuth = () => ({ isLoaded: true, isSignedIn: true, userId: user.id, orgId: organization.id, has: () => true, signOut });
export const useClerk = () => ({ signOut, openUserProfile: () => router.push('/dashboard/profile') });
export const useOrganization = () => ({ isLoaded: true, organization, membership });
export const useOrganizationList = (_?: unknown) => ({
  isLoaded: true,
  setActive: async (_: unknown) => {},
  userMemberships: { data: [membership], revalidate: async () => {} }
});

export const ClerkProvider = ({ children }: { children: ReactNode }) => children;
export const Show = ({ children }: { children: ReactNode; when?: unknown; fallback?: ReactNode }) => children;
export const SignOutButton = ({ redirectUrl, children }: { redirectUrl?: string; children?: ReactNode }) => (
  <button onClick={() => signOut({ redirectUrl })}>{children ?? 'Sign out'}</button>
);

const widget = (name: string, action?: ReactNode) =>
  function ClerkWidget(_: Record<string, unknown>) {
    return (
      <div className='text-muted-foreground flex min-h-40 w-full flex-col items-center justify-center gap-3 rounded-lg border border-dashed p-6 text-sm'>
        <span>Clerk &lt;{name} /&gt; (hosted by Clerk, not available offline)</span>
        {action}
      </div>
    );
  };
const enter = (
  <button className='text-primary underline' onClick={() => router.push('/dashboard/overview')}>
    Continue as Demo User
  </button>
);
export const SignIn = widget('SignIn', enter);
export const SignUp = widget('SignUp', enter);
export const UserProfile = widget('UserProfile');
export const OrganizationProfile = widget('OrganizationProfile');
export const OrganizationList = widget('OrganizationList');
export const PricingTable = widget('PricingTable');
