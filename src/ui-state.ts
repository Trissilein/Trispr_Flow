// UI state management for status and hero sections

import type { RecordingState } from "./types";
import {
  settings,
  devices,
  outputDevices,
  models,
  dynamicSustainThreshold
} from "./state";
import * as dom from "./dom-refs";
import { updateRecordingStatus, updateTranscribeStatus } from "./accessibility";
import { thresholdToPercent } from "./ui-helpers";
import {
  applyFeedbackSettings,
  getFeedbackView,
  transitionCaptureRuntime,
  transitionTranscribeRuntime,
} from "./feedback-state";

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
}

export function setCaptureStatus(state: RecordingState) {
  transitionCaptureRuntime(state);
  renderFeedbackIndicators();
}

export function setTranscribeStatus(state: RecordingState) {
  transitionTranscribeRuntime(state);
  renderFeedbackIndicators();
}

export function renderHero() {
  if (!settings) return;
  const aiFallbackOn = settings.ai_fallback?.enabled ?? settings.cloud_fallback;
  const provider = settings.ai_fallback?.provider ?? "claude";
  const providerLabel = provider === "openai" ? "OpenAI" : provider === "gemini" ? "Gemini" : "Claude";
  if (dom.cloudState) dom.cloudState.textContent = aiFallbackOn ? `${providerLabel} On` : "AI Off";
  if (dom.cloudCheck) dom.cloudCheck.classList.toggle("is-active", aiFallbackOn);
  if (dom.dictationBadge) {
    dom.dictationBadge.textContent = aiFallbackOn
      ? "AI fallback enabled"
      : "Private Mode (Offline)";
    dom.dictationBadge.classList.toggle("badge--online", aiFallbackOn);
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
