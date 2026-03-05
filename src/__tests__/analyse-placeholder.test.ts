import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

describe("analyse placeholder", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
    vi.resetModules();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("routes to modules tab when analyse button is clicked", async () => {
    localStorage.removeItem("trispr-active-tab");
    document.body.innerHTML = `
      <button id="analyse-button">Analyse</button>
      <button id="tab-btn-transcription"></button>
      <button id="tab-btn-settings"></button>
      <button id="tab-btn-ai-refinement"></button>
      <button id="tab-btn-modules"></button>
      <div id="tab-transcription"></div>
      <div id="tab-settings"></div>
      <div id="tab-ai-refinement"></div>
      <div id="tab-modules"></div>
    `;
    const { wireEvents } = await import("../event-listeners");

    wireEvents();
    document.getElementById("analyse-button")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(localStorage.getItem("trispr-active-tab")).toBe("modules");
  });
});
