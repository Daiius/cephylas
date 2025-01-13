import { initTRPC } from '@trpc/server';
import { 
  createHTTPHandler,
  CreateHTTPContextOptions,
} from '@trpc/server/adapters/standalone';
import { createServer } from 'http';


import { readLogs } from './logReader';

const createContext = ({
  req,
  res,
}: CreateHTTPContextOptions) => ({});
type Context = Awaited<ReturnType<typeof createContext>>;

const t = initTRPC.context<Context>().create();

export const router = t.router;
const publicProcedure = t.procedure;

const appRouter = router({
  stats: publicProcedure
    .query(async () => {
      //return 'Hello, tRPC!';
      return await readLogs(512);
    })
});
export type AppRouter = typeof appRouter;

const handler = createHTTPHandler({
  router: appRouter,
  createContext,
});

createServer(handler)
  .listen(3000);
console.log('listening on port 3000...');



