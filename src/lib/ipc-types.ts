/**
 * Hand-written mirror of docs/IPC_CONTRACT.md.
 *
 * Do NOT import backend-generated bindings here. Every type below must match
 * the contract exactly; the backend serialises Rust structs with
 * `#[serde(rename_all = "camelCase")]` and timestamps are Unix milliseconds.
 */

export type ItemKind = "apiKey" | "env" | "password" | "note";

export const ITEM_KINDS: readonly ItemKind[] = ["apiKey", "env", "password", "note"];

export type CategorySource = "manual" | "rule" | "import" | "default";

export type ErrorCode =
  | "locked"
  | "invalidPassword"
  | "vaultNotInitialized"
  | "alreadyInitialized"
  | "notFound"
  | "conflict"
  | "validation"
  | "io"
  | "keychainUnavailable"
  | "unsupportedFormat"
  | "internal";

/** Fixed category display strings. "Uncategorized" is the default bucket. */
export const CATEGORIES = [
  "AI / LLM",
  "Payments",
  "Cloud",
  "VCS",
  ".env",
  "Messaging",
  "Database",
  "Notes",
  "Uncategorized",
] as const;

export type Category = (typeof CATEGORIES)[number];

export interface Project {
  id: string;
  name: string;
  description: string | null;
  color: string | null;
  archivedAt: number | null;
  itemCount: number;
  createdAt: number;
  updatedAt: number;
}

export interface ProjectInput {
  name: string;
  description?: string | null;
  color?: string | null;
}

export interface ProjectUpdate {
  id: string;
  name?: string;
  description?: string | null;
  color?: string | null;
  archived?: boolean;
}

export interface ItemMeta {
  id: string;
  kind: ItemKind;
  title: string;
  username: string | null;
  url: string | null;
  projectId: string | null;
  category: string | null;
  categorySource: CategorySource;
  favorite: boolean;
  tags: string[];
  createdAt: number;
  updatedAt: number;
}

export interface ItemDetail extends ItemMeta {
  notes: string | null;
  hasSecret: boolean;
}

export interface ItemInput {
  kind: ItemKind;
  title: string;
  /** For `env` items this is JSON: `[{"key":"A","value":"B"}]`. */
  secret: string;
  username?: string | null;
  url?: string | null;
  projectId?: string | null;
  notes?: string | null;
  tags?: string[];
}

export interface ItemUpdate {
  id: string;
  kind?: ItemKind;
  title?: string;
  /** Omit to leave unchanged. */
  secret?: string;
  username?: string | null;
  url?: string | null;
  /** Omit to leave unchanged; explicit `null` detaches the project. */
  projectId?: string | null;
  notes?: string | null;
  tags?: string[];
}

export interface ItemFilter {
  projectId?: string | null;
  unassignedOnly: boolean;
  kind?: ItemKind | null;
  favoriteOnly?: boolean;
}

export interface SidebarCounts {
  all: number;
  unassigned: number;
  projects: { projectId: string; count: number }[];
}

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
  autoLockSecs: number;
  counts: SidebarCounts;
}

/**
 * Biometric unlock state. `supported` is false on platforms without a
 * biometric keychain (everything except macOS), so clients hide the feature.
 */
export interface KeychainStatus {
  supported: boolean;
  available: boolean;
  enabled: boolean;
}

export interface Settings {
  /** 60..=3600 */
  autoLockSecs: number;
  /** 5..=300 */
  clipboardClearSecs: number;
  deepSearch: boolean;
}

export interface Classification {
  category: string;
  tags: string[];
  /** 0..1 */
  confidence: number;
  reason: string | null;
}

export interface SearchHit {
  item: ItemMeta;
  score: number;
  matchedField: "title" | "username" | "url" | "tags" | "notes" | "project" | string;
  /** Never contains secret values. */
  snippet: string | null;
}

export interface RevealedSecret {
  value: string;
  hideAfterSecs: number;
}

export interface CopyResult {
  clearAfterSecs: number;
}

export interface AuditEntry {
  id: number;
  action: "create" | "update" | "delete" | "reveal" | "copy" | "projectChange" | string;
  itemId: string | null;
  createdAt: number;
}

export interface AppError {
  code: ErrorCode;
  /** Redacted; never contains secrets. */
  message: string;
}

/* ---------------------------------------------------------------- events */

export const EVENTS = {
  locked: "vault://locked",
  lockWarning: "vault://lock-warning",
  clipboardCleared: "clipboard://cleared",
  backupExportProgress: "backup://export-progress",
  backupRestoreProgress: "backup://restore-progress",
} as const;

export interface LockWarningPayload {
  seconds: number;
}

/* ------------------------------------------------------------- backup */

/** Result of `backup_export`. Never contains secret material. */
export interface BackupExportResult {
  path: string;
  bytes: number;
  itemCount: number;
  projectCount: number;
  createdAtMs: number;
}

/** Preview of a validated `.coffer` file; `sessionId` is passed to commit/cancel. */
export interface BackupPreview {
  sessionId: string;
  sourceVaultId: string;
  createdAtMs: number;
  appVersion: string;
  schemaVersion: number;
  itemCount: number;
  projectCount: number;
  tagCount: number;
}

/** Payload of `backup://export-progress` / `backup://restore-progress`. */
export interface BackupProgressPayload {
  /** e.g. verify | snapshot | encrypt | decrypt | commit | done */
  phase: string;
  done: number;
  total: number;
}

/* ------------------------------------------------------- default values */

export const DEFAULTS = {
  autoLockSecs: 300,
  clipboardClearSecs: 30,
  deepSearch: false,
  hideAfterSecs: 15,
} as const;

export const EMPTY_COUNTS: SidebarCounts = { all: 0, unassigned: 0, projects: [] };
