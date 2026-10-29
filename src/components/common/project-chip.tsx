import { cn } from "@/lib/utils";
import { projectColor } from "@/lib/categories";

export function ProjectDot({ color, className }: { color: string; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("inline-block size-2 shrink-0 rounded-full", className)}
      style={{ backgroundColor: color }}
    />
  );
}

export function ProjectChip({
  name,
  color,
  id,
  className,
}: {
  name: string;
  color: string | null;
  id: string;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-md border border-[#4A3720] bg-[#2A2114] px-2 text-[11px] font-medium text-[#E2A75A]",
        className,
      )}
    >
      <ProjectDot color={projectColor(color, id)} />
      {name}
    </span>
  );
}
