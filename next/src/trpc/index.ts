import { 
  createTRPCClient,
  httpBatchLink,
} from '@trpc/client';

import type { AppRouter } from '@/../../core-node';

export const trpc = createTRPCClient<AppRouter>({
  links: [
    httpBatchLink({
      url: 'http://cephylas-node:3000',
    }),
  ],
});

