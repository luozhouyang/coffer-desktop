import { cn } from "@/lib/utils";

/** Small monospace keycap, matching UI_UX.md §4 (20–22px, #232935). */
export function Kbd({ className, children }: { className?: string; children: React.ReactNode }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded-[4px] border border-white/6 bg-[#232935] px-1.5 font-mono text-[10px] font-medium text-muted-foreground",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
