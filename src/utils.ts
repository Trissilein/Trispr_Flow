/**
 * Escape HTML special characters to prevent XSS when inserting into innerHTML.
 */
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// ---------------------------------------------------------------------------
// Accent color theming
// ---------------------------------------------------------------------------

export const DEFAULT_ACCENT_COLOR = "#4be0d4";

/** Validate a hex color string, falling back to a given default. */
export function normalizeColorHex(value: string | undefined, fallback: string): string {
  const trimmed = (value || "").trim();
  return /^#[0-9a-fA-F]{6}$/.test(trimmed) ? trimmed : fallback;
}

/** Parse a #RRGGBB hex string into [r, g, b] (0–255). */
function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** [r,g,b] (0–255) → [h (0–360), s (0–1), l (0–1)] */
function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  if (max === r)      h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
  else if (max === g) h = ((b - r) / d + 2) / 6;
  else                h = ((r - g) / d + 4) / 6;
  return [h * 360, s, l];
}

/** [h (0–360), s (0–1), l (0–1)] → [r, g, b] (0–255) */
function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  h /= 360;
  if (s === 0) { const v = Math.round(l * 255); return [v, v, v]; }
  const hue2rgb = (p: number, q: number, t: number) => {
    if (t < 0) t += 1; if (t > 1) t -= 1;
    if (t < 1/6) return p + (q - p) * 6 * t;
    if (t < 1/2) return q;
    if (t < 2/3) return p + (q - p) * (2/3 - t) * 6;
    return p;
  };
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  return [
    Math.round(hue2rgb(p, q, h + 1/3) * 255),
    Math.round(hue2rgb(p, q, h) * 255),
    Math.round(hue2rgb(p, q, h - 1/3) * 255),
  ];
}

/** [r, g, b] (0–255) → "#rrggbb" */
function rgbToHex(r: number, g: number, b: number): string {
  return "#" + [r, g, b].map((c) => c.toString(16).padStart(2, "0")).join("");
}

/**
 * Given the bright highlight hex (#RRGGBB), derive the dark base variant
 * by reducing lightness by 21 percentage points (clamped to min 8%).
 * Returns both variants as RGB triplets and the dark hex string.
 */
function deriveAccentPair(brightHex: string): {
  brightRgb: [number, number, number];
  darkRgb: [number, number, number];
  darkHex: string;
} {
  const brightRgb = hexToRgb(brightHex);
  const [h, s, l] = rgbToHsl(...brightRgb);
  const darkL = Math.max(0.08, l - 0.21);
  const darkRgb = hslToRgb(h, s, darkL);
  return { brightRgb, darkRgb, darkHex: rgbToHex(...darkRgb) };
}

/**
 * Apply accent color CSS variables to :root for live theming.
 * Sets --accent-2, --accent-2-rgb, --accent-2-bright, --accent-2-bright-rgb.
 * Call on page load and on every color picker "input" event.
 * Skips DOM writes when the color hasn't changed.
 */
let _lastAppliedAccent = "";
export function applyAccentColor(brightHex: string): void {
  if (brightHex === _lastAppliedAccent) return;
  _lastAppliedAccent = brightHex;
  const { brightRgb, darkRgb, darkHex } = deriveAccentPair(brightHex);
  const root = document.documentElement.style;
  root.setProperty("--accent-2", darkHex);
  root.setProperty("--accent-2-rgb", darkRgb.join(", "));
  root.setProperty("--accent-2-bright", brightHex);
  root.setProperty("--accent-2-bright-rgb", brightRgb.join(", "));
}
