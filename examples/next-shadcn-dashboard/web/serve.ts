// Browser reference server: every path serves the app, which routes by location, as Next.js would.
import page from './index.html';

const server = Bun.serve({ port: Number(process.env.PORT ?? 3000), routes: { '/*': page }, development: true });
console.log(`next-shadcn-dashboard (browser reference): ${server.url}`);
