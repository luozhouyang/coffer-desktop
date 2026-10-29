import { ArrowDownAZ, ArrowDownWideNarrow, ArrowDown01 } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

export type SortKey = "updated" | "title" | "created";

export const SORT_LABELS: Record<SortKey, string> = {
  updated: "Updated",
  title: "Title",
  created: "Created",
};

export function SortSelect({ value, onChange }: { value: SortKey; onChange: (next: SortKey) => void }) {
  const Icon = value === "title" ? ArrowDownAZ : value === "created" ? ArrowDown01 : ArrowDownWideNarrow;
  return (
    <Select value={value} onValueChange={(v) => onChange(v as SortKey)}>
      <SelectTrigger
        size="sm"
        aria-label="Sort secrets"
        className="h-8 w-[128px] gap-1.5 border-transparent bg-transparent text-xs text-muted-foreground hover:bg-accent"
      >
        <Icon className="size-3.5" aria-hidden />
        <SelectValue />
      </SelectTrigger>
      <SelectContent align="end">
        <SelectItem value="updated">Updated</SelectItem>
        <SelectItem value="title">Title</SelectItem>
        <SelectItem value="created">Created</SelectItem>
      </SelectContent>
    </Select>
  );
}
