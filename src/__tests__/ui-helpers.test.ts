import { describe, expect, it } from "vitest";
import type { ModelInfo } from "../types";
import {
  formatProgress,
  formatSize,
  getModelDescription,
  levelToDb,
  thresholdToPercent,
} from "../ui-helpers";

describe("ui-helpers", () => {
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
