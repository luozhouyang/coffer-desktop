import type { ItemKind } from "@/lib/ipc-types";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Compact relative time for list rows: "now", "4m", "3h", "6d". */
export function relativeTime(ms: number, now = Date.now()): string {
  const diff = Math.max(0, now - ms);
  if (diff < MINUTE) return "now";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)}m`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)}h`;
  if (diff < 7 * DAY) return `${Math.floor(diff / DAY)}d`;
  if (diff < 30 * DAY) return `${Math.floor(diff / (7 * DAY))}w`;
  if (diff < 365 * DAY) return `${Math.floor(diff / (30 * DAY))}mo`;
  return `${Math.floor(diff / (365 * DAY))}y`;
}

/** Verbose relative time for detail metadata: "3 hours ago". */
export function longRelativeTime(ms: number, now = Date.now()): string {
  const diff = Math.max(0, now - ms);
  const units: [number, Intl.RelativeTimeFormatUnit][] = [
    [365 * DAY, "year"],
    [30 * DAY, "month"],
    [7 * DAY, "week"],
    [DAY, "day"],
    [HOUR, "hour"],
    [MINUTE, "minute"],
  ];
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [size, unit] of units) {
    if (diff >= size) return rtf.format(-Math.floor(diff / size), unit);
  }
  return "just now";
}

export function absoluteDate(ms: number): string {
  return new Date(ms).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatDuration(totalSecs: number): string {
  const safe = Math.max(0, Math.floor(totalSecs));
  const m = Math.floor(safe / 60);
  const s = safe % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function hostFromUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url.includes("://") ? url : `https://${url}`).hostname.replace(/^www\./, "");
  } catch {
    return null;
  }
}

export const KIND_LABELS: Record<ItemKind, string> = {
  apiKey: "API key",
  env: ".env",
  password: "Password",
  note: "Note",
};

export const KIND_SHORT_LABELS: Record<ItemKind, string> = {
  apiKey: "API keys",
  env: ".env",
  password: "Passwords",
  note: "Notes",
};

export function maskPreview(seed: string): string {
  // A stable pseudo-random mask so rows look distinct without real values.
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  const alphabet = "abcdefghijkmnopqrstuvwxyz23456789";
  let out = "";
  for (let i = 0; i < 12; i++) {
    h = (h * 1103515245 + 12345) >>> 0;
    out += alphabet[h % alphabet.length];
  }
  return `${out.slice(0, 4)}·${out.slice(4, 8)}·${out.slice(8, 12)}`;
}

export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

/** Short kind label used in dense rows ("API key", ".env", "Password", "Note"). */
export function kindLabelShort(kind: ItemKind): string {
  return KIND_LABELS[kind];
}
