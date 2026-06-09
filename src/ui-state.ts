// UI state management for status and hero sections

import type { RecordingState, TranscriptionGpuActivityEvent } from "./types";
import {
  settings,
  models,
  dynamicSustainThreshold,
  runtimeDiagnostics,
  isRefinementEnabled,
} from "./state";
import * as dom from "./dom-refs";
import { updateRecordingStatus, updateRefiningStatus, updateTranscribeStatus } from "./accessibility";
import { thresholdToPercent, formatModelName, formatPresetLabel, formatVram } from "./ui-helpers";
import { BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS } from "./refinement-prompts";
import {
  applyFeedbackSettings,
  getFeedbackView,
  transitionCaptureRuntime,
  transitionTranscribeRuntime,
} from "./feedback-state";

let refiningRuntimeActive = false;
let ollamaModelState: "cold" | "loading" | "warm" = "cold";
let gpuRuntimeState: "idle" | "active" | "cpu" | "error" = "idle";
let gpuAccelerator: "gpu" | "cpu" = "cpu";
let gpuBackend = "unknown";
const GPU_STATUS_STORAGE_KEY = "trispr_gpu_status_snapshot_v1";

type GpuStatusSnapshot = {
  accelerator: "gpu" | "cpu";
  backend: string;
};

function loadGpuStatusSnapshot(): void {
  if (typeof window === "undefined") return;
  try {
    const raw = window.localStorage.getItem(GPU_STATUS_STORAGE_KEY);
    if (!raw) return;
    const parsed = JSON.parse(raw) as Partial<GpuStatusSnapshot>;
    if (
      (parsed?.accelerator === "gpu" || parsed?.accelerator === "cpu")
      && typeof parsed.backend === "string"
      && parsed.backend.trim().length > 0
    ) {
      gpuAccelerator = parsed.accelerator;
      gpuBackend = parsed.backend;
    }
  } catch {
    // ignore
  }
}

function persistGpuStatusSnapshot(): void {
  if (typeof window === "undefined") return;
  try {
    const snapshot: GpuStatusSnapshot = {
      accelerator: gpuAccelerator,
      backend: gpuBackend,
    };
    window.localStorage.setItem(GPU_STATUS_STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // ignore
  }
}

loadGpuStatusSnapshot();

function selectedWhisperBackendPreference(): "cuda" | "vulkan" {
  const configured = (settings?.local_backend_preference ?? "").trim().toLowerCase();
  if (configured === "cuda" || configured === "vulkan") {
    return configured;
  }
  const runtime = (runtimeDiagnostics?.whisper?.backend_selected ?? gpuBackend).trim().toLowerCase();
  if (runtime === "vulkan") {
    return "vulkan";
  }
  return "cuda";
}

function renderFeedbackIndicators() {
  const { capture, transcribe } = getFeedbackView();
  const selectedBackend = selectedWhisperBackendPreference();

  if (dom.statusDot) {
    dom.statusDot.dataset.state = capture.activityState;
  }
  if (dom.statusLabel) {
    dom.statusLabel.textContent = capture.labelText;
  }
  if (dom.recordingPill) {
    dom.recordingPill.classList.toggle("status-pill--enabled", capture.enabled);
    dom.recordingPill.classList.toggle("status-pill--disabled", !capture.enabled);
  }
  if (dom.statusMessage) {
    dom.statusMessage.textContent = "";
  }
  updateRecordingStatus(capture.labelState);

  if (dom.transcribeStatusDot) {
    dom.transcribeStatusDot.dataset.state = transcribe.activityState;
  }
  if (dom.transcribeStatusLabel) {
    dom.transcribeStatusLabel.textContent = transcribe.labelText;
  }
  if (dom.transcribePill) {
    dom.transcribePill.classList.toggle("status-pill--enabled", transcribe.enabled);
    dom.transcribePill.classList.toggle("status-pill--disabled", !transcribe.enabled);
  }
  updateTranscribeStatus(transcribe.labelState);

  const refiningEnabled = isRefinementEnabled();
  const refiningState: "disabled" | "idle" | "refining" = !refiningEnabled
    ? "disabled"
    : refiningRuntimeActive
      ? "refining"
      : "idle";
  // Pipeline "Refine" dot: transient activity only.
  if (dom.refiningStatusDot) {
    dom.refiningStatusDot.dataset.state = refiningState;
  }
  updateRefiningStatus(refiningState);

  // Refinement engine row: model readiness merged with active-inference state.
  const refineRowVisible =
    refiningEnabled && (settings?.ai_fallback?.provider ?? "ollama") === "ollama";
  if (dom.engineRefineRow) {
    dom.engineRefineRow.hidden = !refineRowVisible;
  }
  if (refineRowVisible) {
    const engineState: "cold" | "loading" | "warm" | "refining" = refiningRuntimeActive
      ? "refining"
      : ollamaModelState;
    if (dom.ollamaModelDot) {
      dom.ollamaModelDot.dataset.state =
        engineState === "warm"
          ? "model-ready"
          : engineState === "refining"
            ? "refining"
            : `model-${engineState}`;
    }
    if (dom.engineRefineLabel) {
      dom.engineRefineLabel.textContent = {
        cold: "Cold",
        loading: "Loading…",
        warm: "Ready",
        refining: "Refining…",
      }[engineState];
    }
  }

  // Whisper engine dot (Backend acceleration state).
  const gpuDotState =
    gpuRuntimeState === "active"
      ? "gpu-active"
      : gpuRuntimeState === "cpu"
        ? "cpu"
        : gpuRuntimeState === "error"
          ? "error"
          : "idle";
  if (dom.gpuStatusDot) {
    dom.gpuStatusDot.dataset.state = gpuDotState;
  }
  if (dom.gpuBackendCudaBtn && dom.gpuBackendVulkanBtn) {
    const cudaActive = selectedBackend === "cuda";
    dom.gpuBackendCudaBtn.classList.toggle("is-active", cudaActive);
    dom.gpuBackendVulkanBtn.classList.toggle("is-active", !cudaActive);
    dom.gpuBackendCudaBtn.setAttribute("aria-pressed", cudaActive ? "true" : "false");
    dom.gpuBackendVulkanBtn.setAttribute("aria-pressed", !cudaActive ? "true" : "false");
  }
}

export function setCaptureStatus(state: RecordingState) {
  transitionCaptureRuntime(state);
  renderFeedbackIndicators();
}

export function setTranscribeStatus(state: RecordingState) {
  transitionTranscribeRuntime(state);
  renderFeedbackIndicators();
}

export function setRefiningActive(active: boolean) {
  refiningRuntimeActive = active;
  renderFeedbackIndicators();
}

export function setOllamaModelState(state: "cold" | "loading" | "warm") {
  ollamaModelState = state;
  renderFeedbackIndicators();
}

export interface GpuStats {
  util_pct: number | null;
  vram_used_gb: number;
  vram_total_gb: number;
  whisper_vram_gb: number | null;
  refine_vram_gb: number | null;
}

export function setGpuStats(stats: GpuStats) {
  if (dom.gpuUtil) {
    dom.gpuUtil.textContent = stats.util_pct != null ? `GPU ${stats.util_pct}%` : "GPU —";
  }
  if (dom.gpuVramTotal) {
    dom.gpuVramTotal.textContent =
      stats.vram_total_gb > 0
        ? `${stats.vram_used_gb.toFixed(1)} / ${stats.vram_total_gb.toFixed(1)} GB`
        : "— / — GB";
  }
  if (dom.engineWhisperVram) {
    dom.engineWhisperVram.textContent = formatVram(stats.whisper_vram_gb);
  }
  if (dom.engineRefineVram) {
    dom.engineRefineVram.textContent = formatVram(stats.refine_vram_gb);
  }
}

export function setGpuActivity(event: TranscriptionGpuActivityEvent) {
  gpuRuntimeState = event.state;
  gpuAccelerator = event.accelerator;
  gpuBackend = event.backend || "unknown";
  persistGpuStatusSnapshot();
  renderFeedbackIndicators();
}

export function renderHero() {
  if (!settings) return;
  const aiFallbackOn = isRefinementEnabled();
  const provider = settings.ai_fallback?.provider ?? "ollama";
  const executionMode = settings.ai_fallback?.execution_mode ?? "local_primary";
  const isOnlineRefinement = aiFallbackOn && executionMode === "online_fallback" && provider !== "ollama";
  const configuredModel = settings.ai_fallback?.model?.trim() || "";
  const providerModel =
    provider === "claude"
      ? settings.providers?.claude?.preferred_model?.trim() || ""
      : provider === "openai"
        ? settings.providers?.openai?.preferred_model?.trim() || ""
        : provider === "gemini"
          ? settings.providers?.gemini?.preferred_model?.trim() || ""
          : settings.providers?.ollama?.preferred_model?.trim() || "";
  const effectiveRefinementModel = configuredModel || providerModel || "No model selected";

  if (dom.dictationBadge) {
    dom.dictationBadge.textContent = isOnlineRefinement
      ? "AI Refinement (Online)"
      : "Private Mode (Offline)";
    dom.dictationBadge.classList.toggle("badge--online", isOnlineRefinement);
  }
  if (dom.engineRefineModel) {
    dom.engineRefineModel.textContent = aiFallbackOn ? formatModelName(effectiveRefinementModel) : "—";
  }
  if (dom.engineWhisperModel) {
    const active = models.find((model) => model.id === settings?.model);
    dom.engineWhisperModel.textContent = active?.label ?? settings?.model ?? "—";
  }

  renderPresetQuickMenu();
  applyFeedbackSettings(settings);
  renderFeedbackIndicators();
}

export function renderPresetQuickMenu() {
  const btn = dom.engineRefinePreset;
  const menu = dom.engineRefinePresetMenu;
  if (!btn || !menu) return;
  if (!settings) return;

  const aiFallbackOn = isRefinementEnabled();
  const activeId = settings.ai_fallback?.active_prompt_preset_id
    ?? settings.ai_fallback?.prompt_profile
    ?? "wording";

  btn.textContent = aiFallbackOn
    ? formatPresetLabel(settings.ai_fallback?.prompt_profile)
    : "Off";
  btn.dataset.state = aiFallbackOn ? "" : "off";

  menu.innerHTML = "";
  for (const opt of BUILT_IN_REFINEMENT_PROMPT_PRESET_OPTIONS) {
    const li = document.createElement("li");
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = opt.label;
    b.dataset.presetId = opt.id;
    b.setAttribute("role", "option");
    b.setAttribute("aria-selected", (opt.id === activeId && aiFallbackOn) ? "true" : "false");
    li.appendChild(b);
    menu.appendChild(li);
  }
  const sep = document.createElement("hr");
  sep.className = "preset-quick-menu-separator";
  menu.appendChild(sep);
  const noRefLi = document.createElement("li");
  const noRefBtn = document.createElement("button");
  noRefBtn.type = "button";
  noRefBtn.textContent = "No Refinement";
  noRefBtn.dataset.presetId = "__off__";
  noRefBtn.setAttribute("role", "option");
  noRefBtn.setAttribute("aria-selected", (!aiFallbackOn).toString());
  noRefLi.appendChild(noRefBtn);
  menu.appendChild(noRefLi);
}

export function updateThresholdMarkers() {
  if (dom.vadMarkerStart && settings) {
    const startPercent = thresholdToPercent(settings.vad_threshold_start);
    dom.vadMarkerStart.style.left = `${startPercent}%`;
  }
  if (dom.vadMarkerSustain) {
    const sustainPercent = thresholdToPercent(dynamicSustainThreshold);
    dom.vadMarkerSustain.style.left = `${sustainPercent}%`;
  }
}
