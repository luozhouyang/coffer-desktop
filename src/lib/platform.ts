/** True on macOS-like platforms (Tauri or a browser on macOS). */
export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return true;
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.platform ??
    navigator.userAgent;
  return /mac|iphone|ipad|ipod/i.test(platform);
}

export const MOD_KEY = isMacPlatform() ? "⌘" : "Ctrl+";
export const MOD_RETURN = isMacPlatform() ? "⌘↵" : "Ctrl+Enter";
export const MOD_SAVE = isMacPlatform() ? "⌘S" : "Ctrl+S";
