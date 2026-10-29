import { useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { KeyRound, Plus, SearchX, Tag } from "lucide-react";
import { useItems, useProjects } from "@/app/queries";
import { uiStore } from "@/app/ui-store";
import { CATEGORIES, type ItemFilter, type ItemKind, type ItemMeta } from "@/lib/ipc-types";
import { relativeTime, hostFromUrl, maskPreview, KIND_LABELS } from "@/lib/format";
import { cn } from "@/lib/utils";
import { isUnassignedFilter, parseVaultSearch, projectFilterId, toVaultLinkSearch } from "@/lib/vault-filter";
import { CategoryBadge } from "@/components/common/category-badge";
import { KindIcon } from "@/components/common/kind-icon";
import { SortSelect, type SortKey } from "@/components/common/sort-select";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { EmptyState, ErrorState, ListSkeleton } from "@/components/common/status-bits";
import { ProjectDot } from "@/components/common/project-chip";
import { projectColor } from "@/lib/categories";
import { Button } from "@/components/ui/button";
import { errorMessage } from "@/lib/ipc-errors";

const VIRTUALIZE_THRESHOLD = 200;
const ROW_HEIGHT = 76;
const ALL_CATEGORIES = "__all__";

const KIND_CHIPS: { key: ItemKind | "all"; label: string; ariaLabel: string }[] = [
  { key: "all", label: "All", ariaLabel: "All secrets" },
  { key: "apiKey", label: "Keys", ariaLabel: "API keys" },
  { key: "env", label: ".env", ariaLabel: ".env" },
  { key: "password", label: "Passwords", ariaLabel: "Passwords" },
  { key: "note", label: "Notes", ariaLabel: "Notes" },
];

export function VaultList({ search }: { search: Record<string, unknown> }) {
  const filter = parseVaultSearch(search);
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { itemId?: string };
  const { openCreate } = uiStore;

  const { data: projects } = useProjects();
  const [sort, setSort] = useState<SortKey>("updated");

  const itemFilter: ItemFilter = useMemo(
    () => ({
      unassignedOnly: isUnassignedFilter(filter),
      projectId: projectFilterId(filter),
      kind: filter.kind ?? null,
    }),
    [filter.kind, filter.project],
  );

  const itemsQuery = useItems(itemFilter);

  const title = useMemo(() => {
    if (isUnassignedFilter(filter)) return "Unassigned";
    if (filter.project) return projects?.find((p) => p.id === filter.project)?.name ?? "Project";
    return "All secrets";
  }, [filter.project, projects]);

  const project = filter.project && filter.project !== "unassigned" ? projects?.find((p) => p.id === filter.project) : undefined;

  const items = useMemo(() => {
    let list: ItemMeta[] = itemsQuery.data ? [...itemsQuery.data] : [];
    if (filter.category) {
      const wanted = filter.category;
      list = list.filter((i) =>
        wanted === "Uncategorized" ? !i.category || i.category === "Uncategorized" : i.category === wanted,
      );
    }
    if (filter.q) {
      const q = filter.q.toLowerCase();
      list = list.filter(
        (i) =>
          i.title.toLowerCase().includes(q) ||
          (i.username ?? "").toLowerCase().includes(q) ||
          (i.url ?? "").toLowerCase().includes(q) ||
          i.tags.some((t) => t.toLowerCase().includes(q)),
      );
    }
    if (sort === "title") list.sort((a, b) => a.title.localeCompare(b.title));
    else if (sort === "created") list.sort((a, b) => b.createdAt - a.createdAt);
    else list.sort((a, b) => b.updatedAt - a.updatedAt);
    return list;
  }, [filter.category, filter.q, itemsQuery.data, sort]);

  const setKind = (kind: ItemKind | null) => {
    void navigate({
      to: "/",
      search: toVaultLinkSearch({ ...filter, kind: kind ?? undefined }) as never,
      replace: true,
    });
  };

  const setCategory = (category: string | null) => {
    void navigate({
      to: "/",
      search: toVaultLinkSearch({ ...filter, category: category ?? undefined }) as never,
      replace: true,
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0 border-b border-border px-4 pt-4 pb-2">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <h1 className="flex items-center gap-2 truncate text-[19px] font-semibold tracking-[-0.01em]">
              {project ? <ProjectDot color={projectColor(project.color, project.id)} className="size-2.5" /> : null}
              <span className="truncate">{title}</span>
            </h1>
            <p className="mt-0.5 text-[11px] text-faint" data-testid="list-count">
              {itemsQuery.isPending ? "Loading…" : `${items.length} ${items.length === 1 ? "secret" : "secrets"}`}
              {filter.q ? ` matching “${filter.q}”` : ""}
            </p>
          </div>
          <Button
            size="sm"
            className="h-8 shrink-0 gap-1.5 px-2.5 text-[12px]"
            onClick={() => openCreate(filter.kind ?? null)}
          >
            <Plus className="size-3.5" aria-hidden />
            New
          </Button>
        </div>
        <div
          role="group"
          aria-label="Filter by type"
          className="mt-3 grid grid-cols-5 gap-0.5 rounded-[8px] border border-[#262D38] bg-[#0F131A] p-0.5"
        >
          {KIND_CHIPS.map((chip) => {
            const active = chip.key === "all" ? !filter.kind : filter.kind === chip.key;
            return (
              <button
                key={chip.key}
                type="button"
                aria-label={chip.ariaLabel}
                aria-pressed={active}
                onClick={() => setKind(chip.key === "all" ? null : chip.key)}
                className={cn(
                  "h-7 min-w-0 rounded-[6px] px-1 text-[11px] font-medium transition-colors",
                  active
                    ? "bg-[#2A2114] text-[#E2A75A] shadow-[inset_0_0_0_1px_#4A3720]"
                    : "text-muted-foreground hover:bg-accent hover:text-foreground",
                )}
              >
                <span className="block truncate">{chip.label}</span>
              </button>
            );
          })}
        </div>
        <div className="mt-2 flex items-center justify-between gap-2">
          <Select
            value={filter.category ?? ALL_CATEGORIES}
            onValueChange={(value) => setCategory(value === ALL_CATEGORIES ? null : value)}
          >
            <SelectTrigger
              size="sm"
              aria-label="Filter by category"
              className={cn(
                "h-8 w-auto max-w-[180px] gap-1.5 border-transparent bg-transparent px-2 text-[11px] hover:bg-accent",
                filter.category ? "text-[#E2A75A]" : "text-muted-foreground",
              )}
            >
              <Tag className="size-3.5 shrink-0" aria-hidden />
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="start">
              <SelectItem value={ALL_CATEGORIES}>All categories</SelectItem>
              {CATEGORIES.map((category) => (
                <SelectItem key={category} value={category}>
                  {category}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <SortSelect value={sort} onChange={setSort} />
        </div>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto" data-testid="vault-list">
        {itemsQuery.isPending ? <ListSkeleton /> : null}
        {itemsQuery.isError ? (
          <ErrorState
            title="Could not load secrets"
            message={errorMessage(itemsQuery.error)}
            onRetry={() => void itemsQuery.refetch()}
          />
        ) : null}
        {itemsQuery.isSuccess && items.length === 0 ? (
          <EmptyState
            icon={<KeyRound className="size-5" aria-hidden />}
            title={filter.q || filter.category ? "No matches" : "No secrets here yet"}
            description={
              filter.q
                ? "Try a different search term or clear the filter."
                : "Add your first secret to this vault. Everything stays encrypted on this device."
            }
          />
        ) : null}
        {items.length > 0 ? (
          items.length > VIRTUALIZE_THRESHOLD ? (
            <VirtualRows items={items} selectedId={params.itemId} projects={projects ?? []} />
          ) : (
            <ul className="px-2 py-2">
              {items.map((item) => (
                <li key={item.id}>
                  <ItemRow
                    item={item}
                    selected={item.id === params.itemId}
                    projectName={projectName(projects, item.projectId)}
                  />
                </li>
              ))}
            </ul>
          )
        ) : null}
        {itemsQuery.isSuccess && items.length === 0 && filter.q ? <p className="sr-only"><SearchX /></p> : null}
      </div>
    </div>
  );
}

function projectName(projects: { id: string; name: string }[] | undefined, id: string | null): string | null {
  if (!id) return null;
  return projects?.find((p) => p.id === id)?.name ?? null;
}

function VirtualRows({
  items,
  selectedId,
  projects,
}: {
  items: ItemMeta[];
  selectedId: string | undefined;
  projects: { id: string; name: string }[];
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  useEffect(() => {
    const index = items.findIndex((i) => i.id === selectedId);
    if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  return (
    <div ref={parentRef} className="h-full overflow-y-auto">
      <div style={{ height: virtualizer.getTotalSize(), position: "relative", width: "100%" }}>
        {virtualizer.getVirtualItems().map((row) => {
          const item = items[row.index];
          const pname = projectName(projects, item.projectId);
          return (
            <div
              key={item.id}
              style={{ position: "absolute", top: 0, left: 0, width: "100%", height: row.size, transform: `translateY(${row.start}px)` }}
              className="px-2 py-0.5"
            >
              <ItemRow item={item} selected={item.id === selectedId} projectName={pname} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function ItemRow({
  item,
  selected,
  projectName,
  exactLink,
}: {
  item: ItemMeta;
  selected: boolean;
  projectName: string | null;
  /** Render a static row instead of a router <Link> (tests/previews). */
  exactLink?: boolean;
}) {
  const subtitle = hostFromUrl(item.url) ?? item.username ?? projectName ?? KIND_LABELS[item.kind];
  const linkSearch = (prev: Record<string, unknown>) => toVaultLinkSearch(parseVaultSearch(prev)) as never;

  const content = (
    <>
      {selected ? (
        <span aria-hidden className="absolute top-2 bottom-2 left-0 w-[2px] rounded-full bg-[#E2A75A]" />
      ) : null}
      <KindIcon kind={item.kind} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-semibold leading-tight">{item.title}</div>
        <div className="mt-1 flex items-center gap-1.5 truncate text-[11px] text-muted-foreground">
          <span className="truncate">{subtitle}</span>
          <span aria-hidden className="text-faint">·</span>
          <span className="shrink-0 font-mono text-[10.5px] text-faint">{maskPreview(item.id)}</span>
        </div>
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1.5">
        <CategoryBadge category={item.category} />
        <span className="text-[10.5px] text-faint">{relativeTime(item.updatedAt)}</span>
      </div>
    </>
  );

  const rowClass = cn(
    "relative flex h-[72px] w-full items-center gap-3 rounded-[10px] px-2.5 text-left transition-colors",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
    selected ? "bg-[#1E2530]" : "hover:bg-white/[0.03]",
  );

  if (exactLink) {
    return (
      <div role="option" aria-selected={selected} data-item-id={item.id} className={rowClass}>
        {content}
      </div>
    );
  }

  return (
    <Link
      to="/item/$itemId"
      params={{ itemId: item.id }}
      search={linkSearch}
      data-item-id={item.id}
      data-selected={selected || undefined}
      className={rowClass}
    >
      {content}
    </Link>
  );
}
