import { createFileRoute, redirect } from "@tanstack/react-router";
import { queryKeys } from "@/lib/query-keys";
import type { VaultStatus } from "@/lib/ipc-types";
import { SetupPage } from "@/pages/setup-page";

export const Route = createFileRoute("/setup")({
  // Client-only: the vault status lives in the webview; guards run in the
  // browser after the status query resolves (there is no SSR in Tauri).
  ssr: false,
  beforeLoad: async ({ context }) => {
    const status: VaultStatus = await context.queryClient.ensureQueryData({
      queryKey: queryKeys.vaultStatus,
      queryFn: () => context.ipc.vaultStatus(),
    });
    if (status.initialized) {
      throw redirect({ to: status.unlocked ? "/" : "/unlock", replace: true });
    }
  },
  component: SetupPage,
});
