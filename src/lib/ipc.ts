import type {
  AuditEntry,
  BackupExportResult,
  BackupPreview,
  Classification,
  CopyResult,
  ItemDetail,
  ItemFilter,
  ItemInput,
  ItemMeta,
  ItemUpdate,
  KeychainStatus,
  LockWarningPayload,
  Project,
  ProjectInput,
  ProjectUpdate,
  RevealedSecret,
  SearchHit,
  Settings,
  SidebarCounts,
  VaultStatus,
} from "@/lib/ipc-types";
import { EVENTS } from "@/lib/ipc-types";

type UnlistenFn = () => void;

export type IpcEventName = (typeof EVENTS)[keyof typeof EVENTS];

/**
 * Typed facade over the Tauri IPC surface. Command names are snake_case and
 * argument keys are camelCase, exactly as defined in docs/IPC_CONTRACT.md §3.
 *
 * Security invariant: only `itemRevealSecret` returns a secret value. Nothing
 * else may leak secret material to the renderer.
 */
export interface IpcClient {
  /** Any state. */
  vaultStatus(): Promise<VaultStatus>;
  vaultCreate(password: string): Promise<VaultStatus>;
  vaultUnlock(password: string): Promise<VaultStatus>;
  vaultLock(): Promise<void>;
  vaultTouchActivity(): Promise<void>;
  vaultChangePassword(oldPassword: string, newPassword: string): Promise<void>;
  /** Any state. Biometric unlock is macOS-only; other platforms report supported: false. */
  vaultKeychainStatus(): Promise<KeychainStatus>;
  /** Requires unlock and the master password. */
  vaultKeychainEnable(password: string): Promise<KeychainStatus>;
  vaultKeychainDisable(): Promise<KeychainStatus>;
  /** Restores the master key after the system prompts for biometry. */
  vaultUnlockKeychain(): Promise<VaultStatus>;

  /**
   * Writes the whole vault into one `.coffer` file protected by the master
   * password. Requires unlock; the password is re-checked by the backend.
   * Progress arrives on `backup://export-progress`.
   */
  backupExport(destination: string, password: string): Promise<BackupExportResult>;
  /**
   * Same as `backupExport`, authenticated by Touch ID / the cached master key.
   * Requires biometric unlock to be enabled; the password path remains the
   * fallback.
   */
  backupExportKeychain(destination: string): Promise<BackupExportResult>;
  /**
   * Validates a `.coffer` file and opens a restore session. Only allowed
   * before a vault exists. Progress arrives on `backup://restore-progress`.
   */
  backupRestoreBegin(path: string, password: string): Promise<BackupPreview>;
  /** Commits a restore session; the restored vault comes back unlocked. */
  backupRestoreCommit(sessionId: string): Promise<VaultStatus>;
  /** Drops a restore session and its temporary copy. */
  backupRestoreCancel(sessionId: string): Promise<void>;

  /** Requires unlock. */
  settingsGet(): Promise<Settings>;
  settingsSet(settings: Settings): Promise<Settings>;
  sidebarCounts(): Promise<SidebarCounts>;

  projectsList(includeArchived: boolean): Promise<Project[]>;
  projectCreate(input: ProjectInput): Promise<Project>;
  projectUpdate(input: ProjectUpdate): Promise<Project>;
  projectDelete(id: string): Promise<void>;

  itemsList(filter: ItemFilter): Promise<ItemMeta[]>;
  itemGet(id: string): Promise<ItemDetail>;
  itemCreate(input: ItemInput): Promise<ItemMeta>;
  itemUpdate(input: ItemUpdate): Promise<ItemMeta>;
  itemDelete(id: string): Promise<void>;
  itemSetProject(id: string, projectId: string | null): Promise<ItemMeta>;
  itemSetCategory(id: string, category: string | null): Promise<ItemMeta>;
  itemToggleFavorite(id: string): Promise<ItemMeta>;
  itemRevealSecret(id: string): Promise<RevealedSecret>;
  /** Copy happens inside Rust; the value never crosses IPC. */
  copySecret(id: string): Promise<CopyResult>;

  search(query: string, filter: { projectId?: string | null; unassignedOnly: boolean; deep: boolean }): Promise<SearchHit[]>;
  classifyPreview(input: { title: string; value: string; url?: string | null }): Promise<Classification>;
  auditList(itemId: string): Promise<AuditEntry[]>;

  /** Event subscription. Returns an unsubscribe function. */
  on(event: IpcEventName, handler: (payload: unknown) => void): Promise<UnlistenFn>;
}

export type LockedListener = () => void;
export type LockWarningListener = (payload: LockWarningPayload) => void;
export type ClipboardClearedListener = () => void;
