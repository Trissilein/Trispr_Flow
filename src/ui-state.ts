// UI state management for status and hero sections

import type { RecordingState, TranscriptionGpuActivityEvent } from "./types";
import {
  settings,
  devices,
  outputDevices,
  models,
  dynamicSustainThreshold
} from "./state";
import * as dom from "./dom-refs";
import { updateRecordingStatus, updateRefiningStatus, updateTranscribeStatus } from "./accessibility";
import { thresholdToPercent } from "./ui-helpers";
import {
  applyFeedbackSettings,
  getFeedbackView,
  transitionCaptureRuntime,
  transitionTranscribeRuntime,
} from "./feedback-state";

let refiningRuntimeActive = false;
let gpuRuntimeState: "idle" | "active" | "cpu" | "error" = "idle";
let gpuAccelerator: "gpu" | "cpu" = "cpu";
let gpuBackend = "unknown";
let gpuKnown = false;
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
      gpuKnown = true;
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

function prettifyGpuBackend(backend: string): string {
  const normalized = backend.trim().toLowerCase();
  if (!normalized) return "Unknown";
  if (normalized === "cuda") return "CUDA";
  if (normalized === "vulkan") return "Vulkan";
  if (normalized === "cpu") return "CPU";
  return normalized.toUpperCase();
}

function renderFeedbackIndicators() {
  const { capture, transcribe } = getFeedbackView();

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
  if (dom.transcribeStatusPill) {
    dom.transcribeStatusPill.textContent = transcribe.enabled ? "Enabled" : "Disabled";
    dom.transcribeStatusPill.classList.toggle("status-pill--enabled", transcribe.enabled);
    dom.transcribeStatusPill.classList.toggle("status-pill--disabled", !transcribe.enabled);
  }
  updateTranscribeStatus(transcribe.labelState);

  const refiningEnabled = Boolean(settings?.ai_fallback?.enabled);
  const refiningState: "disabled" | "idle" | "refining" = !refiningEnabled
    ? "disabled"
    : refiningRuntimeActive
      ? "refining"
      : "idle";
  if (dom.refiningStatusDot) {
    dom.refiningStatusDot.dataset.state = refiningState;
  }
  if (dom.refiningStatusLabel) {
    const label =
      refiningState === "refining"
        ? "Active"
        : refiningState === "disabled"
          ? "Deactivated"
          : "Idle";
    dom.refiningStatusLabel.textContent = `Refining: ${label}`;
  }
  if (dom.refiningPill) {
    dom.refiningPill.classList.toggle("status-pill--enabled", refiningEnabled);
    dom.refiningPill.classList.toggle("status-pill--disabled", !refiningEnabled);
  }
  updateRefiningStatus(refiningState);

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
  if (dom.gpuStatusLabel) {
    if (!gpuKnown && gpuRuntimeState === "idle") {
      dom.gpuStatusLabel.textContent = "GPU: Waiting for first run";
    } else if (gpuRuntimeState === "active") {
      dom.gpuStatusLabel.textContent = `GPU: Active (${prettifyGpuBackend(gpuBackend)})`;
    } else if (gpuRuntimeState === "cpu") {
      dom.gpuStatusLabel.textContent = "GPU: CPU mode";
    } else if (gpuRuntimeState === "error") {
      dom.gpuStatusLabel.textContent = "GPU: Runtime error";
    } else {
      dom.gpuStatusLabel.textContent =
        gpuAccelerator === "gpu"
          ? `GPU: Idle (${prettifyGpuBackend(gpuBackend)})`
          : "GPU: Idle (CPU mode)";
    }
  }
  if (dom.gpuPill) {
    const gpuEnabled = gpuKnown && (gpuAccelerator === "gpu" || gpuRuntimeState === "active");
    dom.gpuPill.classList.toggle("status-pill--enabled", gpuEnabled);
    dom.gpuPill.classList.toggle("status-pill--disabled", !gpuEnabled);
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

export function setGpuActivity(event: TranscriptionGpuActivityEvent) {
  gpuRuntimeState = event.state;
  gpuAccelerator = event.accelerator;
  gpuBackend = event.backend || "unknown";
  gpuKnown = true;
  persistGpuStatusSnapshot();
  renderFeedbackIndicators();
}

export function renderHero() {
  if (!settings) return;
  const aiFallbackOn = Boolean(settings.ai_fallback?.enabled);
  const provider = settings.ai_fallback?.provider ?? "ollama";
  const executionMode = settings.ai_fallback?.execution_mode ?? "local_primary";
  const isOnlineRefinement = aiFallbackOn && executionMode === "online_fallback" && provider !== "ollama";
  const providerLabel =
    provider === "openai"
      ? "OpenAI"
      : provider === "gemini"
        ? "Gemini"
        : provider === "ollama"
          ? "Ollama"
          : "Claude";

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

  if (dom.cloudState) dom.cloudState.textContent = aiFallbackOn ? "Yes" : "No";
  if (dom.cloudDetail) {
    dom.cloudDetail.textContent = aiFallbackOn
      ? `${isOnlineRefinement ? "Online" : "Offline"} • ${effectiveRefinementModel} • ${providerLabel}`
      : "Offline • AI refinement disabled";
  }
  if (dom.cloudCheck) dom.cloudCheck.classList.toggle("is-active", aiFallbackOn);
  if (dom.aiModelState) dom.aiModelState.textContent = aiFallbackOn ? effectiveRefinementModel : "—";
  if (dom.dictationBadge) {
    dom.dictationBadge.textContent = isOnlineRefinement
      ? "AI Refinement (Online)"
      : "Private Mode (Offline)";
    dom.dictationBadge.classList.toggle("badge--online", isOnlineRefinement);
  }
  if (dom.modeState) dom.modeState.textContent = settings.mode === "ptt" ? "PTT" : "Voice Activation";

  // Input device
  const device = devices.find((item) => item.id === settings?.input_device);
  if (dom.deviceState) dom.deviceState.textContent = device?.label ?? "Default (System)";
  updateDeviceLineClamp();

  // Output device
  const outputDevice = outputDevices.find((item) => item.id === settings?.transcribe_output_device);
  if (dom.outputDeviceState) dom.outputDeviceState.textContent = outputDevice?.label ?? "Default (System)";

  if (dom.modelState) {
    const active = models.find((model) => model.id === settings?.model);
    dom.modelState.textContent = active?.label ?? settings?.model ?? "—";
  }

  applyFeedbackSettings(settings);
  renderFeedbackIndicators();
}

export function updateDeviceLineClamp() {
  if (!dom.deviceState) return;
  dom.deviceState.classList.remove("is-two-line");
  requestAnimationFrame(() => {
    if (!dom.deviceState) return;
    const styles = getComputedStyle(dom.deviceState);
    const lineHeight = parseFloat(styles.lineHeight);
    if (!Number.isFinite(lineHeight) || lineHeight <= 0) return;
    const height = dom.deviceState.getBoundingClientRect().height;
    if (height > lineHeight * 1.6) {
      dom.deviceState.classList.add("is-two-line");
    }
  });
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
