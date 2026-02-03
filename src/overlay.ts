import { listen } from "@tauri-apps/api/event";

type OverlayState = "idle" | "toggle-idle" | "recording" | "transcribing";

type OverlaySettings = {
  color: string;
  min_radius: number;
  max_radius: number;
  rise_ms: number;
  fall_ms: number;
};

const root = document.getElementById("overlay-root") as HTMLDivElement | null;
const dot = document.getElementById("overlay-dot") as HTMLDivElement | null;

let currentState: OverlayState = "idle";
let targetLevel = 0;
let currentLevel = 0;
let lastFrame = performance.now();

let settings: OverlaySettings = {
  color: "#ff3d2e",
  min_radius: 8,
  max_radius: 24,
  rise_ms: 80,
  fall_ms: 160,
};

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function applySettings(next: OverlaySettings) {
  settings = {
    color: next.color || settings.color,
    min_radius: clamp(next.min_radius ?? settings.min_radius, 4, 64),
    max_radius: clamp(next.max_radius ?? settings.max_radius, 8, 96),
    rise_ms: clamp(next.rise_ms ?? settings.rise_ms, 20, 2000),
    fall_ms: clamp(next.fall_ms ?? settings.fall_ms, 20, 2000),
  };
  if (settings.max_radius < settings.min_radius) {
    settings.max_radius = settings.min_radius;
  }
  if (root) {
    root.style.setProperty("--dot-color", settings.color);
  }
}

function hexToRgb(hex: string) {
  const cleaned = hex.replace("#", "");
  if (cleaned.length === 3) {
    const r = parseInt(cleaned[0] + cleaned[0], 16);
    const g = parseInt(cleaned[1] + cleaned[1], 16);
    const b = parseInt(cleaned[2] + cleaned[2], 16);
    return { r, g, b };
  }
  if (cleaned.length === 6) {
    const r = parseInt(cleaned.slice(0, 2), 16);
    const g = parseInt(cleaned.slice(2, 4), 16);
    const b = parseInt(cleaned.slice(4, 6), 16);
    return { r, g, b };
  }
  return { r: 255, g: 61, b: 46 };
}

function updateDot(level: number) {
  if (!dot) return;
  const clamped = clamp(level, 0, 1);
  const radius = settings.min_radius + (settings.max_radius - settings.min_radius) * clamped;
  const size = Math.max(2, radius * 2);
  const glow = Math.max(4, radius * 0.8);
  const { r, g, b } = hexToRgb(settings.color);
  dot.style.width = `${size}px`;
  dot.style.height = `${size}px`;
  dot.style.boxShadow = `0 0 0 ${glow}px rgba(${r}, ${g}, ${b}, 0.35), 0 0 ${Math.max(8, radius)}px rgba(${r}, ${g}, ${b}, 0.55)`;
}

function updateOverlay(state: OverlayState) {
  if (!root) return;
  const normalized = state === "toggle-idle" ? "idle" : state;
  currentState = normalized;
  root.dataset.state = normalized;
  if (normalized !== "recording") {
    targetLevel = 0;
  }
}

function tick(now: number) {
  const dt = Math.max(0, now - lastFrame);
  lastFrame = now;

  const tau = currentLevel < targetLevel ? settings.rise_ms : settings.fall_ms;
  const denom = Math.max(1, tau);
  const alpha = 1 - Math.exp(-dt / denom);
  currentLevel = currentLevel + (targetLevel - currentLevel) * alpha;
  updateDot(currentLevel);
  requestAnimationFrame(tick);
}

listen<OverlayState>("overlay:state", (event) => {
  updateOverlay(event.payload);
});

listen<number>("overlay:level", (event) => {
  if (currentState !== "recording") return;
  targetLevel = clamp(event.payload ?? 0, 0, 1);
});

listen<OverlaySettings>("overlay:settings", (event) => {
  applySettings(event.payload);
});

applySettings(settings);
updateOverlay("idle");
requestAnimationFrame(tick);
