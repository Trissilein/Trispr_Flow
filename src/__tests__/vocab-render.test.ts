/**
 * Safety-net tests for renderVocabulary and renderLearnedVocabChips.
 *
 * These functions live in settings/vocabulary.settings.ts.
 *
 * The DOM elements are injected via vi.hoisted so dom-refs.ts picks them up
 * at module-load time (before any import is processed).
 */

import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

// Inject DOM nodes before any module import so dom-refs.ts sees them.
vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="postproc-vocab-rows"></div>
    <div id="vocab-terms-list"></div>
    <span id="vocab-terms-count"></span>
    <div id="vocab-observing-list"></div>
  `;
});

import { addVocabRow, renderVocabulary, renderLearnedVocabChips } from "../settings/vocabulary.settings";
import { setSettings, settings } from "../state";
import type { Settings } from "../types";

const mockedInvoke = vi.mocked(invoke);

function makeSettings(overrides: Partial<Settings>): Settings {
  return overrides as unknown as Settings;
}

// Convenience accessors — reads live DOM so DOM mutations are visible.
const vocabRows = () => document.getElementById("postproc-vocab-rows")!;
const termsList = () => document.getElementById("vocab-terms-list")!;
const observingList = () => document.getElementById("vocab-observing-list")!;
const termsCount = () => document.getElementById("vocab-terms-count")!;

// --- renderVocabulary ---

describe("renderVocabulary", () => {
  beforeEach(() => {
    vocabRows().innerHTML = "";
    setSettings(null);
    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValue(undefined);
  });

  it("is a no-op when settings is null", () => {
    renderVocabulary();
    expect(vocabRows().innerHTML).toBe("");
  });

  it("renders the empty-state placeholder when vocab is empty", () => {
    setSettings(makeSettings({ postproc_custom_vocab: {} }));
    renderVocabulary();
    expect(vocabRows().querySelector(".vocab-empty-state")).toBeTruthy();
  });

  it("renders one row per vocab entry", () => {
    setSettings(makeSettings({ postproc_custom_vocab: { api: "API", gdd: "GDD" } }));
    renderVocabulary();
    expect(vocabRows().querySelectorAll(".vocab-row")).toHaveLength(2);
  });

  it("clears previous rows before re-rendering", () => {
    setSettings(makeSettings({ postproc_custom_vocab: { api: "API", gdd: "GDD" } }));
    renderVocabulary();
    // Change vocab and call again
    setSettings(makeSettings({ postproc_custom_vocab: { api: "API" } }));
    renderVocabulary();
    expect(vocabRows().querySelectorAll(".vocab-row")).toHaveLength(1);
  });

  it("row inputs carry the original and replacement values", () => {
    setSettings(makeSettings({ postproc_custom_vocab: { api: "API" } }));
    renderVocabulary();
    const inputs = vocabRows().querySelectorAll<HTMLInputElement>(".vocab-input");
    expect(inputs[0]?.value).toBe("api");
    expect(inputs[1]?.value).toBe("API");
  });
});

// --- addVocabRow ---

describe("addVocabRow", () => {
  beforeEach(() => {
    vocabRows().innerHTML = "";
    setSettings(makeSettings({ postproc_custom_vocab: {} }));
    mockedInvoke.mockReset();
    mockedInvoke.mockResolvedValue(undefined);
  });

  it("appends a row with original and replacement inputs", () => {
    addVocabRow("api", "API");
    const row = vocabRows().querySelector(".vocab-row");
    const inputs = row?.querySelectorAll<HTMLInputElement>("input");
    expect(row).toBeTruthy();
    expect(inputs?.[0]?.value).toBe("api");
    expect(inputs?.[1]?.value).toBe("API");
  });

  it("persists the current row values on input change", async () => {
    addVocabRow("api", "API");
    const inputs = vocabRows().querySelectorAll<HTMLInputElement>("input");
    inputs[0]!.value = "gdd";
    inputs[1]!.value = "GDD";
    inputs[1]!.dispatchEvent(new Event("change"));

    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(settings?.postproc_custom_vocab).toEqual({ gdd: "GDD" });
  });

  it("removes the row and persists when remove is clicked", async () => {
    addVocabRow("api", "API");
    vocabRows().querySelector<HTMLButtonElement>(".vocab-remove")!.click();

    await vi.waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(vocabRows().querySelectorAll(".vocab-row")).toHaveLength(0);
    expect(settings?.postproc_custom_vocab).toEqual({});
  });
});

// --- renderLearnedVocabChips ---

describe("renderLearnedVocabChips", () => {
  beforeEach(() => {
    termsList().innerHTML = "";
    observingList().innerHTML = "";
    termsCount().textContent = "";
    setSettings(null);
  });

  it("renders no chips when settings is null", () => {
    renderLearnedVocabChips();
    expect(termsList().querySelectorAll(".vocab-term-chip")).toHaveLength(0);
  });

  it("renders one chip per learned term", () => {
    setSettings(makeSettings({ vocab_terms: ["API", "GDD"], edit_substitutions: [] }));
    renderLearnedVocabChips();
    expect(termsList().querySelectorAll(".vocab-term-chip")).toHaveLength(2);
  });

  it("renders learned terms sorted alphabetically", () => {
    setSettings(makeSettings({ vocab_terms: ["Zebra", "Alpha", "Mango"], edit_substitutions: [] }));
    renderLearnedVocabChips();
    const chips = [...termsList().querySelectorAll(".vocab-term-chip")];
    const labels = chips.map((c) => c.querySelector("span")?.textContent);
    expect(labels).toEqual(["Alpha", "Mango", "Zebra"]);
  });

  it("renders one observing chip per pending substitution", () => {
    setSettings(
      makeSettings({
        vocab_terms: [],
        edit_substitutions: [
          { from: "api", to: "API", count: 2, first_seen_ms: Date.now(), last_seen_ms: Date.now() },
        ],
      }),
    );
    renderLearnedVocabChips();
    expect(observingList().querySelectorAll(".vocab-term-chip.observing")).toHaveLength(1);
  });

  it("updates the count badge with learned and observed counts", () => {
    setSettings(
      makeSettings({
        vocab_terms: ["API"],
        edit_substitutions: [{ from: "gdd", to: "GDD", count: 1, first_seen_ms: Date.now(), last_seen_ms: Date.now() }],
      }),
    );
    renderLearnedVocabChips();
    expect(termsCount().textContent).toContain("1 learned");
    expect(termsCount().textContent).toContain("1 observed");
  });

  it("clears the badge when there are no terms or substitutions", () => {
    termsCount().textContent = "stale";
    setSettings(makeSettings({ vocab_terms: [], edit_substitutions: [] }));
    renderLearnedVocabChips();
    expect(termsCount().textContent).toBe("");
  });
});
