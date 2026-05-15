import { describe, expect, it } from "vitest";
import {
    normalizeAssistantSettings,
    normalizeEnabledModuleIds,
} from "../state";
import type { Settings } from "../types";

// These tests cover pure functions and intentionally do not exercise any
// Tauri APIs. The default Vitest setup (tauri.setup.ts) is still loaded,
// but it is a no-op for this file.

// Build a Settings fixture from a partial shape. We only set the fields
// that normalizeAssistantSettings actually reads or writes; the rest is
// intentionally absent and cast through `unknown`. This keeps tests
// readable without inlining the full Settings shape.
function makeSettings(overrides: Partial<Settings>): Settings {
    return overrides as unknown as Settings;
}

describe("normalizeEnabledModuleIds", () => {
    it("returns an empty array for undefined input", () => {
        expect(normalizeEnabledModuleIds(undefined)).toEqual([]);
    });

    it("returns an empty array for an empty input", () => {
        expect(normalizeEnabledModuleIds([])).toEqual([]);
    });

    it("migrates the legacy workflow_agent id to assistant_core", () => {
        expect(normalizeEnabledModuleIds(["workflow_agent"])).toEqual(["assistant_core"]);
    });

    it("passes assistant_core through unchanged", () => {
        expect(normalizeEnabledModuleIds(["assistant_core"])).toEqual(["assistant_core"]);
    });

    it("deduplicates after migration when both ids are present", () => {
        expect(normalizeEnabledModuleIds(["workflow_agent", "assistant_core"])).toEqual([
            "assistant_core",
        ]);
    });

    it("skips empty string ids", () => {
        expect(normalizeEnabledModuleIds(["", "assistant_core"])).toEqual(["assistant_core"]);
    });

    it("passes unknown module ids through unchanged", () => {
        expect(normalizeEnabledModuleIds(["custom_module"])).toEqual(["custom_module"]);
    });

    it("preserves order for a mix of legacy and unknown ids", () => {
        expect(normalizeEnabledModuleIds(["workflow_agent", "custom_module"])).toEqual([
            "assistant_core",
            "custom_module",
        ]);
    });
});

describe("normalizeAssistantSettings", () => {
    it("returns null when input is null", () => {
        expect(normalizeAssistantSettings(null)).toBeNull();
    });

    it("migrates enabled_modules workflow_agent to assistant_core", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: ["workflow_agent"],
                    consented_permissions: {},
                    module_overrides: {},
                },
            }),
        );
        expect(result?.module_settings?.enabled_modules).toEqual(["assistant_core"]);
    });

    it("remaps consented_permissions keys from workflow_agent to assistant_core", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: { workflow_agent: ["ptt"] },
                    module_overrides: {},
                },
            }),
        );
        expect(result?.module_settings?.consented_permissions).toEqual({
            assistant_core: ["ptt"],
        });
    });

    it("merges and deduplicates consented_permissions when both keys exist", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: {
                        workflow_agent: ["ptt", "audio"],
                        assistant_core: ["audio", "vision"],
                    },
                    module_overrides: {},
                },
            }),
        );
        const perms = result?.module_settings?.consented_permissions.assistant_core ?? [];
        expect(perms.sort()).toEqual(["audio", "ptt", "vision"]);
    });

    it("remaps module_overrides keys prefixed with workflow_agent.", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: {},
                    module_overrides: { "workflow_agent.some_key": 42 },
                },
            }),
        );
        expect(result?.module_settings?.module_overrides).toEqual({
            "assistant_core.some_key": 42,
        });
    });

    it("passes non-legacy consented_permissions keys through unchanged", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: { custom_module: ["read"] },
                    module_overrides: {},
                },
            }),
        );
        expect(result?.module_settings?.consented_permissions).toEqual({
            custom_module: ["read"],
        });
    });

    it("passes non-legacy module_overrides keys through unchanged", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: {},
                    module_overrides: { "other.key": "value" },
                },
            }),
        );
        expect(result?.module_settings?.module_overrides).toEqual({
            "other.key": "value",
        });
    });

    it("inserts the default workflow_agent block when missing", () => {
        const result = normalizeAssistantSettings(makeSettings({}));
        const agent = result?.workflow_agent;
        expect(agent).toBeDefined();
        expect(agent?.enabled).toBe(false);
        expect(agent?.wakewords).toEqual(["trispr", "hey trispr", "trispr agent"]);
        expect(agent?.activation_mode).toBe("hotkey_first");
        expect(agent?.trusted_action_allowlist).toEqual([]);
        expect(agent?.expert_yolo_enabled).toBe(false);
    });

    it("fills activation_mode default when omitted on existing workflow_agent", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                workflow_agent: {
                    enabled: true,
                    wakewords: [],
                    intent_keywords: {},
                    model: "x",
                    temperature: 0,
                    max_tokens: 0,
                    session_gap_minutes: 0,
                    max_candidates: 0,
                },
            }),
        );
        expect(result?.workflow_agent?.activation_mode).toBe("hotkey_first");
    });

    it("fills trusted_action_allowlist default when omitted", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                workflow_agent: {
                    enabled: true,
                    wakewords: [],
                    intent_keywords: {},
                    model: "x",
                    temperature: 0,
                    max_tokens: 0,
                    session_gap_minutes: 0,
                    max_candidates: 0,
                },
            }),
        );
        expect(result?.workflow_agent?.trusted_action_allowlist).toEqual([]);
    });

    it("fills expert_yolo_enabled default when omitted", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                workflow_agent: {
                    enabled: true,
                    wakewords: [],
                    intent_keywords: {},
                    model: "x",
                    temperature: 0,
                    max_tokens: 0,
                    session_gap_minutes: 0,
                    max_candidates: 0,
                },
            }),
        );
        expect(result?.workflow_agent?.expert_yolo_enabled).toBe(false);
    });

    it("defaults assistant_presence_enabled to true", () => {
        const result = normalizeAssistantSettings(makeSettings({}));
        expect(result?.assistant_presence_enabled).toBe(true);
    });

    it("defaults assistant_presence_pinned to true", () => {
        const result = normalizeAssistantSettings(makeSettings({}));
        expect(result?.assistant_presence_pinned).toBe(true);
    });

    it("resets product_mode from assistant to transcribe when assistant core is not available", () => {
        // Assistant core requires both: enabled_modules containing assistant_core
        // AND workflow_agent.enabled === true. Here neither holds.
        const result = normalizeAssistantSettings(
            makeSettings({
                product_mode: "assistant",
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: {},
                    module_overrides: {},
                },
            }),
        );
        expect(result?.product_mode).toBe("transcribe");
    });

    it("leaves product_mode transcribe unchanged when assistant core is not available", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                product_mode: "transcribe",
                module_settings: {
                    enabled_modules: [],
                    consented_permissions: {},
                    module_overrides: {},
                },
            }),
        );
        expect(result?.product_mode).toBe("transcribe");
    });

    it("preserves product_mode assistant when assistant core is fully enabled", () => {
        const result = normalizeAssistantSettings(
            makeSettings({
                product_mode: "assistant",
                module_settings: {
                    enabled_modules: ["assistant_core"],
                    consented_permissions: {},
                    module_overrides: {},
                },
                workflow_agent: {
                    enabled: true,
                    wakewords: [],
                    intent_keywords: {},
                    model: "x",
                    temperature: 0,
                    max_tokens: 0,
                    session_gap_minutes: 0,
                    max_candidates: 0,
                },
            }),
        );
        expect(result?.product_mode).toBe("assistant");
    });
});
