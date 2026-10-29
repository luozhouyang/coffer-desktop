import { Braces, FileKey, KeyRound, StickyNote } from "lucide-react";
import type { ItemKind } from "@/lib/ipc-types";
import { cn } from "@/lib/utils";

const ICONS = {
  apiKey: KeyRound,
  env: Braces,
  password: FileKey,
  note: StickyNote,
} satisfies Record<ItemKind, unknown>;

/** 34px icon tile used in list rows (UI_UX.md §4). */
export function KindIcon({ kind, className }: { kind: ItemKind; className?: string }) {
  const Icon = ICONS[kind];
  return (
    <span
      className={cn(
        "flex size-[34px] shrink-0 items-center justify-center rounded-[9px] border border-border bg-[#0F131A] text-muted-foreground",
        className,
      )}
      aria-hidden
    >
      <Icon className="size-[17px]" strokeWidth={1.8} />
    </span>
  );
}
