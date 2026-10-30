import { describe, expect, it } from "vitest";
import indexHtml from "../../index.html?raw";

const ALLOWED_COMPOUND_UNITS = new Set([
  "ptt-hotkey",
  "toggle.hotkey",
  "transcribe.hotkey",
  "toggle.activation.words.hotkey",
  "product.mode.hotkey",
  "tts.stop.hotkey",
]);

describe("settings visibility markup", () => {
  it("classifies every non-dialog control with an individual unit or an explicit exclusion", () => {
    const template = document.createElement("template");
    template.innerHTML = indexHtml;
    const root = template.content;
    const controls = Array.from(root.querySelectorAll<HTMLElement>("input, select, textarea, button"))
      .filter((element) => !element.closest("dialog, [role=dialog], [data-settings-visibility-editor-owned=true]"));
    const units = Array.from(root.querySelectorAll<HTMLElement>('[data-settings-visibility-managed="true"]'));
    const keys = units.map((unit) => unit.dataset.settingsVisibilityKey);

    expect(controls).not.toHaveLength(0);
    expect(keys.every(Boolean)).toBe(true);
    expect(new Set(keys).size).toBe(keys.length);
    expect(units.every((unit) => unit.dataset.settingsVisibilityDefault === "basic" || unit.dataset.settingsVisibilityDefault === "expert")).toBe(true);
    expect(units.every((unit) => unit.dataset.settingsVisibilityLabel?.trim() && unit.dataset.settingsVisibilityGroup?.trim())).toBe(true);

    for (const control of controls) {
      expect(control.closest('[data-settings-visibility-managed="true"], [data-settings-visibility-excluded]')).not.toBeNull();
    }

    for (const unit of units) {
      const childControls = unit.querySelectorAll("input, select, textarea, button");
      if (childControls.length > 1) {
        expect(ALLOWED_COMPOUND_UNITS.has(unit.dataset.settingsVisibilityKey ?? "")).toBe(true);
      }
    }
  });
});
