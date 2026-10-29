import { useEffect } from "react";
import { useLocation, useNavigate } from "@tanstack/react-router";
import { toast } from "sonner";
import { checkForUpdateOnce } from "@/components/shell/update-checker-state";
import { isTauriRuntime } from "@/lib/tauri-ipc";
import { checkForUpdate } from "@/lib/updater";

const TOAST_ID = "update-available";

/**
 * Mounted from `__root`. Inside Tauri, checks for updates once per app session
 * ~3s after startup and, when one exists, keeps a persistent toast offering
 * the way to Settings → Updates. Outside Tauri it is a complete no-op: the
 * runtime guard runs in the effect before any timer is scheduled.
 */
export function UpdateChecker() {
  const navigate = useNavigate();
  const pathname = useLocation({ select: (location) => location.pathname });

  // If the check already found something before the user navigated here, the
  // toast is no longer needed once they are on the update page.
  useEffect(() => {
    if (pathname === "/settings/updates") toast.dismiss(TOAST_ID);
  }, [pathname]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void checkForUpdateOnce(checkForUpdate).then((version) => {
      if (!version || cancelled) return;
      toast.info(`Coffer ${version} is available`, {
        id: TOAST_ID,
        description: "Install it from Settings → Updates.",
        duration: Infinity,
        action: {
          label: "View update",
          onClick: () => {
            toast.dismiss(TOAST_ID);
            void navigate({ to: "/settings/updates" });
          },
        },
      });
    });
    return () => {
      cancelled = true;
    };
  }, [navigate]);

  return null;
}
