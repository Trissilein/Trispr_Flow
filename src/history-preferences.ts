const DEFAULT_FONT_SIZE = 14;
const FONT_KEY = "historyFontSize_mic";

export function getHistoryFontSize(): number {
  try {
    const raw = localStorage.getItem(FONT_KEY);
    if (raw) {
      const n = Number(raw);
      if (n >= 12 && n <= 24) return n;
    }
  } catch { /* ignore */ }
  return DEFAULT_FONT_SIZE;
}

export function setHistoryFontSize(size: number): number {
  const normalized = Math.min(24, Math.max(12, Math.round(size)));
  localStorage.setItem(FONT_KEY, String(normalized));
  return normalized;
}
