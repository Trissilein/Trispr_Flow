// Live transcript dumping to disk for crash recovery
// Continuously buffers conversation to local file during recording

import { invoke } from "@tauri-apps/api/core";
import { buildConversationHistory, buildExportJson } from "./history";

let dumpEnabled = false;
let lastDumpTime = 0;
const DUMP_INTERVAL_MS = 5000; // Dump every 5 seconds

/**
 * Initialize live dump system
 */
export function initLiveDump(): void {
  dumpEnabled = true;
}

/**
 * Disable live dump
 */
export function disableLiveDump(): void {
  dumpEnabled = false;
}

/**
 * Dump current history to file (throttled)
 */
export async function dumpHistoryToFile(): Promise<void> {
  if (!dumpEnabled) return;

  const now = Date.now();
  // Throttle dumps to prevent excessive disk I/O
  if (now - lastDumpTime < DUMP_INTERVAL_MS) return;

  lastDumpTime = now;

  try {
    const entries = buildConversationHistory();
    if (entries.length === 0) return;

    // Prepare dump data in multiple formats
    const exportDate = new Date().toISOString();
    const jsonData = buildExportJson(entries, exportDate);

    // Call Rust to save to crash recovery file
    await invoke("save_crash_recovery", { content: jsonData });
  } catch (error) {
    // Silent fail - don't interrupt user experience
    console.debug("Live dump failed (non-critical):", error);
  }
}

/**
 * Force immediate dump (e.g., before export)
 */
export async function flushDump(): Promise<void> {
  lastDumpTime = 0; // Reset throttle
  await dumpHistoryToFile();
}

/**
 * Clear crash recovery file
 */
export async function clearCrashRecovery(): Promise<void> {
  try {
    await invoke("clear_crash_recovery");
  } catch (error) {
    console.error("Failed to clear crash recovery:", error);
  }
}
