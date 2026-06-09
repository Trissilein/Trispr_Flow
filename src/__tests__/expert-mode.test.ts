/**
 * Block K, K5: Expert Mode Toggle — Regression Tests
 *
 * Tests the initExpertMode() behaviour:
 *  - Default initialisation (no localStorage → standard mode)
 *  - Restore from localStorage on load
 *  - Toggle switch → class + localStorage update
 *  - Label text update
 *  - Idempotent re-initialisation guard
 *
 * Note: dom-refs.ts resolves element references at import time, so each test
 * uses vi.resetModules() + dynamic import() to ensure the module is re-evaluated
 * after the DOM fixture is set up.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import indexHtml from "../../index.html?raw";

const EXPERT_MODE_KEY = "trispr-expert-mode";

function setupFixture(storedValue: string | null = null): void {
  if (storedValue !== null) {
    localStorage.setItem(EXPERT_MODE_KEY, storedValue);
  } else {
    localStorage.removeItem(EXPERT_MODE_KEY);
  }

  document.body.innerHTML = `
    <input id="expert-mode-toggle" type="checkbox" />
    <span id="expert-mode-label">Standard mode active.</span>
    <div id="expert-field-a" data-expert-only="true">Expert Field A</div>
    <div id="expert-field-b" data-expert-only="true">Expert Field B</div>
  `;
}

describe("Block K K5 — Expert Mode Toggle", () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
    document.documentElement.classList.remove("expert-mode", "standard-mode");
  });

  afterEach(() => {
    document.body.innerHTML = "";
    document.documentElement.classList.remove("expert-mode", "standard-mode");
    localStorage.clear();
  });

  // ──────────────────────────────────────────────────────────────
  // Initialisation
  // ──────────────────────────────────────────────────────────────

  it("initialises to standard-mode when localStorage is empty", async () => {
    setupFixture(null);
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    expect(document.documentElement.classList.contains("standard-mode")).toBe(true);
    expect(document.documentElement.classList.contains("expert-mode")).toBe(false);
  });

  it("initialises to expert-mode when localStorage stores '1'", async () => {
    setupFixture("1");
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    expect(document.documentElement.classList.contains("expert-mode")).toBe(true);
    expect(document.documentElement.classList.contains("standard-mode")).toBe(false);
  });

  it("sets toggle.checked to false in standard mode on init", async () => {
    setupFixture(null);
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    expect(toggle.checked).toBe(false);
  });

  it("sets toggle.checked to true in expert mode on init", async () => {
    setupFixture("1");
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    expect(toggle.checked).toBe(true);
  });

  // ──────────────────────────────────────────────────────────────
  // Toggle interaction
  // ──────────────────────────────────────────────────────────────

  it("switches to expert-mode when toggle is checked", async () => {
    setupFixture(null);
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    toggle.checked = true;
    toggle.dispatchEvent(new Event("change"));

    expect(document.documentElement.classList.contains("expert-mode")).toBe(true);
    expect(document.documentElement.classList.contains("standard-mode")).toBe(false);
  });

  it("switches back to standard-mode when toggle is unchecked", async () => {
    setupFixture("1");
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    toggle.checked = false;
    toggle.dispatchEvent(new Event("change"));

    expect(document.documentElement.classList.contains("standard-mode")).toBe(true);
    expect(document.documentElement.classList.contains("expert-mode")).toBe(false);
  });

  // ──────────────────────────────────────────────────────────────
  // localStorage persistence
  // ──────────────────────────────────────────────────────────────

  it("persists '1' in localStorage when switching to expert mode", async () => {
    setupFixture(null);
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    toggle.checked = true;
    toggle.dispatchEvent(new Event("change"));

    expect(localStorage.getItem(EXPERT_MODE_KEY)).toBe("1");
  });

  it("persists '0' in localStorage when switching back to standard mode", async () => {
    setupFixture("1");
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    toggle.checked = false;
    toggle.dispatchEvent(new Event("change"));

    expect(localStorage.getItem(EXPERT_MODE_KEY)).toBe("0");
  });

  // ──────────────────────────────────────────────────────────────
  // Label text
  // ──────────────────────────────────────────────────────────────

  it("shows 'Expert mode active' label text in expert mode", async () => {
    setupFixture(null);
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    toggle.checked = true;
    toggle.dispatchEvent(new Event("change"));

    const label = document.getElementById("expert-mode-label") as HTMLSpanElement;
    expect(label.textContent).toContain("Expert mode active");
  });

  it("shows 'Standard mode active' label text in standard mode", async () => {
    setupFixture("1");
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    const toggle = document.getElementById("expert-mode-toggle") as HTMLInputElement;
    toggle.checked = false;
    toggle.dispatchEvent(new Event("change"));

    const label = document.getElementById("expert-mode-label") as HTMLSpanElement;
    expect(label.textContent).toContain("Standard mode active");
  });

  // ──────────────────────────────────────────────────────────────
  // Idempotency
  // ──────────────────────────────────────────────────────────────

  it("does not double-register toggle listener when called twice", async () => {
    setupFixture(null);
    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();
    initExpertMode(); // second call should be a no-op

    // Classes should still reflect standard mode, not flip state
    expect(document.documentElement.classList.contains("standard-mode")).toBe(true);
  });

  it("redirects from an expert-only active tab when standard mode applies", async () => {
    setupFixture(null);
    let redirected = false;
    document.body.insertAdjacentHTML(
      "beforeend",
      `
        <button id="tab-btn-transcription" type="button"></button>
        <div id="tab-agent" class="main-tab-content active" data-expert-only="true"></div>
      `,
    );
    document.getElementById("tab-btn-transcription")?.addEventListener("click", () => {
      redirected = true;
    });

    const { initExpertMode } = await import("../expert-mode");
    initExpertMode();

    expect(redirected).toBe(true);
  });

  it("keeps daily settings visible and marks advanced settings as expert-only in index", () => {
    const template = document.createElement("template");
    template.innerHTML = indexHtml;
    const root = template.content;
    const byId = (id: string): HTMLElement => {
      const element = root.querySelector<HTMLElement>(`#${id}`);
      if (!element) throw new Error(`Missing #${id} in index.html`);
      return element;
    };
    const expectStandardControl = (id: string): void => {
      expect(byId(id).closest('[data-expert-only="true"]')).toBeNull();
    };
    const expectExpertControl = (id: string): void => {
      expect(byId(id).closest('[data-expert-only="true"]')).not.toBeNull();
    };

    expect(indexHtml).toContain("PTT audio standby window");
    expect(indexHtml).toContain("Capture Basics");
    expect(indexHtml).toContain("AI Runtime");
    expect(indexHtml).toContain("Diagnostics &amp; Filters");
    expect(indexHtml).toContain("Does not keep Whisper or Ollama models loaded");
    expect(indexHtml).toContain("Refinement flow");
    expect(indexHtml).toContain("Whisper creates raw text");
    expect(indexHtml).toMatch(/data-panel="system"[^>]*data-expert-only="true"/);
    expect(indexHtml).toMatch(/data-panel="interface"[^>]*data-expert-only="true"/);
    expect(indexHtml).toMatch(/id="ptt-hot-keepalive"[\s\S]*aria-label="PTT audio standby window"/);

    [
      "capture-enabled-toggle",
      "device-select",
      "mode-select",
      "ptt-hotkey",
      "whisper-input-language-select",
      "ai-fallback-enabled",
      "ai-fallback-local-primary-status",
      "ai-refinement-models-expander",
      "refinement-pipeline-note",
      "pipeline-node-transcribe",
      "pipeline-node-rules",
      "pipeline-node-ai",
      "pipeline-node-gate",
      "pipeline-node-output-refined",
      "pipeline-node-output-raw",
      "prompt-preset-list",
      "ai-fallback-custom-prompt",
    ].forEach(expectStandardControl);

    [
      "toggle-hotkey",
      "ptt-use-vad-toggle",
      "ptt-hot-keepalive",
      "vad-block",
      "opus-archive-toggle",
      "continuous-mic-hard-cut",
      "model-source-select",
      "overlay-style",
      "diagnostic-logging-toggle",
      "postproc-punctuation",
      "ai-fallback-local-backend-select",
      "tab-btn-modules",
    ].forEach(expectExpertControl);
  });
});
