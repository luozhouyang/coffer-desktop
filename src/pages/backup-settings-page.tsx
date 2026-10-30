import { useState } from "react";
import { Archive, TriangleAlert } from "lucide-react";
import { NoRecoveryNotice } from "@/components/common/no-recovery-notice";
import { ExportBackupDialog } from "@/components/backup/export-backup-dialog";
import { Button } from "@/components/ui/button";

/**
 * Settings › Backup (IMPORT_EXPORT_DESIGN.md §8.2). Export only: restoring is
 * a first-run action on the Setup page.
 */
export function BackupSettingsPage() {
  const [exportOpen, setExportOpen] = useState(false);

  return (
    <div>
      <header>
        <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-[0.12em] text-faint">
          <Archive className="size-3.5 text-[#E2A75A]" aria-hidden /> Backup
        </div>
        <h1 className="mt-2 text-[24px] font-semibold tracking-[-0.015em]">Backup</h1>
        <p className="mt-1 text-[13px] text-muted-foreground">
          Save your whole vault as a single encrypted file you can restore on another device.
        </p>
      </header>

      <section className="mt-8 space-y-4" aria-labelledby="backup-heading">
        <h2 id="backup-heading" className="text-[13px] font-semibold">
          Encrypted backup
        </h2>
        <div className="rounded-[14px] border border-border bg-card p-4">
          <div className="flex items-center gap-2 text-[13px] font-semibold">
            <Archive className="size-4 text-[#E2A75A]" aria-hidden /> Export a{" "}
            <span className="font-mono font-normal">.coffer</span> file
          </div>
          <p className="mt-1.5 max-w-prose text-[12px] leading-relaxed text-muted-foreground">
            The backup contains every secret in this vault — items, projects, tags, and history — inside one
            encrypted file. It is protected by your master password, so you will be asked to confirm it.
          </p>
          <Button variant="secondary" size="sm" className="mt-3" onClick={() => setExportOpen(true)}>
            Export encrypted backup…
          </Button>
        </div>
      </section>

      <section className="mt-8" aria-labelledby="backup-warning-heading">
        <h2 id="backup-warning-heading" className="sr-only">
          Keep your backup safe
        </h2>
        <div className="flex gap-2.5 rounded-[10px] border border-[#4A3720] bg-[#2A2114]/60 px-3 py-2.5 text-[12px] leading-relaxed text-warning">
          <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
          <p>
            <span className="font-semibold">Treat the backup like your master password.</span> There is no
            recovery: if you forget the master password, the backup cannot be opened. After you change your
            master password, older backups still require the password they were created with.
          </p>
        </div>
      </section>

      <NoRecoveryNotice className="mt-4" />

      <ExportBackupDialog open={exportOpen} onOpenChange={setExportOpen} />
    </div>
  );
}
