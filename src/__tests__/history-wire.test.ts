/**
 * Smoke tests for wireHistory() — R2 slice 1.
 *
 * Integration-style per OQ-3: build the DOM, call wireHistory(), dispatch
 * real events, assert observable state. No mocking of history.ts or
 * history-preferences.ts (the primary helpers being wired); only Tauri
 * boundaries (invoke — globally mocked via tauri.setup) and the modal
 * dispatch targets (export-dialog) are mocked.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.mock("../export-dialog", () => ({
  openExportDialog: vi.fn(async () => {}),
}));

// Inject DOM nodes before module imports so dom-refs.ts captures them at load time.
vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>

    <button id="history-delete-conversation"></button>
    <button id="history-export"></button>

    <input id="history-search" type="text" />
    <button id="history-search-clear"></button>

    <input id="conversation-font-size" type="range" min="12" max="24" value="14" />
    <span id="conversation-font-size-value"></span>

    <!-- Targets of helper render functions; absence would cause helper crashes -->
    <div id="history-list"></div>
    <div id="transcribe-history-list"></div>
    <div id="conversation-history-list"></div>
    <div id="history-empty-state"></div>
  `;
});

import { wireHistory } from "../wiring/history.wire";
import { openExportDialog } from "../export-dialog";
import * as dom from "../dom-refs";
import { history, setSettings } from "../state";
import { getSearchQuery } from "../history";
import type { HistoryEntry, Settings } from "../types";

const mockedInvoke = vi.mocked(invoke);
const mockedOpenExport = vi.mocked(openExportDialog);

function freshSettings(): Settings {
  return {
    history_alias_mic: "Mic",
  } as unknown as Settings;
}

function fakeEntry(text: string): HistoryEntry {
  return { id: `e-${text}`, text, timestamp: Date.now(), source: "mic" } as unknown as HistoryEntry;
}

let wired = false;
function wireOnce() {
  if (wired) return;
  wireHistory();
  wired = true;
}

beforeEach(() => {
  // Reset state and stubs each test
  setSettings(freshSettings());
  history.length = 0;
  mockedInvoke.mockClear();
  mockedInvoke.mockResolvedValue({} as unknown);
  mockedOpenExport.mockClear();

  // Reset search query via the public setter so internal state matches expectations
  dom.historySearch!.value = "";
  dom.historySearch!.dispatchEvent(new Event("input", { bubbles: true }));

  // Stub clipboard (jsdom has no navigator.clipboard by default)
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: vi.fn(async () => {}) },
  });

  // Wire on first run; subsequent runs reuse the same listeners (they're idempotent
  // since they live on persistent DOM nodes).
  wireOnce();
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ---------- Delete conversation ----------

describe("wireHistory — historyDeleteConversation", () => {
  it("shows an info toast and does not invoke when history is empty", async () => {
    history.length = 0;
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockedInvoke).not.toHaveBeenCalledWith("clear_active_transcript_history");
    const toasts = document.getElementById("toast-container")!;
    expect(toasts.querySelector(".toast")).toBeTruthy();
  });

  it("does not invoke when the confirm dialog is declined", async () => {
    history.push(fakeEntry("a"));
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockedInvoke).not.toHaveBeenCalledWith("clear_active_transcript_history");
    confirmSpy.mockRestore();
  });

  it("invokes clear_active_transcript_history when confirmed", async () => {
    history.push(fakeEntry("a"));
    mockedInvoke.mockResolvedValueOnce(2 as unknown);
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockedInvoke).toHaveBeenCalledWith("clear_active_transcript_history");
    confirmSpy.mockRestore();
  });

  it("shows an error toast when the invoke rejects", async () => {
    history.push(fakeEntry("a"));
    mockedInvoke.mockRejectedValueOnce(new Error("boom"));
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    document.getElementById("toast-container")!.innerHTML = "";
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const toasts = document.getElementById("toast-container")!;
    expect(toasts.querySelector(".toast.error")).toBeTruthy();
    confirmSpy.mockRestore();
  });
});

// ---------- Export & archive ----------

describe("wireHistory — historyExport", () => {
  it("historyExport click calls openExportDialog", () => {
    dom.historyExport!.click();
    expect(mockedOpenExport).toHaveBeenCalled();
  });

});

// ---------- Search ----------

describe("wireHistory — search", () => {
  it("typing in the search input updates the lowercased trimmed query", () => {
    dom.historySearch!.value = "  HELLO  ";
    dom.historySearch!.dispatchEvent(new Event("input", { bubbles: true }));
    expect(getSearchQuery()).toBe("hello");
  });

  it("historySearchClear empties the input and resets the query", () => {
    dom.historySearch!.value = "stuff";
    dom.historySearch!.dispatchEvent(new Event("input", { bubbles: true }));
    expect(getSearchQuery()).toBe("stuff");

    dom.historySearchClear!.click();
    expect(dom.historySearch!.value).toBe("");
    expect(getSearchQuery()).toBe("");
  });

  it("historySearchClear focuses the search input", () => {
    dom.historySearch!.value = "x";
    dom.historySearch!.dispatchEvent(new Event("input", { bubbles: true }));
    dom.historySearchClear!.click();
    expect(document.activeElement).toBe(dom.historySearch);
  });
});

// ---------- Font size ----------

describe("wireHistory — conversation font size", () => {
  it("input event updates the CSS variable to the clamped size", () => {
    dom.conversationFontSize!.value = "18";
    dom.conversationFontSize!.dispatchEvent(new Event("input", { bubbles: true }));
    expect(document.documentElement.style.getPropertyValue("--history-active-font-size")).toBe("18px");
  });

  it("input event mirrors the size into the value label", () => {
    dom.conversationFontSize!.value = "20";
    dom.conversationFontSize!.dispatchEvent(new Event("input", { bubbles: true }));
    expect(dom.conversationFontSizeValue!.textContent).toBe("20px");
  });

  it("clamps values below the 12px floor", () => {
    dom.conversationFontSize!.value = "8";
    dom.conversationFontSize!.dispatchEvent(new Event("input", { bubbles: true }));
    expect(document.documentElement.style.getPropertyValue("--history-active-font-size")).toBe("12px");
  });

  it("clamps values above the 24px ceiling", () => {
    dom.conversationFontSize!.value = "32";
    dom.conversationFontSize!.dispatchEvent(new Event("input", { bubbles: true }));
    expect(document.documentElement.style.getPropertyValue("--history-active-font-size")).toBe("24px");
  });
});
