import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const PROFILE_KEY = "trispr-settings-visibility-v1";

function managed(
  key: string,
  defaultVisibility: "basic" | "expert",
  group = "Capture",
): string {
  return `data-settings-visibility-managed="true" data-settings-visibility-key="${key}" data-settings-visibility-label="${key}" data-settings-visibility-default="${defaultVisibility}" data-settings-visibility-group="${group}"`;
}

async function resolver() {
  vi.resetModules();
  return import("../settings-visibility");
}

beforeEach(() => {
  localStorage.clear();
  document.documentElement.className = "standard-mode";
});

afterEach(() => {
  document.body.innerHTML = "";
  document.documentElement.className = "";
  localStorage.clear();
});

describe("settings visibility resolver", () => {
  it("uses defaults, keeps editor-owned controls out, and releases a Basic ancestor", async () => {
    document.body.innerHTML = `
      <section id="capture-group" data-expert-only="true" data-settings-visibility-group-container>
        <label id="daily" ${managed("capture.daily", "basic")}></label>
        <label id="tuning" ${managed("capture.tuning", "expert")}></label>
      </section>
      <div data-settings-visibility-editor-owned="true">
        <button ${managed("editor.fake", "basic")}></button>
      </div>`;
    const { applySettingsVisibility, getSettingsVisibilityEntries } = await resolver();
    applySettingsVisibility();

    expect(getSettingsVisibilityEntries().map((entry) => entry.key)).toEqual([
      "capture.daily",
      "capture.tuning",
    ]);
    expect(document.getElementById("capture-group")?.hasAttribute("data-expert-only")).toBe(false);
    expect(document.getElementById("daily")?.dataset.settingsVisibilityHidden).toBe("false");
    expect(document.getElementById("tuning")?.dataset.settingsVisibilityHidden).toBe("true");
  });

  it("does not revive a runtime-hidden module and hides its empty Basic tab", async () => {
    document.body.innerHTML = `
      <button data-settings-visibility-tab="voice-output"></button>
      <section data-settings-visibility-tab-content="voice-output" hidden>
        <label ${managed("voice.provider", "basic", "Voice Output")}></label>
      </section>`;
    const { applySettingsVisibility } = await resolver();
    applySettingsVisibility();

    const tab = document.querySelector<HTMLElement>("[data-settings-visibility-tab]");
    expect(document.querySelector<HTMLElement>("[data-settings-visibility-tab-content]")?.hidden).toBe(true);
    expect(tab?.dataset.settingsVisibilityNoBasic).toBe("true");
  });

  it("retains an old dynamic override and applies it when the dynamic unit renders", async () => {
    localStorage.setItem(
      PROFILE_KEY,
      JSON.stringify({
        version: 1,
        overrides: { "task.route.work": "expert" },
        removalCandidates: {},
      }),
    );
    document.body.innerHTML = `<label ${managed("capture.daily", "basic")}></label>`;
    const { applySettingsVisibility } = await resolver();
    applySettingsVisibility();

    const dynamic = document.createElement("label");
    dynamic.innerHTML = "Route";
    dynamic.setAttribute("data-settings-visibility-managed", "true");
    dynamic.dataset.settingsVisibilityKey = "task.route.work";
    dynamic.dataset.settingsVisibilityLabel = "Work route";
    dynamic.dataset.settingsVisibilityDefault = "basic";
    dynamic.dataset.settingsVisibilityGroup = "Task Capture";
    document.body.append(dynamic);
    applySettingsVisibility();

    expect(dynamic.dataset.settingsVisibilityHidden).toBe("true");
  });
});
