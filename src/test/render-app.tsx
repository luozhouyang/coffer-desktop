import { render, screen, waitFor, type RenderResult } from "@testing-library/react";
import { RouterProvider } from "@tanstack/react-router";
import { toast } from "sonner";
import { vi } from "vitest";
import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { TooltipProvider } from "@/components/ui/tooltip";
import { IpcProvider } from "@/app/ipc-context";
import { createAppRouter, createTestHistory } from "@/app/providers";
import { createQueryClient } from "@/app/query";
import { createMockIpc, type MockIpc } from "@/lib/mock-ipc";
import { setDefaultIpc } from "@/lib/ipc-client";
import { resetUiStore } from "@/app/ui-store";

export interface RenderAppOptions {
  ipc?: MockIpc;
  initialHref?: string;
}

export interface RenderAppResult extends RenderResult {
  ipc: MockIpc;
  queryClient: QueryClient;
  router: ReturnType<typeof createAppRouter>;
}

/**
 * Renders the full app against an in-memory mock IPC and a memory history.
 * Providers sit *outside* `RouterProvider` (Query → IPC → Router) so that
 * components rendered by `__root` and by route components alike can use query
 * and IPC hooks. No Tauri runtime is involved.
 */
export function renderApp(options: RenderAppOptions = {}): RenderAppResult {
  // Components outside the IPC provider (if any) must never reach a previous
  // test's mock through the process-wide default.
  resetUiStore();
  setDefaultIpc(null);
  const ipc = options.ipc ?? createMockIpc();
  setDefaultIpc(ipc);
  const queryClient = createQueryClient();
  queryClient.setDefaultOptions({ queries: { retry: false }, mutations: { retry: false } });
  const initialHref = options.initialHref ?? "/";
  const history = createTestHistory(initialHref);
  // Point the (hash) URL at the initial route: TanStack reconciles history on
  // first load, and a stale global hash from a previous test would redirect.
  window.history.replaceState(null, "", `#${initialHref}`);
  // defaultPendingMs: 0 renders pending components immediately in jsdom
  // (the app default is also 0; explicit here so harness tweaks stay local).
  const router = createAppRouter({ queryClient, ipc }, history, 0);

  const result = render(
    <QueryClientProvider client={queryClient}>
      <IpcProvider client={ipc}>
        <TooltipProvider>
          <RouterProvider router={router} />
        </TooltipProvider>
      </IpcProvider>
    </QueryClientProvider>,
  );

  return { ...result, ipc, queryClient, router };
}

/**
 * Waits for the router to settle on a pathname. If a route guard redirected
 * elsewhere (e.g. to `/` because a leftover cache entry said "unlocked"),
 * reports the actual location so the failure is obvious.
 */
export async function waitForPath(router: ReturnType<typeof createAppRouter>, pathname: string) {
  await waitFor(() => expect(router.state.location.pathname).toBe(pathname), { timeout: 4000 });
  await waitFor(() => expect(router.state.status).not.toBe("pending"), { timeout: 4000 });
  await waitFor(() => expect(router.state.status).not.toBe("pending"), { timeout: 4000 });
}

export function spyToasts() {
  const success = vi.spyOn(toast, "success").mockImplementation((() => "toast") as never);
  const error = vi.spyOn(toast, "error").mockImplementation((() => "toast") as never);
  const info = vi.spyOn(toast, "info").mockImplementation((() => "toast") as never);
  return { success, error, info };
}

export { screen };
