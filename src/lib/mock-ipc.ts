import type { IpcClient, IpcEventName } from "@/lib/ipc";
import { IpcError } from "@/lib/ipc-errors";
import {
  DEFAULTS,
  EVENTS,
  type AuditEntry,
  type BackupExportResult,
  type BackupPreview,
  type BackupProgressPayload,
  type ItemDetail,
  type ItemFilter,
  type ItemInput,
  type ItemMeta,
  type ItemUpdate,
  type KeychainStatus,
  type Project,
  type ProjectInput,
  type ProjectUpdate,
  type Settings,
  type SidebarCounts,
  type VaultStatus,
} from "@/lib/ipc-types";

/**
 * In-memory mock of the backend contract, used automatically when running in
 * a plain browser (`pnpm dev` outside Tauri) and in tests.
 *
 * Secrets are kept in a private map that only `itemRevealSecret` can read; by
 * construction every other response is metadata-only, matching the backend
 * invariants in docs/IPC_CONTRACT.md §6.
 */

const MASTER_PASSWORD = "coffer";
const MOCK_VAULT_ID = "0199a0c0-0000-7000-8000-000000000001";
const MOCK_APP_VERSION = "0.1.0";
const MOCK_SCHEMA_VERSION = 1;

const copyProject = (p: Project): Project => ({ ...p });
const copyItem = (i: SeedItem & { id: string }): SeedItem & { id: string } => ({
  ...i,
  tags: [...(i.tags ?? [])],
});

export interface MockIpc extends IpcClient {
  /** Test helpers (not part of the production IPC surface). */
  __state: {
    readonly initialized: boolean;
    readonly unlocked: boolean;
    readonly settings: Settings;
    readonly keychain: KeychainStatus;
    deletedProjects: string[];
    copied: string[];
    /** `.coffer` archive produced by the last export (or injected via `__seedBackup`). */
    readonly backupArchive: MockBackupArchive | null;
    readonly restoreSessions: number;
  };
  /** Event bus: emits `vault://locked`, `vault://lock-warning`, … to listeners. */
  __emit(event: IpcEventName, payload?: unknown): void;
  /** Simulates a vault before it has been created (first-run flow). */
  __resetToFirstRun(): void;
  /** Overrides the master password the mock accepts. */
  __setPassword(password: string): void;
  /** Fires for `vault://locked` even if the event was emitted before mounting. */
  __onLocked(fn: () => void): () => void;
  /** Puts a saved archive in place without an export (e.g. "restored from another Mac"). */
  __seedBackup(archive: Partial<MockBackupArchive>): void;
}

interface MockBackupSnapshot {
  projects: Project[];
  items: Map<string, SeedItem & { id: string }>;
  secrets: Map<string, string>;
}

/** The mock's in-memory stand-in for a `.coffer` file. */
export interface MockBackupArchive {
  password: string;
  vaultId: string;
  createdAtMs: number;
  appVersion: string;
  schemaVersion: number;
  itemCount: number;
  projectCount: number;
  tagCount: number;
  snapshot: MockBackupSnapshot | null;
}

let seq = 0;
const nextId = (prefix: string) => `${prefix}-${(++seq).toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

const NOW = Date.now();
const minutesAgo = (m: number) => NOW - m * 60_000;
const daysAgo = (d: number) => NOW - d * 86_400_000;

interface SeedItem extends ItemInput {
  category: string | null;
  categorySource: ItemMeta["categorySource"];
  favorite?: boolean;
  createdAt: number;
  updatedAt: number;
}

const SEED_PROJECTS: Omit<Project, "itemCount">[] = [
  {
    id: "p-cofferdash",
    name: "Coffer dashboard",
    description: "Internal ops console",
    color: "#E2A75A",
    archivedAt: null,
    createdAt: daysAgo(40),
    updatedAt: daysAgo(2),
  },
  {
    id: "p-orbit",
    name: "Orbit API",
    description: "Public API + SDK",
    color: "#8FB4FF",
    archivedAt: null,
    createdAt: daysAgo(30),
    updatedAt: daysAgo(5),
  },
  {
    id: "p-lab",
    name: "Lab experiments",
    description: null,
    color: "#7FD3A4",
    archivedAt: null,
    createdAt: daysAgo(21),
    updatedAt: daysAgo(9),
  },
];

const SEED_ITEMS: SeedItem[] = [
  {
    kind: "apiKey",
    title: "OpenAI production",
    secret: "sk-proj-coffer-production",
    username: null,
    url: "https://platform.openai.com",
    projectId: "p-cofferdash",
    notes: "Primary key for the agent runtime.",
    tags: ["llm", "prod"],
    category: "AI / LLM",
    categorySource: "rule",
    favorite: true,
    createdAt: daysAgo(12),
    updatedAt: minutesAgo(4),
  },
  {
    kind: "apiKey",
    title: "Stripe live",
    secret: "sk_live_coffer_checkout",
    username: null,
    url: "https://dashboard.stripe.com",
    projectId: "p-orbit",
    notes: null,
    tags: ["payments"],
    category: "Payments",
    categorySource: "rule",
    createdAt: daysAgo(20),
    updatedAt: minutesAgo(51),
  },
  {
    kind: "password",
    title: "AWS console",
    secret: "night-heron-44*Quarry",
    username: "ops@coffer.dev",
    url: "https://console.aws.amazon.com",
    projectId: "p-orbit",
    notes: "Requires MFA. Shared by the platform team.",
    tags: ["aws", "infra"],
    category: "Cloud",
    categorySource: "rule",
    createdAt: daysAgo(60),
    updatedAt: minutesAgo(180),
  },
  {
    kind: "env",
    title: "Dashboard .env",
    secret: JSON.stringify([
      { key: "DATABASE_URL", value: "postgres://coffer:coffer@localhost:5432/coffer" },
      { key: "REDIS_URL", value: "redis://localhost:6379" },
      { key: "SESSION_SECRET", value: "local-dev-session" },
    ]),
    username: null,
    url: null,
    projectId: "p-cofferdash",
    notes: "Local development values.",
    tags: ["dev"],
    category: ".env",
    categorySource: "default",
    createdAt: daysAgo(8),
    updatedAt: minutesAgo(320),
  },
  {
    kind: "password",
    title: "GitHub personal",
    secret: "coffer-gh-9f3a2b",
    username: "luozhouyang",
    url: "https://github.com",
    projectId: null,
    notes: null,
    tags: ["git"],
    category: "VCS",
    categorySource: "rule",
    createdAt: daysAgo(90),
    updatedAt: daysAgo(1),
  },
  {
    kind: "apiKey",
    title: "Anthropic workspace",
    secret: "sk-ant-api03-coffer",
    username: null,
    url: "https://console.anthropic.com",
    projectId: "p-lab",
    notes: "Evaluation workspace.",
    tags: ["llm"],
    category: "AI / LLM",
    categorySource: "rule",
    createdAt: daysAgo(14),
    updatedAt: daysAgo(2),
  },
  {
    kind: "note",
    title: "Recovery runbook",
    secret: "",
    username: null,
    url: null,
    projectId: "p-lab",
    notes:
      "1. Rotate root credentials\n2. Restore latest encrypted backup\n3. Verify audit log entries",
    tags: ["runbook"],
    category: "Notes",
    categorySource: "default",
    createdAt: daysAgo(35),
    updatedAt: daysAgo(3),
  },
  {
    kind: "apiKey",
    title: "Slack webhook",
    secret: "T00000000/B00000000/coffer-hook",
    username: null,
    url: null,
    projectId: null,
    notes: null,
    tags: ["alerts"],
    category: "Messaging",
    categorySource: "rule",
    createdAt: daysAgo(50),
    updatedAt: daysAgo(6),
  },
  {
    kind: "password",
    title: "Postgres staging",
    secret: "staging-db-coffer-2024",
    username: "coffer_app",
    url: "postgres://staging.coffer.dev:5432/app",
    projectId: "p-cofferdash",
    notes: null,
    tags: ["db", "staging"],
    category: "Database",
    categorySource: "rule",
    createdAt: daysAgo(25),
    updatedAt: daysAgo(7),
  },
  {
    kind: "apiKey",
    title: "Vercel deploy token",
    secret: "vercel_7f4d9c2a8b",
    username: null,
    url: "https://vercel.com",
    projectId: "p-orbit",
    notes: null,
    tags: ["deploy"],
    category: "Cloud",
    categorySource: "rule",
    createdAt: daysAgo(18),
    updatedAt: daysAgo(10),
  },
];

const RULES: { test: RegExp; category: string; reason: string; tags: string[] }[] = [
  { test: /^sk-(?:proj-|ant-|live_|test_)/, category: "AI / LLM", reason: "sk- prefix", tags: ["llm"] },
  { test: /^pk_|^sk_live_|^rk_/, category: "Payments", reason: "Stripe key prefix", tags: ["payments"] },
  { test: /^vercel_|^AKIA|^ghp_|^glpat-/, category: "Cloud", reason: "Cloud provider token prefix", tags: [] },
  { test: /postgres|mysql|redis|mongodb/i, category: "Database", reason: "database URL", tags: [] },
];

function classify(value: string, url?: string | null): { category: string; reason: string | null; tags: string[] } {
  for (const rule of RULES) {
    if (rule.test.test(value)) return { category: rule.category, reason: rule.reason, tags: rule.tags };
  }
  if (url?.includes("github.com")) return { category: "VCS", reason: "github.com", tags: [] };
  if (url?.includes("slack.com") || url?.includes("hooks.slack")) {
    return { category: "Messaging", reason: "slack.com", tags: [] };
  }
  return { category: "Uncategorized", reason: null, tags: [] };
}

function toMeta(item: SeedItem & { id: string }): ItemMeta {
  return {
    id: item.id,
    kind: item.kind,
    title: item.title,
    username: item.username ?? null,
    url: item.url ?? null,
    projectId: item.projectId ?? null,
    category: item.category,
    categorySource: item.categorySource,
    favorite: item.favorite ?? false,
    tags: [...(item.tags ?? [])],
    createdAt: item.createdAt,
    updatedAt: item.updatedAt,
  };
}

function toDetail(item: SeedItem & { id: string }): ItemDetail {
  return { ...toMeta(item), notes: item.notes ?? null, hasSecret: Boolean(item.secret) };
}

export function createMockIpc(
  options: {
    delayMs?: number;
    password?: string;
    /** Mirrors `KeychainStatus.supported`; opt in per test for determinism. */
    keychainSupported?: boolean;
    /** Biometric hardware present and enrolled. Defaults to `keychainSupported`. */
    keychainAvailable?: boolean;
  } = {},
): MockIpc {
  const delay = options.delayMs ?? 0;
  let password = options.password ?? MASTER_PASSWORD;
  const keychainSupported = options.keychainSupported ?? false;
  const keychainAvailable = options.keychainAvailable ?? keychainSupported;
  let keychainEnabled = false;

  let initialized = true;
  let unlocked = true;
  let locked = false;

  // App-level broadcast: the lock event is sticky so a listener that
  // subscribes *after* `vaultLock()` (e.g. the root provider mounting) still
  // observes the lock, mirroring the backend's locked state on startup.
  let stateListeners: Array<() => void> = [];
  const setLocked = () => {
    if (locked) return;
    locked = true;
    unlocked = false;
    emit(EVENTS.locked, null);
    for (const fn of [...stateListeners]) fn();
  };
  let settings: Settings = {
    autoLockSecs: DEFAULTS.autoLockSecs,
    clipboardClearSecs: DEFAULTS.clipboardClearSecs,
    deepSearch: DEFAULTS.deepSearch,
  };

  const projects: Project[] = SEED_PROJECTS.map((p) => ({
    ...p,
    itemCount: SEED_ITEMS.filter((i) => i.projectId === p.id).length,
  }));
  const items = new Map<string, SeedItem & { id: string }>();
  for (const seed of SEED_ITEMS) {
    const id = nextId("i");
    items.set(id, { ...seed, id });
  }

  const secrets = new Map<string, string>();
  for (const item of items.values()) secrets.set(item.id, item.secret);

  let backupArchive: MockBackupArchive | null = null;
  const restoreSessions = new Map<string, { preview: BackupPreview; archive: MockBackupArchive }>();

  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const emit = (event: IpcEventName, payload: unknown = null) => {
    for (const fn of listeners.get(event) ?? []) fn(payload);
  };

  const guard = () => {
    if (!initialized) throw new IpcError("vaultNotInitialized", "No vault exists yet.");
    if (!unlocked) throw new IpcError("locked", "Vault is locked.");
  };

  const tick = async <T>(fn: () => T): Promise<T> => {
    if (delay > 0) await new Promise((r) => setTimeout(r, delay));
    return fn();
  };

  const counts = (): SidebarCounts => {
    const active = [...items.values()];
    return {
      all: active.length,
      unassigned: active.filter((i) => !i.projectId).length,
      projects: projects
        .filter((p) => !p.archivedAt)
        .map((p) => ({ projectId: p.id, count: active.filter((i) => i.projectId === p.id).length })),
    };
  };

  const status = (): VaultStatus => ({
    initialized,
    unlocked,
    autoLockSecs: settings.autoLockSecs,
    counts: unlocked ? counts() : { all: 0, unassigned: 0, projects: [] },
  });

  const keychainStatus = (): KeychainStatus => ({
    supported: keychainSupported,
    available: keychainAvailable,
    enabled: keychainEnabled,
  });

  const requireItem = (id: string) => {
    const item = items.get(id);
    if (!item) throw new IpcError("notFound", "Secret not found.");
    return item;
  };

  /** Emits a few throttled progress events so the UI can render a bar. */
  const emitProgress = async (event: IpcEventName, phases: string[]) => {
    for (let i = 0; i < phases.length; i++) {
      if (delay > 0) await new Promise((r) => setTimeout(r, delay));
      emit(event, { phase: phases[i], done: i + 1, total: phases.length } satisfies BackupProgressPayload);
    }
  };

  const validateMockExportDestination = (destination: string) => {
    const destinationPath = typeof destination === "string" ? destination : "";
    if (!destinationPath.startsWith("/") || destinationPath.endsWith("/coffer")) {
      throw new IpcError("validation", "The destination path is not valid.");
    }
    if (!destinationPath.endsWith(".coffer")) {
      throw new IpcError("validation", "The file name must end in .coffer.");
    }
    return destinationPath;
  };

  const writeMockArchive = (destinationPath: string, archivePassword: string): BackupExportResult => {
    const activeProjects = projects.filter((p) => !p.archivedAt);
    const itemCount = items.size;
    backupArchive = {
      password: archivePassword,
      vaultId: MOCK_VAULT_ID,
      createdAtMs: Date.now(),
      appVersion: MOCK_APP_VERSION,
      schemaVersion: MOCK_SCHEMA_VERSION,
      itemCount,
      projectCount: activeProjects.length,
      tagCount: new Set([...items.values()].flatMap((i) => i.tags ?? [])).size,
      snapshot: {
        projects: projects.map(copyProject),
        items: new Map([...items.entries()].map(([id, item]) => [id, copyItem(item)])),
        secrets: new Map(secrets),
      },
    };
    return {
      path: destinationPath,
      bytes: 65_536 + itemCount * 4_096,
      itemCount,
      projectCount: activeProjects.length,
      createdAtMs: backupArchive.createdAtMs,
    } satisfies BackupExportResult;
  };

  const refreshProjectCounts = () => {
    for (const p of projects) p.itemCount = [...items.values()].filter((i) => i.projectId === p.id).length;
  };

  const mock: MockIpc = {
    __state: {
      get initialized() {
        return initialized;
      },
      get unlocked() {
        return unlocked;
      },
      get settings() {
        return { ...settings };
      },
      get keychain() {
        return keychainStatus();
      },
      deletedProjects: [],
      copied: [],
      get backupArchive() {
        return backupArchive;
      },
      get restoreSessions() {
        return restoreSessions.size;
      },
    } as MockIpc["__state"],
    __emit: (event, payload = null) => {
      if (event === EVENTS.locked) {
        setLocked();
        return;
      }
      emit(event, payload);
    },
    __setPassword: (next) => {
      password = next;
    },
    __resetToFirstRun: () => {
      initialized = false;
      unlocked = false;
      locked = false;
      keychainEnabled = false;
      restoreSessions.clear();
    },
    __seedBackup: (patch) => {
      backupArchive = {
        password: password,
        vaultId: MOCK_VAULT_ID,
        createdAtMs: daysAgo(1),
        appVersion: MOCK_APP_VERSION,
        schemaVersion: MOCK_SCHEMA_VERSION,
        itemCount: items.size,
        projectCount: projects.length,
        tagCount: new Set([...items.values()].flatMap((i) => i.tags ?? [])).size,
        snapshot: null,
        ...patch,
      };
    },
    __onLocked: (fn) => {
      stateListeners.push(fn);
      return () => {
        stateListeners = stateListeners.filter((f) => f !== fn);
      };
    },

    vaultStatus: () => tick(status),
    vaultCreate: (pw) =>
      tick(() => {
        if (initialized) throw new IpcError("alreadyInitialized", "A vault already exists.");
        if (pw.length < 8) throw new IpcError("validation", "Master password must be at least 8 characters.");
        initialized = true;
        unlocked = true;
        locked = false;
        password = pw;
        return status();
      }),
    vaultUnlock: (pw) =>
      tick(() => {
        if (!initialized) throw new IpcError("vaultNotInitialized", "No vault exists yet.");
        if (pw !== password) throw new IpcError("invalidPassword", "Incorrect master password.");
        unlocked = true;
        locked = false;
        return status();
      }),
    vaultLock: () =>
      tick(() => {
        setLocked();
      }),
    vaultTouchActivity: () => tick(() => undefined),
    vaultChangePassword: (oldPw, newPw) =>
      tick(() => {
        guard();
        if (oldPw !== password) throw new IpcError("invalidPassword", "Incorrect master password.");
        password = newPw;
      }),

    backupExport: async (destination, pw) => {
      guard();
      const destinationPath = validateMockExportDestination(destination);
      await emitProgress(EVENTS.backupExportProgress, ["verify", "snapshot", "encrypt", "done"]);
      return tick(() => {
        if (pw !== password) throw new IpcError("invalidPassword", "Incorrect master password.");
        return writeMockArchive(destinationPath, password);
      });
    },
    backupExportKeychain: async (destination) => {
      guard();
      if (!keychainSupported || !keychainAvailable || !keychainEnabled) {
        throw new IpcError("keychainUnavailable", "Touch ID export is not enabled.");
      }
      const destinationPath = validateMockExportDestination(destination);
      await emitProgress(EVENTS.backupExportProgress, ["verify", "snapshot", "encrypt", "done"]);
      return tick(() => writeMockArchive(destinationPath, password));
    },
    backupRestoreBegin: async (path, pw) => {
      if (initialized) throw new IpcError("alreadyInitialized", "Restore is only available before a vault is created.");
      if (typeof path !== "string" || !path.endsWith(".coffer")) {
        throw new IpcError("unsupportedFormat", "This backup file is not supported.");
      }
      if (!backupArchive) throw new IpcError("notFound", "No backup file was selected.");
      const archive = backupArchive;
      await emitProgress(EVENTS.backupRestoreProgress, ["verify", "decrypt", "validate"]);
      return tick(() => {
        if (pw !== archive.password) throw new IpcError("invalidPassword", "Incorrect master password.");
        const sessionId = nextId("restore");
        const preview: BackupPreview = {
          sessionId,
          sourceVaultId: archive.vaultId,
          createdAtMs: archive.createdAtMs,
          appVersion: archive.appVersion,
          schemaVersion: archive.schemaVersion,
          itemCount: archive.itemCount,
          projectCount: archive.projectCount,
          tagCount: archive.tagCount,
        };
        restoreSessions.set(sessionId, { preview, archive });
        return preview;
      });
    },
    backupRestoreCommit: async (sessionId) => {
      const pending = restoreSessions.get(sessionId);
      if (!pending) throw new IpcError("notFound", "This restore session has expired.");
      if (initialized) {
        restoreSessions.delete(sessionId);
        throw new IpcError("alreadyInitialized", "A vault already exists.");
      }
      await emitProgress(EVENTS.backupRestoreProgress, ["commit", "done"]);
      return tick(() => {
        const session = restoreSessions.get(sessionId);
        if (!session) throw new IpcError("notFound", "This restore session has expired.");
        const { archive } = session;
        restoreSessions.delete(sessionId);
        if (archive.snapshot) {
          projects.splice(0, projects.length, ...archive.snapshot.projects.map(copyProject));
          items.clear();
          secrets.clear();
          for (const [id, item] of archive.snapshot.items) items.set(id, copyItem(item));
          for (const [id, value] of archive.snapshot.secrets) secrets.set(id, value);
        }
        initialized = true;
        unlocked = true;
        locked = false;
        password = archive.password;
        return status();
      });
    },
    backupRestoreCancel: (sessionId) =>
      tick(() => {
        restoreSessions.delete(sessionId);
      }),

    vaultKeychainStatus: () => tick(() => keychainStatus()),
    vaultKeychainEnable: (pw) =>
      tick(() => {
        guard();
        if (pw !== password) throw new IpcError("invalidPassword", "Incorrect master password.");
        if (!keychainSupported || !keychainAvailable) {
          throw new IpcError("keychainUnavailable", "Biometric unlock is not available on this device.");
        }
        keychainEnabled = true;
        return keychainStatus();
      }),
    vaultKeychainDisable: () =>
      tick(() => {
        keychainEnabled = false;
        return keychainStatus();
      }),
    vaultUnlockKeychain: () =>
      tick(() => {
        if (!initialized) throw new IpcError("vaultNotInitialized", "No vault exists yet.");
        if (!keychainEnabled) {
          throw new IpcError("keychainUnavailable", "No biometric key is stored.");
        }
        if (!keychainSupported || !keychainAvailable) {
          throw new IpcError("keychainUnavailable", "Biometric unlock is not available on this device.");
        }
        unlocked = true;
        locked = false;
        return status();
      }),

    settingsGet: () => tick(() => {
      guard();
      return { ...settings };
    }),
    settingsSet: (next) =>
      tick(() => {
        guard();
        if (next.autoLockSecs < 60 || next.autoLockSecs > 3600) {
          throw new IpcError("validation", "Auto-lock must be between 60 and 3600 seconds.");
        }
        if (next.clipboardClearSecs < 5 || next.clipboardClearSecs > 300) {
          throw new IpcError("validation", "Clipboard clear must be between 5 and 300 seconds.");
        }
        settings = { ...settings, ...next };
        return { ...settings };
      }),
    sidebarCounts: () => tick(() => {
      guard();
      return counts();
    }),

    projectsList: (includeArchived) =>
      tick(() => {
        guard();
        refreshProjectCounts();
        return projects.filter((p) => includeArchived || !p.archivedAt).map((p) => ({ ...p }));
      }),
    projectCreate: (input: ProjectInput) =>
      tick(() => {
        guard();
        const name = input.name.trim();
        if (!name) throw new IpcError("validation", "Project name is required.");
        if (name.length > 100) throw new IpcError("validation", "Project name must be at most 100 characters.");
        if (projects.some((p) => !p.archivedAt && p.name.toLowerCase() === name.toLowerCase())) {
          throw new IpcError("conflict", "An active project with this name already exists.");
        }
        const project: Project = {
          id: nextId("p"),
          name,
          description: input.description ?? null,
          color: input.color ?? null,
          archivedAt: null,
          itemCount: 0,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
        projects.push(project);
        return { ...project };
      }),
    projectUpdate: (input: ProjectUpdate) =>
      tick(() => {
        guard();
        const project = projects.find((p) => p.id === input.id);
        if (!project) throw new IpcError("notFound", "Project not found.");
        if (input.name !== undefined) {
          const name = input.name.trim();
          if (!name) throw new IpcError("validation", "Project name is required.");
          if (
            projects.some(
              (p) => p.id !== project.id && !p.archivedAt && p.name.toLowerCase() === name.toLowerCase(),
            )
          ) {
            throw new IpcError("conflict", "An active project with this name already exists.");
          }
          project.name = name;
        }
        if (input.description !== undefined) project.description = input.description;
        if (input.color !== undefined) project.color = input.color;
        if (input.archived !== undefined) project.archivedAt = input.archived ? Date.now() : null;
        project.updatedAt = Date.now();
        return { ...project };
      }),
    projectDelete: (id) =>
      tick(() => {
        guard();
        const idx = projects.findIndex((p) => p.id === id);
        if (idx === -1) throw new IpcError("notFound", "Project not found.");
        projects.splice(idx, 1);
        for (const item of items.values()) if (item.projectId === id) item.projectId = null;
        (mock.__state.deletedProjects as string[]).push(id);
      }),

    itemsList: (filter: ItemFilter) =>
      tick(() => {
        guard();
        if (filter.favoriteOnly) throw new IpcError("validation", "favoriteOnly is not supported yet.");
        return [...items.values()]
          .filter((i) => (filter.unassignedOnly ? !i.projectId : true))
          .filter((i) => (filter.projectId ? i.projectId === filter.projectId : true))
          .filter((i) => (filter.kind ? i.kind === filter.kind : true))
          .sort((a, b) => b.updatedAt - a.updatedAt)
          .map(toMeta);
      }),
    itemGet: (id) => tick(() => {
      guard();
      return toDetail(requireItem(id));
    }),
    itemCreate: (input: ItemInput) =>
      tick(() => {
        guard();
        const title = input.title.trim();
        if (!title) throw new IpcError("validation", "Title is required.");
        if (title.length > 200) throw new IpcError("validation", "Title must be at most 200 characters.");
        if (input.secret.length > 64 * 1024) throw new IpcError("validation", "Secret is too large.");
        const suggestion = classify(input.secret, input.url);
        const id = nextId("i");
        const item: SeedItem & { id: string } = {
          ...input,
          id,
          title,
          tags: input.tags ?? [],
          projectId: input.projectId ?? null,
          category: suggestion.category,
          categorySource: "rule",
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
        items.set(id, item);
        secrets.set(id, input.secret);
        return toMeta(item);
      }),
    itemUpdate: (input: ItemUpdate) =>
      tick(() => {
        guard();
        const item = requireItem(input.id);
        if (input.title !== undefined) {
          const title = input.title.trim();
          if (!title) throw new IpcError("validation", "Title is required.");
          item.title = title;
        }
        if (input.kind !== undefined) item.kind = input.kind;
        if (input.username !== undefined) item.username = input.username;
        if (input.url !== undefined) item.url = input.url;
        if (input.projectId !== undefined) item.projectId = input.projectId;
        if (input.notes !== undefined) item.notes = input.notes;
        if (input.tags !== undefined) item.tags = input.tags;
        if (input.secret !== undefined) {
          secrets.set(item.id, input.secret);
          if (item.categorySource !== "manual") {
            const suggestion = classify(input.secret, item.url);
            item.category = suggestion.category;
            item.categorySource = "rule";
          }
        }
        item.updatedAt = Date.now();
        return toMeta(item);
      }),
    itemDelete: (id) =>
      tick(() => {
        guard();
        requireItem(id);
        items.delete(id);
        secrets.delete(id);
      }),
    itemSetProject: (id, projectId) =>
      tick(() => {
        guard();
        const item = requireItem(id);
        if (projectId !== null && !projects.some((p) => p.id === projectId)) {
          throw new IpcError("notFound", "Project not found.");
        }
        item.projectId = projectId;
        item.updatedAt = Date.now();
        return toMeta(item);
      }),
    itemSetCategory: (id, category) =>
      tick(() => {
        guard();
        const item = requireItem(id);
        item.category = category;
        item.categorySource = category ? "manual" : "default";
        item.updatedAt = Date.now();
        return toMeta(item);
      }),
    itemToggleFavorite: (id) =>
      tick(() => {
        guard();
        const item = requireItem(id);
        item.favorite = !item.favorite;
        return toMeta(item);
      }),
    itemRevealSecret: (id) =>
      tick(() => {
        guard();
        requireItem(id);
        return { value: secrets.get(id) ?? "", hideAfterSecs: DEFAULTS.hideAfterSecs };
      }),
    copySecret: (id) =>
      tick(() => {
        guard();
        requireItem(id);
        (mock.__state.copied as string[]).push(id);
        return { clearAfterSecs: settings.clipboardClearSecs };
      }),

    search: (query, filter) =>
      tick(() => {
        guard();
        const q = query.trim().toLowerCase();
        const projectName = (id: string | null | undefined) => projects.find((p) => p.id === id)?.name ?? "";
        let pool = [...items.values()];
        if (filter.unassignedOnly) pool = pool.filter((i) => !i.projectId);
        else if (filter.projectId) pool = pool.filter((i) => i.projectId === filter.projectId);
        const matched = q
          ? pool.filter((i) => {
              const fields = [i.title, i.username ?? "", i.url ?? "", ...(i.tags ?? []), i.notes ?? "", projectName(i.projectId)].map(
                (f) => f.toLowerCase(),
              );
              return fields.some((f) => f.includes(q));
            })
          : pool;
        return matched
          .sort((a, b) => {
            if (!q) return b.updatedAt - a.updatedAt;
            const aTitle = a.title.toLowerCase().includes(q) ? 0 : 1;
            const bTitle = b.title.toLowerCase().includes(q) ? 0 : 1;
            return aTitle - bTitle || b.updatedAt - a.updatedAt;
          })
          .slice(0, q ? 40 : 20)
          .map((i) => {
            const field = i.title.toLowerCase().includes(q)
              ? "title"
              : i.username?.toLowerCase().includes(q)
                ? "username"
                : i.url?.toLowerCase().includes(q)
                  ? "url"
                  : (i.tags ?? []).some((t) => t.toLowerCase().includes(q))
                    ? "tags"
                    : i.notes?.toLowerCase().includes(q)
                      ? "notes"
                      : "project";
            return {
              item: toMeta(i),
              score: 1,
              matchedField: q ? field : "title",
              snippet: null,
            };
          });
      }),
    classifyPreview: (input) =>
      tick(() => {
        guard();
        const suggestion = classify(input.value, input.url);
        return {
          category: suggestion.category,
          tags: suggestion.tags,
          confidence: suggestion.reason ? 0.86 : 0.3,
          reason: suggestion.reason,
        };
      }),
    auditList: (itemId) =>
      tick((): AuditEntry[] => {
        guard();
        requireItem(itemId);
        return [
          { id: 3, action: "copy", itemId, createdAt: minutesAgo(12) },
          { id: 2, action: "update", itemId, createdAt: minutesAgo(180) },
          { id: 1, action: "create", itemId, createdAt: daysAgo(12) },
        ];
      }),

    on: async (event, handler) => {
      const set = listeners.get(event) ?? new Set();
      set.add(handler);
      listeners.set(event, set);
      // Sticky lock: a listener that subscribes after the lock was applied
      // still gets notified once (microtask), mirroring startup with a
      // locked backend.
      let reported = locked;
      const fn = () => {
        if (reported) return;
        reported = true;
        handler(null);
      };
      if (event === EVENTS.locked) {
        stateListeners.push(fn);
        if (locked) queueMicrotask(fn);
      }
      return () => {
        set.delete(handler);
        stateListeners = stateListeners.filter((f) => f !== fn);
      };
    },
  };

  return mock;
}

export const MOCK_MASTER_PASSWORD = MASTER_PASSWORD;
