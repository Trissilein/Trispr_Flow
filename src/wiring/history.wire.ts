// History toolbar wiring (R2 slice 1).
//
// Owns DOM event listeners for the history panel toolbar: tab switching, copy
// and delete conversation, search, archive browser, export, font size, and
// the mic/system speaker aliases.
//
// Extracted from event-listeners.ts so the file-level dependency graph reflects
// the UI domain boundaries. Pattern (OQ-3, 2026-05-15):
//   - export function wire<Domain>(): void
//   - imports dom and helpers from existing modules directly (file-level peer)
//   - local closures lifted to module-scope private functions

import { invoke } from "@tauri-apps/api/core";
import * as dom from "../dom-refs";
import {
  buildConversationHistory,
  buildConversationText,
  scheduleHistoryRender,
  setHistoryTab,
  setSearchQuery,
  syncHistoryToolbarState,
} from "../history";
import { setHistoryAlias, setHistoryFontSize } from "../history-preferences";
import { openArchiveBrowser } from "../archive-browser";
import { openExportDialog } from "../export-dialog";
import { showToast } from "../toast";
import { updateRangeAria } from "../accessibility";
import { currentHistoryTab, history, transcribeHistory } from "../state";
import { persistSettings } from "../settings-persist";

function commitAlias(key: "mic" | "system", input: HTMLInputElement | null): void {
  if (!input) return;
  input.value = setHistoryAlias(key, input.value);
  syncHistoryToolbarState();
  scheduleHistoryRender();
  void persistSettings();
}

export function wireHistory(): void {
  dom.historyTabMic?.addEventListener("click", () => setHistoryTab("mic"));
  dom.historyTabSystem?.addEventListener("click", () => setHistoryTab("system"));
  dom.historyTabConversation?.addEventListener("click", () => setHistoryTab("conversation"));

  dom.historyCopyConversation?.addEventListener("click", async () => {
    const entries = buildConversationHistory();
    if (!entries.length) return;
    const transcript = buildConversationText(entries);
    try {
      await navigator.clipboard.writeText(transcript);
    } catch {
      showToast({ type: "error", title: "Kopieren fehlgeschlagen", message: "Clipboard-Zugriff verweigert." });
    }
  });

  dom.historyDeleteConversation?.addEventListener("click", async () => {
    const totalEntries = history.length + transcribeHistory.length;
    if (totalEntries === 0) {
      showToast({
        type: "info",
        title: "Nichts zu löschen",
        message: "Der aktuelle Verlauf ist bereits leer.",
      });
      return;
    }

    const confirmed = window.confirm(
      "Gesamtes Transkript (Input + System) aus dem aktuellen Verlauf löschen?\n\nDiese Aktion kann nicht rückgängig gemacht werden."
    );
    if (!confirmed) return;

    try {
      const deletedCount = await invoke<number>("clear_active_transcript_history");
      showToast({
        type: "success",
        title: "Transkript gelöscht",
        message: `${deletedCount} Einträge wurden dauerhaft entfernt.`,
      });
    } catch (error) {
      showToast({
        type: "error",
        title: "Löschen fehlgeschlagen",
        message: String(error),
      });
    }
  });

  dom.historyExport?.addEventListener("click", () => {
    void openExportDialog();
  });

  dom.archiveBrowseBtn?.addEventListener("click", () => {
    void openArchiveBrowser();
  });

  dom.historySearch?.addEventListener("input", () => {
    if (!dom.historySearch) return;
    const query = dom.historySearch.value;
    setSearchQuery(query);
  });

  dom.historySearchClear?.addEventListener("click", () => {
    if (!dom.historySearch) return;
    dom.historySearch.value = "";
    setSearchQuery("");
    dom.historySearch.focus();
  });

  dom.conversationFontSize?.addEventListener("input", () => {
    if (!dom.conversationFontSize) return;
    const size = setHistoryFontSize(currentHistoryTab, Number(dom.conversationFontSize.value));
    document.documentElement.style.setProperty("--history-active-font-size", `${size}px`);
    if (dom.conversationFontSizeValue) {
      dom.conversationFontSizeValue.textContent = `${size}px`;
    }
    updateRangeAria("conversation-font-size", size);
    scheduleHistoryRender();
  });

  dom.historyAliasMicInput?.addEventListener("change", () =>
    commitAlias("mic", dom.historyAliasMicInput));
  dom.historyAliasMicInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); commitAlias("mic", dom.historyAliasMicInput); }
  });

  dom.historyAliasSystemInput?.addEventListener("change", () =>
    commitAlias("system", dom.historyAliasSystemInput));
  dom.historyAliasSystemInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); commitAlias("system", dom.historyAliasSystemInput); }
  });
}
