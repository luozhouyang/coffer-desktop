import { useEffect, useMemo, useRef, useState } from "react";
import { FileText, Loader2, ShieldCheck, Upload } from "lucide-react";
import { toast } from "sonner";
import { useIpc } from "@/app/ipc-context";
import { useInvalidateVaultData, useProjects } from "@/app/queries";
import { uiStore, useUiStore } from "@/app/ui-store";
import { errorMessage } from "@/lib/ipc-errors";
import type { ImportPreview } from "@/lib/ipc-types";
import { CategoryBadge } from "@/components/common/category-badge";
import { InlineError } from "@/components/backup/export-backup-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

const NO_PROJECT = "__none__";

/** Hosted in `__root`; opened via `uiStore.openImport`. */
export function ImportDialog() {
  const request = useUiStore((state) => state.importDialog);
  if (!request) return null;
  return <ImportDialogBody key={request.defaultProjectId ?? "none"} defaultProjectId={request.defaultProjectId} />;
}

function ImportDialogBody({ defaultProjectId }: { defaultProjectId: string | null }) {
  const ipc = useIpc();
  const invalidate = useInvalidateVaultData();
  const { data: projects } = useProjects(true);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [projectId, setProjectId] = useState<string>(NO_PROJECT);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [reading, setReading] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const sessionRef = useRef<string | null>(null);
  const closedRef = useRef(false);

  // Closing or locking the vault drops the backend session (and its plaintext).
  useEffect(() => {
    closedRef.current = false;
    return () => {
      closedRef.current = true;
      const sessionId = sessionRef.current;
      if (sessionId) void ipc.importCancel(sessionId).catch(() => undefined);
      sessionRef.current = null;
    };
  }, [ipc]);

  const projectOptions = projects ?? [];
  const projectValue = useMemo(() => {
    if (projectId === NO_PROJECT) return NO_PROJECT;
    return projectOptions.some((project) => project.id === projectId) ? projectId : NO_PROJECT;
  }, [projectOptions, projectId]);
  const targetProjectId = projectValue === NO_PROJECT ? null : projectValue;

  const statusFor = (entry: ImportPreview["entries"][number]) => {
    const conflict = entry.conflicts.some((item) => (item.projectId ?? null) === targetProjectId);
    return conflict ? "update" : "new";
  };

  const selectedEntries = preview?.entries.filter((entry) => selected.has(entry.index)) ?? [];
  const newCount = selectedEntries.filter((entry) => statusFor(entry) === "new").length;
  const updateCount = selectedEntries.filter((entry) => statusFor(entry) === "update").length;
  const allSelected = Boolean(preview?.entries.length) && selected.size === preview?.entries.length;

  const cancelSession = () => {
    const sessionId = sessionRef.current;
    if (sessionId) void ipc.importCancel(sessionId).catch(() => undefined);
    sessionRef.current = null;
  };

  const close = () => {
    if (pending) return;
    cancelSession();
    uiStore.closeImport();
  };

  const readFile = async (file: File) => {
    setError(null);
    setReading(true);
    try {
      const text = await file.text();
      const next = await ipc.importEnv(text, file.name);
      if (closedRef.current) {
        // The dialog was closed while the file was parsing.
        void ipc.importCancel(next.sessionId).catch(() => undefined);
        return;
      }
      cancelSession();
      sessionRef.current = next.sessionId;
      setPreview(next);
      setFileName(file.name);
      setSelected(new Set(next.entries.map((entry) => entry.index)));
      setProjectId(next.suggestedProjectId ?? defaultProjectId ?? NO_PROJECT);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setReading(false);
    }
  };

  const onPick = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (file) void readFile(file);
  };

  const onDrop = (event: React.DragEvent) => {
    event.preventDefault();
    if (reading || pending) return;
    const file = event.dataTransfer.files?.[0];
    if (file) void readFile(file);
  };

  const chooseAnother = () => {
    if (pending) return;
    cancelSession();
    setPreview(null);
    setFileName(null);
    setSelected(new Set());
    setError(null);
  };

  const toggleAll = (checked: boolean) => {
    if (!preview) return;
    setSelected(checked ? new Set(preview.entries.map((entry) => entry.index)) : new Set());
  };

  const toggle = (index: number, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(index);
      else next.delete(index);
      return next;
    });
  };

  const commit = async () => {
    if (!preview || pending || selected.size === 0) return;
    setError(null);
    setPending(true);
    try {
      const result = await ipc.importCommit({
        sessionId: preview.sessionId,
        projectId: targetProjectId,
        selected: [...selected].sort((a, b) => a - b),
      });
      sessionRef.current = null;
      invalidate();
      toast.success(
        `Imported ${result.created + result.updated} secret${result.created + result.updated === 1 ? "" : "s"}`,
        {
          description: `${result.created} new · ${result.updated} updated`,
        },
      );
      uiStore.closeImport();
    } catch (err) {
      // The backend consumes the session on commit, even when it fails.
      sessionRef.current = null;
      setPreview(null);
      setFileName(null);
      setSelected(new Set());
      setError(errorMessage(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && close()}>
      <DialogContent
        className="sm:max-w-[720px]"
        onInteractOutside={(event) => pending && event.preventDefault()}
        onEscapeKeyDown={(event) => pending && event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Upload className="size-4 text-[#E2A75A]" aria-hidden /> Import .env
          </DialogTitle>
          <DialogDescription>
            Parse an environment file into secrets. Values stay masked here and are written straight
            into the encrypted vault.
          </DialogDescription>
        </DialogHeader>

        <input
          ref={inputRef}
          type="file"
          className="hidden"
          onChange={onPick}
          data-testid="import-file-input"
          aria-label="Choose a .env file"
        />

        {!preview ? (
          <div className="space-y-3">
            <button
              type="button"
              onClick={() => inputRef.current?.click()}
              onDragOver={(event) => event.preventDefault()}
              onDrop={onDrop}
              disabled={reading}
              className="flex w-full flex-col items-center gap-2 rounded-[12px] border border-dashed border-[#2B3240] bg-[#0F131A] px-6 py-10 text-center transition-colors hover:border-[#E2A75A]/60 hover:bg-[#141922] focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none disabled:opacity-60"
            >
              {reading ? (
                <Loader2 className="size-5 animate-spin text-[#E2A75A]" aria-hidden />
              ) : (
                <FileText className="size-5 text-[#E2A75A]" aria-hidden />
              )}
              <span className="text-sm font-medium">{reading ? "Reading file…" : "Choose a .env file"}</span>
              <span className="text-[12px] text-muted-foreground">or drop it here</span>
            </button>
            {error ? <InlineError message={error} /> : null}
            <p className="flex items-start gap-1.5 text-[11px] leading-relaxed text-faint">
              <ShieldCheck className="mt-0.5 size-3.5 shrink-0" aria-hidden />
              Parsing happens on this device. Comments, <span className="font-mono">export</span>, quotes,
              and inline comments are supported; duplicate keys keep the last value.
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="flex flex-wrap items-end justify-between gap-3">
              <div className="min-w-0">
                <p className="truncate text-[13px] font-medium">{fileName}</p>
                <p className="text-[11px] text-faint">
                  {preview.entries.length} variable{preview.entries.length === 1 ? "" : "s"}
                  {preview.skippedLines > 0 ? ` · ${preview.skippedLines} line${preview.skippedLines === 1 ? "" : "s"} skipped` : ""}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-[11px] text-faint">Project</span>
                <Select value={projectValue} onValueChange={setProjectId}>
                  <SelectTrigger size="sm" aria-label="Import into project" className="h-8 w-[190px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent align="end">
                    <SelectItem value={NO_PROJECT}>No project</SelectItem>
                    {projectOptions.map((project) => (
                      <SelectItem key={project.id} value={project.id}>
                        {project.archivedAt ? `${project.name} (archived)` : project.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>

            <div className="max-h-[46vh] overflow-y-auto rounded-[10px] border border-border">
              <table className="w-full border-collapse text-left text-[12px]">
                <thead className="sticky top-0 z-10 bg-card">
                  <tr className="border-b border-border text-[10px] uppercase tracking-[0.08em] text-faint">
                    <th className="w-9 px-2 py-2">
                      <Checkbox
                        checked={allSelected}
                        onCheckedChange={(checked) => toggleAll(checked === true)}
                        aria-label="Select all variables"
                      />
                    </th>
                    <th className="px-2 py-2 font-semibold">Key</th>
                    <th className="px-2 py-2 font-semibold">Value</th>
                    <th className="px-2 py-2 font-semibold">Detected</th>
                    <th className="px-2 py-2 font-semibold">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.entries.map((entry) => {
                    const checked = selected.has(entry.index);
                    const status = statusFor(entry);
                    return (
                      <tr key={entry.index} className="border-b border-border/60 last:border-0">
                        <td className="px-2 py-2 align-middle">
                          <Checkbox
                            checked={checked}
                            onCheckedChange={(value) => toggle(entry.index, value === true)}
                            aria-label={`Import ${entry.key}`}
                          />
                        </td>
                        <td className="max-w-[220px] truncate px-2 py-2 font-mono text-[11.5px]" title={entry.key}>
                          {entry.key}
                        </td>
                        <td className="px-2 py-2 font-mono text-[11.5px] text-faint">{entry.maskedValue}</td>
                        <td className="px-2 py-2">
                          <CategoryBadge category={entry.category} />
                        </td>
                        <td className="px-2 py-2">
                          <span className="flex items-center gap-1.5">
                            <span
                              className={
                                status === "update"
                                  ? "rounded-[5px] bg-[#2A2114] px-1.5 py-0.5 text-[10px] font-semibold text-[#E2A75A]"
                                  : "rounded-[5px] bg-[#14261C] px-1.5 py-0.5 text-[10px] font-semibold text-[#7FD3A4]"
                              }
                            >
                              {status === "update" ? "Update" : "New"}
                            </span>
                            {entry.duplicate ? (
                              <span
                                className="rounded-[5px] bg-[#2B3240] px-1.5 py-0.5 text-[10px] font-semibold text-muted-foreground"
                                title="This key appears more than once; the last value is used."
                              >
                                Repeated
                              </span>
                            ) : null}
                          </span>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>

            {error ? <InlineError message={error} /> : null}
            <p className="text-[11px] text-faint">
              Existing items in the selected project are updated; manual categories are preserved.
            </p>
          </div>
        )}

        <DialogFooter className="gap-2 sm:justify-between">
          <div>
            {preview ? (
              <Button type="button" variant="ghost" onClick={chooseAnother} disabled={pending}>
                Choose another file
              </Button>
            ) : null}
          </div>
          <div className="flex items-center gap-2">
            <Button type="button" variant="ghost" onClick={close} disabled={pending}>
              Cancel
            </Button>
            {preview ? (
              <Button
                type="button"
                onClick={() => void commit()}
                disabled={pending || selected.size === 0}
                className="gap-1.5"
              >
                {pending ? <Loader2 className="size-4 animate-spin" aria-hidden /> : null}
                Import {selected.size} secret{selected.size === 1 ? "" : "s"}
              </Button>
            ) : null}
          </div>
        </DialogFooter>

        {preview ? (
          <p className="sr-only" aria-live="polite">
            {newCount} new, {updateCount} existing will be updated.
          </p>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
