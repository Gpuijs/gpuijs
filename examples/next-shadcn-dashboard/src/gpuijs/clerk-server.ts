// @clerk/nextjs/server stand-in: the demo user of ./clerk is always signed in.
const session = { userId: 'user_demo', orgId: 'org_demo', has: () => true };
export const auth = Object.assign(async () => session, { protect: async () => session });
