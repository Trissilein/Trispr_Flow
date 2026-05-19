// Shared helpers used across the R2 wire modules and the residual wireEvents().
//
// Per OQ-3 (refactoring-plan.md, 2026-05-15), snippets common to multiple wire
// modules live here so the import graph stays one-way:
//   event-listeners.ts → wiring/*.wire.ts → wiring/wire-helpers.ts
// Wire modules never import from event-listeners.ts.

import { renderSettings } from "../settings";
import { persistSettings } from "../settings-persist";
import { settings } from "../state";

let settingsRenderFrame: number | null = null;

export function scheduleSettingsRender(): void {
  if (settingsRenderFrame !== null) return;
  if (typeof window !== "undefined" && typeof window.requestAnimationFrame === "function") {
    settingsRenderFrame = window.requestAnimationFrame(() => {
      settingsRenderFrame = null;
      renderSettings();
    });
  } else {
    settingsRenderFrame = window.setTimeout(() => {
      settingsRenderFrame = null;
      renderSettings();
    }, 16) as unknown as number;
  }
}

/**
 * Registers a "change" event listener that just persists the current settings.
 *
 * Use for sliders / colour pickers / toggles whose value was already written
 * to the settings object by a companion "input" listener; this hook only
 * persists the final value once the user lets go of the control.
 */
export function onChangePersist(el: Element | null | undefined): void {
  el?.addEventListener("change", async () => {
    if (!settings) return;
    await persistSettings();
  });
}
