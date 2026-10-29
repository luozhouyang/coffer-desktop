import { useNavigate, useSearch } from "@tanstack/react-router";
import { KeyRound } from "lucide-react";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Button } from "@/components/ui/button";
import { SettingsSectionNav } from "@/components/shell/settings-section-nav";
import { SettingsSidebar } from "@/components/shell/settings-sidebar";
import { Kbd } from "@/components/common/kbd";

export interface SettingsSearch {
  project?: string;
  kind?: string;
}

/** Settings shell: settings sidebar + a centred, max-width content column. */
export function SettingsShell({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as SettingsSearch;
  return (
    <SidebarProvider className="h-svh min-h-0">
      <SettingsSidebar />
      <SidebarInset className="flex min-h-0 flex-col overflow-hidden bg-background">
        <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
          <Tooltip>
            <TooltipTrigger asChild>
              <SidebarTrigger aria-label="Toggle sidebar" className="-ml-1" />
            </TooltipTrigger>
            <TooltipContent side="bottom" className="flex items-center gap-1.5">
              Toggle sidebar <Kbd>⌘B</Kbd>
            </TooltipContent>
          </Tooltip>
          <SettingsSectionNav className="ml-2" />
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto gap-1.5 text-muted-foreground"
            onClick={() => navigate({ to: "/", search: search as never })}
          >
            <KeyRound className="size-4" aria-hidden />
            Back to vault
          </Button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[952px] px-8 py-8">{children}</div>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
