// Serves the browser reference on every path, so /<page> opens that page as ROUTE does in gpuijs.
import page from './index.html';

const photo = Bun.file(new URL('../public/photo.png', import.meta.url));
const server = Bun.serve({
  port: Number(process.env.PORT ?? 3001),
  routes: { '/photo.png': new Response(photo), '/*': page }
});
console.log(`css-lab reference on ${server.url}`);
