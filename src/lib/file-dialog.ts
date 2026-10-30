import { isTauriRuntime } from "@/lib/tauri-ipc";

/**
 * Native save/open dialogs for `.coffer` files, backed by
 * `@tauri-apps/plugin-dialog`. Outside Tauri (browser dev, tests) the plugin
 * is never imported and deterministic `/tmp` paths are returned instead, so
 * the full export → reset → restore flow works in a plain browser.
 */

const BACKUP_FILTER = { name: "Coffer backup", extensions: ["coffer"] };

/** `/Users/me/Backups/` + `coffer-backup-20261029-101500.coffer` → `coffer-backup-20261029-101500.coffer` */
export function fileNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** Asks where to write the backup. Resolves `null` when the user cancels. */
export async function pickBackupSavePath(defaultName: string): Promise<string | null> {
  if (!isTauriRuntime()) return `/tmp/${defaultName}`;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const picked = await save({
    title: "Export encrypted backup",
    defaultPath: defaultName,
    filters: [BACKUP_FILTER],
  });
  return picked ?? null;
}

/** Asks for an existing `.coffer` file. Resolves `null` when the user cancels. */
export async function pickBackupOpenPath(): Promise<string | null> {
  if (!isTauriRuntime()) return "/tmp/coffer-backup.coffer";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({
    title: "Restore from backup",
    multiple: false,
    directory: false,
    filters: [BACKUP_FILTER],
  });
  return typeof picked === "string" ? picked : null;
}
