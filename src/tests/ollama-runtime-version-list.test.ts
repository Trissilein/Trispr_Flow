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

// The dropdowns for backend, runtime source and fallback endpoints are gone from the page: this
// fixture has none of them, so the render must work without those elements.
vi.hoisted(() => {
  document.body.innerHTML = `
    <input id="ai-fallback-enabled" type="checkbox" />
    <div id="ai-fallback-settings"></div>
    <div id="ai-fallback-loading-scrim" hidden></div>
    <div id="ai-fallback-loading-title"></div>
    <div id="ai-fallback-loading-detail"></div>
    <button id="ai-fallback-local-lane"></button>
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
    <div id="ai-fallback-runtime-version-list"></div>
    <div id="ai-fallback-local-lane-title-text"></div>
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

import { __resetForTesting, renderAIFallbackSettingsUi } from "../settings/ai-refinement.settings";
import {
  getOllamaRuntimeCardState,
  getOllamaRuntimeVersionCatalog,
  isOnlineVersionFetchInProgress,
} from "../ollama-models";
import { RUNTIME_VERSION_ACTION_EVENT } from "../ollama-runtime-versions";
import { setOverlayHealth, setSettings, setStartupStatus, settings } from "../state";
import type { OllamaRuntimeVersionInfo, Settings, StartupStatus } from "../types";

const runtimeCardMock = vi.mocked(getOllamaRuntimeCardState);
const runtimeCatalogMock = vi.mocked(getOllamaRuntimeVersionCatalog);
const onlineFetchMock = vi.mocked(isOnlineVersionFetchInProgress);

type RuntimeCardState = ReturnType<typeof getOllamaRuntimeCardState>;

function mkCard(overrides: Partial<RuntimeCardState> = {}): RuntimeCardState {
  return {
    healthy: true,
    busy: false,
    detected: true,
    endpoint: "http://127.0.0.1:11434",
    source: "per_user_zip",
    version: "0.40.13",
    detail: "Healthy",
    primaryAction: "ready",
    primaryLabel: "Runtime ready",
    primaryDisabled: true,
    busyAction: null,
    stage: "ready",
    managedAlive: true,
    managedPid: 1,
    backgroundStarting: false,
    compatibilityWarning: "",
    ...overrides,
  };
}

function mkSettings(
  target: string,
  recorded = "0.40.13",
  overrides: { path?: string; explicit?: boolean } = {}
): Settings {
  return {
    mode: "ptt",
    language_mode: "auto",
    language_pinned: false,
    postproc_enabled: true,
    postproc_language: "multi",
    topic_keywords: { bug: ["bug"], summary: ["summary"], actions: ["todo"] },
    providers: {
      ollama: {
        endpoint: "http://127.0.0.1:11434",
        available_models: ["llama3"],
        preferred_model: "llama3",
        runtime_source: "per_user_zip",
        runtime_path: overrides.path ?? "",
        runtime_version: recorded,
        runtime_target_version: target,
        runtime_target_explicit: overrides.explicit ?? false,
        last_health_check: null,
      },
    },
    ai_fallback: {
      enabled: true,
      provider: "ollama",
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
  } as unknown as Settings;
}

function entry(version: string, overrides: Partial<OllamaRuntimeVersionInfo> = {}): OllamaRuntimeVersionInfo {
  return {
    version,
    source: "online",
    selected: false,
    installed: false,
    recommended: false,
    prerelease: false,
    installable: true,
    installable_reason: null,
    ...overrides,
  };
}

const list = () => document.getElementById("ai-fallback-runtime-version-list") as HTMLElement;
const rows = () => Array.from(list().querySelectorAll<HTMLElement>("tr.ollama-zebra-row"));
const versions = () => rows().map((row) => row.dataset.zebraRow);
const rowOf = (version: string) => rows().find((row) => row.dataset.zebraRow === version);
const chipsOf = (version: string) =>
  Array.from(rowOf(version)?.querySelectorAll(".ollama-zebra-chip") ?? []).map((chip) => chip.textContent);
const buttonsOf = (version: string) =>
  Array.from(rowOf(version)?.querySelectorAll("button") ?? []).map((button) => button.textContent);
const cellsOf = (version: string) =>
  Array.from(rowOf(version)?.querySelectorAll("td") ?? []).map((cell) => cell.textContent);
const status = () => document.getElementById("ai-fallback-local-primary-status") as HTMLElement;

beforeEach(() => {
  __resetForTesting();
  setSettings(mkSettings("0.40.13"));
  setStartupStatus({ ollama_ready: true, ollama_starting: false } as StartupStatus);
  setOverlayHealth(null);
  runtimeCardMock.mockReturnValue(mkCard());
  onlineFetchMock.mockReturnValue(false);
  runtimeCatalogMock.mockReturnValue([
    entry("0.40.13", { selected: true, installed: true, recommended: true, latest: true, source: "pinned", released_at: "2026-10-28T10:00:00Z", size_bytes: 1_600_000_000 }),
    entry("0.40.7", { installed: true, source: "pinned", released_at: "2026-10-18T10:00:00Z", size_bytes: 1_500_000_000 }),
    entry("0.35.1", { released_at: "2026-09-02T10:00:00Z", size_bytes: 1_400_000_000 }),
    entry("0.30.4", { installed: true, installable: false }),
  ]);
});

describe("runtime version list", () => {
  it("shows the running version first, then newest first, with only the sensible buttons per row", () => {
    renderAIFallbackSettingsUi();

    expect(versions()).toEqual(["0.40.13", "0.40.7", "0.35.1", "0.30.4"]);
    // running: marker only, no button
    expect(buttonsOf("0.40.13")).toEqual([]);
    expect(chipsOf("0.40.13")).toEqual(["Recommended", "Latest", "Running"]);
    expect(rowOf("0.40.13")?.getAttribute("aria-current")).toBe("true");
    // installed, not running: Switch to + Delete
    expect(buttonsOf("0.40.7")).toEqual(["Switch to", "Delete"]);
    expect(chipsOf("0.40.7")).toEqual(["Installed"]);
    // not installed, verified: Install
    expect(buttonsOf("0.35.1")).toEqual(["Install"]);
    // installed outside the catalog: still switchable and deletable
    expect(buttonsOf("0.30.4")).toEqual(["Switch to", "Delete"]);
  });

  it("fills version, publish date and size and uses a dash for what is not known", () => {
    renderAIFallbackSettingsUi();
    expect(cellsOf("0.40.13").slice(0, 3)).toEqual(["0.40.13", "28 Oct 2026", "1.6 GB"]);
    expect(cellsOf("0.30.4").slice(0, 3)).toEqual(["0.30.4", "—", "—"]);
  });

  it("alternates the row tint", () => {
    renderAIFallbackSettingsUi();
    expect(rows().map((row) => (row.classList.contains("is-odd") ? "odd" : "even"))).toEqual(["odd", "even", "odd", "even"]);
  });

  it("the selected target is not deletable and says so with a chip; a stopped recorded version is not either", () => {
    runtimeCardMock.mockReturnValue(mkCard({ healthy: false, version: "0.40.13", detected: true, managedAlive: false }));
    setStartupStatus({ ollama_ready: false, ollama_starting: false } as StartupStatus);
    setSettings(mkSettings("0.40.7", "0.40.13", { explicit: true }));

    renderAIFallbackSettingsUi();

    // nothing runs: no "Running" marker, every installed row can be switched to
    expect(chipsOf("0.40.13")).not.toContain("Running");
    expect(buttonsOf("0.40.13")).toEqual(["Switch to"]); // recorded binary: no Delete
    expect(chipsOf("0.40.7")).toContain("Target");
    expect(buttonsOf("0.40.7")).toEqual(["Switch to"]); // target: no Delete
    expect(buttonsOf("0.30.4")).toEqual(["Switch to", "Delete"]);
  });

  it("lists a running version that the catalog does not know", () => {
    runtimeCardMock.mockReturnValue(mkCard({ version: "0.38.2" }));
    setSettings(mkSettings("0.38.2", "0.38.2"));

    renderAIFallbackSettingsUi();

    expect(versions()[0]).toBe("0.38.2");
    expect(buttonsOf("0.38.2")).toEqual([]);
    expect(chipsOf("0.38.2")).toContain("Running");
  });

  it("marks Prerelease and explains a version that is neither installed nor verified", () => {
    runtimeCatalogMock.mockReturnValue([
      entry("0.40.13", { installed: true, recommended: true, source: "pinned" }),
      entry("0.41.0-rc1", { prerelease: true }),
      entry("0.99.0", { installable: false, installable_reason: "Missing checksum asset." }),
    ]);

    renderAIFallbackSettingsUi();

    expect(chipsOf("0.41.0-rc1")).toContain("Prerelease");
    expect(buttonsOf("0.41.0-rc1")).toEqual(["Install"]);
    expect(buttonsOf("0.99.0")).toEqual([]);
    const marker = rowOf("0.99.0")?.querySelector<HTMLElement>(".ollama-zebra-chip");
    expect(marker?.textContent).toBe("Not installable");
    expect(marker?.title).toBe("Missing checksum asset.");
  });

  it("does not invent an 'unknown' runtime version while nothing is detected yet", () => {
    runtimeCardMock.mockReturnValue(mkCard({ version: "unknown", detected: false, healthy: false }));
    setSettings(mkSettings("0.40.13", ""));

    renderAIFallbackSettingsUi();

    expect(versions()).not.toContain("unknown");
    expect(versions()).toContain("0.40.13");
    expect(rows().some((row) => row.getAttribute("aria-current") === "true")).toBe(false);
  });

  it("keeps every button but makes it inert while a runtime operation runs", () => {
    runtimeCardMock.mockReturnValue(mkCard({ busy: true }));

    renderAIFallbackSettingsUi();

    const buttons = Array.from(list().querySelectorAll("button"));
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button.getAttribute("aria-disabled")).toBe("true");
      expect(button.title).toBe("Another runtime operation is running.");
    }
    expect(list().querySelector("table")?.getAttribute("aria-busy")).toBe("true");
  });

  it("a click on a row button reports the version and the action on the list element", () => {
    renderAIFallbackSettingsUi();
    const events: Array<{ version: string; action: string }> = [];
    list().addEventListener(RUNTIME_VERSION_ACTION_EVENT, (event) => {
      events.push((event as CustomEvent).detail);
    });

    rowOf("0.35.1")?.querySelector<HTMLButtonElement>("button")?.click();
    rowOf("0.40.7")?.querySelectorAll<HTMLButtonElement>("button")[1]?.click();

    return Promise.resolve().then(() => {
      expect(events).toEqual([
        { version: "0.35.1", action: "install" },
        { version: "0.40.7", action: "delete" },
      ]);
    });
  });

  it("an older target nobody picked is lifted to the running version while rendering", () => {
    setSettings(mkSettings("0.30.4", "0.40.13", { explicit: false }));
    renderAIFallbackSettingsUi();
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
    expect(chipsOf("0.30.4")).not.toContain("Target");
  });

  it("an older target picked in the list stays", () => {
    setSettings(mkSettings("0.30.4", "0.40.13", { explicit: true }));
    renderAIFallbackSettingsUi();
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.30.4");
    expect(chipsOf("0.30.4")).toContain("Target");
    expect(buttonsOf("0.30.4")).toEqual(["Switch to"]);
  });

  it("keeps keyboard focus on the same button across a re-render", () => {
    renderAIFallbackSettingsUi();
    const before = rowOf("0.35.1")?.querySelector<HTMLButtonElement>("button");
    before?.focus();
    expect(document.activeElement).toBe(before);

    renderAIFallbackSettingsUi();

    const active = document.activeElement as HTMLElement;
    expect(active).not.toBe(before);
    expect(active.dataset.zebraAction).toBe("install");
    expect(active.closest("tr")?.dataset.zebraRow).toBe("0.35.1");
  });
});

describe("the removed provider, source and endpoint controls", () => {
  it("renders without those elements and shows the plain status line for managed Ollama", () => {
    renderAIFallbackSettingsUi();
    expect(status().textContent).toContain("Runtime running");
    expect(status().textContent).not.toContain("supported setup");
    expect(status().hidden).toBe(false);
  });

  it("a stale LM Studio provider lands on Ollama", () => {
    const stale = mkSettings("0.40.13", "0.40.13");
    (stale.ai_fallback as { provider: string }).provider = "lm_studio";
    setSettings(stale);
    renderAIFallbackSettingsUi();
    expect(settings?.ai_fallback.provider).toBe("ollama");
    expect(status().textContent).toContain("Runtime running");
  });
});
