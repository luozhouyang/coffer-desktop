import { createFileRoute, redirect } from "@tanstack/react-router";
import { unlockSearchSchema } from "@/app/search-schemas";
import { queryKeys } from "@/lib/query-keys";
import type { VaultStatus } from "@/lib/ipc-types";
import { UnlockPage } from "@/pages/unlock-page";

export const Route = createFileRoute("/unlock")({
  validateSearch: unlockSearchSchema,
  // Client-only: the vault status lives in the webview; guards run in the
  // browser after the status query resolves (there is no SSR in Tauri).
  ssr: false,
  beforeLoad: async ({ context }) => {
    // Always read fresh state from the backend so a lock that landed after a
    // previous cached read cannot render a stale unlocked screen.
    const status: VaultStatus = await context.queryClient.fetchQuery({
      queryKey: queryKeys.vaultStatus,
      queryFn: () => context.ipc.vaultStatus(),
      staleTime: 0,
    });
    if (!status.initialized) throw redirect({ to: "/setup", replace: true });
    if (status.unlocked) throw redirect({ to: "/", replace: true });
  },
  component: UnlockPage,
});
