import { createFileRoute, Outlet, redirect } from "@tanstack/react-router";
import { settingsSearchSchema } from "@/app/search-schemas";
import { queryKeys } from "@/lib/query-keys";
import type { VaultStatus } from "@/lib/ipc-types";
import { SettingsShell } from "@/components/shell/settings-shell";

export const Route = createFileRoute("/settings")({
  validateSearch: settingsSearchSchema,
  // Client-only: the vault status lives in the webview; guards run in the
  // browser after the status query resolves (there is no SSR in Tauri).
  ssr: false,
  beforeLoad: async ({ context }) => {
    const status: VaultStatus = await context.queryClient.ensureQueryData({
      queryKey: queryKeys.vaultStatus,
      queryFn: () => context.ipc.vaultStatus(),
    });
    if (!status.initialized) throw redirect({ to: "/setup", replace: true });
    if (!status.unlocked) throw redirect({ to: "/unlock", replace: true });
  },
  component: SettingsLayout,
});

function SettingsLayout() {
  return (
    <SettingsShell>
      <Outlet />
    </SettingsShell>
  );
}
