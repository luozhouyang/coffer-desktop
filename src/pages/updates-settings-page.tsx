import { useCallback, useEffect, useState } from "react";
import { ArrowUpCircle, RefreshCw, Rocket } from "lucide-react";
import { toast } from "sonner";
import { errorMessage } from "@/lib/ipc-errors";
import { isTauriRuntime } from "@/lib/tauri-ipc";
import {
  checkForUpdate,
  getAppVersion,
  installUpdate,
  relaunchApp,
  type DownloadProgress,
  type Update,
} from "@/lib/updater";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { InlineSpinner } from "@/components/common/status-bits";

type CheckStatus = "idle" | "checking" | "upToDate" | "available" | "error";
type InstallStatus = "idle" | "downloading" | "ready";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}

export function UpdatesSettingsPage() {
  const native = isTauriRuntime();
  const [version, setVersion] = useState<string | null>(null);
  const [status, setStatus] = useState<CheckStatus>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [installStatus, setInstallStatus] = useState<InstallStatus>("idle");
  const [progress, setProgress] = useState<DownloadProgress | null>(null);

  useEffect(() => {
    if (!native) return;
    let cancelled = false;
    getAppVersion()
      .then((v) => {
        if (!cancelled) setVersion(v);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [native]);

  const check = useCallback(async () => {
    setStatus("checking");
    setError(null);
    try {
      const found = await checkForUpdate();
      setUpdate(found);
      setStatus(found ? "available" : "upToDate");
    } catch (err) {
      setError(errorMessage(err));
      setStatus("error");
      toast.error("Could not check for updates", { description: errorMessage(err) });
    }
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setInstallStatus("downloading");
    setProgress({ downloaded: 0, total: null });
    try {
      await installUpdate(update, setProgress);
      setInstallStatus("ready");
      toast.success("Update installed", { description: "Restart Coffer to finish updating." });
    } catch (err) {
      setInstallStatus("idle");
      toast.error("Could not install the update", { description: errorMessage(err) });
    }
  }, [update]);

  const restart = useCallback(async () => {
    try {
      await relaunchApp();
    } catch (err) {
      toast.error("Could not restart Coffer", { description: errorMessage(err) });
    }
  }, []);

  const percent =
    progress && progress.total && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;
  const checking = status === "checking";

  return (
    <div>
      <header>
        <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-[0.12em] text-faint">
          <ArrowUpCircle className="size-3.5 text-[#E2A75A]" aria-hidden /> Updates
        </div>
        <h1 className="mt-2 text-[24px] font-semibold tracking-[-0.015em]">Updates</h1>
        <p className="mt-1 text-[13px] text-muted-foreground">
          Keep Coffer up to date with signed releases published on GitHub.
        </p>
      </header>

      <section className="mt-8 space-y-4" aria-labelledby="version-heading">
        <h2 id="version-heading" className="text-[13px] font-semibold">
          Version
        </h2>
        <div className="rounded-[14px] border border-border bg-card">
          <div className="flex items-center justify-between gap-6 px-4 py-3.5">
            <div className="min-w-0">
              <p className="text-[13px] font-medium">Current version</p>
              <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">
                {native ? "Installed on this computer." : "Not available in browser preview."}
              </p>
            </div>
            <div className="shrink-0 text-right">
              <span className="font-mono text-[13px] text-foreground" data-testid="current-version">
                {native ? (version ?? "…") : "—"}
              </span>
            </div>
          </div>
          <Separator />
          <div className="flex items-center justify-between gap-6 px-4 py-3.5">
            <div className="min-w-0">
              <p className="text-[13px] font-medium">Check for updates</p>
              <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground" role="status">
                {checkHint(status, error, native)}
              </p>
            </div>
            <div className="shrink-0">
              <Button
                size="sm"
                variant="outline"
                onClick={() => void check()}
                disabled={!native || checking || installStatus !== "idle"}
              >
                {checking ? <InlineSpinner className="size-3.5" /> : <RefreshCw className="size-3.5" aria-hidden />}
                {checking ? "Checking…" : "Check for updates"}
              </Button>
            </div>
          </div>
        </div>
        {!native ? (
          <p className="text-[11.5px] text-faint">
            Update checks are only available in the desktop app, not in browser preview.
          </p>
        ) : null}
      </section>

      {status === "available" && update ? (
        <section className="mt-8 space-y-4" aria-labelledby="available-heading">
          <h2 id="available-heading" className="text-[13px] font-semibold">
            Available update
          </h2>
          <div className="rounded-[14px] border border-border bg-card p-4">
            <div className="flex items-center gap-2 text-[13px] font-semibold">
              <Rocket className="size-4 text-[#E2A75A]" aria-hidden /> Coffer{" "}
              <span className="font-mono" data-testid="available-version">
                {update.version}
              </span>
            </div>
            <p className="mt-1.5 text-[12px] leading-relaxed text-muted-foreground">
              {update.currentVersion ? `You are on ${update.currentVersion}. ` : ""}
              Download and install to get the latest fixes.
            </p>
            {update.body ? (
              <pre className="mt-3 max-h-48 overflow-y-auto whitespace-pre-wrap rounded-[10px] border border-border bg-background/60 p-3 font-sans text-[12px] leading-relaxed text-muted-foreground">
                {update.body}
              </pre>
            ) : null}

            {installStatus === "downloading" ? (
              <div className="mt-4">
                <p className="text-[12px] text-muted-foreground" data-testid="download-progress" role="status">
                  Downloading…{percent !== null ? ` ${percent}%` : ""}
                  {progress?.total ? ` (${formatBytes(progress.downloaded)} of ${formatBytes(progress.total)})` : ""}
                </p>
                <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-border" aria-hidden>
                  <div
                    className="h-full rounded-full bg-[#E2A75A] transition-[width]"
                    style={{ width: `${percent ?? 5}%` }}
                  />
                </div>
              </div>
            ) : null}

            <div className="mt-4 flex items-center gap-2">
              {installStatus === "ready" ? (
                <Button size="sm" onClick={() => void restart()}>
                  <Rocket className="size-3.5" aria-hidden /> Restart to finish update
                </Button>
              ) : (
                <Button size="sm" onClick={() => void install()} disabled={installStatus === "downloading"}>
                  {installStatus === "downloading" ? (
                    <InlineSpinner className="size-3.5" />
                  ) : (
                    <ArrowUpCircle className="size-3.5" aria-hidden />
                  )}
                  Download and install
                </Button>
              )}
            </div>
          </div>
        </section>
      ) : null}
    </div>
  );
}

function checkHint(status: CheckStatus, error: string | null, native: boolean): string {
  if (!native) return "Update checks are disabled outside the desktop app.";
  switch (status) {
    case "checking":
      return "Looking for a newer release…";
    case "upToDate":
      return "You are running the latest version.";
    case "available":
      return "A new version is ready to install.";
    case "error":
      return `Update check failed: ${error ?? "unknown error"}`;
    default:
      return "Updates are checked automatically at startup.";
  }
}
