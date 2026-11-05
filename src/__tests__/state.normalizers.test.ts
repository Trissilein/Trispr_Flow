import { describe, expect, it } from "vitest";
import { normalizeSettings } from "../state";
import type { Settings } from "../types";

// These tests cover pure functions and intentionally do not exercise any
// Tauri APIs. The default Vitest setup (tauri.setup.ts) is still loaded,
// but it is a no-op for this file.

// Build a Settings fixture from a partial shape. We only set the fields
// that normalizeSettings actually reads or writes; the rest is
// intentionally absent and cast through `unknown`. This keeps tests
// readable without inlining the full Settings shape.
function makeSettings(overrides: Partial<Settings>): Settings {
    return overrides as unknown as Settings;
}

describe("normalizeSettings", () => {
    it("returns null when input is null", () => {
        expect(normalizeSettings(null)).toBeNull();
    });

    it("ignores a legacy module_settings block", () => {
        const legacy = makeSettings({ module_settings: { enabled_modules: ["ai_refinement"] } } as unknown as Partial<Settings>);
        expect(normalizeSettings(legacy)).not.toBeNull();
    });

    it("defaults diagnostic_logging_enabled to false and keeps an explicit true", () => {
        expect(normalizeSettings(makeSettings({}))?.diagnostic_logging_enabled).toBe(false);
        expect(
            normalizeSettings(makeSettings({ diagnostic_logging_enabled: true }))?.diagnostic_logging_enabled,
        ).toBe(true);
    });
});
