import type { IpcClient } from "@/lib/ipc";
import { createMockIpc } from "@/lib/mock-ipc";
import { createTauriIpc, isTauriRuntime } from "@/lib/tauri-ipc";

let active: IpcClient | null = null;

/**
 * Returns the process-wide IPC client: the real Tauri client inside the
 * webview, an in-memory mock everywhere else (browser dev and tests).
 * Components never call this directly — they use `useIpc()` so tests can
 * inject deterministic mocks through `IpcProvider`.
 */
export function resolveIpc(): IpcClient {
  if (!active) active = isTauriRuntime() ? createTauriIpc() : createMockIpc();
  return active;
}

/** Test hook: replace the auto-resolved client (pass `null` to restore). */
export function setDefaultIpc(client: IpcClient | null) {
  active = client;
}

export function defaultIpc(): IpcClient {
  return resolveIpc();
}

export { isTauriRuntime };
