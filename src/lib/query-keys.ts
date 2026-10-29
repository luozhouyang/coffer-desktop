import type { ItemFilter } from "@/lib/ipc-types";

/** Query keys from IMPLEMENTATION_PLAN.md §3.4. Secrets never enter these. */
export const queryKeys = {
  vaultStatus: ["vault", "status"] as const,
  sidebarCounts: ["sidebar", "counts"] as const,
  projects: (includeArchived: boolean) => ["projects", includeArchived] as const,
  items: (filter: ItemFilter) => ["items", filter] as const,
  item: (id: string) => ["item", id] as const,
  settings: ["settings"] as const,
  audit: (itemId: string) => ["audit", itemId] as const,
  search: (q: string, filter: { projectId?: string | null; unassignedOnly: boolean }) =>
    ["search", { q, ...filter }] as const,
};

export const itemsPrefix = ["items"] as const;
export const itemPrefix = ["item"] as const;
export const projectsPrefix = ["projects"] as const;
