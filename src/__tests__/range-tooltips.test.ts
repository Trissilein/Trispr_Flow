import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { initRangeTooltips, RANGE_TOOLTIP_DEFINITIONS } from "../range-tooltips";
import indexHtml from "../../index.html?raw";

function tooltipText(inputId: string): string {
  const definition = RANGE_TOOLTIP_DEFINITIONS.find((entry) => entry.inputId === inputId);
  if (!definition) {
    throw new Error(`Missing tooltip definition for ${inputId}`);
  }
  return definition.text;
}

describe("Range Tooltips", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("injects tooltip trigger and bubble for a standard field range slider", () => {
    document.body.innerHTML = `
      <label class="field range">
        <span class="field-label">Voice Activation threshold</span>
        <div class="range-row">
          <input id="vad-threshold" type="range" />
        </div>
      </label>
    `;

    initRangeTooltips();

    const host = document.querySelector('.tooltip-host[data-tooltip-for="vad-threshold"]');
    expect(host).not.toBeNull();
    const trigger = host?.querySelector(".tooltip-trigger");
    const bubble = host?.querySelector(".tooltip-bubble");
    const input = document.getElementById("vad-threshold") as HTMLInputElement | null;

    expect(trigger).not.toBeNull();
    expect(bubble).not.toBeNull();
    expect(bubble?.textContent).toBe(tooltipText("vad-threshold"));
    expect(trigger?.getAttribute("aria-describedby")).toBe("tooltip-vad-threshold");
    expect(input?.title).toBe(tooltipText("vad-threshold"));
  });

  it("anchors conversation font tooltip to #conversation-font-controls .font-label", () => {
    document.body.innerHTML = `
      <div id="conversation-font-controls" class="font-controls">
        <span class="font-label">Font</span>
        <input id="conversation-font-size" type="range" />
      </div>
    `;

    initRangeTooltips();

    const fontLabel = document.querySelector("#conversation-font-controls .font-label");
    const host = fontLabel?.querySelector(
      '.tooltip-host[data-tooltip-for="conversation-font-size"]'
    );
    const input = document.getElementById("conversation-font-size") as HTMLInputElement | null;

    expect(host).not.toBeNull();
    expect(input?.title).toBe(tooltipText("conversation-font-size"));
  });

  it("is idempotent and does not inject duplicate tooltip hosts", () => {
    document.body.innerHTML = `
      <label class="field range">
        <span class="field-label">Silence grace</span>
        <div class="range-row">
          <input id="vad-silence" type="range" />
        </div>
      </label>
    `;

    initRangeTooltips();
    initRangeTooltips();

    const hosts = document.querySelectorAll('.tooltip-host[data-tooltip-for="vad-silence"]');
    expect(hosts).toHaveLength(1);
  });

  it("supports click toggle, outside click close, switch-over, and Escape close", () => {
    document.body.innerHTML = `
      <label class="field range">
        <span class="field-label">VAD threshold</span>
        <div class="range-row">
          <input id="vad-threshold" type="range" />
        </div>
      </label>
      <label class="field range">
        <span class="field-label">VAD silence</span>
        <div class="range-row">
          <input id="vad-silence" type="range" />
        </div>
      </label>
    `;

    initRangeTooltips();

    const hostA = document.querySelector(
      '.tooltip-host[data-tooltip-for="vad-threshold"]'
    ) as HTMLElement | null;
    const hostB = document.querySelector(
      '.tooltip-host[data-tooltip-for="vad-silence"]'
    ) as HTMLElement | null;
    const triggerA = hostA?.querySelector(".tooltip-trigger") as HTMLButtonElement | null;
    const triggerB = hostB?.querySelector(".tooltip-trigger") as HTMLButtonElement | null;

    triggerA?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(hostA?.classList.contains("tooltip-open")).toBe(true);
    expect(triggerA?.getAttribute("aria-expanded")).toBe("true");

    document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(hostA?.classList.contains("tooltip-open")).toBe(false);
    expect(triggerA?.getAttribute("aria-expanded")).toBe("false");

    triggerA?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(hostA?.classList.contains("tooltip-open")).toBe(true);
    triggerB?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(hostA?.classList.contains("tooltip-open")).toBe(false);
    expect(hostB?.classList.contains("tooltip-open")).toBe(true);

    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(hostB?.classList.contains("tooltip-open")).toBe(false);
    expect(triggerB?.getAttribute("aria-expanded")).toBe("false");
  });

  it("skips missing elements gracefully", () => {
    expect(() => initRangeTooltips()).not.toThrow();
  });

  it("covers every range input in index.html with a tooltip definition", () => {
    const rangeIdRegex = /id="([^"]+)"\s+type="range"/g;
    const rangeIds = new Set<string>();
    let match: RegExpExecArray | null = null;

    while ((match = rangeIdRegex.exec(indexHtml)) !== null) {
      rangeIds.add(match[1]);
    }

    const tooltipIds = new Set(RANGE_TOOLTIP_DEFINITIONS.map((entry) => entry.inputId));
    const missing = [...rangeIds].filter((id) => !tooltipIds.has(id));
    const extra = [...tooltipIds].filter((id) => !rangeIds.has(id));

    if (missing.length > 0) {
      throw new Error(`Missing tooltip definitions for sliders: ${missing.join(", ")}`);
    }
    if (extra.length > 0) {
      throw new Error(`Tooltip definitions without matching sliders: ${extra.join(", ")}`);
    }

    expect(rangeIds.size).toBeGreaterThan(0);
  });
});
