/**
 * The old default runtime target (0.20.2) was never a user choice. Whatever is stored, installed or
 * on disk, it must read as the recommended runtime: "Install 0.40.13" (or "Switch to 0.40.13" when
 * that folder exists) and never an offer to go back to 0.20.2. Also: what the settings page shows
 * after a re-render, and that it shows server text as text. A target that is older than the running
 * runtime and was not picked in the version list is lifted to the running one; a pick stays.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../frontend-trace", () => ({
  traceFrontendWarn: vi.fn(),
  traceFrontendInfo: vi.fn(),
  traceFrontendError: vi.fn(),
}));
vi.mock("../refinement-pipeline-graph", () => ({
  syncRefinementPipelineGraphFromSettings: vi.fn(),
}));
vi.mock("../ai-refinement-help", () => ({ renderAIRefinementStaticHelp: vi.fn() }));
vi.mock("../settings-persist", () => ({
  ensureSetupDefaults: vi.fn(),
  persistSettings: vi.fn(async () => undefined),
  syncDerivedLanguageSettings: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

vi.hoisted(() => {
  document.body.innerHTML = `
    <div id="toast-container"></div>
    <div id="ollama-model-manager"></div>
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
    <div id="ai-fallback-local-advanced"></div>
    <div id="ai-fallback-compat-config"></div>
    <div id="ai-fallback-ollama-managed-note"></div>
    <button id="ai-fallback-lm-studio-install-action"></button>
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

import { invoke } from "@tauri-apps/api/core";
import { __resetForTesting, renderAIFallbackSettingsUi } from "../settings/ai-refinement.settings";
import * as models from "../ollama-models";
import { setOverlayHealth, setSettings, setStartupStatus, settings } from "../state";
import type { OllamaRuntimeVersionInfo, Settings, StartupStatus } from "../types";

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

function mkSettings(installed: string, target: string, explicit = false): Settings {
  return {
    mode: "ptt",
    product_mode: "assistant",
    language_mode: "auto",
    language_pinned: false,
    postproc_enabled: true,
    postproc_language: "multi",
    postproc_llm_provider: "ollama",
    topic_keywords: { bug: ["bug"], summary: ["summary"], actions: ["todo"] },
    workflow_agent: { online_enabled: false },
    providers: {
      claude: { api_key_stored: false, auth_method_preference: "api_key", auth_status: "locked", auth_verified_at: null, available_models: ["c"], preferred_model: "c" },
      openai: { api_key_stored: false, auth_method_preference: "api_key", auth_status: "locked", auth_verified_at: null, available_models: ["o"], preferred_model: "o" },
      gemini: { api_key_stored: false, auth_method_preference: "api_key", auth_status: "locked", auth_verified_at: null, available_models: ["g"], preferred_model: "g" },
      ollama: { endpoint: "http://127.0.0.1:11434", available_models: [], preferred_model: "gemma4:e4b", runtime_source: "per_user_zip", runtime_path: "", runtime_version: installed, runtime_target_version: target, runtime_target_explicit: explicit, fallback_endpoints: ["http://127.0.0.1:11434"], last_health_check: null },
      lm_studio: { endpoint: "http://127.0.0.1:1234", api_key: "", preferred_model: "a-model", available_models: ["a-model"] },
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
      model: "gemma4:e4b",
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

let catalog: OllamaRuntimeVersionInfo[] = [];
/** The folder of the selected target exists (what `detect_ollama_runtime` reports as `target_installed`). */
let targetOnDisk = false;
let runningVersion = "0.30.4";
const mockedInvoke = vi.mocked(invoke);

const list = () => document.getElementById("ai-fallback-runtime-version-list") as HTMLElement;
const listedVersions = () =>
  Array.from(list().querySelectorAll<HTMLElement>("tr.ollama-zebra-row")).map((row) => row.dataset.zebraRow);
const rowOf = (version: string) =>
  list().querySelector<HTMLElement>(`tr.ollama-zebra-row[data-zebra-row="${version}"]`);
const chipsOf = (version: string) =>
  Array.from(rowOf(version)?.querySelectorAll(".ollama-zebra-chip") ?? []).map((chip) => chip.textContent);
const buttonsOf = (version: string) =>
  Array.from(rowOf(version)?.querySelectorAll("button") ?? []).map((button) => button.textContent);
const primary = () => document.getElementById("ai-fallback-local-primary-action") as HTMLButtonElement;
const callsOf = (command: string) => mockedInvoke.mock.calls.filter((call) => call[0] === command);

async function openSettingsPage() {
  await models.refreshOllamaRuntimeState({ force: true });
  renderAIFallbackSettingsUi();
}

beforeEach(() => {
  __resetForTesting();
  setOverlayHealth(null);
  setStartupStatus({ ollama_ready: true, ollama_starting: false } as StartupStatus);
  runningVersion = "0.30.4";
  targetOnDisk = false;
  catalog = [entry("0.30.4", { installed: true, installable: false, installable_reason: "Installed locally." }), entry("0.40.13", { source: "pinned", recommended: true, latest: true })];
  mockedInvoke.mockReset();
  mockedInvoke.mockImplementation((async (command: string) => {
    switch (command) {
      case "list_ollama_runtime_versions":
      case "fetch_ollama_online_versions":
        return catalog;
      case "detect_ollama_runtime":
        return { found: true, is_serving: true, source: "per_user_zip", path: "p", version: runningVersion, target_installed: targetOnDisk };
      case "verify_ollama_runtime":
        return { ok: true, endpoint: "http://127.0.0.1:11434", models_count: 0 };
      case "start_ollama_runtime":
        return { pid: 1, endpoint: "http://127.0.0.1:11434", already_running: false, pending_start: false };
      case "download_ollama_runtime":
        return { sha256_ok: true, archive_path: "a.zip", version: "0.40.13" };
      case "fetch_ollama_models_with_size":
        return [];
      default:
        return {};
    }
  }) as never);
});

describe("stored legacy default 0.20.2 with 0.30.4 installed", () => {
  it("moves to the recommended runtime and offers 'Install 0.40.13', never a downgrade", async () => {
    setSettings(mkSettings("0.30.4", "0.20.2"));
    await openSettingsPage();

    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
    expect(listedVersions()).not.toContain("0.20.2");
    expect(chipsOf("0.40.13")).toContain("Target");
    expect(buttonsOf("0.40.13")).toEqual(["Install"]);
    expect(primary().textContent).toBe("Install 0.40.13");
    expect(primary().dataset.runtimeAction).toBe("install");
    expect(primary().disabled).toBe(false);
    expect(list().textContent).not.toContain("0.20.2");
  });

  it("says 'Switch to 0.40.13' when that runtime is already on disk (no download, no install)", async () => {
    targetOnDisk = true;
    catalog = [entry("0.30.4", { installed: true, installable: false }), entry("0.40.13", { source: "pinned", recommended: true, installed: true })];
    setSettings(mkSettings("0.30.4", "0.20.2"));
    await openSettingsPage();

    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
    expect(primary().textContent).toBe("Switch to 0.40.13");

    await models.ensureLocalRuntimeReady();
    expect(callsOf("download_ollama_runtime")).toHaveLength(0);
    expect(callsOf("install_ollama_runtime")).toHaveLength(0);
    expect(callsOf("start_ollama_runtime")).toHaveLength(1);
  });

  it("the runtime flow itself never targets 0.20.2, even if nothing normalised the settings first", async () => {
    setSettings(mkSettings("0.30.4", "0.20.2"));
    // Neither a render pass nor a refresh ran: the stored value is still the legacy one.
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.20.2");

    await models.ensureLocalRuntimeReady();
    expect(callsOf("download_ollama_runtime")).toHaveLength(1);
    expect(callsOf("download_ollama_runtime")[0][1]).toEqual({ version: "0.40.13" });
  });

  it("a legacy target without any installed runtime also becomes the recommended one", async () => {
    setSettings(mkSettings("", "0.20.2"));
    runningVersion = "";
    await openSettingsPage();
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
  });

  it("an explicit older choice (picked in the version list) is still honoured", async () => {
    targetOnDisk = true;
    setSettings(mkSettings("0.40.13", "0.30.4", true));
    runningVersion = "0.40.13";
    catalog = [entry("0.30.4", { installed: true, installable: false }), entry("0.40.13", { source: "pinned", recommended: true, installed: true })];
    await openSettingsPage();
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.30.4");
    expect(chipsOf("0.30.4")).toContain("Target");
    expect(buttonsOf("0.30.4")).toEqual(["Switch to"]);
    expect(primary().textContent).toBe("Switch to 0.30.4");
  });

  it("an older target nobody picked is lifted to the running runtime: no downgrade offer", async () => {
    // The state found on the machine: runtime 0.40.13 running, target left behind at 0.30.4.
    setSettings(mkSettings("0.40.13", "0.30.4", false));
    runningVersion = "0.40.13";
    catalog = [entry("0.30.4", { installed: true, installable: false }), entry("0.40.13", { source: "pinned", recommended: true, installed: true })];
    await openSettingsPage();
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
    expect(primary().textContent).toBe("Runtime ready");
    expect(chipsOf("0.30.4")).not.toContain("Target");
    expect(buttonsOf("0.30.4")).toEqual(["Switch to", "Delete"]);
  });

  it("the primary action never offers the downgrade, and the refresh heals the stored target", async () => {
    setSettings(mkSettings("0.40.13", "0.30.4", false));
    runningVersion = "0.40.13";
    // Before anything read the runtime or healed the settings: no detect result yet.
    expect(models.getOllamaRuntimeCardState().primaryLabel).not.toBe("Switch to 0.30.4");
    await models.refreshOllamaRuntimeState({ force: true });
    expect(models.getOllamaRuntimeCardState().primaryLabel).toBe("Runtime ready");
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
  });
});

describe("the settings page after changes", () => {
  it("keeps the target through re-renders and a changed catalog", async () => {
    setSettings(mkSettings("0.30.4", "0.30.4"));
    await openSettingsPage();
    settings!.providers.ollama.runtime_target_version = "0.40.13";
    renderAIFallbackSettingsUi();
    expect(chipsOf("0.40.13")).toContain("Target");

    catalog = [entry("0.40.13", { source: "pinned", recommended: true }), entry("0.40.9"), entry("0.30.4", { installed: true, installable: false })];
    await models.refreshOllamaRuntimeVersionCatalog(true);
    renderAIFallbackSettingsUi();
    renderAIFallbackSettingsUi();
    expect(chipsOf("0.40.13")).toContain("Target");
    expect(settings?.providers.ollama.runtime_target_version).toBe("0.40.13");
  });

  it("does not offer 'Switch to X' for a version just picked that is not on disk (the last detect was about another target)", async () => {
    targetOnDisk = true; // the detect ran while the target was 0.30.4, which is on disk
    setSettings(mkSettings("0.30.4", "0.30.4"));
    await openSettingsPage();
    expect(primary().textContent).toBe("Runtime ready");

    settings!.providers.ollama.runtime_target_version = "0.40.13"; // what a pick in the list stores
    renderAIFallbackSettingsUi();
    expect(primary().textContent).toBe("Install 0.40.13");
  });

  it("renders hostile catalog text as text in the list", async () => {
    const evil = '<img src=x onerror="window.__pwn=1">';
    catalog = [
      entry("0.30.4", { installed: true, installable: false }),
      entry("0.99.0", { installable: false, installable_reason: evil }),
      entry(`0.98.0${evil}`, { installable: true }),
    ];
    setSettings(mkSettings("0.30.4", "0.99.0"));
    await openSettingsPage();
    expect(list().querySelector("img")).toBeNull();
    expect(list().textContent).toContain(evil);
    const notInstallable = rowOf("0.99.0")?.querySelector<HTMLElement>("td:last-child .ollama-zebra-chip");
    expect(notInstallable?.title).toBe(evil);
    expect((window as unknown as { __pwn?: number }).__pwn).toBeUndefined();
  });
});
