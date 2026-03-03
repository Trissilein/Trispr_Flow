import { invoke } from "@tauri-apps/api/core";
import * as dom from "./dom-refs";
import { buildExportText, type ExportFormat } from "./history";
import { resolveSourceLabel } from "./history-preferences";
import { focusFirstElement, trapFocusInModal } from "./modal-focus";
import { showToast } from "./toast";
import { escapeHtml } from "./utils";
import type { HistoryEntry, PartitionInfo } from "./types";

type HistoryKind = "mic" | "system";

let initialized = false;
let micPartitions: PartitionInfo[] = [];
let systemPartitions: PartitionInfo[] = [];
let currentKind: HistoryKind | null = null;
let currentKey: string | null = null;
let currentEntries: HistoryEntry[] = [];
let lastFocusedBeforeOpen: HTMLElement | null = null;

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

function formatEntryTime(timestampMs: number): string {
  return new Date(timestampMs).toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function renderPartitionGroup(
  target: HTMLDivElement | null,
  kind: HistoryKind,
  partitions: PartitionInfo[]
): void {
  if (!target) return;
  if (!partitions.length) {
    target.innerHTML = '<div class="archive-empty">No partitions found.</div>';
    return;
  }

  target.innerHTML = partitions
    .map((partition) => {
      const selected = currentKind === kind && currentKey === partition.key;
      const activeBadge = partition.is_active ? '<span class="archive-active-tag">active</span>' : "";
      const safeLabel = escapeHtml(partition.label);
      const safeKey = escapeHtml(partition.key);
      return `
        <button
          type="button"
          class="archive-partition-btn ${selected ? "is-selected" : ""}"
          data-archive-kind="${kind}"
          data-archive-key="${safeKey}"
          aria-pressed="${selected ? "true" : "false"}"
        >
          <span class="archive-partition-title">${safeLabel} ${activeBadge}</span>
          <span class="archive-partition-meta">${partition.entry_count} entries • ${formatBytes(partition.size_bytes)}</span>
        </button>
      `;
    })
    .join("");
}

function renderPartitionLists(): void {
  renderPartitionGroup(dom.archiveMicPartitions, "mic", micPartitions);
  renderPartitionGroup(dom.archiveSystemPartitions, "system", systemPartitions);
}

function renderEntries(entries: HistoryEntry[]): void {
  if (!dom.archiveEntries) return;
  if (!entries.length) {
    dom.archiveEntries.innerHTML = '<div class="archive-empty">Select a partition to preview entries.</div>';
    return;
  }

  const sorted = [...entries].sort((a, b) => b.timestamp_ms - a.timestamp_ms);
  dom.archiveEntries.innerHTML = sorted
    .map((entry) => {
      const speaker = escapeHtml(entry.speaker_name?.trim() || resolveSourceLabel(entry.source));
      const text = escapeHtml(entry.refinement?.refined ?? entry.text);
      const time = escapeHtml(formatEntryTime(entry.timestamp_ms));
      return `
        <article class="archive-entry-item">
          <div class="archive-entry-meta">
            <span>${time}</span>
            <span>${speaker}</span>
          </div>
          <div class="archive-entry-text">${text}</div>
        </article>
      `;
    })
    .join("");
}

function partitionLabel(kind: HistoryKind, key: string): string {
  const source = kind === "mic" ? micPartitions : systemPartitions;
  const match = source.find((partition) => partition.key === key);
  return match?.label ?? key;
}

function updateSelectionMeta(): void {
  if (!dom.archiveSelectionMeta) return;
  if (!currentKind || !currentKey) {
    dom.archiveSelectionMeta.textContent = "No partition selected.";
    return;
  }
  const kindLabel = currentKind === "mic" ? "Input" : "System Audio";
  const label = partitionLabel(currentKind, currentKey);
  const count = currentEntries.length;
  const countLabel = count === 1 ? "entry" : "entries";
  dom.archiveSelectionMeta.textContent = `${kindLabel} • ${label} • ${count} ${countLabel}`;
}

async function refreshPartitionLists(): Promise<void> {
  const [mic, system] = await Promise.all([
    invoke<PartitionInfo[]>("list_history_partitions", { kind: "mic" }),
    invoke<PartitionInfo[]>("list_history_partitions", { kind: "system" }),
  ]);

  micPartitions = mic;
  systemPartitions = system;
  renderPartitionLists();

  const selectedExists = (() => {
    if (!currentKind || !currentKey) return false;
    const source = currentKind === "mic" ? micPartitions : systemPartitions;
    return source.some((partition) => partition.key === currentKey);
  })();

  if (selectedExists) {
    return;
  }

  currentKind = null;
  currentKey = null;
  currentEntries = [];

  const firstMic = micPartitions[0];
  const firstSystem = systemPartitions[0];
  const fallback = firstMic
    ? ({ kind: "mic", key: firstMic.key } as const)
    : firstSystem
      ? ({ kind: "system", key: firstSystem.key } as const)
      : null;

  if (fallback) {
    await loadPartition(fallback.kind, fallback.key);
  } else {
    renderEntries([]);
    updateSelectionMeta();
  }
}

async function loadPartition(kind: HistoryKind, key: string): Promise<void> {
  try {
    const entries = await invoke<HistoryEntry[]>("load_history_partition", { kind, key });
    currentKind = kind;
    currentKey = key;
    currentEntries = entries;
    renderPartitionLists();
    renderEntries(entries);
    updateSelectionMeta();
  } catch (error) {
    console.error("Failed to load partition:", error);
    showToast({
      type: "error",
      title: "Archive load failed",
      message: String(error),
      duration: 4500,
    });
  }
}

async function exportCurrentPartition(): Promise<void> {
  if (!currentEntries.length) {
    showToast({
      type: "warning",
      title: "Nothing to export",
      message: "Load a partition first.",
      duration: 3200,
    });
    return;
  }

  const format = (dom.archiveExportFormat?.value as ExportFormat) || "txt";
  const content = buildExportText(currentEntries, format);
  const extension = format === "md" ? "md" : format;
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const kindLabel = currentKind ?? "history";
  const keyLabel = currentKey ?? "archive";
  const filename = `transcript-${kindLabel}-${keyLabel}-${timestamp}.${extension}`;

  try {
    const path = await invoke<string>("save_transcript", {
      filename,
      content,
      format,
    });
    showToast({
      type: "success",
      title: "Archive export saved",
      message: path ? `Saved to ${path}` : filename,
      duration: 4500,
    });
  } catch (error) {
    console.error("Archive export failed:", error);
    showToast({
      type: "error",
      title: "Archive export failed",
      message: String(error),
      duration: 5000,
    });
  }
}

function onPartitionListClick(event: Event): void {
  const target = event.target as HTMLElement | null;
  const button = target?.closest<HTMLButtonElement>(".archive-partition-btn");
  if (!button) return;
  const kind = button.dataset.archiveKind;
  const key = button.dataset.archiveKey;
  if ((kind !== "mic" && kind !== "system") || !key) return;
  void loadPartition(kind, key);
}

export function closeArchiveBrowser(): void {
  if (!dom.archiveBrowser) return;
  dom.archiveBrowser.hidden = true;
  const restoreTarget = lastFocusedBeforeOpen ?? dom.archiveBrowseBtn ?? null;
  lastFocusedBeforeOpen = null;
  restoreTarget?.focus();
}

export async function openArchiveBrowser(): Promise<void> {
  initArchiveBrowser();
  if (!dom.archiveBrowser) return;
  lastFocusedBeforeOpen = document.activeElement as HTMLElement | null;
  dom.archiveBrowser.hidden = false;
  const modalCard = dom.archiveBrowser.querySelector<HTMLElement>(".archive-modal-card");
  focusFirstElement(modalCard ?? dom.archiveBrowser, dom.archiveBrowserClose ?? dom.archiveExportBtn);
  await refreshPartitionLists();
}

export function initArchiveBrowser(): void {
  if (initialized) return;
  initialized = true;

  if (!dom.archiveBrowser) return;

  dom.archiveMicPartitions?.addEventListener("click", onPartitionListClick);
  dom.archiveSystemPartitions?.addEventListener("click", onPartitionListClick);

  dom.archiveBrowserBackdrop?.addEventListener("click", () => {
    closeArchiveBrowser();
  });

  dom.archiveBrowserClose?.addEventListener("click", () => {
    closeArchiveBrowser();
  });

  dom.archiveExportBtn?.addEventListener("click", () => {
    void exportCurrentPartition();
  });

  document.addEventListener("keydown", (event) => {
    if (!dom.archiveBrowser || dom.archiveBrowser.hidden) return;
    if (event.key === "Escape") {
      closeArchiveBrowser();
      return;
    }
    const modalCard = dom.archiveBrowser.querySelector<HTMLElement>(".archive-modal-card");
    trapFocusInModal(event, modalCard ?? dom.archiveBrowser);
  });

  renderEntries([]);
  updateSelectionMeta();
}
