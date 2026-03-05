import type { HistoryTab } from "./types";
import { settings } from "./state";

const DEFAULT_ALIASES = { mic: "Input", system: "System audio" } as const;
const DEFAULT_FONT_SIZE = 14;
const ALIAS_KEY = "historyAliases";
const FONT_KEY_PREFIX = "historyFontSize_";

function normalizeAlias(value: string | null | undefined, fallback: string): string {
  const trimmed = (value ?? "").trim();
  return trimmed.slice(0, 32) || fallback;
}

function readStoredAliases(): Partial<{ mic: string; system: string }> | null {
  try {
    const raw = localStorage.getItem(ALIAS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<{ mic: string; system: string }>;
      if (parsed && typeof parsed === "object") return parsed;
    }
  } catch {
    // ignore malformed localStorage
  }
  return null;
}

function writeStoredAliases(aliases: { mic: string; system: string }): void {
  try {
    localStorage.setItem(ALIAS_KEY, JSON.stringify(aliases));
  } catch {
    // ignore storage write failures
  }
}

export function getHistoryAliases(): { mic: string; system: string } {
  const stored = readStoredAliases();
  const storedAliases = stored
    ? {
      mic: normalizeAlias(stored.mic, DEFAULT_ALIASES.mic),
      system: normalizeAlias(stored.system, DEFAULT_ALIASES.system),
    }
    : null;

  if (settings) {
    const settingsAliases = {
      mic: normalizeAlias(settings.history_alias_mic, DEFAULT_ALIASES.mic),
      system: normalizeAlias(settings.history_alias_system, DEFAULT_ALIASES.system),
    };
    const settingsAreDefaults =
      settingsAliases.mic === DEFAULT_ALIASES.mic &&
      settingsAliases.system === DEFAULT_ALIASES.system;
    if (storedAliases && settingsAreDefaults) {
      return storedAliases;
    }
    return settingsAliases;
  }

  if (storedAliases) {
    return storedAliases;
  }

  return { ...DEFAULT_ALIASES };
}

export function setHistoryAlias(key: "mic" | "system", value: string): string {
  const normalized = normalizeAlias(value, DEFAULT_ALIASES[key]);
  const aliases = getHistoryAliases();
  aliases[key] = normalized;
  writeStoredAliases(aliases);
  if (settings) {
    settings.history_alias_mic = aliases.mic;
    settings.history_alias_system = aliases.system;
  }
  return normalized;
}

export function syncHistoryAliasesIntoSettings(): boolean {
  if (!settings) return false;
  const aliases = getHistoryAliases();
  let changed = false;
  if (settings.history_alias_mic !== aliases.mic) {
    settings.history_alias_mic = aliases.mic;
    changed = true;
  }
  if (settings.history_alias_system !== aliases.system) {
    settings.history_alias_system = aliases.system;
    changed = true;
  }
  writeStoredAliases(aliases);
  return changed;
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
