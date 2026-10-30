import type { IpcClient, IpcEventName } from "@/lib/ipc";

/**
 * True when running inside the Tauri webview. Tauri 2 injects
 * `window.__TAURI_INTERNALS__`; a plain browser or jsdom does not have it.
 */
export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;
  return "__TAURI_INTERNALS__" in window;
}

/**
 * Real IPC backed by `@tauri-apps/api`. `@tauri-apps/api` reads
 * `window.__TAURI_INTERNALS__` at call time, so importing this module in a
 * browser/test is harmless — only `invoke` calls fail. The app never calls
 * this implementation outside Tauri (see `resolveIpc`).
 */
export function createTauriIpc(): IpcClient {
  const call = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  };

  return {
    vaultStatus: () => call("vault_status"),
    vaultCreate: (password) => call("vault_create", { password }),
    vaultUnlock: (password) => call("vault_unlock", { password }),
    vaultLock: () => call("vault_lock"),
    vaultTouchActivity: () => call("vault_touch_activity"),
    vaultChangePassword: (oldPassword, newPassword) =>
      call("vault_change_password", { oldPassword, newPassword }),
    vaultKeychainStatus: () => call("vault_keychain_status"),
    vaultKeychainEnable: (password) => call("vault_keychain_enable", { password }),
    vaultKeychainDisable: () => call("vault_keychain_disable"),
    vaultUnlockKeychain: () => call("vault_unlock_keychain"),

    backupExport: (destination, password) => call("backup_export", { destination, password }),
    backupExportKeychain: (destination) => call("backup_export_keychain", { destination }),
    backupRestoreBegin: (path, password) => call("backup_restore_begin", { path, password }),
    backupRestoreCommit: (sessionId) => call("backup_restore_commit", { sessionId }),
    backupRestoreCancel: (sessionId) => call("backup_restore_cancel", { sessionId }),

    settingsGet: () => call("settings_get"),
    settingsSet: (settings) => call("settings_set", { settings }),
    sidebarCounts: () => call("sidebar_counts"),

    projectsList: (includeArchived) => call("projects_list", { includeArchived }),
    projectCreate: (input) => call("project_create", { input }),
    projectUpdate: (input) => call("project_update", { input }),
    projectDelete: (id) => call("project_delete", { id }),

    itemsList: (filter) => call("items_list", { filter }),
    itemGet: (id) => call("item_get", { id }),
    itemCreate: (input) => call("item_create", { input }),
    itemUpdate: (input) => call("item_update", { input }),
    itemDelete: (id) => call("item_delete", { id }),
    itemSetProject: (id, projectId) => call("item_set_project", { id, projectId }),
    itemSetCategory: (id, category) => call("item_set_category", { id, category }),
    itemToggleFavorite: (id) => call("item_toggle_favorite", { id }),
    itemRevealSecret: (id) => call("item_reveal_secret", { id }),
    copySecret: (id) => call("copy_secret", { id }),

    search: (query, filter) => call("search", { query, ...filter }),
    classifyPreview: (input) =>
      call("classify_preview", {
        title: input.title,
        value: input.value,
        url: input.url ?? null,
      }),
    auditList: (itemId) => call("audit_list", { itemId }),

    on: async (event: IpcEventName, handler: (payload: unknown) => void) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen(event, (e) => handler(e.payload));
    },
  };
}
