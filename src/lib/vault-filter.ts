import { ITEM_KINDS, type ItemKind } from "@/lib/ipc-types";

/** Parsed vault list filter, derived from the `_vault` search params. */
export interface ActiveFilter {
  /** Project id, or "unassigned", or undefined for all. */
  project?: string | "unassigned";
  kind?: ItemKind;
  q?: string;
}

export function parseVaultSearch(search: Record<string, unknown>): ActiveFilter {
  const project =
    typeof search.project === "string" && search.project.length > 0 ? search.project : undefined;
  const kind =
    typeof search.kind === "string" && (ITEM_KINDS as readonly string[]).includes(search.kind)
      ? (search.kind as ItemKind)
      : undefined;
  const q = typeof search.q === "string" && search.q.trim() ? search.q.trim() : undefined;
  return { project, kind, q };
}

/** Serialises a filter to a search object, dropping empty keys. */
export function toVaultLinkSearch(filter: ActiveFilter): Record<string, string | undefined> {
  const out: Record<string, string | undefined> = {};
  if (filter.project) out.project = filter.project;
  if (filter.kind) out.kind = filter.kind;
  if (filter.q) out.q = filter.q;
  return out;
}

export function isUnassignedFilter(filter: ActiveFilter): boolean {
  return filter.project === "unassigned";
}

export function projectFilterId(filter: ActiveFilter): string | null {
  return filter.project && filter.project !== "unassigned" ? filter.project : null;
}
