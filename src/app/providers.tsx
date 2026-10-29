import { createHashHistory, createMemoryHistory, createRouter } from "@tanstack/react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { IpcProvider } from "@/app/ipc-context";
import { routeTree } from "@/routeTree.gen";
import type { VaultStatus } from "@/lib/ipc-types";

import type { QueryClient } from "@tanstack/react-query";
import type { IpcClient } from "@/lib/ipc";

/** Route context injected by `createAppRouter`; typed in `beforeLoad`. */
export interface RouterContext {
  queryClient: QueryClient;
  ipc: IpcClient;
}

// Register the router and its context for type-safe navigation and beforeLoad.
declare module "@tanstack/react-router" {
  interface Register {
    router: AppRouter;
    context: RouterContext;
  }
}


/** In-memory history for tests (no hash side-effects on window.location). */
export function createTestHistory(initialHref = "/") {
  return createMemoryHistory({ initialEntries: [initialHref] });
}

export function createAppRouter(
  context: RouterContext,
  history = createHashHistory(),
  // Pending/error rendering is immediate in the app; tests render synchronously.
  defaultPendingMs?: number,
) {
  return createRouter({
    routeTree,
    history,
    context: { queryClient: context.queryClient, ipc: context.ipc } satisfies RouterContext,
    defaultPreload: "intent",
    defaultPendingMs: defaultPendingMs ?? 0,
    scrollRestoration: false,
  });
}

export type AppRouter = ReturnType<typeof createAppRouter>;

export type { VaultStatus };

/**
 * Query → IPC providers live outside the router. `VaultProvider` is inside
 * `<RouterProvider>` (see `__root`), because it navigates on lock and its
 * status is read from the Query cache, which the route guards use too.
 */
export function AppProviders({ context, children }: { context: RouterContext; children: React.ReactNode }) {
  return (
    <QueryClientProvider client={context.queryClient}>
      <IpcProvider client={context.ipc}>{children}</IpcProvider>
    </QueryClientProvider>
  );
}
