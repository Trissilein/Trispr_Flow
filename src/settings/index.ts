// Settings persistence and UI rendering
import { settings } from "../state";
import * as dom from "../dom-refs";
import { applyAccentColor, DEFAULT_ACCENT_COLOR, normalizeColorHex } from "../utils";
import {
  ensureSetupDefaults,
  persistSettings,
  syncDerivedLanguageSettings,
} from "../settings-persist";
import {
  ensureContinuousDumpDefaults,
  renderContinuousDumpSettings,
} from "./continuous-dump.settings";
import { renderOverlaySettings } from "./overlay.settings";
import { renderPostProcessingSettings } from "./post-processing.settings";
import { renderTranscriptionSettings } from "./transcription.settings";
import {
  renderAIRefinementTab,
  renderOverlayHealthNote,
} from "./ai-refinement.settings";

export { persistSettings };
export { ensureContinuousDumpDefaults };

export function ensureCaptureRuntimeDefaults() {
  if (!settings) return;
  settings.ptt_hot_keepalive_ms ??= 30000;
}

export function ensureDiagnosticsDefaults() {
  if (!settings) return;
  settings.diagnostic_logging_enabled ??= false;
}

export function renderSettings() {
  if (!settings) return;
  ensureContinuousDumpDefaults();
  ensureSetupDefaults();
  syncDerivedLanguageSettings();
  renderTranscriptionSettings();
  renderContinuousDumpSettings();
  renderOverlaySettings();
  // Apply accent color
  settings.accent_color = normalizeColorHex(settings.accent_color, DEFAULT_ACCENT_COLOR);
  if (dom.accentColor) dom.accentColor.value = settings.accent_color;
  applyAccentColor(settings.accent_color);
  renderOverlayHealthNote();
  renderPostProcessingSettings();
  renderAIRefinementTab();
}
