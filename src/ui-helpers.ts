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

export const VAD_DB_FLOOR = -60;

// Convert linear level (0-1) to dB (assuming 0dB = 1.0)
export function levelToDb(level: number): number {
  if (level <= 0.00001) return -100;
  return 20 * Math.log10(level);
}

// Convert linear threshold (0-1) to dB, clamped to floor.
export function thresholdToDb(threshold: number, floor = VAD_DB_FLOOR): number {
  if (threshold <= 0) return floor;
  const db = 20 * Math.log10(threshold);
  return Math.max(floor, db);
}

// Convert dB value to linear level (0-1).
export function dbToLevel(db: number): number {
  return Math.pow(10, db / 20);
}

// Convert linear threshold (0-1) to percentage position on dB scale (-60 to 0).
export function thresholdToPercent(threshold: number): number {
  const db = thresholdToDb(threshold, VAD_DB_FLOOR);
  // Scale: -60dB = 0%, 0dB = 100%
  const percent = ((db - VAD_DB_FLOOR) / (0 - VAD_DB_FLOOR)) * 100;
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
