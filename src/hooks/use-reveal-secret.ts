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

/** Rust-side copy: the secret never crosses IPC; we only learn the clear delay. */
export function useCopySecret() {
  const ipc = useIpc();
  const [pendingId, setPendingId] = useState<string | null>(null);
  const inFlight = useRef(false);
  const copy = async (itemId: string, title?: string) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setPendingId(itemId);
    try {
      const { clearAfterSecs } = await ipc.copySecret(itemId);
      toast.success(`Copied${title ? ` “${title}”` : ""}`, {
        description: `Clipboard clears in ${clearAfterSecs}s.`,
        id: "copy-secret",
      });
      return clearAfterSecs;
    } catch (err) {
      toast.error("Could not copy secret", { description: errorMessage(err) });
      return null;
    } finally {
      inFlight.current = false;
      setPendingId((current) => (current === itemId ? null : current));
    }
  };
  return { copy, pendingId };
}
