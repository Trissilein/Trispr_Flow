import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { buildExportText, type ExportFormat } from "./history";
import { appRuntimeStartedMs, history, transcribeHistory } from "./state";
import { focusFirstElement, trapFocusInModal } from "./modal-focus";
import { showToast } from "./toast";
import type { HistoryEntry, PartitionInfo } from "./types";

type HistoryKind = "mic" | "system";
type ExportRange = "session" | "today" | "yesterday" | "week" | "month" | "custom";

interface ExportConfig {
  range: ExportRange;
  customFrom?: number;
  customTo?: number;
  includeMic: boolean;
  includeSystem: boolean;
  format: ExportFormat;
}

const EXPORT_RANGES: ExportRange[] = [
  "session",
  "today",
  "yesterday",
  "week",
  "month",
  "custom",
];

const partitionInfoCache = new Map<HistoryKind, PartitionInfo[]>();
const partitionEntriesCache = new Map<string, HistoryEntry[]>();

let initialized = false;
let activeRange: ExportRange = "session";
let previewGeneration = 0;
let lastFocusedBeforeOpen: HTMLElement | null = null;

function isExportRange(value: string): value is ExportRange {
  return EXPORT_RANGES.includes(value as ExportRange);
}

function partitionCacheKey(kind: HistoryKind, key: string): string {
  return `${kind}:${key}`;
}

function setRange(range: ExportRange): void {
  activeRange = range;
  const buttons = document.querySelectorAll<HTMLButtonElement>("[data-export-range]");
  buttons.forEach((button) => {
    const isActive = button.dataset.exportRange === range;
    button.classList.toggle("is-active", isActive);
    button.setAttribute("aria-pressed", isActive ? "true" : "false");
  });
  if (dom.exportCustomRange) {
    dom.exportCustomRange.hidden = range !== "custom";
  }
}

function parseDateTimeLocal(value: string | undefined): number | undefined {
  if (!value) return undefined;
  const parsed = new Date(value).getTime();
  return Number.isFinite(parsed) ? parsed : undefined;
}

function readConfig(): ExportConfig {
  const format = (dom.exportDialogFormat?.value as ExportFormat) || "txt";
  return {
    range: activeRange,
    customFrom: parseDateTimeLocal(dom.exportCustomFrom?.value),
    customTo: parseDateTimeLocal(dom.exportCustomTo?.value),
    includeMic: dom.exportIncludeMic?.checked ?? true,
    includeSystem: dom.exportIncludeSystem?.checked ?? true,
    format,
  };
}

function computeRangeTimestamps(config: ExportConfig): [number, number] {
  const now = Date.now();
  const nowDate = new Date(now);

  if (config.range === "session") {
    return [Math.min(appRuntimeStartedMs, now), now];
  }

  if (config.range === "today") {
    const start = new Date(nowDate);
    start.setHours(0, 0, 0, 0);
    return [start.getTime(), now];
  }

  if (config.range === "yesterday") {
    const todayStart = new Date(nowDate);
    todayStart.setHours(0, 0, 0, 0);
    const start = new Date(todayStart);
    start.setDate(start.getDate() - 1);
    const end = todayStart.getTime() - 1;
    return [start.getTime(), end];
  }

  if (config.range === "week") {
    const start = new Date(nowDate);
    start.setDate(start.getDate() - 6);
    start.setHours(0, 0, 0, 0);
    return [start.getTime(), now];
  }

  if (config.range === "month") {
    const start = new Date(nowDate.getFullYear(), nowDate.getMonth(), 1);
    start.setHours(0, 0, 0, 0);
    return [start.getTime(), now];
  }

  const from = config.customFrom ?? 0;
  const to = config.customTo ?? now;
  return [from, to];
}

function computeNeededPartitionKeys(fromMs: number, toMs: number): string[] {
  if (fromMs > toMs) return [];
  const keys: string[] = [];
  const start = new Date(fromMs);
  const end = new Date(toMs);
  let year = start.getUTCFullYear();
  let month = start.getUTCMonth();
  const endYear = end.getUTCFullYear();
  const endMonth = end.getUTCMonth();

  while (year < endYear || (year === endYear && month <= endMonth)) {
    keys.push(`${year}-${String(month + 1).padStart(2, "0")}`);
    month += 1;
    if (month >= 12) {
      month = 0;
      year += 1;
    }
  }

  return keys;
}

function dedupeAndSort(entries: HistoryEntry[], fromMs: number, toMs: number): HistoryEntry[] {
  const byId = new Map<string, HistoryEntry>();
  for (const entry of entries) {
    if (entry.timestamp_ms < fromMs || entry.timestamp_ms > toMs) continue;
    if (!byId.has(entry.id)) {
      byId.set(entry.id, entry);
    }
  }
  return Array.from(byId.values()).sort((a, b) => a.timestamp_ms - b.timestamp_ms);
}

async function listPartitions(kind: HistoryKind): Promise<PartitionInfo[]> {
  const cached = partitionInfoCache.get(kind);
  if (cached) return cached;
  const partitions = await invoke<PartitionInfo[]>("list_history_partitions", { kind });
  partitionInfoCache.set(kind, partitions);
  return partitions;
}

async function loadPartition(kind: HistoryKind, key: string): Promise<HistoryEntry[]> {
  const cacheKey = partitionCacheKey(kind, key);
  const cached = partitionEntriesCache.get(cacheKey);
  if (cached) return cached;
  const entries = await invoke<HistoryEntry[]>("load_history_partition", { kind, key });
  partitionEntriesCache.set(cacheKey, entries);
  return entries;
}

async function gatherEntries(config: ExportConfig): Promise<HistoryEntry[]> {
  const [fromMs, toMs] = computeRangeTimestamps(config);
  if (fromMs > toMs) return [];

  const collected: HistoryEntry[] = [];

  if (config.includeMic) {
    collected.push(...history.filter((entry) => entry.timestamp_ms >= fromMs && entry.timestamp_ms <= toMs));
  }
  if (config.includeSystem) {
    collected.push(...transcribeHistory.filter((entry) => entry.timestamp_ms >= fromMs && entry.timestamp_ms <= toMs));
  }

  const neededKeys = computeNeededPartitionKeys(fromMs, toMs);
  const sessionCrossesMonthBoundary = config.range === "session" && neededKeys.length > 1;
  if (config.range !== "session" || sessionCrossesMonthBoundary) {
    if (neededKeys.length > 0) {
      if (config.includeMic) {
        const partitions = await listPartitions("mic");
        const available = new Set(partitions.map((partition) => partition.key));
        for (const key of neededKeys) {
          if (!available.has(key)) continue;
          const entries = await loadPartition("mic", key);
          collected.push(...entries);
        }
      }

      if (config.includeSystem) {
        const partitions = await listPartitions("system");
        const available = new Set(partitions.map((partition) => partition.key));
        for (const key of neededKeys) {
          if (!available.has(key)) continue;
          const entries = await loadPartition("system", key);
          collected.push(...entries);
        }
      }
    }
  }

  return dedupeAndSort(collected, fromMs, toMs);
}

function formatDateTime(value: number): string {
  return new Date(value).toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function formatPreviewRange(config: ExportConfig, fromMs: number, toMs: number): string {
  if (config.range === "session") {
    return `Runtime session (${formatDateTime(fromMs)} → ${formatDateTime(toMs)})`;
  }
  return `${formatDateTime(fromMs)} → ${formatDateTime(toMs)}`;
}

async function updatePreview(): Promise<void> {
  if (!dom.exportDialog) return;

  const generation = ++previewGeneration;
  const config = readConfig();
  const [fromMs, toMs] = computeRangeTimestamps(config);

  if (!config.includeMic && !config.includeSystem) {
    if (dom.exportPreviewCount) dom.exportPreviewCount.textContent = "0 entries";
    if (dom.exportPreviewSpan) dom.exportPreviewSpan.textContent = "Select at least one source.";
    if (dom.exportDialogRun) dom.exportDialogRun.disabled = true;
    return;
  }

  if (fromMs > toMs) {
    if (dom.exportPreviewCount) dom.exportPreviewCount.textContent = "0 entries";
    if (dom.exportPreviewSpan) dom.exportPreviewSpan.textContent = "Invalid custom range.";
    if (dom.exportDialogRun) dom.exportDialogRun.disabled = true;
    return;
  }

  if (dom.exportDialogRun) dom.exportDialogRun.disabled = true;
  if (dom.exportPreviewSpan) dom.exportPreviewSpan.textContent = "Loading preview...";

  try {
    const entries = await gatherEntries(config);
    if (generation !== previewGeneration) return;

    const countLabel = entries.length === 1 ? "entry" : "entries";
    if (dom.exportPreviewCount) {
      dom.exportPreviewCount.textContent = `${entries.length} ${countLabel}`;
    }

    if (dom.exportPreviewSpan) {
      if (!entries.length) {
        dom.exportPreviewSpan.textContent = formatPreviewRange(config, fromMs, toMs);
      } else {
        const start = entries[0].timestamp_ms;
        const end = entries[entries.length - 1].timestamp_ms;
        dom.exportPreviewSpan.textContent = `${formatDateTime(start)} → ${formatDateTime(end)}`;
      }
    }

    if (dom.exportDialogRun) {
      dom.exportDialogRun.disabled = entries.length === 0;
    }
  } catch (error) {
    if (generation !== previewGeneration) return;
    if (dom.exportPreviewCount) dom.exportPreviewCount.textContent = "0 entries";
    if (dom.exportPreviewSpan) dom.exportPreviewSpan.textContent = "Failed to compute preview.";
    if (dom.exportDialogRun) dom.exportDialogRun.disabled = true;
    console.error("Export preview failed:", error);
  }
}

async function executeExport(): Promise<void> {
  const config = readConfig();
  if (!config.includeMic && !config.includeSystem) {
    showToast({
      type: "warning",
      title: "Export blocked",
      message: "Select at least one source.",
      duration: 3500,
    });
    return;
  }

  const entries = await gatherEntries(config);
  if (!entries.length) {
    showToast({
      type: "warning",
      title: "Nothing to export",
      message: "No entries match the selected range.",
      duration: 3500,
    });
    return;
  }

  const format = config.format;
  const content = buildExportText(entries, format);
  const extension = format === "md" ? "md" : format;
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const filename = `transcript-${timestamp}.${extension}`;

  try {
    const path = await invoke<string>("save_transcript", {
      filename,
      content,
      format,
    });

    showToast({
      type: "success",
      title: "Export saved",
      message: path ? `Saved to ${path}` : filename,
      duration: 4500,
    });

    closeExportDialog();
  } catch (error) {
    console.error("Export failed:", error);
    showToast({
      type: "error",
      title: "Export failed",
      message: String(error),
      duration: 5000,
    });
  }
}

export function closeExportDialog(): void {
  if (!dom.exportDialog) return;
  dom.exportDialog.hidden = true;
  const restoreTarget = lastFocusedBeforeOpen ?? dom.historyExport ?? null;
  lastFocusedBeforeOpen = null;
  restoreTarget?.focus();
}

export async function openExportDialog(): Promise<void> {
  initExportDialog();
  if (!dom.exportDialog) return;

  lastFocusedBeforeOpen = document.activeElement as HTMLElement | null;
  partitionInfoCache.clear();
  partitionEntriesCache.clear();
  dom.exportDialog.hidden = false;
  const modalCard = dom.exportDialog.querySelector<HTMLElement>(".export-modal-card");
  focusFirstElement(modalCard ?? dom.exportDialog, dom.exportDialogClose ?? dom.exportDialogRun);
  await updatePreview();
}

export function initExportDialog(): void {
  if (initialized) return;
  initialized = true;

  if (!dom.exportDialog) return;

  setRange(activeRange);
  void updatePreview();

  document.querySelectorAll<HTMLButtonElement>("[data-export-range]").forEach((button) => {
    button.addEventListener("click", () => {
      const value = button.dataset.exportRange;
      if (!value || !isExportRange(value)) return;
      setRange(value);
      void updatePreview();
    });
  });

  dom.exportDialogBackdrop?.addEventListener("click", () => {
    closeExportDialog();
  });
  dom.exportDialogClose?.addEventListener("click", () => {
    closeExportDialog();
  });

  dom.exportCustomFrom?.addEventListener("change", () => void updatePreview());
  dom.exportCustomTo?.addEventListener("change", () => void updatePreview());
  dom.exportIncludeMic?.addEventListener("change", () => void updatePreview());
  dom.exportIncludeSystem?.addEventListener("change", () => void updatePreview());
  dom.exportDialogFormat?.addEventListener("change", () => void updatePreview());
  dom.exportDialogRun?.addEventListener("click", () => {
    void executeExport();
  });

  document.addEventListener("keydown", (event) => {
    if (!dom.exportDialog || dom.exportDialog.hidden) return;
    if (event.key === "Escape") {
      closeExportDialog();
      return;
    }
    const modalCard = dom.exportDialog.querySelector<HTMLElement>(".export-modal-card");
    trapFocusInModal(event, modalCard ?? dom.exportDialog);
  });
}
