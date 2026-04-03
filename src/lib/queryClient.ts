import { QueryClient } from '@tanstack/react-query';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 0,       // local IPC — retries are never useful
      staleTime: 5_000,
      gcTime: 10_000, // clear stale cache quickly for transient menu bar window
    },
  },
});
