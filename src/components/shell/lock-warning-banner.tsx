import { TimerReset } from "lucide-react";
import { useVault } from "@/app/vault-context";
import { Button } from "@/components/ui/button";

/** Shown after `vault://lock-warning`; any activity dismisses it (backend timer resets). */
export function LockWarningBanner() {
  const { warning, unlocked, recordActivity } = useVault();
  if (!warning || !unlocked || warning.seconds <= 0) return null;
  return (
    <div
      role="status"
      className="fixed bottom-5 left-1/2 z-40 flex -translate-x-1/2 items-center gap-3 rounded-[12px] border border-[#4A3720] bg-[#171B23] px-4 py-2.5 text-sm shadow-lg"
    >
      <TimerReset className="size-4 text-[#E2A75A]" aria-hidden />
      <span>
        Locking in <span className="font-mono font-semibold">{warning.seconds}s</span> due to inactivity.
      </span>
      <Button
        size="sm"
        variant="secondary"
        className="h-7"
        onClick={() => recordActivity()}
      >
        Stay unlocked
      </Button>
    </div>
  );
}
