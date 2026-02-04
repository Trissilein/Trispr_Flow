// UI utility functions for formatting and conversion

import type { ModelInfo, DownloadProgress } from "./types";
import { MODEL_DESCRIPTIONS } from "./state";

export function getModelDescription(model: ModelInfo) {
  const entry = MODEL_DESCRIPTIONS[model.id];
  if (entry) {
    return `${entry.summary} • Speed: ${entry.speed} • Accuracy: ${entry.accuracy} • ${entry.languages}`;
  }
  if (model.source === "local" || model.source === "custom") {
    return "Custom/local model. No benchmark data available.";
  }
  return "Model details unavailable.";
}

// Convert linear level (0-1) to dB (assuming 0dB = 1.0)
export function levelToDb(level: number): number {
  if (level <= 0.00001) return -100;
  return 20 * Math.log10(level);
}

// Convert linear threshold (0-1) to percentage position on dB scale (-60 to 0)
export function thresholdToPercent(threshold: number): number {
  const db = levelToDb(threshold);
  // Scale: -60dB = 0%, 0dB = 100%
  const percent = ((db + 60) / 60) * 100;
  return Math.max(0, Math.min(100, percent));
}

export function formatTime(timestamp: number) {
  const date = new Date(timestamp);
  const base = date.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const hundredths = Math.floor(date.getMilliseconds() / 10)
    .toString()
    .padStart(2, "0");
  return `${base}.${hundredths}`;
}

export function formatSize(sizeMb: number) {
  if (sizeMb >= 1024) {
    return `${(sizeMb / 1024).toFixed(1)} GB`;
  }
  return `${sizeMb} MB`;
}

export function formatProgress(progress?: DownloadProgress) {
  if (!progress) return "";
  if (progress.total && progress.total > 0) {
    const percent = Math.min(100, Math.round((progress.downloaded / progress.total) * 100));
    return `${percent}%`;
  }
  const mb = Math.round(progress.downloaded / (1024 * 1024));
  return `${mb} MB`;
}
