import { describe, expect, it } from "vitest";
import type { ModelInfo } from "../types";
import {
  formatProgress,
  formatSize,
  formatVram,
  getModelDescription,
  dbToLevel,
  levelToDb,
  thresholdToDb,
  thresholdToPercent,
  VAD_DB_FLOOR,
} from "../ui-helpers";

describe("ui-helpers", () => {
  it("formatVram shows a dash for missing or negligible values", () => {
    expect(formatVram(null)).toBe("—");
    expect(formatVram(undefined)).toBe("—");
    expect(formatVram(Number.NaN)).toBe("—");
    expect(formatVram(0)).toBe("—");
    expect(formatVram(0.0003)).toBe("—");
    expect(formatVram(0.049)).toBe("—");
    expect(formatVram(0.05)).toBe("0.1 GB");
    expect(formatVram(5.64)).toBe("5.6 GB");
  });

  it("formats model descriptions for known ids", () => {
    const model: ModelInfo = {
      id: "whisper-large-v3",
      label: "Whisper large-v3",
      file_name: "ggml-large-v3.bin",
      size_mb: 2900,
      installed: false,
      downloading: false,
      source: "default",
      available: true,
      removable: false,
    };

    const description = getModelDescription(model);
    expect(description).toContain("Best overall quality");
    expect(description).toContain("Speed:");
    expect(description).toContain("Accuracy:");
  });

  it("formats model descriptions for custom/local models", () => {
    const model: ModelInfo = {
      id: "custom-model",
      label: "Custom Model",
      file_name: "custom.bin",
      size_mb: 1,
      installed: true,
      downloading: false,
      source: "custom",
      available: true,
      removable: true,
    };

    expect(getModelDescription(model)).toBe("Custom/local model. No benchmark data available.");
  });

  it("converts levels to dB safely", () => {
    expect(levelToDb(1)).toBeCloseTo(0);
    expect(levelToDb(0.5)).toBeCloseTo(-6.0206, 3);
    expect(levelToDb(0)).toBe(-100);
  });

  it("converts thresholds to dB with floor", () => {
    expect(thresholdToDb(1)).toBeCloseTo(0);
    expect(thresholdToDb(0.001)).toBeCloseTo(-60, 1);
    expect(thresholdToDb(0)).toBe(VAD_DB_FLOOR);
  });

  it("converts dB to linear levels", () => {
    expect(dbToLevel(0)).toBeCloseTo(1);
    expect(dbToLevel(-6)).toBeCloseTo(0.501, 2);
  });

  it("maps thresholds to 0-100%", () => {
    expect(thresholdToPercent(1)).toBe(100);
    expect(thresholdToPercent(0)).toBe(0);
  });

  it("formats sizes in MB/GB", () => {
    expect(formatSize(512)).toBe("512 MB");
    expect(formatSize(2048)).toBe("2.0 GB");
  });

  it("formats download progress", () => {
    expect(formatProgress()).toBe("");
    expect(
      formatProgress({
        id: "m1",
        downloaded: 50,
        total: 100,
      })
    ).toBe("50%");
    expect(
      formatProgress({
        id: "m2",
        downloaded: 5 * 1024 * 1024,
      })
    ).toBe("5 MB");
  });
});
