// UI state management for status and hero sections

import type { RecordingState } from "./types";
import {
  settings,
  currentCaptureStatus,
  currentTranscribeStatus,
  setCurrentCaptureStatus,
  setCurrentTranscribeStatus,
  devices,
  models,
  dynamicSustainThreshold
} from "./state";
import * as dom from "./dom-refs";
import { updateRecordingStatus, updateTranscribeStatus } from "./accessibility";
import { thresholdToPercent } from "./ui-helpers";

function updateTranscribeIndicator() {
  const active = currentCaptureStatus === "transcribing" || currentTranscribeStatus === "transcribing";
  const enabled = settings?.transcribe_enabled ?? false;
  const indicatorState: RecordingState =
    !enabled && !active ? "disabled" : active ? "transcribing" : "idle";
  if (dom.transcribeStatusDot) dom.transcribeStatusDot.dataset.state = indicatorState;
  if (dom.transcribeStatusLabel) {
    dom.transcribeStatusLabel.textContent =
      indicatorState === "disabled"
        ? "Transcribing: Deactivated"
        : indicatorState === "transcribing"
          ? "Transcribing: Active"
          : "Transcribing: Idle";
  }
  updateTranscribeStatus(indicatorState);
}

export function setCaptureStatus(state: RecordingState) {
  setCurrentCaptureStatus(state);
  const enabled = settings?.capture_enabled ?? true;
  const isRecording = state === "recording";
  const indicatorState: RecordingState =
    !enabled && !isRecording ? "disabled" : isRecording ? "recording" : "idle";
  if (dom.statusDot) dom.statusDot.dataset.state = indicatorState;
  if (dom.statusLabel) {
    dom.statusLabel.textContent =
      indicatorState === "disabled"
        ? "Recording: Deactivated"
        : isRecording
          ? "Recording: Active"
          : "Recording: Idle";
  }
  if (dom.statusMessage) dom.statusMessage.textContent = "";
  updateRecordingStatus(indicatorState);
  updateTranscribeIndicator();
}

export function setTranscribeStatus(state: RecordingState) {
  setCurrentTranscribeStatus(state);
  const enabled = settings?.transcribe_enabled ?? false;
  if (dom.transcribeStatus) {
    dom.transcribeStatus.textContent =
      !enabled && state === "idle"
        ? "Deactivated"
        : state === "recording"
          ? "Monitoring"
          : state === "transcribing"
            ? "Transcribing"
            : "Idle";
  }
  updateTranscribeIndicator();
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
