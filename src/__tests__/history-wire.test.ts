/**
 * Smoke tests for wireHistory() — R2 slice 1.
 *
 * Integration-style per OQ-3: build the DOM, call wireHistory(), dispatch
 * real events, assert observable state. No mocking of history.ts or
 * history-preferences.ts (the primary helpers being wired); only Tauri
 * boundaries (invoke — globally mocked via tauri.setup) and the modal
 * dispatch targets (archive-browser, export-dialog) are mocked.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.mock("../archive-browser", () => ({
  openArchiveBrowser: vi.fn(async () => {}),
}));

vi.mock("../export-dialog", () => ({
  openExportDialog: vi.fn(async () => {}),
}));

// Inject DOM nodes before module imports so dom-refs.ts captures them at load time.
vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>

    <button id="history-tab-mic" class="history-tab"></button>
    <button id="history-tab-system" class="history-tab"></button>
    <button id="history-tab-conversation" class="history-tab"></button>

    <button id="history-copy-conversation"></button>
    <button id="history-delete-conversation"></button>
    <button id="history-export"></button>
    <button id="archive-browse-btn"></button>

    <input id="history-search" type="text" />
    <button id="history-search-clear"></button>

    <input id="conversation-font-size" type="range" min="12" max="24" value="14" />
    <span id="conversation-font-size-value"></span>

    <div id="history-alias-controls">
      <input id="history-alias-mic-input" type="text" />
      <input id="history-alias-system-input" type="text" />
    </div>

    <!-- Targets of helper render functions; absence would cause helper crashes -->
    <div id="history-list"></div>
    <div id="transcribe-history-list"></div>
    <div id="conversation-history-list"></div>
    <div id="history-empty-state"></div>
  `;
});

import { wireHistory } from "../wiring/history.wire";
import { openArchiveBrowser } from "../archive-browser";
import { openExportDialog } from "../export-dialog";
import * as dom from "../dom-refs";
import { currentHistoryTab, history, transcribeHistory, setSettings } from "../state";
import { getSearchQuery } from "../history";
import { getHistoryAliases } from "../history-preferences";
import type { HistoryEntry, Settings } from "../types";

const mockedInvoke = vi.mocked(invoke);
const mockedOpenArchive = vi.mocked(openArchiveBrowser);
const mockedOpenExport = vi.mocked(openExportDialog);

function freshSettings(): Settings {
  return {
    history_alias_mic: "Mic",
    history_alias_system: "System",
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
  transcribeHistory.length = 0;
  mockedInvoke.mockClear();
  mockedInvoke.mockResolvedValue({} as unknown);
  mockedOpenArchive.mockClear();
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

// ---------- Tab switching ----------

describe("wireHistory — tab buttons", () => {
  it("historyTabMic click sets currentHistoryTab to mic", () => {
    dom.historyTabMic!.click();
    expect(currentHistoryTab).toBe("mic");
  });

  it("historyTabSystem click sets currentHistoryTab to system", () => {
    dom.historyTabSystem!.click();
    expect(currentHistoryTab).toBe("system");
  });

  it("historyTabConversation click sets currentHistoryTab to conversation", () => {
    dom.historyTabConversation!.click();
    expect(currentHistoryTab).toBe("conversation");
  });

  it("clicking a tab adds the active class", () => {
    dom.historyTabSystem!.click();
    expect(dom.historyTabSystem!.classList.contains("active")).toBe(true);
  });

  it("clicking a different tab clears the active class on the previous one", () => {
    dom.historyTabMic!.click();
    expect(dom.historyTabMic!.classList.contains("active")).toBe(true);
    dom.historyTabSystem!.click();
    expect(dom.historyTabMic!.classList.contains("active")).toBe(false);
    expect(dom.historyTabSystem!.classList.contains("active")).toBe(true);
  });
});

// ---------- Copy conversation ----------

describe("wireHistory — historyCopyConversation", () => {
  it("does nothing when there are no entries", async () => {
    history.length = 0;
    transcribeHistory.length = 0;
    dom.historyCopyConversation!.click();
    await Promise.resolve();
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
  });

  it("writes the built transcript to the clipboard when entries exist", async () => {
    transcribeHistory.push(fakeEntry("hello world"));
    dom.historyCopyConversation!.click();
    // Allow the async handler to resolve
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
  });
});

// ---------- Delete conversation ----------

describe("wireHistory — historyDeleteConversation", () => {
  it("shows an info toast and does not invoke when history is empty", async () => {
    history.length = 0;
    transcribeHistory.length = 0;
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockedInvoke).not.toHaveBeenCalledWith("clear_active_transcript_history");
    const toasts = document.getElementById("toast-container")!;
    expect(toasts.querySelector(".toast")).toBeTruthy();
  });

  it("does not invoke when the confirm dialog is declined", async () => {
    transcribeHistory.push(fakeEntry("a"));
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockedInvoke).not.toHaveBeenCalledWith("clear_active_transcript_history");
    confirmSpy.mockRestore();
  });

  it("invokes clear_active_transcript_history when confirmed", async () => {
    transcribeHistory.push(fakeEntry("a"));
    mockedInvoke.mockResolvedValueOnce(2 as unknown);
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    dom.historyDeleteConversation!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(mockedInvoke).toHaveBeenCalledWith("clear_active_transcript_history");
    confirmSpy.mockRestore();
  });

  it("shows an error toast when the invoke rejects", async () => {
    transcribeHistory.push(fakeEntry("a"));
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

describe("wireHistory — historyExport and archiveBrowse", () => {
  it("historyExport click calls openExportDialog", () => {
    dom.historyExport!.click();
    expect(mockedOpenExport).toHaveBeenCalled();
  });

  it("archiveBrowseBtn click calls openArchiveBrowser", () => {
    dom.archiveBrowseBtn!.click();
    expect(mockedOpenArchive).toHaveBeenCalled();
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

// ---------- Aliases ----------

describe("wireHistory — alias inputs", () => {
  it("mic alias change updates the stored mic alias", () => {
    dom.historyAliasMicInput!.value = "  Microphone  ";
    dom.historyAliasMicInput!.dispatchEvent(new Event("change", { bubbles: true }));
    expect(getHistoryAliases().mic).toBe("Microphone");
  });

  it("system alias change updates the stored system alias", () => {
    dom.historyAliasSystemInput!.value = "Speakers";
    dom.historyAliasSystemInput!.dispatchEvent(new Event("change", { bubbles: true }));
    expect(getHistoryAliases().system).toBe("Speakers");
  });

  it("Enter key on the mic alias commits the value", () => {
    dom.historyAliasMicInput!.value = "Headset";
    const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    dom.historyAliasMicInput!.dispatchEvent(event);
    expect(getHistoryAliases().mic).toBe("Headset");
    expect(event.defaultPrevented).toBe(true);
  });

  it("Enter key on the system alias commits the value", () => {
    dom.historyAliasSystemInput!.value = "Output";
    const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    dom.historyAliasSystemInput!.dispatchEvent(event);
    expect(getHistoryAliases().system).toBe("Output");
    expect(event.defaultPrevented).toBe(true);
  });

  it("non-Enter keys do not commit the alias", () => {
    dom.historyAliasMicInput!.value = "Pending";
    const before = getHistoryAliases().mic;
    const event = new KeyboardEvent("keydown", { key: "a", bubbles: true, cancelable: true });
    dom.historyAliasMicInput!.dispatchEvent(event);
    // Alias unchanged until change or Enter
    expect(getHistoryAliases().mic).toBe(before);
  });

  it("alias commit normalises an empty value back to the default", () => {
    // First set a non-default alias
    dom.historyAliasMicInput!.value = "Custom";
    dom.historyAliasMicInput!.dispatchEvent(new Event("change", { bubbles: true }));
    expect(getHistoryAliases().mic).toBe("Custom");

    // Then clear it
    dom.historyAliasMicInput!.value = "   ";
    dom.historyAliasMicInput!.dispatchEvent(new Event("change", { bubbles: true }));
    // setHistoryAlias normalises empty/whitespace back to the per-key default
    expect(getHistoryAliases().mic).not.toBe("Custom");
  });
});
