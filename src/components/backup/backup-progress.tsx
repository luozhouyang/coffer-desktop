import { useEffect, useState } from "react";
import { useIpc } from "@/app/ipc-context";
import type { IpcEventName } from "@/lib/ipc";
import type { BackupProgressPayload } from "@/lib/ipc-types";

const PHASE_LABELS: Record<string, string> = {
  verify: "Verifying",
  snapshot: "Snapshotting vault",
  encrypt: "Encrypting",
  decrypt: "Decrypting",
  validate: "Validating backup",
  commit: "Restoring vault",
  done: "Finishing up",
};

function isProgressPayload(value: unknown): value is BackupProgressPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    "phase" in value &&
    typeof (value as BackupProgressPayload).phase === "string"
  );
}

/**
 * Subscribes to a backup progress event while `active`, keeping only the
 * latest payload. Events carry byte counts only — never paths or secrets.
 */
export function useBackupProgress(event: IpcEventName, active: boolean) {
  const ipc = useIpc();
  const [progress, setProgress] = useState<BackupProgressPayload | null>(null);

  useEffect(() => {
    if (!active) {
      setProgress(null);
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void ipc.on(event, (payload) => {
      if (isProgressPayload(payload)) setProgress(payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [active, event, ipc]);

  return progress;
}

export function progressPercent(progress: BackupProgressPayload | null): number | null {
  if (!progress || !Number.isFinite(progress.total) || progress.total <= 0) return null;
  return Math.max(0, Math.min(100, Math.round((progress.done / progress.total) * 100)));
}

/** Phase label + simple progress bar. No cancel action in P0. */
export function BackupProgress({ progress }: { progress: BackupProgressPayload | null }) {
  const percent = progressPercent(progress);
  const label = progress ? PHASE_LABELS[progress.phase] ?? "Working" : "Starting…";
  return (
    <div className="space-y-1.5" role="status" aria-live="polite">
      <div className="flex items-center justify-between text-[12px] text-muted-foreground">
        <span>{label}…</span>
        {percent !== null ? <span className="tabular-nums">{percent}%</span> : null}
      </div>
      <div
        className="h-1.5 overflow-hidden rounded-full bg-[#2A303B]"
        role="progressbar"
        aria-label="Backup progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
      >
        <div
          className="h-full rounded-full bg-[#E2A75A] transition-[width] duration-200"
          style={{ width: `${percent ?? 0}%` }}
        />
      </div>
    </div>
  );
}
