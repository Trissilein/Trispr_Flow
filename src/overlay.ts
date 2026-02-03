import { emit, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type OverlayState = "idle" | "toggle-idle" | "recording" | "transcribing";

type OverlaySettings = {
  color: string;
  min_radius: number;
  max_radius: number;
  rise_ms: number;
  fall_ms: number;
  opacity_inactive: number;
  opacity_active: number;
};

type AppSettings = {
  overlay_color: string;
  overlay_min_radius: number;
  overlay_max_radius: number;
  overlay_rise_ms: number;
  overlay_fall_ms: number;
  overlay_opacity_inactive: number;
  overlay_opacity_active: number;
};

const root = document.getElementById("overlay-root") as HTMLDivElement | null;
const ring = document.getElementById("overlay-ring") as HTMLDivElement | null;
const dot = document.getElementById("overlay-dot") as HTMLDivElement | null;
const debug = document.getElementById("overlay-debug") as HTMLDivElement | null;

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
  opacity_inactive: 0.2,
  opacity_active: 0.8,
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
    opacity_inactive: clamp(next.opacity_inactive ?? settings.opacity_inactive, 0.05, 1),
    opacity_active: clamp(next.opacity_active ?? settings.opacity_active, 0.05, 1),
  };
  if (settings.max_radius < settings.min_radius) {
    settings.max_radius = settings.min_radius;
  }
  if (settings.opacity_active < settings.opacity_inactive) {
    settings.opacity_active = settings.opacity_inactive;
  }
  if (root) {
    root.style.setProperty("--dot-color", settings.color);
    root.style.setProperty("--overlay-opacity-idle", settings.opacity_inactive.toString());
    root.style.setProperty("--overlay-opacity-active", settings.opacity_active.toString());
  }
  // Update ring to show max_radius boundary
  updateRing();
}

function updateRing() {
  if (!ring) return;
  const ringSize = settings.max_radius * 2;
  ring.style.width = `${ringSize}px`;
  ring.style.height = `${ringSize}px`;
}

function applySettingsFromApp(payload: Partial<AppSettings>) {
  applySettings({
    color: payload.overlay_color ?? settings.color,
    min_radius: payload.overlay_min_radius ?? settings.min_radius,
    max_radius: payload.overlay_max_radius ?? settings.max_radius,
    rise_ms: payload.overlay_rise_ms ?? settings.rise_ms,
    fall_ms: payload.overlay_fall_ms ?? settings.fall_ms,
    opacity_inactive: payload.overlay_opacity_inactive ?? settings.opacity_inactive,
    opacity_active: payload.overlay_opacity_active ?? settings.opacity_active,
  });
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

function updateDebug(level: number, radius: number) {
  if (!debug) return;
  const opacity = root ? getComputedStyle(root).opacity : "?";
  debug.textContent = `L:${level.toFixed(2)} R:${radius.toFixed(0)} min:${settings.min_radius} max:${settings.max_radius} op:${opacity}`;
}

function updateDot(level: number) {
  if (!dot) return;
  const clamped = clamp(level, 0, 1);
  const radius = settings.min_radius + (settings.max_radius - settings.min_radius) * clamped;
  const size = Math.max(2, radius * 2);
  dot.style.width = `${size}px`;
  dot.style.height = `${size}px`;
  // No box-shadow - clean look with inner dot + outer ring
  updateDebug(clamped, radius);
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
  console.log("[overlay] state event:", event.payload);
  updateOverlay(event.payload);
});

listen<number>("overlay:level", (event) => {
  console.log("[overlay] level event:", event.payload, "state:", currentState);
  if (currentState !== "recording") return;
  targetLevel = clamp(event.payload ?? 0, 0, 1);
});

listen<OverlaySettings>("overlay:settings", (event) => {
  console.log("[overlay] settings event:", event.payload);
  applySettings(event.payload);
});

listen<AppSettings>("settings-changed", (event) => {
  applySettingsFromApp(event.payload);
});

invoke<AppSettings>("get_settings")
  .then((payload) => {
    console.log("[overlay] initial settings from app:", payload);
    applySettingsFromApp(payload);
  })
  .catch((err) => {
    console.warn("[overlay] failed to get initial settings:", err);
  });

emit("overlay:ready").catch(() => {
  // ignore if not available
});

applySettings(settings);
updateOverlay("idle");
requestAnimationFrame(tick);
