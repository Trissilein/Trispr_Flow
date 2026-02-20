import { afterEach, describe, expect, it } from "vitest";
import {
  initPanelState,
  isPanelCollapsed,
  setPanelCollapsed,
  togglePanel,
} from "../panels";

function renderPanelFixture(): void {
  document.body.innerHTML = `
    <section class="panel" data-panel="transcription" data-default-collapsed="false">
      <div class="panel-title"><h2>Transcription</h2></div>
      <button class="panel-collapse-btn" data-panel-collapse="transcription" aria-controls="transcription-panel-body"></button>
      <div class="panel-body" id="transcription-panel-body"></div>
    </section>
    <section class="panel" data-panel="capture" data-panel-group="input-source" data-default-collapsed="true">
      <div class="panel-title"><h2>Capture Input</h2></div>
      <button class="panel-collapse-btn" data-panel-collapse="capture" aria-controls="capture-panel-body"></button>
      <div class="panel-body" id="capture-panel-body"></div>
    </section>
    <section class="panel" data-panel="system" data-panel-group="input-source" data-default-collapsed="true">
      <div class="panel-title"><h2>System Audio Capture</h2></div>
      <button class="panel-collapse-btn" data-panel-collapse="system" aria-controls="system-panel-body"></button>
      <div class="panel-body" id="system-panel-body"></div>
    </section>
    <section class="panel" data-panel="postprocessing" data-default-collapsed="true">
      <div class="panel-title"><h2>Post-Processing</h2></div>
      <button class="panel-collapse-btn" data-panel-collapse="postprocessing" aria-controls="postprocessing-panel-body"></button>
      <div class="panel-body" id="postprocessing-panel-body"></div>
    </section>
    <section class="panel" data-panel="model" data-default-collapsed="true">
      <div class="panel-title"><h2>Model Manager</h2></div>
      <button class="panel-collapse-btn" data-panel-collapse="model" aria-controls="model-panel-body"></button>
      <div class="panel-body" id="model-panel-body"></div>
    </section>
    <section class="panel" data-panel="interface" data-default-collapsed="true">
      <div class="panel-title"><h2>UX / UI-Adjustments</h2></div>
      <button class="panel-collapse-btn" data-panel-collapse="interface" aria-controls="interface-panel-body"></button>
      <div class="panel-body" id="interface-panel-body"></div>
    </section>
  `;
}

describe("panels", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("applies deterministic startup collapse states", () => {
    renderPanelFixture();
    initPanelState();

    expect(isPanelCollapsed("transcription")).toBe(false);
    expect(isPanelCollapsed("capture")).toBe(true);
    expect(isPanelCollapsed("system")).toBe(true);
    expect(isPanelCollapsed("postprocessing")).toBe(true);
    expect(isPanelCollapsed("model")).toBe(true);
    expect(isPanelCollapsed("interface")).toBe(true);
  });

  it("keeps capture and system collapse state in sync", () => {
    renderPanelFixture();
    initPanelState();

    setPanelCollapsed("capture", false);
    expect(isPanelCollapsed("capture")).toBe(false);
    expect(isPanelCollapsed("system")).toBe(false);

    togglePanel("system");
    expect(isPanelCollapsed("capture")).toBe(true);
    expect(isPanelCollapsed("system")).toBe(true);
  });

  it("updates aria metadata on collapse actions", () => {
    renderPanelFixture();
    initPanelState();

    togglePanel("transcription");
    const button = document.querySelector<HTMLButtonElement>(
      '[data-panel-collapse="transcription"]',
    );
    expect(button).not.toBeNull();
    expect(button?.getAttribute("aria-expanded")).toBe("false");
    expect(button?.getAttribute("title")).toBe("Expand");
    expect(button?.getAttribute("aria-label")).toBe("Expand Transcription panel");
  });

  it("supports model panel expansion based on class collapse state", () => {
    renderPanelFixture();
    initPanelState();

    expect(isPanelCollapsed("model")).toBe(true);
    setPanelCollapsed("model", false);
    expect(isPanelCollapsed("model")).toBe(false);
  });
});
