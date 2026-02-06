// UI state management for status and hero sections

import type { RecordingState } from "./types";
import {
  settings,
  currentCaptureStatus,
  currentTranscribeStatus,
  setCurrentCaptureStatus,
  setCurrentTranscribeStatus,
  devices,
  outputDevices,
  models,
  dynamicSustainThreshold
} from "./state";
import * as dom from "./dom-refs";
import { updateRecordingStatus, updateTranscribeStatus } from "./accessibility";
import { thresholdToPercent } from "./ui-helpers";

function updateTranscribeIndicator() {
  const enabled = settings?.transcribe_enabled ?? false;
  const activityState: RecordingState =
    currentTranscribeStatus === "transcribing"
      ? "transcribing"
      : currentTranscribeStatus === "recording"
        ? "recording"
        : "idle";
  const labelState: RecordingState = !enabled ? "disabled" : activityState;
  if (dom.transcribeStatusDot) {
    dom.transcribeStatusDot.dataset.state = enabled ? activityState : "idle";
  }
  if (dom.transcribeStatusLabel) {
    dom.transcribeStatusLabel.textContent =
      labelState === "disabled"
        ? "Deactivated"
        : labelState === "recording"
          ? "Monitoring"
          : labelState === "transcribing"
            ? "Active"
            : "Idle";
  }
  if (dom.transcribePill) {
    dom.transcribePill.classList.toggle("status-pill--enabled", enabled);
    dom.transcribePill.classList.toggle("status-pill--disabled", !enabled);
  }
  updateTranscribeStatus(labelState);
}

export function setCaptureStatus(state: RecordingState) {
  setCurrentCaptureStatus(state);
  const enabled = settings?.capture_enabled ?? true;
  const isRecording = state === "recording";
  const labelState: RecordingState = !enabled ? "disabled" : isRecording ? "recording" : "idle";
  const activityState: RecordingState = isRecording ? "recording" : "idle";
  if (dom.statusDot) {
    dom.statusDot.dataset.state = enabled ? activityState : "idle";
  }
  if (dom.statusLabel) {
    dom.statusLabel.textContent =
      labelState === "disabled"
        ? "Deactivated"
        : isRecording
          ? "Active"
          : "Idle";
  }
  if (dom.recordingPill) {
    dom.recordingPill.classList.toggle("status-pill--enabled", enabled);
    dom.recordingPill.classList.toggle("status-pill--disabled", !enabled);
  }
  if (dom.statusMessage) dom.statusMessage.textContent = "";
  updateRecordingStatus(labelState);
  updateTranscribeIndicator();
}

export function setTranscribeStatus(state: RecordingState) {
  setCurrentTranscribeStatus(state);
  const enabled = settings?.transcribe_enabled ?? false;
  if (dom.transcribeStatusPill) {
    dom.transcribeStatusPill.textContent = enabled ? "Enabled" : "Disabled";
    dom.transcribeStatusPill.classList.toggle("status-pill--enabled", enabled);
    dom.transcribeStatusPill.classList.toggle("status-pill--disabled", !enabled);
  }
  updateTranscribeIndicator();
}

export function renderHero() {
  if (!settings) return;
  const cloudOn = settings.cloud_fallback;
  if (dom.cloudState) dom.cloudState.textContent = cloudOn ? "Claude On" : "Claude Off";
  if (dom.cloudCheck) dom.cloudCheck.classList.toggle("is-active", cloudOn);
  if (dom.dictationBadge) {
    dom.dictationBadge.textContent = cloudOn
      ? "AI-enhanced Mode (Online)"
      : "Private Mode (Offline)";
    dom.dictationBadge.classList.toggle("badge--online", cloudOn);
  }
  if (dom.modeState) dom.modeState.textContent = settings.mode === "ptt" ? "PTT" : "Voice Activation";

  // Input device
  const device = devices.find((item) => item.id === settings?.input_device);
  if (dom.deviceState) dom.deviceState.textContent = device?.label ?? "Default (System)";
  updateDeviceLineClamp();

  // Output device
  const outputDevice = outputDevices.find((item) => item.id === settings?.output_device);
  if (dom.outputDeviceState) dom.outputDeviceState.textContent = outputDevice?.label ?? "Default (System)";

  if (dom.modelState) {
    const active = models.find((model) => model.id === settings?.model);
    dom.modelState.textContent = active?.label ?? settings?.model ?? "—";
  }
  setCaptureStatus(currentCaptureStatus);
  setTranscribeStatus(currentTranscribeStatus);
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
