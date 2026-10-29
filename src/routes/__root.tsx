import { useCallback } from "react";
import {
  createRootRouteWithContext,
  Outlet,
  useNavigate,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router";
import type { RouterContext } from "@/app/providers";
import { Lock } from "lucide-react";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Button } from "@/components/ui/button";
import { CommandPalette } from "@/components/command/command-palette";
import { ItemEditorSheet } from "@/components/editor/item-editor-sheet";
import { ProjectDialogs } from "@/components/projects/project-dialogs";
import { Kbd } from "@/components/common/kbd";
import { LockWarningBanner } from "@/components/shell/lock-warning-banner";
import { UpdateChecker } from "@/components/shell/update-checker";
import { VaultProvider } from "@/app/vault-context";

function RootComponent() {
  const navigate = useNavigate();
  const redirect = useCallback((path: string) => {
    void navigate({ to: path, replace: true });
  }, [navigate]);

  return (
    <VaultProvider redirect={redirect}>
      {/* Unlock/setup pages render outside the sidebar (which has its own provider). */}
      <TooltipProvider>
        <Outlet />
        <CommandPalette />
        <ItemEditorSheet />
        <ProjectDialogs />
        <LockWarningBanner />
        <UpdateChecker />
      </TooltipProvider>
      <Toaster position="top-center" offset={16} richColors closeButton />
    </VaultProvider>
  );
}

function RootPending() {
  return (
    <div className="flex h-svh items-center justify-center bg-background text-sm text-muted-foreground">
      Loading vault…
    </div>
  );
}

function ErrorComponent({ error }: ErrorComponentProps) {
  const router = useRouter();
  return (
    <div className="flex h-svh flex-col items-center justify-center gap-3 bg-background p-8 text-center">
      <div className="flex size-11 items-center justify-center rounded-xl border border-border bg-card">
        <Lock className="size-5 text-destructive" aria-hidden />
      </div>
      <h1 className="text-lg font-semibold">Something went wrong</h1>
      <p className="max-w-sm text-sm text-muted-foreground">
        {error instanceof Error ? error.message : "Unexpected error."}
      </p>
      <Button variant="outline" className="mt-2" onClick={() => void router.invalidate()}>
        Try again
      </Button>
    </div>
  );
}

function NotFoundComponent() {
  return (
    <div className="flex h-svh flex-col items-center justify-center gap-2 bg-background">
      <p className="text-sm text-muted-foreground">Page not found.</p>
      <p className="flex items-center gap-1.5 text-xs text-faint">
        Press <Kbd>⌘K</Kbd> to search your secrets.
      </p>
    </div>
  );
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootComponent,
  errorComponent: ErrorComponent,
  pendingComponent: RootPending,
  notFoundComponent: NotFoundComponent,
});
