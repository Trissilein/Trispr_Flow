// History management and panel state functions

import type { HistoryEntry, HistoryTab } from "./types";
import { history, transcribeHistory, currentHistoryTab, setCurrentHistoryTab as setCurrentTab } from "./state";
import * as dom from "./dom-refs";
import { formatTime } from "./ui-helpers";
import { updateChaptersVisibility } from "./chapters";

export function buildConversationHistory(): HistoryEntry[] {
  const combined = [...history, ...transcribeHistory];
  return combined.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
}

export function buildConversationText(entries: HistoryEntry[]) {
  return entries
    .map((entry) => {
      const speaker = entry.source === "output" ? "System audio" : "Input";
      return `[${formatTime(entry.timestamp_ms)}] ${speaker}: ${entry.text}`;
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

function buildExportTxt(entries: HistoryEntry[], exportDate: string): string {
  const lines = [
    "Trispr Flow - Transcript Export",
    `Date: ${exportDate}`,
    `Entries: ${entries.length}`,
    "",
    "---",
    "",
  ];

  entries.forEach((entry) => {
    const speaker = entry.source === "output" ? "System audio" : "Input";
    const time = formatTime(entry.timestamp_ms);
    lines.push(`[${time}] ${speaker}: ${entry.text}`);
  });

  return lines.join("\n");
}

function buildExportMarkdown(entries: HistoryEntry[], exportDate: string): string {
  const lines = [
    "# Transcript Export",
    "",
    "**Date**: " + exportDate,
    "**Total Entries**: " + entries.length,
    "",
    "---",
    "",
  ];

  // Group by source for cleaner organization
  const inputEntries = entries.filter((e) => e.source === "mic");
  const outputEntries = entries.filter((e) => e.source === "output" || e.source === "system");

  if (inputEntries.length > 0) {
    lines.push("## Input Transcription");
    lines.push("");
    inputEntries.forEach((entry) => {
      const time = formatTime(entry.timestamp_ms);
      lines.push(`- **${time}**: ${entry.text}`);
    });
    lines.push("");
  }

  if (outputEntries.length > 0) {
    lines.push("## Output Transcription");
    lines.push("");
    outputEntries.forEach((entry) => {
      const time = formatTime(entry.timestamp_ms);
      const source = entry.source === "output" ? "System audio" : "System";
      lines.push(`- **${time}** (${source}): ${entry.text}`);
    });
    lines.push("");
  }

  lines.push("---");
  lines.push("*Generated by Trispr Flow*");

  return lines.join("\n");
}

export function buildExportJson(entries: HistoryEntry[], exportDate: string): string {
  const exportData = {
    export_date: exportDate,
    format_version: "1.0",
    entry_count: entries.length,
    entries: entries.map((entry) => ({
      id: entry.id,
      timestamp_ms: entry.timestamp_ms,
      timestamp: new Date(entry.timestamp_ms).toISOString(),
      source: entry.source,
      text: entry.text,
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
      // Word boundary matching: match whole words only
      const regex = new RegExp(`\\b${keyword}\\b`, "gi");
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
    result[entry.id] = detectTopics(entry.text);
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
    .map((topic) => `<span class="topic-badge" data-topic="${topic}">${topic}</span>`)
    .join("");
}

let currentSearchQuery = "";
let selectedTopicFilters: Set<string> = new Set();

/**
 * Set search query and re-render history
 */
export function setSearchQuery(query: string) {
  currentSearchQuery = query.trim().toLowerCase();
  renderHistory();
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
  renderHistory();
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
  renderHistory();
}

/**
 * Filter entries based on search query
 */
function filterEntriesBySearch(entries: HistoryEntry[]): HistoryEntry[] {
  if (!currentSearchQuery) return entries;
  return entries.filter((entry) => entry.text.toLowerCase().includes(currentSearchQuery));
}

/**
 * Filter entries by selected topics
 */
function filterEntriesByTopic(entries: HistoryEntry[]): HistoryEntry[] {
  if (selectedTopicFilters.size === 0) return entries;
  return entries.filter((entry) => {
    const entryTopics = detectTopics(entry.text);
    return entryTopics.some((topic) => selectedTopicFilters.has(topic));
  });
}

/**
 * Highlight search matches in text
 */
function highlightSearchMatches(text: string): string {
  if (!currentSearchQuery) return text;

  const regex = new RegExp(`(${currentSearchQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  return text.replace(regex, "<mark>$1</mark>");
}

export function renderHistory() {
  if (!dom.historyList) return;
  const historyList = dom.historyList;
  let dataset =
    currentHistoryTab === "mic"
      ? history
      : currentHistoryTab === "system"
        ? transcribeHistory
        : buildConversationHistory();

  // Apply search and topic filters
  dataset = filterEntriesBySearch(dataset);
  dataset = filterEntriesByTopic(dataset);

  if (dom.historyCopyConversation) {
    dom.historyCopyConversation.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }
  if (dom.conversationFontControls) {
    dom.conversationFontControls.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }

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
    const block = document.createElement("div");
    block.className = "conversation-block";
    block.textContent = buildConversationText(dataset);
    historyList.appendChild(block);
    return;
  }

  dataset.forEach((entry) => {
    const wrapper = document.createElement("div");
    wrapper.className = "history-item";
    wrapper.className += " history-entry"; // For search highlighting
    wrapper.dataset.entryId = entry.id; // For chapter navigation

    const textWrap = document.createElement("div");
    textWrap.className = "history-content";
    const text = document.createElement("div");
    text.className = "history-text";
    // Use innerHTML for search highlighting
    if (currentSearchQuery) {
      text.innerHTML = highlightSearchMatches(entry.text);
    } else {
      text.textContent = entry.text;
    }

    // Add topic badges
    const topics = detectTopics(entry.text);
    if (topics.length > 0) {
      const topicContainer = document.createElement("div");
      topicContainer.className = "history-topics";
      topicContainer.innerHTML = buildTopicBadges(topics);

      // Add click handlers to topic badges for filtering
      topicContainer.querySelectorAll(".topic-badge").forEach((badge) => {
        badge.addEventListener("click", (e) => {
          e.stopPropagation();
          const topic = (badge as HTMLElement).dataset.topic;
          if (topic) {
            toggleTopicFilter(topic);
          }
        });
      });

      text.appendChild(topicContainer);
    }

    const meta = document.createElement("div");
    meta.className = "history-meta";
    if (currentHistoryTab === "conversation") {
      const speaker =
        entry.source === "output"
          ? "System audio"
          : entry.source && entry.source !== "local"
            ? `Input (${entry.source})`
            : "Input";
      meta.textContent = `${formatTime(entry.timestamp_ms)} · ${speaker}`;
    } else {
      meta.textContent = `${formatTime(entry.timestamp_ms)} · ${entry.source}`;
    }

    textWrap.appendChild(text);
    textWrap.appendChild(meta);

    const actions = document.createElement("div");
    actions.className = "history-actions";

    const copyButton = document.createElement("button");
    copyButton.textContent = "Copy";
    copyButton.addEventListener("click", async () => {
      await navigator.clipboard.writeText(entry.text);
    });

    actions.appendChild(copyButton);

    wrapper.appendChild(textWrap);
    wrapper.appendChild(actions);

    historyList.appendChild(wrapper);
  });
}

export function setHistoryTab(tab: HistoryTab) {
  setCurrentTab(tab);
  if (dom.historyTabMic) dom.historyTabMic.classList.toggle("active", tab === "mic");
  if (dom.historyTabSystem) dom.historyTabSystem.classList.toggle("active", tab === "system");
  if (dom.historyTabConversation) dom.historyTabConversation.classList.toggle("active", tab === "conversation");
  renderHistory();
  updateChaptersVisibility();
}

