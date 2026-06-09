import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../frontend-trace", () => ({ traceFrontendWarn: vi.fn() }));
vi.mock("../refinement-pipeline-graph", () => ({
    syncRefinementPipelineGraphFromSettings: vi.fn(),
}));
vi.mock("../ai-refinement-help", () => ({ renderAIRefinementStaticHelp: vi.fn() }));
vi.mock("../settings-persist", () => ({
    ensureSetupDefaults: vi.fn(),
    persistSettings: vi.fn(async () => undefined),
    syncDerivedLanguageSettings: vi.fn(),
}));
vi.mock("../ollama-models", () => ({
    getOllamaRuntimeCardState: vi.fn(),
    getOllamaRuntimeVersionCatalog: vi.fn(),
    isOnlineVersionFetchInProgress: vi.fn(),
}));

vi.hoisted(() => {
    document.body.innerHTML = `
    <input id="ai-fallback-enabled" type="checkbox" />
    <div id="ai-fallback-settings"></div>
    <div id="ai-fallback-loading-scrim" hidden></div>
    <div id="ai-fallback-loading-title"></div>
    <div id="ai-fallback-loading-detail"></div>
    <div id="ai-fallback-cloud-provider-list"></div>
    <div id="ai-fallback-fallback-status"></div>
    <button id="ai-fallback-local-lane"></button>
    <button id="ai-fallback-online-lane"></button>
    <div id="ai-fallback-online-status-badge"></div>
    <div id="ai-fallback-local-primary-status"></div>
    <div id="ai-fallback-local-runtime-note"></div>
    <div id="ai-fallback-runtime-progress" hidden>
      <div id="ai-fallback-runtime-progress-fill"></div>
      <div id="ai-fallback-runtime-progress-text"></div>
    </div>
    <button id="ai-fallback-local-primary-action"></button>
    <button id="ai-fallback-local-import-action"></button>
    <button id="ai-fallback-local-verify-action"></button>
    <button id="ai-fallback-local-refresh-action"></button>
    <button id="ai-fallback-fetch-versions-action"></button>
    <select id="ai-fallback-local-runtime-source"><option value="per_user_zip">per_user_zip</option></select>
    <select id="ai-fallback-local-runtime-version"></select>
    <div id="ai-fallback-local-runtime-version-note"></div>
    <select id="ai-fallback-local-backend-select"><option value="ollama">ollama</option><option value="lm_studio">lm_studio</option><option value="oobabooga">oobabooga</option></select>
    <div id="ai-fallback-local-lane-title-text"></div>
    <div id="ai-fallback-local-advanced"></div>
    <div id="ai-fallback-compat-config"></div>
    <div id="ai-fallback-ollama-managed-note"></div>
    <button id="ai-fallback-lm-studio-install-action"></button>
    <textarea id="ai-fallback-local-fallback-endpoints"></textarea>
    <div id="ai-fallback-compat-guide"></div>
    <input id="ai-fallback-compat-endpoint" />
    <div id="ai-fallback-compat-endpoint-hint"></div>
    <input id="ai-fallback-compat-api-key" />
    <div id="ai-fallback-compat-model-list"></div>
    <div id="ai-fallback-provider-lanes"></div>
    <div id="ai-fallback-model-field"></div>
    <select id="ai-fallback-model"></select>
    <input id="ai-fallback-temperature" type="range" />
    <span id="ai-fallback-temperature-value"></span>
    <input id="ai-fallback-preserve-language" type="checkbox" />
    <span id="ai-fallback-preserve-language-note"></span>
    <input id="ai-fallback-low-latency-mode" type="checkbox" />
    <span id="ai-fallback-low-latency-note"></span>
    <input id="ai-fallback-max-tokens" />
    <div id="prompt-preset-list"></div>
    <div id="ai-fallback-prompt-preview-label"></div>
    <div id="ai-fallback-prompt-preview-hint"></div>
    <textarea id="ai-fallback-custom-prompt"></textarea>
    <div id="ai-fallback-preset-name-field"></div>
    <div id="ai-fallback-preset-name-input-wrap"></div>
    <input id="ai-fallback-prompt-preset-name" />
    <button id="ai-fallback-prompt-preset-save"></button>
    <button id="ai-fallback-prompt-preset-reset"></button>
    <button id="ai-fallback-prompt-preset-revert"></button>
    <button id="ai-fallback-prompt-preset-discard"></button>
    <button id="ai-fallback-prompt-preset-delete"></button>
    <div id="refinement-pipeline-note"></div>
    <div id="overlay-health-note"></div>
    <details id="ai-refinement-runtime-expander"></details>
    <details id="ai-refinement-models-expander"></details>
    <details id="ai-refinement-topic-expander"></details>
    <div id="topic-keywords-list"></div>
  `;
});

import {
    __resetForTesting,
    renderAIFallbackSettingsUi,
    renderAIRefinementTab,
    renderOverlayHealthNote,
    renderTopicKeywords,
} from "../settings/ai-refinement.settings";
import { renderAIRefinementStaticHelp } from "../ai-refinement-help";
import { NEW_REFINEMENT_PROMPT_OPTION_ID } from "../refinement-prompts";
import { persistSettings } from "../settings-persist";
import {
    getOllamaRuntimeCardState,
    getOllamaRuntimeVersionCatalog,
    isOnlineVersionFetchInProgress,
} from "../ollama-models";
import { setOverlayHealth, setSettings, setStartupStatus, settings } from "../state";
import type { Settings, StartupStatus } from "../types";

const runtimeCardMock = vi.mocked(getOllamaRuntimeCardState);
const runtimeCatalogMock = vi.mocked(getOllamaRuntimeVersionCatalog);
const onlineFetchMock = vi.mocked(isOnlineVersionFetchInProgress);
const renderHelpMock = vi.mocked(renderAIRefinementStaticHelp);
const persistSettingsMock = vi.mocked(persistSettings);

function byId<T extends HTMLElement>(id: string): T {
    return document.getElementById(id) as T;
}

type RuntimeCardState = ReturnType<typeof getOllamaRuntimeCardState>;

function mkCard(overrides: Partial<RuntimeCardState> = {}): RuntimeCardState {
    return {
        healthy: true,
        busy: false,
        detected: true,
        endpoint: "http://127.0.0.1:11434",
        source: "managed",
        version: "0.20.2",
        detail: "Healthy",
        primaryAction: "start",
        primaryLabel: "Start",
        primaryDisabled: false,
        busyAction: null,
        stage: "ready",
        managedAlive: false,
        managedPid: null,
        backgroundStarting: false,
        compatibilityWarning: "",
        ...overrides,
    };
}

function mkSettings(overrides: Partial<Settings> = {}): Settings {
    return {
        mode: "ptt",
        product_mode: "assistant",
        language_mode: "auto",
        language_pinned: false,
        postproc_enabled: true,
        postproc_language: "multi",
        postproc_llm_provider: "ollama",
        topic_keywords: { bug: ["bug"], summary: ["summary"], actions: ["todo"] },
        workflow_agent: { online_enabled: false } as Settings["workflow_agent"],
        providers: {
            claude: { api_key_stored: false, auth_method_preference: "api_key", auth_status: "locked", auth_verified_at: null, available_models: ["c"], preferred_model: "c" },
            openai: { api_key_stored: false, auth_method_preference: "api_key", auth_status: "locked", auth_verified_at: null, available_models: ["o"], preferred_model: "o" },
            gemini: { api_key_stored: false, auth_method_preference: "api_key", auth_status: "locked", auth_verified_at: null, available_models: ["g"], preferred_model: "g" },
            ollama: { endpoint: "http://127.0.0.1:11434", available_models: ["llama3"], preferred_model: "llama3", runtime_source: "per_user_zip", runtime_path: "", runtime_version: "0.20.2", runtime_target_version: "0.20.2", fallback_endpoints: ["http://127.0.0.1:11434"], last_health_check: null },
            lm_studio: { endpoint: "http://127.0.0.1:1234", api_key: "", preferred_model: "a-model", available_models: ["a-model", "deepseek-r1"] },
            oobabooga: { endpoint: "http://127.0.0.1:5000", api_key: "", preferred_model: "textgen", available_models: ["textgen"] },
        },
        ai_fallback: {
            enabled: true,
            provider: "ollama",
            fallback_provider: "openai",
            execution_mode: "local_primary",
            strict_local_mode: true,
            preserve_source_language: true,
            low_latency_mode: false,
            model: "llama3",
            temperature: 0.3,
            max_tokens: 4000,
            prompt_profile: "balanced",
            custom_prompt_enabled: false,
            custom_prompt: "",
            use_default_prompt: true,
            prompt_presets: [],
            prompt_preset_overrides: {},
            active_prompt_preset_id: "balanced",
        },
        ...overrides,
    } as unknown as Settings;
}

beforeEach(() => {
    __resetForTesting();
    window.localStorage.clear();
    delete (window as unknown as { runtimeInstallProgress?: unknown }).runtimeInstallProgress;
    setSettings(mkSettings());
    setStartupStatus({ ollama_ready: true, ollama_starting: false } as StartupStatus);
    setOverlayHealth(null);
    runtimeCardMock.mockReturnValue(mkCard());
    runtimeCatalogMock.mockReturnValue([
        { version: "0.20.2", source: "online", selected: true, installed: true, recommended: true, prerelease: false, installable: true, installable_reason: "" },
    ]);
    onlineFetchMock.mockReturnValue(false);
    renderHelpMock.mockClear();
    persistSettingsMock.mockClear();
});

describe("ai-refinement settings", () => {
    it("no-op on null settings", () => { setSettings(null); expect(() => renderAIFallbackSettingsUi()).not.toThrow(); });
    it("renders 3 cloud rows", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-cloud-provider-list").querySelectorAll(".cloud-provider-row").length).toBe(3); });
    it("roadmap badge text", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-online-status-badge").textContent).toContain("Roadmap"); });
    it("local lane active", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-local-lane").classList.contains("is-active")).toBe(true); });
    it("execution mode pinned", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, execution_mode: "online_fallback" as never } })); renderAIFallbackSettingsUi(); expect(settings?.ai_fallback.execution_mode).toBe("local_primary"); });
    it("cloud provider migration", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: "openai" as never, fallback_provider: null } })); renderAIFallbackSettingsUi(); expect(settings?.ai_fallback.provider).toBe("ollama"); expect(settings?.ai_fallback.fallback_provider).toBe("openai"); });

    it.each([
        ["ollama", false, true],
        ["lm_studio", true, false],
        ["oobabooga", true, false],
    ])("provider visuals %s", (provider, compatVisible, primaryVisible) => {
        setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: provider as never } }));
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("ai-fallback-compat-config").hidden).toBe(!compatVisible);
        expect(byId<HTMLButtonElement>("ai-fallback-local-primary-action").hidden).toBe(!primaryVisible);
    });

    it.each([
        ["ollama", "Ollama"],
        ["lm_studio", "LM Studio"],
        ["oobabooga", "Oobabooga"],
    ])("provider note copy %s", (provider, expected) => {
        setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: provider as never } }));
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("ai-fallback-ollama-managed-note").textContent).toContain(expected);
    });

    it.each([
        [true, false, false, "running"],
        [false, true, false, "detected"],
        [false, false, true, "detected"],
        [false, false, false, "detected"],
    ])("runtime status cases %#", (healthy, busy, starting, expected) => {
        runtimeCardMock.mockReturnValue(mkCard({ healthy, busy, backgroundStarting: starting }));
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("ai-fallback-local-primary-status").textContent).toContain(expected);
    });

    it.each([
        [false, true, false, true, "Running in background"],
        [false, false, true, true, "Starting in background"],
        [false, false, false, true, "fallback active"],
        [false, false, false, false, "Available later"],
        [true, false, false, true, "Healthy"],
    ])("runtime note cases %#", (healthy, busy, starting, postproc, expected) => {
        setSettings(mkSettings({ postproc_enabled: postproc }));
        runtimeCardMock.mockReturnValue(mkCard({ healthy, busy, backgroundStarting: starting, detail: "Healthy" }));
        setStartupStatus({ ollama_ready: healthy, ollama_starting: starting } as StartupStatus);
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("ai-fallback-local-runtime-note").textContent).toContain(expected);
    });

    it("scrim shown when starting", () => { runtimeCardMock.mockReturnValue(mkCard({ healthy: false, backgroundStarting: true })); setStartupStatus({ ollama_ready: false, ollama_starting: true } as StartupStatus); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-loading-scrim").hidden).toBe(false); });
    it("scrim hidden when healthy", () => { runtimeCardMock.mockReturnValue(mkCard({ healthy: true })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-loading-scrim").hidden).toBe(true); });
    it("busy aria when runtime busy", () => { runtimeCardMock.mockReturnValue(mkCard({ busy: true })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-settings").getAttribute("aria-busy")).toBe("true"); });
    it("verify disabled when not detected", () => { runtimeCardMock.mockReturnValue(mkCard({ detected: false })); renderAIFallbackSettingsUi(); expect(byId<HTMLButtonElement>("ai-fallback-local-verify-action").disabled).toBe(true); });
    it("versions button disabled while online fetch", () => { onlineFetchMock.mockReturnValue(true); renderAIFallbackSettingsUi(); expect(byId<HTMLButtonElement>("ai-fallback-fetch-versions-action").disabled).toBe(true); });

    it("progress hidden without progress", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-runtime-progress").hasAttribute("hidden")).toBe(true); });
    it("progress shown with object", () => { (window as any).runtimeInstallProgress = { message: "Downloading", downloaded: 5, total: 10 }; renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-runtime-progress").hasAttribute("hidden")).toBe(false); });
    it("progress width computed", () => { (window as any).runtimeInstallProgress = { message: "Downloading", downloaded: 5, total: 10 }; renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-runtime-progress-fill").style.width).toBe("50%"); });
    it("progress mb text", () => { (window as any).runtimeInstallProgress = { message: "Downloading", downloaded: 1048576, total: 2097152 }; renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-runtime-progress-text").textContent).toContain("(1/2 MB)"); });

    it.each([
        [true, true, true, false, "AI refinement"],
        [true, true, false, true, "starts in background"],
        [true, true, false, false, "is unavailable"],
        [true, false, true, false, "AI refinement only"],
        [false, true, true, false, "Rule-based refiner only"],
        [false, false, true, false, "No refinement active"],
    ])("pipeline note cases %#", (aiEnabled, rules, healthy, starting, expected) => {
        setSettings(mkSettings({ postproc_enabled: rules, ai_fallback: { ...settings!.ai_fallback, enabled: aiEnabled, provider: "ollama" } }));
        runtimeCardMock.mockReturnValue(mkCard({ healthy, backgroundStarting: starting }));
        setStartupStatus({ ollama_ready: healthy, ollama_starting: starting } as StartupStatus);
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("refinement-pipeline-note").textContent).toContain(expected);
    });

    it("pipeline warning when rules disabled", () => { setSettings(mkSettings({ postproc_enabled: false })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("refinement-pipeline-note").classList.contains("is-warning")).toBe(true); });

    it.each([[true, "Low latency active"], [false, "Standard latency"]])("latency notes %#", (enabled, expected) => {
        setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, low_latency_mode: enabled as boolean } }));
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("ai-fallback-low-latency-note").textContent).toContain(expected as string);
    });

    it.each([[true, "Language lock is active"], [false, "Language lock is off"]])("preserve language notes %#", (enabled, expected) => {
        setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, preserve_source_language: enabled as boolean } }));
        renderAIFallbackSettingsUi();
        expect(byId<HTMLElement>("ai-fallback-preserve-language-note").textContent).toContain(expected as string);
    });

    it("renders preset chips", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("prompt-preset-list").querySelectorAll(".preset-chip").length).toBeGreaterThan(0); });
    it("renders + New chip", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("prompt-preset-list").textContent).toContain("+ New"); });
    it("prompt label built-in", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-prompt-preview-label").textContent).toContain("Prompt preview"); });
    it("prompt label user", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, prompt_presets: [{ id: "u1", name: "User", prompt: "X" }], active_prompt_preset_id: "user:u1" } })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-prompt-preview-label").textContent).toContain("User preset"); });
    it("prompt label new", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, active_prompt_preset_id: NEW_REFINEMENT_PROMPT_OPTION_ID } })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-prompt-preview-label").textContent).toContain("New preset prompt"); });
    it("delete visible for user preset", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, prompt_presets: [{ id: "u1", name: "User", prompt: "X" }], active_prompt_preset_id: "user:u1" } })); renderAIFallbackSettingsUi(); expect(byId<HTMLButtonElement>("ai-fallback-prompt-preset-delete").hidden).toBe(false); });
    it("delete hidden for built-in", () => { renderAIFallbackSettingsUi(); expect(byId<HTMLButtonElement>("ai-fallback-prompt-preset-delete").hidden).toBe(true); });
    it("dirty prompt shows discard", () => { renderAIFallbackSettingsUi(); const t = byId<HTMLTextAreaElement>("ai-fallback-custom-prompt"); t.focus(); t.value = "edited"; renderAIFallbackSettingsUi(); expect(byId<HTMLButtonElement>("ai-fallback-prompt-preset-discard").hidden).toBe(false); });

    it("compat model list renders text", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: "lm_studio" } })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-compat-model-list").textContent).toContain("a-model"); });
    it("compat model XSS not injected", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: "lm_studio" }, providers: { ...settings!.providers, lm_studio: { ...settings!.providers.lm_studio!, endpoint: settings!.providers.lm_studio!.endpoint, api_key: settings!.providers.lm_studio!.api_key, available_models: ["<img src=x onerror=alert(1)>"], preferred_model: "<img src=x onerror=alert(1)>" } } })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-compat-model-list").querySelector("img")).toBeNull(); });
    it("reasoning warning shown", () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: "lm_studio" }, providers: { ...settings!.providers, lm_studio: { ...settings!.providers.lm_studio!, endpoint: settings!.providers.lm_studio!.endpoint, api_key: settings!.providers.lm_studio!.api_key, available_models: ["deepseek-r1"], preferred_model: "deepseek-r1" } } })); renderAIFallbackSettingsUi(); expect(byId<HTMLElement>("ai-fallback-compat-model-list").textContent).toContain("Reasoning model"); });
    it("activate model persists", async () => { setSettings(mkSettings({ ai_fallback: { ...settings!.ai_fallback, provider: "lm_studio" }, providers: { ...settings!.providers, lm_studio: { ...settings!.providers.lm_studio!, endpoint: settings!.providers.lm_studio!.endpoint, api_key: settings!.providers.lm_studio!.api_key, available_models: ["a", "b"], preferred_model: "a" } } })); renderAIFallbackSettingsUi(); const btn = byId<HTMLElement>("ai-fallback-compat-model-list").querySelector("button") as HTMLButtonElement; btn.click(); await Promise.resolve(); expect(settings!.providers.lm_studio!.preferred_model).toBe("b"); expect(persistSettingsMock).toHaveBeenCalled(); });

    it("expander defaults true", () => { renderAIRefinementTab(); expect(byId<HTMLDetailsElement>("ai-refinement-runtime-expander").open).toBe(true); });
    it("expander writes to localStorage", () => { renderAIRefinementTab(); const d = byId<HTMLDetailsElement>("ai-refinement-runtime-expander"); d.open = false; d.dispatchEvent(new Event("toggle")); expect(window.localStorage.getItem("ai_refinement_expanders_v1") || "").toContain("ai-refinement-runtime-expander"); });
    it("expander reads localStorage", () => { window.localStorage.setItem("ai_refinement_expanders_v1", JSON.stringify({ "ai-refinement-runtime-expander": false })); __resetForTesting(); renderAIRefinementTab(); expect(byId<HTMLDetailsElement>("ai-refinement-runtime-expander").open).toBe(false); });
    it("expander reset helper clears cache", () => { window.localStorage.setItem("ai_refinement_expanders_v1", JSON.stringify({ "ai-refinement-runtime-expander": false })); renderAIRefinementTab(); window.localStorage.removeItem("ai_refinement_expanders_v1"); __resetForTesting(); renderAIRefinementTab(); expect(byId<HTMLDetailsElement>("ai-refinement-runtime-expander").open).toBe(true); });

    it("renderTopicKeywords rows", async () => { await renderTopicKeywords(); expect(byId<HTMLElement>("topic-keywords-list").querySelectorAll("input").length).toBeGreaterThan(0); });
    it("renderTopicKeywords normalizes null", async () => { setSettings(mkSettings({ topic_keywords: null as never })); await renderTopicKeywords(); expect(byId<HTMLElement>("topic-keywords-list").querySelectorAll("input").length).toBeGreaterThan(0); });
    it("renderTopicKeywords persists changes", async () => { await renderTopicKeywords(); const i = byId<HTMLElement>("topic-keywords-list").querySelector("input") as HTMLInputElement; i.value = "x,y"; i.dispatchEvent(new Event("change")); await Promise.resolve(); expect(persistSettingsMock).toHaveBeenCalled(); });

    it.each([
        [null, true, ""],
        [{ status: "failed", attempt: 1, reason: "oops" }, false, "degraded"],
        [{ status: "recovered", attempt: 1, reason: "ok" }, false, "recovered"],
        [{ status: "recovering", attempt: 2, reason: "retry" }, false, "recovering"],
    ])("overlay health variants %#", (health, hidden, txt) => {
        setOverlayHealth(health as any);
        renderOverlayHealthNote();
        expect(byId<HTMLElement>("overlay-health-note").hidden).toBe(hidden as boolean);
        if (txt) expect((byId<HTMLElement>("overlay-health-note").textContent || "").toLowerCase()).toContain(txt as string);
    });

    it("renderAIRefinementTab composes fallback+keywords+help", () => { renderAIRefinementTab(); expect(byId<HTMLElement>("prompt-preset-list").querySelectorAll(".preset-chip").length).toBeGreaterThan(0); expect(byId<HTMLElement>("topic-keywords-list").querySelectorAll("input").length).toBeGreaterThan(0); expect(renderHelpMock).toHaveBeenCalledTimes(1); });
    it("renderAIRefinementTab safe on null settings", () => { setSettings(null); expect(() => renderAIRefinementTab()).not.toThrow(); });
});
