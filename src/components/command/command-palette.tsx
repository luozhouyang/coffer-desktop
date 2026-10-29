import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { ArrowRight, Copy, Sparkles } from "lucide-react";
import { useIpc } from "@/app/ipc-context";
import { useProjects, useSettings } from "@/app/queries";
import { uiStore, useUiStore } from "@/app/ui-store";
import { useCopySecret } from "@/hooks/use-reveal-secret";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import type { SearchHit } from "@/lib/ipc-types";
import { kindLabelShort } from "@/lib/format";
import { isMacPlatform } from "@/lib/platform";
import { errorMessage } from "@/lib/ipc-errors";
import { toVaultLinkSearch } from "@/lib/vault-filter";
import { CategoryBadge } from "@/components/common/category-badge";
import { Kbd } from "@/components/common/kbd";
import { ProjectDot } from "@/components/common/project-chip";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { InlineSpinner } from "@/components/common/status-bits";

const ALL_PROJECTS = "__all__";

/**
 * Global command palette (⌘K). Hosted in `__root`; state lives in `uiStore`.
 *
 * Keyboard: ↑↓ navigate (cmdk), ↵ copy (`copy_secret`, Rust-side),
 * ⌘↵ reveal (navigates to the item where the user can reveal explicitly),
 * Esc closes. No secret values are ever fetched here.
 */
export function CommandPalette() {
  const open = useUiStore((s) => s.paletteOpen);
  const ipc = useIpc();
  const navigate = useNavigate();
  const projectsQuery = useProjects();
  const settings = useSettings();
  const { copy } = useCopySecret();

  const [query, setQuery] = useState("");
  const [projectFilter, setProjectFilter] = useState<string>(ALL_PROJECTS);
  const debouncedQuery = useDebouncedValue(query, 120);

  const [hits, setHits] = useState<SearchHit[]>([]);
  const [activeId, setActiveId] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);

  // Runtime validation: deep search only when the user enabled it in settings.
  const deep = settings.data?.deepSearch ?? false;

  // Global ⌘K / Ctrl+K opens the palette from anywhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        uiStore.setPaletteOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setHits([]);
      setActiveId("");
      setError(null);
      setLoading(false);
      requestId.current += 1;
    }
  }, [open]);

  // cmdk only selects an item after a key press when results are loaded
  // asynchronously; keep the first hit active so plain Enter works.
  useEffect(() => {
    if (!open) return;
    setActiveId((current) =>
      hits.some((hit) => hit.item.id === current) ? current : (hits[0]?.item.id ?? ""),
    );
  }, [hits, open]);

  // Debounced, ordered search: stale responses are discarded by request id.
  useEffect(() => {
    if (!open) return;
    const id = ++requestId.current;
    setLoading(true);
    setError(null);
    const filter =
      projectFilter === ALL_PROJECTS
        ? { unassignedOnly: false, deep }
        : projectFilter === "unassigned"
          ? { unassignedOnly: true, deep }
          : { projectId: projectFilter, unassignedOnly: false, deep };
    ipc
      .search(debouncedQuery.trim(), filter)
      .then((result) => {
        if (requestId.current !== id) return; // superseded
        setHits(result);
      })
      .catch((err) => {
        if (requestId.current !== id) return;
        setError(errorMessage(err));
        setHits([]);
      })
      .finally(() => {
        if (requestId.current === id) setLoading(false);
      });
  }, [debouncedQuery, deep, ipc, open, projectFilter]);

  const projectName = useMemo(() => {
    const map = new Map<string, { name: string; color: string | null }>();
    for (const p of projectsQuery.data ?? []) map.set(p.id, { name: p.name, color: p.color });
    return map;
  }, [projectsQuery.data]);

  const openItem = (id: string) => {
    uiStore.setPaletteOpen(false);
    void navigate({ to: "/item/$itemId", params: { itemId: id }, search: toVaultLinkSearch({}) as never });
  };

  const onActivate = async (hit: SearchHit, reveal: boolean) => {
    if (reveal) {
      openItem(hit.item.id);
      return;
    }
    const clearAfter = await copy(hit.item.id, hit.item.title);
    if (clearAfter !== null) uiStore.setPaletteOpen(false);
  };

  const mod = isMacPlatform();

  return (
    <Dialog open={open} onOpenChange={(next) => uiStore.setPaletteOpen(next)}>
      <DialogContent showCloseButton={false} className="top-[18%] flex max-w-[720px] translate-y-0 flex-col gap-0 overflow-hidden p-0 sm:max-w-[720px]">
        <DialogTitle className="sr-only">Search secrets</DialogTitle>
        <DialogDescription className="sr-only">
          Type to search your vault. Enter copies the selected secret. Command or Control plus Enter opens it.
        </DialogDescription>
        <Command
          shouldFilter={false}
          value={activeId}
          onValueChange={setActiveId}
          className="min-h-0 flex-1 rounded-none bg-transparent"
        >
          <div className="flex items-center gap-1 border-b border-border pr-3">
            <CommandInput
              autoFocus
              value={query}
              onValueChange={setQuery}
              className="h-12 text-[14px]"
              placeholder="Search vault…"
            />
            <Select value={projectFilter} onValueChange={setProjectFilter}>
              <SelectTrigger
                size="sm"
                tabIndex={0}
                dir="ltr"
                aria-label="Filter by project"
                className="h-7 w-auto max-w-[168px] shrink-0 gap-1.5 border-[#2B3240] bg-[#0F131A] text-[11px]"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent align="end">
                <SelectItem value={ALL_PROJECTS}>All projects</SelectItem>
                <SelectItem value="unassigned">Unassigned</SelectItem>
                {(projectsQuery.data ?? []).map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    <ProjectDot color={p.color ?? "#E2A75A"} />
                    {p.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <CommandList className="max-h-[min(56vh,420px)] py-1">
            {loading && hits.length === 0 ? (
              <div className="flex items-center justify-center gap-2 py-10 text-xs text-muted-foreground" role="status">
                <InlineSpinner /> Searching…
              </div>
            ) : null}
            {error ? (
              <p role="alert" className="px-4 py-8 text-center text-xs text-destructive">
                {error}
              </p>
            ) : null}
            {!loading && !error && hits.length === 0 ? (
              <CommandEmpty className="py-10 text-center text-xs text-muted-foreground">
                No matches. Try a project filter or a shorter query.
              </CommandEmpty>
            ) : null}
            <CommandGroup heading={debouncedQuery.trim() ? "Results" : "Recent"}>
              {hits.map((hit) => {
                const project = hit.item.projectId ? projectName.get(hit.item.projectId) : null;
                return (
                  <CommandItem
                    key={hit.item.id}
                    value={hit.item.id}
                    onSelect={(value) => {
                      // cmdk also emits select("") when the filtered list empties.
                      if (!value) return;
                      onActivate(hit, false);
                    }}
                    className="group flex h-11 items-center gap-3 px-3 data-[selected=true]:bg-[#1E2530]"
                    data-testid="palette-result"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-[13px] font-medium">{hit.item.title}</span>
                        <span className="shrink-0 text-[10.5px] text-faint">{kindLabelShort(hit.item.kind)}</span>
                      </div>
                      <div className="mt-0.5 flex items-center gap-1.5 text-[11px] text-muted-foreground">
                        {project ? (
                          <span className="inline-flex items-center gap-1 text-[#E2A75A]">
                            <ProjectDot color={project.color ?? "#E2A75A"} className="size-1.5" />
                            {project.name}
                          </span>
                        ) : (
                          <span className="text-faint">No project</span>
                        )}
                        {hit.item.url ? <span className="truncate text-faint">· {hit.snippet ?? hit.item.url}</span> : null}
                      </div>
                    </div>
                    <CategoryBadge category={hit.item.category} />
                    <span className="flex items-center gap-1 opacity-70">
                      <button
                        type="button"
                        aria-label={`Open ${hit.item.title}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          openItem(hit.item.id);
                        }}
                        className="rounded p-1 text-faint hover:text-foreground"
                        onPointerDown={(e) => e.preventDefault()}
                      >
                        <ArrowRight className="size-3.5" aria-hidden />
                      </button>
                      <Copy className="size-3.5 text-faint" aria-hidden />
                      <Kbd>↵</Kbd>
                    </span>
                  </CommandItem>
                );
              })}
            </CommandGroup>
          </CommandList>

          <div className="flex items-center gap-3 border-t border-border px-3 py-2 text-[10.5px] text-faint">
            <span className="flex shrink-0 items-center gap-1 whitespace-nowrap">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd> navigate
            </span>
            <span className="flex shrink-0 items-center gap-1 whitespace-nowrap">
              <Kbd>↵</Kbd> copy
            </span>
            <span className="flex shrink-0 items-center gap-1 whitespace-nowrap">
              <Kbd>{mod ? "⌘" : "Ctrl+"}↵</Kbd> open
            </span>
            <span className="flex shrink-0 items-center gap-1 whitespace-nowrap">
              <Kbd>Esc</Kbd> close
            </span>
            <span className="ml-auto flex shrink-0 items-center gap-1.5 whitespace-nowrap">
              {deep ? (
                <>
                  <Sparkles className="size-3" aria-hidden /> Deep search on
                </>
              ) : (
                "Metadata only · secrets never leave Rust"
              )}
            </span>
          </div>
          <div className="sr-only" aria-live="polite">
            {hits.length} results
          </div>
          <RevealShortcutHandler
            onReveal={() => {
              const first = hits[0];
              if (first) onActivate(first, true);
            }}
            active={open && hits.length > 0}
          />
        </Command>
      </DialogContent>
    </Dialog>
  );
}

/**
 * cmdk consumes Enter for `onSelect`. Cmd/Ctrl+Enter should reveal (open the
 * item where reveal is explicit) rather than copy, so we intercept it on the
 * capture phase before cmdk's listbox handler runs.
 */
function RevealShortcutHandler({ onReveal, active }: { onReveal: () => void; active: boolean }) {
  const cb = useRef(onReveal);
  cb.current = onReveal;
  useEffect(() => {
    if (!active) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        e.stopPropagation();
        cb.current();
      }
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [active]);
  return null;
}
