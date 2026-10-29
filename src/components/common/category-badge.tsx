import { categoryLabel, categoryStyle } from "@/lib/categories";
import { cn } from "@/lib/utils";

export function CategoryBadge({ category, className }: { category: string | null | undefined; className?: string }) {
  const label = categoryLabel(category);
  const style = categoryStyle(category);
  return (
    <span
      data-testid="category-badge"
      className={cn(
        "inline-flex h-[21px] shrink-0 items-center rounded-[5px] px-2 text-[10px] font-semibold tracking-[0.04em]",
        className,
      )}
      style={{ backgroundColor: style.bg, color: style.fg }}
    >
      {label}
    </span>
  );
}
