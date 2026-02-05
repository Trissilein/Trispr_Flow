// Accessibility helper functions for screen readers and ARIA

import type { RecordingState } from "./types";

/**
 * Updates the aria-valuenow attribute for a range slider to support screen readers.
 * @param elementId - The ID of the range input element
 * @param value - The current numeric value
 */
export function updateRangeAria(elementId: string, value: number): void {
  const input = document.getElementById(elementId) as HTMLInputElement | null;
  if (input && input.type === "range") {
    input.setAttribute("aria-valuenow", String(value));
  }
}

/**
 * Updates recording status announcement for screen readers.
 * @param state - The current recording state
 */
export function updateRecordingStatus(state: RecordingState): void {
  const statusDot = document.getElementById("status-dot");
  if (statusDot) {
    statusDot.setAttribute("aria-label", `Recording status: ${state}`);
  }

  const announcement = document.getElementById("recording-announcement");
  if (announcement) {
    const announcements = {
      idle: "Recording stopped. Ready to record.",
      recording: "Recording started. Speaking now.",
      transcribing: "Recording stopped. Transcribing audio..."
    };
    announcement.textContent = announcements[state];
  }
}

export function updateTranscribeStatus(state: RecordingState): void {
  const statusDot = document.getElementById("transcribe-dot");
  if (statusDot) {
    statusDot.setAttribute("aria-label", `Transcribing status: ${state}`);
  }

  const announcement = document.getElementById("transcribe-announcement");
  if (announcement) {
    const announcements = {
      idle: "Transcription idle.",
      recording: "System audio monitoring active.",
      transcribing: "Transcription in progress."
    };
    announcement.textContent = announcements[state];
  }
}
