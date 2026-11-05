import { useEffect, useRef, useState } from "react";
import { useIpc } from "@/app/ipc-context";
import { DEFAULTS } from "@/lib/ipc-types";
import { errorMessage } from "@/lib/ipc-errors";
import { toast } from "sonner";

/**
 * Imperative reveal: calls `item_reveal_secret` directly (never through the
 * Query cache), keeps the value only in component state, and hides it
 * after `hideAfterSecs` (15), on unmount, or when the item changes. Locking
 * tears the whole vault tree down, which clears the state with it.
 */
export function useRevealSecret(itemId: string | undefined) {
  const ipc = useIpc();
  const [value, setValue] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [secondsLeft, setSecondsLeft] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const requestRef = useRef(0);

  const stopTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  };

  const clear = () => {
    requestRef.current += 1;
    stopTimer();
    setSecondsLeft(0);
    setValue(null);
  };

  // Invalidate any in-flight request and stop the countdown on unmount / id change.
  useEffect(() => {
    return () => {
      requestRef.current += 1;
      stopTimer();
    };
  }, [itemId]);

  const reveal = async () => {
    if (!itemId || pending) return;
    const token = ++requestRef.current;
    setPending(true);
    setError(null);
    try {
      const result = await ipc.itemRevealSecret(itemId);
      if (requestRef.current !== token) return; // superseded or cleared
      const hideAfter = result.hideAfterSecs || DEFAULTS.hideAfterSecs;
      stopTimer();
      setValue(result.value);
      setSecondsLeft(hideAfter);
      timerRef.current = setInterval(() => {
        setSecondsLeft((s) => {
          if (s <= 1) {
            stopTimer();
            setValue(null);
            return 0;
          }
          return s - 1;
        });
      }, 1000);
    } catch (err) {
      if (requestRef.current === token) {
        setError(errorMessage(err));
        toast.error(errorMessage(err));
      }
    } finally {
      if (requestRef.current === token) setPending(false);
    }
  };

  return { value, revealed: value !== null, pending, error, secondsLeft, reveal, hide: clear };
}

/** Shared single-flight clipboard action for the two copy commands. */
function useClipboardAction() {
  const [pending, setPending] = useState<string | null>(null);
  const inFlight = useRef(false);
  const run = async (
    token: string,
    action: () => Promise<{ clearAfterSecs: number }>,
    onSuccess: (clearAfterSecs: number) => void,
    onError: (err: unknown) => void,
  ): Promise<number | null> => {
    if (inFlight.current) return null;
    inFlight.current = true;
    setPending(token);
    try {
      const { clearAfterSecs } = await action();
      onSuccess(clearAfterSecs);
      return clearAfterSecs;
    } catch (err) {
      onError(err);
      return null;
    } finally {
      inFlight.current = false;
      setPending((current) => (current === token ? null : current));
    }
  };
  return { pending, run };
}

/** Rust-side copy: the secret never crosses IPC; we only learn the clear delay. */
export function useCopySecret() {
  const ipc = useIpc();
  const { pending, run } = useClipboardAction();
  const copy = (itemId: string, title?: string) =>
    run(
      itemId,
      () => ipc.copySecret(itemId),
      (clearAfterSecs) =>
        toast.success(`Copied${title ? ` “${title}”` : ""}`, {
          description: `Clipboard clears in ${clearAfterSecs}s.`,
          id: "copy-secret",
        }),
      (err) => toast.error("Could not copy secret", { description: errorMessage(err) }),
    );
  return { copy, pendingId: pending };
}

/** Copies one row of an `.env` item by position; the value never crosses IPC. */
export function useCopyEnvValue() {
  const ipc = useIpc();
  const { pending, run } = useClipboardAction();
  const copy = (itemId: string, key: string, index: number) =>
    run(
      key,
      () => ipc.copyEnvValue(itemId, key, index),
      (clearAfterSecs) =>
        toast.success(`Copied ${key}`, {
          description: `Clipboard clears in ${clearAfterSecs}s.`,
          id: "copy-secret",
        }),
      (err) => toast.error("Could not copy variable", { description: errorMessage(err) }),
    );
  return { copy, pendingKey: pending };
}
