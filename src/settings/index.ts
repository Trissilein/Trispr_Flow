// Settings persistence and UI rendering
import { isAssistantCoreAvailable, settings } from "../state";
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
import { renderRecordingQualitySettings } from "./recording-quality.settings";
import { renderTranscriptionSettings } from "./transcription.settings";
import { renderVoiceOutputSettings } from "./voice-output.settings";
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

function renderProductModeSettings(): void {
  if (!settings) return;
  const assistantCoreAvailable = isAssistantCoreAvailable();
  if (!assistantCoreAvailable) {
    settings.product_mode = "transcribe";
  }
  const productMode = settings.product_mode === "assistant" ? "assistant" : "transcribe";
  settings.product_mode = productMode;
  const transcribeActive = productMode === "transcribe";
  const assistantActive = productMode === "assistant";
  dom.productModeControl?.toggleAttribute("hidden", !assistantCoreAvailable);
  if (dom.productModeTranscribeBtn) {
    dom.productModeTranscribeBtn.classList.toggle("is-active", transcribeActive);
    dom.productModeTranscribeBtn.setAttribute("aria-pressed", transcribeActive ? "true" : "false");
  }
  if (dom.productModeAssistantBtn) {
    dom.productModeAssistantBtn.classList.toggle("is-active", assistantActive);
    dom.productModeAssistantBtn.setAttribute("aria-pressed", assistantActive ? "true" : "false");
    dom.productModeAssistantBtn.disabled = !assistantCoreAvailable;
  }
  dom.globalOnlineControl?.toggleAttribute(
    "hidden",
    !assistantCoreAvailable || productMode !== "assistant"
  );
}

function renderGlobalOnlineModeSettings(): void {
  if (!settings) return;
  const onlineEnabled = Boolean(settings.workflow_agent?.online_enabled);
  if (dom.globalOnlineOfflineBtn) {
    dom.globalOnlineOfflineBtn.classList.toggle("is-active", !onlineEnabled);
    dom.globalOnlineOfflineBtn.setAttribute("aria-pressed", onlineEnabled ? "false" : "true");
  }
  if (dom.globalOnlineEnabledBtn) {
    dom.globalOnlineEnabledBtn.classList.toggle("is-active", onlineEnabled);
    dom.globalOnlineEnabledBtn.setAttribute("aria-pressed", onlineEnabled ? "true" : "false");
  }
}

export function renderSettings() {
  if (!settings) return;
  ensureContinuousDumpDefaults();
  ensureSetupDefaults();
  syncDerivedLanguageSettings();
  renderProductModeSettings();
  renderGlobalOnlineModeSettings();
  renderTranscriptionSettings();
  renderContinuousDumpSettings();
  renderRecordingQualitySettings();
  renderOverlaySettings();
  renderVoiceOutputSettings();
  // Apply accent color
  settings.accent_color = normalizeColorHex(settings.accent_color, DEFAULT_ACCENT_COLOR);
  if (dom.accentColor) dom.accentColor.value = settings.accent_color;
  applyAccentColor(settings.accent_color);
  renderOverlayHealthNote();
  renderPostProcessingSettings();
  renderAIRefinementTab();
}

