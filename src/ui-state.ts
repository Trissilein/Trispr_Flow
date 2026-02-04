// UI state management for status and hero sections

import type { RecordingState } from "./types";
import { settings, currentStatus, setCurrentStatus, devices, models, dynamicSustainThreshold } from "./state";
import * as dom from "./dom-refs";
import { updateRecordingStatus } from "./accessibility";
import { thresholdToPercent } from "./ui-helpers";

export function setStatus(state: RecordingState) {
  setCurrentStatus(state);
  if (dom.statusDot) dom.statusDot.dataset.state = state;
  if (!dom.statusLabel) return;
  dom.statusLabel.textContent =
    state === "idle" ? "Idle" : state === "recording" ? "Recording" : "Transcribing";
  if (dom.statusMessage) dom.statusMessage.textContent = "";
  // Update accessibility attributes for screen readers
  updateRecordingStatus(state);
}

export function renderHero() {
  if (!settings) return;
  const cloudOn = settings.cloud_fallback;
  if (dom.cloudState) dom.cloudState.textContent = cloudOn ? "Claude On" : "Claude Off";
  if (dom.cloudCheck) dom.cloudCheck.classList.toggle("is-active", cloudOn);
  if (dom.dictationBadge) {
    dom.dictationBadge.textContent = cloudOn ? "Online Supported Dictation" : "Offline Dictation";
    dom.dictationBadge.classList.toggle("badge--online", cloudOn);
  }
  if (dom.modeState) dom.modeState.textContent = settings.mode === "ptt" ? "PTT" : "VAD";
  const device = devices.find((item) => item.id === settings?.input_device);
  if (dom.deviceState) dom.deviceState.textContent = device?.label ?? "Default";
  updateDeviceLineClamp();
  if (dom.modelState) {
    const active = models.find((model) => model.id === settings?.model);
    dom.modelState.textContent = active?.label ?? settings?.model ?? "—";
  }
  if (dom.engineLabel) dom.engineLabel.textContent = "whisper.cpp (GPU auto)";
  setStatus(currentStatus);
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
