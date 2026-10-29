import { TriangleAlert } from "lucide-react";
import { cn } from "@/lib/utils";

/** Consistent "no recovery" wording (UI_UX.md §8: same copy in setup/unlock/settings). */
export function NoRecoveryNotice({ className }: { className?: string }) {
  return (
    <div
      role="note"
      className={cn(
        "flex gap-2.5 rounded-[10px] border border-[#4A3720] bg-[#2A2114]/60 px-3 py-2.5 text-[12px] leading-relaxed text-warning",
        className,
      )}
    >
      <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
      <p>
        <span className="font-semibold">No recovery.</span> If you forget your master password, the vault
        cannot be opened.
      </p>
    </div>
  );
}
