// History management and panel state functions

import type { HistoryEntry, HistoryTab } from "./types";
import { history, transcribeHistory, currentHistoryTab, setCurrentHistoryTab as setCurrentTab } from "./state";
import * as dom from "./dom-refs";
import { formatTime } from "./ui-helpers";

export function buildConversationHistory(): HistoryEntry[] {
  const combined = [...history, ...transcribeHistory];
  return combined.sort((a, b) => b.timestamp_ms - a.timestamp_ms);
}

export function buildConversationText(entries: HistoryEntry[]) {
  return entries
    .map((entry) => {
      const speaker = entry.source === "output" ? "Output" : "Input";
      return `[${formatTime(entry.timestamp_ms)}] ${speaker}: ${entry.text}`;
    })
    .join("\n");
}

export function applyPanelCollapsed(panelId: string, collapsed: boolean) {
  const panel = document.querySelector(`[data-panel="${panelId}"]`) as HTMLElement | null;
  if (!panel) return;
  panel.classList.toggle("panel-collapsed", collapsed);

  // Update aria-expanded for accessibility
  const collapseButton = panel.querySelector<HTMLButtonElement>(".panel-collapse-btn");
  if (collapseButton) {
    collapseButton.setAttribute("aria-expanded", String(!collapsed));
    collapseButton.setAttribute("title", collapsed ? "Expand" : "Collapse");
    collapseButton.setAttribute("aria-label",
      collapsed ? `Expand ${panelId} panel` : `Collapse ${panelId} panel`
    );
  }
}

export function initPanelState() {
  const panelIds = ["output", "capture", "system", "interface", "model"];
  panelIds.forEach((id) => {
    const collapsed = id !== "output";
    applyPanelCollapsed(id, collapsed);
  });
}

export function renderHistory() {
  if (!dom.historyList) return;
  const historyList = dom.historyList;
  const dataset =
    currentHistoryTab === "mic"
      ? history
      : currentHistoryTab === "system"
        ? transcribeHistory
        : buildConversationHistory();

  if (dom.historyCompose) {
    dom.historyCompose.style.display = currentHistoryTab === "mic" ? "flex" : "none";
  }
  if (dom.historyCopyConversation) {
    dom.historyCopyConversation.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }
  if (dom.historyDetachConversation) {
    dom.historyDetachConversation.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }
  if (dom.conversationFontControls) {
    dom.conversationFontControls.style.display =
      currentHistoryTab === "conversation" ? "inline-flex" : "none";
  }

  if (!dataset.length) {
    const emptyMessage =
      currentHistoryTab === "mic"
        ? "Start dictating to build your input history."
        : currentHistoryTab === "system"
          ? "Start output capture to build your output history."
          : "Build input or output entries to generate the conversation view.";
    historyList.innerHTML =
      `<div class="history-item"><div><div class="history-text">No transcripts yet.</div><div class="history-meta">${emptyMessage}</div></div></div>`;
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

    const textWrap = document.createElement("div");
    textWrap.className = "history-content";
    const text = document.createElement("div");
    text.className = "history-text";
    text.textContent = entry.text;

    const meta = document.createElement("div");
    meta.className = "history-meta";
    if (currentHistoryTab === "conversation") {
      const speaker =
        entry.source === "output"
          ? "Output"
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
}
