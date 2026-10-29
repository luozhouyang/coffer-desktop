import { QueryClient } from "@tanstack/react-query";
import { IpcError, toIpcError } from "@/lib/ipc-errors";

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // Local IPC is cheap; stale-while-revalidate keeps navigation instant.
        staleTime: 5_000,
        refetchOnWindowFocus: false,
        retry: (failureCount, error) => {
          const code = toIpcError(error).code;
          if (code === "locked" || code === "notFound" || code === "validation" || code === "invalidPassword") {
            return false;
          }
          return failureCount < 1;
        },
      },
      mutations: { retry: false },
    },
  });
}

export { IpcError };
