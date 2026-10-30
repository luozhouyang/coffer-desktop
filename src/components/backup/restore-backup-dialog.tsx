import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Eye, EyeOff, FileArchive, FolderOpen, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { useIpc } from "@/app/ipc-context";
import { fileNameFromPath, pickBackupOpenPath } from "@/lib/file-dialog";
import { absoluteDate } from "@/lib/format";
import { errorMessage } from "@/lib/ipc-errors";
import { EVENTS, type BackupPreview } from "@/lib/ipc-types";
import { queryKeys } from "@/lib/query-keys";
import { BackupProgress, useBackupProgress } from "@/components/backup/backup-progress";
import { InlineError } from "@/components/backup/export-backup-dialog";
import { InlineSpinner } from "@/components/common/status-bits";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

type Step = "pick" | "password" | "preview";

const PHASES = { pick: 0, password: 1, preview: 2, done: 3 } as const;

/**
 * Restore flow for first run (IMPORT_EXPORT_DESIGN.md §1.2): pick a
 * `.coffer` file, enter that backup's master password, review the preview,
 * then commit. Only reachable from the Setup page.
 */
export function RestoreBackupDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const ipc = useIpc();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>("pick");
  const [path, setPath] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [picking, setPicking] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const progress = useBackupProgress(EVENTS.backupRestoreProgress, pending || step === "preview");

  const reset = () => {
    setStep("pick");
    setPath(null);
    setPassword("");
    setShowPassword(false);
    setPreview(null);
    setPicking(false);
    setPending(false);
    setError(null);
  };

  const changeOpen = (next: boolean) => {
    // Keep the dialog open while a begin/commit is in flight; the backend
    // session would otherwise outlive the UI that knows about it.
    if (pending) return;
    if (!next) {
      // A preview that was never committed holds a backend session (temp
      // copy + derived key); drop it. The backend also expires stale ones.
      const sessionId = preview?.sessionId;
      if (sessionId) void ipc.backupRestoreCancel(sessionId).catch(() => undefined);
      reset();
    }
    onOpenChange(next);
  };

  const chooseFile = async () => {
    setError(null);
    setPicking(true);
    try {
      const picked = await pickBackupOpenPath();
      if (!picked) return;
      setPath(picked);
      setPassword("");
      setStep("password");
    } catch {
      setError("Could not open the file dialog.");
    } finally {
      setPicking(false);
    }
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!path || password.length === 0 || pending) return;
    setError(null);
    setPending(true);
    try {
      const next = await ipc.backupRestoreBegin(path, password);
      setPreview(next);
      setStep("preview");
    } catch (err) {
      setPassword("");
      setError(errorMessage(err));
    } finally {
      // The password is only needed for `backup_restore_begin`.
      setPassword("");
      setPending(false);
    }
  };

  const commit = async () => {
    if (!preview || pending) return;
    setError(null);
    setPending(true);
    try {
      const status = await ipc.backupRestoreCommit(preview.sessionId);
      qc.setQueryData(queryKeys.vaultStatus, status);
      setPreview(null);
      reset();
      onOpenChange(false);
      toast.success("Vault restored");
      void navigate({ to: "/", replace: true });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(false);
    }
  };

  const back = () => {
    setError(null);
    setPassword("");
    setStep(path ? "password" : "pick");
  };

  const fileLabel = path ? fileNameFromPath(path) : "";
  const stepIndex = pending && step === "preview" ? PHASES.done : PHASES[step];

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent
        className="sm:max-w-[420px]"
        onInteractOutside={(event) => pending && event.preventDefault()}
        onEscapeKeyDown={(event) => pending && event.preventDefault()}
      >
        <DialogHeader>
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <ShieldCheck className="size-4 text-[#E2A75A]" aria-hidden /> Restore a vault
          </div>
          <DialogTitle className="sr-only">Restore from backup</DialogTitle>
          <DialogDescription>
            Restore replaces this device's empty vault with the contents of an encrypted backup file.
          </DialogDescription>
        </DialogHeader>

        <ol className="flex items-center gap-2 text-[11px] text-faint" aria-label="Restore steps">
          {["File", "Password", "Restore"].map((label, i) => (
            <li
              key={label}
              aria-current={stepIndex === i ? "step" : undefined}
              className={stepIndex >= i ? "font-medium text-foreground" : undefined}
            >
              {i + 1}. {label}
            </li>
          ))}
        </ol>

        {step === "pick" ? (
          <div className="space-y-3">
            <p className="text-[12px] leading-relaxed text-muted-foreground">
              Select the <span className="font-mono">.coffer</span> file you exported from another device.
              Restoring is only possible before a vault is created on this device.
            </p>
            {error ? <InlineError message={error} /> : null}
            <div className="flex justify-end gap-2 pt-1">
              <DialogClose asChild>
                <Button type="button" variant="ghost">
                  Cancel
                </Button>
              </DialogClose>
              <Button type="button" onClick={() => void chooseFile()} disabled={picking}>
                {picking ? <InlineSpinner /> : <FolderOpen className="size-4" aria-hidden />}
                {picking ? "Opening…" : "Choose file…"}
              </Button>
            </div>
          </div>
        ) : step === "password" ? (
          <form onSubmit={(event) => void submit(event)} className="space-y-3" noValidate>
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-[#0F131A] px-3 py-2 text-[12px]">
              <FileArchive className="size-3.5 shrink-0 text-faint" aria-hidden />
              <span className="truncate font-mono" title={path ?? undefined}>
                {fileLabel}
              </span>
            </div>
            <div>
              <Label htmlFor="restore-backup-password">Master password for this backup</Label>
              <div className="relative mt-1.5">
                <Input
                  id="restore-backup-password"
                  type={showPassword ? "text" : "password"}
                  autoFocus
                  autoComplete="current-password"
                  value={password}
                  disabled={pending}
                  onChange={(event) => setPassword(event.target.value)}
                  className="h-10 pr-10 font-mono"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  aria-label={showPassword ? "Hide password" : "Show password"}
                  className="absolute top-1/2 right-2 -translate-y-1/2 rounded p-1.5 text-faint hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                >
                  {showPassword ? <EyeOff className="size-4" aria-hidden /> : <Eye className="size-4" aria-hidden />}
                </button>
              </div>
              <p className="mt-1.5 text-[11px] leading-relaxed text-muted-foreground">
                Use the master password that was set when this backup was created. Later changes on the other
                device do not apply to old backups.
              </p>
            </div>
            {error ? <InlineError message={error} /> : null}
            {pending ? <BackupProgress progress={progress} /> : null}
            <div className="flex justify-end gap-2 pt-1">
              <Button type="button" variant="ghost" disabled={pending} onClick={back}>
                Back
              </Button>
              <Button type="submit" disabled={pending || password.length === 0}>
                {pending ? <InlineSpinner /> : null}
                {pending ? "Checking…" : "Continue"}
              </Button>
            </div>
          </form>
        ) : (
          <div className="space-y-3">
            <div className="divide-y divide-border rounded-[10px] border border-border bg-card text-[12px]">
              <PreviewRow label="Created" value={preview ? absoluteDate(preview.createdAtMs) : "—"} />
              <PreviewRow label="Secrets" value={preview ? String(preview.itemCount) : "—"} />
              <PreviewRow label="Projects" value={preview ? String(preview.projectCount) : "—"} />
              <PreviewRow label="App version" value={preview ? preview.appVersion : "—"} />
              <PreviewRow label="File" value={fileLabel} mono />
            </div>
            {error ? <InlineError message={error} /> : null}
            {pending ? <BackupProgress progress={progress} /> : (
              <p className="text-[11px] leading-relaxed text-muted-foreground">
                Restoring replaces this device's vault. You will be taken straight into the restored vault.
              </p>
            )}
            <div className="flex justify-end gap-2 pt-1">
              <Button type="button" variant="ghost" disabled={pending} onClick={back}>
                Back
              </Button>
              <Button type="button" disabled={pending || !preview} onClick={() => void commit()}>
                {pending ? <InlineSpinner /> : null}
                {pending ? "Restoring…" : "Restore vault"}
              </Button>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function PreviewRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4 px-3 py-2">
      <span className="text-muted-foreground">{label}</span>
      <span className={`truncate ${mono ? "font-mono" : "tabular-nums"}`} title={value}>
        {value}
      </span>
    </div>
  );
}
