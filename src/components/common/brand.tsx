import { cn } from "@/lib/utils";

/** Brass "coin" mark: a square vault dial. */
export function BrandMark({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "relative flex size-7 shrink-0 items-center justify-center rounded-[8px] bg-gradient-to-b from-[#F0BE78] to-[#C98D42] shadow-[inset_0_1px_0_rgba(255,255,255,0.35),0_1px_2px_rgba(0,0,0,0.5)]",
        className,
      )}
    >
      <span className="size-3 rounded-full border-[1.5px] border-[#3A2810]" />
      <span className="absolute top-1/2 h-[1.5px] w-[7px] -translate-y-1/2 bg-[#3A2810]" />
    </span>
  );
}

export function Brand({ subtitle, className }: { subtitle?: string; className?: string }) {
  return (
    <div className={cn("flex items-center gap-2.5", className)}>
      <BrandMark />
      <div className="flex min-w-0 flex-col leading-none">
        <span className="text-[15px] font-semibold tracking-[-0.01em]">Coffer</span>
        {subtitle ? (
          <span className="mt-1 truncate text-[10px] font-medium uppercase tracking-[0.12em] text-faint">
            {subtitle}
          </span>
        ) : null}
      </div>
    </div>
  );
}
