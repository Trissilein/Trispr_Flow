// Accessibility helper tests
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { updateRangeAria, updateRecordingStatus, updateTranscribeStatus } from "../accessibility";
import type { RecordingState } from "../types";

describe("Accessibility Helpers", () => {
  beforeEach(() => {
    // Clear document body
    document.body.innerHTML = "";
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  describe("updateRangeAria", () => {
    it("should update aria-valuenow for range input", () => {
      const input = document.createElement("input");
      input.id = "test-range";
      input.type = "range";
      input.setAttribute("aria-valuenow", "0");
      document.body.appendChild(input);

      updateRangeAria("test-range", 50);

      expect(input.getAttribute("aria-valuenow")).toBe("50");
    });

    it("should handle non-range inputs gracefully", () => {
      const input = document.createElement("input");
      input.id = "test-text";
      input.type = "text";
      document.body.appendChild(input);

      expect(() => updateRangeAria("test-text", 50)).not.toThrow();
      expect(input.getAttribute("aria-valuenow")).toBeNull();
    });

    it("should handle non-existent element gracefully", () => {
      expect(() => updateRangeAria("non-existent", 50)).not.toThrow();
    });

    it("should handle null element gracefully", () => {
      expect(() => updateRangeAria("", 50)).not.toThrow();
    });

    it("should convert number to string", () => {
      const input = document.createElement("input");
      input.id = "test-range";
      input.type = "range";
      document.body.appendChild(input);

      updateRangeAria("test-range", 42.5);

      expect(input.getAttribute("aria-valuenow")).toBe("42.5");
    });
  });

  describe("updateRecordingStatus", () => {
    let statusDot: HTMLElement;
    let announcement: HTMLElement;

    beforeEach(() => {
      statusDot = document.createElement("div");
      statusDot.id = "status-dot";
      document.body.appendChild(statusDot);

      announcement = document.createElement("div");
      announcement.id = "recording-announcement";
      document.body.appendChild(announcement);
    });

    it("should update aria-label for disabled state", () => {
      updateRecordingStatus("disabled");

      expect(statusDot.getAttribute("aria-label")).toBe("Recording status: disabled");
      expect(announcement.textContent).toBe("Recording disabled.");
    });

    it("should update aria-label for idle state", () => {
      updateRecordingStatus("idle");

      expect(statusDot.getAttribute("aria-label")).toBe("Recording status: idle");
      expect(announcement.textContent).toBe("Recording stopped. Ready to record.");
    });

    it("should update aria-label for recording state", () => {
      updateRecordingStatus("recording");

      expect(statusDot.getAttribute("aria-label")).toBe("Recording status: recording");
      expect(announcement.textContent).toBe("Recording started. Speaking now.");
    });

    it("should update aria-label for transcribing state", () => {
      updateRecordingStatus("transcribing");

      expect(statusDot.getAttribute("aria-label")).toBe("Recording status: transcribing");
      expect(announcement.textContent).toBe("Recording stopped. Transcribing audio...");
    });

    it("should handle missing status-dot element gracefully", () => {
      document.body.removeChild(statusDot);

      expect(() => updateRecordingStatus("idle")).not.toThrow();
      expect(announcement.textContent).toBe("Recording stopped. Ready to record.");
    });

    it("should handle missing announcement element gracefully", () => {
      document.body.removeChild(announcement);

      expect(() => updateRecordingStatus("idle")).not.toThrow();
      expect(statusDot.getAttribute("aria-label")).toBe("Recording status: idle");
    });

    it("should handle both elements missing gracefully", () => {
      document.body.removeChild(statusDot);
      document.body.removeChild(announcement);

      expect(() => updateRecordingStatus("idle")).not.toThrow();
    });

    it("should update both elements for all states", () => {
      const states: RecordingState[] = ["disabled", "idle", "recording", "transcribing"];

      states.forEach((state) => {
        updateRecordingStatus(state);

        expect(statusDot.getAttribute("aria-label")).toBe(`Recording status: ${state}`);
        expect(announcement.textContent).toBeTruthy();
      });
    });
  });

  describe("updateTranscribeStatus", () => {
    let statusDot: HTMLElement;
    let announcement: HTMLElement;

    beforeEach(() => {
      statusDot = document.createElement("div");
      statusDot.id = "transcribe-dot";
      document.body.appendChild(statusDot);

      announcement = document.createElement("div");
      announcement.id = "transcribe-announcement";
      document.body.appendChild(announcement);
    });

    it("should update aria-label for disabled state", () => {
      updateTranscribeStatus("disabled");

      expect(statusDot.getAttribute("aria-label")).toBe("Transcribing status: disabled");
      expect(announcement.textContent).toBe("Transcription disabled.");
    });

    it("should update aria-label for idle state", () => {
      updateTranscribeStatus("idle");

      expect(statusDot.getAttribute("aria-label")).toBe("Transcribing status: idle");
      expect(announcement.textContent).toBe("Transcription idle.");
    });

    it("should update aria-label for recording state", () => {
      updateTranscribeStatus("recording");

      expect(statusDot.getAttribute("aria-label")).toBe("Transcribing status: recording");
      expect(announcement.textContent).toBe("Output monitoring active.");
    });

    it("should update aria-label for transcribing state", () => {
      updateTranscribeStatus("transcribing");

      expect(statusDot.getAttribute("aria-label")).toBe("Transcribing status: transcribing");
      expect(announcement.textContent).toBe("Transcription in progress.");
    });

    it("should handle missing transcribe-dot element gracefully", () => {
      document.body.removeChild(statusDot);

      expect(() => updateTranscribeStatus("idle")).not.toThrow();
      expect(announcement.textContent).toBe("Transcription idle.");
    });

    it("should handle missing announcement element gracefully", () => {
      document.body.removeChild(announcement);

      expect(() => updateTranscribeStatus("idle")).not.toThrow();
      expect(statusDot.getAttribute("aria-label")).toBe("Transcribing status: idle");
    });

    it("should handle both elements missing gracefully", () => {
      document.body.removeChild(statusDot);
      document.body.removeChild(announcement);

      expect(() => updateTranscribeStatus("idle")).not.toThrow();
    });

    it("should update both elements for all states", () => {
      const states: RecordingState[] = ["disabled", "idle", "recording", "transcribing"];

      states.forEach((state) => {
        updateTranscribeStatus(state);

        expect(statusDot.getAttribute("aria-label")).toBe(`Transcribing status: ${state}`);
        expect(announcement.textContent).toBeTruthy();
      });
    });
  });

  describe("Integration - Recording and Transcribe Status", () => {
    it("should handle both status updates independently", () => {
      const recordingDot = document.createElement("div");
      recordingDot.id = "status-dot";
      document.body.appendChild(recordingDot);

      const transcribeDot = document.createElement("div");
      transcribeDot.id = "transcribe-dot";
      document.body.appendChild(transcribeDot);

      const recordingAnnouncement = document.createElement("div");
      recordingAnnouncement.id = "recording-announcement";
      document.body.appendChild(recordingAnnouncement);

      const transcribeAnnouncement = document.createElement("div");
      transcribeAnnouncement.id = "transcribe-announcement";
      document.body.appendChild(transcribeAnnouncement);

      updateRecordingStatus("recording");
      updateTranscribeStatus("idle");

      expect(recordingDot.getAttribute("aria-label")).toBe("Recording status: recording");
      expect(transcribeDot.getAttribute("aria-label")).toBe("Transcribing status: idle");
      expect(recordingAnnouncement.textContent).toBe("Recording started. Speaking now.");
      expect(transcribeAnnouncement.textContent).toBe("Transcription idle.");
    });
  });
});
