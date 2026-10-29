/** Category badge colors from docs/UI_UX.md §2.1. Text is never colour-only. */
export interface CategoryStyle {
  bg: string;
  fg: string;
}

const STYLES: Record<string, CategoryStyle> = {
  "AI / LLM": { bg: "#2A2140", fg: "#C3A6FF" },
  Payments: { bg: "#2A2114", fg: "#E2A75A" },
  Cloud: { bg: "#17233A", fg: "#8FB4FF" },
  VCS: { bg: "#16261E", fg: "#7FD3A4" },
  ".env": { bg: "#17282A", fg: "#7FD1D8" },
  Messaging: { bg: "#1C2A2E", fg: "#74CFD6" },
  Database: { bg: "#2E1D22", fg: "#E88BA0" },
  Notes: { bg: "#24262B", fg: "#B8C0CC" },
  Uncategorized: { bg: "#24262B", fg: "#B8C0CC" },
};

export const DEFAULT_CATEGORY = "Uncategorized";

export function categoryStyle(category: string | null | undefined): CategoryStyle {
  return STYLES[category ?? ""] ?? STYLES.Uncategorized;
}

export function categoryLabel(category: string | null | undefined): string {
  return category && category.trim() ? category : DEFAULT_CATEGORY;
}

/** Project dot colours used when a project has no colour. */
const PROJECT_PALETTE = ["#E2A75A", "#8FB4FF", "#7FD3A4", "#C3A6FF", "#E88BA0", "#74CFD6"];

export function projectColor(color: string | null | undefined, id: string): string {
  if (color && /^#[0-9a-fA-F]{6}$/.test(color)) return color;
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return PROJECT_PALETTE[h % PROJECT_PALETTE.length];
}
