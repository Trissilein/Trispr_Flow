// UI utility functions for formatting and conversion

import type { ModelInfo, DownloadProgress } from "./types";
import { MODEL_DESCRIPTIONS } from "./model-descriptions";

export function getModelDescription(model: ModelInfo) {
  const entry = MODEL_DESCRIPTIONS[model.id];
  if (entry) {
    return `${entry.summary} • Speed: ${entry.speed} • Accuracy: ${entry.accuracy} • ${entry.languages}`;
  }
  if (model.source === "local" || model.source === "custom") {
    return "Custom/local model. No benchmark data available.";
  }
  return "Model details unavailable.";
}

// Shorten an Ollama model id for compact display, e.g. "qwen3.5:9b" → "Qwen3.5-9B".
export function formatModelName(raw: string | undefined | null): string {
  const value = (raw ?? "").trim();
  if (!value) return "—";
  const [name, tag] = value.split(":");
  const capName = name.charAt(0).toUpperCase() + name.slice(1);
  const sized = tag ? `${capName}-${tag.toUpperCase()}` : capName;
  return sized;
}

// Human-readable label for a refinement prompt profile.
export function formatPresetLabel(profile: string | undefined | null): string {
  const map: Record<string, string> = {
    wording: "Wording",
    summary: "Summary",
    technical_specs: "Technical Specs",
    action_items: "Action Items",
    llm_prompt: "LLM Prompt",
    custom: "Custom",
  };
  const key = (profile ?? "").trim();
  return map[key] ?? (key ? key : "—");
}

// Format a VRAM value in GB for display, e.g. 5.6 → "5.6 GB".
export function formatVram(gb: number | null | undefined): string {
  if (gb === null || gb === undefined || Number.isNaN(gb)) return "—";
  return `${gb.toFixed(1)} GB`;
}

export const VAD_DB_FLOOR = -60;

// Convert linear level (0-1) to dB (assuming 0dB = 1.0)
export function levelToDb(level: number): number {
  if (level <= 0.00001) return -100;
  return 20 * Math.log10(level);
}

// Convert linear threshold (0-1) to dB, clamped to floor.
export function thresholdToDb(threshold: number, floor = VAD_DB_FLOOR): number {
  if (threshold <= 0) return floor;
  const db = 20 * Math.log10(threshold);
  return Math.max(floor, db);
}

// Convert dB value to linear level (0-1).
export function dbToLevel(db: number): number {
  return Math.pow(10, db / 20);
}

// Convert linear threshold (0-1) to percentage position on dB scale (-60 to 0).
export function thresholdToPercent(threshold: number): number {
  const db = thresholdToDb(threshold, VAD_DB_FLOOR);
  // Scale: -60dB = 0%, 0dB = 100%
  const percent = ((db - VAD_DB_FLOOR) / (0 - VAD_DB_FLOOR)) * 100;
  return Math.max(0, Math.min(100, percent));
}

export function formatTime(timestamp: number) {
  const date = new Date(timestamp);
  const base = date.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const hundredths = Math.floor(date.getMilliseconds() / 10)
    .toString()
    .padStart(2, "0");
  return `${base}.${hundredths}`;
}

// Human-readable display labels for hotkey key-names that would otherwise be cryptic.
// The stored/backend key string is unchanged; this map is only for display.
const KEY_DISPLAY_NAMES: Record<string, string> = {
  IntlBackslash: "< >",
  IntlRo: "Ro",
  IntlYen: "¥",
  Space: "Space",
  Enter: "Enter",
  Escape: "Esc",
  Backspace: "Backspace",
  Tab: "Tab",
  Delete: "Del",
  Insert: "Ins",
  Home: "Home",
  End: "End",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  PageUp: "PgUp",
  PageDown: "PgDn",
  PrintScreen: "PrtSc",
  ScrollLock: "ScrLk",
  NumLock: "NumLk",
  CapsLock: "CapsLk",
  NumpadAdd: "Num+",
  NumpadSubtract: "Num-",
  NumpadMultiply: "Num*",
  NumpadDivide: "Num/",
  NumpadDecimal: "Num.",
  NumpadEnter: "NumEnter",
  Numpad0: "Num0", Numpad1: "Num1", Numpad2: "Num2", Numpad3: "Num3",
  Numpad4: "Num4", Numpad5: "Num5", Numpad6: "Num6", Numpad7: "Num7",
  Numpad8: "Num8", Numpad9: "Num9",
  MediaPlayPause: "⏯",
  MediaStop: "⏹",
  MediaTrackNext: "⏭",
  MediaTrackPrevious: "⏮",
  AudioVolumeUp: "Vol+",
  AudioVolumeDown: "Vol-",
  AudioVolumeMute: "Mute",
};

/** Formats a stored hotkey string for human-readable display.
 *  e.g. "Ctrl+IntlBackslash" → "Ctrl + < >"
 *  The stored value is unchanged; this is only for UI display. */
export function formatHotkeyForDisplay(hotkey: string): string {
  if (!hotkey) return "";
  return hotkey
    .split("+")
    .map(part => KEY_DISPLAY_NAMES[part] ?? part)
    .join(" + ");
}

export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  const digits = size >= 100 ? 0 : size >= 10 ? 1 : 2;
  return `${size.toFixed(digits)} ${units[unitIndex]}`;
}

export function formatSize(sizeMb: number) {
  if (sizeMb >= 1024) {
    return `${(sizeMb / 1024).toFixed(1)} GB`;
  }
  return `${sizeMb} MB`;
}

export function formatProgress(progress?: DownloadProgress) {
  if (!progress) return "";
  if (progress.total && progress.total > 0) {
    const percent = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
    return `${percent}%`;
  }
  const mb = Math.round(progress.downloaded / (1024 * 1024));
  return `${mb} MB`;
}
