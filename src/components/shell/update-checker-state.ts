import { isTauriRuntime } from "@/lib/tauri-ipc";

/** Delay before the one-shot startup check, so it never competes with unlock. */
export const UPDATE_CHECK_DELAY_MS = 3000;

/** In-memory guard: at most one automatic check per app session. */
let checkedThisSession = false;

/**
 * Checks for updates once per app session, after a short delay. Returns the
 * update version found, or null when up to date. Errors are swallowed: startup
 * must stay quiet, and Settings exposes a manual retry.
 *
 * The runtime guard runs synchronously, before any timer is scheduled, so in
 * a plain browser or jsdom this never does anything.
 */
export async function checkForUpdateOnce(
  check: () => Promise<{ version: string } | null>,
): Promise<string | null> {
  if (!isTauriRuntime() || checkedThisSession) return null;
  checkedThisSession = true;
  await new Promise((resolve) => setTimeout(resolve, UPDATE_CHECK_DELAY_MS));
  try {
    const update = await check();
    return update?.version ?? null;
  } catch {
    return null;
  }
}

/** Test-only: forget the per-session guard. */
export function __resetStartupUpdateCheckForTests() {
  checkedThisSession = false;
}
