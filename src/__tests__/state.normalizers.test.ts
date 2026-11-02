import { describe, expect, it } from "vitest";
import {
    normalizeEnabledModuleIds,
    normalizeSettings,
} from "../state";
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

describe("normalizeEnabledModuleIds", () => {
    it("returns an empty array for undefined input", () => {
        expect(normalizeEnabledModuleIds(undefined)).toEqual([]);
    });

    it("returns an empty array for empty input", () => {
        expect(normalizeEnabledModuleIds([])).toEqual([]);
    });

    it("keeps known module ids unchanged", () => {
        expect(normalizeEnabledModuleIds(["ai_refinement"])).toEqual(["ai_refinement"]);
    });

    it("drops empty ids", () => {
        expect(normalizeEnabledModuleIds(["", "ai_refinement"])).toEqual(["ai_refinement"]);
    });

    it("deduplicates repeated ids", () => {
        expect(normalizeEnabledModuleIds(["ai_refinement", "ai_refinement"])).toEqual(["ai_refinement"]);
    });

    it("drops every removed module id", () => {
        expect(
            normalizeEnabledModuleIds([
                "analysis",
                "assistant_core",
                "assistant_presence",
                "gdd",
                "input_vision",
                "integrations_confluence",
                "output_video_generation",
                "output_voice_tts",
                "piper_tts",
                "task_capture",
                "workflow_agent",
                "ai_refinement",
            ]),
        ).toEqual(["ai_refinement"]);
    });

    it("passes unknown module ids through unchanged", () => {
        expect(normalizeEnabledModuleIds(["custom_module"])).toEqual(["custom_module"]);
    });
});

describe("normalizeSettings", () => {
    it("returns null when input is null", () => {
        expect(normalizeSettings(null)).toBeNull();
    });

    it("drops removed module ids, permissions and overrides while preserving unknown module data", () => {
        const result = normalizeSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: ["assistant_core", "gdd", "output_voice_tts", "custom_module"],
                    consented_permissions: {
                        assistant_core: ["filesystem_history"],
                        integrations_confluence: ["network_confluence"],
                        output_voice_tts: ["audio_output"],
                        output_video_generation: ["filesystem_exports"],
                        input_vision: ["screen_capture"],
                        custom_module: ["read"],
                    },
                    module_overrides: {
                        "assistant_core.migrated_legacy_binding": true,
                        "workflow_agent.some_key": 42,
                        "gdd.installed": true,
                        "analysis.last_error": "stale",
                        "output_voice_tts.last_error": "stale",
                        "piper_tts.installed": true,
                        "custom_module.option": true,
                    },
                },
            }),
        );
        expect(result?.module_settings).toEqual({
            enabled_modules: ["custom_module"],
            consented_permissions: { custom_module: ["read"] },
            module_overrides: { "custom_module.option": true },
        });
    });

    it("passes non-removed consented_permissions and module_overrides through unchanged", () => {
        const result = normalizeSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: { custom_module: ["read"] },
                    module_overrides: { "other.key": "value" },
                },
            }),
        );
        expect(result?.module_settings?.consented_permissions).toEqual({
            custom_module: ["read"],
        });
        expect(result?.module_settings?.module_overrides).toEqual({
            "other.key": "value",
        });
    });

    it("does not touch settings without module_settings", () => {
        const result = normalizeSettings(makeSettings({}));
        expect(result?.module_settings).toBeUndefined();
    });

    it("defaults diagnostic_logging_enabled to false and keeps an explicit true", () => {
        expect(normalizeSettings(makeSettings({}))?.diagnostic_logging_enabled).toBe(false);
        expect(
            normalizeSettings(makeSettings({ diagnostic_logging_enabled: true }))?.diagnostic_logging_enabled,
        ).toBe(true);
    });
});
