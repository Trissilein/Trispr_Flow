// History management and panel state functions

import { escapeHtml } from "./utils";
import type { HistoryEntry, HistoryTab } from "./types";
import { history, transcribeHistory, currentHistoryTab, setCurrentHistoryTab as setCurrentTab } from "./state";
import * as dom from "./dom-refs";
import { formatTime } from "./ui-helpers";
import { updateChaptersVisibility } from "./chapters";
import { updateRangeAria } from "./accessibility";
import {
  getHistoryAliases,
  getHistoryFontSize,
} from "./history-preferences";
import {
  buildRefinementWordDiff,
  getRefinementSnapshot,
  type RefinementDiffToken,
  setInspectorFocus,
} from "./refinement-inspector";

export function buildConversationHistory(): HistoryEntry[] {
  const combined = [...history, ...transcribeHistory];
  return combined.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
}

export function buildConversationText(entries: HistoryEntry[]) {
  return entries
    .map((entry) => {
      const speaker = speakerName(entry);
      return `[${formatTime(entry.timestamp_ms)}] ${speaker}: ${getPreferredEntryText(entry)}`;
    })
    .join("\n");
}

export type ExportFormat = "txt" | "md" | "json";

export interface Chapter {
  id: string;
  label: string;
  timestamp_ms: number;
  entry_count: number;
}

/**
 * Generates chapters based on time intervals
 * @param entries - History entries to segment
 * @param intervalMinutes - Time interval in minutes between chapters (default: 5)
 * @returns Array of chapters
 */
export function generateTimeBasedChapters(entries: HistoryEntry[], intervalMinutes: number = 5): Chapter[] {
  if (!entries.length) return [];

  const sortedEntries = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const intervalMs = intervalMinutes * 60 * 1000;
  const startTime = sortedEntries[0].timestamp_ms;

  const chapters: Chapter[] = [];
  let currentChapterTime = startTime;
  let currentChapterEntries = 0;
  let chapterIndex = 1;

  sortedEntries.forEach((entry) => {
    const timeSinceChapter = entry.timestamp_ms - currentChapterTime;

    if (timeSinceChapter >= intervalMs && currentChapterEntries > 0) {
      // Create chapter
      chapters.push({
        id: `chapter-${chapterIndex}`,
        label: `Chapter ${chapterIndex}`,
        timestamp_ms: currentChapterTime,
        entry_count: currentChapterEntries,
      });

      // Start new chapter
      currentChapterTime = entry.timestamp_ms;
      currentChapterEntries = 1;
      chapterIndex++;
    } else {
      currentChapterEntries++;
    }
  });

  // Add final chapter
  if (currentChapterEntries > 0) {
    chapters.push({
      id: `chapter-${chapterIndex}`,
      label: `Chapter ${chapterIndex}`,
      timestamp_ms: currentChapterTime,
      entry_count: currentChapterEntries,
    });
  }

  return chapters;
}

/**
 * Generates chapters based on silence gaps between entries
 * @param entries - History entries to segment
 * @param silenceThresholdMs - Minimum silence gap in milliseconds to trigger new chapter (default: 2000ms = 2s)
 * @returns Array of chapters
 */
export function generateSilenceBasedChapters(entries: HistoryEntry[], silenceThresholdMs: number = 2000): Chapter[] {
  if (!entries.length) return [];

  const sortedEntries = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const chapters: Chapter[] = [];
  let currentChapterStartTime = sortedEntries[0].timestamp_ms;
  let currentChapterEntries: HistoryEntry[] = [];
  let chapterIndex = 1;

  sortedEntries.forEach((entry, index) => {
    const isLastEntry = index === sortedEntries.length - 1;

    // Add entry to current chapter
    currentChapterEntries.push(entry);

    if (!isLastEntry) {
      const nextEntry = sortedEntries[index + 1];
      const silenceGap = nextEntry.timestamp_ms - entry.timestamp_ms;

      // If silence gap exceeds threshold, create a new chapter
      if (silenceGap >= silenceThresholdMs) {
        chapters.push({
          id: `chapter-silence-${chapterIndex}`,
          label: `Chapter ${chapterIndex}`,
          timestamp_ms: currentChapterStartTime,
          entry_count: currentChapterEntries.length,
        });

        // Start new chapter
        currentChapterStartTime = nextEntry.timestamp_ms;
        currentChapterEntries = [];
        chapterIndex++;
      }
    } else {
      // Last entry - finalize current chapter
      if (currentChapterEntries.length > 0) {
        chapters.push({
          id: `chapter-silence-${chapterIndex}`,
          label: `Chapter ${chapterIndex}`,
          timestamp_ms: currentChapterStartTime,
          entry_count: currentChapterEntries.length,
        });
      }
    }
  });

  return chapters;
}

/**
 * Generates chapters using hybrid approach (silence + time)
 * @param entries - History entries to segment
 * @param silenceThresholdMs - Minimum silence gap to trigger new chapter (default: 2000ms)
 * @param maxChapterDurationMs - Maximum chapter duration before forcing split (default: 10 minutes)
 * @returns Array of chapters
 */
export function generateHybridChapters(
  entries: HistoryEntry[],
  silenceThresholdMs: number = 2000,
  maxChapterDurationMs: number = 10 * 60 * 1000
): Chapter[] {
  if (!entries.length) return [];

  const sortedEntries = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const chapters: Chapter[] = [];
  let currentChapterStartTime = sortedEntries[0].timestamp_ms;
  let currentChapterEntries: HistoryEntry[] = [];
  let chapterIndex = 1;

  sortedEntries.forEach((entry, index) => {
    const isLastEntry = index === sortedEntries.length - 1;

    // Check if adding this entry would exceed max duration
    const chapterDurationWithEntry = entry.timestamp_ms - currentChapterStartTime;
    const wouldExceedDuration = chapterDurationWithEntry >= maxChapterDurationMs && currentChapterEntries.length > 0;

    // If adding this entry would force a break, create chapter BEFORE adding it
    if (wouldExceedDuration) {
      chapters.push({
        id: `chapter-hybrid-${chapterIndex}`,
        label: `Chapter ${chapterIndex}`,
        timestamp_ms: currentChapterStartTime,
        entry_count: currentChapterEntries.length,
      });

      // Start new chapter with current entry
      currentChapterStartTime = entry.timestamp_ms;
      currentChapterEntries = [];
      chapterIndex++;
    }

    // Add entry to current chapter
    currentChapterEntries.push(entry);

    if (!isLastEntry) {
      const nextEntry = sortedEntries[index + 1];
      const silenceGap = nextEntry.timestamp_ms - entry.timestamp_ms;

      // Create new chapter if silence gap exceeded
      if (silenceGap >= silenceThresholdMs) {
        chapters.push({
          id: `chapter-hybrid-${chapterIndex}`,
          label: `Chapter ${chapterIndex}`,
          timestamp_ms: currentChapterStartTime,
          entry_count: currentChapterEntries.length,
        });

        // Start new chapter with the NEXT entry
        currentChapterStartTime = nextEntry.timestamp_ms;
        currentChapterEntries = [];
        chapterIndex++;
      }
    } else {
      // Last entry - finalize current chapter
      if (currentChapterEntries.length > 0) {
        chapters.push({
          id: `chapter-hybrid-${chapterIndex}`,
          label: `Chapter ${chapterIndex}`,
          timestamp_ms: currentChapterStartTime,
          entry_count: currentChapterEntries.length,
        });
      }
    }
  });

  return chapters;
}

// ---------------------------------------------------------------------------
// Export helper functions
// ---------------------------------------------------------------------------

const PAUSE_THRESHOLD_MS = 5 * 60 * 1000; // 5 minutes

function fmtRelOffset(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  return h > 0
    ? `+${h}:${String(m).padStart(2, "0")}:${String(ss).padStart(2, "0")}`
    : `+${String(m).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

function fmtTime(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function fmtDate(ms: number): string {
  return new Date(ms).toLocaleDateString("sv"); // "2026-03-02"
}

function speakerName(entry: HistoryEntry): string {
  const snapshot = entry.speaker_name?.trim();
  if (snapshot) return snapshot;
  const aliases = getHistoryAliases();
  if (entry.source === "mic") return aliases.mic;
  return aliases.system;
}

function entryText(entry: HistoryEntry): string {
  return entry.refinement?.refined ?? entry.text;
}

/**
 * Builds export text in the requested format
 * @param entries - Array of history entries to export
 * @param format - Export format: 'txt' | 'md' | 'json'
 * @returns Formatted text ready for file export
 */
export function buildExportText(entries: HistoryEntry[], format: ExportFormat): string {
  const sortedEntries = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const now = new Date().toISOString();

  if (format === "txt") {
    return buildExportTxt(sortedEntries, now);
  } else if (format === "md") {
    return buildExportMarkdown(sortedEntries, now);
  } else if (format === "json") {
    return buildExportJson(sortedEntries, now);
  }

  return "";
}

function buildExportTxt(entries: HistoryEntry[], _exportDate: string): string {
  const sorted = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const lines: string[] = [];
  let sectionStart: number | null = null;
  let lastEntryMs = 0;
  let lastDate: string | null = null;

  for (const entry of sorted) {
    const isPause =
      lastEntryMs > 0 && entry.timestamp_ms - lastEntryMs > PAUSE_THRESHOLD_MS;

    if (sectionStart === null || isPause) {
      const dateStr = fmtDate(entry.timestamp_ms);
      const timeStr = fmtTime(entry.timestamp_ms);

      if (sectionStart === null) {
        // First section header
        lines.push(`${dateStr} \u2014 ${timeStr}`);
        lines.push("\u2500".repeat(40));
        lines.push("");
      } else if (dateStr !== lastDate) {
        // New day
        lines.push("");
        lines.push(`\u2500\u2500 ${dateStr}  ${timeStr} ${"\u2500".repeat(20)}`);
        lines.push("");
      } else {
        // Same day, new section
        lines.push("");
        lines.push(`\u2500\u2500 ${timeStr} ${"\u2500".repeat(32)}`);
        lines.push("");
      }

      sectionStart = entry.timestamp_ms;
      lastDate = dateStr;
    }

    const offset = fmtRelOffset(entry.timestamp_ms - sectionStart!);
    const speaker = speakerName(entry);
    lines.push(`[${offset}] ${speaker}: ${entryText(entry)}`);
    lastEntryMs = entry.timestamp_ms;
  }

  return lines.join("\n");
}

function buildExportMarkdown(entries: HistoryEntry[], exportDate: string): string {
  const sorted = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const lines: string[] = [
    "# Transcript Export",
    "",
    `**Date**: ${exportDate}`,
    `**Entries**: ${sorted.length}`,
    "",
    "---",
    "",
  ];

  let sectionStart: number | null = null;
  let lastEntryMs = 0;
  let lastDate: string | null = null;
  let tableOpen = false;

  for (const entry of sorted) {
    const isPause =
      lastEntryMs > 0 && entry.timestamp_ms - lastEntryMs > PAUSE_THRESHOLD_MS;

    if (sectionStart === null || isPause) {
      const dateStr = fmtDate(entry.timestamp_ms);
      const timeStr = fmtTime(entry.timestamp_ms);

      if (tableOpen) {
        lines.push("");
      }

      if (sectionStart === null || dateStr !== lastDate) {
        lines.push(`## ${dateStr} \u2014 ${timeStr}`);
      } else {
        lines.push(`## ${timeStr}`);
      }
      lines.push("");
      lines.push("| Zeit | Sprecher | Text |");
      lines.push("| ---- | -------- | ---- |");

      sectionStart = entry.timestamp_ms;
      lastDate = dateStr;
      tableOpen = true;
    }

    const offset = fmtRelOffset(entry.timestamp_ms - sectionStart!);
    const speaker = speakerName(entry);
    // Escape pipe characters in text for markdown table
    const text = entryText(entry).replace(/\|/g, "\\|");
    lines.push(`| ${offset} | ${speaker} | ${text} |`);
    lastEntryMs = entry.timestamp_ms;
  }

  lines.push("");
  lines.push("---");
  lines.push("*Generated by Trispr Flow*");

  return lines.join("\n");
}

export function buildExportJson(entries: HistoryEntry[], exportDate: string): string {
  const sorted = [...entries].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  const exportData = {
    export_date: exportDate,
    format_version: "2.0",
    entry_count: sorted.length,
    entries: sorted.map((entry) => ({
      id: entry.id,
      timestamp_ms: entry.timestamp_ms,
      timestamp: new Date(entry.timestamp_ms).toISOString(),
      source: entry.source,
      speaker_name: speakerName(entry),
      text: entry.text,
      refined_text: entry.refinement?.refined ?? null,
    })),
  };

  return JSON.stringify(exportData, null, 2);
}

export interface TopicKeywords {
  [key: string]: string[];
}

export const DEFAULT_TOPICS: TopicKeywords = {
  technical: ["code", "debug", "error", "function", "variable", "api", "database"],
  meeting: ["meeting", "discuss", "agenda", "action", "deadline", "responsible"],
  personal: ["personal", "note", "reminder", "todo", "follow-up"],
};

let manualChapters: Chapter[] = [];
let topicKeywords: TopicKeywords = { ...DEFAULT_TOPICS };

// Module-level regex cache: keyword string → compiled RegExp.
// Keyed by keyword so we only compile each unique keyword once.
// Invalidated whenever topicKeywords changes via setTopicKeywords().
const _topicRegexCache = new Map<string, RegExp>();

/**
 * Get current manual chapters
 */
export function getManualChapters(): Chapter[] {
  return [...manualChapters];
}

/**
 * Add manual chapter at current position
 * @param label - Chapter label
 * @param timestamp_ms - Timestamp in milliseconds
 * @returns Updated chapters array
 */
export function addManualChapter(label: string, timestamp_ms: number): Chapter[] {
  const entries = buildConversationHistory();
  const relevantEntries = entries.filter((e) => e.timestamp_ms >= timestamp_ms);

  const chapter: Chapter = {
    id: `chapter-manual-${Date.now()}`,
    label: label || `Chapter ${manualChapters.length + 1}`,
    timestamp_ms,
    entry_count: relevantEntries.length,
  };

  manualChapters = [...manualChapters, chapter].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
  return manualChapters;
}

/**
 * Remove manual chapter by ID
 */
export function removeManualChapter(chapterId: string): Chapter[] {
  manualChapters = manualChapters.filter((c) => c.id !== chapterId);
  return manualChapters;
}

/**
 * Update manual chapter label
 */
export function updateChapterLabel(chapterId: string, newLabel: string): Chapter[] {
  manualChapters = manualChapters.map((c) =>
    c.id === chapterId ? { ...c, label: newLabel } : c
  );
  return manualChapters;
}

/**
 * Get topic keywords
 */
export function getTopicKeywords(): TopicKeywords {
  return { ...topicKeywords };
}

/**
 * Set topic keywords
 */
export function setTopicKeywords(keywords: TopicKeywords): void {
  topicKeywords = { ...keywords };
  _topicRegexCache.clear();
}

/**
 * Detect topics in text based on keywords
 * @param text - Text to analyze
 * @returns Array of detected topic names
 */
export function detectTopics(text: string): string[] {
  const lowerText = text.toLowerCase();
  const detectedTopics = new Set<string>();

  Object.entries(topicKeywords).forEach(([topic, keywords]) => {
    keywords.forEach((keyword) => {
      // Retrieve or compile cached RegExp for this keyword.
      let regex = _topicRegexCache.get(keyword);
      if (!regex) {
        regex = new RegExp(`\\b${keyword}\\b`, "gi");
        _topicRegexCache.set(keyword, regex);
      }
      // Reset lastIndex since the same RegExp instance (with flag g) is reused.
      regex.lastIndex = 0;
      if (regex.test(lowerText)) {
        detectedTopics.add(topic);
      }
    });
  });

  return Array.from(detectedTopics);
}

/**
 * Detect topics for all history entries
 * @returns Object with entry ID to topics mapping
 */
export function detectTopicsForHistory(): Record<string, string[]> {
  const result: Record<string, string[]> = {};
  const entries = buildConversationHistory();

  entries.forEach((entry) => {
    result[entry.id] = detectTopics(getPreferredEntryText(entry));
  });

  return result;
}

/**
 * Build HTML for topic badges
 * @param topics - Array of topic names
 * @returns HTML string for topic badges
 */
export function buildTopicBadges(topics: string[]): string {
  if (!topics.length) return "";
  return topics
    .map((topic) => {
      const safe = escapeHtml(topic);
      return `<span class="topic-badge" data-topic="${safe}">${safe}</span>`;
    })
    .join("");
}

// RAF guard: ensures renderHistory() is called at most once per animation frame
// even when multiple state changes occur synchronously in one tick.
let _historyRenderFrame: number | null = null;

export function scheduleHistoryRender(): void {
  if (_historyRenderFrame !== null) return;
  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    _historyRenderFrame = window.requestAnimationFrame(() => {
      _historyRenderFrame = null;
      renderHistory();
    });
  } else {
    // Fallback for environments without RAF (e.g. tests)
    _historyRenderFrame = window.setTimeout(() => {
      _historyRenderFrame = null;
      renderHistory();
    }, 16) as unknown as number;
  }
}

let currentSearchQuery = "";
let selectedTopicFilters: Set<string> = new Set();

/**
 * Set search query and re-render history
 */
export function setSearchQuery(query: string) {
  currentSearchQuery = query.trim().toLowerCase();
  scheduleHistoryRender();
}

/**
 * Get current search query
 */
export function getSearchQuery(): string {
  return currentSearchQuery;
}

/**
 * Toggle topic filter
 */
export function toggleTopicFilter(topic: string): void {
  if (selectedTopicFilters.has(topic)) {
    selectedTopicFilters.delete(topic);
  } else {
    selectedTopicFilters.add(topic);
  }
  scheduleHistoryRender();
}

/**
 * Get selected topic filters
 */
export function getSelectedTopicFilters(): string[] {
  return Array.from(selectedTopicFilters);
}

/**
 * Clear all topic filters
 */
export function clearTopicFilters(): void {
  selectedTopicFilters.clear();
  scheduleHistoryRender();
}

/**
 * Filter entries based on search query
 */
function filterEntriesBySearch(entries: HistoryEntry[]): HistoryEntry[] {
  if (!currentSearchQuery) return entries;
  return entries.filter((entry) => {
    if (entry.text.toLowerCase().includes(currentSearchQuery)) return true;
    const refinementState = getRefinementViewState(entry);
    if (!refinementState) return false;
    if ((refinementState.raw ?? "").toLowerCase().includes(currentSearchQuery)) return true;
    if ((refinementState.refined ?? "").toLowerCase().includes(currentSearchQuery)) return true;
    return false;
  });
}

/**
 * Filter entries by selected topics
 */
function filterEntriesByTopic(entries: HistoryEntry[]): HistoryEntry[] {
  if (selectedTopicFilters.size === 0) return entries;
  return entries.filter((entry) => {
    const entryTopics = detectTopics(getPreferredEntryText(entry));
    return entryTopics.some((topic) => selectedTopicFilters.has(topic));
  });
}

/**
 * Highlight search matches in text
 */
function highlightSearchMatches(text: string): string {
  const escaped = escapeHtml(text);
  if (!currentSearchQuery) return escaped;

  const regex = new RegExp(`(${currentSearchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  return escaped.replace(regex, "<mark>$1</mark>");
}

function setEntryTextContent(node: HTMLElement, text: string): void {
  if (currentSearchQuery) {
    node.innerHTML = highlightSearchMatches(text);
  } else {
    node.textContent = text;
  }
}

function normalizeForComparison(text: string): string {
  return text.replace(/\s+/g, " ").trim().toLowerCase();
}

type RefinementViewState = {
  status: "idle" | "refining" | "refined" | "error";
  raw: string;
  refined?: string;
};

function getRefinementViewState(entry: HistoryEntry): RefinementViewState | null {
  const snapshot = getRefinementSnapshot(entry.id);
  if (snapshot) {
    return {
      status: snapshot.status,
      raw: snapshot.raw?.trim() ? snapshot.raw : entry.text,
      refined: snapshot.refined?.trim() ? snapshot.refined : undefined,
    };
  }

  const refinement = entry.refinement;
  if (!refinement) return null;
  const status =
    refinement.status === "refining"
    || refinement.status === "refined"
    || refinement.status === "error"
      ? refinement.status
      : "idle";
  return {
    status,
    raw: (refinement.raw ?? "").trim() ? refinement.raw : entry.text,
    refined: (refinement.refined ?? "").trim() ? refinement.refined : undefined,
  };
}

function getRefinementTextPair(entry: HistoryEntry): { raw: string; refined: string } | null {
  const state = getRefinementViewState(entry);
  if (!state || state.status !== "refined") return null;
  const refined = state.refined?.trim();
  if (!refined) return null;
  return { raw: state.raw, refined };
}

function getPreferredEntryText(entry: HistoryEntry): string {
  const pair = getRefinementTextPair(entry);
  if (!pair) return entry.text;
  return pair.refined;
}

function buildRefinementDiffSummary(raw: string, refined: string): HTMLElement | null {
  const diff = buildRefinementWordDiff(raw, refined).filter((token) => token.kind !== "same");
  if (diff.length === 0) return null;

  const MAX_DIFF_TOKENS = 40;
  const summary = document.createElement("div");
  summary.className = "history-refinement-diff";

  diff.slice(0, MAX_DIFF_TOKENS).forEach((token: RefinementDiffToken) => {
    const el = document.createElement("span");
    el.className = `history-refinement-diff-token ${token.kind === "added" ? "is-added" : "is-removed"}`;
    el.textContent = `${token.kind === "added" ? "+" : "-"}${token.token}`;
    summary.appendChild(el);
  });

  if (diff.length > MAX_DIFF_TOKENS) {
    const more = document.createElement("span");
    more.className = "history-refinement-diff-token is-more";
    more.textContent = `+${diff.length - MAX_DIFF_TOKENS} more changes`;
    summary.appendChild(more);
  }

  return summary;
}

type EntryTextPresentation = {
  element: HTMLElement;
  displayText: string;
};

function buildEntryTextPresentation(entry: HistoryEntry, baseClassName: string): EntryTextPresentation {
  const pair = getRefinementTextPair(entry);
  if (!pair) {
    const fallback = document.createElement("div");
    fallback.className = baseClassName;
    setEntryTextContent(fallback, entry.text);
    return {
      element: fallback,
      displayText: entry.text,
    };
  }

  const root = document.createElement("div");
  root.className = `${baseClassName} history-refinement-view`;

  const rawBlock = document.createElement("div");
  rawBlock.className = "history-refinement-original";
  const rawLabel = document.createElement("div");
  rawLabel.className = "history-refinement-label";
  rawLabel.textContent = "Original";
  const rawText = document.createElement("div");
  rawText.className = "history-refinement-original-text";
  setEntryTextContent(rawText, pair.raw);
  rawBlock.append(rawLabel, rawText);

  const refinedBlock = document.createElement("div");
  refinedBlock.className = "history-refinement-final";
  const refinedLabel = document.createElement("div");
  refinedLabel.className = "history-refinement-label";
  refinedLabel.textContent = "Refined";
  const refinedText = document.createElement("div");
  refinedText.className = "history-refinement-final-text";
  setEntryTextContent(refinedText, pair.refined);
  refinedBlock.append(refinedLabel, refinedText);

  root.append(refinedBlock, rawBlock);

  if (normalizeForComparison(pair.raw) !== normalizeForComparison(pair.refined)) {
    const diffSummary = buildRefinementDiffSummary(pair.raw, pair.refined);
    if (diffSummary) {
      root.appendChild(diffSummary);
    }
  }

  return {
    element: root,
    displayText: pair.refined,
  };
}

type RefinementChipState =
  | "raw_only"
  | "refining"
  | "refined"
  | "refined_no_change"
  | "failed";

function getRefinementChipState(entry: HistoryEntry): RefinementChipState {
  const state = getRefinementViewState(entry);
  if (!state) return "raw_only";
  if (state.status === "refining") return "refining";
  if (state.status === "error") return "failed";
  if (state.status === "refined") {
    const raw = state.raw.trim();
    const refined = (state.refined ?? "").trim();
    if (raw.length > 0 && raw === refined) return "refined_no_change";
    return "refined";
  }
  return "raw_only";
}

function getRefinementChipLabel(state: RefinementChipState): string {
  if (state === "refining") return "Refining";
  if (state === "refined") return "Refined";
  if (state === "refined_no_change") return "Refined (no change)";
  if (state === "failed") return "Refine failed";
  return "Raw only";
}

function getRefinementChipClass(state: RefinementChipState): string {
  if (state === "refining") return "is-refining";
  if (state === "refined") return "is-refined";
  if (state === "refined_no_change") return "is-no-change";
  if (state === "failed") return "is-failed";
  return "is-raw";
}

function buildRefinementChip(entry: HistoryEntry): HTMLElement {
  const state = getRefinementChipState(entry);
  const hasSnapshot = Boolean(getRefinementSnapshot(entry.id) || entry.refinement);
  const chipTag = hasSnapshot ? "button" : "span";
  const chip = document.createElement(chipTag);
  chip.className = `refinement-chip ${getRefinementChipClass(state)}`;
  chip.textContent = getRefinementChipLabel(state);
  chip.setAttribute("aria-label", `Refinement status: ${chip.textContent}`);

  if (hasSnapshot && chip instanceof HTMLButtonElement) {
    chip.type = "button";
    chip.title = "Open refinement inspector for this entry";
    chip.addEventListener("click", (event) => {
      event.stopPropagation();
      setInspectorFocus(entry.id);
    });
  }

  return chip;
}

function buildConversationMessage(entry: HistoryEntry, role: "mic" | "system"): HTMLElement {
  const sender = speakerName(entry);

  const wrapper = document.createElement("article");
  wrapper.className = `chat-message history-entry chat-message--${role}`;
  wrapper.dataset.entryId = entry.id;

  const bubble = document.createElement("div");
  bubble.className = "chat-message-bubble";

  const meta = document.createElement("div");
  meta.className = "chat-message-meta";

  const senderEl = document.createElement("span");
  senderEl.className = "chat-message-sender";
  senderEl.textContent = sender;

  const timeEl = document.createElement("span");
  timeEl.className = "chat-message-time";
  timeEl.textContent = formatTime(entry.timestamp_ms);

  const metaRight = document.createElement("span");
  metaRight.className = "chat-message-meta-right";
  metaRight.append(timeEl, buildRefinementChip(entry));

  meta.append(senderEl, metaRight);

  const textPresentation = buildEntryTextPresentation(entry, "chat-message-text");
  bubble.append(meta, textPresentation.element);
  wrapper.appendChild(bubble);
  return wrapper;
}

function applyActiveHistoryFontSize(): number {
  const fontSize = getHistoryFontSize(currentHistoryTab);
  document.documentElement.style.setProperty("--history-active-font-size", `${fontSize}px`);
  return fontSize;
}

export function syncHistoryToolbarState(): void {
  const isConversation = currentHistoryTab === "conversation";

  if (dom.historyCopyConversation) {
    dom.historyCopyConversation.style.display = isConversation ? "inline-flex" : "none";
  }
  if (dom.historyAliasControls) {
    dom.historyAliasControls.style.display = isConversation ? "inline-flex" : "none";
  }
  if (dom.conversationFontControls) {
    dom.conversationFontControls.style.display = "inline-flex";
  }

  const aliases = getHistoryAliases();
  if (dom.historyAliasMicInput) dom.historyAliasMicInput.value = aliases.mic;
  if (dom.historyAliasSystemInput) dom.historyAliasSystemInput.value = aliases.system;

  const fontSize = applyActiveHistoryFontSize();
  if (dom.conversationFontSize) {
    dom.conversationFontSize.value = String(fontSize);
  }
  if (dom.conversationFontSizeValue) {
    dom.conversationFontSizeValue.textContent = `${fontSize}px`;
  }
  updateRangeAria("conversation-font-size", fontSize);
}

export function renderHistory() {
  if (!dom.historyList) return;
  const historyList = dom.historyList;
  syncHistoryToolbarState();
  let dataset =
    currentHistoryTab === "mic"
      ? history
      : currentHistoryTab === "system"
        ? transcribeHistory
        : buildConversationHistory();

  // Apply search and topic filters
  dataset = filterEntriesBySearch(dataset);
  dataset = filterEntriesByTopic(dataset);

  if (!dataset.length) {
    const emptyIcon = currentHistoryTab === "mic" ? "🎤" : currentHistoryTab === "system" ? "🔊" : "💬";
    const emptyTitle = currentHistoryTab === "conversation" ? "No conversation yet" : "No transcripts yet";
    const emptyMessage =
      currentHistoryTab === "mic"
        ? "Start dictating to build your input history."
        : currentHistoryTab === "system"
          ? "Start system audio capture to build your system audio history."
          : "Build input or system audio entries to generate the conversation view.";
    historyList.innerHTML =
      `<div class="empty-state compact">
        <div class="empty-state-icon">${emptyIcon}</div>
        <div class="empty-state-text">${emptyTitle}</div>
        <div class="empty-state-hint">${emptyMessage}</div>
      </div>`;
    return;
  }

  historyList.innerHTML = "";

  if (currentHistoryTab === "conversation") {
    const systemEntryIds = new Set(transcribeHistory.map((e) => e.id));
    const feed = document.createElement("div");
    feed.className = "chat-feed";
    dataset.forEach((entry) => {
      const role = systemEntryIds.has(entry.id) ? "system" : "mic";
      feed.appendChild(buildConversationMessage(entry, role));
    });
    historyList.appendChild(feed);
    return;
  }

  dataset.forEach((entry) => {
    const wrapper = document.createElement("div");
    wrapper.className = "history-item history-entry"; // For search highlighting
    wrapper.dataset.entryId = entry.id; // For chapter navigation

    const textWrap = document.createElement("div");
    textWrap.className = "history-content";
    const textPresentation = buildEntryTextPresentation(entry, "history-text");
    const text = textPresentation.element;
    textWrap.appendChild(text);

    // Add topic badges
    const topics = detectTopics(textPresentation.displayText);
    if (topics.length > 0) {
      const topicContainer = document.createElement("div");
      topicContainer.className = "history-topics";
      topicContainer.innerHTML = buildTopicBadges(topics);

      // Add click handlers to topic badges for filtering
      // Click handled by delegated listener on historyList (see initHistoryDelegation)

      textWrap.appendChild(topicContainer);
    }

    const meta = document.createElement("div");
    meta.className = "history-meta";
    const metaText = document.createElement("span");
    if (currentHistoryTab === "conversation") {
      const speaker =
        entry.source === "output"
          ? "System audio"
          : entry.source && entry.source !== "local"
            ? `Input (${entry.source})`
            : "Input";
      metaText.textContent = `${formatTime(entry.timestamp_ms)} · ${speaker}`;
    } else {
      metaText.textContent = `${formatTime(entry.timestamp_ms)} · ${entry.source}`;
    }
    meta.append(metaText, buildRefinementChip(entry));

    textWrap.appendChild(meta);

    const actions = document.createElement("div");
    actions.className = "history-actions";

    const copyButton = document.createElement("button");
    copyButton.textContent = "Copy";
    copyButton.title = "Copy transcript text to clipboard";
    // Click handled by delegated listener on historyList (see initHistoryDelegation)

    actions.appendChild(copyButton);

    wrapper.appendChild(textWrap);
    wrapper.appendChild(actions);

    historyList.appendChild(wrapper);
  });
}

/**
 * Initialize delegated event handlers on the history list container.
 * Called once at startup instead of attaching per-element listeners on every render.
 */
export function initHistoryDelegation(): void {
  if (!dom.historyList) return;
  dom.historyList.addEventListener("click", (e) => {
    const target = e.target as Element;

    // Topic badge: filter by topic on click
    const badge = target.closest(".topic-badge");
    if (badge) {
      e.stopPropagation();
      const topic = (badge as HTMLElement).dataset.topic;
      if (topic) toggleTopicFilter(topic);
      return;
    }

    // Copy button: look up entry by wrapper's data-entry-id
    const button = target.closest(".history-actions button");
    if (button) {
      const wrapper = target.closest("[data-entry-id]");
      const entryId = (wrapper as HTMLElement | null)?.dataset.entryId;
      if (!entryId) return;
      const entry = [...history, ...transcribeHistory].find((e) => e.id === entryId);
      if (entry) {
        const text = buildEntryTextPresentation(entry, "history-text").displayText;
        navigator.clipboard.writeText(text).catch(() => {});
      }
    }
  });
}

export function setHistoryTab(tab: HistoryTab) {
  setCurrentTab(tab);
  if (dom.historyTabMic) dom.historyTabMic.classList.toggle("active", tab === "mic");
  if (dom.historyTabSystem) dom.historyTabSystem.classList.toggle("active", tab === "system");
  if (dom.historyTabConversation) dom.historyTabConversation.classList.toggle("active", tab === "conversation");
  scheduleHistoryRender();
  updateChaptersVisibility();
}
