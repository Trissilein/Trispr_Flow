import type { HistoryTab } from "./types";

const DEFAULT_ALIASES = { mic: "Input", system: "System audio" } as const;
const DEFAULT_FONT_SIZE = 14;
const ALIAS_KEY = "historyAliases";
const FONT_KEY_PREFIX = "historyFontSize_";

export function getHistoryAliases(): { mic: string; system: string } {
  try {
    const raw = localStorage.getItem(ALIAS_KEY);
    if (raw) return { ...DEFAULT_ALIASES, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return { ...DEFAULT_ALIASES };
}

export function setHistoryAlias(key: "mic" | "system", value: string): string {
  const normalized = value.trim().slice(0, 32) || DEFAULT_ALIASES[key];
  const aliases = getHistoryAliases();
  aliases[key] = normalized;
  localStorage.setItem(ALIAS_KEY, JSON.stringify(aliases));
  return normalized;
}

export function getHistoryFontSize(tab: HistoryTab): number {
  try {
    const raw = localStorage.getItem(`${FONT_KEY_PREFIX}${tab}`);
    if (raw) {
      const n = Number(raw);
      if (n >= 12 && n <= 24) return n;
    }
  } catch { /* ignore */ }
  return DEFAULT_FONT_SIZE;
}

export function setHistoryFontSize(tab: HistoryTab, size: number): number {
  const normalized = Math.min(24, Math.max(12, Math.round(size)));
  localStorage.setItem(`${FONT_KEY_PREFIX}${tab}`, String(normalized));
  return normalized;
}

export function resolveSourceAliasKey(source: string): "mic" | "system" | null {
  if (source === "mic") return "mic";
  if (source === "output" || source === "system") return "system";
  return null;
}

export function resolveSourceLabel(source: string): string {
  const key = resolveSourceAliasKey(source);
  return key ? getHistoryAliases()[key] : source;
}
