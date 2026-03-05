// Overlay controlled by Rust backend via window.eval() and Tauri events.
// Settings flow:
//   1. Rust calls window.eval() with set* functions (primary path)
//   2. Tauri "settings-changed" event as backup (from save_settings)
//   3. Initial settings loaded via invoke("get_settings") on page load

const container = document.getElementById("container");
const dot = document.getElementById("dot");
const kitt = document.getElementById("kitt");
const refineIndicator = document.getElementById("refine-indicator");

// State
let isActive = false;
let opacityActive = 1.0;
let opacityInactive = 0.25;
let baseColor = "#ff3d2e";
let currentStyle = "dot";
let refiningActive = false;
let refiningEnabled = true;
let refiningPreset = "standard";
let refiningColor = "#6ec8ff";
let refiningSpeedMs = 1150;
let refiningRangePercent = 100;

// KITT settings
let kittMinWidth = 20;
let kittMaxWidth = 200;
let kittHeight = 20;

// Dot settings
let dotMinRadius = 8;
let dotMaxRadius = 24;

function updateOpacity() {
  const opacity = isActive ? opacityActive : opacityInactive;
  dot.style.opacity = opacity;
  kitt.style.opacity = opacity;
}

function updateRefiningIndicator() {
  if (!container || !refineIndicator) return;
  container.dataset.refining = refiningActive ? "on" : "off";
  container.dataset.refiningEnabled = refiningEnabled ? "true" : "false";
  container.dataset.refiningPreset = refiningPreset;
}

function normalizeRefiningPreset(value) {
  if (value === "subtle" || value === "intense") return value;
  return "standard";
}

function normalizeRefiningColor(value) {
  if (typeof value !== "string") return "#6ec8ff";
  const trimmed = value.trim();
  return /^#[0-9a-fA-F]{6}$/.test(trimmed) ? trimmed : "#6ec8ff";
}

function normalizeRefiningSpeedMs(value) {
  const numberValue = Number(value);
  if (!Number.isFinite(numberValue)) return 1150;
  return Math.max(450, Math.min(3000, Math.round(numberValue)));
}

function normalizeRefiningRange(value) {
  const numberValue = Number(value);
  if (!Number.isFinite(numberValue)) return 100;
  return Math.max(60, Math.min(180, Math.round(numberValue)));
}

function hexToRgb(hex) {
  const value = hex.replace("#", "");
  if (value.length === 3) {
    const r = parseInt(value[0] + value[0], 16);
    const g = parseInt(value[1] + value[1], 16);
    const b = parseInt(value[2] + value[2], 16);
    return { r, g, b };
  }
  if (value.length !== 6) return null;
  const r = parseInt(value.slice(0, 2), 16);
  const g = parseInt(value.slice(2, 4), 16);
  const b = parseInt(value.slice(4, 6), 16);
  return { r, g, b };
}

function updateDotGradient() {
  const rgb = hexToRgb(baseColor);
  if (!rgb) return;
  dot.style.background = `radial-gradient(circle, rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 1) 0%, rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.2) 60%, rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0) 100%)`;
}

function updateKittGradient() {
  const rgb = hexToRgb(baseColor);
  if (!rgb) return;
  const active = Math.max(0.05, Math.min(1, opacityActive));
  const inactive = Math.max(0.05, Math.min(1, opacityInactive));
  // Keep edge fade subtle so perceived width tracks configured pixel width.
  const edge = Math.max(inactive, active * 0.42);
  const shoulder = Math.max(edge, active * 0.78);
  kitt.style.background = `linear-gradient(90deg,
    rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${edge}) 0%,
    rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${shoulder}) 14%,
    rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${active}) 35%,
    rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${active}) 65%,
    rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${shoulder}) 86%,
    rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${edge}) 100%)`;
  kitt.style.boxShadow = `0 0 10px rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${Math.max(0.18, active * 0.38)})`;
}

function updateRefiningAppearance() {
  if (!refineIndicator) return;
  const rgb = hexToRgb(refiningColor);
  if (rgb) {
    refineIndicator.style.setProperty("--refine-rgb", `${rgb.r}, ${rgb.g}, ${rgb.b}`);
  }
  refineIndicator.style.setProperty("--refine-pulse-duration", `${refiningSpeedMs}ms`);
  refineIndicator.style.setProperty("--refine-range-scale", `${(refiningRangePercent / 100).toFixed(2)}`);
}

// --- Public API called from Rust via window.eval() ---

window.setOverlayState = function(state) {
  isActive = (state === "recording" || state === "transcribing");
  container.dataset.state = state;
  updateOpacity();
  updateRefiningIndicator();
};

window.setOverlayColor = function(color) {
  baseColor = color;
  updateDotGradient();
  updateKittGradient();
};

window.setOverlayOpacity = function(active, inactive) {
  opacityActive = Math.max(0.1, Math.min(1, active));
  opacityInactive = Math.max(0.05, Math.min(1, inactive));
  updateOpacity();
  updateDotGradient();
  updateKittGradient();
};

window.setOverlayStyle = function(style) {
  currentStyle = style;
  container.dataset.style = style;
};

window.setOverlayRefining = function(active) {
  refiningActive = Boolean(active);
  updateRefiningIndicator();
};

window.setOverlayRefiningEnabled = function(enabled) {
  refiningEnabled = Boolean(enabled);
  updateRefiningIndicator();
};

window.setOverlayRefiningPreset = function(preset) {
  refiningPreset = normalizeRefiningPreset(preset);
  updateRefiningIndicator();
};

window.setOverlayRefiningAppearance = function(color, speedMs, rangePercent) {
  refiningColor = normalizeRefiningColor(color);
  refiningSpeedMs = normalizeRefiningSpeedMs(speedMs);
  refiningRangePercent = normalizeRefiningRange(rangePercent);
  updateRefiningAppearance();
};

window.setKittDimensions = function(minWidth, maxWidth, height) {
  const parsedMin = Number(minWidth);
  const parsedMax = Number(maxWidth);
  const parsedHeight = Number(height);
  const normalizedMin = Number.isFinite(parsedMin) ? parsedMin : 20;
  const normalizedMax = Number.isFinite(parsedMax) ? parsedMax : 200;
  const normalizedHeight = Number.isFinite(parsedHeight) ? parsedHeight : 20;

  kittMinWidth = Math.max(4, Math.min(10000, normalizedMin));
  kittMaxWidth = Math.max(50, Math.min(20000, normalizedMax));
  if (kittMaxWidth < kittMinWidth) {
    kittMaxWidth = Math.max(50, kittMinWidth);
  }
  kittHeight = Math.max(8, Math.min(400, normalizedHeight));
  kitt.style.height = kittHeight + "px";
  kitt.style.minWidth = kittMinWidth + "px";
  kitt.style.maxWidth = kittMaxWidth + "px";
  kitt.style.width = kittMinWidth + "px";
};

window.setDotDimensions = function(minRadius, maxRadius) {
  const parsedMin = Number(minRadius);
  const parsedMax = Number(maxRadius);
  const normalizedMin = Number.isFinite(parsedMin) ? parsedMin : 16;
  const normalizedMax = Number.isFinite(parsedMax) ? parsedMax : 64;

  dotMinRadius = Math.max(4, Math.min(5000, normalizedMin));
  dotMaxRadius = Math.max(8, Math.min(10000, normalizedMax));
  if (dotMaxRadius < dotMinRadius) {
    dotMaxRadius = dotMinRadius;
  }
  const size = dotMinRadius * 2;
  dot.style.width = size + "px";
  dot.style.height = size + "px";
};

window.setOverlayLevel = function(level) {
  const clamped = Math.max(0, Math.min(1, level));
  if (currentStyle === "kitt") {
    const widthRaw = kittMinWidth + (kittMaxWidth - kittMinWidth) * clamped;
    const width = Math.max(kittMinWidth, Math.min(kittMaxWidth, widthRaw));
    kitt.style.width = width + "px";
  } else {
    const radius = dotMinRadius + (dotMaxRadius - dotMinRadius) * clamped;
    const size = Math.max(2, radius * 2);
    dot.style.width = size + "px";
    dot.style.height = size + "px";
  }
};

// --- Initialization ---

updateOpacity();
updateDotGradient();
updateKittGradient();
updateRefiningIndicator();
updateRefiningAppearance();

// Signal readiness to Rust backend
if (window.__TAURI__?.event?.emit) {
  window.__TAURI__.event.emit("overlay:ready").catch(() => {});
}

// Apply full settings payload from app settings object
function applySettingsPayload(payload) {
  if (!payload) return;
  const style = payload.overlay_style === "kitt" ? "kitt" : "dot";
  const isKitt = style === "kitt";
  const color = isKitt ? (payload.overlay_kitt_color || payload.overlay_color) : payload.overlay_color;
  const activeOpacity = isKitt
    ? (payload.overlay_kitt_opacity_active ?? payload.overlay_opacity_active)
    : payload.overlay_opacity_active;
  const inactiveOpacity = isKitt
    ? (payload.overlay_kitt_opacity_inactive ?? payload.overlay_opacity_inactive)
    : payload.overlay_opacity_inactive;
  const minWidth = payload.overlay_kitt_min_width ?? kittMinWidth;
  const maxWidth = payload.overlay_kitt_max_width ?? kittMaxWidth;
  const height = payload.overlay_kitt_height ?? kittHeight;
  const minRadius = payload.overlay_min_radius ?? dotMinRadius;
  const maxRadius = payload.overlay_max_radius ?? dotMaxRadius;
  const refiningIndicatorEnabled = payload.overlay_refining_indicator_enabled;
  const refiningIndicatorPreset = normalizeRefiningPreset(payload.overlay_refining_indicator_preset);
  const refiningIndicatorColor = normalizeRefiningColor(payload.overlay_refining_indicator_color);
  const refiningIndicatorSpeedMs = normalizeRefiningSpeedMs(payload.overlay_refining_indicator_speed_ms);
  const refiningIndicatorRange = normalizeRefiningRange(payload.overlay_refining_indicator_range);
  if (color) {
    window.setOverlayColor(color);
  }
  if (typeof activeOpacity === "number" && typeof inactiveOpacity === "number") {
    window.setOverlayOpacity(activeOpacity, inactiveOpacity);
  }
  window.setOverlayStyle(style);
  window.setKittDimensions(minWidth, maxWidth, height);
  window.setDotDimensions(minRadius, maxRadius);
  if (typeof refiningIndicatorEnabled === "boolean") {
    window.setOverlayRefiningEnabled(refiningIndicatorEnabled);
  }
  window.setOverlayRefiningPreset(refiningIndicatorPreset);
  window.setOverlayRefiningAppearance(
    refiningIndicatorColor,
    refiningIndicatorSpeedMs,
    refiningIndicatorRange
  );
  // Position is handled by Rust via window.set_position() - no JS positioning needed
}

// Load initial settings from backend
const invoke = window.__TAURI__?.core?.invoke;
if (invoke) {
  invoke("get_settings")
    .then(applySettingsPayload)
    .catch(() => {});
}

// Listen for settings changes (backup path - primary is window.eval from Rust)
const listen = window.__TAURI__?.event?.listen;
if (listen) {
  listen("overlay:state", (event) => {
    const payload = event?.payload;
    if (!payload) return;
    if (typeof payload === "string") {
      window.setOverlayState(payload);
    } else if (typeof payload.state === "string") {
      window.setOverlayState(payload.state);
    }
  }).catch(() => {});

  listen("overlay:refining", (event) => {
    window.setOverlayRefining(Boolean(event?.payload));
  }).catch(() => {});

  listen("overlay:settings", (event) => {
    const payload = event?.payload;
    if (!payload) return;
    if (payload.style === "kitt" || payload.style === "dot") {
      applySettingsPayload({
        overlay_style: payload.style,
        overlay_color: payload.color,
        overlay_min_radius: payload.min_radius,
        overlay_max_radius: payload.max_radius,
        overlay_opacity_inactive: payload.opacity_inactive,
        overlay_opacity_active: payload.opacity_active,
        overlay_kitt_min_width: payload.kitt_min_width,
        overlay_kitt_max_width: payload.kitt_max_width,
        overlay_kitt_height: payload.kitt_height,
        overlay_refining_indicator_enabled: payload.refining_indicator_enabled,
        overlay_refining_indicator_preset: payload.refining_indicator_preset,
        overlay_refining_indicator_color: payload.refining_indicator_color,
        overlay_refining_indicator_speed_ms: payload.refining_indicator_speed_ms,
        overlay_refining_indicator_range: payload.refining_indicator_range,
      });
    }
  }).catch(() => {});

  listen("settings-changed", (event) => {
    applySettingsPayload(event?.payload);
  }).catch(() => {});
}
