// History toolbar wiring (R2 slice 1).
//
// Owns DOM event listeners for the history panel toolbar: delete, search,
// export and font size.
//
// Extracted from event-listeners.ts so the file-level dependency graph reflects
// the UI domain boundaries. Pattern (OQ-3, 2026-05-15):
//   - export function wire<Domain>(): void
//   - imports dom and helpers from existing modules directly (file-level peer)
//   - local closures lifted to module-scope private functions

import { invoke } from "@tauri-apps/api/core";
import * as dom from "../dom-refs";
import { scheduleHistoryRender, setSearchQuery } from "../history";
import { setHistoryFontSize } from "../history-preferences";
import { openExportDialog } from "../export-dialog";
import { showToast } from "../toast";
import { updateRangeAria } from "../accessibility";
import { history } from "../state";

export function wireHistory(): void {
  dom.historyDeleteConversation?.addEventListener("click", async () => {
    const totalEntries = history.length;
    if (totalEntries === 0) {
      showToast({
        type: "info",
        title: "Nichts zu löschen",
        message: "Der aktuelle Verlauf ist bereits leer.",
      });
      return;
    }

    const confirmed = window.confirm(
      "Gesamtes Transkript aus dem aktuellen Verlauf löschen?\n\nDiese Aktion kann nicht rückgängig gemacht werden."
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
    const size = setHistoryFontSize(Number(dom.conversationFontSize.value));
    document.documentElement.style.setProperty("--history-active-font-size", `${size}px`);
    if (dom.conversationFontSizeValue) {
      dom.conversationFontSizeValue.textContent = `${size}px`;
    }
    updateRangeAria("conversation-font-size", size);
    scheduleHistoryRender();
  });
}
