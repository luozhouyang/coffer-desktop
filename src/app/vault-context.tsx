/* eslint-disable react-refresh/only-export-components */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useIpc } from "@/app/ipc-context";
import { resetUiStore } from "@/app/ui-store";
import { queryKeys } from "@/lib/query-keys";
import {
  DEFAULTS,
  EMPTY_COUNTS,
  EVENTS,
  type LockWarningPayload,
  type VaultStatus,
} from "@/lib/ipc-types";

export const UNLOCK_ROUTE = "/unlock";
export const SETUP_ROUTE = "/setup";

interface VaultContextValue {
  status: VaultStatus | undefined;
  initialized: boolean;
  unlocked: boolean;
  /** Last auto-lock warning, if one was received. */
  warning: LockWarningPayload | null;
  /** Epoch ms when the last user activity was observed by this client. */
  lastActivityAt: number;
  /** Epoch ms of the last lock warning; used for the countdown. */
  warnedAt: number | null;
  lock: () => Promise<void>;
  /** Clears cached metadata and routes to /unlock (triggered by `vault://locked`). */
  handleLocked: () => void;
  /** Throttled activity reporting (≥10s) via `vault_touch_activity`. */
  recordActivity: () => void;
}

const VaultContext = createContext<VaultContextValue | null>(null);

/** Throttle window for `vault_touch_activity` from the contract (≥10s). */
export const TOUCH_THROTTLE_MS = 10_000;

/** Reads the vault status from the Query cache into a plain value. */
export function useVaultStatusValue(): VaultStatus | undefined {
  const qc = useQueryClient();
  const subscribe = useCallback(
    (onChange: () => void) => qc.getQueryCache().subscribe((event) => {
      if (event.query.queryKey[0] === "vault") onChange();
    }),
    [qc],
  );
  const get = useCallback(() => qc.getQueryData<VaultStatus>(queryKeys.vaultStatus), [qc]);
  return useSyncExternalStore(subscribe, get, get);
}

/**
 * Vault status, global lock listeners, and auto-lock activity reporting.
 *
 * Status lives in the Query cache (`["vault","status"]`) — the same entry the
 * route guards fill with `ensureQueryData`. On `vault://locked` we clear the
 * cache (all metadata) and perform a full router navigation to `/unlock`, so
 * the route tree remounts and any revealed secret held in component state is
 * discarded along with it.
 */
export function VaultProvider({
  children,
  redirect,
}: {
  children: ReactNode;
  /** Router navigation used for the post-lock redirect. */
  redirect: (path: string) => void;
}) {
  const ipc = useIpc();
  const qc = useQueryClient();
  const status = useVaultStatusValue();
  const [warning, setWarning] = useState<LockWarningPayload | null>(null);
  const [lastActivityAt, setLastActivityAt] = useState(() => Date.now());
  const [warnedAt, setWarnedAt] = useState<number | null>(null);
  const lastTouchRef = useRef(0);
  const lockedRef = useRef(false);
  const redirectRef = useRef(redirect);
  redirectRef.current = redirect;

  const handleLocked = useCallback(() => {
    if (lockedRef.current) return;
    lockedRef.current = true;
    qc.clear();
    resetUiStore();
    setWarning(null);
    setWarnedAt(null);
    qc.setQueryData(queryKeys.vaultStatus, {
      initialized: true,
      unlocked: false,
      autoLockSecs: status?.autoLockSecs ?? DEFAULTS.autoLockSecs,
      counts: EMPTY_COUNTS,
    } satisfies VaultStatus);
    redirectRef.current(UNLOCK_ROUTE);
  }, [qc, status?.autoLockSecs]);

  const lock = useCallback(async () => {
    try {
      await ipc.vaultLock();
    } catch {
      // Even if the command fails the UI must stop showing secrets.
    }
    handleLocked();
  }, [handleLocked, ipc]);

  const recordActivity = useCallback(() => {
    const now = Date.now();
    setLastActivityAt(now);
    setWarning(null);
    if (now - lastTouchRef.current < TOUCH_THROTTLE_MS) return;
    lastTouchRef.current = now;
    void ipc.vaultTouchActivity().catch(() => undefined);
  }, [ipc]);

  // Re-arm the guard whenever the vault is unlocked again (e.g. after unlock).
  useEffect(() => {
    if (status?.unlocked) lockedRef.current = false;
  }, [status?.unlocked]);

  // Global lock listeners.
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void ipc
      .on(EVENTS.locked, () => handleLocked())
      .then((fn) => (disposed ? fn() : unlisteners.push(fn)));
    void ipc
      .on(EVENTS.lockWarning, (payload) => {
        if (lockedRef.current) return;
        const seconds =
          typeof payload === "object" && payload !== null && "seconds" in payload
            ? Number((payload as LockWarningPayload).seconds) || 0
            : 0;
        setWarning({ seconds });
        setWarnedAt(Date.now());
      })
      .then((fn) => (disposed ? fn() : unlisteners.push(fn)));
    return () => {
      disposed = true;
      for (const fn of unlisteners) fn();
    };
  }, [handleLocked, ipc]);

  // Throttled activity reporting on pointer and keyboard input.
  useEffect(() => {
    if (!status?.unlocked) return;
    const onActivity = () => recordActivity();
    window.addEventListener("pointerdown", onActivity, { passive: true });
    window.addEventListener("keydown", onActivity);
    return () => {
      window.removeEventListener("pointerdown", onActivity);
      window.removeEventListener("keydown", onActivity);
    };
  }, [recordActivity, status?.unlocked]);

  const value = useMemo<VaultContextValue>(
    () => ({
      status,
      initialized: status?.initialized ?? false,
      unlocked: status?.unlocked ?? false,
      warning,
      lastActivityAt,
      warnedAt,
      lock,
      handleLocked,
      recordActivity,
    }),
    [handleLocked, lastActivityAt, lock, recordActivity, status, warnedAt, warning],
  );

  return <VaultContext.Provider value={value}>{children}</VaultContext.Provider>;
}

export function useVault(): VaultContextValue {
  const ctx = useContext(VaultContext);
  if (!ctx) throw new Error("useVault must be used inside <VaultProvider>");
  return ctx;
}
