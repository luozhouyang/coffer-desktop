import type { Update } from "@tauri-apps/plugin-updater";
import { isTauriRuntime } from "@/lib/tauri-ipc";

/**
 * Thin wrapper around the Tauri updater and process plugins. This is the only
 * module that imports them, and every function is a no-op outside Tauri so
 * browser dev and vitest never reach the native side. Imports are dynamic so
 * the plugins are only loaded when actually used inside the webview.
 */

export type { Update };

/** Bytes downloaded so far (and the total, when the server reported it). */
export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

/** The running app version, or null outside Tauri. */
export async function getAppVersion(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

/** Asks the updater endpoint for a newer release. Null outside Tauri or when up to date. */
export async function checkForUpdate(): Promise<Update | null> {
  if (!isTauriRuntime()) return null;
  const { check } = await import("@tauri-apps/plugin-updater");
  return check();
}

/** Downloads and installs `update`, reporting byte progress through `onProgress`. */
export async function installUpdate(
  update: Update,
  onProgress?: (progress: DownloadProgress) => void,
): Promise<void> {
  let total: number | null = null;
  let downloaded = 0;
  const report = () => onProgress?.({ downloaded, total });
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? null;
        downloaded = 0;
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        break;
      case "Finished":
        total = downloaded;
        break;
    }
    report();
  });
}

/** Restarts the app to finish applying an update. No-op outside Tauri. */
export async function relaunchApp(): Promise<void> {
  if (!isTauriRuntime()) return;
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}
