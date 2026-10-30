import { useState } from "react";
import { Eye, EyeOff, FileArchive, Fingerprint, Lock } from "lucide-react";
import { toast } from "sonner";
import { useIpc } from "@/app/ipc-context";
import { useKeychainStatus } from "@/app/queries";
import { fileNameFromPath, pickBackupSavePath } from "@/lib/file-dialog";
import { errorMessage } from "@/lib/ipc-errors";
import { EVENTS, type BackupExportResult } from "@/lib/ipc-types";
import { BackupProgress, useBackupProgress } from "@/components/backup/backup-progress";
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

/** `coffer-backup-20261029-101500.coffer` in local time. */
export function defaultBackupFileName(now = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const date = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`;
  const time = `${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  return `coffer-backup-${date}-${time}.coffer`;
}

type Step = "pick" | "password";

/**
 * Export flow (IMPORT_EXPORT_DESIGN.md §1.2): choose the destination first,
 * then re-enter the master password. The password only lives in this
 * component's state and is cleared on close and after every attempt.
 */
export function ExportBackupDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const ipc = useIpc();
  const keychainQuery = useKeychainStatus(open);
  const canUseTouchId = Boolean(
    keychainQuery.data?.supported && keychainQuery.data.available && keychainQuery.data.enabled,
  );
  const [step, setStep] = useState<Step>("pick");
  const [destination, setDestination] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [picking, setPicking] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const progress = useBackupProgress(EVENTS.backupExportProgress, pending);

  const reset = () => {
    setStep("pick");
    setDestination(null);
    setPassword("");
    setShowPassword(false);
    setPending(false);
    setError(null);
  };

  const changeOpen = (next: boolean) => {
    if (pending) return;
    onOpenChange(next);
  };

  const chooseDestination = async () => {
    setError(null);
    setPicking(true);
    try {
      const path = await pickBackupSavePath(defaultBackupFileName());
      if (!path) return;
      setDestination(path);
      setStep("password");
    } catch {
      setError("Could not open the save dialog.");
    } finally {
      setPicking(false);
    }
  };

  const runExport = async (invoke: () => Promise<BackupExportResult>, clearPassword: boolean) => {
    if (!destination || pending) return;
    setError(null);
    setPending(true);
    try {
      const result = await invoke();
      toast.success("Backup exported", { description: fileNameFromPath(result.path) });
      reset();
      onOpenChange(false);
    } catch (err) {
      if (clearPassword) setPassword("");
      setError(errorMessage(err));
    } finally {
      setPending(false);
    }
  };

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!destination || password.length === 0 || pending) return;
    void runExport(() => ipc.backupExport(destination, password), true);
  };

  const submitTouchId = () => {
    if (!destination || pending) return;
    void runExport(() => ipc.backupExportKeychain(destination), false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        // A running export owns the dialog; closing mid-flight would drop the
        // progress state while the backend keeps writing.
        if (pending) return;
        if (!next) reset();
        changeOpen(next);
      }}
    >
      <DialogContent
        className="sm:max-w-[420px]"
        onInteractOutside={(event) => pending && event.preventDefault()}
        onEscapeKeyDown={(event) => pending && event.preventDefault()}
      >
        <DialogHeader>
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <FileArchive className="size-4 text-[#E2A75A]" aria-hidden /> Export encrypted backup
          </div>
          <DialogTitle className="sr-only">Export encrypted backup</DialogTitle>
          <DialogDescription>
            The file contains every secret in this vault, encrypted with your master password.
          </DialogDescription>
        </DialogHeader>

        {step === "pick" ? (
          <div className="space-y-3">
            <p className="text-[12px] leading-relaxed text-muted-foreground">
              Choose where to save the <span className="font-mono">.coffer</span> file. Keep it somewhere you
              trust — anyone with the file and your master password can open it.
            </p>
            {error ? <InlineError message={error} /> : null}
            <div className="flex justify-end gap-2 pt-1">
              <DialogClose asChild>
                <Button type="button" variant="ghost">
                  Cancel
                </Button>
              </DialogClose>
              <Button type="button" onClick={() => void chooseDestination()} disabled={picking}>
                {picking ? <InlineSpinner /> : null}
                {picking ? "Opening…" : "Choose location…"}
              </Button>
            </div>
          </div>
        ) : (
          <form onSubmit={(event) => void submit(event)} className="space-y-3" noValidate>
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-[#0F131A] px-3 py-2 text-[12px]">
              <FileArchive className="size-3.5 shrink-0 text-faint" aria-hidden />
              <span className="truncate font-mono" title={destination ?? undefined}>
                {destination ? fileNameFromPath(destination) : ""}
              </span>
            </div>
            {canUseTouchId ? (
              <div className="space-y-3">
                <Button
                  type="button"
                  className="w-full gap-2"
                  onClick={submitTouchId}
                  disabled={pending}
                >
                  {pending ? <InlineSpinner /> : <Fingerprint className="size-4" aria-hidden />}
                  {pending ? "Exporting…" : "Use Touch ID"}
                </Button>
                <div className="flex items-center gap-2 text-[11px] text-faint">
                  <span className="h-px flex-1 bg-border" aria-hidden />
                  or use your master password
                  <span className="h-px flex-1 bg-border" aria-hidden />
                </div>
              </div>
            ) : null}
            <div>
              <Label htmlFor="export-backup-password">Master password</Label>
              <div className="relative mt-1.5">
                <Input
                  id="export-backup-password"
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
              <p className="mt-1.5 flex items-start gap-1.5 text-[11px] leading-relaxed text-muted-foreground">
                <Lock className="mt-0.5 size-3 shrink-0" aria-hidden />
                Confirm your master password. It is checked once and never stored.
              </p>
            </div>
            {error ? <InlineError message={error} /> : null}
            {pending ? <BackupProgress progress={progress} /> : null}
            <div className="flex justify-end gap-2 pt-1">
              <Button type="button" variant="ghost" disabled={pending} onClick={() => setStep("pick")}>
                Back
              </Button>
              <Button
                type="submit"
                variant={canUseTouchId ? "secondary" : "default"}
                disabled={pending || password.length === 0}
              >
                {pending ? <InlineSpinner /> : null}
                {pending ? "Exporting…" : "Export"}
              </Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

export function InlineError({ message }: { message: string }) {
  return (
    <p role="alert" className="rounded-md border border-[#3A1F23] bg-[#261214] px-3 py-2 text-[12px] text-[#F6DCDF]">
      {message}
    </p>
  );
}
