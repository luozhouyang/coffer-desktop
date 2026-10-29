import { Outlet, useSearch } from "@tanstack/react-router";
import { AppSidebar, SIDEBAR_STYLE } from "@/components/shell/app-sidebar";
import { VaultList } from "@/components/vault/vault-list";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";

/** Sidebar + 372px list pane + detail outlet. Both panes scroll independently. */
export function VaultShell() {
  const search = useSearch({ strict: false }) as Record<string, unknown>;
  return (
    <SidebarProvider style={SIDEBAR_STYLE} className="h-svh min-h-0 overflow-hidden">
      <AppSidebar />
      <SidebarInset className="flex min-h-0 flex-row overflow-hidden bg-background">
        <aside
          aria-label="Secrets list"
          className="relative flex w-[372px] min-w-[300px] shrink-0 flex-col border-r border-border bg-card/40"
        >
          <VaultList search={search} />
        </aside>
        <main className="min-w-0 flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
