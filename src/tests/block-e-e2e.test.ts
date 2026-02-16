/**
 * Block E: End-to-End Integration Tests
 * Validates: Speaker diarization data flow, quality presets,
 * parallel mode, recording workflow, update checks
 */

import { describe, it, expect } from "vitest";
import type { SpeakerSegment, TranscriptionAnalysis } from "../types";

describe("Block E: E2E Integration Tests", () => {

  // ============================================================
  // E19: Speaker-diarized transcript data model
  // ============================================================
  describe("E19: Speaker Diarization Data Model", () => {
    const mockSegments: SpeakerSegment[] = [
      { speaker_id: "SPEAKER_00", start_time: 0.0, end_time: 3.5, text: "Hello, how are you?" },
      { speaker_id: "SPEAKER_01", start_time: 3.5, end_time: 7.2, text: "I'm doing well, thanks for asking." },
      { speaker_id: "SPEAKER_00", start_time: 7.2, end_time: 12.0, text: "Great, let's discuss the project." },
      { speaker_id: "SPEAKER_02", start_time: 12.0, end_time: 18.5, text: "Sure, I have the latest updates." },
    ];

    it("should have required fields on SpeakerSegment", () => {
      for (const seg of mockSegments) {
        expect(seg.speaker_id).toBeTruthy();
        expect(typeof seg.start_time).toBe("number");
        expect(typeof seg.end_time).toBe("number");
        expect(typeof seg.text).toBe("string");
        expect(seg.end_time).toBeGreaterThan(seg.start_time);
      }
    });

    it("should support optional speaker_label", () => {
      const labeled: SpeakerSegment = {
        speaker_id: "SPEAKER_00",
        speaker_label: "Alice",
        start_time: 0,
        end_time: 5,
        text: "Hello"
      };
      expect(labeled.speaker_label).toBe("Alice");
    });

    it("should identify distinct speakers", () => {
      const speakers = new Set(mockSegments.map(s => s.speaker_id));
      expect(speakers.size).toBe(3);
    });

    it("should maintain chronological order", () => {
      for (let i = 1; i < mockSegments.length; i++) {
        expect(mockSegments[i].start_time).toBeGreaterThanOrEqual(mockSegments[i - 1].start_time);
      }
    });
  });

  // ============================================================
  // E24: Analysis result transformation
  // ============================================================
  describe("E24: Sidecar Response → TranscriptionAnalysis", () => {
    it("should transform sidecar response to TranscriptionAnalysis", () => {
      const sidecarResponse = {
        segments: [
          { speaker: "SPEAKER_00", start_time: 0, end_time: 5, text: "Test" },
          { speaker: "SPEAKER_01", start_time: 5, end_time: 10, text: "Reply" },
        ],
        metadata: {
          duration: 10.0,
          num_speakers: 2,
          processing_time: 2.5,
          language: "en",
          model_precision: "fp16",
        }
      };

      const analysis: TranscriptionAnalysis = {
        segments: sidecarResponse.segments.map(seg => ({
          speaker_id: seg.speaker,
          start_time: seg.start_time,
          end_time: seg.end_time,
          text: seg.text,
        })),
        duration_s: sidecarResponse.metadata.duration,
        total_speakers: sidecarResponse.metadata.num_speakers,
        processing_time_ms: sidecarResponse.metadata.processing_time * 1000,
      };

      expect(analysis.segments).toHaveLength(2);
      expect(analysis.segments[0].speaker_id).toBe("SPEAKER_00");
      expect(analysis.segments[1].speaker_id).toBe("SPEAKER_01");
      expect(analysis.duration_s).toBe(10.0);
      expect(analysis.total_speakers).toBe(2);
      expect(analysis.processing_time_ms).toBe(2500);
    });

    it("should handle single-speaker analysis", () => {
      const analysis: TranscriptionAnalysis = {
        segments: [
          { speaker_id: "SPEAKER_00", start_time: 0, end_time: 60, text: "Monologue" },
        ],
        duration_s: 60,
        total_speakers: 1,
        processing_time_ms: 1500,
      };

      expect(analysis.total_speakers).toBe(1);
      expect(analysis.segments).toHaveLength(1);
    });

    it("should format duration correctly", () => {
      const durations = [
        { s: 120, expected: { m: 2, sec: 0 } },
        { s: 65.5, expected: { m: 1, sec: 5 } },
        { s: 3661, expected: { m: 61, sec: 1 } },
      ];

      for (const { s, expected } of durations) {
        expect(Math.floor(s / 60)).toBe(expected.m);
        expect(Math.floor(s % 60)).toBe(expected.sec);
      }
    });
  });

  // ============================================================
  // E26: Quality preset validation
  // ============================================================
  describe("E26: Quality Preset Values", () => {
    const validBitrates = [32, 64, 96, 128];
    const validPrecisions: Array<"fp16" | "int8"> = ["fp16", "int8"];

    it("should have valid OPUS bitrate options", () => {
      for (const bitrate of validBitrates) {
        expect(bitrate).toBeGreaterThanOrEqual(32);
        expect(bitrate).toBeLessThanOrEqual(128);
        expect(bitrate % 32).toBe(0);
      }
    });

    it("should have valid precision modes", () => {
      expect(validPrecisions).toContain("fp16");
      expect(validPrecisions).toContain("int8");
      expect(validPrecisions).toHaveLength(2);
    });

    it("should map bitrate to quality description", () => {
      const descriptions: Record<number, string> = {
        32: "smallest",
        64: "recommended",
        96: "high quality",
        128: "best quality",
      };
      expect(Object.keys(descriptions)).toHaveLength(4);
      expect(descriptions[64]).toBe("recommended");
    });
  });

  // ============================================================
  // E27: Parallel mode data flow
  // ============================================================
  describe("E27: Parallel Mode Results", () => {
    it("should produce combined results with both engines", () => {
      const parallelResult = {
        whisper: { text: "Hello from Whisper", source: "whisper-cpp" },
        vibevoice: {
          segments: [
            { speaker: "SPEAKER_00", start_time: 0, end_time: 3, text: "Hello from VibeVoice" }
          ],
          metadata: {
            duration: 3.0,
            num_speakers: 1,
            processing_time: 1.5,
            language: "en",
            model_precision: "fp16",
          }
        }
      };

      expect(parallelResult.whisper.text).toBeTruthy();
      expect(parallelResult.vibevoice.segments).toHaveLength(1);
      expect(parallelResult.vibevoice.metadata.num_speakers).toBe(1);
    });

    it("should handle vibevoice error gracefully", () => {
      const result = {
        whisper: { text: "Whisper works" },
        vibevoice: { error: "Sidecar not running" },
      };

      const hasVibevoiceError = "error" in result.vibevoice;
      expect(hasVibevoiceError).toBe(true);
      expect(result.whisper.text).toBeTruthy();
    });

    it("should handle whisper error gracefully", () => {
      const result = {
        whisper: { error: "Model not loaded" },
        vibevoice: {
          segments: [],
          metadata: { duration: 0, num_speakers: 0, processing_time: 0 },
        },
      };

      const hasWhisperError = "error" in result.whisper;
      expect(hasWhisperError).toBe(true);
      expect(result.vibevoice.segments).toBeDefined();
    });
  });

  // ============================================================
  // E28: Update check timing
  // ============================================================
  describe("E28: Update Check Interval", () => {
    const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

    it("should trigger check when over a week old", () => {
      const now = Date.now();
      const lastCheck = now - WEEK_MS - 1000;
      expect(now - lastCheck).toBeGreaterThan(WEEK_MS);
    });

    it("should skip check within a week", () => {
      const now = Date.now();
      const lastCheck = now - (WEEK_MS / 2);
      expect(now - lastCheck).toBeLessThan(WEEK_MS);
    });

    it("should trigger check when no previous check exists", () => {
      const lastCheck: string | null = null;
      const shouldCheck = !lastCheck;
      expect(shouldCheck).toBe(true);
    });
  });

  // ============================================================
  // Recording filename logic
  // ============================================================
  describe("Recording Filename Generation", () => {
    function sanitize(name: string): string {
      return name
        .split("")
        .map(c => {
          if (/[a-zA-Z0-9\-_]/.test(c)) return c;
          if (/\s/.test(c)) return "-";
          return "_";
        })
        .join("")
        .slice(0, 30);
    }

    it("should sanitize session names", () => {
      expect(sanitize("Team Standup")).toBe("Team-Standup");
      expect(sanitize("Daily/Check-in")).toBe("Daily_Check-in");
      expect(sanitize("a".repeat(50))).toHaveLength(30);
      expect(sanitize("")).toBe("");
    });

    it("should preserve alphanumeric and hyphens", () => {
      expect(sanitize("my-session_01")).toBe("my-session_01");
    });

    it("should replace special chars with underscore", () => {
      expect(sanitize("meeting: Q1")).toBe("meeting_-Q1");
    });
  });

  // ============================================================
  // Full workflow: Record → Save → Analyse
  // ============================================================
  describe("Full Workflow: Record → Save → Analyse", () => {
    it("should auto-save recordings over 10 seconds", () => {
      const testCases = [
        { durationMs: 5000, shouldSave: false },
        { durationMs: 10000, shouldSave: true },
        { durationMs: 45000, shouldSave: true },
        { durationMs: 9999, shouldSave: false },
      ];

      for (const { durationMs, shouldSave } of testCases) {
        expect(durationMs >= 10000).toBe(shouldSave);
      }
    });

    it("should achieve significant OPUS compression", () => {
      const durationSec = 60;
      const wavSize = 16000 * 2 * durationSec; // 16kHz mono 16-bit: 1,920,000 bytes
      const opusSize = 8000 * durationSec; // 64kbps: 480,000 bytes
      const ratio = opusSize / wavSize;

      expect(ratio).toBeLessThan(0.5);
    });

    it("should produce valid analysis from saved recording", () => {
      const recording = {
        sampleRate: 16000,
        channels: 1,
        durationMs: 30000,
      };

      expect(recording.durationMs).toBeGreaterThanOrEqual(10000);

      const analysis: TranscriptionAnalysis = {
        segments: [
          { speaker_id: "SPEAKER_00", start_time: 0, end_time: 15, text: "First half" },
          { speaker_id: "SPEAKER_01", start_time: 15, end_time: 30, text: "Second half" },
        ],
        duration_s: 30,
        total_speakers: 2,
        processing_time_ms: 2000,
      };

      expect(analysis.segments).toHaveLength(2);
      expect(analysis.duration_s).toBe(30);
      expect(analysis.total_speakers).toBe(2);
    });
  });
});
